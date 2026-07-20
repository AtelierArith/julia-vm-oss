//! Numeric type constructor builtin functions for the VM.
//!
//! BigInt, BigFloat, and numeric type conversions (Int8, Int16, etc.)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// SAFETY: i64→usize cast for BigFloat precision is guarded by `if n < 1`;
// i64→u8 cast for rounding mode is guarded by `if !(0..=5).contains(&n)`.
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::field_indices::{RATIONAL_DENOMINATOR_FIELD_INDEX, RATIONAL_NUMERATOR_FIELD_INDEX};
use super::stack_ops::StackOps;
use super::value::{
    get_bigfloat_precision, get_bigfloat_rounding_mode, set_bigfloat_precision,
    set_bigfloat_rounding_mode, BigFloatRoundingMode, RustBigFloat, RustBigInt, StructInstance,
    Value,
};
use super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute numeric type constructor builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a numeric builtin.
    pub(super) fn execute_builtin_numeric(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        if matches!(
            builtin,
            BuiltinId::BigInt
                | BuiltinId::BigFloat
                | BuiltinId::Int8
                | BuiltinId::Int16
                | BuiltinId::Int32
                | BuiltinId::Int64
                | BuiltinId::Int128
                | BuiltinId::UInt8
                | BuiltinId::UInt16
                | BuiltinId::UInt32
                | BuiltinId::UInt64
                | BuiltinId::UInt128
                | BuiltinId::Float16
                | BuiltinId::Float32
                | BuiltinId::Float64
        ) && argc != 1
        {
            return Err(VmError::MethodError(format!(
                "numeric type constructor requires exactly 1 argument, got {}",
                argc
            )));
        }

        match builtin {
            // =========================================================================
            // BigInt Operations
            // =========================================================================
            BuiltinId::BigInt => {
                // BigInt(x) - convert to arbitrary precision integer
                let val = self.stack.pop_value()?;
                let bigint = match val {
                    Value::Bool(n) => RustBigInt::from(if n { 1u8 } else { 0u8 }),
                    Value::I8(n) => RustBigInt::from(n),
                    Value::I16(n) => RustBigInt::from(n),
                    Value::I32(n) => RustBigInt::from(n),
                    Value::I64(n) => RustBigInt::from(n),
                    Value::I128(n) => RustBigInt::from(n),
                    Value::U8(n) => RustBigInt::from(n),
                    Value::U16(n) => RustBigInt::from(n),
                    Value::U32(n) => RustBigInt::from(n),
                    Value::U64(n) => RustBigInt::from(n),
                    Value::U128(n) => RustBigInt::from(n),
                    Value::F64(n) => RustBigInt::from(n as i64),
                    Value::Str(s) => s.parse::<RustBigInt>().map_err(|_| {
                        VmError::TypeError(format!("Cannot parse '{}' as BigInt", s))
                    })?,
                    Value::BigInt(n) => n,
                    Value::BigFloat(ref n) => bigfloat_to_bigint_exact(n)?,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "Cannot convert {:?} to BigInt",
                            other
                        )));
                    }
                };
                self.stack.push(Value::BigInt(bigint));
            }

            // =========================================================================
            // BigFloat Operations
            // =========================================================================
            BuiltinId::BigFloat => {
                // BigFloat(x) - convert to arbitrary precision float
                let val = self.stack.pop_value()?;
                let precision = get_bigfloat_precision();
                let parse_bigfloat_decimal = |s: &str| -> Result<RustBigFloat, VmError> {
                    let mut consts = astro_float::Consts::new().map_err(|e| {
                        VmError::InternalError(format!(
                            "Failed to initialize BigFloat constants: {}",
                            e
                        ))
                    })?;
                    let bf = RustBigFloat::parse(
                        s,
                        astro_float::Radix::Dec,
                        precision,
                        BigFloatRoundingMode::ToEven,
                        &mut consts,
                    );
                    if bf.is_nan() && !s.to_lowercase().contains("nan") {
                        return Err(VmError::TypeError(format!(
                            "Cannot parse '{}' as BigFloat",
                            s
                        )));
                    }
                    Ok(bf)
                };
                let bigfloat = match val {
                    Value::I8(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::I16(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::I32(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::I64(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::I128(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::U8(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::U16(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::U32(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::U64(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::U128(n) => parse_bigfloat_decimal(&n.to_string())?,
                    Value::Bool(n) => RustBigFloat::from_f64(if n { 1.0 } else { 0.0 }, precision),
                    Value::F16(n) => RustBigFloat::from_f64(n.to_f64(), precision),
                    Value::F32(n) => RustBigFloat::from_f64(n as f64, precision),
                    Value::F64(n) => RustBigFloat::from_f64(n, precision),
                    Value::Str(s) => parse_bigfloat_decimal(&s)?,
                    Value::BigFloat(n) => n,
                    Value::BigInt(n) => {
                        // Convert BigInt to BigFloat via string
                        let s = n.to_string();
                        parse_bigfloat_decimal(&s)?
                    }
                    // Rational{T}: BigFloat(num) / BigFloat(den) at the active
                    // precision, mirroring upstream base/mpfr.jl
                    // `BigFloat(x::Rational)` (Issue #9288). `BigFloat(x)` is
                    // compiled to a direct `CallBuiltin(BigFloat, 1)`, so a
                    // Julia-level `BigFloat(::Rational)` method would never be
                    // dispatched — the Rational must be handled here so the
                    // generic `+ - * / ==` promote-fallback can widen it. An
                    // Irrational singleton falls back to its decimal expansion.
                    Value::Struct(s) => {
                        if let Some(bf) =
                            rational_struct_to_bigfloat(&s, precision, &parse_bigfloat_decimal)?
                        {
                            bf
                        } else if let Some(decimal) = s.irrational_decimal() {
                            parse_bigfloat_decimal(decimal)?
                        } else {
                            return Err(VmError::TypeError(format!(
                                "Cannot convert {:?} to BigFloat",
                                Value::Struct(s)
                            )));
                        }
                    }
                    Value::StructRef(idx) => {
                        let s = self.struct_heap.get(idx).ok_or_else(|| {
                            VmError::TypeError(format!(
                                "Cannot convert StructRef({}) to BigFloat",
                                idx
                            ))
                        })?;
                        if let Some(bf) =
                            rational_struct_to_bigfloat(s, precision, &parse_bigfloat_decimal)?
                        {
                            bf
                        } else if let Some(decimal) = s.irrational_decimal() {
                            parse_bigfloat_decimal(decimal)?
                        } else {
                            return Err(VmError::TypeError(format!(
                                "Cannot convert StructRef({}) to BigFloat",
                                idx
                            )));
                        }
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "Cannot convert {:?} to BigFloat",
                            other
                        )));
                    }
                };
                self.stack.push(Value::BigFloat(bigfloat));
            }

            BuiltinId::BigFloatPrecision => {
                // _bigfloat_precision(x) - get the precision of a BigFloat value
                let val = self.stack.pop_value()?;
                match val {
                    Value::BigFloat(bf) => {
                        let prec = bf.allocation_precision();
                        self.stack.push(Value::I64(prec as i64));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_precision requires BigFloat, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            BuiltinId::BigFloatDefaultPrecision => {
                // _bigfloat_default_precision() - get the default precision for new BigFloats
                let prec = get_bigfloat_precision();
                self.stack.push(Value::I64(prec as i64));
            }

            BuiltinId::SetBigFloatDefaultPrecision => {
                // _set_bigfloat_default_precision!(n) - set the default precision
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(n) => {
                        if n < 1 {
                            return Err(VmError::DomainError(
                                "precision must be at least 1".to_string(),
                            ));
                        }
                        let old_prec = set_bigfloat_precision(n as usize);
                        self.stack.push(Value::I64(old_prec as i64));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_set_bigfloat_default_precision! requires Int64, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            BuiltinId::BigFloatRounding => {
                // _bigfloat_rounding() - get the current rounding mode
                // Returns: 0=ToEven (RoundNearest), 1=ToZero, 2=Up, 3=Down, 4=FromZero, 5=ToOdd
                let mode = get_bigfloat_rounding_mode();
                self.stack.push(Value::I64(mode as i64));
            }

            BuiltinId::SetBigFloatRounding => {
                // _set_bigfloat_rounding!(mode) - set the rounding mode
                // mode: 0=ToEven (RoundNearest), 1=ToZero, 2=Up, 3=Down, 4=FromZero, 5=ToOdd
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(n) => {
                        if !(0..=5).contains(&n) {
                            return Err(VmError::DomainError(format!(
                                "invalid rounding mode: {}, must be 0-5",
                                n
                            )));
                        }
                        let old_mode = set_bigfloat_rounding_mode(n as u8);
                        self.stack.push(Value::I64(old_mode as i64));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_set_bigfloat_rounding! requires Int64, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            // =========================================================================
            // Subnormal (Denormal) Float Control
            // =========================================================================
            BuiltinId::GetZeroSubnormals => {
                // get_zero_subnormals() - check if subnormals are flushed to zero
                // In SubsetJuliaVM, we always follow IEEE standard (subnormals preserved)
                // This returns false since we don't support flushing subnormals to zero
                self.stack.push(Value::Bool(false));
            }

            BuiltinId::SetZeroSubnormals => {
                // set_zero_subnormals(yes::Bool) - enable/disable flushing subnormals to zero
                // Returns true if successful, false if hardware doesn't support it
                // In SubsetJuliaVM, we cannot change the subnormal handling mode,
                // so we return false when yes=true (can't enable), true when yes=false (already disabled)
                let val = self.stack.pop_value()?;
                match val {
                    Value::Bool(yes) => {
                        // If yes=true, we can't enable flush-to-zero, so return false
                        // If yes=false, subnormals are already preserved, so return true
                        self.stack.push(Value::Bool(!yes));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "set_zero_subnormals requires Bool, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            // =========================================================================
            // Numeric Type Constructors
            // =========================================================================
            BuiltinId::Int8 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i8(&val)?;
                self.stack.push(Value::I8(result));
            }
            BuiltinId::Int16 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i16(&val)?;
                self.stack.push(Value::I16(result));
            }
            BuiltinId::Int32 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i32(&val)?;
                self.stack.push(Value::I32(result));
            }
            BuiltinId::Int64 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i64(&val)?;
                self.stack.push(Value::I64(result));
            }
            BuiltinId::Int128 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i128(&val)?;
                self.stack.push(Value::I128(result));
            }
            BuiltinId::UInt8 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u8(&val)?;
                self.stack.push(Value::U8(result));
            }
            BuiltinId::UInt16 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u16(&val)?;
                self.stack.push(Value::U16(result));
            }
            BuiltinId::UInt32 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u32(&val)?;
                self.stack.push(Value::U32(result));
            }
            BuiltinId::UInt64 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u64(&val)?;
                self.stack.push(Value::U64(result));
            }
            BuiltinId::UInt128 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u128(&val)?;
                self.stack.push(Value::U128(result));
            }
            BuiltinId::Float16 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_f16(&val)?;
                self.stack.push(Value::F16(result));
            }
            BuiltinId::Float32 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_f32(&val)?;
                self.stack.push(Value::F32(result));
            }
            BuiltinId::Float64 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_f64(&val)?;
                self.stack.push(Value::F64(result));
            }

            BuiltinId::BigFloatNextfloat => {
                // _bigfloat_nextfloat(x::BigFloat, up::Bool) -> BigFloat
                // One-ULP step at the value's own precision (Issue #9280): the
                // MPFR mpfr_nextabove / mpfr_nextbelow behaviour, implemented over
                // the astro_float backend (sjulia's BigFloat is not MPFR-backed).
                // Args are pushed x, up (up popped first).
                let up = match self.stack.pop_value()? {
                    Value::Bool(b) => b,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_nextfloat direction must be Bool, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                let x = match self.stack.pop_value()? {
                    Value::BigFloat(bf) => bf,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_nextfloat requires BigFloat, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                self.stack.push(Value::BigFloat(bigfloat_step(&x, up)));
            }

            BuiltinId::BigFloatGetExp => {
                // _bigfloat_get_exp(x::BigFloat) -> Int64
                // Base-2 exponent E of a finite nonzero BigFloat, where
                // x = m·2^E with m ∈ [0.5, 1) — MPFR's mpfr_get_exp convention,
                // read from astro_float's exponent field (Issue #9286). The
                // Julia caller guards zero/Inf/NaN, so a non-finite/zero argument
                // (which has no meaningful exponent) yields 0.
                let x = match self.stack.pop_value()? {
                    Value::BigFloat(bf) => bf,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_get_exp requires BigFloat, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                let e = x.exponent().unwrap_or(0) as i64;
                self.stack.push(Value::I64(e));
            }

            BuiltinId::BigFloatScale2 => {
                // _bigfloat_scale2(x::BigFloat, n::Int64) -> BigFloat
                // x · 2^n computed exactly by shifting astro_float's exponent
                // field (no rounding), used to extract the frexp mantissa /
                // significand of a BigFloat (Issue #9286). Args pushed x, n
                // (n popped first).
                let n = match self.stack.pop_value()? {
                    Value::I64(n) => n,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_scale2 shift must be Int64, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                let x = match self.stack.pop_value()? {
                    Value::BigFloat(bf) => bf,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_scale2 requires BigFloat, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                self.stack.push(Value::BigFloat(bigfloat_scale2(&x, n)));
            }

            BuiltinId::BigFloatSignbit => {
                // _bigfloat_signbit(x::BigFloat) -> Bool
                // The sign bit read from astro_float's sign field (Issue #9450):
                // unlike the generic `signbit(x) = x < 0`, this observes a
                // negative zero, so abs/copysign/mod sign BigFloat zeros like
                // MPFR/Julia. `is_negative()` reports false for NaN, matching
                // Julia's `signbit(big(NaN)) == false`.
                let x = match self.stack.pop_value()? {
                    Value::BigFloat(bf) => bf,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_signbit requires BigFloat, got {:?}",
                            other.value_type()
                        )));
                    }
                };
                self.stack.push(Value::Bool(x.is_negative()));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}

/// Convert a `Rational{T}` struct value to a `BigFloat` as
/// `BigFloat(numerator) / BigFloat(denominator)` at the given precision,
/// mirroring upstream `base/mpfr.jl` `BigFloat(x::Rational)` (Issue #9288).
///
/// Returns `Ok(None)` when `s` is not a Rational, so the caller can fall back to
/// the Irrational decimal-expansion path. Numerator/denominator fields are
/// converted exactly through their decimal string, so `Rational{BigInt}` (of
/// arbitrary magnitude) is handled as faithfully as `Rational{Int64}`.
fn rational_struct_to_bigfloat<F>(
    s: &StructInstance,
    precision: usize,
    parse_bigfloat_decimal: &F,
) -> Result<Option<RustBigFloat>, VmError>
where
    F: Fn(&str) -> Result<RustBigFloat, VmError>,
{
    if !s.is_rational() {
        return Ok(None);
    }
    let num_v = s
        .values
        .get(RATIONAL_NUMERATOR_FIELD_INDEX)
        .ok_or_else(|| {
            VmError::TypeError("BigFloat: Rational must have a numerator field".to_string())
        })?;
    let den_v = s
        .values
        .get(RATIONAL_DENOMINATOR_FIELD_INDEX)
        .ok_or_else(|| {
            VmError::TypeError("BigFloat: Rational must have a denominator field".to_string())
        })?;
    let num = parse_bigfloat_decimal(&rational_field_decimal(num_v)?)?;
    let den = parse_bigfloat_decimal(&rational_field_decimal(den_v)?)?;
    Ok(Some(num.div(&den, precision, BigFloatRoundingMode::ToEven)))
}

/// Exact decimal string for an integer-valued `Rational` field, covering the
/// integer `Value` representations the VM stores for `Rational{Int*/UInt*/Bool}`
/// and `Rational{BigInt}` (Issue #9288).
fn rational_field_decimal(v: &Value) -> Result<String, VmError> {
    match v {
        Value::Bool(b) => Ok(if *b { "1" } else { "0" }.to_string()),
        Value::I8(n) => Ok(n.to_string()),
        Value::I16(n) => Ok(n.to_string()),
        Value::I32(n) => Ok(n.to_string()),
        Value::I64(n) => Ok(n.to_string()),
        Value::I128(n) => Ok(n.to_string()),
        Value::U8(n) => Ok(n.to_string()),
        Value::U16(n) => Ok(n.to_string()),
        Value::U32(n) => Ok(n.to_string()),
        Value::U64(n) => Ok(n.to_string()),
        Value::U128(n) => Ok(n.to_string()),
        Value::BigInt(n) => Ok(n.to_string()),
        other => Err(VmError::TypeError(format!(
            "BigFloat: Rational field must be an integer, got {:?}",
            other
        ))),
    }
}

/// One-ULP step of a `BigFloat`, mirroring MPFR's `mpfr_nextabove`
/// (`up == true`) / `mpfr_nextbelow` (`up == false`) at the value's own
/// precision (Issue #9280).
///
/// sjulia's `BigFloat` is backed by `astro_float`, not MPFR, so the step is
/// computed directly on `astro_float`'s representation: a finite value is
/// `mantissa · 2^e` with the mantissa normalized to `[0.5, 1)` and
/// `p = mantissa_max_bit_len` bits, so the grid spacing (ULP) inside its binade
/// is `2^(e - p)`, and `2^(e - 1 - p)` in the binade just below (used only when
/// a magnitude *decrease* crosses an exact power-of-two boundary). Zero steps to
/// the smallest subnormal, `±Inf` saturate to `±floatmax`, and `NaN` is returned
/// unchanged — matching upstream `Base.nextfloat` / `prevfloat` for `BigFloat`.
fn bigfloat_step(x: &RustBigFloat, up: bool) -> RustBigFloat {
    use astro_float::BigFloat;
    let inner: &BigFloat = x;
    let rm = BigFloatRoundingMode::ToEven;
    let default_p = x.allocation_precision();

    if inner.is_nan() {
        return x.clone();
    }
    if inner.is_inf_pos() {
        // nextfloat(+Inf) = +Inf ; prevfloat(+Inf) = floatmax
        return RustBigFloat::new_with_precision(
            if up {
                astro_float::INF_POS
            } else {
                BigFloat::max_value(default_p)
            },
            default_p,
        );
    }
    if inner.is_inf_neg() {
        // nextfloat(-Inf) = -floatmax ; prevfloat(-Inf) = -Inf
        return RustBigFloat::new_with_precision(
            if up {
                BigFloat::min_value(default_p)
            } else {
                astro_float::INF_NEG
            },
            default_p,
        );
    }
    if inner.is_zero() {
        // nextfloat(0) = smallest positive ; prevfloat(0) = -(smallest positive)
        let smallest = BigFloat::min_positive(default_p);
        return RustBigFloat::new_with_precision(
            if up { smallest } else { smallest.neg() },
            default_p,
        );
    }

    // Finite, nonzero. Full mantissa width sets the grid spacing.
    let p = inner.mantissa_max_bit_len().unwrap_or(default_p);

    // Construct 2^k exactly at precision p: value 1.0 is `0.5 · 2^1` in
    // astro_float, so setting its exponent to `k + 1` yields `0.5 · 2^(k+1) = 2^k`.
    let two_pow = |k: i32| -> BigFloat {
        let mut v = BigFloat::from_f64(1.0, p);
        v.set_exponent(k.saturating_add(1));
        v
    };

    // Subnormal values share a single ULP across the whole subnormal range
    // (= the smallest positive value), so no binade/boundary logic is needed.
    if inner.is_subnormal() {
        let ulp = BigFloat::min_positive(p);
        return RustBigFloat::new_with_precision(
            if up {
                inner.add(&ulp, p, rm)
            } else {
                inner.sub(&ulp, p, rm)
            },
            p,
        );
    }

    let e = match inner.exponent() {
        Some(e) => e,
        None => return x.clone(),
    };
    let neg = inner.is_negative();
    let p_i32 = p as i32;
    let ulp_hi = two_pow(e - p_i32); // spacing inside x's binade
    let ulp_lo = two_pow(e - 1 - p_i32); // spacing one binade below

    // `x` sits at an exact power of two (mantissa == 0.5) iff |x| == 2^(e-1);
    // only then does a magnitude decrease cross into the smaller-ULP binade.
    let on_pow2_boundary = inner.abs_cmp(&two_pow(e - 1)) == Some(0);
    let ulp_down = if on_pow2_boundary { &ulp_lo } else { &ulp_hi };

    // `up` (nextfloat) moves toward +∞; `!up` (prevfloat) toward -∞. A magnitude
    // *decrease* occurs for nextfloat of a negative value and prevfloat of a
    // positive value — the only places the boundary ULP applies. A magnitude
    // *increase* always uses the current binade's ULP.
    let stepped = match (up, neg) {
        (true, false) => inner.add(&ulp_hi, p, rm), // x > 0, increase magnitude
        (true, true) => inner.add(ulp_down, p, rm), // x < 0, decrease magnitude
        (false, false) => inner.sub(ulp_down, p, rm), // x > 0, decrease magnitude
        (false, true) => inner.sub(&ulp_hi, p, rm), // x < 0, increase magnitude
    };
    RustBigFloat::new_with_precision(stepped, p)
}

/// Multiply a `BigFloat` by `2^n` exactly by shifting its astro_float exponent
/// field, with no rounding or precision loss (Issue #9286). Backs the BigFloat
/// `frexp`/`significand` methods, which normalize the mantissa via a shift of
/// `n = -E` (mantissa into `[0.5, 1)`) or `n = 1 - E` (significand into
/// `[1, 2)`), where `E` is the value's own exponent — so the resulting exponent
/// is a small constant and never overflows. `±0`, `±Inf`, and `NaN` have no
/// exponent field and are returned unchanged.
fn bigfloat_scale2(x: &RustBigFloat, n: i64) -> RustBigFloat {
    use astro_float::BigFloat;
    let mut v: BigFloat = (**x).clone();
    if let Some(e) = v.exponent() {
        let shifted = (e as i64).saturating_add(n);
        v.set_exponent(shifted.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    }
    RustBigFloat::new_with_precision(v, x.allocation_precision())
}

/// Convert an integer-valued `BigFloat` to a `BigInt` exactly, mirroring
/// upstream `base/mpfr.jl` `BigInt(x::BigFloat)`: non-finite or fractional
/// values throw `InexactError` (Issue #9424).
///
/// sjulia's `BigFloat` is backed by `astro_float`, whose finite values are
/// `±M · 2^(e − total_bits)` where `M` is the little-endian mantissa word
/// slice read as an unsigned integer and `total_bits` is the full bit width
/// of that slice (the mantissa is left-normalized, so this also covers
/// values whose significant bits are fewer than the storage width). The
/// conversion therefore shifts `M` by `e − total_bits`; a right shift that
/// would drop a set bit means the value has a fractional part.
fn bigfloat_to_bigint_exact(x: &RustBigFloat) -> Result<RustBigInt, VmError> {
    use num_bigint::{BigInt as NumBigInt, BigUint};

    let inner: &astro_float::BigFloat = x;
    let inexact = || VmError::InexactError(format!("BigInt({})", super::format_bigfloat_julia(x)));
    if inner.is_zero() {
        return Ok(RustBigInt::from(0));
    }
    // NaN / ±Inf carry no mantissa parts (`as_raw_parts()` returns None).
    let (words, _significant_bits, sign, e, _inexact_flag) =
        inner.as_raw_parts().ok_or_else(inexact)?;
    let total_bits = words.len() as i64 * astro_float::WORD_BIT_SIZE as i64;
    let mut mag = BigUint::from(0u8);
    for w in words.iter().rev() {
        mag = (mag << astro_float::WORD_BIT_SIZE) | BigUint::from(*w);
    }
    let shift = i64::from(e) - total_bits;
    if shift >= 0 {
        // `e` is an i32, so the shift fits u64 losslessly.
        mag <<= shift as u64;
    } else {
        let s = (-shift) as u64;
        // All shifted-out low bits must be zero, or the value is fractional.
        // (`mag` is nonzero here, so `trailing_zeros()` is always Some.)
        if mag.trailing_zeros().unwrap_or(0) < s {
            return Err(inexact());
        }
        mag >>= s;
    }
    let num_sign = if sign == astro_float::Sign::Neg {
        num_bigint::Sign::Minus
    } else {
        num_bigint::Sign::Plus
    };
    Ok(RustBigInt::new(NumBigInt::from_biguint(num_sign, mag)))
}
