//! Constant propagation for type inference.
//!
//! This module implements constant evaluation and propagation during abstract interpretation.
//! When expressions involve only constant values, we can evaluate them at compile time
//! and track the resulting constant value rather than just the type.
//!
//! This enables:
//! - Better type precision (Const(42) is more specific than Int64)
//! - Dead code elimination (if constant condition is false)
//! - Constant folding during compilation

mod eval;

pub use eval::{eval_const_binary, eval_const_unary};

use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
#[cfg(test)]
use crate::inference_core::{CorePrimitive, CoreType};
use crate::ir::core::{Expr, Literal};
use crate::types::JuliaType;

use super::utils::{binary_op_to_function_name, unary_op_to_function_name};

/// Try to evaluate a binary operation on two lattice types.
///
/// If both operands are constants, evaluate the operation and return Const(result).
/// Otherwise, return None.
///
/// Also folds `===`/`!==` between two statically-known type *objects*
/// (`LatticeType::Concrete(ConcreteType::DataType { name })`, e.g. a
/// where-bound `Type{T}` parameter narrowed to a concrete argument, or a bare
/// reference to a global type name). This lets a type-level ternary like
/// `T === BigInt ? BigFloat : Float64` collapse to the precise branch during
/// abstract interpretation instead of widening to `Union`/`Any`
/// (Issue #10133, a narrow slice of the `TypeValue` lattice epic #10045 that
/// intentionally does not generalize beyond `===`/`!==` identity folding).
///
/// Folding to `true` on name equality is always sound (identical canonical
/// spellings denote the same type object). Folding to `false` is only
/// attempted when the two names round-trip through [`JuliaType::from_name`]
/// (i.e., are recognized builtin type names) AND both resolve to a
/// *concrete* type ([`JuliaType::is_concrete`]) — a where-bound type
/// parameter reflected on with an ABSTRACT call-site type (e.g.
/// `Base.infer_return_type(f, Tuple{Integer})` binds `T` to the abstract
/// name `"Integer"`, not a specific concrete instantiation) denotes a SET of
/// possible concrete types, so `T === Int64` is not always `false` merely
/// because the spellings differ — some member of that set (`Int64` itself)
/// makes it `true`. Folding `false` there would prune a reachable branch
/// (verified against upstream: `f(x::T) where T = T === Int64 ? 1 : "no"`
/// reflected on `Tuple{Integer}` returns the conservative `Union{Int64,
/// String}`, not the over-pruned `String`). Struct/parametric spellings are
/// also left as `None` here (no struct table is reachable from this free
/// function, so their canonical form can't be verified) rather than risking
/// a wrong `false` fold on two different spellings of the same type.
pub fn try_eval_binary(op: &str, left: &LatticeType, right: &LatticeType) -> Option<LatticeType> {
    match (left, right) {
        (LatticeType::Const(lv), LatticeType::Const(rv)) => {
            eval_const_binary(op, lv, rv).map(LatticeType::Const)
        }
        (
            LatticeType::Concrete(ConcreteType::DataType { name: left_name }),
            LatticeType::Concrete(ConcreteType::DataType { name: right_name }),
        ) if op == "===" || op == "!==" => {
            let equal = if left_name == right_name {
                true
            } else {
                match (
                    JuliaType::from_name(left_name),
                    JuliaType::from_name(right_name),
                ) {
                    (Some(lt), Some(rt)) if lt.is_concrete() && rt.is_concrete() => false,
                    _ => return None,
                }
            };
            let result = if op == "===" { equal } else { !equal };
            Some(LatticeType::Const(ConstValue::Bool(result)))
        }
        _ => None,
    }
}

/// Try to evaluate a unary operation on a lattice type.
///
/// If the operand is a constant, evaluate the operation and return Const(result).
/// Otherwise, return None.
pub fn try_eval_unary(op: &str, operand: &LatticeType) -> Option<LatticeType> {
    match operand {
        LatticeType::Const(v) => eval_const_unary(op, v).map(LatticeType::Const),
        _ => None,
    }
}

/// Convert a scalar Core IR literal into a [`ConstValue`].
///
/// Non-scalar literal forms (arrays, structs, modules, ...) return `None` so
/// callers cannot accidentally treat aggregate payloads as foldable scalars.
pub fn literal_to_const_value(lit: &Literal) -> Option<ConstValue> {
    match lit {
        Literal::Int(v) => Some(ConstValue::Int64(*v)),
        Literal::Float(v) => Some(ConstValue::Float64(*v)),
        Literal::Bool(v) => Some(ConstValue::Bool(*v)),
        Literal::Str(s) => Some(ConstValue::String(s.clone())),
        Literal::Symbol(s) => Some(ConstValue::Symbol(s.clone())),
        Literal::Nothing => Some(ConstValue::Nothing),
        _ => None,
    }
}

/// Convert an evaluated [`ConstValue`] back into a Core IR literal (the exact
/// inverse of [`literal_to_const_value`] on its supported subset).
pub fn const_value_to_literal(value: ConstValue) -> Literal {
    match value {
        ConstValue::Int64(v) => Literal::Int(v),
        ConstValue::Float64(v) => Literal::Float(v),
        ConstValue::Bool(v) => Literal::Bool(v),
        ConstValue::String(s) => Literal::Str(s),
        ConstValue::Symbol(s) => Literal::Symbol(s),
        ConstValue::Nothing => Literal::Nothing,
    }
}

/// Fold a pure expression to a compile-time constant.
///
/// `lookup_const` supplies already-known `const` bindings. Only literals,
/// `const` variable reads, unary operators, and binary operators handled by the
/// shared const evaluator participate; calls, indexing, and other expressions
/// return `None` so folding cannot erase side effects.
pub fn fold_expr_const_value<F>(expr: &Expr, lookup_const: &F) -> Option<ConstValue>
where
    F: Fn(&str) -> Option<ConstValue>,
{
    match expr {
        Expr::Literal(lit, _) => literal_to_const_value(lit),
        Expr::Var(name, _) => lookup_const(name),
        Expr::UnaryOp { op, operand, .. } => {
            let v = fold_expr_const_value(operand, lookup_const)?;
            eval_const_unary(unary_op_to_function_name(op), &v)
        }
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let op_str = binary_op_to_function_name(op);
            let l = fold_expr_const_value(left, lookup_const)?;
            let r = fold_expr_const_value(right, lookup_const)?;
            eval_const_binary(op_str, &l, &r)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::{ConstValue, LatticeType};

    // ── try_eval_binary ──────────────────────────────────────────────────────

    #[test]
    fn test_try_eval_binary_both_const_int_add() {
        let left = LatticeType::Const(ConstValue::Int64(10));
        let right = LatticeType::Const(ConstValue::Int64(5));
        let result = try_eval_binary("+", &left, &right);
        assert_eq!(result, Some(LatticeType::Const(ConstValue::Int64(15))));
    }

    #[test]
    fn test_try_eval_binary_both_const_bool_and() {
        let left = LatticeType::Const(ConstValue::Bool(true));
        let right = LatticeType::Const(ConstValue::Bool(false));
        let result = try_eval_binary("&&", &left, &right);
        assert_eq!(result, Some(LatticeType::Const(ConstValue::Bool(false))));
    }

    #[test]
    fn test_try_eval_binary_left_non_const_returns_none() {
        // When left is not Const, result is always None (can't evaluate)
        let left = LatticeType::Concrete(crate::compile::lattice::types::ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ));
        let right = LatticeType::Const(ConstValue::Int64(5));
        let result = try_eval_binary("+", &left, &right);
        assert!(result.is_none(), "Expected None when left is not Const");
    }

    #[test]
    fn test_try_eval_binary_right_non_const_returns_none() {
        // When right is not Const, result is always None
        let left = LatticeType::Const(ConstValue::Int64(10));
        let right = LatticeType::Top;
        let result = try_eval_binary("+", &left, &right);
        assert!(result.is_none(), "Expected None when right is Top");
    }

    #[test]
    fn test_try_eval_binary_both_non_const_returns_none() {
        let left = LatticeType::Top;
        let right = LatticeType::Bottom;
        let result = try_eval_binary("+", &left, &right);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_eval_binary_unsupported_op_returns_none() {
        // Even with two Const values, unsupported ops return None
        let left = LatticeType::Const(ConstValue::Int64(10));
        let right = LatticeType::Const(ConstValue::Int64(5));
        let result = try_eval_binary("unknown_op", &left, &right);
        assert!(result.is_none(), "Expected None for unsupported op");
    }

    // ── try_eval_unary ───────────────────────────────────────────────────────

    #[test]
    fn test_try_eval_unary_const_int_negation() {
        let operand = LatticeType::Const(ConstValue::Int64(42));
        let result = try_eval_unary("-", &operand);
        assert_eq!(result, Some(LatticeType::Const(ConstValue::Int64(-42))));
    }

    #[test]
    fn test_try_eval_unary_const_bool_not() {
        let operand = LatticeType::Const(ConstValue::Bool(true));
        let result = try_eval_unary("!", &operand);
        assert_eq!(result, Some(LatticeType::Const(ConstValue::Bool(false))));
    }

    #[test]
    fn test_try_eval_unary_non_const_returns_none() {
        let operand = LatticeType::Top;
        let result = try_eval_unary("-", &operand);
        assert!(result.is_none(), "Expected None when operand is Top");
    }

    #[test]
    fn test_try_eval_unary_concrete_type_returns_none() {
        let operand = LatticeType::Concrete(crate::compile::lattice::types::ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ));
        let result = try_eval_unary("-", &operand);
        assert!(
            result.is_none(),
            "Expected None for non-Const Concrete type"
        );
    }
}
