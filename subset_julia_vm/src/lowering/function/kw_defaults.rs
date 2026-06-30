//! Per-call keyword-argument default re-evaluation (Issue #5121).
//!
//! Upstream Julia evaluates a keyword argument's default *expression* on every
//! call where the keyword is omitted — not once at definition time. For a
//! side-effecting default (e.g. `f(x; k = next!())`) the side effect must run
//! per call.
//!
//! SubsetJuliaVM previously evaluated optional kwarg defaults either by baking a
//! constant `Value` into the method or by running a throwaway "default
//! side-interpreter" that cannot persist global mutation and cannot run an
//! arbitrary user function call. Both produce wrong results for defaults that
//! call a (potentially side-effecting) function.
//!
//! The fix mirrors how upstream's kwsorter generates the body method: when a
//! default expression *invokes a function* (and is therefore potentially
//! side-effecting / not a pure constant), the keyword's slot is bound to a
//! sentinel (`Value::Undef`) by the kwsorter, and a guard is prepended to the
//! function body that re-evaluates the default in the real call frame:
//!
//! ```text
//! function f(x; k = compute())   # `compute()` is a Call -> body-evaluated
//!     if k === <undef sentinel>
//!         k = compute()
//!     end
//!     ...original body...
//! end
//! ```
//!
//! Because the guard runs in the real frame, the default sees the call site's
//! captured environment, mutates globals/refs that persist, and re-runs on
//! every omitted-keyword call. Trivial defaults (literals, plain variable
//! references, arithmetic on earlier parameters/keywords) are left on the
//! existing fast path so type inference and the kwarg-default fixtures that
//! depend on it are unaffected.

use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt};

/// Rewrite a function so that any keyword argument whose default expression is
/// potentially side-effecting (i.e. invokes a function) is re-evaluated per
/// call inside the body. See the module docs for the desugaring.
pub(super) fn inject_kwarg_default_prologues(mut func: Function) -> Function {
    if func.kwparams.is_empty() {
        return func;
    }

    // Build the prologue guards in declaration order so that a later default can
    // observe an earlier default that was just re-evaluated (matching upstream's
    // left-to-right kwsorter evaluation). Guards are prepended in front of the
    // original body, preserving that order.
    let mut prologue: Vec<Stmt> = Vec::new();
    for kwparam in func.kwparams.iter_mut() {
        if kwparam.is_varargs {
            continue;
        }
        // Required kwargs (Undef default) and trivial defaults stay on the
        // existing path.
        if !default_needs_body_eval(&kwparam.default) {
            continue;
        }

        kwparam.body_evaluated_default = true;
        let span = kwparam.span;
        let name = kwparam.name.clone();

        // if <name> === Undef { <name> = <default> }
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var(name.clone(), span)),
            right: Box::new(Expr::Literal(Literal::Undef, span)),
            span,
        };
        let assign = Stmt::Assign {
            var: name,
            value: kwparam.default.clone(),
            span,
        };
        prologue.push(Stmt::If {
            condition,
            then_branch: Block {
                stmts: vec![assign],
                span,
            },
            else_branch: None,
            span,
        });
    }

    if prologue.is_empty() {
        return func;
    }

    prologue.extend(func.body.stmts);
    func.body.stmts = prologue;
    func
}

/// Returns `true` when the default expression invokes a function and therefore
/// must be re-evaluated per call in the real frame (it may be side-effecting,
/// observe mutable state, or call a user function the default side-interpreter
/// cannot run). Plain literals, variable references, and operators applied to
/// such operands are left on the existing fast path.
fn default_needs_body_eval(expr: &Expr) -> bool {
    match expr {
        // A direct function invocation: the primary gap this fix targets.
        Expr::Call { .. } | Expr::ModuleCall { .. } => true,

        // A conditional default (`atol > 0 ? 0 : eps`) cannot be evaluated by
        // the throwaway default side-interpreter (it has no `Ternary` arm), so
        // evaluating it in the real body is both correct and necessary —
        // independent of whether it contains a call.
        Expr::Ternary { .. } => true,

        // Recurse through operators / containers so that a call buried inside a
        // larger default expression (e.g. `k = 1 + compute()`) is still caught.
        Expr::BinaryOp { left, right, .. } => {
            default_needs_body_eval(left) || default_needs_body_eval(right)
        }
        Expr::UnaryOp { operand, .. } => default_needs_body_eval(operand),
        // An array/tuple-*construction* default cannot be materialized by the
        // pre-evaluated default fast path: `eval_literal_default` only handles
        // the folded `Literal::*` variants, so a source `Expr::ArrayLiteral` /
        // `Expr::TypedEmptyArray` / comprehension / `Expr::TupleLiteral` falls
        // through to its `Value::I64(0)` fallback and the slot is bound to `0`.
        // Re-evaluating in the real frame both fixes that and matches upstream's
        // per-call semantics: an array-literal default yields a *fresh* array on
        // every omitted-keyword call (Issue #6876).
        Expr::TupleLiteral { .. }
        | Expr::ArrayLiteral { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::Comprehension { .. }
        | Expr::MultiComprehension { .. } => true,
        Expr::StringConcat { parts, .. } => parts.iter().any(default_needs_body_eval),
        Expr::Pair { key, value, .. } => {
            default_needs_body_eval(key) || default_needs_body_eval(value)
        }
        _ => false,
    }
}
