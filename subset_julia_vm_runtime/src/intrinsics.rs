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

/// Round to nearest integer, ties to even (Julia's default `RoundNearest`).
#[inline]
pub fn round(x: f64) -> f64 {
    x.round_ties_even()
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

// ========== Float display (Julia-faithful) ==========

/// Format an `f64` the way Julia's `print`/`println`/`string` do (Issue #7256).
///
/// This mirrors the VM-side formatter (`vm::formatting::numeric::format_float_julia`)
/// so AoT-compiled output matches both upstream Julia and the interpreter:
///   * whole numbers below `1e6` keep a `.0` suffix (`3.0`, `100000.0`);
///   * magnitudes outside `[1e-4, 1e6)` switch to scientific notation
///     (`1.0e30`, `1.5e20`, `1.0e-7`) instead of Rust's default decimal
///     expansion (`1000000000000000000000000000000`);
///   * `Inf`/`-Inf`/`NaN` use Julia's spelling, and `-0.0` keeps its sign.
///
/// Julia uses `e` notation (`1.0e30`), never `1e+30`, and an integer mantissa
/// always carries a `.0` (`1.0e30`, not `1e30`).
pub fn format_float64_julia(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Inf".to_string()
        } else {
            "Inf".to_string()
        };
    }

    // Whole numbers below 1e6 use fixed-point form with a `.0` suffix. Larger
    // whole numbers fall through to the scientific arm below (Julia switches to
    // scientific at 1e6, so `100000.0` is fixed but `1.0e6` is scientific).
    if value.fract() == 0.0 && value.abs() < 1e6 {
        // `-0.0 as i64` is `0`, losing the sign; IEEE 754 and Julia both keep
        // `-0.0`, so guard before the cast.
        if value.is_sign_negative() && value == 0.0 {
            return "-0.0".to_string();
        }
        return format!("{}.0", value as i64);
    }

    // For very small / very large magnitudes, Julia uses scientific notation
    // rather than the multi-hundred-digit fixed-point form Rust's default
    // `Display` would produce. Thresholds match Julia's shortest-roundtrip
    // cutoff: `|x| < 1e-4` (small) or `|x| >= 1e6` (large).
    let mag = value.abs();
    if mag != 0.0 && !(1e-4..1e6).contains(&mag) {
        let raw = format!("{:e}", value);
        let mut parts = raw.splitn(2, 'e');
        let mantissa = parts.next().unwrap_or("");
        let exponent = parts.next().unwrap_or("");
        return if mantissa.contains('.') {
            format!("{}e{}", mantissa, exponent)
        } else {
            // Integer mantissa: Julia still shows the decimal point (`1.0e30`).
            format!("{}.0e{}", mantissa, exponent)
        };
    }

    // Normal range: Rust's default `Display` already matches Julia.
    value.to_string()
}

/// Format an `f32` the way Julia does, delegating to the `f64` path on the
/// widened value (matches the VM's `format_float32_julia` fixed/scientific
/// thresholds; Issue #7256).
#[inline]
pub fn format_float32_julia(value: f32) -> String {
    format_float64_julia(value as f64)
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
        // Julia's default RoundNearest is round-half-to-even (banker's), not
        // half-away-from-zero: round(2.5)==2.0, round(0.5)==0.0, round(4.5)==4.0.
        assert_eq!(round(2.5), 2.0);
        assert_eq!(round(0.5), 0.0);
        assert_eq!(round(4.5), 4.0);
        assert_eq!(round(-2.5), -2.0);
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

    /// Issue #7256: AoT float display must match upstream Julia (1.12.6) and the
    /// VM, switching to scientific notation for large/small magnitudes rather
    /// than emitting Rust's default decimal expansion.
    #[test]
    fn test_format_float64_julia_matches_upstream() {
        // The expected strings were captured from `julia -e 'println(v)'` on
        // Julia 1.12.6 (the parity gold) and confirmed identical to the VM's
        // `format_float_julia`.
        let cases: &[(f64, &str)] = &[
            // Large whole-value floats: scientific, NOT decimal expansion.
            (1e30, "1.0e30"),
            (1.5e20, "1.5e20"),
            (-1.5e20, "-1.5e20"),
            (1.0e6, "1.0e6"),
            (1.0e7, "1.0e7"),
            (1.0e15, "1.0e15"),
            (1.0e16, "1.0e16"),
            (1.0e21, "1.0e21"),
            (6.022e23, "6.022e23"),
            (1.0e300, "1.0e300"),
            // Small magnitudes: scientific once |x| < 1e-4.
            (1.0e-7, "1.0e-7"),
            (1.0e-5, "1.0e-5"),
            (1.0e-300, "1.0e-300"),
            // Fixed-point range: decimal form, with `.0` on whole numbers.
            (0.0001, "0.0001"),
            (0.001, "0.001"),
            (0.1, "0.1"),
            (0.25, "0.25"),
            (1.0, "1.0"),
            (1.5, "1.5"),
            (100.0, "100.0"),
            (1000.0, "1000.0"),
            (100000.0, "100000.0"),
            (123456.0, "123456.0"),
            // Just past the 1e6 fixed/scientific switch.
            (1234567.0, "1.234567e6"),
            (12345678.0, "1.2345678e7"),
            // Signed zero and plain zero.
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            // Non-finite values use Julia's spelling.
            (f64::INFINITY, "Inf"),
            (f64::NEG_INFINITY, "-Inf"),
            (f64::NAN, "NaN"),
        ];
        for &(value, expected) in cases {
            assert_eq!(
                format_float64_julia(value),
                expected,
                "format_float64_julia({:?}) should match upstream Julia",
                value
            );
        }
    }

    #[test]
    fn test_format_float32_julia_delegates_to_f64() {
        assert_eq!(format_float32_julia(1.5_f32), "1.5");
        assert_eq!(format_float32_julia(100000.0_f32), "100000.0");
        assert_eq!(format_float32_julia(1.0e7_f32), "1.0e7");
        assert_eq!(format_float32_julia(-0.0_f32), "-0.0");
        assert_eq!(format_float32_julia(f32::INFINITY), "Inf");
    }
}
