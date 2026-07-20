//! Function-body effect walker shared by compile-time propagation and runtime
//! reflection.
//!
//! This module stays below `compile` so VM reflection can compose
//! `Base.infer_effects` summaries without depending on
//! `compile::effects::propagation` ownership (Issue #9090). Whole-program
//! fixpoint propagation and name-level method merging remain compile-side.

use super::effect_inference::infer_expr_effects_with_callees;
use super::Effects;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Stmt};
use std::collections::HashMap;

/// Function identifier for call graph tracking.
pub type FuncId = String;

/// Infer the effect summary for one already-resolved method body.
///
/// Reflection call sites already resolved dispatch to a concrete method set, so
/// they can reuse the same body walker as whole-program propagation without
/// rebuilding a synthetic Program.
pub fn infer_function_effects(
    func: &Function,
    callee_effects: &HashMap<FuncId, Effects>,
) -> Effects {
    compute_function_effects(func, callee_effects)
}

/// Compute effects for a single function based on its body and callee effects.
///
/// Exposed to `compile::effects` so whole-program propagation and static
/// dispatch summaries use the same body walker as runtime reflection.
pub fn compute_function_effects(
    func: &Function,
    effects_map: &HashMap<FuncId, Effects>,
) -> Effects {
    compute_block_effects(&func.body, effects_map)
}

/// Compute effects for a block of statements.
fn compute_block_effects(block: &Block, effects_map: &HashMap<FuncId, Effects>) -> Effects {
    let mut result = Effects::total();
    for stmt in &block.stmts {
        result = result.merge(&compute_stmt_effects(stmt, effects_map));
    }
    result
}

/// Compute effects for a statement.
fn compute_stmt_effects(stmt: &Stmt, effects_map: &HashMap<FuncId, Effects>) -> Effects {
    match stmt {
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Expr { expr: value, .. } => compute_expr_effects(value, effects_map),
        Stmt::For {
            body,
            start,
            end,
            step,
            ..
        } => {
            let mut eff = compute_expr_effects(start, effects_map);
            eff = eff.merge(&compute_expr_effects(end, effects_map));
            if let Some(step_expr) = step {
                eff = eff.merge(&compute_expr_effects(step_expr, effects_map));
            }
            eff = eff.merge(&compute_block_effects(body, effects_map));
            Effects {
                terminates: false,
                ..eff
            }
        }
        Stmt::ForEach { body, iterable, .. } | Stmt::ForEachTuple { body, iterable, .. } => {
            let mut eff = compute_expr_effects(iterable, effects_map);
            eff = eff.merge(&compute_block_effects(body, effects_map));
            Effects {
                terminates: false,
                ..eff
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            let mut eff = compute_expr_effects(condition, effects_map);
            eff = eff.merge(&compute_block_effects(body, effects_map));
            Effects {
                terminates: false,
                ..eff
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let mut eff = compute_expr_effects(condition, effects_map);
            eff = eff.merge(&compute_block_effects(then_branch, effects_map));
            if let Some(else_b) = else_branch {
                eff = eff.merge(&compute_block_effects(else_b, effects_map));
            }
            eff
        }
        Stmt::Return { value, .. } => {
            if let Some(val) = value {
                compute_expr_effects(val, effects_map)
            } else {
                Effects::total()
            }
        }
        Stmt::Block(block) => compute_block_effects(block, effects_map),
        Stmt::Try {
            try_block,
            catch_block,
            finally_block,
            ..
        } => {
            let mut eff = compute_block_effects(try_block, effects_map);
            if let Some(catch_b) = catch_block {
                eff = eff.merge(&compute_block_effects(catch_b, effects_map));
            }
            if let Some(finally_b) = finally_block {
                eff = eff.merge(&compute_block_effects(finally_b, effects_map));
            }
            eff
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            compute_block_effects(body, effects_map)
        }
        _ => Effects::total(),
    }
}

/// Compute effects for an expression, looking up callee effects.
fn compute_expr_effects(expr: &Expr, effects_map: &HashMap<FuncId, Effects>) -> Effects {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            let callee_effects = effects_map
                .get(function.as_str())
                .copied()
                .unwrap_or_else(Effects::arbitrary);

            let mut result = callee_effects;
            for arg in args {
                result = result.merge(&compute_expr_effects(arg, effects_map));
            }
            for (_, value) in kwargs {
                result = result.merge(&compute_expr_effects(value, effects_map));
            }
            result
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            let callee_effects = effects_map
                .get(function.as_str())
                .copied()
                .unwrap_or_else(Effects::arbitrary);

            let mut result = callee_effects;
            for arg in args {
                result = result.merge(&compute_expr_effects(arg, effects_map));
            }
            for (_, value) in kwargs {
                result = result.merge(&compute_expr_effects(value, effects_map));
            }
            result
        }
        Expr::LetBlock { bindings, body, .. } => {
            let mut result = Effects::total();
            for (_, value) in bindings {
                result = result.merge(&compute_expr_effects(value, effects_map));
            }
            result.merge(&compute_block_effects(body, effects_map))
        }
        // Ternary (`cond ? a : b`) and short-circuit `&&`/`||` are
        // control-flow-as-expression: exactly like `Stmt::If`, at most one of
        // the branch/operand sub-expressions actually runs, so a call hiding
        // in an unreached branch must still taint the summary conservatively
        // (its effects are unknowable statically) — the same reasoning
        // `Stmt::If` already applies to `then_branch`/`else_branch` above.
        // Route each sub-expression back through `compute_expr_effects`
        // (not the `infer_expr_effects_with_callees` bridge below) so a
        // `Call` nested in a branch/operand — e.g. `println` in
        // `x ? println("p") : 1` or `x && println("p")` — resolves via the
        // same effects_map lookup (missing callee => `Effects::arbitrary()`)
        // as a bare top-level statement call or an `if`/`else` branch,
        // instead of the curated builtin-name-table fallback the bridge uses
        // for calls nested in ordinary (non-control-flow) operator operands.
        // Without this arm, an unresolved side-effecting call reachable only
        // through a ternary/short-circuit branch was scored by the
        // optimistic name-table hint (e.g. `println` classified
        // `with_side_effects()`, which claims `terminates = true`) instead
        // of the conservative `arbitrary()` default — silently over-claiming
        // termination (and other bits) for both `Base.infer_effects`
        // reflection and this walker's compile-time CSE/DCE consumers
        // (Issue #10368).
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            let mut eff = compute_expr_effects(condition, effects_map);
            eff = eff.merge(&compute_expr_effects(then_expr, effects_map));
            eff = eff.merge(&compute_expr_effects(else_expr, effects_map));
            eff
        }
        Expr::BinaryOp {
            op: BinaryOp::And | BinaryOp::Or,
            left,
            right,
            ..
        } => {
            compute_expr_effects(left, effects_map).merge(&compute_expr_effects(right, effects_map))
        }
        _ => {
            // Calls nested inside (non-control-flow) operator operands,
            // literal elements, index expressions, and similar positions
            // still consult body-derived callee summaries first. Names
            // without a summary fall back to the curated builtin name table
            // (Issue #8441). `Expr::Ternary` and short-circuit
            // `Expr::BinaryOp{And,Or}` are handled above instead, NOT here:
            // their branches/operands are control-flow-conditional (like
            // `Stmt::If`), so an unresolved callee must fall back to
            // `Effects::arbitrary()` the same way a bare statement call or an
            // if/else branch does, not the curated name table (Issue #10368).
            infer_expr_effects_with_callees(expr, &|name| effects_map.get(name).copied())
        }
    }
}
