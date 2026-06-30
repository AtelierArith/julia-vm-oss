//! Math builtin functions for the VM.
//!
//! Rounding operations, bit operations, float decomposition, and fused multiply-add.
//! Note: Trigonometric (sin, cos, tan, asin, acos, atan) and exponential/logarithmic
//! (exp, log) functions have been migrated to Pure Julia (base/math.jl).

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::intrinsics_exec::{
    apply_unary_float_op_with_heap, apply_unary_rounding_op_with_heap, value_to_f64_with_heap,
};
use super::stack_ops::StackOps;
use super::value::{RustBigFloat, Value};
use super::Vm;

/// Safely narrow i64 to i32 by clamping to i32 range.
/// Used for powi() exponents: extreme values produce 0.0 or Inf, matching Julia behavior.
fn saturating_i64_to_i32(n: i64) -> i32 {
    n.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

// Note: next_float_f16 / prev_float_f16 / step_f64 / step_f32 / step_f16 removed —
// nextfloat/prevfloat (1- and 2-arg) are now Pure Julia (base/float.jl, Issue #6740).

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
    // Note: step_next_prev_float_n removed — two-argument nextfloat(x, n)/
    // prevfloat(x, n) are now Pure Julia (base/float.jl, Issue #6740).

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
            BuiltinId::Sqrt => {
                let value = self.stack.pop_value()?;
                let x = value_to_f64_with_heap(&value, &self.struct_heap)?;
                if x < 0.0 {
                    self.raise(VmError::DomainError(format!(
                        "sqrt was called with a negative real argument ({}) but will only return a complex result if called with a complex argument. Try sqrt(Complex(x)).",
                        x
                    )))?;
                    return Ok(Some(()));
                }
                self.stack.push(apply_unary_float_op_with_heap(
                    value,
                    &self.struct_heap,
                    f64::sqrt,
                )?);
            }

            // Rounding
            BuiltinId::Round => {
                // Julia's default `RoundNearest` is round-half-to-even (banker's
                // rounding): round(2.5) == 2.0, round(3.5) == 4.0. Rust's
                // `f64::round` rounds half away from zero, so use
                // `round_ties_even` to match upstream (round(0.5)==0.0).
                let value = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    value,
                    &self.struct_heap,
                    f64::round_ties_even,
                    RustBigFloat::round_nearest_even,
                )?);
            }
            BuiltinId::RoundDigits => {
                // round(x, digits=N) - round to N decimal places (Issue #2051).
                // Uses round-half-to-even per Julia's default RoundNearest.
                let n = self.stack.pop_i64()?;
                let x = self.pop_f64_or_i64()?;
                let factor = 10f64.powi(saturating_i64_to_i32(n));
                self.stack
                    .push(Value::F64((x * factor).round_ties_even() / factor));
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
                    self.stack
                        .push(Value::F64((x * factor).round_ties_even() / factor));
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
                let value = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    value,
                    &self.struct_heap,
                    f64::trunc,
                    RustBigFloat::trunc,
                )?);
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
            // Note: NextFloat/PrevFloat (1-arg) and NextFloatN/PrevFloatN (2-arg)
            // removed — nextfloat/prevfloat are now Pure Julia (base/float.jl, Issue #6740).

            // Bit operations (work on integers). Issue #4785: must
            // dispatch on element type so the bit width is preserved.
            // Previously all popped via pop_i64 which zero-/sign-
            // extends narrower types to 64 bits, inflating zero counts.
            BuiltinId::CountOnes => {
                let v = self.stack.pop_value()?;
                let n = match v {
                    Value::I8(x) => i64::from(x.count_ones()),
                    Value::I16(x) => i64::from(x.count_ones()),
                    Value::I32(x) => i64::from(x.count_ones()),
                    Value::I64(x) => i64::from(x.count_ones()),
                    Value::I128(x) => i64::from(x.count_ones()),
                    Value::U8(x) => i64::from(x.count_ones()),
                    Value::U16(x) => i64::from(x.count_ones()),
                    Value::U32(x) => i64::from(x.count_ones()),
                    Value::U64(x) => i64::from(x.count_ones()),
                    Value::U128(x) => i64::from(x.count_ones()),
                    Value::Bool(b) => i64::from(b),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "count_ones: expected integer, got {:?}",
                            v.value_type()
                        )))
                    }
                };
                self.stack.push(Value::I64(n));
            }
            BuiltinId::LeadingZeros => {
                let v = self.stack.pop_value()?;
                let n = match v {
                    Value::I8(x) => i64::from(x.leading_zeros()),
                    Value::I16(x) => i64::from(x.leading_zeros()),
                    Value::I32(x) => i64::from(x.leading_zeros()),
                    Value::I64(x) => i64::from(x.leading_zeros()),
                    Value::I128(x) => i64::from(x.leading_zeros()),
                    Value::U8(x) => i64::from(x.leading_zeros()),
                    Value::U16(x) => i64::from(x.leading_zeros()),
                    Value::U32(x) => i64::from(x.leading_zeros()),
                    Value::U64(x) => i64::from(x.leading_zeros()),
                    Value::U128(x) => i64::from(x.leading_zeros()),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "leading_zeros: expected integer, got {:?}",
                            v.value_type()
                        )))
                    }
                };
                self.stack.push(Value::I64(n));
            }
            BuiltinId::Bitreverse => {
                let v = self.stack.pop_value()?;
                let result = match v {
                    Value::I8(x) => Value::I8(x.reverse_bits()),
                    Value::I16(x) => Value::I16(x.reverse_bits()),
                    Value::I32(x) => Value::I32(x.reverse_bits()),
                    Value::I64(x) => Value::I64(x.reverse_bits()),
                    Value::I128(x) => Value::I128(x.reverse_bits()),
                    Value::U8(x) => Value::U8(x.reverse_bits()),
                    Value::U16(x) => Value::U16(x.reverse_bits()),
                    Value::U32(x) => Value::U32(x.reverse_bits()),
                    Value::U64(x) => Value::U64(x.reverse_bits()),
                    Value::U128(x) => Value::U128(x.reverse_bits()),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "bitreverse: expected integer, got {:?}",
                            v.value_type()
                        )))
                    }
                };
                self.stack.push(result);
            }
            BuiltinId::TrailingZeros => {
                let v = self.stack.pop_value()?;
                let n = match v {
                    Value::I8(x) => i64::from(x.trailing_zeros()),
                    Value::I16(x) => i64::from(x.trailing_zeros()),
                    Value::I32(x) => i64::from(x.trailing_zeros()),
                    Value::I64(x) => i64::from(x.trailing_zeros()),
                    Value::I128(x) => i64::from(x.trailing_zeros()),
                    Value::U8(x) => i64::from(x.trailing_zeros()),
                    Value::U16(x) => i64::from(x.trailing_zeros()),
                    Value::U32(x) => i64::from(x.trailing_zeros()),
                    Value::U64(x) => i64::from(x.trailing_zeros()),
                    Value::U128(x) => i64::from(x.trailing_zeros()),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "trailing_zeros: expected integer, got {:?}",
                            v.value_type()
                        )))
                    }
                };
                self.stack.push(Value::I64(n));
            }
            BuiltinId::Bswap => {
                // Issue #4787: swap bytes within the original integer
                // type's width and preserve the element type (UInt8
                // -> UInt8, UInt16 -> UInt16, ...). Was coercing all
                // narrower types to I64 via pop_i64 and swapping 8
                // bytes regardless. Mirrors the bit-op fix from #4785.
                let v = self.stack.pop_value()?;
                let result = match v {
                    Value::I8(x) => Value::I8(x.swap_bytes()),
                    Value::I16(x) => Value::I16(x.swap_bytes()),
                    Value::I32(x) => Value::I32(x.swap_bytes()),
                    Value::I64(x) => Value::I64(x.swap_bytes()),
                    Value::I128(x) => Value::I128(x.swap_bytes()),
                    Value::U8(x) => Value::U8(x.swap_bytes()),
                    Value::U16(x) => Value::U16(x.swap_bytes()),
                    Value::U32(x) => Value::U32(x.swap_bytes()),
                    Value::U64(x) => Value::U64(x.swap_bytes()),
                    Value::U128(x) => Value::U128(x.swap_bytes()),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "bswap: expected integer, got {:?}",
                            v.value_type()
                        )))
                    }
                };
                self.stack.push(result);
            }

            // Note: Float decomposition (Exponent/Significand/Frexp) and inspection
            // (Issubnormal) removed — now Pure Julia (base/float.jl, Issue #6740).
            // Note: Maxintfloat removed — Pure Julia (base/floatfuncs.jl, Issue #3732).
            // Note: Muladd removed — Pure Julia (base/math.jl, Issue #3732).

            // Internal IEEE fused multiply-add primitive (Issue #3732).
            // Pure Julia `fma(x::Float64, y::Float64, z::Float64)` calls `_fma`
            // to preserve single-rounded fused semantics; non-Float64 forms use
            // the plain `x*y + z` Pure Julia path.
            BuiltinId::Fma => {
                // _fma(x, y, z) = fma(x, y, z) with single rounding
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
