//! Transfer functions for mathematical intrinsic operations.
//!
//! This module implements type inference for Julia's mathematical intrinsics,
//! including sign, div, rem, mod, floor, ceil, and round operations.

use crate::compile::lattice::types::{ConcreteType, LatticeType};

/// Transfer function for `sign` (sign of a number).
///
/// Type rules:
/// - sign(Int*) → Int64
/// - sign(Float*) → Float64
///
/// # Examples
/// ```text
/// sign(Int64) → Int64
/// sign(Float64) → Float64
/// ```
pub fn tfunc_sign(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) => match ct {
            ConcreteType::Int8
            | ConcreteType::Int16
            | ConcreteType::Int32
            | ConcreteType::Int64
            | ConcreteType::Int128
            | ConcreteType::UInt8
            | ConcreteType::UInt16
            | ConcreteType::UInt32
            | ConcreteType::UInt64
            | ConcreteType::UInt128 => LatticeType::Concrete(ConcreteType::Int64),
            ConcreteType::Float32 | ConcreteType::Float64 => {
                LatticeType::Concrete(ConcreteType::Float64)
            }
            ConcreteType::BigInt => LatticeType::Concrete(ConcreteType::BigInt),
            ConcreteType::BigFloat => LatticeType::Concrete(ConcreteType::BigFloat),
            _ => LatticeType::Top,
        },
        _ => LatticeType::Top,
    }
}

/// Transfer function for `div` (integer division).
///
/// Type rules:
/// - div(Int, Int) → Int
/// - div(Float, Float) → Float
///
/// # Examples
/// ```text
/// div(Int64, Int64) → Int64
/// div(Float64, Float64) → Float64
/// ```
pub fn tfunc_div(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match (&args[0], &args[1]) {
        (LatticeType::Concrete(ct1), LatticeType::Concrete(ct2)) => {
            match (ct1, ct2) {
                // Integer division
                (ConcreteType::Int64, ConcreteType::Int64) => {
                    LatticeType::Concrete(ConcreteType::Int64)
                }
                (ConcreteType::Int32, ConcreteType::Int32) => {
                    LatticeType::Concrete(ConcreteType::Int32)
                }
                (ConcreteType::BigInt, _) | (_, ConcreteType::BigInt) => {
                    LatticeType::Concrete(ConcreteType::BigInt)
                }
                // Float division
                (ConcreteType::Float64, ConcreteType::Float64) => {
                    LatticeType::Concrete(ConcreteType::Float64)
                }
                (ConcreteType::Float32, ConcreteType::Float32) => {
                    LatticeType::Concrete(ConcreteType::Float32)
                }
                // Mixed integer types - promote to larger
                _ if ct1.is_integer() && ct2.is_integer() => {
                    LatticeType::Concrete(ConcreteType::Int64)
                }
                // Mixed float types - promote to Float64
                _ if ct1.is_float() && ct2.is_float() => {
                    LatticeType::Concrete(ConcreteType::Float64)
                }
                _ => LatticeType::Top,
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `rem` (remainder).
///
/// Type rules:
/// - rem(Int, Int) → Int
///
/// # Examples
/// ```text
/// rem(Int64, Int64) → Int64
/// ```
pub fn tfunc_rem(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match (&args[0], &args[1]) {
        (LatticeType::Concrete(ct1), LatticeType::Concrete(ct2)) => {
            if ct1.is_integer() && ct2.is_integer() {
                // rem returns the same type as the first argument for integers
                LatticeType::Concrete(ct1.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `mod` (modulo).
///
/// Type rules:
/// - mod(Int, Int) → Int
///
/// # Examples
/// ```text
/// mod(Int64, Int64) → Int64
/// ```
pub fn tfunc_mod(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match (&args[0], &args[1]) {
        (LatticeType::Concrete(ct1), LatticeType::Concrete(ct2)) => {
            if ct1.is_integer() && ct2.is_integer() {
                // mod returns the same type as the first argument for integers
                LatticeType::Concrete(ct1.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `floor` (round down).
///
/// Type rules:
/// - floor(Int) → Int
/// - floor(Float) → Float
///
/// # Examples
/// ```text
/// floor(Float64) → Float64
/// floor(Int64) → Int64
/// ```
pub fn tfunc_floor(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) => {
            if ct.is_numeric() {
                LatticeType::Concrete(ct.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `ceil` (round up).
///
/// Type rules:
/// - ceil(Int) → Int
/// - ceil(Float) → Float
///
/// # Examples
/// ```text
/// ceil(Float64) → Float64
/// ceil(Int64) → Int64
/// ```
pub fn tfunc_ceil(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) => {
            if ct.is_numeric() {
                LatticeType::Concrete(ct.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `round` (round to nearest).
///
/// Type rules:
/// - round(Int) → Int
/// - round(Float) → Float
///
/// # Examples
/// ```text
/// round(Float64) → Float64
/// round(Int64) → Int64
/// ```
pub fn tfunc_round(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) => {
            if ct.is_numeric() {
                LatticeType::Concrete(ct.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `<<` (left bit shift).
///
/// Type rules:
/// - <<(Int, Int) → Int
///
/// # Examples
/// ```text
/// <<(Int64, Int64) → Int64
/// ```
pub fn tfunc_lshift(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match &args[0] {
        LatticeType::Concrete(ct) => {
            if ct.is_integer() {
                LatticeType::Concrete(ct.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `>>` (right bit shift).
///
/// Type rules:
/// - >>(Int, Int) → Int
///
/// # Examples
/// ```text
/// >>(Int64, Int64) → Int64
/// ```
pub fn tfunc_rshift(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match &args[0] {
        LatticeType::Concrete(ct) => {
            if ct.is_integer() {
                LatticeType::Concrete(ct.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `&` (bitwise and).
///
/// Type rules:
/// - &(Int, Int) → Int
/// - &(Bool, Bool) → Bool
///
/// # Examples
/// ```text
/// &(Int64, Int64) → Int64
/// &(Bool, Bool) → Bool
/// ```
pub fn tfunc_bitand(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match (&args[0], &args[1]) {
        (LatticeType::Concrete(ConcreteType::Bool), LatticeType::Concrete(ConcreteType::Bool)) => {
            LatticeType::Concrete(ConcreteType::Bool)
        }
        (LatticeType::Concrete(ct1), LatticeType::Concrete(ct2)) => {
            if ct1.is_integer() && ct2.is_integer() {
                // Return the type of the first argument
                LatticeType::Concrete(ct1.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `|` (bitwise or).
///
/// Type rules:
/// - |(Int, Int) → Int
/// - |(Bool, Bool) → Bool
///
/// # Examples
/// ```text
/// |(Int64, Int64) → Int64
/// |(Bool, Bool) → Bool
/// ```
pub fn tfunc_bitor(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match (&args[0], &args[1]) {
        (LatticeType::Concrete(ConcreteType::Bool), LatticeType::Concrete(ConcreteType::Bool)) => {
            LatticeType::Concrete(ConcreteType::Bool)
        }
        (LatticeType::Concrete(ct1), LatticeType::Concrete(ct2)) => {
            if ct1.is_integer() && ct2.is_integer() {
                // Return the type of the first argument
                LatticeType::Concrete(ct1.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `xor` (bitwise exclusive or).
///
/// Type rules:
/// - xor(Int, Int) → Int
/// - xor(Bool, Bool) → Bool
///
/// # Examples
/// ```text
/// xor(Int64, Int64) → Int64
/// xor(Bool, Bool) → Bool
/// ```
pub fn tfunc_xor(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // If either operand is Bottom, the result is Bottom (unreachable code)
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    match (&args[0], &args[1]) {
        (LatticeType::Concrete(ConcreteType::Bool), LatticeType::Concrete(ConcreteType::Bool)) => {
            LatticeType::Concrete(ConcreteType::Bool)
        }
        (LatticeType::Concrete(ct1), LatticeType::Concrete(ct2)) => {
            if ct1.is_integer() && ct2.is_integer() {
                // Return the type of the first argument
                LatticeType::Concrete(ct1.clone())
            } else {
                LatticeType::Top
            }
        }
        _ => LatticeType::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_int() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_sign(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_sign_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_sign(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_div_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_div(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_rem_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_rem(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_mod_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_mod(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_floor_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_floor(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_ceil_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_ceil(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_round_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_round(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_lshift() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_lshift(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_rshift() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_rshift(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_bitand_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_bitand(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_bitand_bool() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Bool),
            LatticeType::Concrete(ConcreteType::Bool),
        ];
        let result = tfunc_bitand(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_bitor_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_bitor(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_xor_bool() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Bool),
            LatticeType::Concrete(ConcreteType::Bool),
        ];
        let result = tfunc_xor(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    // Bottom propagation tests (Issue #1717 prevention)
    // When either operand is Bottom, the result should be Bottom
    // to correctly represent unreachable code paths.

    #[test]
    fn test_div_bottom_left() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_div(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_div_bottom_right() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Bottom,
        ];
        let result = tfunc_div(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_rem_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_rem(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_mod_bottom() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Bottom,
        ];
        let result = tfunc_mod(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_lshift_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_lshift(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_rshift_bottom() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Bottom,
        ];
        let result = tfunc_rshift(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_bitand_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_bitand(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_bitor_bottom() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Bool),
            LatticeType::Bottom,
        ];
        let result = tfunc_bitor(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_xor_bottom() {
        let args = vec![LatticeType::Bottom, LatticeType::Bottom];
        let result = tfunc_xor(&args);
        assert_eq!(result, LatticeType::Bottom);
    }
}
