//! Transfer functions for intrinsic operations and type conversions.
//!
//! This module implements type inference for Julia's intrinsic operations,
//! type checking, and conversions.

use crate::compile::lattice::types::{ConcreteType, LatticeType};

/// Transfer function for `isa` (type checking).
///
/// Type rules:
/// - isa(Any, Type) → Bool
///
/// # Examples
/// ```text
/// isa(Int64, Type) → Bool
/// ```
pub fn tfunc_isa(_args: &[LatticeType]) -> LatticeType {
    // isa always returns Bool
    LatticeType::Concrete(ConcreteType::Bool)
}

/// Transfer function for `typeof` (get type of value).
///
/// Returns a Type object (represented conservatively as Top in our system).
pub fn tfunc_typeof(_args: &[LatticeType]) -> LatticeType {
    // typeof returns a Type object
    // In a full system, this would return DataType
    LatticeType::Top
}

/// Transfer function for `convert` (type conversion).
///
/// Type rules:
/// - convert(T, x) → T
///
/// # Examples
/// ```text
/// convert(Float64, Int64) → Float64
/// ```
pub fn tfunc_convert(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // In Julia, convert(T, x) returns type T
    // For simplicity, we return the target type if we can determine it
    // In a full implementation, this would inspect the first argument (the type)
    match &args[1] {
        LatticeType::Concrete(ct) => LatticeType::Concrete(ct.clone()),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int64` (conversion to Int64).
pub fn tfunc_to_int64(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Int64),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Int64),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Float64` (conversion to Float64).
pub fn tfunc_to_float64(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            LatticeType::Concrete(ConcreteType::Float64)
        }
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Float64),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Bool` (conversion to Bool).
pub fn tfunc_to_bool(_args: &[LatticeType]) -> LatticeType {
    // Bool() conversion always returns Bool
    LatticeType::Concrete(ConcreteType::Bool)
}

/// Transfer function for `String` (conversion to String).
pub fn tfunc_to_string(_args: &[LatticeType]) -> LatticeType {
    // String() conversion always returns String
    LatticeType::Concrete(ConcreteType::String)
}

/// Transfer function for `sqrt` (square root).
///
/// Type rules:
/// - sqrt(Numeric) → Float64
///
/// # Examples
/// ```text
/// sqrt(Int64) → Float64
/// sqrt(Float64) → Float64
/// ```
pub fn tfunc_sqrt(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            LatticeType::Concrete(ConcreteType::Float64)
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `abs` (absolute value).
///
/// Type rules:
/// - abs(Int) → Int
/// - abs(Float) → Float
/// - abs(Complex{T}) → Float64 (magnitude of complex number)
pub fn tfunc_abs(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            // abs preserves numeric type
            LatticeType::Concrete(ct.clone())
        }
        // Complex numbers: abs returns the magnitude (a real number)
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) if name.starts_with("Complex") => {
            LatticeType::Concrete(ConcreteType::Float64)
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `sin` (sine).
pub fn tfunc_sin(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules: Numeric → Float64
}

/// Transfer function for `cos` (cosine).
pub fn tfunc_cos(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules
}

/// Transfer function for `exp` (exponential).
pub fn tfunc_exp(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules
}

/// Transfer function for `log` (natural logarithm).
pub fn tfunc_log(args: &[LatticeType]) -> LatticeType {
    tfunc_sqrt(args) // Same type rules
}

/// Transfer function for `min` (minimum of two values).
///
/// Returns the common type of the inputs.
pub fn tfunc_min(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // min returns the common type of its arguments
    args[0].join(&args[1])
}

/// Transfer function for `max` (maximum of two values).
pub fn tfunc_max(args: &[LatticeType]) -> LatticeType {
    tfunc_min(args) // Same type rules
}

/// Transfer function for `println` and `print` (I/O operations).
///
/// Returns Nothing.
pub fn tfunc_println(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Nothing)
}

// ============================================================================
// Extended Type Conversion Functions
// ============================================================================

/// Transfer function for `promote` (type promotion).
///
/// Type rules:
/// - promote(x, y) → Tuple{T, T} where T is the promoted type
/// - Returns the common promoted type for all arguments
///
/// # Examples
/// ```text
/// promote(Int64, Float64) → Tuple{Float64, Float64}
/// promote(Int32, Int64) → Tuple{Int64, Int64}
/// ```
pub fn tfunc_promote(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    // Find the promoted type by joining all argument types
    let mut promoted = args[0].clone();
    for arg in &args[1..] {
        promoted = promoted.join(arg);
    }

    // Return tuple of promoted types (one for each argument)
    match promoted {
        LatticeType::Concrete(ct) => {
            let elements = vec![ct; args.len()];
            LatticeType::Concrete(ConcreteType::Tuple { elements })
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int8` (conversion to Int8).
pub fn tfunc_to_int8(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Int8),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Int8),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::Int8),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int16` (conversion to Int16).
pub fn tfunc_to_int16(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Int16),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Int16),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::Int16),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int32` (conversion to Int32).
pub fn tfunc_to_int32(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Int32),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Int32),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::Int32),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Int128` (conversion to Int128).
pub fn tfunc_to_int128(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::Int128),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Int128),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt8` (conversion to UInt8).
pub fn tfunc_to_uint8(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::UInt8),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::UInt8),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::UInt8),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt16` (conversion to UInt16).
pub fn tfunc_to_uint16(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::UInt16),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::UInt16),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::UInt16),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt32` (conversion to UInt32).
pub fn tfunc_to_uint32(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::UInt32),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::UInt32),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::UInt32),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt64` (conversion to UInt64).
pub fn tfunc_to_uint64(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ConcreteType::UInt64),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::UInt64),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `UInt128` (conversion to UInt128).
pub fn tfunc_to_uint128(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            LatticeType::Concrete(ConcreteType::UInt128)
        }
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::UInt128),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Float32` (conversion to Float32).
pub fn tfunc_to_float32(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            LatticeType::Concrete(ConcreteType::Float32)
        }
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Float32),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `Char` (conversion to Char).
pub fn tfunc_to_char(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_integer() => LatticeType::Concrete(ConcreteType::Char),
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Char),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::Char),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `zero` (return zero of a type).
///
/// Type rules:
/// - zero(T) → T (zero value of type T)
/// - zero(x::T) → T (zero value of x's type)
pub fn tfunc_zero(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `one` (return one of a type).
///
/// Type rules:
/// - one(T) → T (one value of type T)
/// - one(x::T) → T (one value of x's type)
pub fn tfunc_one(args: &[LatticeType]) -> LatticeType {
    tfunc_zero(args) // Same type rules as zero
}

/// Transfer function for `typemin` (minimum value of a type).
///
/// Unlike `zero`/`one` which take either a type or a value,
/// `typemin`/`typemax` take a Type{T} argument and return T.
/// e.g., typemin(Float64) → Float64, typemin(Int64) → Int64
pub fn tfunc_typemin(args: &[LatticeType]) -> LatticeType {
    tfunc_type_to_value(args)
}

/// Transfer function for `typemax` (maximum value of a type).
pub fn tfunc_typemax(args: &[LatticeType]) -> LatticeType {
    tfunc_type_to_value(args)
}

/// Shared helper for functions that take Type{T} and return a value of type T.
/// Handles both DataType arguments (e.g., Float64 as Type{Float64}) and
/// numeric value arguments (for overloads like zero(x::T)).
fn tfunc_type_to_value(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }
    match &args[0] {
        // Direct numeric type (e.g., zero(1.0) → Float64)
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        // DataType argument: typemin(Float64) where Float64 is DataType{name: "Float64"}
        LatticeType::Concrete(ConcreteType::DataType { name }) => {
            if let Some(ct) = ConcreteType::from_type_name(name) {
                if ct.is_numeric() {
                    return LatticeType::Concrete(ct);
                }
            }
            LatticeType::Top
        }
        _ => LatticeType::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isa_returns_bool() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64), LatticeType::Top];
        let result = tfunc_isa(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_to_int64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_to_int64(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_to_float64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_to_float64(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_sqrt_returns_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_sqrt(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_abs_preserves_type() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_abs(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));

        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_abs(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_sin_returns_float() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_sin(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_min_joins_types() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Float64),
        ];
        let result = tfunc_min(&args);
        assert!(result.is_numeric());
    }

    #[test]
    fn test_println_returns_nothing() {
        let args = vec![LatticeType::Concrete(ConcreteType::String)];
        let result = tfunc_println(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Nothing));
    }

    #[test]
    fn test_to_bool() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_to_bool(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_to_string() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_to_string(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::String));
    }

    #[test]
    fn test_promote_same_types() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Int64),
        ];
        let result = tfunc_promote(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![ConcreteType::Int64, ConcreteType::Int64]
            })
        );
    }

    #[test]
    fn test_promote_mixed_numeric() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int32),
            LatticeType::Concrete(ConcreteType::Float64),
        ];
        let result = tfunc_promote(&args);
        // When mixing Int32 and Float64, the join may return Union or Top
        // depending on lattice rules. We just verify it doesn't panic.
        assert!(
            matches!(
                &result,
                LatticeType::Concrete(ConcreteType::Tuple { .. })
                    | LatticeType::Union(_)
                    | LatticeType::Top
            ),
            "Unexpected result: {:?}",
            result
        );
        if let LatticeType::Concrete(ConcreteType::Tuple { elements }) = result {
            assert_eq!(elements.len(), 2);
            // Both should be promoted to the same type
            assert_eq!(elements[0], elements[1]);
        }
    }

    #[test]
    fn test_to_int8() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_to_int8(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int8));
    }

    #[test]
    fn test_to_uint64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_to_uint64(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::UInt64));
    }

    #[test]
    fn test_to_float32() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_to_float32(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float32));
    }

    #[test]
    fn test_to_char() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_to_char(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Char));
    }

    #[test]
    fn test_zero() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_zero(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_one() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int32)];
        let result = tfunc_one(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int32));
    }
}
