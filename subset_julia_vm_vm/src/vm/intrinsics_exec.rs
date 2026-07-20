//! Intrinsic instruction execution for the VM.
//!
//! Intrinsics are CPU-level operations (Layer 1 in the VM hierarchy).
//! They map directly to LLVM intrinsics or simple CPU operations.

// SAFETY: i64→u32 casts for bit-shift amounts (ShlInt/LshrInt/AshrInt) are
// standard intrinsic semantics; i64→u32 for BigInt pow is guarded by `if exp < 0`.
#![allow(clippy::cast_sign_loss)]

use crate::intrinsics::Intrinsic;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{
    get_bigfloat_precision, get_bigfloat_rounding, BigFloatConsts, BigFloatRoundingMode,
    RustBigFloat, RustBigInt, StructInstance, Value,
};
use super::Vm;

use num_traits::{ToPrimitive, Zero};

/// Apply a unary float operation with access to heap-backed numeric structs.
///
/// F16/F32 inputs preserve their primitive width; other numeric values produce
/// Float64. Heap-backed Rational/Irrational values also convert to Float64,
/// matching the existing VM numeric builtin path (Issue #6266).
pub(crate) fn apply_unary_float_op_with_heap(
    val: Value,
    struct_heap: &[StructInstance],
    op: fn(f64) -> f64,
) -> Result<Value, VmError> {
    match val {
        Value::F16(a) => Ok(Value::F16(half::f16::from_f64(op(a.to_f64())))),
        Value::F32(a) => Ok(Value::F32(op(a as f64) as f32)),
        other => {
            let a = value_to_f64_with_heap(&other, struct_heap)?;
            Ok(Value::F64(op(a)))
        }
    }
}

/// Apply a unary rounding op (floor / ceil / trunc / round), routing
/// `BigFloat` through `astro_float`'s native rounding so arbitrary precision is
/// preserved instead of erroring in `value_to_f64` (Issue #6801). All other
/// numeric types keep the existing f64 path (which preserves F16/F32 width).
pub(crate) fn apply_unary_rounding_op_with_heap(
    val: Value,
    struct_heap: &[StructInstance],
    f64_op: fn(f64) -> f64,
    bf_op: fn(&RustBigFloat) -> RustBigFloat,
) -> Result<Value, VmError> {
    if let Value::BigFloat(a) = &val {
        return Ok(Value::BigFloat(bf_op(a)));
    }
    apply_unary_float_op_with_heap(val, struct_heap, f64_op)
}

/// Julia-facing type name of a numeric fast-path operand for error messages.
/// Error path only: resolves heap-backed struct names so a user struct reports
/// its actual type instead of the internal `Struct`/`StructRef` placeholder.
#[cold]
fn fastpath_operand_type_name(val: &Value, struct_heap: &[StructInstance]) -> String {
    match val {
        Value::Struct(s) if !s.struct_name.is_empty() => s.struct_name.to_string(),
        Value::StructRef(idx) => match struct_heap.get(*idx) {
            Some(s) if !s.struct_name.is_empty() => s.struct_name.to_string(),
            _ => super::util::value_type_name(val).to_string(),
        },
        other => super::util::value_type_name(other).to_string(),
    }
}

/// A compiled numeric fast path (`SqrtF64`, `CallBuiltin(Round)`, …) whose
/// operand fails the numeric check is a dispatch miss in upstream Julia:
/// `sqrt("a")` raises `MethodError`, not an internal conversion `TypeError`
/// (Issue #10481). Build the `MethodError` upstream raises, naming the
/// function the fast path stands for. `#[cold]`: error path only — the fast
/// path's success case never reaches this.
#[cold]
pub(crate) fn numeric_fastpath_method_error(
    func: &str,
    val: &Value,
    struct_heap: &[StructInstance],
) -> VmError {
    VmError::MethodError(format!(
        "no method matching {}(::{})",
        func,
        fastpath_operand_type_name(val, struct_heap)
    ))
}

/// Remap only the operand-type failure (`TypeError`) of a numeric fast path to
/// the upstream-faithful `MethodError`; every other error (DomainError, host
/// errors) passes through unchanged (Issue #10481).
pub(crate) fn remap_numeric_fastpath_error(
    func: &str,
    err: VmError,
    val: &Value,
    struct_heap: &[StructInstance],
) -> VmError {
    match err {
        VmError::TypeError(_) => numeric_fastpath_method_error(func, val, struct_heap),
        other => other,
    }
}

/// [`apply_unary_float_op_with_heap`] for a fast path with a known function
/// identity: a non-numeric operand produces the `MethodError` upstream raises
/// for `func` instead of the internal conversion `TypeError` (Issue #10481).
pub(crate) fn apply_unary_float_op_named_with_heap(
    func: &str,
    val: Value,
    struct_heap: &[StructInstance],
    op: fn(f64) -> f64,
) -> Result<Value, VmError> {
    match val {
        Value::F16(_) | Value::F32(_) => apply_unary_float_op_with_heap(val, struct_heap, op),
        other => match value_to_f64_with_heap(&other, struct_heap) {
            Ok(a) => Ok(Value::F64(op(a))),
            Err(err) => Err(remap_numeric_fastpath_error(func, err, &other, struct_heap)),
        },
    }
}

/// [`apply_unary_rounding_op_with_heap`] for a fast path with a known function
/// identity (floor / ceil / round / trunc): a non-numeric operand produces the
/// `MethodError` upstream raises for `func` (Issue #10481).
pub(crate) fn apply_unary_rounding_op_named_with_heap(
    func: &str,
    val: Value,
    struct_heap: &[StructInstance],
    f64_op: fn(f64) -> f64,
    bf_op: fn(&RustBigFloat) -> RustBigFloat,
) -> Result<Value, VmError> {
    if let Value::BigFloat(a) = &val {
        return Ok(Value::BigFloat(bf_op(a)));
    }
    apply_unary_float_op_named_with_heap(func, val, struct_heap, f64_op)
}

/// Convert a Value to f64 for float intrinsics.
/// Covers all numeric types including small integers and unsigned types (Issue #2284).
pub(crate) fn value_to_f64(val: &Value) -> Result<f64, VmError> {
    match val {
        Value::F64(v) => Ok(*v),
        Value::F32(v) => Ok(*v as f64),
        Value::F16(v) => Ok(v.to_f64()),
        Value::I64(v) => Ok(*v as f64),
        Value::I128(v) => Ok(*v as f64),
        Value::I32(v) => Ok(*v as f64),
        Value::I16(v) => Ok(*v as f64),
        Value::I8(v) => Ok(*v as f64),
        Value::U64(v) => Ok(*v as f64),
        Value::U128(v) => Ok(*v as f64),
        Value::U32(v) => Ok(*v as f64),
        Value::U16(v) => Ok(*v as f64),
        Value::U8(v) => Ok(*v as f64),
        Value::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        _ => Err(VmError::TypeError(format!(
            "expected numeric value, got {:?}",
            val
        ))),
    }
}

fn bigfloat_from_integer_decimal_exact(
    s: &str,
    min_precision: usize,
) -> Result<RustBigFloat, VmError> {
    let mut consts = BigFloatConsts::new().map_err(|e| {
        VmError::InternalError(format!("Failed to initialize BigFloat constants: {}", e))
    })?;
    Ok(RustBigFloat::parse_integer_exact_decimal(
        s,
        min_precision,
        &mut consts,
    ))
}

fn value_to_bigfloat_exact(value: &Value) -> Result<RustBigFloat, VmError> {
    let p = get_bigfloat_precision();
    match value {
        Value::BigFloat(v) => Ok(v.clone()),
        Value::F64(v) => Ok(RustBigFloat::from_f64(*v, p)),
        Value::F32(v) => Ok(RustBigFloat::from_f64(*v as f64, p)),
        Value::F16(v) => Ok(RustBigFloat::from_f64(v.to_f64(), p)),
        Value::Bool(v) => Ok(RustBigFloat::from_f64(if *v { 1.0 } else { 0.0 }, p)),
        Value::I8(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::I16(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::I32(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::I64(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::I128(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::U8(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::U16(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::U32(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::U64(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::U128(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        Value::BigInt(v) => bigfloat_from_integer_decimal_exact(&v.to_string(), p),
        _ => Err(VmError::TypeError(format!(
            "expected BigFloat-compatible numeric value, got {:?}",
            value
        ))),
    }
}

fn exact_integer_zero_value(value: &Value) -> bool {
    match value {
        Value::Bool(v) => !*v,
        Value::I8(v) => *v == 0,
        Value::I16(v) => *v == 0,
        Value::I32(v) => *v == 0,
        Value::I64(v) => *v == 0,
        Value::I128(v) => *v == 0,
        Value::U8(v) => *v == 0,
        Value::U16(v) => *v == 0,
        Value::U32(v) => *v == 0,
        Value::U64(v) => *v == 0,
        Value::U128(v) => *v == 0,
        Value::BigInt(v) => v.is_zero(),
        _ => false,
    }
}

fn exact_wide_integer_zero_value(value: &Value) -> bool {
    matches!(value, Value::I128(0) | Value::U128(0))
}

fn exact_bool_zero_value(value: &Value) -> bool {
    matches!(value, Value::Bool(false))
}

fn negative_mpfr_integer_decimal(value: &Value) -> Option<String> {
    match value {
        Value::I8(v) if *v < 0 => Some(v.to_string()),
        Value::I16(v) if *v < 0 => Some(v.to_string()),
        Value::I32(v) if *v < 0 => Some(v.to_string()),
        Value::I64(v) if *v < 0 => Some(v.to_string()),
        Value::BigInt(v) if v.sign() == num_bigint::Sign::Minus => Some(v.to_string()),
        _ => None,
    }
}

fn signed_bigfloat_zero(negative: bool) -> RustBigFloat {
    RustBigFloat::from_f64(if negative { -0.0 } else { 0.0 }, get_bigfloat_precision())
}

fn bigfloat_zero_plus_exact_zero(lhs: &Value, rhs: &Value, subtract: bool) -> Option<RustBigFloat> {
    match (lhs, rhs, subtract) {
        (Value::BigFloat(x), y, false) if x.is_zero() && exact_integer_zero_value(y) => {
            if exact_wide_integer_zero_value(y) {
                Some(signed_bigfloat_zero(false))
            } else {
                Some(x.with_precision(get_bigfloat_precision(), get_bigfloat_rounding()))
            }
        }
        (x, Value::BigFloat(y), false) if exact_integer_zero_value(x) && y.is_zero() => {
            if exact_wide_integer_zero_value(x) {
                Some(signed_bigfloat_zero(false))
            } else {
                Some(y.with_precision(get_bigfloat_precision(), get_bigfloat_rounding()))
            }
        }
        (Value::BigFloat(x), y, true) if x.is_zero() && exact_integer_zero_value(y) => {
            Some(x.with_precision(get_bigfloat_precision(), get_bigfloat_rounding()))
        }
        (x, Value::BigFloat(y), true) if exact_integer_zero_value(x) && y.is_zero() => {
            if exact_bool_zero_value(x) || exact_wide_integer_zero_value(x) {
                Some(signed_bigfloat_zero(false))
            } else {
                Some(
                    y.neg()
                        .with_precision(get_bigfloat_precision(), get_bigfloat_rounding()),
                )
            }
        }
        (x, Value::BigFloat(y), true) => {
            let decimal = negative_mpfr_integer_decimal(x)?;
            let x_bf =
                bigfloat_from_integer_decimal_exact(&decimal, get_bigfloat_precision()).ok()?;
            matches!(x_bf.cmp(y), Some(0)).then(|| signed_bigfloat_zero(true))
        }
        _ => None,
    }
}

fn numeric_value_is_negative(value: &Value) -> Result<bool, VmError> {
    match value {
        Value::BigFloat(v) => Ok(v.is_negative()),
        Value::F64(v) => Ok(v.is_sign_negative()),
        Value::F32(v) => Ok(v.is_sign_negative()),
        Value::F16(v) => Ok(v.to_f64().is_sign_negative()),
        Value::I8(v) => Ok(*v < 0),
        Value::I16(v) => Ok(*v < 0),
        Value::I32(v) => Ok(*v < 0),
        Value::I64(v) => Ok(*v < 0),
        Value::I128(v) => Ok(*v < 0),
        Value::U8(_) | Value::U16(_) | Value::U32(_) | Value::U64(_) | Value::U128(_) => Ok(false),
        Value::Bool(_) => Ok(false),
        Value::BigInt(v) => Ok(v.sign() == num_bigint::Sign::Minus),
        _ => Err(VmError::TypeError(format!(
            "expected numeric value for BigFloat strong zero, got {:?}",
            value
        ))),
    }
}

fn bigfloat_bool_strong_zero_mul(
    lhs: &Value,
    rhs: &Value,
) -> Result<Option<RustBigFloat>, VmError> {
    if matches!(lhs, Value::Bool(false)) {
        return Ok(Some(signed_bigfloat_zero(numeric_value_is_negative(rhs)?)));
    }
    if matches!(rhs, Value::Bool(false)) {
        return Ok(Some(signed_bigfloat_zero(numeric_value_is_negative(lhs)?)));
    }
    Ok(None)
}

/// Dekker's TwoProduct: returns `(hi, lo)` such that `hi == x*y` (rounded) and
/// `hi + lo` is the exact product. Mirrors upstream Julia's `Base.Math.two_mul`
/// (`base/twiceprecision.jl`), which uses `fma` when available.
#[inline]
fn two_mul_f64(x: f64, y: f64) -> (f64, f64) {
    let hi = x * y;
    let lo = x.mul_add(y, -hi);
    (hi, lo)
}

/// Compensated power-by-squaring for `Float64 ^ Integer`, a direct port of
/// upstream `pow_body(x::Float64, n::Integer)` in `base/special/pow.jl`. It
/// tracks the low-order error terms (`xnlo`, `ynlo`) so the result matches the
/// IEEE-correct `Float64^Integer` to the ULP — e.g. `10.0^-2` is
/// `0.010000000000000002`, not the `0.01` that a plain `powf`/`powi` produces
/// (Issue #7308). Reliable for `-2^20 < n < 2^20` (cf. upstream #53881/#53886);
/// callers route larger magnitudes elsewhere.
#[inline]
fn pow_body_f64_int(x_in: f64, n_in: i64) -> f64 {
    let mut x = x_in;
    let mut y = 1.0_f64;
    let mut xnlo = -0.0_f64;
    let mut ynlo = 0.0_f64;
    // keep compatibility with literal_pow
    if n_in == 3 {
        return x * x * x;
    }
    let mut n = n_in;
    if n < 0 {
        let rx = 1.0 / x;
        // keep compatibility with literal_pow
        if n == -2 {
            return rx * rx;
        }
        if x.is_finite() {
            xnlo = -x.mul_add(rx, -1.0) * rx;
        }
        x = rx;
        n = -n;
    }
    while n > 1 {
        if n & 1 > 0 {
            let err = y.mul_add(xnlo, x * ynlo);
            let (yy, yynlo) = two_mul_f64(x, y);
            y = yy;
            ynlo = yynlo + err;
        }
        let err = x * 2.0 * xnlo;
        let (xx, xxnlo) = two_mul_f64(x, x);
        x = xx;
        xnlo = xxnlo + err;
        n >>= 1;
    }
    let err = y.mul_add(xnlo, x * ynlo);
    if x.is_finite() && err.is_finite() {
        x.mul_add(y, err)
    } else {
        x * y
    }
}

/// `Float64 ^ Float64` matching upstream Julia's reduction order. When the
/// exponent is an exactly integer value within the reliable range of
/// `pow_body_f64_int`, this routes to the compensated integer algorithm
/// (upstream `^(x::Float64, n::Integer)`); otherwise it falls back to the
/// correctly-rounded `powf` (which agrees with upstream `pow_body(::Float64,
/// ::Float64)` for fractional and out-of-range exponents). Issue #7308.
#[inline]
pub(crate) fn pow_f64(base: f64, exp: f64) -> f64 {
    // `-2^20 < n < 2^20` is the range upstream uses for compensated squaring.
    const POW_BY_SQUARING_BOUND: f64 = (1i64 << 20) as f64;
    if exp == exp.trunc() && exp.abs() < POW_BY_SQUARING_BOUND {
        // upstream `^(x::Float64, n::Integer)` short-circuits `n == 0` to
        // `one(x)` before calling `pow_body` (which assumes `n != 0`).
        if exp == 0.0 {
            return 1.0;
        }
        // Safe: |exp| < 2^20 and exp is integer-valued, so it fits an i64.
        return pow_body_f64_int(base, exp as i64);
    }
    base.powf(exp)
}

/// Convert a Value to f64, resolving heap-backed Rational/Irrational structs.
pub(crate) fn value_to_f64_with_heap(
    val: &Value,
    struct_heap: &[StructInstance],
) -> Result<f64, VmError> {
    match val {
        Value::BigInt(v) => Ok(v.to_f64().unwrap_or(f64::INFINITY)),
        Value::Struct(s) => struct_instance_to_f64(s, val),
        Value::StructRef(idx) => {
            let s = struct_heap
                .get(*idx)
                .ok_or_else(|| VmError::TypeError(format!("invalid struct reference: {}", idx)))?;
            struct_instance_to_f64(s, val)
        }
        _ => value_to_f64(val),
    }
}

fn struct_instance_to_f64(s: &StructInstance, original: &Value) -> Result<f64, VmError> {
    if let Some(v) = s.as_irrational_f64() {
        return Ok(v);
    }
    if let Some((num, den)) = s.as_rational_parts_f64() {
        return Ok(num / den);
    }
    Err(VmError::TypeError(format!(
        "expected numeric value, got {:?}",
        original
    )))
}

// === Bit-shift helpers (Issue #3565) ===
// Julia's `<<`/`>>`/`>>>` operators saturate to 0 for shift amounts >= bit-width
// (and use the negated direction for negative amounts). The plain `<<`/`>>` Rust
// operators panic in debug or are UB in release for out-of-range shifts, so we
// implement explicit saturation here.

fn saturating_shl_i64(a: i64, b: i64) -> i64 {
    if b >= 0 {
        if b >= 64 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -64 {
        saturating_ashr_i64(a, -b)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_ashr_i64(a: i64, b: i64) -> i64 {
    if b >= 0 {
        if b >= 64 {
            if a < 0 {
                -1
            } else {
                0
            }
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -64 {
        a.wrapping_shl((-b) as u32)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_lshr_u64(a: u64, b: i64) -> u64 {
    if b >= 0 {
        if b >= 64 {
            0
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -64 {
        a.wrapping_shl((-b) as u32)
    } else {
        0
    }
}

fn saturating_shl_u8(a: u8, b: i64) -> u8 {
    if b >= 0 {
        if b >= 8 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -8 {
        a.wrapping_shr((-b) as u32)
    } else {
        0
    }
}
fn saturating_lshr_u8(a: u8, b: i64) -> u8 {
    if b >= 0 {
        if b >= 8 {
            0
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -8 {
        a.wrapping_shl((-b) as u32)
    } else {
        0
    }
}
fn saturating_shl_u16(a: u16, b: i64) -> u16 {
    if b >= 0 {
        if b >= 16 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -16 {
        a.wrapping_shr((-b) as u32)
    } else {
        0
    }
}
fn saturating_lshr_u16(a: u16, b: i64) -> u16 {
    if b >= 0 {
        if b >= 16 {
            0
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -16 {
        a.wrapping_shl((-b) as u32)
    } else {
        0
    }
}
fn saturating_shl_u32(a: u32, b: i64) -> u32 {
    if b >= 0 {
        if b >= 32 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -32 {
        a.wrapping_shr((-b) as u32)
    } else {
        0
    }
}
fn saturating_lshr_u32(a: u32, b: i64) -> u32 {
    if b >= 0 {
        if b >= 32 {
            0
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -32 {
        a.wrapping_shl((-b) as u32)
    } else {
        0
    }
}
fn saturating_shl_u64(a: u64, b: i64) -> u64 {
    if b >= 0 {
        if b >= 64 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -64 {
        a.wrapping_shr((-b) as u32)
    } else {
        0
    }
}
fn saturating_shl_u128(a: u128, b: i64) -> u128 {
    if b >= 0 {
        if b >= 128 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -128 {
        a.wrapping_shr((-b) as u32)
    } else {
        0
    }
}
fn saturating_lshr_u128(a: u128, b: i64) -> u128 {
    if b >= 0 {
        if b >= 128 {
            0
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -128 {
        a.wrapping_shl((-b) as u32)
    } else {
        0
    }
}
fn saturating_shl_i8(a: i8, b: i64) -> i8 {
    if b >= 0 {
        if b >= 8 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -8 {
        saturating_ashr_i8(a, -b)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_ashr_i8(a: i8, b: i64) -> i8 {
    if b >= 0 {
        if b >= 8 {
            if a < 0 {
                -1
            } else {
                0
            }
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -8 {
        a.wrapping_shl((-b) as u32)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_shl_i16(a: i16, b: i64) -> i16 {
    if b >= 0 {
        if b >= 16 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -16 {
        saturating_ashr_i16(a, -b)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_ashr_i16(a: i16, b: i64) -> i16 {
    if b >= 0 {
        if b >= 16 {
            if a < 0 {
                -1
            } else {
                0
            }
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -16 {
        a.wrapping_shl((-b) as u32)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_shl_i32(a: i32, b: i64) -> i32 {
    if b >= 0 {
        if b >= 32 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -32 {
        saturating_ashr_i32(a, -b)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_ashr_i32(a: i32, b: i64) -> i32 {
    if b >= 0 {
        if b >= 32 {
            if a < 0 {
                -1
            } else {
                0
            }
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -32 {
        a.wrapping_shl((-b) as u32)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_shl_i128(a: i128, b: i64) -> i128 {
    if b >= 0 {
        if b >= 128 {
            0
        } else {
            a.wrapping_shl(b as u32)
        }
    } else if b > -128 {
        saturating_ashr_i128(a, -b)
    } else if a < 0 {
        -1
    } else {
        0
    }
}
fn saturating_ashr_i128(a: i128, b: i64) -> i128 {
    if b >= 0 {
        if b >= 128 {
            if a < 0 {
                -1
            } else {
                0
            }
        } else {
            a.wrapping_shr(b as u32)
        }
    } else if b > -128 {
        a.wrapping_shl((-b) as u32)
    } else if a < 0 {
        -1
    } else {
        0
    }
}

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_intrinsic(&mut self, intrinsic: Intrinsic) -> Result<(), VmError> {
        match intrinsic {
            // === Integer Arithmetic ===
            Intrinsic::NegInt => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_neg()));
            }
            Intrinsic::AddInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_add(b)));
            }
            Intrinsic::SubInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_sub(b)));
            }
            Intrinsic::MulInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_mul(b)));
            }
            Intrinsic::SdivInt => {
                // Issue #3694: handle (I128, I128) natively so Pure Julia
                // div(::Int128, ::Int128) preserves Int128 instead of falling
                // back to floor(x / y) which returns Float64.
                // Issue #3696: same treatment for (U128, U128) using unsigned
                // division so values ≥ 2^127 are handled correctly.
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::I128(a), Value::I128(b)) => {
                        let quotient = (*a).checked_div(*b).ok_or(VmError::DivisionByZero)?;
                        self.stack.push(Value::I128(quotient));
                    }
                    (Value::U128(a), Value::U128(b)) => {
                        if *b == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        self.stack.push(Value::U128(a / b));
                    }
                    (Value::U64(a), Value::U64(b)) => {
                        // Issue #3701: native U64 division. The legacy I64
                        // fallback below would `try_from(u64) -> i64` and
                        // raise OverflowError for any value above i64::MAX.
                        if *b == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        self.stack.push(Value::U64(a / b));
                    }
                    _ => {
                        // Fall back to I64 path (matches the legacy behavior)
                        self.stack.push(a_val);
                        self.stack.push(b_val);
                        let b = self.stack.pop_i64()?;
                        let a = self.stack.pop_i64()?;
                        let quotient = a.checked_div(b).ok_or(VmError::DivisionByZero)?;
                        self.stack.push(Value::I64(quotient));
                    }
                }
            }
            Intrinsic::SremInt => {
                // Check for Float32 operands to preserve type (same pattern as DynamicAdd etc.)
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::F32(a), Value::F32(b)) => {
                        self.stack.push(Value::F32(a % b));
                    }
                    (Value::F64(_) | Value::F32(_), _) | (_, Value::F64(_) | Value::F32(_)) => {
                        let a = value_to_f64(&a_val)?;
                        let b = value_to_f64(&b_val)?;
                        self.stack.push(Value::F64(a % b));
                    }
                    (Value::I128(a), Value::I128(b)) => {
                        if *b == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        self.stack.push(Value::I128(a.wrapping_rem(*b)));
                    }
                    (Value::U128(a), Value::U128(b)) => {
                        if *b == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        self.stack.push(Value::U128(a % b));
                    }
                    (Value::U64(a), Value::U64(b)) => {
                        if *b == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        self.stack.push(Value::U64(a % b));
                    }
                    _ => {
                        let a = match a_val {
                            Value::I64(v) => v,
                            Value::I32(v) => v as i64,
                            Value::I16(v) => v as i64,
                            Value::I8(v) => v as i64,
                            Value::I128(v) => v as i64,
                            Value::U8(v) => v as i64,
                            Value::U16(v) => v as i64,
                            Value::U32(v) => v as i64,
                            Value::U64(v) => v as i64,
                            Value::U128(v) => v as i64,
                            Value::Bool(v) => {
                                if v {
                                    1
                                } else {
                                    0
                                }
                            }
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "expected integer for SremInt, got {:?}",
                                    a_val
                                )))
                            }
                        };
                        let b = match b_val {
                            Value::I64(v) => v,
                            Value::I32(v) => v as i64,
                            Value::I16(v) => v as i64,
                            Value::I8(v) => v as i64,
                            Value::I128(v) => v as i64,
                            Value::U8(v) => v as i64,
                            Value::U16(v) => v as i64,
                            Value::U32(v) => v as i64,
                            Value::U64(v) => v as i64,
                            Value::U128(v) => v as i64,
                            Value::Bool(v) => {
                                if v {
                                    1
                                } else {
                                    0
                                }
                            }
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "expected integer for SremInt, got {:?}",
                                    b_val
                                )))
                            }
                        };
                        if b == 0 {
                            return Err(VmError::DivisionByZero);
                        }
                        // wrapping_rem: rem(typemin(Int64), -1) == 0 in Julia; a
                        // plain `%` panics on the i64::MIN % -1 overflow (Issue #9429).
                        self.stack.push(Value::I64(a.wrapping_rem(b)));
                    }
                }
            }

            // === Floating-Point Arithmetic ===
            Intrinsic::NegFloat => {
                // Preserve input float type (F16→F16, F32→F32, F64→F64)
                match self.stack.pop_value()? {
                    Value::F16(a) => self.stack.push(Value::F16(-a)),
                    Value::F32(a) => self.stack.push(Value::F32(-a)),
                    other => {
                        let a = value_to_f64(&other)?;
                        self.stack.push(Value::F64(-a));
                    }
                }
            }

            // === Runtime-Dispatched Operations ===
            Intrinsic::NegAny => {
                // Negate value preserving its type. Issue #3705: handle every
                // primitive integer width so unary `-` works for I8/I16/I32/I128
                // and the unsigned types. Unsigned negation wraps as in Julia
                // (`-UInt8(5) == UInt8(251)`); the wrapping_neg builtins compute
                // exactly the two's-complement value Julia returns.
                match self.stack.pop_value()? {
                    Value::I8(a) => self.stack.push(Value::I8(a.wrapping_neg())),
                    Value::I16(a) => self.stack.push(Value::I16(a.wrapping_neg())),
                    Value::I32(a) => self.stack.push(Value::I32(a.wrapping_neg())),
                    Value::I64(a) => self.stack.push(Value::I64(a.wrapping_neg())),
                    Value::I128(a) => self.stack.push(Value::I128(a.wrapping_neg())),
                    Value::U8(a) => self.stack.push(Value::U8(a.wrapping_neg())),
                    Value::U16(a) => self.stack.push(Value::U16(a.wrapping_neg())),
                    Value::U32(a) => self.stack.push(Value::U32(a.wrapping_neg())),
                    Value::U64(a) => self.stack.push(Value::U64(a.wrapping_neg())),
                    Value::U128(a) => self.stack.push(Value::U128(a.wrapping_neg())),
                    Value::F16(a) => self.stack.push(Value::F16(-a)),
                    Value::F32(a) => self.stack.push(Value::F32(-a)),
                    Value::F64(a) => self.stack.push(Value::F64(-a)),
                    Value::Bool(a) => {
                        // -true == -1, -false == 0 (Julia widens Bool to Int64)
                        self.stack.push(Value::I64(if a { -1 } else { 0 }));
                    }
                    Value::BigInt(a) => self.stack.push(Value::BigInt(-a)),
                    // `-x` allocates its result at the current precision, like
                    // upstream `z = BigFloat(); mpfr_neg(z, x, ...)` (Issue #9332).
                    Value::BigFloat(a) => self.stack.push(Value::BigFloat(
                        a.neg()
                            .with_precision(get_bigfloat_precision(), get_bigfloat_rounding()),
                    )),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "neg_any: expected numeric, got {:?}",
                            other
                        )))
                    }
                }
            }
            Intrinsic::DynamicAdd => {
                // Check if both operands are F32 to preserve type
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::F32(a), Value::F32(b)) => {
                        self.stack.push(Value::F32(a + b));
                    }
                    _ => {
                        let a = value_to_f64(&a_val)?;
                        let b = value_to_f64(&b_val)?;
                        self.stack.push(Value::F64(a + b));
                    }
                }
            }
            Intrinsic::DynamicSub => {
                // Check if both operands are F32 to preserve type
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::F32(a), Value::F32(b)) => {
                        self.stack.push(Value::F32(a - b));
                    }
                    _ => {
                        let a = value_to_f64(&a_val)?;
                        let b = value_to_f64(&b_val)?;
                        self.stack.push(Value::F64(a - b));
                    }
                }
            }
            Intrinsic::DynamicMul => {
                // Check if both operands are F32 to preserve type
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::F32(a), Value::F32(b)) => {
                        self.stack.push(Value::F32(a * b));
                    }
                    _ => {
                        let a = value_to_f64(&a_val)?;
                        let b = value_to_f64(&b_val)?;
                        self.stack.push(Value::F64(a * b));
                    }
                }
            }
            Intrinsic::DynamicDiv => {
                // Check if both operands are F32 to preserve type
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::F32(a), Value::F32(b)) => {
                        // IEEE 754: 0.0/0.0 = NaN, x/0.0 = ±Inf
                        self.stack.push(Value::F32(a / b));
                    }
                    _ => {
                        let a = value_to_f64(&a_val)?;
                        let b = value_to_f64(&b_val)?;
                        // IEEE 754: 0.0/0.0 = NaN, x/0.0 = ±Inf
                        self.stack.push(Value::F64(a / b));
                    }
                }
            }
            Intrinsic::DynamicPow => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(pow_f64(a, b)));
            }

            // === Integer Comparisons - return Bool (Julia semantics) ===
            Intrinsic::EqInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::Bool(a == b));
            }
            Intrinsic::NeInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::Bool(a != b));
            }
            Intrinsic::SltInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::Bool(a < b));
            }
            Intrinsic::SleInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::Bool(a <= b));
            }
            Intrinsic::SgtInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::Bool(a > b));
            }
            Intrinsic::SgeInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::Bool(a >= b));
            }

            // === Floating-Point Comparisons - return Bool (Julia semantics) ===
            Intrinsic::EqFloat => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::Bool(a == b));
            }
            Intrinsic::NeFloat => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::Bool(a != b));
            }
            Intrinsic::LtFloat => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::Bool(a < b));
            }
            Intrinsic::LeFloat => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::Bool(a <= b));
            }
            Intrinsic::GtFloat => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::Bool(a > b));
            }
            Intrinsic::GeFloat => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::Bool(a >= b));
            }

            // === Bitwise Operations ===
            // Issue #3565: Bitwise intrinsics preserve narrow integer types
            // (UInt8/UInt16/UInt32/UInt64/Int8/Int16/Int32/Int128 ⊕ same → same type).
            // Mixed narrow + I64 (or any other combination) widens to I64 to match
            // the existing `+`/`-`/`*` widening semantics for unsigned integer pairs.
            Intrinsic::AndInt => {
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::U8(a), Value::U8(b)) => self.stack.push(Value::U8(a & b)),
                    (Value::U16(a), Value::U16(b)) => self.stack.push(Value::U16(a & b)),
                    (Value::U32(a), Value::U32(b)) => self.stack.push(Value::U32(a & b)),
                    (Value::U64(a), Value::U64(b)) => self.stack.push(Value::U64(a & b)),
                    (Value::U128(a), Value::U128(b)) => self.stack.push(Value::U128(a & b)),
                    (Value::I8(a), Value::I8(b)) => self.stack.push(Value::I8(a & b)),
                    (Value::I16(a), Value::I16(b)) => self.stack.push(Value::I16(a & b)),
                    (Value::I32(a), Value::I32(b)) => self.stack.push(Value::I32(a & b)),
                    (Value::I128(a), Value::I128(b)) => self.stack.push(Value::I128(a & b)),
                    (Value::Bool(a), Value::Bool(b)) => self.stack.push(Value::Bool(*a & *b)),
                    _ => {
                        // Fallback: widen to I64
                        self.stack.push(a_val);
                        self.stack.push(b_val);
                        let b = self.stack.pop_i64()?;
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(a & b));
                    }
                }
            }
            Intrinsic::OrInt => {
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::U8(a), Value::U8(b)) => self.stack.push(Value::U8(a | b)),
                    (Value::U16(a), Value::U16(b)) => self.stack.push(Value::U16(a | b)),
                    (Value::U32(a), Value::U32(b)) => self.stack.push(Value::U32(a | b)),
                    (Value::U64(a), Value::U64(b)) => self.stack.push(Value::U64(a | b)),
                    (Value::U128(a), Value::U128(b)) => self.stack.push(Value::U128(a | b)),
                    (Value::I8(a), Value::I8(b)) => self.stack.push(Value::I8(a | b)),
                    (Value::I16(a), Value::I16(b)) => self.stack.push(Value::I16(a | b)),
                    (Value::I32(a), Value::I32(b)) => self.stack.push(Value::I32(a | b)),
                    (Value::I128(a), Value::I128(b)) => self.stack.push(Value::I128(a | b)),
                    (Value::Bool(a), Value::Bool(b)) => self.stack.push(Value::Bool(*a | *b)),
                    _ => {
                        self.stack.push(a_val);
                        self.stack.push(b_val);
                        let b = self.stack.pop_i64()?;
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(a | b));
                    }
                }
            }
            Intrinsic::XorInt => {
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::U8(a), Value::U8(b)) => self.stack.push(Value::U8(a ^ b)),
                    (Value::U16(a), Value::U16(b)) => self.stack.push(Value::U16(a ^ b)),
                    (Value::U32(a), Value::U32(b)) => self.stack.push(Value::U32(a ^ b)),
                    (Value::U64(a), Value::U64(b)) => self.stack.push(Value::U64(a ^ b)),
                    (Value::U128(a), Value::U128(b)) => self.stack.push(Value::U128(a ^ b)),
                    (Value::I8(a), Value::I8(b)) => self.stack.push(Value::I8(a ^ b)),
                    (Value::I16(a), Value::I16(b)) => self.stack.push(Value::I16(a ^ b)),
                    (Value::I32(a), Value::I32(b)) => self.stack.push(Value::I32(a ^ b)),
                    (Value::I128(a), Value::I128(b)) => self.stack.push(Value::I128(a ^ b)),
                    (Value::Bool(a), Value::Bool(b)) => self.stack.push(Value::Bool(*a ^ *b)),
                    _ => {
                        self.stack.push(a_val);
                        self.stack.push(b_val);
                        let b = self.stack.pop_i64()?;
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(a ^ b));
                    }
                }
            }
            Intrinsic::NotInt => {
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match a_val {
                    Value::U8(a) => self.stack.push(Value::U8(!a)),
                    Value::U16(a) => self.stack.push(Value::U16(!a)),
                    Value::U32(a) => self.stack.push(Value::U32(!a)),
                    Value::U64(a) => self.stack.push(Value::U64(!a)),
                    Value::U128(a) => self.stack.push(Value::U128(!a)),
                    Value::I8(a) => self.stack.push(Value::I8(!a)),
                    Value::I16(a) => self.stack.push(Value::I16(!a)),
                    Value::I32(a) => self.stack.push(Value::I32(!a)),
                    Value::I128(a) => self.stack.push(Value::I128(!a)),
                    Value::Bool(a) => self.stack.push(Value::Bool(!a)),
                    other => {
                        self.stack.push(other);
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(!a));
                    }
                }
            }
            Intrinsic::ShlInt => {
                // Shift count is taken as an Int; result type preserves the value's width.
                // For shift counts >= width, Julia returns 0 (saturating). Negative counts
                // mean shift in opposite direction, matching `shl_int` semantics where
                // result is undefined per LLVM, but we follow Julia's `<<`/`>>` interface.
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let b = match &b_val {
                    Value::I64(v) => *v,
                    _ => {
                        // Coerce shift count via existing widening
                        self.stack.push(a_val);
                        self.stack.push(b_val);
                        let b = self.stack.pop_i64()?;
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(saturating_shl_i64(a, b)));
                        return Ok(());
                    }
                };
                match a_val {
                    Value::U8(a) => self.stack.push(Value::U8(saturating_shl_u8(a, b))),
                    Value::U16(a) => self.stack.push(Value::U16(saturating_shl_u16(a, b))),
                    Value::U32(a) => self.stack.push(Value::U32(saturating_shl_u32(a, b))),
                    Value::U64(a) => self.stack.push(Value::U64(saturating_shl_u64(a, b))),
                    Value::U128(a) => self.stack.push(Value::U128(saturating_shl_u128(a, b))),
                    Value::I8(a) => self.stack.push(Value::I8(saturating_shl_i8(a, b))),
                    Value::I16(a) => self.stack.push(Value::I16(saturating_shl_i16(a, b))),
                    Value::I32(a) => self.stack.push(Value::I32(saturating_shl_i32(a, b))),
                    Value::I128(a) => self.stack.push(Value::I128(saturating_shl_i128(a, b))),
                    Value::I64(a) => self.stack.push(Value::I64(saturating_shl_i64(a, b))),
                    Value::Bool(a) => {
                        let v = if a { 1i64 } else { 0i64 };
                        self.stack.push(Value::I64(saturating_shl_i64(v, b)));
                    }
                    other => {
                        self.stack.push(other);
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(saturating_shl_i64(a, b)));
                    }
                }
            }
            Intrinsic::LshrInt => {
                // Logical shift right (zero-fill). Preserves narrow integer types.
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let b = match &b_val {
                    Value::I64(v) => *v,
                    _ => {
                        self.stack.push(a_val);
                        self.stack.push(b_val);
                        let b = self.stack.pop_i64()?;
                        let a = self.stack.pop_i64()? as u64;
                        self.stack
                            .push(Value::I64(saturating_lshr_u64(a, b) as i64));
                        return Ok(());
                    }
                };
                match a_val {
                    Value::U8(a) => self.stack.push(Value::U8(saturating_lshr_u8(a, b))),
                    Value::U16(a) => self.stack.push(Value::U16(saturating_lshr_u16(a, b))),
                    Value::U32(a) => self.stack.push(Value::U32(saturating_lshr_u32(a, b))),
                    Value::U64(a) => self.stack.push(Value::U64(saturating_lshr_u64(a, b))),
                    Value::U128(a) => self.stack.push(Value::U128(saturating_lshr_u128(a, b))),
                    Value::I8(a) => self
                        .stack
                        .push(Value::I8(saturating_lshr_u8(a as u8, b) as i8)),
                    Value::I16(a) => self
                        .stack
                        .push(Value::I16(saturating_lshr_u16(a as u16, b) as i16)),
                    Value::I32(a) => self
                        .stack
                        .push(Value::I32(saturating_lshr_u32(a as u32, b) as i32)),
                    Value::I128(a) => self
                        .stack
                        .push(Value::I128(saturating_lshr_u128(a as u128, b) as i128)),
                    Value::I64(a) => self
                        .stack
                        .push(Value::I64(saturating_lshr_u64(a as u64, b) as i64)),
                    Value::Bool(a) => {
                        let v = if a { 1u64 } else { 0u64 };
                        self.stack
                            .push(Value::I64(saturating_lshr_u64(v, b) as i64));
                    }
                    other => {
                        self.stack.push(other);
                        let a = self.stack.pop_i64()? as u64;
                        self.stack
                            .push(Value::I64(saturating_lshr_u64(a, b) as i64));
                    }
                }
            }
            Intrinsic::AshrInt => {
                // Arithmetic shift right (sign-extend for signed). For unsigned types
                // there is no sign bit so this matches `lshr_int` (logical shift).
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let b = match &b_val {
                    Value::I64(v) => *v,
                    _ => {
                        self.stack.push(a_val);
                        self.stack.push(b_val);
                        let b = self.stack.pop_i64()?;
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(saturating_ashr_i64(a, b)));
                        return Ok(());
                    }
                };
                match a_val {
                    // Unsigned: `>>` is the same as `>>>` (logical shift)
                    Value::U8(a) => self.stack.push(Value::U8(saturating_lshr_u8(a, b))),
                    Value::U16(a) => self.stack.push(Value::U16(saturating_lshr_u16(a, b))),
                    Value::U32(a) => self.stack.push(Value::U32(saturating_lshr_u32(a, b))),
                    Value::U64(a) => self.stack.push(Value::U64(saturating_lshr_u64(a, b))),
                    Value::U128(a) => self.stack.push(Value::U128(saturating_lshr_u128(a, b))),
                    Value::I8(a) => self.stack.push(Value::I8(saturating_ashr_i8(a, b))),
                    Value::I16(a) => self.stack.push(Value::I16(saturating_ashr_i16(a, b))),
                    Value::I32(a) => self.stack.push(Value::I32(saturating_ashr_i32(a, b))),
                    Value::I128(a) => self.stack.push(Value::I128(saturating_ashr_i128(a, b))),
                    Value::I64(a) => self.stack.push(Value::I64(saturating_ashr_i64(a, b))),
                    Value::Bool(a) => {
                        let v = if a { 1i64 } else { 0i64 };
                        self.stack.push(Value::I64(saturating_ashr_i64(v, b)));
                    }
                    other => {
                        self.stack.push(other);
                        let a = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(saturating_ashr_i64(a, b)));
                    }
                }
            }

            // === Type Conversions ===
            Intrinsic::Sitofp => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::F64(a as f64));
            }
            Intrinsic::Fptosi => {
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::I64(a as i64));
            }

            // === Low-Level Math ===
            // These intrinsics preserve the input float type (F16→F16, F32→F32, F64→F64).
            Intrinsic::SqrtLlvm => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_float_op_with_heap(
                    val,
                    &self.struct_heap,
                    f64::sqrt,
                )?);
            }
            Intrinsic::FloorLlvm => {
                // BigFloat results round to the current default precision in the
                // direction of the rounding function, mirroring MPFR's
                // `mpfr_floor(z, x)` into a destination allocated at the current
                // precision (Issue #9332).
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    val,
                    &self.struct_heap,
                    f64::floor,
                    |x| {
                        x.floor()
                            .with_precision(get_bigfloat_precision(), BigFloatRoundingMode::Down)
                    },
                )?);
            }
            Intrinsic::CeilLlvm => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    val,
                    &self.struct_heap,
                    f64::ceil,
                    |x| {
                        x.ceil()
                            .with_precision(get_bigfloat_precision(), BigFloatRoundingMode::Up)
                    },
                )?);
            }
            Intrinsic::TruncLlvm => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    val,
                    &self.struct_heap,
                    f64::trunc,
                    |x| {
                        x.trunc()
                            .with_precision(get_bigfloat_precision(), BigFloatRoundingMode::ToZero)
                    },
                )?);
            }
            Intrinsic::RintLlvm => {
                // Round to nearest integer, ties to even (banker's rounding) —
                // matches LLVM's llvm.rint with the default rounding mode and
                // Julia's `rint_llvm`. f64::round rounds half away from zero, so
                // use round_ties_even (round(0.5)==0.0, round(2.5)==2.0).
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    val,
                    &self.struct_heap,
                    f64::round_ties_even,
                    |x| {
                        x.round_nearest_even()
                            .with_precision(get_bigfloat_precision(), BigFloatRoundingMode::ToEven)
                    },
                )?);
            }
            Intrinsic::AbsFloat => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_float_op_with_heap(
                    val,
                    &self.struct_heap,
                    f64::abs,
                )?);
            }
            Intrinsic::CopysignFloat => {
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;
                match (&a_val, &b_val) {
                    (Value::F16(a), Value::F16(b)) => {
                        self.stack.push(Value::F16(half::f16::from_f64(
                            a.to_f64().copysign(b.to_f64()),
                        )));
                    }
                    (Value::F32(a), Value::F32(b)) => {
                        self.stack.push(Value::F32(a.copysign(*b)));
                    }
                    _ => {
                        let a = value_to_f64(&a_val)?;
                        let b = value_to_f64(&b_val)?;
                        self.stack.push(Value::F64(a.copysign(b)));
                    }
                }
            }

            // === BigInt Arithmetic ===
            Intrinsic::NegBigInt => {
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::BigInt(-a));
            }
            Intrinsic::AddBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::BigInt(a + b));
            }
            Intrinsic::SubBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::BigInt(a - b));
            }
            Intrinsic::MulBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::BigInt(a * b));
            }
            Intrinsic::DivBigInt => {
                let b = self.stack.pop_bigint()?;
                if b.is_zero() {
                    return Err(VmError::DivisionByZero);
                }
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::BigInt(a / b));
            }
            Intrinsic::RemBigInt => {
                let b = self.stack.pop_bigint()?;
                if b.is_zero() {
                    return Err(VmError::DivisionByZero);
                }
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::BigInt(a % b));
            }
            Intrinsic::AbsBigInt => {
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::BigInt(a.abs()));
            }
            Intrinsic::PowBigInt => {
                // BigInt power: base^exp where exp is an Integer.
                // Pop exponent first (stack order is reversed)
                let exp = self.stack.pop_bigint()?;
                let base = self.stack.pop_bigint()?;
                if exp.as_inner().sign() == num_bigint::Sign::Minus {
                    return Err(VmError::DomainError(
                        "BigInt power exponent cannot be negative".to_string(),
                    ));
                }
                let Some(exp) = exp.as_inner().to_u32() else {
                    return Err(VmError::DomainError(
                        "BigInt power exponent is too large".to_string(),
                    ));
                };
                let result = base.pow(exp);
                self.stack.push(Value::BigInt(result.into()));
            }

            // === BigInt Comparisons - return Bool (Julia semantics) ===
            Intrinsic::EqBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::Bool(a == b));
            }
            Intrinsic::NeBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::Bool(a != b));
            }
            Intrinsic::LtBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::Bool(a < b));
            }
            Intrinsic::LeBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::Bool(a <= b));
            }
            Intrinsic::GtBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::Bool(a > b));
            }
            Intrinsic::GeBigInt => {
                let b = self.stack.pop_bigint()?;
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::Bool(a >= b));
            }

            // === BigInt Conversions ===
            Intrinsic::I64ToBigInt => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::BigInt(RustBigInt::from(a)));
            }
            Intrinsic::BigIntToI64 => {
                let a = self.stack.pop_bigint()?;
                let result = a.to_i64().ok_or_else(|| {
                    VmError::TypeError("BigInt value too large to convert to Int64".to_string())
                })?;
                self.stack.push(Value::I64(result));
            }
            Intrinsic::StringToBigInt => {
                let s = self.stack.pop_str()?;
                let result = s
                    .parse::<RustBigInt>()
                    .map_err(|_| VmError::TypeError(format!("Cannot parse '{}' as BigInt", s)))?;
                self.stack.push(Value::BigInt(result));
            }
            Intrinsic::BigIntToString => {
                let a = self.stack.pop_bigint()?;
                self.stack.push(Value::str_new(a.to_string()));
            }

            // === BigFloat Arithmetic ===
            // All results are produced at the CURRENT default precision (the
            // active `setprecision(BigFloat, p)` context), mirroring upstream
            // MPFR where every result is allocated as `z = BigFloat()` at the
            // current precision and the operation rounds into it (Issue #9332).
            Intrinsic::NegBigFloat => {
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::BigFloat(
                    a.neg()
                        .with_precision(get_bigfloat_precision(), get_bigfloat_rounding()),
                ));
            }
            Intrinsic::AddBigFloat => {
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;
                if let Some(result) = bigfloat_zero_plus_exact_zero(&a_val, &b_val, false) {
                    self.stack.push(Value::BigFloat(result));
                    return Ok(());
                }
                let b = value_to_bigfloat_exact(&b_val)?;
                let a = value_to_bigfloat_exact(&a_val)?;
                self.stack.push(Value::BigFloat(a.add(
                    &b,
                    get_bigfloat_precision(),
                    get_bigfloat_rounding(),
                )));
            }
            Intrinsic::SubBigFloat => {
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;
                if let Some(result) = bigfloat_zero_plus_exact_zero(&a_val, &b_val, true) {
                    self.stack.push(Value::BigFloat(result));
                    return Ok(());
                }
                let b = value_to_bigfloat_exact(&b_val)?;
                let a = value_to_bigfloat_exact(&a_val)?;
                self.stack.push(Value::BigFloat(a.sub(
                    &b,
                    get_bigfloat_precision(),
                    get_bigfloat_rounding(),
                )));
            }
            Intrinsic::MulBigFloat => {
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;
                if let Some(result) = bigfloat_bool_strong_zero_mul(&a_val, &b_val)? {
                    self.stack.push(Value::BigFloat(result));
                    return Ok(());
                }
                let b = value_to_bigfloat_exact(&b_val)?;
                let a = value_to_bigfloat_exact(&a_val)?;
                self.stack.push(Value::BigFloat(a.mul(
                    &b,
                    get_bigfloat_precision(),
                    get_bigfloat_rounding(),
                )));
            }
            Intrinsic::DivBigFloat => {
                // No zero-divisor guard: BigFloat division is IEEE like Float64,
                // so x/0 must yield ±Inf (and 0/0 → NaN), not raise
                // DivisionByZero. astro_float's `div` produces the correct
                // ±Inf / NaN result directly (Issue #6791).
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;
                let b = value_to_bigfloat_exact(&b_val)?;
                let a = value_to_bigfloat_exact(&a_val)?;
                self.stack.push(Value::BigFloat(a.div(
                    &b,
                    get_bigfloat_precision(),
                    get_bigfloat_rounding(),
                )));
            }
            Intrinsic::RemBigFloat => {
                // BigFloat remainder (`%` / `rem`), sign follows the dividend
                // like Float64 (Issue #6796). `x % 0` is NaN in Julia, but
                // astro_float's `rem` returns ±Inf for a zero divisor, so
                // special-case it. astro_float's `rem` is exact, so round the
                // result to the current precision like MPFR's `mpfr_fmod` does
                // when storing into the destination (Issue #9332).
                let b_val = self.stack.pop_value()?;
                let a_val = self.stack.pop_value()?;
                let b = value_to_bigfloat_exact(&b_val)?;
                let a = value_to_bigfloat_exact(&a_val)?;
                let result = if b.is_zero() {
                    RustBigFloat::from_f64(f64::NAN, get_bigfloat_precision())
                } else {
                    a.rem(&b)
                        .with_precision(get_bigfloat_precision(), get_bigfloat_rounding())
                };
                self.stack.push(Value::BigFloat(result));
            }
            Intrinsic::AbsBigFloat => {
                // Upstream `abs(x::BigFloat)` is `flipsign(x, x)`: a positive
                // input is returned unchanged (keeps its own precision), while
                // a negative input allocates `-x` at the current precision
                // (Issue #9332).
                let a = self.stack.pop_bigfloat()?;
                let result = if a.is_negative() {
                    a.abs()
                        .with_precision(get_bigfloat_precision(), get_bigfloat_rounding())
                } else {
                    a.abs()
                };
                self.stack.push(Value::BigFloat(result));
            }

            // === BigFloat Comparisons - return Bool (Julia semantics) ===
            // cmp() returns Some(sign): positive if a > b, negative if a < b, 0 if equal, None if NaN
            Intrinsic::EqBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                let result = matches!(a.cmp(&b), Some(0));
                self.stack.push(Value::Bool(result));
            }
            Intrinsic::NeBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                let result = match a.cmp(&b) {
                    Some(0) => false,
                    _ => true, // NaN != anything is true
                };
                self.stack.push(Value::Bool(result));
            }
            Intrinsic::LtBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                let result = matches!(a.cmp(&b), Some(x) if x < 0);
                self.stack.push(Value::Bool(result));
            }
            Intrinsic::LeBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                let result = matches!(a.cmp(&b), Some(x) if x <= 0);
                self.stack.push(Value::Bool(result));
            }
            Intrinsic::GtBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                let result = matches!(a.cmp(&b), Some(x) if x > 0);
                self.stack.push(Value::Bool(result));
            }
            Intrinsic::GeBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                let result = matches!(a.cmp(&b), Some(x) if x >= 0);
                self.stack.push(Value::Bool(result));
            }

            // === BigFloat Conversions ===
            Intrinsic::F64ToBigFloat => {
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::BigFloat(RustBigFloat::from_f64(
                    a,
                    get_bigfloat_precision(),
                )));
            }
            Intrinsic::BigFloatToF64 => {
                let a = self.stack.pop_bigfloat()?;
                let result = a.to_string().parse::<f64>().unwrap_or(f64::NAN);
                self.stack.push(Value::F64(result));
            }
            Intrinsic::StringToBigFloat => {
                let s = self.stack.pop_str()?;
                let mut consts = astro_float::Consts::new().map_err(|e| {
                    VmError::InternalError(format!(
                        "Failed to initialize BigFloat constants: {}",
                        e
                    ))
                })?;
                let bf = RustBigFloat::parse(
                    &s,
                    astro_float::Radix::Dec,
                    get_bigfloat_precision(),
                    BigFloatRoundingMode::ToEven,
                    &mut consts,
                );
                // parse returns BigFloat directly; check if result is NaN for invalid input
                if bf.is_nan() && !s.to_lowercase().contains("nan") {
                    return Err(VmError::TypeError(format!(
                        "Cannot parse '{}' as BigFloat",
                        s
                    )));
                }
                self.stack.push(Value::BigFloat(bf));
            }
            Intrinsic::BigFloatToString => {
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::str_new(a.to_string()));
            }
        }
        Ok(())
    }
}
