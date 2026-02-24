//! Transfer functions for complex number operations.
//!
//! This module implements type inference for Julia's complex number accessor functions:
//! `real`, `imag`, `conj`, `abs2`, `angle`, and `reim`.
//!
//! # Type Rules
//!
//! For `Complex{T}` where `T <: Real`:
//! - `real(z::Complex{T}) → T`
//! - `imag(z::Complex{T}) → T`
//! - `conj(z::Complex{T}) → Complex{T}`
//! - `abs2(z::Complex{T}) → T` (for T = Float64, Int64, etc.)
//! - `angle(z::Complex{T}) → Float64`
//! - `reim(z::Complex{T}) → Tuple{T, T}`
//!
//! For real types:
//! - `real(x::T) → T` (identity)
//! - `imag(x::T) → T` (returns zero of same type)
//! - `conj(x::T) → T` (identity for reals)
//! - `reim(x::T) → Tuple{T, T}`

use crate::compile::lattice::types::{ConcreteType, LatticeType};

/// Extract the element type from a Complex{T} struct name.
///
/// Returns the element type (e.g., Float64 from Complex{Float64}).
fn extract_complex_element_type(struct_name: &str) -> Option<ConcreteType> {
    // Handle Complex{T} patterns
    if struct_name.starts_with("Complex{") && struct_name.ends_with('}') {
        let inner = &struct_name[8..struct_name.len() - 1];
        return match inner {
            "Float64" => Some(ConcreteType::Float64),
            "Float32" => Some(ConcreteType::Float32),
            "Int64" => Some(ConcreteType::Int64),
            "Int32" => Some(ConcreteType::Int32),
            "Int16" => Some(ConcreteType::Int16),
            "Int8" => Some(ConcreteType::Int8),
            "Bool" => Some(ConcreteType::Bool),
            "UInt64" => Some(ConcreteType::UInt64),
            "UInt32" => Some(ConcreteType::UInt32),
            "UInt16" => Some(ConcreteType::UInt16),
            "UInt8" => Some(ConcreteType::UInt8),
            _ => None,
        };
    }

    // Handle type aliases
    match struct_name {
        "ComplexF64" => Some(ConcreteType::Float64),
        "ComplexF32" => Some(ConcreteType::Float32),
        _ => None,
    }
}

/// Transfer function for `real` (extract real part of complex number).
///
/// Type rules:
/// - `real(z::Complex{T}) → T`
/// - `real(x::T) → T` for real types
///
/// # Examples
/// ```text
/// real(Complex{Float64}) → Float64
/// real(Complex{Int64}) → Int64
/// real(Float64) → Float64
/// ```
pub fn tfunc_real(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Complex numbers: extract element type
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            if let Some(elem_type) = extract_complex_element_type(name) {
                LatticeType::Concrete(elem_type)
            } else {
                // Not a known Complex type
                LatticeType::Top
            }
        }
        // Real numbers: identity function
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `imag` (extract imaginary part of complex number).
///
/// Type rules:
/// - `imag(z::Complex{T}) → T`
/// - `imag(x::T) → T` for real types (returns zero of same type)
///
/// # Examples
/// ```text
/// imag(Complex{Float64}) → Float64
/// imag(Float64) → Float64
/// ```
pub fn tfunc_imag(args: &[LatticeType]) -> LatticeType {
    // Same type signature as real()
    tfunc_real(args)
}

/// Transfer function for `conj` (complex conjugate).
///
/// Type rules:
/// - `conj(z::Complex{T}) → Complex{T}`
/// - `conj(x::T) → T` for real types (identity)
///
/// # Examples
/// ```text
/// conj(Complex{Float64}) → Complex{Float64}
/// conj(Float64) → Float64
/// ```
pub fn tfunc_conj(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Complex numbers: return same type
        LatticeType::Concrete(ct @ ConcreteType::Struct { name, .. })
            if name.starts_with("Complex") =>
        {
            LatticeType::Concrete(ct.clone())
        }
        // Real numbers: identity function
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `abs2` (squared magnitude).
///
/// Type rules:
/// - `abs2(z::Complex{T}) → T` (re^2 + im^2 has type T for numeric T)
/// - `abs2(x::T) → T` (x^2 has same type)
///
/// Note: For complex numbers, the return type is the element type,
/// not Float64, to preserve integer precision when T is Int64.
pub fn tfunc_abs2(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Complex numbers: extract element type
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            if let Some(elem_type) = extract_complex_element_type(name) {
                LatticeType::Concrete(elem_type)
            } else {
                LatticeType::Top
            }
        }
        // Real numbers: same type
        LatticeType::Concrete(ct) if ct.is_numeric() => LatticeType::Concrete(ct.clone()),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `angle` (argument/phase of complex number).
///
/// Type rules:
/// - `angle(z::Complex{T}) → Float64` (phase is always a floating-point angle)
///
/// # Examples
/// ```text
/// angle(Complex{Float64}) → Float64
/// angle(Complex{Int64}) → Float64
/// ```
pub fn tfunc_angle(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Complex numbers: angle is always Float64
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) if name.starts_with("Complex") => {
            LatticeType::Concrete(ConcreteType::Float64)
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `reim` (decompose into (real, imag) tuple).
///
/// Type rules:
/// - `reim(z::Complex{T}) → Tuple{T, T}`
/// - `reim(x::T) → Tuple{T, T}` for real types
///
/// # Examples
/// ```text
/// reim(Complex{Float64}) → Tuple{Float64, Float64}
/// reim(Int64) → Tuple{Int64, Int64}
/// ```
pub fn tfunc_reim(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        // Complex numbers: return tuple of element types
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            if let Some(elem_type) = extract_complex_element_type(name) {
                LatticeType::Concrete(ConcreteType::Tuple {
                    elements: vec![elem_type.clone(), elem_type],
                })
            } else {
                LatticeType::Top
            }
        }
        // Real numbers: return tuple of same type
        LatticeType::Concrete(ct) if ct.is_numeric() => {
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![ct.clone(), ct.clone()],
            })
        }
        _ => LatticeType::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complex_f64() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Struct {
            name: "Complex{Float64}".to_string(),
            type_id: 0,
        })
    }

    fn complex_int64() -> LatticeType {
        LatticeType::Concrete(ConcreteType::Struct {
            name: "Complex{Int64}".to_string(),
            type_id: 0,
        })
    }

    #[test]
    fn test_real_complex_f64() {
        let args = vec![complex_f64()];
        let result = tfunc_real(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_real_complex_int64() {
        let args = vec![complex_int64()];
        let result = tfunc_real(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_real_float64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_real(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_real_int64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_real(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_imag_complex_f64() {
        let args = vec![complex_f64()];
        let result = tfunc_imag(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_conj_complex_f64() {
        let args = vec![complex_f64()];
        let result = tfunc_conj(&args);
        // conj returns the same Complex type
        assert!(
            matches!(result, LatticeType::Concrete(ConcreteType::Struct { name, .. }) if name == "Complex{Float64}")
        );
    }

    #[test]
    fn test_conj_float64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Float64)];
        let result = tfunc_conj(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_abs2_complex_f64() {
        let args = vec![complex_f64()];
        let result = tfunc_abs2(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_angle_complex_f64() {
        let args = vec![complex_f64()];
        let result = tfunc_angle(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Float64));
    }

    #[test]
    fn test_reim_complex_f64() {
        let args = vec![complex_f64()];
        let result = tfunc_reim(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![ConcreteType::Float64, ConcreteType::Float64]
            })
        );
    }

    #[test]
    fn test_reim_int64() {
        let args = vec![LatticeType::Concrete(ConcreteType::Int64)];
        let result = tfunc_reim(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![ConcreteType::Int64, ConcreteType::Int64]
            })
        );
    }

    #[test]
    fn test_real_wrong_arity() {
        let args = vec![];
        let result = tfunc_real(&args);
        assert_eq!(result, LatticeType::Top);

        let args = vec![complex_f64(), complex_f64()];
        let result = tfunc_real(&args);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_extract_complex_element_type() {
        assert_eq!(
            extract_complex_element_type("Complex{Float64}"),
            Some(ConcreteType::Float64)
        );
        assert_eq!(
            extract_complex_element_type("Complex{Int64}"),
            Some(ConcreteType::Int64)
        );
        assert_eq!(
            extract_complex_element_type("Complex{Float32}"),
            Some(ConcreteType::Float32)
        );
        assert_eq!(
            extract_complex_element_type("ComplexF64"),
            Some(ConcreteType::Float64)
        );
        assert_eq!(
            extract_complex_element_type("ComplexF32"),
            Some(ConcreteType::Float32)
        );
        assert_eq!(extract_complex_element_type("NotComplex"), None);
        assert_eq!(extract_complex_element_type("Complex{Unknown}"), None);
    }
}
