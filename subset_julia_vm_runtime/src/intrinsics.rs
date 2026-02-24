//! Intrinsic functions for AoT runtime
//!
//! This module provides built-in mathematical and utility functions.

use crate::error::{RuntimeError, RuntimeResult};

// ========== Mathematical functions ==========

/// Square root
#[inline]
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// Square root with domain check
pub fn sqrt_checked(x: f64) -> RuntimeResult<f64> {
    if x < 0.0 {
        Err(RuntimeError::domain_error(
            "sqrt will only return a complex result if called with a complex argument. Try sqrt(Complex(x, 0))."
        ))
    } else {
        Ok(x.sqrt())
    }
}

/// Sine
#[inline]
pub fn sin(x: f64) -> f64 {
    x.sin()
}

/// Cosine
#[inline]
pub fn cos(x: f64) -> f64 {
    x.cos()
}

/// Tangent
#[inline]
pub fn tan(x: f64) -> f64 {
    x.tan()
}

/// Arcsine
#[inline]
pub fn asin(x: f64) -> f64 {
    x.asin()
}

/// Arccosine
#[inline]
pub fn acos(x: f64) -> f64 {
    x.acos()
}

/// Arctangent
#[inline]
pub fn atan(x: f64) -> f64 {
    x.atan()
}

/// Arctangent of y/x
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// Hyperbolic sine
#[inline]
pub fn sinh(x: f64) -> f64 {
    x.sinh()
}

/// Hyperbolic cosine
#[inline]
pub fn cosh(x: f64) -> f64 {
    x.cosh()
}

/// Hyperbolic tangent
#[inline]
pub fn tanh(x: f64) -> f64 {
    x.tanh()
}

/// Exponential (e^x)
#[inline]
pub fn exp(x: f64) -> f64 {
    x.exp()
}

/// Exponential (2^x)
#[inline]
pub fn exp2(x: f64) -> f64 {
    x.exp2()
}

/// Exponential (10^x)
#[inline]
pub fn exp10(x: f64) -> f64 {
    (10.0_f64).powf(x)
}

/// Natural logarithm
#[inline]
pub fn log(x: f64) -> f64 {
    x.ln()
}

/// Natural logarithm with domain check
pub fn log_checked(x: f64) -> RuntimeResult<f64> {
    if x <= 0.0 {
        Err(RuntimeError::domain_error(
            "log will only return a complex result if called with a complex argument",
        ))
    } else {
        Ok(x.ln())
    }
}

/// Base-2 logarithm
#[inline]
pub fn log2(x: f64) -> f64 {
    x.log2()
}

/// Base-10 logarithm
#[inline]
pub fn log10(x: f64) -> f64 {
    x.log10()
}

/// Logarithm with specified base
#[inline]
pub fn log_base(b: f64, x: f64) -> f64 {
    x.log(b)
}

// ========== Absolute value ==========

/// Absolute value (i64)
#[inline]
pub fn abs_i64(x: i64) -> i64 {
    x.abs()
}

/// Absolute value (f64)
#[inline]
pub fn abs_f64(x: f64) -> f64 {
    x.abs()
}

/// Squared absolute value (f64)
#[inline]
pub fn abs2_f64(x: f64) -> f64 {
    x * x
}

/// Sign function (i64)
#[inline]
pub fn sign_i64(x: i64) -> i64 {
    x.signum()
}

/// Sign function (f64)
#[inline]
pub fn sign_f64(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

// ========== Rounding functions ==========

/// Floor
#[inline]
pub fn floor(x: f64) -> f64 {
    x.floor()
}

/// Ceiling
#[inline]
pub fn ceil(x: f64) -> f64 {
    x.ceil()
}

/// Round to nearest integer
#[inline]
pub fn round(x: f64) -> f64 {
    x.round()
}

/// Truncate towards zero
#[inline]
pub fn trunc(x: f64) -> f64 {
    x.trunc()
}

// ========== Min/Max ==========

/// Minimum of two values (i64)
#[inline]
pub fn min_i64(a: i64, b: i64) -> i64 {
    a.min(b)
}

/// Maximum of two values (i64)
#[inline]
pub fn max_i64(a: i64, b: i64) -> i64 {
    a.max(b)
}

/// Minimum of two values (f64)
#[inline]
pub fn min_f64(a: f64, b: f64) -> f64 {
    a.min(b)
}

/// Maximum of two values (f64)
#[inline]
pub fn max_f64(a: f64, b: f64) -> f64 {
    a.max(b)
}

/// Clamp value to range (i64)
#[inline]
pub fn clamp_i64(x: i64, lo: i64, hi: i64) -> i64 {
    x.clamp(lo, hi)
}

/// Clamp value to range (f64)
#[inline]
pub fn clamp_f64(x: f64, lo: f64, hi: f64) -> f64 {
    x.clamp(lo, hi)
}

// ========== Type predicates ==========

/// Check if value is finite
#[inline]
pub fn isfinite(x: f64) -> bool {
    x.is_finite()
}

/// Check if value is NaN
#[inline]
pub fn isnan(x: f64) -> bool {
    x.is_nan()
}

/// Check if value is infinite
#[inline]
pub fn isinf(x: f64) -> bool {
    x.is_infinite()
}

/// Check if value is zero (i64)
#[inline]
pub fn iszero_i64(x: i64) -> bool {
    x == 0
}

/// Check if value is zero (f64)
#[inline]
pub fn iszero_f64(x: f64) -> bool {
    x == 0.0
}

/// Check if value is one (i64)
#[inline]
pub fn isone_i64(x: i64) -> bool {
    x == 1
}

/// Check if value is one (f64)
#[inline]
pub fn isone_f64(x: f64) -> bool {
    x == 1.0
}

/// Check if value is even
#[inline]
pub fn iseven(x: i64) -> bool {
    x % 2 == 0
}

/// Check if value is odd
#[inline]
pub fn isodd(x: i64) -> bool {
    x % 2 != 0
}

// ========== Type conversion ==========

/// Convert i64 to f64
#[inline]
pub fn i64_to_f64(x: i64) -> f64 {
    x as f64
}

/// Convert f64 to i64 (truncating)
#[inline]
pub fn f64_to_i64(x: f64) -> i64 {
    x as i64
}

/// Convert f64 to i64 (checked)
pub fn f64_to_i64_checked(x: f64) -> RuntimeResult<i64> {
    if x.fract() != 0.0 {
        Err(RuntimeError::inexact_error(format!(
            "cannot convert {} to Int64",
            x
        )))
    } else {
        Ok(x as i64)
    }
}

// ========== I/O functions ==========

/// Print a value with newline
pub fn println_value<T: std::fmt::Display>(x: T) {
    println!("{}", x);
}

/// Print a value without newline
pub fn print_value<T: std::fmt::Display>(x: T) {
    print!("{}", x);
}

/// Print multiple values
pub fn println_values(values: &[&dyn std::fmt::Display]) {
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", v);
    }
    println!();
}

// ========== Constants ==========

/// Mathematical constant π
pub const PI: f64 = std::f64::consts::PI;

/// Mathematical constant e
pub const E: f64 = std::f64::consts::E;

/// Positive infinity
pub const INF: f64 = f64::INFINITY;

/// Not a Number
pub const NAN: f64 = f64::NAN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_functions() {
        assert_eq!(sqrt(4.0), 2.0);
        assert!((sin(PI / 2.0) - 1.0).abs() < 1e-10);
        assert!((cos(0.0) - 1.0).abs() < 1e-10);
        assert!((exp(0.0) - 1.0).abs() < 1e-10);
        assert!((log(E) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_abs_sign() {
        assert_eq!(abs_i64(-5), 5);
        assert_eq!(abs_f64(-3.125), 3.125);
        assert_eq!(sign_i64(-10), -1);
        assert_eq!(sign_f64(3.125), 1.0);
    }

    #[test]
    fn test_rounding() {
        assert_eq!(floor(3.7), 3.0);
        assert_eq!(ceil(3.2), 4.0);
        assert_eq!(round(3.5), 4.0);
        assert_eq!(trunc(-3.7), -3.0);
    }

    #[test]
    fn test_predicates() {
        assert!(isfinite(1.0));
        assert!(!isfinite(INF));
        assert!(isnan(NAN));
        assert!(isinf(INF));
        assert!(iszero_i64(0));
        assert!(iseven(4));
        assert!(isodd(5));
    }
}
