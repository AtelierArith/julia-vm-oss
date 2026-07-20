//! Transfer functions for mathematical intrinsic operations.
//!
//! This module implements type inference for Julia's mathematical intrinsics,
//! including sign, div, rem, mod, floor, ceil, and round operations.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::{CorePrimitive, CoreType};

fn lattice_concrete_type(ty: &LatticeType) -> Option<ConcreteType> {
    match ty {
        LatticeType::Const(value) => Some(value.to_concrete_type()),
        LatticeType::Concrete(concrete) => Some(concrete.clone()),
        _ => None,
    }
}

/// Transfer function for `sign` (sign of a number).
///
/// Type rules:
/// - sign(T) → T for concrete numeric `T`
///
/// # Examples
/// ```text
/// sign(Int64) → Int64
/// sign(Float32) → Float32
/// ```
pub fn tfunc_sign(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `rand` / `randn` (Issue #5922).
///
/// Type rules:
/// - `rand()` / `randn()` → `Float64` (a single uniform/normal sample)
/// - any argument form → `Top`: the result shape depends on the call form
///   (`rand(n)` → `Vector{Float64}`, `rand(T)` → `T`, `rand(itr)` → eltype,
///   ...), so the registry stays conservative and the expression-inference
///   adapter pins the legacy unparameterized `Array` fallback.
pub fn tfunc_rand(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float64,
        )))
    } else {
        LatticeType::Top
    }
}

/// Transfer function for `signbit` (sign-bit predicate).
///
/// Type rules:
/// - signbit(T) -> Bool for concrete numeric `T`
pub fn tfunc_signbit(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `clamp(x, lo, hi)`.
///
/// This stays deliberately conservative: when all three operands are the same
/// concrete numeric type, Julia's `ifelse`/comparison path preserves that type.
/// Mixed numeric bounds are left to body inference or method snapshots.
pub fn tfunc_clamp(args: &[LatticeType]) -> LatticeType {
    if args.len() != 3 {
        return LatticeType::Top;
    }

    let Some(x) = lattice_concrete_type(&args[0]) else {
        return LatticeType::Top;
    };
    let Some(lo) = lattice_concrete_type(&args[1]) else {
        return LatticeType::Top;
    };
    let Some(hi) = lattice_concrete_type(&args[2]) else {
        return LatticeType::Top;
    };

    if x == lo && x == hi && x.is_numeric() {
        LatticeType::Concrete(x)
    } else {
        LatticeType::Top
    }
}

/// Transfer function for `binomial(n, k)`.
///
/// The supported Base method used by the VM returns native `Int64` for
/// integer arguments, matching upstream for the representative `Int` path.
pub fn tfunc_binomial(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    let Some(n) = lattice_concrete_type(&args[0]) else {
        return LatticeType::Top;
    };
    let Some(k) = lattice_concrete_type(&args[1]) else {
        return LatticeType::Top;
    };

    if n.is_integer() && k.is_integer() {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )))
    } else {
        LatticeType::Top
    }
}

/// Transfer function for `copysign(x, y)`.
pub fn tfunc_copysign(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    let Some(x) = lattice_concrete_type(&args[0]) else {
        return LatticeType::Top;
    };
    let Some(y) = lattice_concrete_type(&args[1]) else {
        return LatticeType::Top;
    };

    if x.is_numeric() && y.is_numeric() {
        LatticeType::Concrete(x)
    } else {
        LatticeType::Top
    }
}

/// Transfer function for `ndigits(n)`.
pub fn tfunc_ndigits(args: &[LatticeType]) -> LatticeType {
    if args.len() == 1 && args[0].is_integer() {
        LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int64,
        )))
    } else {
        LatticeType::Top
    }
}

/// Transfer function for value-based `widen(x)`.
pub fn tfunc_widen(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    let Some(arg_type) = lattice_concrete_type(&args[0]) else {
        return LatticeType::Top;
    };

    let widened = match arg_type {
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int8)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int16)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int32)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int128)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt8)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt16)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt32)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt64)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::UInt128)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float16)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float32)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat))
        }
        ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat)) => {
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigFloat))
        }
        _ => return LatticeType::Top,
    };

    LatticeType::Concrete(widened)
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
                // Same concrete numeric type preserves width, matching Julia's
                // div(::T, ::T) methods across narrow/128-bit ints and floats.
                _ if ct1 == ct2 && ct1.is_numeric() => LatticeType::Concrete(ct1.clone()),
                (ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt)), _)
                | (_, ConcreteType::Core(CoreType::Primitive(CorePrimitive::BigInt))) => {
                    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                        CorePrimitive::BigInt,
                    )))
                }
                // Mixed integer types - promote to larger
                _ if ct1.is_integer() && ct2.is_integer() => LatticeType::Concrete(
                    ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ),
                // Mixed float types - promote to Float64
                _ if ct1.is_float() && ct2.is_float() => LatticeType::Concrete(ConcreteType::Core(
                    CoreType::Primitive(CorePrimitive::Float64),
                )),
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
            if ct1 == ct2 && ct1.is_numeric() {
                return LatticeType::Concrete(ct1.clone());
            }
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
            if ct1 == ct2 && ct1.is_numeric() {
                return LatticeType::Concrete(ct1.clone());
            }
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

/// Transfer function for `trunc` (truncate toward zero).
///
/// Type rules:
/// - trunc(Int) -> Int
/// - trunc(Float) -> Float
///
/// # Examples
/// ```text
/// trunc(Float64) -> Float64
/// trunc(Int64) -> Int64
/// ```
pub fn tfunc_trunc(args: &[LatticeType]) -> LatticeType {
    tfunc_round(args)
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
        (
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        ) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
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
        (
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        ) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
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
        (
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        ) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
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

    fn concrete(ty: ConcreteType) -> LatticeType {
        LatticeType::Concrete(ty)
    }

    #[test]
    fn numeric_snapshot_precision_helpers_issue_6547() {
        assert_eq!(
            tfunc_clamp(&[
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
            ]),
            concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
        assert_eq!(
            tfunc_clamp(&[
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
            ]),
            concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            tfunc_binomial(&[
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                ))),
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64
                )))
            ]),
            concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            tfunc_copysign(&[
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
                concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64
                ))),
            ]),
            concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
        assert_eq!(
            tfunc_ndigits(&[concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))]),
            concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            tfunc_widen(&[concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int32
            )))]),
            concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_sign_int() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))];
        let result = tfunc_sign(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_sign_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_sign(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_sign_preserves_narrow_unsigned_and_float_widths() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::UInt8),
        ))];
        let result = tfunc_sign(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8
            )))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int128),
        ))];
        let result = tfunc_sign(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128
            )))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))];
        let result = tfunc_sign(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_signbit_returns_bool_for_numeric_types() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int8),
        ))];
        let result = tfunc_signbit(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))];
        let result = tfunc_signbit(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );

        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Bool),
        ))];
        let result = tfunc_signbit(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_div_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_div_same_type_preserves_width() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::UInt8
            )))
        );

        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int128
            )))
        );
    }

    #[test]
    fn test_rem_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_rem(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_rem_float_preserves_width() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32,
            ))),
        ];
        let result = tfunc_rem(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_mod_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_mod(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_mod_float_preserves_width() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32,
            ))),
        ];
        let result = tfunc_mod(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_floor_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_floor(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_ceil_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_ceil(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_round_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float64),
        ))];
        let result = tfunc_round(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
    }

    #[test]
    fn test_trunc_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Float32),
        ))];
        let result = tfunc_trunc(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float32
            )))
        );
    }

    #[test]
    fn test_lshift() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_lshift(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_rshift() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_rshift(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_bitand_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_bitand(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_bitand_bool() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        ];
        let result = tfunc_bitand(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    #[test]
    fn test_bitor_int() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_bitor(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
    }

    #[test]
    fn test_xor_bool() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        ];
        let result = tfunc_xor(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool)))
        );
    }

    // Bottom propagation tests (Issue #1717 prevention)
    // When either operand is Bottom, the result should be Bottom
    // to correctly represent unreachable code paths.

    #[test]
    fn test_div_bottom_left() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_div(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_div_bottom_right() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Bottom,
        ];
        let result = tfunc_div(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_rem_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_rem(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_mod_bottom() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Bottom,
        ];
        let result = tfunc_mod(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_lshift_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_lshift(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_rshift_bottom() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
            LatticeType::Bottom,
        ];
        let result = tfunc_rshift(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_bitand_bottom() {
        let args = vec![
            LatticeType::Bottom,
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        ];
        let result = tfunc_bitand(&args);
        assert_eq!(result, LatticeType::Bottom);
    }

    #[test]
    fn test_bitor_bottom() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
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
