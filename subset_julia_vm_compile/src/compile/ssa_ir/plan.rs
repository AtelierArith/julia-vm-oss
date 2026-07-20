//! Shared backend plan for SSA lowering (Issue #9089).
//!
//! This module owns the backend-neutral planning decisions that used to live
//! inside the stack-bytecode lowering module: block order, expression
//! materialization, spill names, and phi edge copies. Stack and register
//! backends consume this plan instead of deriving independent control-flow
//! schedules.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ir::core::{BinaryOp, Expr, NumericConvertTarget};
use crate::span::Span;
pub use subset_julia_vm_bytecode::{
    SharedBlockPlan, SharedCopyPlan, SharedFunctionPlan, SharedRootPlan, SharedTermPlan,
};

use super::model::{SsaBlock, SsaFunction, SsaOp, SsaStatement, SsaValue, SsaValueId, Terminator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanError {
    reason: String,
}

impl PlanError {
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

fn fallback<T>(reason: impl Into<String>) -> Result<T, PlanError> {
    Err(PlanError {
        reason: reason.into(),
    })
}

/// Per-function gate for the structural numeric-conversion rewrite (Issue
/// #9803): whether a bare call to `Int64` / `Float64` in this function is
/// PROVEN to resolve to the builtin constructor, using the same evidence the
/// stack compiler's `compile_generic_dispatch_call` consults (no user/module/
/// `Base.`-qualified method table for the name, no shadowing function binder).
/// `false` (the `Default`, and the conservative choice) keeps the call as a
/// plain `Expr::Call`, which both backends handle exactly as before this
/// rewrite existed — the stack backend performs full user-method dispatch and
/// the register backend falls back. The gate is computed by the caller
/// (`lower.rs`), which has the compiler context; the plan module itself never
/// inspects method tables.
#[derive(Debug, Clone, Copy, Default)]
pub struct NumericConvertGate {
    pub int64: bool,
    pub float64: bool,
}

fn slot_name(id: SsaValueId) -> String {
    format!("#ssa{}", id.0)
}

fn temp_name(id: SsaValueId) -> String {
    format!("#ssatmp{}", id.0)
}

struct DefSite {
    block: usize,
    index: usize,
}

/// Shared analysis state for the whole function.
struct Planner<'a> {
    ssa: &'a SsaFunction,
    /// Span used for statements that have no span of their own (empty-block
    /// terminator payloads): the function's span.
    fallback_span: Span,
    sites: HashMap<u32, DefSite>,
    uses: HashMap<u32, u32>,
    /// Definitions that must always live in a slot (phi results, values
    /// flowing on conditional branch edges or multi-copy jump edges).
    force_spill: HashSet<u32>,
    /// Incoming copies per (pred, succ) edge, in phi order. `bool` marks an
    /// edge whose parallel copies interfere and need temp staging.
    edge_copies: HashMap<(u32, u32), (Vec<(SsaValueId, SsaValue)>, bool)>,
    /// Phi-copy coalescing (Issue #8440): definitions that write their phi's
    /// slot directly (def id -> phi id). The corresponding edge copy is elided.
    coalesced: HashMap<u32, SsaValueId>,
    /// Structural numeric-conversion rewrite gate (Issue #9803).
    convert_gate: NumericConvertGate,
}

pub fn plan_function(
    ssa: &SsaFunction,
    fallback_span: Span,
    convert_gate: NumericConvertGate,
) -> Result<SharedFunctionPlan, PlanError> {
    let mut planner = Planner {
        ssa,
        fallback_span,
        sites: HashMap::new(),
        uses: HashMap::new(),
        force_spill: HashSet::new(),
        edge_copies: HashMap::new(),
        coalesced: HashMap::new(),
        convert_gate,
    };
    planner.collect_sites_and_uses();
    planner.collect_edge_copies();
    planner.coalesce_phi_copies();
    planner.apply_force_spill();

    let mut blocks = Vec::with_capacity(ssa.blocks.len());
    for (block_idx, block) in ssa.blocks.iter().enumerate() {
        blocks.push(planner.plan_block(block_idx, block)?);
    }
    Ok(SharedFunctionPlan::new(blocks))
}

impl<'a> Planner<'a> {
    fn collect_sites_and_uses(&mut self) {
        for (block_idx, block) in self.ssa.blocks.iter().enumerate() {
            for (index, stmt) in block.stmts.iter().enumerate() {
                self.sites.insert(
                    stmt.id.0,
                    DefSite {
                        block: block_idx,
                        index,
                    },
                );
                if let SsaOp::Phi(phi) = &stmt.op {
                    self.force_spill.insert(stmt.id.0);
                    for value in phi.values.iter().flatten() {
                        self.note_use(value);
                    }
                } else {
                    for value in stmt.op.operands() {
                        self.note_use(value);
                    }
                }
            }
            for value in block.terminator.operands() {
                self.note_use(value);
            }
        }
    }

    fn note_use(&mut self, value: &SsaValue) {
        if let SsaValue::Def(id) = value {
            *self.uses.entry(id.0).or_insert(0) += 1;
        }
    }

    fn collect_edge_copies(&mut self) {
        for block in &self.ssa.blocks {
            let phis: Vec<&SsaStatement> = block.phis().collect();
            if phis.is_empty() {
                continue;
            }
            let phi_ids: HashSet<u32> = phis.iter().map(|stmt| stmt.id.0).collect();
            for (edge_pos, pred) in match &phis[0].op {
                SsaOp::Phi(phi) => phi.edges.iter().enumerate(),
                _ => unreachable!("phis() returns phi statements"),
            } {
                let mut copies = Vec::new();
                let mut interference = false;
                for stmt in &phis {
                    let SsaOp::Phi(phi) = &stmt.op else {
                        continue;
                    };
                    let Some(Some(value)) = phi.values.get(edge_pos) else {
                        continue;
                    };
                    if *value == SsaValue::Def(stmt.id) {
                        continue;
                    }
                    if let SsaValue::Def(id) = value {
                        if phi_ids.contains(&id.0) {
                            interference = true;
                        }
                    }
                    copies.push((stmt.id, value.clone()));
                }
                if copies.is_empty() {
                    continue;
                }
                self.edge_copies
                    .insert((pred.0, block.id.0), (copies, interference));
            }
        }
    }

    fn coalesce_phi_copies(&mut self) {
        let mut planned: Vec<((u32, u32), SsaValueId, u32)> = Vec::new();
        for ((pred, succ), (copies, interference)) in &self.edge_copies {
            if *interference {
                continue;
            }
            let pred_block = &self.ssa.blocks[*pred as usize];
            if !matches!(pred_block.terminator, Terminator::Jump { .. }) {
                continue;
            }
            for (phi_id, value) in copies {
                let SsaValue::Def(def) = value else {
                    continue;
                };
                let Some(site) = self.sites.get(&def.0) else {
                    continue;
                };
                if site.block != *pred as usize
                    || !is_inline_op(&pred_block.stmts[site.index].op)
                    || self.uses.get(&def.0).copied().unwrap_or(0) != 1
                {
                    continue;
                }
                let phi_value = SsaValue::Def(*phi_id);
                let phi_read_after = pred_block.stmts[site.index + 1..].iter().any(|stmt| {
                    stmt.op
                        .operands()
                        .into_iter()
                        .any(|operand| *operand == phi_value)
                }) || pred_block
                    .terminator
                    .operands()
                    .into_iter()
                    .any(|operand| *operand == phi_value)
                    || copies
                        .iter()
                        .any(|(other, source)| other != phi_id && *source == phi_value);
                if phi_read_after {
                    continue;
                }
                planned.push(((*pred, *succ), *phi_id, def.0));
            }
        }
        for ((pred, succ), phi_id, def) in planned {
            self.coalesced.insert(def, phi_id);
            self.force_spill.insert(def);
            if let Some((copies, _)) = self.edge_copies.get_mut(&(pred, succ)) {
                copies.retain(|(phi, _)| *phi != phi_id);
            }
        }
    }

    fn apply_force_spill(&mut self) {
        let mut spills: Vec<u32> = Vec::new();
        for ((pred, _succ), (copies, interference)) in &self.edge_copies {
            if copies.is_empty() {
                continue;
            }
            let pred_is_branch = matches!(
                self.ssa.blocks[*pred as usize].terminator,
                Terminator::Branch { .. }
            );
            if pred_is_branch || *interference || copies.len() > 1 {
                for (_, value) in copies {
                    if let SsaValue::Def(id) = value {
                        spills.push(id.0);
                    }
                }
            }
        }
        self.force_spill.extend(spills);
    }

    fn plan_block(&self, block_idx: usize, block: &SsaBlock) -> Result<SharedBlockPlan, PlanError> {
        let mut consumed = vec![false; block.stmts.len()];
        let mut cursor = block.stmts.len();

        let terminator = match &block.terminator {
            Terminator::Return { value } => {
                let expr = match value {
                    Some(value) => Some(self.build_value(
                        block_idx,
                        block,
                        &mut consumed,
                        &mut cursor,
                        value,
                        self.block_span(block),
                    )?),
                    None => None,
                };
                SharedTermPlan::Return { expr }
            }
            Terminator::Branch {
                condition,
                then_target,
                else_target,
            } => {
                let cond = self.build_value(
                    block_idx,
                    block,
                    &mut consumed,
                    &mut cursor,
                    condition,
                    self.block_span(block),
                )?;
                SharedTermPlan::Branch {
                    cond,
                    then_target: then_target.0,
                    else_target: else_target.0,
                    then_copies: self.plan_edge_copies(
                        block_idx,
                        block,
                        &mut consumed,
                        &mut cursor,
                        then_target.0,
                        false,
                    )?,
                    else_copies: self.plan_edge_copies(
                        block_idx,
                        block,
                        &mut consumed,
                        &mut cursor,
                        else_target.0,
                        false,
                    )?,
                }
            }
            Terminator::Jump { target } => {
                let copies = self.plan_edge_copies(
                    block_idx,
                    block,
                    &mut consumed,
                    &mut cursor,
                    target.0,
                    true,
                )?;
                SharedTermPlan::Jump {
                    target: target.0,
                    copies,
                }
            }
        };

        let mut roots_rev = Vec::new();
        let mut idx = cursor;
        while idx > 0 {
            idx -= 1;
            if consumed[idx] {
                continue;
            }
            let stmt = &block.stmts[idx];
            if stmt.op.is_phi() {
                continue;
            }
            let mut chain_cursor = idx;
            let plan = self.plan_root(block_idx, block, &mut consumed, &mut chain_cursor, stmt)?;
            roots_rev.push(plan);
        }
        roots_rev.reverse();

        Ok(SharedBlockPlan::new(roots_rev, terminator))
    }

    fn plan_root(
        &self,
        block_idx: usize,
        block: &SsaBlock,
        consumed: &mut [bool],
        cursor: &mut usize,
        stmt: &SsaStatement,
    ) -> Result<SharedRootPlan, PlanError> {
        if let SsaOp::StoreGlobal { name, value } = &stmt.op {
            let expr = self.build_value(block_idx, block, consumed, cursor, value, stmt.span)?;
            return Ok(SharedRootPlan::Assign {
                name: name.clone(),
                expr,
                span: stmt.span,
            });
        }
        let expr = self.build_op_expr(block_idx, block, consumed, cursor, stmt)?;
        let uses = self.uses.get(&stmt.id.0).copied().unwrap_or(0);
        if uses > 0 {
            Ok(SharedRootPlan::Assign {
                name: self.spill_name(stmt.id),
                expr,
                span: stmt.span,
            })
        } else {
            Ok(SharedRootPlan::Discard {
                expr,
                span: stmt.span,
            })
        }
    }

    fn plan_edge_copies(
        &self,
        block_idx: usize,
        block: &SsaBlock,
        consumed: &mut [bool],
        cursor: &mut usize,
        target: u32,
        allow_inline: bool,
    ) -> Result<Vec<SharedCopyPlan>, PlanError> {
        let Some((copies, interference)) = self.edge_copies.get(&(block.id.0, target)) else {
            return Ok(Vec::new());
        };
        let span = self.block_span(block);
        if *interference {
            let mut plans = Vec::with_capacity(copies.len() * 2);
            for (phi_id, value) in copies {
                let expr = self.materialize(value, span);
                plans.push(SharedCopyPlan {
                    name: temp_name(*phi_id),
                    expr,
                    span,
                });
            }
            for (phi_id, _) in copies {
                plans.push(SharedCopyPlan {
                    name: slot_name(*phi_id),
                    expr: Expr::Var(temp_name(*phi_id).into(), span),
                    span,
                });
            }
            return Ok(plans);
        }
        let inline_single = allow_inline && copies.len() == 1;
        let mut plans = Vec::with_capacity(copies.len());
        for (phi_id, value) in copies {
            let expr = if inline_single {
                self.build_value(block_idx, block, consumed, cursor, value, span)?
            } else {
                self.materialize(value, span)
            };
            plans.push(SharedCopyPlan {
                name: slot_name(*phi_id),
                expr,
                span,
            });
        }
        Ok(plans)
    }

    fn materialize(&self, value: &SsaValue, span: Span) -> Expr {
        match value {
            SsaValue::Const(lit) => Expr::Literal(lit.clone(), span),
            SsaValue::Argument(i) => Expr::Var(self.param_name(*i).into(), span),
            SsaValue::Def(id) => Expr::Var(self.spill_name(*id).into(), span),
        }
    }

    fn spill_name(&self, id: SsaValueId) -> String {
        match self.coalesced.get(&id.0) {
            Some(phi) => slot_name(*phi),
            None => slot_name(id),
        }
    }

    fn param_name(&self, index: usize) -> String {
        self.ssa
            .params
            .get(index)
            .map(|param| param.name.clone())
            .unwrap_or_else(|| format!("#arg{index}"))
    }

    fn block_span(&self, block: &SsaBlock) -> Span {
        block
            .stmts
            .last()
            .map(|stmt| stmt.span)
            .unwrap_or(self.fallback_span)
    }

    fn build_value(
        &self,
        block_idx: usize,
        block: &SsaBlock,
        consumed: &mut [bool],
        cursor: &mut usize,
        value: &SsaValue,
        span: Span,
    ) -> Result<Expr, PlanError> {
        let SsaValue::Def(id) = value else {
            return Ok(self.materialize(value, span));
        };
        let Some(site) = self.sites.get(&id.0) else {
            return fallback("use of unknown definition");
        };
        let inlinable = site.block == block_idx
            && site.index + 1 == *cursor
            && self.uses.get(&id.0).copied().unwrap_or(0) == 1
            && !self.force_spill.contains(&id.0)
            && is_inline_op(&block.stmts[site.index].op);
        if !inlinable {
            return Ok(self.materialize(value, span));
        }
        *cursor = site.index;
        consumed[site.index] = true;
        self.build_op_expr(block_idx, block, consumed, cursor, &block.stmts[site.index])
    }

    fn build_op_expr(
        &self,
        block_idx: usize,
        block: &SsaBlock,
        consumed: &mut [bool],
        cursor: &mut usize,
        stmt: &SsaStatement,
    ) -> Result<Expr, PlanError> {
        let span = stmt.span;
        match &stmt.op {
            SsaOp::Unary { op, operand } => {
                let operand =
                    self.build_value(block_idx, block, consumed, cursor, operand, span)?;
                Ok(Expr::UnaryOp {
                    op: *op,
                    operand: Box::new(operand),
                    span,
                })
            }
            SsaOp::Binary { op, left, right } => {
                let right_expr =
                    self.build_value(block_idx, block, consumed, cursor, right, span)?;
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    self.check_short_circuit_right(block_idx, right, &right_expr)?;
                }
                let left_expr = self.build_value(block_idx, block, consumed, cursor, left, span)?;
                Ok(Expr::BinaryOp {
                    op: *op,
                    left: Box::new(left_expr),
                    right: Box::new(right_expr),
                    span,
                })
            }
            SsaOp::Call {
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            } => {
                let mut kwarg_exprs: Vec<Option<(String, Expr)>> = vec![None; kwargs.len()];
                for (i, (name, value)) in kwargs.iter().enumerate().rev() {
                    let expr = self.build_value(block_idx, block, consumed, cursor, value, span)?;
                    kwarg_exprs[i] = Some((name.clone(), expr));
                }
                let mut arg_exprs: Vec<Option<Expr>> = vec![None; args.len()];
                for (i, value) in args.iter().enumerate().rev() {
                    arg_exprs[i] =
                        Some(self.build_value(block_idx, block, consumed, cursor, value, span)?);
                }
                let mut args: Vec<Expr> = arg_exprs.into_iter().flatten().collect();
                let kwargs: Vec<(crate::ir::core::InternedStr, Expr)> = kwarg_exprs
                    .into_iter()
                    .flatten()
                    .map(|(name, expr)| (name.into(), expr))
                    .collect();

                // Structural explicit numeric type-constructor calls (Issue
                // #9803): recognize the closed, bare, single-argument
                // `Int64(x)` / `Float64(x)` shape here, at plan-build time,
                // where resolving a call target by name is already the
                // established mechanism (mirrors `compile_builtin_types`'s
                // `"Int64"`/`"Float64"` arms in the stack compiler). Rewrite
                // into the structural `Expr::Convert` node so the register
                // backend can lower it by matching the `NumericConvertTarget`
                // enum instead of special-casing a type name by string.
                //
                // The rewrite fires only when `self.convert_gate` proves the
                // name resolves to the BUILTIN constructor for this function
                // (no user-defined `Float64`/`Int64` method table, no
                // shadowing binder — the same decision the stack compiler's
                // `compile_generic_dispatch_call` makes; computed in
                // lower.rs). A program that defines e.g.
                // `Float64(::MyIrrational{:tau})` must keep the plain
                // `Expr::Call`, whose stack lowering performs full
                // user-method dispatch (dispatch fixture
                // `dispatch/symbol_type_param_dispatch.jl`, Issue #633). Any
                // other call shape (qualified, extra args, kwargs, splats)
                // is also left as an ordinary call, unaffected.
                if module.is_none() {
                    if let Some(target) = numeric_convert_target(
                        self.convert_gate,
                        function,
                        args.len(),
                        kwargs.is_empty(),
                        splat_mask,
                        kwargs_splat_mask,
                    ) {
                        let operand = match args.pop() {
                            Some(operand) => operand,
                            None => {
                                return fallback(
                                    "internal: numeric_convert_target checked arity above",
                                )
                            }
                        };
                        return Ok(Expr::Convert {
                            target,
                            operand: Box::new(operand),
                            span,
                        });
                    }
                }

                Ok(match module {
                    Some(module) => Expr::ModuleCall {
                        module: module.clone().into(),
                        function: function.clone().into(),
                        args,
                        kwargs,
                        splat_mask: splat_mask.clone(),
                        kwargs_splat_mask: kwargs_splat_mask.clone(),
                        span,
                    },
                    None => Expr::Call {
                        function: function.clone().into(),
                        args,
                        kwargs,
                        splat_mask: splat_mask.clone(),
                        kwargs_splat_mask: kwargs_splat_mask.clone(),
                        span,
                    },
                })
            }
            SsaOp::Builtin { name, args } => {
                let mut arg_exprs: Vec<Option<Expr>> = vec![None; args.len()];
                for (i, value) in args.iter().enumerate().rev() {
                    arg_exprs[i] =
                        Some(self.build_value(block_idx, block, consumed, cursor, value, span)?);
                }
                Ok(Expr::Builtin {
                    name: *name,
                    args: arg_exprs.into_iter().flatten().collect(),
                    span,
                })
            }
            SsaOp::LoadGlobal { name } => Ok(Expr::Var(name.clone().into(), span)),
            SsaOp::Phi(_)
            | SsaOp::StoreGlobal { .. }
            | SsaOp::Opaque { .. }
            | SsaOp::OpaqueStmt { .. }
            | SsaOp::BarrierReload { .. } => fallback("operation is not expression-shaped"),
        }
    }

    fn check_short_circuit_right(
        &self,
        block_idx: usize,
        right_value: &SsaValue,
        right_expr: &Expr,
    ) -> Result<(), PlanError> {
        let mut suspects = Vec::new();
        collect_spill_reads(right_expr, &mut suspects);
        for id in suspects {
            if let Some(site) = self.sites.get(&id) {
                let is_phi = self.ssa.blocks[site.block].stmts[site.index].op.is_phi();
                if site.block == block_idx && !is_phi {
                    return fallback("short-circuit right operand evaluated eagerly");
                }
            }
        }
        let _ = right_value;
        Ok(())
    }
}

/// Recognizes the closed set of explicit numeric type-constructor calls the
/// shared plan represents structurally instead of as a generic `Expr::Call`
/// (Issue #9803): a bare (unqualified, `module.is_none()` at the call site),
/// single-positional-argument, no-keyword, no-splat call to `Int64` or
/// `Float64` whose name the per-function [`NumericConvertGate`] proves
/// resolves to the builtin constructor (no user override — see the gate
/// docs). `Int`/`UInt`/other aliases are intentionally excluded — those
/// resolve to a platform-dependent concrete type in the stack compiler
/// (`compile_builtin_types`), unlike the two fixed-width constructors here.
fn numeric_convert_target(
    gate: NumericConvertGate,
    function: &str,
    arg_count: usize,
    kwargs_empty: bool,
    splat_mask: &[bool],
    kwargs_splat_mask: &[bool],
) -> Option<NumericConvertTarget> {
    if arg_count != 1
        || !kwargs_empty
        || splat_mask.iter().any(|is_splat| *is_splat)
        || kwargs_splat_mask.iter().any(|is_splat| *is_splat)
    {
        return None;
    }
    match function {
        "Int64" if gate.int64 => Some(NumericConvertTarget::Int64),
        "Float64" if gate.float64 => Some(NumericConvertTarget::Float64),
        _ => None,
    }
}

fn is_inline_op(op: &SsaOp) -> bool {
    matches!(
        op,
        SsaOp::Unary { .. }
            | SsaOp::Binary { .. }
            | SsaOp::Call { .. }
            | SsaOp::Builtin { .. }
            | SsaOp::LoadGlobal { .. }
    )
}

/// Collect the SSA ids of spill-slot reads (`#ssaN` variables) in `expr`.
fn collect_spill_reads(expr: &Expr, out: &mut Vec<u32>) {
    match expr {
        Expr::Var(name, _) => {
            if let Some(rest) = name.strip_prefix("#ssa") {
                if let Ok(id) = rest.parse::<u32>() {
                    out.push(id);
                }
            }
        }
        Expr::UnaryOp { operand, .. } => collect_spill_reads(operand, out),
        Expr::BinaryOp { left, right, .. } => {
            collect_spill_reads(left, out);
            collect_spill_reads(right, out);
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for arg in args {
                collect_spill_reads(arg, out);
            }
            for (_, value) in kwargs {
                collect_spill_reads(value, out);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                collect_spill_reads(arg, out);
            }
        }
        Expr::Convert { operand, .. } => collect_spill_reads(operand, out),
        _ => {}
    }
}
