//! Math builtin functions for the VM.
//!
//! Rounding operations, bit operations, float decomposition, and fused multiply-add.
//! Note: Trigonometric (sin, cos, tan, asin, acos, atan) and exponential/logarithmic
//! (exp, log) functions have been migrated to Pure Julia (base/math.jl).

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{TupleValue, Value};
use super::Vm;

/// Safely narrow i64 to i32 by clamping to i32 range.
/// Used for powi() exponents: extreme values produce 0.0 or Inf, matching Julia behavior.
fn saturating_i64_to_i32(n: i64) -> i32 {
    n.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// Safely convert f64 to i32 by clamping to i32 range.
/// Returns 0 for NaN (matches Rust's saturating float-to-int behavior).
fn saturating_f64_to_i32(x: f64) -> i32 {
    if x.is_nan() {
        0
    } else if x >= i32::MAX as f64 {
        i32::MAX
    } else if x <= i32::MIN as f64 {
        i32::MIN
    } else {
        x as i32
    }
}

impl<R: RngLike> Vm<R> {
    /// Execute math builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a math builtin.
    pub(super) fn execute_builtin_math(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            // Note: Trigonometric (Sin, Cos, Tan, Asin, Acos, Atan) removed — now Pure Julia (base/math.jl)
            // Note: Exponential/Logarithmic (Exp, Log) removed — now Pure Julia (base/math.jl)

            // Rounding
            BuiltinId::Round => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.round()));
            }
            BuiltinId::RoundDigits => {
                // round(x, digits=N) - round to N decimal places (Issue #2051)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                let factor = 10f64.powi(saturating_i64_to_i32(n));
                self.stack.push(Value::F64((x * factor).round() / factor));
            }
            BuiltinId::RoundSigDigits => {
                // round(x, sigdigits=N) - round to N significant digits (Issue #2051)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                if x == 0.0 || n <= 0 {
                    self.stack.push(Value::F64(0.0));
                } else {
                    let d = saturating_f64_to_i32(x.abs().log10().floor() + 1.0);
                    let factor = 10f64.powi(saturating_i64_to_i32(n) - d);
                    self.stack.push(Value::F64((x * factor).round() / factor));
                }
            }
            BuiltinId::FloorDigits => {
                // floor(x, digits=N) - floor to N decimal places (Issue #2054)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                let factor = 10f64.powi(saturating_i64_to_i32(n));
                self.stack.push(Value::F64((x * factor).floor() / factor));
            }
            BuiltinId::FloorSigDigits => {
                // floor(x, sigdigits=N) - floor to N significant digits (Issue #2054)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                if x == 0.0 || n <= 0 {
                    self.stack.push(Value::F64(0.0));
                } else {
                    let d = saturating_f64_to_i32(x.abs().log10().floor() + 1.0);
                    let factor = 10f64.powi(saturating_i64_to_i32(n) - d);
                    self.stack.push(Value::F64((x * factor).floor() / factor));
                }
            }
            BuiltinId::CeilDigits => {
                // ceil(x, digits=N) - ceil to N decimal places (Issue #2054)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                let factor = 10f64.powi(saturating_i64_to_i32(n));
                self.stack.push(Value::F64((x * factor).ceil() / factor));
            }
            BuiltinId::CeilSigDigits => {
                // ceil(x, sigdigits=N) - ceil to N significant digits (Issue #2054)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                if x == 0.0 || n <= 0 {
                    self.stack.push(Value::F64(0.0));
                } else {
                    let d = saturating_f64_to_i32(x.abs().log10().floor() + 1.0);
                    let factor = 10f64.powi(saturating_i64_to_i32(n) - d);
                    self.stack.push(Value::F64((x * factor).ceil() / factor));
                }
            }
            BuiltinId::Trunc => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.trunc()));
            }
            BuiltinId::TruncDigits => {
                // trunc(x, digits=N) - trunc to N decimal places (Issue #2059)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                let factor = 10f64.powi(saturating_i64_to_i32(n));
                self.stack.push(Value::F64((x * factor).trunc() / factor));
            }
            BuiltinId::TruncSigDigits => {
                // trunc(x, sigdigits=N) - trunc to N significant digits (Issue #2059)
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                if x == 0.0 || n <= 0 {
                    self.stack.push(Value::F64(0.0));
                } else {
                    let d = saturating_f64_to_i32(x.abs().log10().floor() + 1.0);
                    let factor = 10f64.powi(saturating_i64_to_i32(n) - d);
                    self.stack.push(Value::F64((x * factor).trunc() / factor));
                }
            }
            BuiltinId::NextFloat => {
                let x = self.pop_f64_or_i64()?;
                let result = if x.is_nan() || x == f64::INFINITY {
                    x
                } else if x == f64::NEG_INFINITY {
                    f64::MIN
                } else if x == 0.0 {
                    // Both +0.0 and -0.0 go to smallest positive
                    f64::from_bits(1)
                } else if x > 0.0 {
                    f64::from_bits(x.to_bits() + 1)
                } else {
                    // x < 0: decrement bits (moves toward zero)
                    f64::from_bits(x.to_bits() - 1)
                };
                self.stack.push(Value::F64(result));
            }
            BuiltinId::PrevFloat => {
                let x = self.pop_f64_or_i64()?;
                let result = if x.is_nan() || x == f64::NEG_INFINITY {
                    x
                } else if x == f64::INFINITY {
                    f64::MAX
                } else if x == 0.0 {
                    // Both +0.0 and -0.0 go to smallest negative
                    -f64::from_bits(1)
                } else if x > 0.0 {
                    f64::from_bits(x.to_bits() - 1)
                } else {
                    // x < 0: increment bits (moves away from zero)
                    f64::from_bits(x.to_bits() + 1)
                };
                self.stack.push(Value::F64(result));
            }

            // Bit operations (work on integers)
            BuiltinId::CountOnes => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(i64::from(x.count_ones())));
            }
            BuiltinId::CountZeros => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(i64::from(x.count_zeros())));
            }
            BuiltinId::LeadingZeros => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(i64::from(x.leading_zeros())));
            }
            BuiltinId::TrailingOnes => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(i64::from(x.trailing_ones())));
            }
            BuiltinId::Bitreverse => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(x.reverse_bits()));
            }
            BuiltinId::LeadingOnes => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(i64::from(x.leading_ones())));
            }
            BuiltinId::TrailingZeros => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(i64::from(x.trailing_zeros())));
            }
            BuiltinId::Bitrotate => {
                // bitrotate(x, k) - rotate bits left by k (or right if k < 0)
                let k = self.stack.pop_i64()?;
                let x = self.stack.pop_i64()?;
                // Julia's bitrotate for Int64: rotation amount = k mod 64 (mathematical modulo).
                // rem_euclid always returns 0..63, safe to cast to u32.
                // This avoids: (1) truncation in `k as u32`, (2) overflow in `-k` for i64::MIN.
                let result = x.rotate_left(k.rem_euclid(64) as u32);
                self.stack.push(Value::I64(result));
            }
            BuiltinId::Bswap => {
                let x = self.stack.pop_i64()?;
                self.stack.push(Value::I64(x.swap_bytes()));
            }

            // Float decomposition (IEEE 754)
            BuiltinId::Exponent => {
                // exponent(x) returns the exponent of x as an integer
                // For normalized numbers: floor(log2(abs(x)))
                // Special cases: 0 -> error in Julia, we return 0; Inf/NaN -> error
                let x = self.pop_f64_or_i64()?;
                if x == 0.0 {
                    // Julia throws DomainError for 0, we return a sentinel
                    self.stack.push(Value::I64(i64::MIN));
                } else if x.is_infinite() || x.is_nan() {
                    self.stack.push(Value::I64(i64::MAX));
                } else {
                    // Extract exponent from IEEE 754 representation
                    let bits = x.abs().to_bits();
                    // (bits >> 52) & 0x7FF is at most 2047, always fits in i64
                    let biased_exp = ((bits >> 52) & 0x7FF) as i64;
                    let exp = biased_exp - 1023; // Remove bias
                    self.stack.push(Value::I64(exp));
                }
            }
            BuiltinId::Significand => {
                // significand(x) returns the significand (mantissa) in [1, 2) for normalized
                let x = self.pop_f64_or_i64()?;
                if x == 0.0 || x.is_infinite() || x.is_nan() {
                    self.stack.push(Value::F64(x));
                } else {
                    // Set exponent to 0 (biased 1023) to get value in [1, 2)
                    let bits = x.to_bits();
                    let sign_bit = bits & (1u64 << 63);
                    let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;
                    let new_bits = sign_bit | (1023u64 << 52) | mantissa;
                    self.stack.push(Value::F64(f64::from_bits(new_bits)));
                }
            }
            BuiltinId::Frexp => {
                // frexp(x) returns (significand, exponent) where x = significand * 2^exponent
                // significand is in [0.5, 1.0) for normalized numbers
                let x = self.pop_f64_or_i64()?;
                if x == 0.0 {
                    // Return (0.0, 0)
                    self.stack.push(Value::Tuple(TupleValue::new(vec![
                        Value::F64(0.0),
                        Value::I64(0),
                    ])));
                } else if x.is_infinite() || x.is_nan() {
                    self.stack.push(Value::Tuple(TupleValue::new(vec![
                        Value::F64(x),
                        Value::I64(0),
                    ])));
                } else {
                    let bits = x.abs().to_bits();
                    // (bits >> 52) & 0x7FF is at most 2047, always fits in i64
                    let biased_exp = ((bits >> 52) & 0x7FF) as i64;
                    let exp = biased_exp - 1022; // Adjust for [0.5, 1.0) range
                                                 // Set exponent to -1 (biased 1022) to get value in [0.5, 1.0)
                    let sign_bit = x.to_bits() & (1u64 << 63);
                    let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;
                    let new_bits = sign_bit | (1022u64 << 52) | mantissa;
                    let sig = f64::from_bits(new_bits);
                    self.stack.push(Value::Tuple(TupleValue::new(vec![
                        Value::F64(sig),
                        Value::I64(exp),
                    ])));
                }
            }

            // Float inspection
            BuiltinId::Issubnormal => {
                let x = self.pop_f64_or_i64()?;
                // Subnormal: exponent bits are all 0, mantissa is non-zero
                let bits = x.to_bits();
                let exp_bits = (bits >> 52) & 0x7FF;
                let mantissa = bits & 0x000F_FFFF_FFFF_FFFF;
                let is_subnormal = exp_bits == 0 && mantissa != 0;
                self.stack.push(Value::Bool(is_subnormal));
            }
            BuiltinId::Maxintfloat => {
                // maxintfloat(Float64) = 2^53 (largest integer exactly representable)
                // 2^53 = 9007199254740992.0
                self.stack.push(Value::F64(9007199254740992.0));
            }

            // Fused multiply-add
            BuiltinId::Fma => {
                // fma(x, y, z) = x*y + z with single rounding
                let z = self.pop_f64_or_i64()?;
                let y = self.pop_f64_or_i64()?;
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.mul_add(y, z)));
            }
            BuiltinId::Muladd => {
                // muladd(x, y, z) = x*y + z (may use fma if available)
                // In Rust, mul_add uses fma when available
                let z = self.pop_f64_or_i64()?;
                let y = self.pop_f64_or_i64()?;
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.mul_add(y, z)));
            }

            // Note: gcd, lcm, factorial removed - now Pure Julia (base/intfuncs.jl)
            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
