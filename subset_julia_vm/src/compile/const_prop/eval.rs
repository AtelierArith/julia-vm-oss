//! Constant expression evaluation.
//!
//! This module implements compile-time evaluation of pure operations on constant values.

use crate::compile::lattice::types::ConstValue;

/// Evaluate a binary operation on two constant values.
///
/// Returns Some(result) if the operation can be evaluated at compile time,
/// or None if the operation is not supported or would cause an error.
pub fn eval_const_binary(op: &str, lhs: &ConstValue, rhs: &ConstValue) -> Option<ConstValue> {
    match (op, lhs, rhs) {
        // Integer arithmetic
        ("+", ConstValue::Int64(a), ConstValue::Int64(b)) => {
            a.checked_add(*b).map(ConstValue::Int64)
        }
        ("-", ConstValue::Int64(a), ConstValue::Int64(b)) => {
            a.checked_sub(*b).map(ConstValue::Int64)
        }
        ("*", ConstValue::Int64(a), ConstValue::Int64(b)) => {
            a.checked_mul(*b).map(ConstValue::Int64)
        }
        ("/", ConstValue::Int64(a), ConstValue::Int64(b)) => {
            if *b != 0 {
                // Julia's / always returns Float64 for integers
                Some(ConstValue::Float64(*a as f64 / *b as f64))
            } else {
                None // Division by zero
            }
        }
        ("÷", ConstValue::Int64(a), ConstValue::Int64(b)) => {
            if *b != 0 {
                a.checked_div(*b).map(ConstValue::Int64)
            } else {
                None // Division by zero
            }
        }
        ("%", ConstValue::Int64(a), ConstValue::Int64(b)) => {
            if *b != 0 {
                // Julia's % is rem (truncated remainder), same as Rust's %
                Some(ConstValue::Int64(a % b))
            } else {
                None // Division by zero
            }
        }

        // Float arithmetic
        ("+", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Float64(a + b)),
        ("-", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Float64(a - b)),
        ("*", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Float64(a * b)),
        ("/", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Float64(a / b)),
        ("%", ConstValue::Float64(a), ConstValue::Float64(b)) => {
            // Julia's % is rem (truncated remainder), same as Rust's % for f64
            Some(ConstValue::Float64(a % b))
        }

        // Mixed int/float arithmetic (promote to float)
        ("+", ConstValue::Int64(a), ConstValue::Float64(b)) => {
            Some(ConstValue::Float64(*a as f64 + b))
        }
        ("+", ConstValue::Float64(a), ConstValue::Int64(b)) => {
            Some(ConstValue::Float64(a + *b as f64))
        }
        ("-", ConstValue::Int64(a), ConstValue::Float64(b)) => {
            Some(ConstValue::Float64(*a as f64 - b))
        }
        ("-", ConstValue::Float64(a), ConstValue::Int64(b)) => {
            Some(ConstValue::Float64(a - *b as f64))
        }
        ("*", ConstValue::Int64(a), ConstValue::Float64(b)) => {
            Some(ConstValue::Float64(*a as f64 * b))
        }
        ("*", ConstValue::Float64(a), ConstValue::Int64(b)) => {
            Some(ConstValue::Float64(a * *b as f64))
        }
        ("/", ConstValue::Int64(a), ConstValue::Float64(b)) => {
            Some(ConstValue::Float64(*a as f64 / b))
        }
        ("/", ConstValue::Float64(a), ConstValue::Int64(b)) => {
            Some(ConstValue::Float64(a / *b as f64))
        }

        // Integer comparisons
        ("<", ConstValue::Int64(a), ConstValue::Int64(b)) => Some(ConstValue::Bool(a < b)),
        ("<=", ConstValue::Int64(a), ConstValue::Int64(b)) => Some(ConstValue::Bool(a <= b)),
        (">", ConstValue::Int64(a), ConstValue::Int64(b)) => Some(ConstValue::Bool(a > b)),
        (">=", ConstValue::Int64(a), ConstValue::Int64(b)) => Some(ConstValue::Bool(a >= b)),
        ("==", ConstValue::Int64(a), ConstValue::Int64(b)) => Some(ConstValue::Bool(a == b)),
        ("!=", ConstValue::Int64(a), ConstValue::Int64(b)) => Some(ConstValue::Bool(a != b)),

        // Float comparisons
        ("<", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Bool(a < b)),
        ("<=", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Bool(a <= b)),
        (">", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Bool(a > b)),
        (">=", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Bool(a >= b)),
        ("==", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Bool(a == b)),
        ("!=", ConstValue::Float64(a), ConstValue::Float64(b)) => Some(ConstValue::Bool(a != b)),

        // Boolean operations
        ("&&", ConstValue::Bool(a), ConstValue::Bool(b)) => Some(ConstValue::Bool(*a && *b)),
        ("||", ConstValue::Bool(a), ConstValue::Bool(b)) => Some(ConstValue::Bool(*a || *b)),
        ("==", ConstValue::Bool(a), ConstValue::Bool(b)) => Some(ConstValue::Bool(a == b)),
        ("!=", ConstValue::Bool(a), ConstValue::Bool(b)) => Some(ConstValue::Bool(a != b)),

        // String operations
        ("*", ConstValue::String(a), ConstValue::String(b)) => {
            Some(ConstValue::String(format!("{}{}", a, b)))
        }
        ("==", ConstValue::String(a), ConstValue::String(b)) => Some(ConstValue::Bool(a == b)),
        ("!=", ConstValue::String(a), ConstValue::String(b)) => Some(ConstValue::Bool(a != b)),

        // Nothing comparisons
        ("==", ConstValue::Nothing, ConstValue::Nothing) => Some(ConstValue::Bool(true)),
        ("!=", ConstValue::Nothing, ConstValue::Nothing) => Some(ConstValue::Bool(false)),

        _ => None, // Unsupported operation
    }
}

/// Evaluate a unary operation on a constant value.
///
/// Returns Some(result) if the operation can be evaluated at compile time,
/// or None if the operation is not supported.
pub fn eval_const_unary(op: &str, operand: &ConstValue) -> Option<ConstValue> {
    match (op, operand) {
        // Numeric negation
        ("-", ConstValue::Int64(v)) => v.checked_neg().map(ConstValue::Int64),
        ("-", ConstValue::Float64(v)) => Some(ConstValue::Float64(-v)),

        // Numeric positive (identity)
        ("+", ConstValue::Int64(v)) => Some(ConstValue::Int64(*v)),
        ("+", ConstValue::Float64(v)) => Some(ConstValue::Float64(*v)),

        // Boolean negation
        ("!", ConstValue::Bool(v)) => Some(ConstValue::Bool(!v)),

        _ => None, // Unsupported operation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_arithmetic() {
        assert_eq!(
            eval_const_binary("+", &ConstValue::Int64(2), &ConstValue::Int64(3)),
            Some(ConstValue::Int64(5))
        );
        assert_eq!(
            eval_const_binary("-", &ConstValue::Int64(5), &ConstValue::Int64(3)),
            Some(ConstValue::Int64(2))
        );
        assert_eq!(
            eval_const_binary("*", &ConstValue::Int64(2), &ConstValue::Int64(3)),
            Some(ConstValue::Int64(6))
        );
    }

    #[test]
    fn test_int_division() {
        // Julia's / always returns Float64
        assert_eq!(
            eval_const_binary("/", &ConstValue::Int64(6), &ConstValue::Int64(2)),
            Some(ConstValue::Float64(3.0))
        );
        // Integer division
        assert_eq!(
            eval_const_binary("÷", &ConstValue::Int64(7), &ConstValue::Int64(2)),
            Some(ConstValue::Int64(3))
        );
    }

    #[test]
    fn test_bool_ops() {
        assert_eq!(
            eval_const_binary("&&", &ConstValue::Bool(true), &ConstValue::Bool(false)),
            Some(ConstValue::Bool(false))
        );
        assert_eq!(
            eval_const_binary("||", &ConstValue::Bool(true), &ConstValue::Bool(false)),
            Some(ConstValue::Bool(true))
        );
    }

    #[test]
    fn test_comparisons() {
        assert_eq!(
            eval_const_binary("<", &ConstValue::Int64(2), &ConstValue::Int64(3)),
            Some(ConstValue::Bool(true))
        );
        assert_eq!(
            eval_const_binary("==", &ConstValue::Int64(2), &ConstValue::Int64(2)),
            Some(ConstValue::Bool(true))
        );
    }

    #[test]
    fn test_int_remainder_positive() {
        // Julia: 7 % 3 == 1
        assert_eq!(
            eval_const_binary("%", &ConstValue::Int64(7), &ConstValue::Int64(3)),
            Some(ConstValue::Int64(1))
        );
    }

    #[test]
    fn test_int_remainder_negative_dividend() {
        // Julia: -7 % 3 == -1  (truncated remainder, NOT rem_euclid)
        assert_eq!(
            eval_const_binary("%", &ConstValue::Int64(-7), &ConstValue::Int64(3)),
            Some(ConstValue::Int64(-1))
        );
    }

    #[test]
    fn test_int_remainder_by_zero_returns_none() {
        assert_eq!(
            eval_const_binary("%", &ConstValue::Int64(7), &ConstValue::Int64(0)),
            None
        );
    }

    #[test]
    fn test_float_remainder_positive() {
        // Julia: 7.0 % 3.0 == 1.0
        let result = eval_const_binary("%", &ConstValue::Float64(7.0), &ConstValue::Float64(3.0));
        assert_eq!(result, Some(ConstValue::Float64(1.0)));
    }

    #[test]
    fn test_float_remainder_negative_dividend() {
        // Julia: -7.0 % 3.0 == -1.0  (truncated remainder, NOT floor-division mod)
        let result =
            eval_const_binary("%", &ConstValue::Float64(-7.0), &ConstValue::Float64(3.0));
        assert_eq!(result, Some(ConstValue::Float64(-1.0)));
    }

    #[test]
    fn test_unary_ops() {
        assert_eq!(
            eval_const_unary("-", &ConstValue::Int64(5)),
            Some(ConstValue::Int64(-5))
        );
        assert_eq!(
            eval_const_unary("!", &ConstValue::Bool(true)),
            Some(ConstValue::Bool(false))
        );
    }
}
