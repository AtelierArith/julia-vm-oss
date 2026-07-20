//! Numeric type conversion helpers.
//!
// All `as` casts in this module are intentional numeric coercions (Julia
// explicit type constructor calls, e.g. `UInt8(x)`).  Sign loss / truncation
// is detected by the InexactError pattern: `(x as TargetType) as f64 != x`.
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use half::f16;
use num_traits::ToPrimitive;

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::value::{RustBigFloat, Value};
use crate::vm::Vm;

/// Exact `BigFloat -> integer` value, or `InexactError` when the BigFloat is
/// non-finite or not integer-valued — matching upstream `(::Type{<:Integer})
/// (x::BigFloat)` (Issue #6890). `ty` is the Julia type name for the error
/// message (e.g. `"Int64"`). The returned `BigInt` is range-checked into the
/// concrete width by each caller via `ToPrimitive`.
fn bigfloat_to_exact_bigint(bf: &RustBigFloat, ty: &str) -> Result<num_bigint::BigInt, VmError> {
    bf.to_bigint_exact().ok_or_else(|| {
        VmError::InexactError(format!("{}({})", ty, crate::vm::format_bigfloat_julia(bf)))
    })
}

/// Whether the finite float `n` is both integer-valued and inside
/// `[min, max_exclusive)` — the exact predicate the checked integer
/// constructors need, matching upstream `(::Type{T})(x::AbstractFloat)`'s
/// range check (`julia/base/float.jl`: `isinteger(x) && min <= x < max`).
///
/// This must run on the ORIGINAL float value with an explicit range compare,
/// not "cast to the target type, then cast back and compare" (Issue #11214):
/// Rust's float-to-int `as` cast SATURATES on overflow, and the saturated
/// boundary value can round-trip back to a float equal to an out-of-range
/// input. E.g. `i64::MAX` (9223372036854775807) is not exactly representable
/// in `f64` — the nearest representable value is `2.0^63` — so the old
/// `(x as i64) as f64 == x` check silently accepted `Float64(2.0^63)` and
/// saturated it to `typemax(Int64)` instead of raising `InexactError`.
fn float_in_range(n: f64, min: f64, max_exclusive: f64) -> bool {
    n.is_finite() && n.trunc() == n && n >= min && n < max_exclusive
}

/// `[min, max_exclusive)` bounds for a signed integer type of `bits` width,
/// as `f64`. Both bounds are powers of two (or their negation), so both are
/// exactly representable in `f64` for every width up to 128.
fn signed_int_f64_bounds(bits: i32) -> (f64, f64) {
    let max_exclusive = 2f64.powi(bits - 1);
    (-max_exclusive, max_exclusive)
}

/// `[0, max_exclusive)` bounds for an unsigned integer type of `bits` width,
/// as `f64`.
fn unsigned_int_f64_max_exclusive(bits: i32) -> f64 {
    2f64.powi(bits)
}

impl<R: RngLike> Vm<R> {
    /// The `MethodError` upstream raises when a value has no `convert` method
    /// for the target type (Issue #11146, corpus row `convert_failure`).
    ///
    /// `convert(Int, "a")` raises, verified against julia 1.12.6:
    /// `MethodError: Cannot \`convert\` an object of type String to an object of
    /// type Int64`. sjulia raised a `TypeError` from every `convert_to_*`
    /// fallback — the same TypeError-vs-MethodError class Issue #10481 closed
    /// for `sqrt(::String)`, surviving at a second, independent call site
    /// because each site picked its own "nearest" error instead of consulting
    /// one taxonomy. A `catch e; e isa MethodError` block (the upstream-
    /// idiomatic pattern) silently took the wrong branch.
    ///
    /// Note this is deliberately NOT the path for a value that HAS a conversion
    /// but cannot represent the result (`Int(1.5)`): that stays `InexactError`,
    /// matching upstream.
    fn convert_method_error(&self, val: &Value, target: &str) -> VmError {
        VmError::MethodError(format!(
            "Cannot `convert` an object of type {} to an object of type {}",
            self.get_type_name(val),
            target
        ))
    }

    // =========================================================================
    // Numeric Type Conversion Helpers
    // =========================================================================

    /// Issue #5355: resolve a `Rational` struct value (inline `Value::Struct`
    /// or heap `Value::StructRef`) to its `(numerator, denominator)` i64 parts.
    ///
    /// The numeric type *constructors* (`Float64(r)`, `Int(r)`, ...) route
    /// through the `convert_to_*` helpers below, which previously only matched
    /// primitive `Value`s and errored on a Rational. The conversion
    /// *instructions* (`exec/conversion.rs`) already handled Rationals; this
    /// brings the constructors into line via the shared
    /// `StructInstance::as_rational_parts_i64` helper.
    fn value_as_rational_parts(&self, val: &Value) -> Option<(i64, i64)> {
        match val {
            Value::Struct(s) => s.as_rational_parts_i64(),
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .and_then(|s| s.as_rational_parts_i64()),
            _ => None,
        }
    }

    pub(in crate::vm) fn value_as_irrational_f64(&self, val: &Value) -> Option<f64> {
        match val {
            Value::Struct(s) => s.as_irrational_f64(),
            Value::StructRef(idx) => self
                .struct_heap
                .get(*idx)
                .and_then(|s| s.as_irrational_f64()),
            _ => None,
        }
    }

    pub(in crate::vm) fn convert_to_i8(&self, val: &Value) -> Result<i8, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => Ok(*n),
            Value::I16(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::I32(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::I64(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::I128(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U8(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U16(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U32(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U64(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U128(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::F32(n) => {
                let (min, max) = signed_int_f64_bounds(8);
                if float_in_range(*n as f64, min, max) {
                    Ok(*n as i8)
                } else {
                    Err(VmError::InexactError(format!("Int8({})", n)))
                }
            }
            Value::F64(n) => {
                let (min, max) = signed_int_f64_bounds(8);
                if float_in_range(*n, min, max) {
                    Ok(*n as i8)
                } else {
                    Err(VmError::InexactError(format!("Int8({})", n)))
                }
            }
            // Issue #6890: BigFloat -> Int8; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "Int8")?;
                bi.to_i8()
                    .ok_or_else(|| VmError::InexactError(format!("Int8({})", bi)))
            }
            // Issue #5355: Rational -> Int8; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => {
                    i8::try_from(num).map_err(|_| VmError::InexactError(format!("Int8({})", num)))
                }
                Some((num, den)) => Err(VmError::InexactError(format!("Int8({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "Int8")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_i16(&self, val: &Value) -> Result<i16, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => Ok(*n as i16),
            Value::I16(n) => Ok(*n),
            Value::I32(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::I64(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::I128(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U8(n) => Ok(*n as i16),
            Value::U16(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U32(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U64(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U128(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::F32(n) => {
                let (min, max) = signed_int_f64_bounds(16);
                if float_in_range(*n as f64, min, max) {
                    Ok(*n as i16)
                } else {
                    Err(VmError::InexactError(format!("Int16({})", n)))
                }
            }
            Value::F64(n) => {
                let (min, max) = signed_int_f64_bounds(16);
                if float_in_range(*n, min, max) {
                    Ok(*n as i16)
                } else {
                    Err(VmError::InexactError(format!("Int16({})", n)))
                }
            }
            // Issue #6890: BigFloat -> Int16; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "Int16")?;
                bi.to_i16()
                    .ok_or_else(|| VmError::InexactError(format!("Int16({})", bi)))
            }
            // Issue #5355: Rational -> Int16; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => {
                    i16::try_from(num).map_err(|_| VmError::InexactError(format!("Int16({})", num)))
                }
                Some((num, den)) => Err(VmError::InexactError(format!("Int16({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "Int16")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_i32(&self, val: &Value) -> Result<i32, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => Ok(*n as i32),
            Value::I16(n) => Ok(*n as i32),
            Value::I32(n) => Ok(*n),
            // Enum members convert to their backing integer (Issue #5139).
            Value::Enum { value, .. } => i32::try_from(*value)
                .map_err(|_| VmError::InexactError(format!("Int32({})", value))),
            Value::I64(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::I128(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::U8(n) => Ok(*n as i32),
            Value::U16(n) => Ok(*n as i32),
            Value::U32(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::U64(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::U128(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::F32(n) => {
                let (min, max) = signed_int_f64_bounds(32);
                if float_in_range(*n as f64, min, max) {
                    Ok(*n as i32)
                } else {
                    Err(VmError::InexactError(format!("Int32({})", n)))
                }
            }
            Value::F64(n) => {
                let (min, max) = signed_int_f64_bounds(32);
                if float_in_range(*n, min, max) {
                    Ok(*n as i32)
                } else {
                    Err(VmError::InexactError(format!("Int32({})", n)))
                }
            }
            // Issue #6890: BigFloat -> Int32; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "Int32")?;
                bi.to_i32()
                    .ok_or_else(|| VmError::InexactError(format!("Int32({})", bi)))
            }
            // Issue #5355: Rational -> Int32; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => {
                    i32::try_from(num).map_err(|_| VmError::InexactError(format!("Int32({})", num)))
                }
                Some((num, den)) => Err(VmError::InexactError(format!("Int32({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "Int32")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_i64(&self, val: &Value) -> Result<i64, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => Ok(*n as i64),
            Value::I16(n) => Ok(*n as i64),
            Value::I32(n) => Ok(*n as i64),
            Value::I64(n) => Ok(*n),
            Value::I128(n) => {
                i64::try_from(*n).map_err(|_| VmError::InexactError(format!("Int64({})", n)))
            }
            Value::U8(n) => Ok(*n as i64),
            Value::U16(n) => Ok(*n as i64),
            Value::U32(n) => Ok(*n as i64),
            Value::U64(n) => {
                i64::try_from(*n).map_err(|_| VmError::InexactError(format!("Int64({})", n)))
            }
            Value::U128(n) => {
                i64::try_from(*n).map_err(|_| VmError::InexactError(format!("Int64({})", n)))
            }
            Value::F32(n) => {
                let (min, max) = signed_int_f64_bounds(64);
                if float_in_range(*n as f64, min, max) {
                    Ok(*n as i64)
                } else {
                    Err(VmError::InexactError(format!("Int64({})", n)))
                }
            }
            Value::F64(n) => {
                let (min, max) = signed_int_f64_bounds(64);
                if float_in_range(*n, min, max) {
                    Ok(*n as i64)
                } else {
                    Err(VmError::InexactError(format!("Int64({})", n)))
                }
            }
            Value::Char(c) => Ok(*c as i64),
            // Enum members convert to their backing integer (Issue #5139):
            // `Int(red) == 0`. The default `Int` width is Int64 on 64-bit.
            Value::Enum { value, .. } => Ok(*value),
            // Issue #11214: out-of-range BigInt raises InexactError upstream
            // (`Int64(big"2^63")`), not TypeError.
            Value::BigInt(n) => n
                .to_i64()
                .ok_or_else(|| VmError::InexactError(format!("Int64({})", n))),
            // Issue #6890: BigFloat -> Int64; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "Int64")?;
                bi.to_i64()
                    .ok_or_else(|| VmError::InexactError(format!("Int64({})", bi)))
            }
            // Issue #5355: Rational -> Int64; exact only (den == 1), else
            // InexactError, matching upstream `(::Type{T})(x::Rational)`.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => Ok(num),
                Some((num, den)) => Err(VmError::InexactError(format!("Int64({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "Int64")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_i128(&self, val: &Value) -> Result<i128, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => Ok(*n as i128),
            Value::I16(n) => Ok(*n as i128),
            Value::I32(n) => Ok(*n as i128),
            Value::I64(n) => Ok(*n as i128),
            Value::I128(n) => Ok(*n),
            Value::U8(n) => Ok(*n as i128),
            Value::U16(n) => Ok(*n as i128),
            Value::U32(n) => Ok(*n as i128),
            Value::U64(n) => Ok(*n as i128),
            Value::U128(n) => {
                i128::try_from(*n).map_err(|_| VmError::InexactError(format!("Int128({})", n)))
            }
            Value::F32(n) => {
                let (min, max) = signed_int_f64_bounds(128);
                if float_in_range(*n as f64, min, max) {
                    Ok(*n as i128)
                } else {
                    Err(VmError::InexactError(format!("Int128({})", n)))
                }
            }
            Value::F64(n) => {
                let (min, max) = signed_int_f64_bounds(128);
                if float_in_range(*n, min, max) {
                    Ok(*n as i128)
                } else {
                    Err(VmError::InexactError(format!("Int128({})", n)))
                }
            }
            // Issue #6890: BigFloat -> Int128; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "Int128")?;
                bi.to_i128()
                    .ok_or_else(|| VmError::InexactError(format!("Int128({})", bi)))
            }
            // Issue #5355: Rational -> Int128; exact only (den == 1).
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => Ok(num as i128),
                Some((num, den)) => Err(VmError::InexactError(format!("Int128({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "Int128")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_u8(&self, val: &Value) -> Result<u8, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I16(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I32(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I64(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I128(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U8(n) => Ok(*n),
            Value::U16(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U32(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U64(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U128(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::F32(n) => {
                let max = unsigned_int_f64_max_exclusive(8);
                if float_in_range(*n as f64, 0.0, max) {
                    Ok(*n as u8)
                } else {
                    Err(VmError::InexactError(format!("UInt8({})", n)))
                }
            }
            Value::F64(n) => {
                let max = unsigned_int_f64_max_exclusive(8);
                if float_in_range(*n, 0.0, max) {
                    Ok(*n as u8)
                } else {
                    Err(VmError::InexactError(format!("UInt8({})", n)))
                }
            }
            // Issue #6890: BigFloat -> UInt8; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "UInt8")?;
                bi.to_u8()
                    .ok_or_else(|| VmError::InexactError(format!("UInt8({})", bi)))
            }
            // Issue #5355: Rational -> UInt8; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => {
                    u8::try_from(num).map_err(|_| VmError::InexactError(format!("UInt8({})", num)))
                }
                Some((num, den)) => Err(VmError::InexactError(format!("UInt8({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "UInt8")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_u16(&self, val: &Value) -> Result<u16, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I16(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I32(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I64(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I128(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::U8(n) => Ok(*n as u16),
            Value::U16(n) => Ok(*n),
            Value::U32(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::U64(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::U128(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::F32(n) => {
                let max = unsigned_int_f64_max_exclusive(16);
                if float_in_range(*n as f64, 0.0, max) {
                    Ok(*n as u16)
                } else {
                    Err(VmError::InexactError(format!("UInt16({})", n)))
                }
            }
            Value::F64(n) => {
                let max = unsigned_int_f64_max_exclusive(16);
                if float_in_range(*n, 0.0, max) {
                    Ok(*n as u16)
                } else {
                    Err(VmError::InexactError(format!("UInt16({})", n)))
                }
            }
            // Issue #6890: BigFloat -> UInt16; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "UInt16")?;
                bi.to_u16()
                    .ok_or_else(|| VmError::InexactError(format!("UInt16({})", bi)))
            }
            // Issue #5355: Rational -> UInt16; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => u16::try_from(num)
                    .map_err(|_| VmError::InexactError(format!("UInt16({})", num))),
                Some((num, den)) => Err(VmError::InexactError(format!("UInt16({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "UInt16")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_u32(&self, val: &Value) -> Result<u32, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I16(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I32(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I64(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I128(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::U8(n) => Ok(*n as u32),
            Value::U16(n) => Ok(*n as u32),
            Value::U32(n) => Ok(*n),
            Value::U64(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::U128(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::F32(n) => {
                let max = unsigned_int_f64_max_exclusive(32);
                if float_in_range(*n as f64, 0.0, max) {
                    Ok(*n as u32)
                } else {
                    Err(VmError::InexactError(format!("UInt32({})", n)))
                }
            }
            Value::F64(n) => {
                let max = unsigned_int_f64_max_exclusive(32);
                if float_in_range(*n, 0.0, max) {
                    Ok(*n as u32)
                } else {
                    Err(VmError::InexactError(format!("UInt32({})", n)))
                }
            }
            // Issue #6890: BigFloat -> UInt32; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "UInt32")?;
                bi.to_u32()
                    .ok_or_else(|| VmError::InexactError(format!("UInt32({})", bi)))
            }
            // Issue #5355: Rational -> UInt32; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => u32::try_from(num)
                    .map_err(|_| VmError::InexactError(format!("UInt32({})", num))),
                Some((num, den)) => Err(VmError::InexactError(format!("UInt32({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "UInt32")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_u64(&self, val: &Value) -> Result<u64, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I16(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I32(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I64(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I128(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::U8(n) => Ok(*n as u64),
            Value::U16(n) => Ok(*n as u64),
            Value::U32(n) => Ok(*n as u64),
            Value::U64(n) => Ok(*n),
            Value::U128(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::F32(n) => {
                let max = unsigned_int_f64_max_exclusive(64);
                if float_in_range(*n as f64, 0.0, max) {
                    Ok(*n as u64)
                } else {
                    Err(VmError::InexactError(format!("UInt64({})", n)))
                }
            }
            Value::F64(n) => {
                let max = unsigned_int_f64_max_exclusive(64);
                if float_in_range(*n, 0.0, max) {
                    Ok(*n as u64)
                } else {
                    Err(VmError::InexactError(format!("UInt64({})", n)))
                }
            }
            // Issue #6890: BigFloat -> UInt64; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "UInt64")?;
                bi.to_u64()
                    .ok_or_else(|| VmError::InexactError(format!("UInt64({})", bi)))
            }
            // Issue #5355: Rational -> UInt64; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => u64::try_from(num)
                    .map_err(|_| VmError::InexactError(format!("UInt64({})", num))),
                Some((num, den)) => Err(VmError::InexactError(format!("UInt64({}//{})", num, den))),
                None => Err(self.convert_method_error(val, "UInt64")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_u128(&self, val: &Value) -> Result<u128, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I16(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I32(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I64(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I128(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::U8(n) => Ok(*n as u128),
            Value::U16(n) => Ok(*n as u128),
            Value::U32(n) => Ok(*n as u128),
            Value::U64(n) => Ok(*n as u128),
            Value::U128(n) => Ok(*n),
            // Issue #3559: large hex / binary literals (e.g.
            // `0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF`) overflow `i128` and are
            // parsed as `BigInt`. Allow `UInt128(::BigInt)` when the value
            // fits in `u128`.
            Value::BigInt(n) => n
                .to_u128()
                .ok_or_else(|| VmError::InexactError(format!("UInt128({})", n))),
            Value::F32(n) => {
                let max = unsigned_int_f64_max_exclusive(128);
                if float_in_range(*n as f64, 0.0, max) {
                    Ok(*n as u128)
                } else {
                    Err(VmError::InexactError(format!("UInt128({})", n)))
                }
            }
            Value::F64(n) => {
                let max = unsigned_int_f64_max_exclusive(128);
                if float_in_range(*n, 0.0, max) {
                    Ok(*n as u128)
                } else {
                    Err(VmError::InexactError(format!("UInt128({})", n)))
                }
            }
            // Issue #6890: BigFloat -> UInt128; exact integer-valued only.
            Value::BigFloat(bf) => {
                let bi = bigfloat_to_exact_bigint(bf, "UInt128")?;
                bi.to_u128()
                    .ok_or_else(|| VmError::InexactError(format!("UInt128({})", bi)))
            }
            // Issue #5355: Rational -> UInt128; exact only (den == 1), range-checked.
            _ => match self.value_as_rational_parts(val) {
                Some((num, 1)) => u128::try_from(num)
                    .map_err(|_| VmError::InexactError(format!("UInt128({})", num))),
                Some((num, den)) => {
                    Err(VmError::InexactError(format!("UInt128({}//{})", num, den)))
                }
                None => Err(self.convert_method_error(val, "UInt128")),
            },
        }
    }

    pub(in crate::vm) fn convert_to_f16(&self, val: &Value) -> Result<f16, VmError> {
        match val {
            Value::I8(n) => Ok(f16::from_f32(*n as f32)),
            Value::I16(n) => Ok(f16::from_f32(*n as f32)),
            Value::I32(n) => Ok(f16::from_f32(*n as f32)),
            Value::I64(n) => Ok(f16::from_f32(*n as f32)),
            Value::I128(n) => Ok(f16::from_f64(*n as f64)),
            Value::U8(n) => Ok(f16::from_f32(*n as f32)),
            Value::U16(n) => Ok(f16::from_f32(*n as f32)),
            Value::U32(n) => Ok(f16::from_f32(*n as f32)),
            Value::U64(n) => Ok(f16::from_f64(*n as f64)),
            Value::U128(n) => Ok(f16::from_f64(*n as f64)),
            Value::F16(n) => Ok(*n),
            Value::F32(n) => Ok(f16::from_f32(*n)),
            Value::F64(n) => Ok(f16::from_f64(*n)),
            Value::Bool(b) => Ok(f16::from_f32(if *b { 1.0 } else { 0.0 })),
            // Issue #5133: Irrational singleton -> Float16.
            _ if self.value_as_irrational_f64(val).is_some() => Ok(f16::from_f64(
                self.value_as_irrational_f64(val)
                    .ok_or_else(|| self.convert_method_error(val, "Float16"))?,
            )),
            // Issue #5355: Rational -> Float16 (num/den).
            _ => self
                .value_as_rational_parts(val)
                .map(|(num, den)| f16::from_f64(num as f64 / den as f64))
                .ok_or_else(|| self.convert_method_error(val, "Float16")),
        }
    }

    pub(in crate::vm) fn convert_to_f32(&self, val: &Value) -> Result<f32, VmError> {
        match val {
            Value::I8(n) => Ok(*n as f32),
            Value::I16(n) => Ok(*n as f32),
            Value::I32(n) => Ok(*n as f32),
            Value::I64(n) => Ok(*n as f32),
            Value::I128(n) => Ok(*n as f32),
            Value::U8(n) => Ok(*n as f32),
            Value::U16(n) => Ok(*n as f32),
            Value::U32(n) => Ok(*n as f32),
            Value::U64(n) => Ok(*n as f32),
            Value::U128(n) => Ok(*n as f32),
            Value::F16(n) => Ok(n.to_f32()),
            Value::F32(n) => Ok(*n),
            Value::F64(n) => Ok(*n as f32),
            // Issue #5133: Irrational singleton -> Float32.
            _ if self.value_as_irrational_f64(val).is_some() => self
                .value_as_irrational_f64(val)
                .map(|n| n as f32)
                .ok_or_else(|| self.convert_method_error(val, "Float32")),
            // Issue #5355: Rational -> Float32 (num/den).
            _ => self
                .value_as_rational_parts(val)
                .map(|(num, den)| num as f32 / den as f32)
                .ok_or_else(|| self.convert_method_error(val, "Float32")),
        }
    }

    pub(in crate::vm) fn convert_to_f64(&self, val: &Value) -> Result<f64, VmError> {
        match val {
            Value::I8(n) => Ok(*n as f64),
            Value::I16(n) => Ok(*n as f64),
            Value::I32(n) => Ok(*n as f64),
            Value::I64(n) => Ok(*n as f64),
            Value::I128(n) => Ok(*n as f64),
            Value::U8(n) => Ok(*n as f64),
            Value::U16(n) => Ok(*n as f64),
            Value::U32(n) => Ok(*n as f64),
            Value::U64(n) => Ok(*n as f64),
            Value::U128(n) => Ok(*n as f64),
            Value::F16(n) => Ok(n.to_f64()),
            Value::F32(n) => Ok(*n as f64),
            Value::F64(n) => Ok(*n),
            Value::BigFloat(n) => Ok(n.to_string().parse::<f64>().unwrap_or(f64::NAN)),
            // Issue #9383: BigInt -> Float64 (out-of-range magnitudes round to
            // ±Inf, matching upstream `Float64(::BigInt)`), mirroring the
            // established `intrinsics_exec.rs` BigInt->f64 pattern. Without this
            // arm `Float64(big(1))` fell through to the error below, and the
            // pure-Julia `+(x::Integer, y::AbstractIrrational)` / `promote`
            // methods crashed for a dynamically-typed BigInt operand.
            Value::BigInt(n) => Ok(n.to_f64().unwrap_or(f64::INFINITY)),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            // Issue #5133: Irrational singleton -> Float64.
            _ if self.value_as_irrational_f64(val).is_some() => self
                .value_as_irrational_f64(val)
                .ok_or_else(|| self.convert_method_error(val, "Float64")),
            // Issue #5355: Rational -> Float64 (num/den), matching upstream
            // `Float64(x::Rational)`.
            _ => self
                .value_as_rational_parts(val)
                .map(|(num, den)| num as f64 / den as f64)
                .ok_or_else(|| self.convert_method_error(val, "Float64")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::rng::StableRng;
    use crate::vm::error::VmError;
    use crate::vm::value::{RustBigInt, Value};
    use crate::vm::Vm;

    fn make_vm() -> Vm<StableRng> {
        Vm::new(vec![], StableRng::new(0))
    }

    // =========================================================================
    // BigInt -> Float64 (Issue #9383): convert_to_f64 previously had no
    // Value::BigInt arm, so Float64(big(1)) errored and the pure-Julia
    // `+(x::Integer, y::AbstractIrrational)` / `promote` methods crashed for a
    // dynamically-typed BigInt operand.
    // =========================================================================

    #[test]
    fn convert_to_f64_handles_bigint_issue_9383() {
        use std::str::FromStr;
        let vm = make_vm();
        assert_eq!(
            vm.convert_to_f64(&Value::BigInt(RustBigInt::from(1i64))),
            Ok(1.0)
        );
        assert_eq!(
            vm.convert_to_f64(&Value::BigInt(RustBigInt::from(-5i64))),
            Ok(-5.0)
        );
        // Out-of-Float64-range magnitude rounds to +Inf, matching upstream
        // `Float64(big(10)^400)`.
        let huge = RustBigInt::from_str(&format!("1{}", "0".repeat(400))).unwrap();
        assert_eq!(vm.convert_to_f64(&Value::BigInt(huge)), Ok(f64::INFINITY));
    }

    // =========================================================================
    // UInt8 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u8_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u8(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::I8(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::I16(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u8_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u8(&Value::I64(256)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::U16(256)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::I64(1000)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u8_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u8(&Value::I64(0)), Ok(0));
        assert_eq!(vm.convert_to_u8(&Value::I64(255)), Ok(255));
        assert_eq!(vm.convert_to_u8(&Value::U8(42)), Ok(42));
        assert_eq!(vm.convert_to_u8(&Value::I64(128)), Ok(128));
    }

    // =========================================================================
    // UInt16 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u16_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u16(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u16(&Value::I8(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u16_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u16(&Value::I64(65536)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u16(&Value::U32(65536)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u16_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u16(&Value::I64(0)), Ok(0));
        assert_eq!(vm.convert_to_u16(&Value::I64(65535)), Ok(65535));
        assert_eq!(vm.convert_to_u16(&Value::U8(200)), Ok(200));
    }

    // =========================================================================
    // UInt32 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u32_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u32(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u32(&Value::I32(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u32_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u32(&Value::I64(4_294_967_296)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u32(&Value::U64(4_294_967_296)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u32_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u32(&Value::I64(0)), Ok(0));
        assert_eq!(
            vm.convert_to_u32(&Value::I64(4_294_967_295)),
            Ok(4_294_967_295)
        );
        assert_eq!(vm.convert_to_u32(&Value::U16(1000)), Ok(1000));
    }

    // =========================================================================
    // UInt64 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u64_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u64(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u64(&Value::I8(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u64_rejects_overflow() {
        let vm = make_vm();
        // U128 values larger than u64::MAX overflow
        assert!(matches!(
            vm.convert_to_u64(&Value::U128(u64::MAX as u128 + 1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u64_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u64(&Value::I64(0)), Ok(0));
        assert_eq!(
            vm.convert_to_u64(&Value::I64(i64::MAX)),
            Ok(i64::MAX as u64)
        );
        assert_eq!(vm.convert_to_u64(&Value::U8(255)), Ok(255));
    }

    // =========================================================================
    // UInt128 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u128_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u128(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u128(&Value::I128(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u128_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u128(&Value::I64(0)), Ok(0));
        assert_eq!(
            vm.convert_to_u128(&Value::U64(u64::MAX)),
            Ok(u64::MAX as u128)
        );
        assert_eq!(vm.convert_to_u128(&Value::U128(u128::MAX)), Ok(u128::MAX));
    }

    // =========================================================================
    // Int8 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i8_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_i8(&Value::I64(128)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i8(&Value::I64(-129)),
            Err(VmError::InexactError(_))
        ));
        // U8 values > 127 don't fit in i8
        assert!(matches!(
            vm.convert_to_i8(&Value::U8(128)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i8(&Value::U8(255)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i8_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i8(&Value::I64(127)), Ok(127));
        assert_eq!(vm.convert_to_i8(&Value::I64(-128)), Ok(-128));
        assert_eq!(vm.convert_to_i8(&Value::I8(0)), Ok(0));
        assert_eq!(vm.convert_to_i8(&Value::U8(0)), Ok(0));
        assert_eq!(vm.convert_to_i8(&Value::U8(127)), Ok(127));
    }

    // =========================================================================
    // Int16 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i16_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_i16(&Value::I64(32768)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i16(&Value::I64(-32769)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i16(&Value::U16(32768)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i16_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i16(&Value::I64(32767)), Ok(32767));
        assert_eq!(vm.convert_to_i16(&Value::I64(-32768)), Ok(-32768));
        assert_eq!(vm.convert_to_i16(&Value::I8(-128)), Ok(-128));
        assert_eq!(vm.convert_to_i16(&Value::U8(255)), Ok(255));
    }

    // =========================================================================
    // Int32 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i32_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_i32(&Value::I64(2_147_483_648)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i32(&Value::I64(-2_147_483_649)),
            Err(VmError::InexactError(_))
        ));
        // U32 values > i32::MAX overflow
        assert!(matches!(
            vm.convert_to_i32(&Value::U32(2_147_483_648)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i32_accepts_valid() {
        let vm = make_vm();
        assert_eq!(
            vm.convert_to_i32(&Value::I64(2_147_483_647)),
            Ok(2_147_483_647)
        );
        assert_eq!(
            vm.convert_to_i32(&Value::I64(-2_147_483_648)),
            Ok(-2_147_483_648)
        );
        assert_eq!(vm.convert_to_i32(&Value::U16(65535)), Ok(65535));
    }

    // =========================================================================
    // Int64 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i64_rejects_overflow() {
        let vm = make_vm();
        // U64 values > i64::MAX don't fit
        assert!(matches!(
            vm.convert_to_i64(&Value::U64(u64::MAX)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i64(&Value::U64(i64::MAX as u64 + 1)),
            Err(VmError::InexactError(_))
        ));
        // I128 values outside i64 range
        assert!(matches!(
            vm.convert_to_i64(&Value::I128(i64::MAX as i128 + 1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i64(&Value::I128(i64::MIN as i128 - 1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i64_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i64(&Value::I64(i64::MAX)), Ok(i64::MAX));
        assert_eq!(vm.convert_to_i64(&Value::I64(i64::MIN)), Ok(i64::MIN));
        assert_eq!(vm.convert_to_i64(&Value::U64(0)), Ok(0));
        assert_eq!(
            vm.convert_to_i64(&Value::U64(i64::MAX as u64)),
            Ok(i64::MAX)
        );
        assert_eq!(vm.convert_to_i64(&Value::U8(255)), Ok(255));
    }

    // =========================================================================
    // Issue #11214: Float64(2^63) saturated to typemax(Int64) instead of
    // raising InexactError; out-of-range BigInt raised TypeError instead of
    // InexactError. Both are boundary defects in the Int64 range check.
    // =========================================================================

    #[test]
    fn test_convert_to_i64_rejects_float_boundary_overflow_issue_11214() {
        let vm = make_vm();
        // Float64(2.0^63): the OLD round-trip check cast to i64 (saturating
        // to i64::MAX), then cast back to f64 -- i64::MAX rounds to exactly
        // 2.0^63, so the check falsely passed and silently saturated.
        assert!(matches!(
            vm.convert_to_i64(&Value::F64(9223372036854775808.0)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i64(&Value::F32(9223372036854775808.0_f32)),
            Err(VmError::InexactError(_))
        ));
        // The next f64 representable below -2^63 (ULP near this magnitude is
        // 2^11) also must raise, not saturate to i64::MIN.
        assert!(matches!(
            vm.convert_to_i64(&Value::F64(-9223372036854777856.0)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i64_accepts_float_boundary_valid_issue_11214() {
        let vm = make_vm();
        // -2^63 IS exactly typemin(Int64) and must remain valid (the lower
        // bound is inclusive; only the upper bound at 2^63 is out of range).
        assert_eq!(
            vm.convert_to_i64(&Value::F64(-9223372036854775808.0)),
            Ok(i64::MIN)
        );
        assert_eq!(
            vm.convert_to_i64(&Value::F32(-9223372036854775808.0_f32)),
            Ok(i64::MIN)
        );
    }

    #[test]
    fn test_convert_to_i64_rejects_bigint_overflow_as_inexact_issue_11214() {
        use std::str::FromStr;
        let vm = make_vm();
        // Out-of-range BigInt must raise InexactError (matching upstream),
        // not TypeError.
        let big = RustBigInt::from_str("9223372036854775808").unwrap();
        assert!(matches!(
            vm.convert_to_i64(&Value::BigInt(big)),
            Err(VmError::InexactError(_))
        ));
        let big_neg = RustBigInt::from_str("-9223372036854775809").unwrap();
        assert!(matches!(
            vm.convert_to_i64(&Value::BigInt(big_neg)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i64_accepts_bigint_boundary_valid_issue_11214() {
        use std::str::FromStr;
        let vm = make_vm();
        let big_min = RustBigInt::from_str("-9223372036854775808").unwrap();
        assert_eq!(vm.convert_to_i64(&Value::BigInt(big_min)), Ok(i64::MIN));
        let big_max = RustBigInt::from_str("9223372036854775807").unwrap();
        assert_eq!(vm.convert_to_i64(&Value::BigInt(big_max)), Ok(i64::MAX));
    }

    #[test]
    fn test_convert_to_i32_rejects_float32_boundary_overflow_issue_11214() {
        // Same defect shape, one width down: Float32(2^31) is exactly
        // representable in f32 but out of Int32 range.
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_i32(&Value::F32(2147483648.0_f32)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u64_rejects_float_boundary_overflow_issue_11214() {
        // UInt64(2^64) is exactly representable in f64 but out of UInt64
        // range.
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u64(&Value::F64(18446744073709551616.0)),
            Err(VmError::InexactError(_))
        ));
    }

    // =========================================================================
    // Int128 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i128_rejects_overflow() {
        let vm = make_vm();
        // U128 values > i128::MAX don't fit
        assert!(matches!(
            vm.convert_to_i128(&Value::U128(u128::MAX)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i128(&Value::U128(i128::MAX as u128 + 1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i128_accepts_valid() {
        let vm = make_vm();
        assert_eq!(
            vm.convert_to_i128(&Value::I64(i64::MIN)),
            Ok(i64::MIN as i128)
        );
        assert_eq!(
            vm.convert_to_i128(&Value::U64(u64::MAX)),
            Ok(u64::MAX as i128)
        );
        assert_eq!(
            vm.convert_to_i128(&Value::U128(i128::MAX as u128)),
            Ok(i128::MAX)
        );
        assert_eq!(vm.convert_to_i128(&Value::I128(i128::MIN)), Ok(i128::MIN));
    }
}
