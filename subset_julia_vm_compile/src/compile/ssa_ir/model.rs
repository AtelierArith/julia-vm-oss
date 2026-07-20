//! Durable SSA IR data model (Issue #8550).
//!
//! This mirrors the shape sketched in `docs/vm/SSA_IR.md`:
//!
//! * [`SsaFunction`]: function name, typed parameters, basic blocks, entry.
//! * [`SsaBlock`]: block id, ordered statements, terminator, pred/succ edges.
//! * [`SsaValue`]: numbered SSA definitions plus constants and arguments.
//! * [`PhiNode`]: per-predecessor value joins at control-flow merge points.
//!
//! The model deliberately keeps rich Core IR payloads for operations this
//! slice does not decompose ([`SsaOp::Opaque`] / [`SsaOp::OpaqueStmt`]), the
//! same way upstream `Compiler/src/ssair/ir.jl` keeps arbitrary `Expr`
//! statements whose operands are `SSAValue` / `Argument` / constants.

use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, Stmt, UnaryOp};
use crate::span::Span;
use crate::types::JuliaType;

/// Identifier of a basic block inside one [`SsaFunction`].
///
/// Block ids are dense indices into [`SsaFunction::blocks`]
/// (`blocks[id.0 as usize].id == id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

/// Identifier of one numbered SSA definition inside one [`SsaFunction`].
///
/// Every [`SsaStatement`] defines exactly one value; ids are unique within a
/// function but carry no positional meaning (unlike upstream's
/// statement-index `SSAValue`s, ids stay stable when blocks are edited).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SsaValueId(pub u32);

/// A value usable in operand position: a numbered definition, a function
/// argument, or a constant.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaValue {
    /// Result of the statement with the given id.
    Def(SsaValueId),
    /// The i-th function parameter (positional parameters first, then
    /// keyword parameters, in declaration order).
    Argument(usize),
    /// A Core IR literal constant.
    Const(Literal),
}

/// Typed function parameter of an [`SsaFunction`].
#[derive(Debug, Clone, PartialEq)]
pub struct SsaParam {
    /// Parameter name (used only for diagnostics; code references parameters
    /// through [`SsaValue::Argument`]).
    pub name: String,
    /// Declared type annotation, if any.
    pub ty: Option<JuliaType>,
}

/// Per-predecessor value join at a control-flow merge point.
///
/// `values[i]` is the value flowing in along the edge from `edges[i]`.
/// `None` mirrors upstream `PhiNode` `#undef` entries: the variable is
/// possibly undefined when control arrives along that edge.
///
/// Invariant (checked by [`super::verify`]): `edges` equals the owning
/// block's predecessor list exactly (same order, ascending by block id).
#[derive(Debug, Clone, PartialEq)]
pub struct PhiNode {
    /// Predecessor blocks contributing a value.
    pub edges: Vec<BlockId>,
    /// Incoming value per edge; `None` = maybe-undefined on that path.
    pub values: Vec<Option<SsaValue>>,
}

/// One SSA statement: a single definition produced by an operation.
#[derive(Debug, Clone, PartialEq)]
pub struct SsaStatement {
    /// The definition this statement produces.
    pub id: SsaValueId,
    /// The operation computing the definition.
    pub op: SsaOp,
    /// Source span of the originating Core IR node.
    pub span: Span,
}

/// Operations of the durable SSA model.
///
/// The decomposed set is intentionally small in this slice; every Core IR
/// expression or statement outside it is carried verbatim as
/// [`SsaOp::Opaque`] / [`SsaOp::OpaqueStmt`] with its resolved variable reads,
/// so no construct is silently dropped.
#[derive(Debug, Clone, PartialEq)]
pub enum SsaOp {
    /// Control-flow merge of per-predecessor values (block-prefix only).
    Phi(PhiNode),
    /// Unary operator application.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand value.
        operand: SsaValue,
    },
    /// Binary operator application. Note: `&&` / `||` short-circuiting is
    /// not yet modeled as control flow (see `docs/vm/SSA_IR.md`).
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: SsaValue,
        /// Right operand.
        right: SsaValue,
    },
    /// Function call (bare or module-qualified) by name.
    Call {
        /// Qualifying module for `Module.f(...)` calls, `None` for bare calls.
        module: Option<String>,
        /// Callee name.
        function: String,
        /// Positional argument values.
        args: Vec<SsaValue>,
        /// Keyword argument values.
        kwargs: Vec<(String, SsaValue)>,
        /// Positional splat mask (parallel to `args`), from Core IR.
        splat_mask: Vec<bool>,
        /// Keyword splat mask (parallel to `kwargs`), from Core IR.
        kwargs_splat_mask: Vec<bool>,
    },
    /// Builtin operation call.
    Builtin {
        /// Builtin identifier.
        name: BuiltinOp,
        /// Argument values.
        args: Vec<SsaValue>,
    },
    /// Read of a name with no dominating local binding (global / Base
    /// binding). Globals are not SSA-numbered in this slice; every read is a
    /// fresh load.
    LoadGlobal {
        /// Global binding name.
        name: String,
    },
    /// Write to a `global`-declared name.
    StoreGlobal {
        /// Global binding name.
        name: String,
        /// Stored value.
        value: SsaValue,
    },
    /// Core IR expression this slice does not decompose. `reads` resolves the
    /// locally-bound variables the expression may read (over-approximated);
    /// unbound names remain implicit global reads inside the payload.
    Opaque {
        /// Verbatim Core IR expression.
        expr: Box<Expr>,
        /// Locally-bound variables possibly read, with their SSA values.
        reads: Vec<(String, SsaValue)>,
    },
    /// Core IR statement treated as an opaque barrier (try/catch, loops not
    /// yet decomposed, mutation statements, ...). Every local variable the
    /// statement may rebind receives a fresh [`SsaOp::BarrierReload`]
    /// definition immediately afterwards.
    OpaqueStmt {
        /// Verbatim Core IR statement.
        stmt: Box<Stmt>,
        /// Locally-bound variables possibly read, with their SSA values.
        reads: Vec<(String, SsaValue)>,
    },
    /// Value of a local variable after an opaque barrier possibly rebound it
    /// ("spill across the barrier, reload after"). `barrier` is the def of
    /// the opaque statement, encoding the ordering dependency in dataflow.
    BarrierReload {
        /// Variable name reloaded.
        var: String,
        /// The barrier definition this reload depends on.
        barrier: SsaValue,
    },
}

impl SsaOp {
    /// Whether this operation is a [`PhiNode`].
    pub fn is_phi(&self) -> bool {
        matches!(self, Self::Phi(_))
    }

    /// Non-phi operand values, in evaluation order. Phi incoming values are
    /// per-edge and must be inspected through [`PhiNode`] directly.
    pub fn operands(&self) -> Vec<&SsaValue> {
        match self {
            Self::Phi(_) | Self::LoadGlobal { .. } => Vec::new(),
            Self::Unary { operand, .. } => vec![operand],
            Self::Binary { left, right, .. } => vec![left, right],
            Self::Call { args, kwargs, .. } => args
                .iter()
                .chain(kwargs.iter().map(|(_, value)| value))
                .collect(),
            Self::Builtin { args, .. } => args.iter().collect(),
            Self::StoreGlobal { value, .. } => vec![value],
            Self::Opaque { reads, .. } | Self::OpaqueStmt { reads, .. } => {
                reads.iter().map(|(_, value)| value).collect()
            }
            Self::BarrierReload { barrier, .. } => vec![barrier],
        }
    }

    /// Mutable references to every operand value, **including** phi incoming
    /// values. Unlike [`Self::operands`] (which excludes per-edge phi values
    /// so read-side analyses handle them edge-aware), mutation is used by the
    /// optimization passes to rewrite all use sites of a definition in place,
    /// and a phi incoming value is a use site like any other.
    pub fn operands_mut(&mut self) -> Vec<&mut SsaValue> {
        match self {
            Self::Phi(phi) => phi.values.iter_mut().flatten().collect(),
            Self::LoadGlobal { .. } => Vec::new(),
            Self::Unary { operand, .. } => vec![operand],
            Self::Binary { left, right, .. } => vec![left, right],
            Self::Call { args, kwargs, .. } => args
                .iter_mut()
                .chain(kwargs.iter_mut().map(|(_, value)| value))
                .collect(),
            Self::Builtin { args, .. } => args.iter_mut().collect(),
            Self::StoreGlobal { value, .. } => vec![value],
            Self::Opaque { reads, .. } | Self::OpaqueStmt { reads, .. } => {
                reads.iter_mut().map(|(_, value)| value).collect()
            }
            Self::BarrierReload { barrier, .. } => vec![barrier],
        }
    }
}

/// Block terminator. Every [`SsaBlock`] ends in exactly one terminator; the
/// block edge lists are derived from it (see [`Terminator::targets`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Unconditional jump.
    Jump {
        /// Target block.
        target: BlockId,
    },
    /// Two-way conditional branch on a Bool value.
    Branch {
        /// Branch condition (must evaluate to a Bool at runtime).
        condition: SsaValue,
        /// Target when the condition is true.
        then_target: BlockId,
        /// Target when the condition is false.
        else_target: BlockId,
    },
    /// Function return. `None` returns `nothing`.
    Return {
        /// Returned value, if any.
        value: Option<SsaValue>,
    },
}

impl Terminator {
    /// Successor blocks in terminator order, deduplicated.
    pub fn targets(&self) -> Vec<BlockId> {
        match self {
            Self::Jump { target } => vec![*target],
            Self::Branch {
                then_target,
                else_target,
                ..
            } => {
                if then_target == else_target {
                    vec![*then_target]
                } else {
                    vec![*then_target, *else_target]
                }
            }
            Self::Return { .. } => Vec::new(),
        }
    }

    /// Operand values read by the terminator.
    pub fn operands(&self) -> Vec<&SsaValue> {
        match self {
            Self::Jump { .. } | Self::Return { value: None } => Vec::new(),
            Self::Branch { condition, .. } => vec![condition],
            Self::Return { value: Some(value) } => vec![value],
        }
    }

    /// Mutable references to the operand values read by the terminator (used
    /// by the optimization passes to rewrite use sites in place).
    pub fn operands_mut(&mut self) -> Vec<&mut SsaValue> {
        match self {
            Self::Jump { .. } | Self::Return { value: None } => Vec::new(),
            Self::Branch { condition, .. } => vec![condition],
            Self::Return { value: Some(value) } => vec![value],
        }
    }
}

/// A basic block: phi prefix, ordered statements, one terminator, and
/// explicit predecessor/successor edges.
#[derive(Debug, Clone, PartialEq)]
pub struct SsaBlock {
    /// This block's id (equal to its index in [`SsaFunction::blocks`]).
    pub id: BlockId,
    /// Ordered statements; all [`SsaOp::Phi`] statements form a prefix.
    pub stmts: Vec<SsaStatement>,
    /// The single terminator ending the block.
    pub terminator: Terminator,
    /// Predecessor blocks, ascending by id (consistent with phi edges).
    pub preds: Vec<BlockId>,
    /// Successor blocks, in terminator order (consistent with
    /// [`Terminator::targets`]).
    pub succs: Vec<BlockId>,
}

impl SsaBlock {
    /// The phi statements at the head of the block.
    pub fn phis(&self) -> impl Iterator<Item = &SsaStatement> {
        self.stmts.iter().take_while(|stmt| stmt.op.is_phi())
    }
}

/// A function in SSA form.
#[derive(Debug, Clone, PartialEq)]
pub struct SsaFunction {
    /// Function name.
    pub name: String,
    /// Typed parameters (positional then keyword), referenced by
    /// [`SsaValue::Argument`] index.
    pub params: Vec<SsaParam>,
    /// Entry block id. The entry block has no predecessors.
    pub entry: BlockId,
    /// Basic blocks, indexed by [`BlockId`].
    pub blocks: Vec<SsaBlock>,
}

impl SsaFunction {
    /// Look up a block by id.
    pub fn block(&self, id: BlockId) -> Option<&SsaBlock> {
        self.blocks.get(id.0 as usize)
    }
}
