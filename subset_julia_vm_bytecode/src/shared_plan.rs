//! Backend-neutral SSA lowering plan shared by stack and register backends.
//!
//! The planner itself lives in `subset_julia_vm::compile::ssa_ir::plan`; these
//! data types live in the bytecode crate so compiled function metadata can
//! carry a runtime-only copy without depending back on the compiler crate.

use serde::{Deserialize, Serialize};
use subset_julia_vm_ir::Span;
use subset_julia_vm_types::ir::core::Expr;

/// One reconstructed root statement (a definition that could not be folded
/// into a consumer's operand tree, or a value discarded in statement position).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SharedRootPlan {
    /// `name = expr` — spilled definition, global store, or phi-edge copy.
    Assign {
        name: String,
        expr: Expr,
        span: Span,
    },
    /// Value computed for effect only; the legacy `Stmt::Expr` emission
    /// discards it.
    Discard { expr: Expr, span: Span },
}

/// Phi-edge copy: `#ssaN = expr`, emitted on the incoming edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedCopyPlan {
    pub name: String,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SharedTermPlan {
    Return {
        expr: Option<Expr>,
    },
    Jump {
        target: u32,
        copies: Vec<SharedCopyPlan>,
    },
    Branch {
        cond: Expr,
        then_target: u32,
        else_target: u32,
        then_copies: Vec<SharedCopyPlan>,
        else_copies: Vec<SharedCopyPlan>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedBlockPlan {
    roots: Vec<SharedRootPlan>,
    terminator: SharedTermPlan,
}

impl SharedBlockPlan {
    pub fn new(roots: Vec<SharedRootPlan>, terminator: SharedTermPlan) -> Self {
        Self { roots, terminator }
    }

    pub fn roots(&self) -> &[SharedRootPlan] {
        &self.roots
    }

    pub fn terminator(&self) -> &SharedTermPlan {
        &self.terminator
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedFunctionPlan {
    blocks: Vec<SharedBlockPlan>,
}

impl SharedFunctionPlan {
    pub fn new(blocks: Vec<SharedBlockPlan>) -> Self {
        Self { blocks }
    }

    pub fn blocks(&self) -> &[SharedBlockPlan] {
        &self.blocks
    }
}
