//! Core IR → SSA conversion for structured control flow (Issue #8550).
//!
//! Core IR is structured (`Expr`/`Stmt`/`Block`, no arbitrary goto), so SSA
//! construction does not need general iterated dominance frontiers: at every
//! merge point the incoming environments are known, and a [`PhiNode`] is
//! placed exactly where the incoming values disagree (Braun et al. 2013 style
//! construction specialized to structured CFGs).
//!
//! Decomposed constructs: straight-line statements, `x = e` / `x += e`,
//! `if`/`else` (nested), `while` with `break`/`continue`, ternary expressions,
//! calls/builtins/unary/binary operators, and explicit `return`. Everything
//! else is carried as an *opaque barrier* ([`SsaOp::Opaque`] /
//! [`SsaOp::OpaqueStmt`]): the barrier lists the locally-bound variables it
//! may read, and every variable it may rebind receives a fresh
//! [`SsaOp::BarrierReload`] definition afterwards ("spill everything live
//! across it"). `@label`/`@goto` are rejected as unstructured. See
//! `docs/vm/SSA_IR.md` for the full limitation list.
//!
//! This slice does not change the compilation pipeline: conversion runs
//! behind unit tests only. In debug builds the [`super::verify`] checker runs
//! after every construction.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::ir::core::{decode_tuple_comprehension_binding, BinaryOp, Block, Expr, Function, Stmt};
use crate::span::Span;

use super::model::{
    BlockId, PhiNode, SsaBlock, SsaFunction, SsaOp, SsaParam, SsaStatement, SsaValue, SsaValueId,
    Terminator,
};
use super::scan;

/// Variable environment: current SSA value of each locally-bound name.
/// `BTreeMap` keeps merge-point phi creation deterministic.
type Env = BTreeMap<String, SsaValue>;

/// Errors from Core IR → SSA conversion.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaBuildError {
    /// `@label`/`@goto` produce unstructured control flow that structured SSA
    /// construction does not model.
    UnstructuredControlFlow {
        /// Human-readable construct name.
        construct: &'static str,
        /// Location of the construct.
        span: Span,
    },
    /// `break`/`continue` encountered outside any enclosing loop.
    LoopControlOutsideLoop {
        /// Human-readable construct name.
        construct: &'static str,
        /// Location of the construct.
        span: Span,
    },
    /// A proof-backed SSA-builder invariant was violated (Issue #10905, Phase
    /// 1b of #10869): this should be unreachable given the immediately
    /// preceding control flow (e.g. a loop-frame stack entry pushed a few
    /// lines above). Reported as a typed error instead of a raw unwrap, so a
    /// future refactor that breaks the invariant surfaces a diagnosable bug
    /// report instead of an uncaught host crash.
    InternalInvariant {
        /// Human-readable description of the violated invariant.
        detail: &'static str,
        /// Location associated with the construct being converted.
        span: Span,
    },
}

impl fmt::Display for SsaBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnstructuredControlFlow { construct, .. } => write!(
                f,
                "SSA construction does not support unstructured control flow: {construct}"
            ),
            Self::LoopControlOutsideLoop { construct, .. } => {
                write!(f, "`{construct}` outside of a loop")
            }
            Self::InternalInvariant { detail, .. } => {
                write!(f, "internal SSA builder error: {detail}")
            }
        }
    }
}

impl std::error::Error for SsaBuildError {}

/// Convert a Core IR function into SSA form.
///
/// Positional parameters and keyword parameters are bound as
/// [`SsaValue::Argument`]s in declaration order (keyword defaults are not yet
/// modeled). In debug builds the result is checked with [`super::verify`].
pub fn build_function(func: &Function) -> Result<SsaFunction, SsaBuildError> {
    let mut params = Vec::new();
    let mut env = Env::new();
    for param in &func.params {
        env.insert(param.name.clone(), SsaValue::Argument(params.len()));
        params.push(SsaParam {
            name: param.name.clone(),
            ty: param.type_annotation.clone(),
        });
    }
    for kwparam in &func.kwparams {
        env.insert(kwparam.name.clone(), SsaValue::Argument(params.len()));
        params.push(SsaParam {
            name: kwparam.name.clone(),
            ty: kwparam.type_annotation.clone(),
        });
    }

    let mut globals = BTreeSet::new();
    scan::collect_global_decls(&func.body, &mut globals);
    // Parameters shadow any (invalid in Julia anyway) global declaration.
    globals.retain(|name| !env.contains_key(name));

    let mut builder = Builder {
        blocks: Vec::new(),
        next_value: 0,
        env,
        globals,
        loop_stack: Vec::new(),
        current: BlockId(0),
    };
    let entry = builder.new_block();
    builder.current = entry;
    let last_value = builder.convert_block(&func.body)?;
    if !builder.is_closed(builder.current) {
        builder.set_terminator(builder.current, Terminator::Return { value: last_value });
    }
    let ssa = builder.finish(func.name.clone(), params, entry);
    debug_assert_eq!(
        super::verify(&ssa),
        Ok(()),
        "SSA verifier failed after construction (Issue #8550)"
    );
    Ok(ssa)
}

/// A basic block under construction.
struct BlockBuild {
    stmts: Vec<SsaStatement>,
    terminator: Option<Terminator>,
    /// True once the block has ended, even when the terminator is still a
    /// pending patch (a `break` whose loop exit block is not allocated yet).
    closed: bool,
}

/// Per-loop conversion state.
struct LoopFrame {
    header: BlockId,
    /// Blocks ending in `break`, terminated with `Jump { exit }` once the
    /// exit block is allocated.
    break_blocks: Vec<BlockId>,
    /// Latch edges into the header (`continue` plus natural body end).
    latch_incoming: Vec<(BlockId, Env)>,
    /// Break edges into the exit block.
    exit_incoming: Vec<(BlockId, Env)>,
}

struct Builder {
    blocks: Vec<BlockBuild>,
    next_value: u32,
    env: Env,
    /// Names declared `global` in this function: reads become
    /// [`SsaOp::LoadGlobal`], writes become [`SsaOp::StoreGlobal`].
    globals: BTreeSet<String>,
    loop_stack: Vec<LoopFrame>,
    current: BlockId,
}

impl Builder {
    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(BlockBuild {
            stmts: Vec::new(),
            terminator: None,
            closed: false,
        });
        id
    }

    fn alloc_id(&mut self) -> SsaValueId {
        let id = SsaValueId(self.next_value);
        self.next_value += 1;
        id
    }

    fn emit(&mut self, op: SsaOp, span: Span) -> SsaValue {
        let id = self.alloc_id();
        self.blocks[self.current.0 as usize]
            .stmts
            .push(SsaStatement { id, op, span });
        SsaValue::Def(id)
    }

    fn is_closed(&self, block: BlockId) -> bool {
        self.blocks[block.0 as usize].closed
    }

    fn set_terminator(&mut self, block: BlockId, terminator: Terminator) {
        let block = &mut self.blocks[block.0 as usize];
        debug_assert!(
            block.terminator.is_none(),
            "block terminated twice (Issue #8550)"
        );
        block.terminator = Some(terminator);
        block.closed = true;
    }

    /// Close a block whose terminator target is not allocated yet (break).
    fn close_pending(&mut self, block: BlockId) {
        self.blocks[block.0 as usize].closed = true;
    }

    fn read_var(&mut self, name: &str, span: Span) -> SsaValue {
        if !self.globals.contains(name) {
            if let Some(value) = self.env.get(name) {
                return value.clone();
            }
        }
        self.emit(
            SsaOp::LoadGlobal {
                name: name.to_string(),
            },
            span,
        )
    }

    fn write_var(&mut self, name: &str, value: SsaValue, span: Span) {
        if self.globals.contains(name) {
            self.emit(
                SsaOp::StoreGlobal {
                    name: name.to_string(),
                    value,
                },
                span,
            );
        } else {
            self.env.insert(name.to_string(), value);
        }
    }

    /// Resolve possibly-read names to their current SSA values; names with no
    /// local binding stay implicit global reads inside the opaque payload.
    fn resolve_reads(&self, names: Vec<String>) -> Vec<(String, SsaValue)> {
        names
            .into_iter()
            .filter(|name| !self.globals.contains(name))
            .filter_map(|name| self.env.get(&name).cloned().map(|value| (name, value)))
            .collect()
    }

    fn convert_block(&mut self, block: &Block) -> Result<Option<SsaValue>, SsaBuildError> {
        let mut last_value = None;
        for stmt in &block.stmts {
            if self.is_closed(self.current) {
                // Statements after return/break/continue are unreachable but
                // still converted, into a predecessor-less block, so no user
                // code is silently dropped.
                self.current = self.new_block();
            }
            last_value = self.convert_stmt(stmt)?;
        }
        Ok(last_value)
    }

    fn convert_stmt(&mut self, stmt: &Stmt) -> Result<Option<SsaValue>, SsaBuildError> {
        match stmt {
            Stmt::Block(block) => self.convert_block(block),
            Stmt::Assign { var, value, span } => {
                if decode_tuple_comprehension_binding(var).is_some() {
                    // Multi-target comprehension binding: not decomposed yet.
                    self.convert_opaque_stmt(stmt, *span)?;
                    return Ok(None);
                }
                let value = self.convert_expr(value)?;
                self.write_var(var, value.clone(), *span);
                Ok(Some(value))
            }
            Stmt::AddAssign { var, value, span } => {
                // `x += e` is `x = x + e`.
                let left = self.read_var(var, *span);
                let right = self.convert_expr(value)?;
                let sum = self.emit(
                    SsaOp::Binary {
                        op: BinaryOp::Add,
                        left,
                        right,
                    },
                    *span,
                );
                self.write_var(var, sum.clone(), *span);
                Ok(Some(sum))
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                self.convert_if(condition, then_branch, else_branch.as_ref(), *span)?;
                Ok(None)
            }
            Stmt::While {
                condition,
                body,
                span,
            } => {
                self.convert_while(condition, body, *span)?;
                Ok(None)
            }
            Stmt::Break { span } => {
                let current = self.current;
                let env = self.env.clone();
                let Some(frame) = self.loop_stack.last_mut() else {
                    return Err(SsaBuildError::LoopControlOutsideLoop {
                        construct: "break",
                        span: *span,
                    });
                };
                frame.break_blocks.push(current);
                frame.exit_incoming.push((current, env));
                self.close_pending(current);
                Ok(None)
            }
            Stmt::Continue { span } => {
                let current = self.current;
                let env = self.env.clone();
                let Some(frame) = self.loop_stack.last_mut() else {
                    return Err(SsaBuildError::LoopControlOutsideLoop {
                        construct: "continue",
                        span: *span,
                    });
                };
                let header = frame.header;
                frame.latch_incoming.push((current, env));
                self.set_terminator(current, Terminator::Jump { target: header });
                Ok(None)
            }
            Stmt::Return { value, span } => {
                let value = match value {
                    Some(value) => Some(self.convert_expr(value)?),
                    None => None,
                };
                self.set_terminator(self.current, Terminator::Return { value });
                let _ = span;
                Ok(None)
            }
            Stmt::Expr { expr, .. } => Ok(Some(self.convert_expr(expr)?)),
            // No local dataflow: `global` routing is pre-scanned, the rest
            // declare module-level facts.
            Stmt::Global { .. }
            | Stmt::Meta { .. }
            | Stmt::LocalDecl { .. }
            | Stmt::Using { .. }
            | Stmt::Export { .. }
            | Stmt::EnumDef { .. }
            | Stmt::RuntimeNominalDef { .. } => Ok(None),
            Stmt::Label { span, .. } => Err(SsaBuildError::UnstructuredControlFlow {
                construct: "@label",
                span: *span,
            }),
            Stmt::Goto { span, .. } => Err(SsaBuildError::UnstructuredControlFlow {
                construct: "@goto",
                span: *span,
            }),
            // Everything else is an opaque barrier in this slice.
            Stmt::For { .. }
            | Stmt::ForEach { .. }
            | Stmt::ForEachTuple { .. }
            | Stmt::Try { .. }
            | Stmt::Timed { .. }
            | Stmt::Test { .. }
            | Stmt::TestSet { .. }
            | Stmt::TestThrows { .. }
            | Stmt::IndexAssign { .. }
            | Stmt::FieldAssign { .. }
            | Stmt::DestructuringAssign { .. }
            | Stmt::DictAssign { .. }
            | Stmt::FunctionDef { .. }
            | Stmt::EvalFunctionDef { .. } => {
                self.convert_opaque_stmt(stmt, stmt.span())?;
                Ok(None)
            }
        }
    }

    /// Lower a statement as an opaque barrier: one [`SsaOp::OpaqueStmt`]
    /// carrying the verbatim Core IR plus resolved reads, followed by a fresh
    /// [`SsaOp::BarrierReload`] definition for every local it may rebind.
    fn convert_opaque_stmt(&mut self, stmt: &Stmt, span: Span) -> Result<(), SsaBuildError> {
        let reads = self.resolve_reads(scan::stmt_read_names(stmt));
        let barrier = self.emit(
            SsaOp::OpaqueStmt {
                stmt: Box::new(stmt.clone()),
                reads,
            },
            span,
        );
        self.reload_after_barrier(stmt_writes(stmt), barrier, span);
        Ok(())
    }

    fn reload_after_barrier(&mut self, writes: BTreeSet<String>, barrier: SsaValue, span: Span) {
        for var in writes {
            if self.globals.contains(&var) {
                continue;
            }
            let reload = self.emit(
                SsaOp::BarrierReload {
                    var: var.clone(),
                    barrier: barrier.clone(),
                },
                span,
            );
            self.env.insert(var, reload);
        }
    }

    fn convert_if(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Block>,
        span: Span,
    ) -> Result<(), SsaBuildError> {
        let condition = self.convert_expr(condition)?;
        let cond_block = self.current;
        let env_at_branch = self.env.clone();

        let then_entry = self.new_block();
        self.current = then_entry;
        self.env = env_at_branch.clone();
        self.convert_block(then_branch)?;
        let then_end = (!self.is_closed(self.current)).then(|| (self.current, self.env.clone()));

        let mut else_end = None;
        let else_entry = match else_branch {
            Some(else_branch) => {
                let else_entry = self.new_block();
                self.current = else_entry;
                self.env = env_at_branch.clone();
                self.convert_block(else_branch)?;
                else_end =
                    (!self.is_closed(self.current)).then(|| (self.current, self.env.clone()));
                Some(else_entry)
            }
            None => None,
        };

        let join = self.new_block();
        self.set_terminator(
            cond_block,
            Terminator::Branch {
                condition,
                then_target: then_entry,
                else_target: else_entry.unwrap_or(join),
            },
        );

        let mut incoming = Vec::new();
        if else_entry.is_none() {
            incoming.push((cond_block, env_at_branch.clone()));
        }
        for (block, env) in [then_end, else_end].into_iter().flatten() {
            self.set_terminator(block, Terminator::Jump { target: join });
            incoming.push((block, env));
        }
        self.merge_into(join, incoming, env_at_branch, span);
        Ok(())
    }

    fn convert_while(
        &mut self,
        condition: &Expr,
        body: &Block,
        span: Span,
    ) -> Result<(), SsaBuildError> {
        let pre_block = self.current;
        let pre_env = self.env.clone();
        let header = self.new_block();
        self.set_terminator(pre_block, Terminator::Jump { target: header });

        // Pre-scan the variables the loop may rebind: each gets a header phi
        // whose latch operands are filled in after the body is converted.
        // The scan is syntactic, so a variable only assigned its own value
        // still gets a (redundant but valid) phi; construction is minimal at
        // if-joins but not pruned at loop headers.
        let mut assigned = BTreeSet::new();
        scan::expr_write_names(condition, &mut assigned);
        scan::block_write_names(body, &mut assigned);
        assigned.retain(|name| !self.globals.contains(name));

        self.current = header;
        self.env = pre_env.clone();
        let mut header_phis = Vec::new();
        for var in &assigned {
            let id = self.alloc_id();
            self.blocks[header.0 as usize].stmts.push(SsaStatement {
                id,
                op: SsaOp::Phi(PhiNode {
                    edges: vec![pre_block],
                    values: vec![pre_env.get(var).cloned()],
                }),
                span,
            });
            self.env.insert(var.clone(), SsaValue::Def(id));
            header_phis.push(var.clone());
        }

        let condition = self.convert_expr(condition)?;
        let cond_block = self.current;
        let env_at_branch = self.env.clone();

        let body_entry = self.new_block();
        self.loop_stack.push(LoopFrame {
            header,
            break_blocks: Vec::new(),
            latch_incoming: Vec::new(),
            exit_incoming: Vec::new(),
        });
        self.current = body_entry;
        self.env = env_at_branch.clone();
        self.convert_block(body)?;
        let mut frame = self
            .loop_stack
            .pop()
            .ok_or(SsaBuildError::InternalInvariant {
                detail: "loop frame pushed above (Issue #8550)",
                span,
            })?;
        if !self.is_closed(self.current) {
            frame.latch_incoming.push((self.current, self.env.clone()));
            self.set_terminator(self.current, Terminator::Jump { target: header });
        }

        // Seal the header phis with one operand per latch edge, ascending by
        // block id to match the finalized predecessor order.
        frame.latch_incoming.sort_by_key(|(block, _)| *block);
        for (index, var) in header_phis.iter().enumerate() {
            if let SsaOp::Phi(phi) = &mut self.blocks[header.0 as usize].stmts[index].op {
                for (block, env) in &frame.latch_incoming {
                    phi.edges.push(*block);
                    phi.values.push(env.get(var).cloned());
                }
            }
        }

        let exit = self.new_block();
        self.set_terminator(
            cond_block,
            Terminator::Branch {
                condition,
                then_target: body_entry,
                else_target: exit,
            },
        );
        for block in frame.break_blocks {
            self.set_terminator(block, Terminator::Jump { target: exit });
        }
        let mut incoming = frame.exit_incoming;
        incoming.push((cond_block, env_at_branch.clone()));
        self.merge_into(exit, incoming, env_at_branch, span);
        Ok(())
    }

    /// Enter `join` with the merge of the incoming environments, creating one
    /// phi per variable whose incoming values disagree (or that is missing on
    /// some path: `None` = maybe-undef). With zero incoming edges the block
    /// is unreachable and `fallback_env` is used so conversion can continue.
    fn merge_into(
        &mut self,
        join: BlockId,
        mut incoming: Vec<(BlockId, Env)>,
        fallback_env: Env,
        span: Span,
    ) {
        incoming.sort_by_key(|(block, _)| *block);
        self.current = join;
        if incoming.is_empty() {
            self.env = fallback_env;
            return;
        }
        if incoming.len() == 1 {
            if let Some((_, env)) = incoming.pop() {
                self.env = env;
            }
            return;
        }

        let mut keys = BTreeSet::new();
        for (_, env) in &incoming {
            keys.extend(env.keys().cloned());
        }
        let mut merged = Env::new();
        for key in keys {
            let values: Vec<Option<SsaValue>> = incoming
                .iter()
                .map(|(_, env)| env.get(&key).cloned())
                .collect();
            if let [Some(first), rest @ ..] = values.as_slice() {
                if rest.iter().all(|value| value.as_ref() == Some(first)) {
                    merged.insert(key, first.clone());
                    continue;
                }
            }
            let phi = self.emit(
                SsaOp::Phi(PhiNode {
                    edges: incoming.iter().map(|(block, _)| *block).collect(),
                    values,
                }),
                span,
            );
            merged.insert(key, phi);
        }
        self.env = merged;
    }

    fn convert_expr(&mut self, expr: &Expr) -> Result<SsaValue, SsaBuildError> {
        match expr {
            Expr::Literal(literal, _) => Ok(SsaValue::Const(literal.clone())),
            Expr::Var(name, span) => Ok(self.read_var(name, *span)),
            Expr::BinaryOp {
                op,
                left,
                right,
                span,
            } => {
                let left = self.convert_expr(left)?;
                let right = self.convert_expr(right)?;
                Ok(self.emit(
                    SsaOp::Binary {
                        op: *op,
                        left,
                        right,
                    },
                    *span,
                ))
            }
            Expr::UnaryOp { op, operand, span } => {
                let operand = self.convert_expr(operand)?;
                Ok(self.emit(SsaOp::Unary { op: *op, operand }, *span))
            }
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            } => {
                // A parametric-type call like `Foo{typeof(v)}(v)` or
                // `Wrapper{T,N}(a)` where `T = eltype(a)` is a local embeds
                // hidden local-variable reads inside the function-name string.
                // The SSA use-counter only sees positional args, so the
                // referenced locals could be incorrectly inlined — emitting the
                // computation without storing the value to a slot, while the
                // compiler re-parses the type-arg string and emits
                // `LoadAny("T")` / `LoadAny("v")` which then fails at runtime.
                // Route these calls through the opaque path so that
                // `scan_lowerable` falls back to the legacy compiler, which
                // re-parses the type-arg string after every local is safely
                // stored in a slot (Issue #8832).
                if self.call_needs_opaque_path(function) {
                    return self.convert_opaque_expr(expr);
                }
                self.convert_call(
                    None,
                    function,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    *span,
                )
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            } => self.convert_call(
                Some(module.to_string()),
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                *span,
            ),
            Expr::Builtin { name, args, span } => {
                let args = args
                    .iter()
                    .map(|arg| self.convert_expr(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.emit(SsaOp::Builtin { name: *name, args }, *span))
            }
            Expr::AssignExpr { var, value, span } => {
                let value = self.convert_expr(value)?;
                self.write_var(var.as_str(), value.clone(), *span);
                Ok(value)
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                span,
            } => self.convert_ternary(condition, then_expr, else_expr, *span),
            // Every other expression form is carried verbatim with its
            // resolved variable reads (and barrier reloads for embedded
            // assignments).
            other => self.convert_opaque_expr(other),
        }
    }

    #[allow(clippy::too_many_arguments)] // mirrors the Core IR Call payload
    fn convert_call(
        &mut self,
        module: Option<String>,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
        span: Span,
    ) -> Result<SsaValue, SsaBuildError> {
        let args = args
            .iter()
            .map(|arg| self.convert_expr(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let mut kwarg_values = Vec::with_capacity(kwargs.len());
        for (name, value) in kwargs {
            kwarg_values.push((name.to_string(), self.convert_expr(value)?));
        }
        Ok(self.emit(
            SsaOp::Call {
                module,
                function: function.to_string(),
                args,
                kwargs: kwarg_values,
                splat_mask: splat_mask.to_vec(),
                kwargs_splat_mask: kwargs_splat_mask.to_vec(),
            },
            span,
        ))
    }

    fn convert_ternary(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
        span: Span,
    ) -> Result<SsaValue, SsaBuildError> {
        let condition = self.convert_expr(condition)?;
        let cond_block = self.current;
        let env_at_branch = self.env.clone();

        let then_entry = self.new_block();
        self.current = then_entry;
        self.env = env_at_branch.clone();
        let then_value = self.convert_expr(then_expr)?;
        let then_end = (self.current, self.env.clone());

        let else_entry = self.new_block();
        self.current = else_entry;
        self.env = env_at_branch.clone();
        let else_value = self.convert_expr(else_expr)?;
        let else_end = (self.current, self.env.clone());

        let join = self.new_block();
        self.set_terminator(
            cond_block,
            Terminator::Branch {
                condition,
                then_target: then_entry,
                else_target: else_entry,
            },
        );
        self.set_terminator(then_end.0, Terminator::Jump { target: join });
        self.set_terminator(else_end.0, Terminator::Jump { target: join });
        let incoming = vec![(then_end.0, then_end.1), (else_end.0, else_end.1)];
        self.merge_into(join, incoming, env_at_branch, span);

        if then_value == else_value {
            return Ok(then_value);
        }
        // Result phi; block allocation order guarantees ascending edges.
        Ok(self.emit(
            SsaOp::Phi(PhiNode {
                edges: vec![then_end.0, else_end.0],
                values: vec![Some(then_value), Some(else_value)],
            }),
            span,
        ))
    }

    fn convert_opaque_expr(&mut self, expr: &Expr) -> Result<SsaValue, SsaBuildError> {
        let span = expr.span();
        let reads = self.resolve_reads(scan::expr_read_names(expr));
        let barrier = self.emit(
            SsaOp::Opaque {
                expr: Box::new(expr.clone()),
                reads,
            },
            span,
        );
        let mut writes = BTreeSet::new();
        scan::expr_write_names(expr, &mut writes);
        self.reload_after_barrier(writes, barrier.clone(), span);
        Ok(barrier)
    }

    fn finish(self, name: String, params: Vec<SsaParam>, entry: BlockId) -> SsaFunction {
        debug_assert!(
            self.blocks.iter().all(|block| block.terminator.is_some()),
            "all blocks must be terminated at finish (Issue #8550)"
        );
        let mut blocks: Vec<SsaBlock> = self
            .blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                let terminator = block
                    .terminator
                    .unwrap_or(Terminator::Return { value: None });
                let succs = terminator.targets();
                SsaBlock {
                    id: BlockId(index as u32),
                    stmts: block.stmts,
                    terminator,
                    preds: Vec::new(),
                    succs,
                }
            })
            .collect();
        // Predecessors ascending by id (blocks are scanned in id order).
        let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); blocks.len()];
        for block in &blocks {
            for succ in &block.succs {
                preds[succ.0 as usize].push(block.id);
            }
        }
        for (block, preds) in blocks.iter_mut().zip(preds) {
            block.preds = preds;
        }
        SsaFunction {
            name,
            params,
            entry,
            blocks,
        }
    }
}

fn stmt_writes(stmt: &Stmt) -> BTreeSet<String> {
    let mut writes = BTreeSet::new();
    scan::stmt_write_names(stmt, &mut writes);
    writes
}

impl Builder {
    /// Returns `true` when `function` names a parametric-type constructor that
    /// embeds hidden local-variable reads inside the function-name string.
    ///
    /// The SSA use-counter only tracks positional args, so any type-argument
    /// that requires a runtime local read (either a call expression such as
    /// `typeof(v)` or a plain local variable like `T` from `T = eltype(a)`)
    /// can cause the value to be inlined without being stored to a slot.  The
    /// compiler then re-parses the type-arg string and emits `LoadAny("T")`
    /// which fails at runtime.  Routing such calls through `convert_opaque_expr`
    /// forces `scan_lowerable` to fall back to the legacy compiler, which
    /// re-parses the type-arg string only after every local is safely stored
    /// (Issue #8832).
    fn call_needs_opaque_path(&self, function: &str) -> bool {
        let Some(open) = function.find('{') else {
            return false;
        };
        let Some(close) = function.rfind('}') else {
            return false;
        };
        if close <= open {
            return false;
        }
        let type_args = &function[open + 1..close];
        // Fast path: any '(' means a call expression like typeof(v).
        if type_args.contains('(') {
            return true;
        }
        // Walk top-level comma-separated type args (skip nested {…} regions).
        let mut depth = 0i32;
        let mut start = 0;
        for (i, c) in type_args.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                ',' if depth == 0 => {
                    if self.type_arg_is_local(type_args[start..i].trim()) {
                        return true;
                    }
                    start = i + 1;
                }
                _ => {}
            }
        }
        self.type_arg_is_local(type_args[start..].trim())
    }

    /// Returns `true` when a single type-argument token is a simple identifier
    /// that is currently live in the SSA environment (i.e. it was assigned as a
    /// local variable, not dispatched as a where-clause type parameter).
    ///
    /// Numeric literals and nested types (containing `{`) are excluded because
    /// they do not reference local slots.
    fn type_arg_is_local(&self, arg: &str) -> bool {
        if arg.is_empty() {
            return false;
        }
        // Numeric literal (e.g. "2", "16")
        if arg.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        // Nested parametric type (e.g. "Tuple{}", "Vector{Int64}") — skip;
        // the outer loop doesn't recurse into nested braces anyway.
        if arg.contains('{') {
            return false;
        }
        // Simple identifier — check whether it is currently bound as a local
        // SSA value (i.e. it was assigned inside the function body, not
        // dispatched as a where-clause type parameter).
        let is_ident = arg
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && arg.chars().all(|c| c.is_alphanumeric() || c == '_');
        is_ident && self.env.contains_key(arg)
    }
}
