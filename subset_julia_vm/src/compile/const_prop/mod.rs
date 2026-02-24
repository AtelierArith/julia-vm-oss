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

use crate::compile::lattice::types::LatticeType;

/// Try to evaluate a binary operation on two lattice types.
///
/// If both operands are constants, evaluate the operation and return Const(result).
/// Otherwise, return None.
pub fn try_eval_binary(op: &str, left: &LatticeType, right: &LatticeType) -> Option<LatticeType> {
    match (left, right) {
        (LatticeType::Const(lv), LatticeType::Const(rv)) => {
            eval_const_binary(op, lv, rv).map(LatticeType::Const)
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
        let left = LatticeType::Concrete(crate::compile::lattice::types::ConcreteType::Int64);
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
        let operand =
            LatticeType::Concrete(crate::compile::lattice::types::ConcreteType::Float64);
        let result = try_eval_unary("-", &operand);
        assert!(result.is_none(), "Expected None for non-Const Concrete type");
    }
}
