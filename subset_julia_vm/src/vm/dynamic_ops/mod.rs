//! Dynamic arithmetic operations for runtime type dispatch.
//!
//! These operations implement Julia's type promotion rules at runtime,
//! used when parameter types are not known at compile time.

// SAFETY: i64→u32 casts in pow operations are guarded by `if *exp < 0` checks.
#![allow(clippy::cast_sign_loss)]

mod dispatch;
mod helpers;

use crate::rng::RngLike;
use crate::vm::intrinsics_exec::pow_f64;
use crate::vm::value::is_native_array_value;

use super::broadcast::Broadcastable;
use super::error::VmError;
use super::narrow_int_arith::{same_type_narrow_int_arith, NarrowIntArithOp};
use super::value::{
    array_wrapper_value_to_array_value, is_complex_type_name, native_array_value_ref,
    ArrayElementType, ArrayValue, RustBigInt, StructInstance, Value,
};
use super::Vm;
use helpers::broadcastable_array_like;

/// Predicate that recognizes the array-like carriers eligible for the dynamic
/// arithmetic broadcast paths. Centralizing the check keeps the per-arm
/// dispatch guards below free of explicit native-array enum spelling while the
/// Memory-first migration retires the native carrier. Delegates the legacy
/// native-array half to the shared
/// [`crate::vm::value::native_array_value_ref`] helper so the native-array
/// match lives in a single place across the VM (Issue #3908).
fn irrational_f64_from_value<R: RngLike>(vm: &Vm<R>, value: &Value) -> Option<f64> {
    match value {
        Value::Struct(s) => s.as_irrational_f64(),
        Value::StructRef(idx) => vm.struct_heap.get(*idx).and_then(|s| s.as_irrational_f64()),
        _ => None,
    }
}

fn is_integer_pow_operand(value: &Value) -> bool {
    matches!(
        value,
        Value::Bool(_)
            | Value::I8(_)
            | Value::I16(_)
            | Value::I32(_)
            | Value::I64(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
    )
}

/// A real (non-Complex) numeric operand that BigFloat power can promote and
/// compute inline (Issue #6790). Excludes Complex/Rational/structs so
/// `BigFloat ^ Complex` / `Real ^ Complex` still reach the Julia `^` methods.
fn is_real_numeric_pow_operand(value: &Value) -> bool {
    is_integer_pow_operand(value)
        || matches!(
            value,
            Value::F64(_) | Value::F32(_) | Value::F16(_) | Value::BigInt(_) | Value::BigFloat(_)
        )
}

/// Whether a `^` should be computed inline as a BigFloat power: at least one
/// operand is a `BigFloat` and both are real numerics (Issue #6790). Without
/// this, `BigFloat ^ <real>` is left to runtime `^` dispatch, which has no
/// terminating `BigFloat` method and infinite-recurses (stack overflow).
/// Complex/Rational operands are excluded and keep going through Julia dispatch.
///
/// Fractional (non-integer) exponents are included (Issue #6794): they take
/// `astro_float`'s `exp(n·ln x)` route, whose Ziv refinement loop is bounded by
/// our vendored patch (see `vendor/astro-float-num/`) so it always terminates.
fn is_bigfloat_pow(a: &Value, b: &Value) -> bool {
    (matches!(a, Value::BigFloat(_)) || matches!(b, Value::BigFloat(_)))
        && is_real_numeric_pow_operand(a)
        && is_real_numeric_pow_operand(b)
}

/// Convert a real-numeric value to a `BigFloat` for an inline BigFloat power
/// (Issue #6790). Integers (including `BigInt`/`Int128`) are parsed from their
/// exact decimal string; floats use `from_f64`. Returns `None` for operands
/// that are not real numerics.
fn value_to_bigfloat_for_pow(
    value: &Value,
    consts: &mut astro_float::Consts,
) -> Option<crate::vm::value::RustBigFloat> {
    use crate::vm::value::{BigFloatRoundingMode, RustBigFloat, BIGFLOAT_PRECISION};
    let int_to_bf = |s: String, consts: &mut astro_float::Consts| {
        RustBigFloat::parse(
            &s,
            astro_float::Radix::Dec,
            BIGFLOAT_PRECISION,
            BigFloatRoundingMode::ToEven,
            consts,
        )
    };
    Some(match value {
        Value::BigFloat(x) => x.clone(),
        Value::F64(x) => RustBigFloat::from_f64(*x, BIGFLOAT_PRECISION),
        Value::F32(x) => RustBigFloat::from_f64(f64::from(*x), BIGFLOAT_PRECISION),
        Value::F16(x) => RustBigFloat::from_f64(f64::from(*x), BIGFLOAT_PRECISION),
        Value::Bool(x) => RustBigFloat::from_f64(if *x { 1.0 } else { 0.0 }, BIGFLOAT_PRECISION),
        Value::I8(x) => int_to_bf(x.to_string(), consts),
        Value::I16(x) => int_to_bf(x.to_string(), consts),
        Value::I32(x) => int_to_bf(x.to_string(), consts),
        Value::I64(x) => int_to_bf(x.to_string(), consts),
        Value::I128(x) => int_to_bf(x.to_string(), consts),
        Value::U8(x) => int_to_bf(x.to_string(), consts),
        Value::U16(x) => int_to_bf(x.to_string(), consts),
        Value::U32(x) => int_to_bf(x.to_string(), consts),
        Value::U64(x) => int_to_bf(x.to_string(), consts),
        Value::U128(x) => int_to_bf(x.to_string(), consts),
        Value::BigInt(x) => int_to_bf(x.to_string(), consts),
        _ => return None,
    })
}

fn bigint_pow_exponent_as_f64(exp: &RustBigInt) -> f64 {
    use num_traits::ToPrimitive;

    exp.as_inner()
        .to_f64()
        .unwrap_or_else(|| match exp.as_inner().sign() {
            num_bigint::Sign::Minus => f64::NEG_INFINITY,
            _ => f64::INFINITY,
        })
}

fn dynamic_float_bigint_pow(a: &Value, b: &Value) -> Option<Value> {
    let Value::BigInt(exp) = b else {
        return None;
    };
    let exp = bigint_pow_exponent_as_f64(exp);

    match a {
        Value::F64(base) => Some(Value::F64(pow_f64(*base, exp))),
        Value::F32(base) => Some(Value::F32((*base as f64).powf(exp) as f32)),
        Value::F16(base) => {
            let result = f64::from(*base).powf(exp);
            Some(Value::F16(half::f16::from_f64(result)))
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct NonnegativePowExponent {
    wrapping_pow_exponent: u32,
    is_zero: bool,
}

#[derive(Clone, Copy)]
enum IntegerPowExponent {
    Nonnegative(NonnegativePowExponent),
    Negative,
}

fn integer_pow_exponent(value: &Value) -> Option<IntegerPowExponent> {
    use IntegerPowExponent::{Negative, Nonnegative};

    match value {
        Value::Bool(exp) => Some(Nonnegative(NonnegativePowExponent {
            wrapping_pow_exponent: if *exp { 1 } else { 0 },
            is_zero: !*exp,
        })),
        Value::I8(exp) => Some(if *exp < 0 {
            Negative
        } else {
            Nonnegative(NonnegativePowExponent {
                wrapping_pow_exponent: *exp as u32,
                is_zero: *exp == 0,
            })
        }),
        Value::I16(exp) => Some(if *exp < 0 {
            Negative
        } else {
            Nonnegative(NonnegativePowExponent {
                wrapping_pow_exponent: *exp as u32,
                is_zero: *exp == 0,
            })
        }),
        Value::I32(exp) => Some(if *exp < 0 {
            Negative
        } else {
            Nonnegative(NonnegativePowExponent {
                wrapping_pow_exponent: *exp as u32,
                is_zero: *exp == 0,
            })
        }),
        Value::I64(exp) => Some(if *exp < 0 {
            Negative
        } else {
            Nonnegative(NonnegativePowExponent {
                wrapping_pow_exponent: *exp as u32,
                is_zero: *exp == 0,
            })
        }),
        Value::I128(exp) => Some(if *exp < 0 {
            Negative
        } else {
            Nonnegative(NonnegativePowExponent {
                wrapping_pow_exponent: *exp as u32,
                is_zero: *exp == 0,
            })
        }),
        Value::U8(exp) => Some(Nonnegative(NonnegativePowExponent {
            wrapping_pow_exponent: *exp as u32,
            is_zero: *exp == 0,
        })),
        Value::U16(exp) => Some(Nonnegative(NonnegativePowExponent {
            wrapping_pow_exponent: *exp as u32,
            is_zero: *exp == 0,
        })),
        Value::U32(exp) => Some(Nonnegative(NonnegativePowExponent {
            wrapping_pow_exponent: *exp,
            is_zero: *exp == 0,
        })),
        Value::U64(exp) => Some(Nonnegative(NonnegativePowExponent {
            wrapping_pow_exponent: *exp as u32,
            is_zero: *exp == 0,
        })),
        Value::U128(exp) => Some(Nonnegative(NonnegativePowExponent {
            wrapping_pow_exponent: *exp as u32,
            is_zero: *exp == 0,
        })),
        _ => None,
    }
}

fn negative_integer_pow_error() -> VmError {
    VmError::DomainError(
        "Cannot raise an integer x to a negative power - make x or the exponent a float"
            .to_string(),
    )
}

fn bigint_pow_exponent(value: &Value) -> Result<Option<u32>, VmError> {
    use num_traits::ToPrimitive;

    if let Value::BigInt(exp) = value {
        if exp.as_inner().sign() == num_bigint::Sign::Minus {
            return Err(negative_integer_pow_error());
        }
        return exp
            .as_inner()
            .to_u32()
            .ok_or_else(|| VmError::DomainError("BigInt power exponent is too large".to_string()))
            .map(Some);
    }

    match integer_pow_exponent(value) {
        Some(IntegerPowExponent::Nonnegative(exp)) => Ok(Some(exp.wrapping_pow_exponent)),
        Some(IntegerPowExponent::Negative) => Err(negative_integer_pow_error()),
        None => Ok(None),
    }
}

fn dynamic_bigint_pow(a: &Value, b: &Value) -> Result<Option<Value>, VmError> {
    let Value::BigInt(base) = a else {
        return Ok(None);
    };
    let Some(exp) = bigint_pow_exponent(b)? else {
        return Ok(None);
    };

    Ok(Some(Value::BigInt(base.pow(exp).into())))
}

fn dynamic_integer_pow(a: &Value, b: &Value) -> Result<Option<Value>, VmError> {
    let Some(exp) = integer_pow_exponent(b) else {
        return Ok(None);
    };

    let exp = match exp {
        IntegerPowExponent::Nonnegative(exp) => exp,
        IntegerPowExponent::Negative if is_integer_pow_operand(a) => {
            return Err(negative_integer_pow_error());
        }
        IntegerPowExponent::Negative => return Ok(None),
    };

    let result = match a {
        Value::Bool(base) => Value::Bool(*base || exp.is_zero),
        Value::I8(base) => Value::I8(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::I16(base) => Value::I16(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::I32(base) => Value::I32(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::I64(base) => Value::I64(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::I128(base) => Value::I128(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::U8(base) => Value::U8(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::U16(base) => Value::U16(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::U32(base) => Value::U32(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::U64(base) => Value::U64(base.wrapping_pow(exp.wrapping_pow_exponent)),
        Value::U128(base) => Value::U128(base.wrapping_pow(exp.wrapping_pow_exponent)),
        _ => return Ok(None),
    };

    Ok(Some(result))
}

fn scalar_f64_broadcastable(value: &Value) -> Option<Broadcastable> {
    match value {
        Value::F64(v) => Some(Broadcastable::ScalarF64(*v)),
        Value::I64(v) => Some(Broadcastable::ScalarF64(*v as f64)),
        Value::F32(v) => Some(Broadcastable::ScalarF64(*v as f64)),
        Value::Bool(v) => Some(Broadcastable::ScalarF64(if *v { 1.0 } else { 0.0 })),
        _ => None,
    }
}

fn negated_array_empty_element_type(element_type: &ArrayElementType) -> ArrayElementType {
    match element_type {
        ArrayElementType::Bool => ArrayElementType::I64,
        other => other.clone(),
    }
}

impl<R: RngLike> Vm<R> {
    fn is_array_like_value(&self, value: &Value) -> bool {
        is_native_array_value(value)
            || matches!(value, Value::Memory(_))
            || array_wrapper_value_to_array_value(value, &self.struct_heap)
                .ok()
                .flatten()
                .is_some()
    }

    /// Dynamic addition with type promotion.
    /// Follows Julia semantics: Int64 + Int64 → Int64, Int64 + Float64 → Float64
    #[inline]
    pub(super) fn dynamic_add(&mut self, a: &Value, b: &Value) -> Result<Value, VmError> {
        if let Some(result) = same_type_narrow_int_arith(a, b, NarrowIntArithOp::Add) {
            return Ok(result);
        }
        // Complex and Rational arithmetic is handled by Julia dispatch
        match (a, b) {
            // Int64 + Int64 → Int64
            (Value::I64(x), Value::I64(y)) => Ok(Value::I64(x.wrapping_add(*y))),
            // Int32 + Int32 → Int32
            (Value::I32(x), Value::I32(y)) => Ok(Value::I32(x.wrapping_add(*y))),
            // Float64 + Float64 → Float64
            (Value::F64(x), Value::F64(y)) => Ok(Value::F64(x + y)),
            // Int64 + Float64 → Float64 (promotion)
            (Value::I64(x), Value::F64(y)) => Ok(Value::F64(*x as f64 + y)),
            (Value::F64(x), Value::I64(y)) => Ok(Value::F64(x + *y as f64)),
            // Float32 operations
            (Value::F32(x), Value::F32(y)) => Ok(Value::F32(x + y)),
            (Value::F32(x), Value::I64(y)) => Ok(Value::F32(x + *y as f32)),
            (Value::I64(x), Value::F32(y)) => Ok(Value::F32(*x as f32 + y)),
            // F32 <-> F64 mixed operations promote to F64
            (Value::F32(x), Value::F64(y)) => Ok(Value::F64(*x as f64 + y)),
            (Value::F64(x), Value::F32(y)) => Ok(Value::F64(x + *y as f64)),
            // Bool as Int64
            (Value::Bool(x), Value::Bool(y)) => {
                // Bool + Bool -> Int64 (Julia semantics)
                Ok(Value::I64(if *x { 1 } else { 0 } + if *y { 1 } else { 0 }))
            }
            (Value::Bool(x), Value::I64(y)) => Ok(Value::I64(if *x { 1 } else { 0 } + y)),
            (Value::I64(x), Value::Bool(y)) => Ok(Value::I64(x + if *y { 1 } else { 0 })),
            (Value::Bool(x), Value::F64(y)) => Ok(Value::F64(if *x { 1.0 } else { 0.0 } + y)),
            (Value::F64(x), Value::Bool(y)) => Ok(Value::F64(x + if *y { 1.0 } else { 0.0 })),
            (Value::Bool(x), Value::F32(y)) => Ok(Value::F32(if *x { 1.0f32 } else { 0.0f32 } + y)),
            (Value::F32(x), Value::Bool(y)) => Ok(Value::F32(x + if *y { 1.0f32 } else { 0.0f32 })),
            // Array + Array → element-wise addition
            (lhs, rhs) if self.is_array_like_value(lhs) && self.is_array_like_value(rhs) => {
                use super::broadcast::{broadcast_op_complex, broadcast_op_f64, complex_add};
                let broadcastable_a = broadcastable_array_like(self, a).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot add {:?}", self.value_type_name(a)))
                })?;
                let broadcastable_b = broadcastable_array_like(self, b).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot add {:?}", self.value_type_name(b)))
                })?;
                // Use complex broadcast if either operand is complex
                let result = if broadcastable_a.is_complex() || broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_add)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x + y)?
                };
                Ok(self.array_value_to_wrapper(result)?)
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot add {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    /// Dynamic subtraction with type promotion.
    #[inline]
    pub(super) fn dynamic_sub(&mut self, a: &Value, b: &Value) -> Result<Value, VmError> {
        if let Some(result) = same_type_narrow_int_arith(a, b, NarrowIntArithOp::Sub) {
            return Ok(result);
        }
        // Complex and Rational arithmetic is handled by Julia dispatch
        match (a, b) {
            (Value::I64(x), Value::I64(y)) => Ok(Value::I64(x.wrapping_sub(*y))),
            (Value::F64(x), Value::F64(y)) => Ok(Value::F64(x - y)),
            (Value::I64(x), Value::F64(y)) => Ok(Value::F64(*x as f64 - y)),
            (Value::F64(x), Value::I64(y)) => Ok(Value::F64(x - *y as f64)),
            // Float32 operations
            (Value::F32(x), Value::F32(y)) => Ok(Value::F32(x - y)),
            (Value::F32(x), Value::I64(y)) => Ok(Value::F32(x - *y as f32)),
            (Value::I64(x), Value::F32(y)) => Ok(Value::F32(*x as f32 - y)),
            // F32 <-> F64 mixed operations promote to F64
            (Value::F32(x), Value::F64(y)) => Ok(Value::F64(*x as f64 - y)),
            (Value::F64(x), Value::F32(y)) => Ok(Value::F64(x - *y as f64)),
            // Bool as Int64
            (Value::Bool(x), Value::Bool(y)) => {
                Ok(Value::I64(if *x { 1 } else { 0 } - if *y { 1 } else { 0 }))
            }
            (Value::Bool(x), Value::I64(y)) => Ok(Value::I64(if *x { 1 } else { 0 } - y)),
            (Value::I64(x), Value::Bool(y)) => Ok(Value::I64(x - if *y { 1 } else { 0 })),
            (Value::Bool(x), Value::F64(y)) => Ok(Value::F64(if *x { 1.0 } else { 0.0 } - y)),
            (Value::F64(x), Value::Bool(y)) => Ok(Value::F64(x - if *y { 1.0 } else { 0.0 })),
            (Value::Bool(x), Value::F32(y)) => Ok(Value::F32(if *x { 1.0f32 } else { 0.0f32 } - y)),
            (Value::F32(x), Value::Bool(y)) => Ok(Value::F32(x - if *y { 1.0f32 } else { 0.0f32 })),
            // Array - Array → element-wise subtraction
            (lhs, rhs) if self.is_array_like_value(lhs) && self.is_array_like_value(rhs) => {
                use super::broadcast::{broadcast_op_complex, broadcast_op_f64, complex_sub};
                let broadcastable_a = broadcastable_array_like(self, a).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot subtract {:?}", self.value_type_name(a)))
                })?;
                let broadcastable_b = broadcastable_array_like(self, b).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot subtract {:?}", self.value_type_name(b)))
                })?;
                // Use complex broadcast if either operand is complex
                let result = if broadcastable_a.is_complex() || broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_sub)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x - y)?
                };
                Ok(self.array_value_to_wrapper(result)?)
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot subtract {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    /// Dynamic multiplication with type promotion.
    #[inline]
    pub(super) fn dynamic_mul(&mut self, a: &Value, b: &Value) -> Result<Value, VmError> {
        if let Some(result) = same_type_narrow_int_arith(a, b, NarrowIntArithOp::Mul) {
            return Ok(result);
        }
        // Complex and Rational arithmetic is handled by Julia dispatch
        match (a, b) {
            (Value::I64(x), Value::I64(y)) => Ok(Value::I64(x.wrapping_mul(*y))),
            (Value::I32(x), Value::I32(y)) => Ok(Value::I32(x.wrapping_mul(*y))),
            (Value::F64(x), Value::F64(y)) => Ok(Value::F64(x * y)),
            (Value::I64(x), Value::F64(y)) => Ok(Value::F64(*x as f64 * y)),
            (Value::F64(x), Value::I64(y)) => Ok(Value::F64(x * *y as f64)),
            // Float32 operations
            (Value::F32(x), Value::F32(y)) => Ok(Value::F32(x * y)),
            (Value::F32(x), Value::I64(y)) => Ok(Value::F32(x * *y as f32)),
            (Value::I64(x), Value::F32(y)) => Ok(Value::F32(*x as f32 * y)),
            // F32 <-> F64 mixed operations promote to F64
            (Value::F32(x), Value::F64(y)) => Ok(Value::F64(*x as f64 * y)),
            (Value::F64(x), Value::F32(y)) => Ok(Value::F64(x * *y as f64)),
            // Bool as Int64
            (Value::Bool(x), Value::Bool(y)) => {
                Ok(Value::I64(if *x { 1 } else { 0 } * if *y { 1 } else { 0 }))
            }
            (Value::Bool(x), Value::I64(y)) => Ok(Value::I64(if *x { 1 } else { 0 } * y)),
            (Value::I64(x), Value::Bool(y)) => Ok(Value::I64(x * if *y { 1 } else { 0 })),
            // Bool * Float: Julia strong zero semantics (false * NaN == 0.0, false * Inf == 0.0)
            // Julia: *(x::Bool, y::T) = ifelse(x, y, copysign(zero(y), y))
            (Value::Bool(x), Value::F64(y)) => {
                Ok(Value::F64(if *x { *y } else { 0.0_f64.copysign(*y) }))
            }
            (Value::F64(x), Value::Bool(y)) => {
                Ok(Value::F64(if *y { *x } else { 0.0_f64.copysign(*x) }))
            }
            (Value::Bool(x), Value::F32(y)) => {
                Ok(Value::F32(if *x { *y } else { 0.0_f32.copysign(*y) }))
            }
            (Value::F32(x), Value::Bool(y)) => {
                Ok(Value::F32(if *y { *x } else { 0.0_f32.copysign(*x) }))
            }
            (lhs, scalar)
                if self.is_array_like_value(lhs) && scalar_f64_broadcastable(scalar).is_some() =>
            {
                use super::broadcast::{broadcast_op_complex, broadcast_op_f64, complex_mul};
                let broadcastable_a = broadcastable_array_like(self, a).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot multiply {:?}", self.value_type_name(a)))
                })?;
                let broadcastable_b = scalar_f64_broadcastable(scalar).ok_or_else(|| {
                    VmError::TypeError(format!(
                        "Cannot multiply array by {:?}",
                        self.value_type_name(scalar)
                    ))
                })?;
                let result = if broadcastable_a.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_mul)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x * y)?
                };
                Ok(self.array_value_to_wrapper(result)?)
            }
            (scalar, rhs)
                if scalar_f64_broadcastable(scalar).is_some() && self.is_array_like_value(rhs) =>
            {
                use super::broadcast::{broadcast_op_complex, broadcast_op_f64, complex_mul};
                let broadcastable_a = scalar_f64_broadcastable(scalar).ok_or_else(|| {
                    VmError::TypeError(format!(
                        "Cannot multiply {:?} by array",
                        self.value_type_name(scalar)
                    ))
                })?;
                let broadcastable_b = broadcastable_array_like(self, b).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot multiply {:?}", self.value_type_name(b)))
                })?;
                let result = if broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_mul)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x * y)?
                };
                Ok(self.array_value_to_wrapper(result)?)
            }
            // Array * Array → matrix multiplication. In Julia `*` on arrays is the
            // matrix product; element-wise is `.*`, a separate broadcast path that
            // never reaches `dynamic_mul`. The previous element-wise fallback here
            // silently produced wrong results whenever a `*` on two arrays reached
            // the dynamic path — e.g. a struct-field matrix used in tail position,
            // where typed dispatch can't prove the matmul method (Issue #7175).
            (lhs, rhs) if self.is_array_like_value(lhs) && self.is_array_like_value(rhs) => {
                use crate::vm::builtins_linalg::linalg_value_to_array_value;
                use crate::vm::matmul::{is_complex_array, matmul, matmul_complex};
                let a_arr = linalg_value_to_array_value(
                    a.clone(),
                    &self.struct_heap,
                    "*",
                    Some("left operand"),
                )?;
                let b_arr = linalg_value_to_array_value(
                    b.clone(),
                    &self.struct_heap,
                    "*",
                    Some("right operand"),
                )?;
                let mut result = if is_complex_array(&a_arr) || is_complex_array(&b_arr) {
                    matmul_complex(&a_arr, &b_arr, &self.struct_heap)?
                } else {
                    matmul(&a_arr, &b_arr)?
                };
                if result
                    .element_type_override
                    .as_ref()
                    .is_some_and(|e| e.is_complex())
                {
                    result.struct_type_id = Some(self.get_complex_type_id());
                }
                Ok(self.array_value_to_wrapper(result)?)
            }
            // String/Char concatenation: in Julia `*` concatenates strings and chars.
            // The typed-fast-path handler in `binary_both.rs` already covers this,
            // but `dynamic_mul` is also reachable when the typed dispatch can't pick
            // the path at compile time — e.g. `result * s[i:j]` inside a function
            // where `s[i:j]` is inferred as `Any` (Issue #3671). Without the cases
            // below we'd raise "Cannot multiply String and String" even though both
            // operands are concretely String.
            (Value::Str(x), Value::Str(y)) => Ok(Value::Str(format!("{}{}", x, y))),
            (Value::Str(x), Value::Char(y)) => {
                let mut out = String::with_capacity(x.len() + y.len_utf8());
                out.push_str(x);
                out.push(*y);
                Ok(Value::Str(out))
            }
            (Value::Char(x), Value::Str(y)) => {
                let mut out = String::with_capacity(x.len_utf8() + y.len());
                out.push(*x);
                out.push_str(y);
                Ok(Value::Str(out))
            }
            (Value::Char(x), Value::Char(y)) => {
                let mut out = String::with_capacity(x.len_utf8() + y.len_utf8());
                out.push(*x);
                out.push(*y);
                Ok(Value::Str(out))
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot multiply {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    /// Dynamic division with type promotion.
    /// In Julia, integer division with / always returns Float64.
    #[inline]
    pub(super) fn dynamic_div(&mut self, a: &Value, b: &Value) -> Result<Value, VmError> {
        // Complex and Rational arithmetic is handled by Julia dispatch
        match (a, b) {
            // Julia: Int / Int → Float64
            (Value::I64(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(*x as f64 / *y as f64))
            }
            (Value::F64(x), Value::F64(y)) => {
                // IEEE 754: 0.0/0.0 = NaN, x/0.0 = ±Inf
                Ok(Value::F64(x / y))
            }
            (Value::I64(x), Value::F64(y)) => {
                // IEEE 754: result is F64, follow float semantics
                Ok(Value::F64(*x as f64 / y))
            }
            (Value::F64(x), Value::I64(y)) => {
                // IEEE 754: result is F64, follow float semantics
                Ok(Value::F64(x / *y as f64))
            }
            // Float32 operations
            (Value::F32(x), Value::F32(y)) => {
                // IEEE 754: 0.0/0.0 = NaN, x/0.0 = ±Inf
                Ok(Value::F32(x / y))
            }
            (Value::F32(x), Value::I64(y)) => {
                // IEEE 754: result is F32, follow float semantics
                Ok(Value::F32(x / *y as f32))
            }
            (Value::I64(x), Value::F32(y)) => {
                // IEEE 754: result is F32, follow float semantics
                Ok(Value::F32(*x as f32 / y))
            }
            // F32 <-> F64 mixed operations promote to F64
            (Value::F32(x), Value::F64(y)) => Ok(Value::F64(*x as f64 / y)),
            (Value::F64(x), Value::F32(y)) => Ok(Value::F64(x / *y as f64)),
            // F16 division operations (type preservation, Issue #3699)
            // Mirrors the F16 arms already present in dynamic_mod / dynamic_add
            // so that `div(x, y) = floor(x / y)` and any other dispatch path
            // that lands here for Float16 keeps the F16 result type.
            (Value::F16(x), Value::F16(y)) => Ok(Value::F16(half::f16::from_f32(
                f32::from(*x) / f32::from(*y),
            ))),
            (Value::F16(x), Value::I64(y)) => {
                Ok(Value::F16(half::f16::from_f32(f32::from(*x) / *y as f32)))
            }
            (Value::I64(x), Value::F16(y)) => {
                Ok(Value::F16(half::f16::from_f32(*x as f32 / f32::from(*y))))
            }
            // F16 <-> F64 mixed promotes to F64
            (Value::F16(x), Value::F64(y)) => Ok(Value::F64(f64::from(*x) / y)),
            (Value::F64(x), Value::F16(y)) => Ok(Value::F64(x / f64::from(*y))),
            // F16 <-> F32 mixed promotes to F32
            (Value::F16(x), Value::F32(y)) => Ok(Value::F32(f32::from(*x) / y)),
            (Value::F32(x), Value::F16(y)) => Ok(Value::F32(x / f32::from(*y))),
            // F16 <-> Bool: Bool is treated as 0/1 → result stays F16
            (Value::F16(x), Value::Bool(y)) => Ok(Value::F16(half::f16::from_f32(
                f32::from(*x) / if *y { 1.0f32 } else { 0.0f32 },
            ))),
            (Value::Bool(x), Value::F16(y)) => Ok(Value::F16(half::f16::from_f32(
                if *x { 1.0f32 } else { 0.0f32 } / f32::from(*y),
            ))),
            // BigInt division (integer division, truncated)
            (Value::BigInt(x), Value::BigInt(y)) => {
                use num_traits::Zero;
                if y.is_zero() {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::BigInt(x / y))
            }
            (Value::BigInt(x), Value::I64(y)) => {
                use num_bigint::BigInt;
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::BigInt(x / BigInt::from(*y)))
            }
            (Value::I64(x), Value::BigInt(y)) => {
                use num_bigint::BigInt;
                use num_traits::Zero;
                if y.is_zero() {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::BigInt(
                    (BigInt::from(*x) / y.as_inner().clone()).into(),
                ))
            }
            // Bool as Int64, division returns Float64
            (Value::Bool(x), Value::Bool(y)) => {
                let y_int = if *y { 1.0 } else { 0.0 };
                Ok(Value::F64(if *x { 1.0 } else { 0.0 } / y_int))
            }
            (Value::Bool(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(if *x { 1.0 } else { 0.0 } / *y as f64))
            }
            (Value::I64(x), Value::Bool(y)) => {
                let y_val = if *y { 1.0 } else { 0.0 };
                Ok(Value::F64(*x as f64 / y_val))
            }
            (Value::Bool(x), Value::F64(y)) => Ok(Value::F64(if *x { 1.0 } else { 0.0 } / y)),
            (Value::F64(x), Value::Bool(y)) => Ok(Value::F64(x / if *y { 1.0 } else { 0.0 })),
            (Value::Bool(x), Value::F32(y)) => Ok(Value::F32(if *x { 1.0f32 } else { 0.0f32 } / y)),
            (Value::F32(x), Value::Bool(y)) => Ok(Value::F32(x / if *y { 1.0f32 } else { 0.0f32 })),
            // Array / Scalar → element-wise division
            (lhs, scalar) if self.is_array_like_value(lhs) => {
                use super::broadcast::{
                    broadcast_op_complex, broadcast_op_f64, complex_div, Broadcastable,
                };
                let broadcastable_a = broadcastable_array_like(self, a).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot divide {:?}", self.value_type_name(a)))
                })?;
                let broadcastable_b = match scalar {
                    Value::F64(v) => Broadcastable::ScalarF64(*v),
                    Value::I64(v) => Broadcastable::ScalarF64(*v as f64),
                    Value::F32(v) => Broadcastable::ScalarF64(*v as f64),
                    Value::Bool(v) => Broadcastable::ScalarF64(if *v { 1.0 } else { 0.0 }),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "Cannot divide array by {:?}",
                            self.value_type_name(scalar)
                        )));
                    }
                };
                let result = if broadcastable_a.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_div)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x / y)?
                };
                Ok(self.array_value_to_wrapper(result)?)
            }
            // Scalar / Array → element-wise division
            (scalar, rhs) if self.is_array_like_value(rhs) => {
                use super::broadcast::{
                    broadcast_op_complex, broadcast_op_f64, complex_div, Broadcastable,
                };
                let broadcastable_a = match scalar {
                    Value::F64(v) => Broadcastable::ScalarF64(*v),
                    Value::I64(v) => Broadcastable::ScalarF64(*v as f64),
                    Value::F32(v) => Broadcastable::ScalarF64(*v as f64),
                    Value::Bool(v) => Broadcastable::ScalarF64(if *v { 1.0 } else { 0.0 }),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "Cannot divide {:?} by array",
                            self.value_type_name(scalar)
                        )));
                    }
                };
                let broadcastable_b = broadcastable_array_like(self, b).ok_or_else(|| {
                    VmError::TypeError(format!("Cannot divide by {:?}", self.value_type_name(b)))
                })?;
                let result = if broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_div)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x / y)?
                };
                Ok(self.array_value_to_wrapper(result)?)
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot divide {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    /// Dynamic modulo with type promotion.
    #[inline]
    pub(super) fn dynamic_mod(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::I64(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(x % y))
            }
            (Value::F64(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(x % y))
            }
            (Value::I64(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(*x as f64 % y))
            }
            (Value::F64(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(x % *y as f64))
            }
            // Bool as Int64
            (Value::Bool(x), Value::Bool(y)) => {
                let y_int = if *y { 1 } else { 0 };
                if y_int == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(if *x { 1 } else { 0 } % y_int))
            }
            (Value::Bool(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(if *x { 1 } else { 0 } % y))
            }
            (Value::I64(x), Value::Bool(y)) => {
                let y_int = if *y { 1 } else { 0 };
                if y_int == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(x % y_int))
            }
            (Value::Bool(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(if *x { 1.0 } else { 0.0 } % y))
            }
            (Value::F64(x), Value::Bool(y)) => {
                let y_val = if *y { 1.0 } else { 0.0 };
                if y_val == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(x % y_val))
            }
            // F32 mod operations (type preservation)
            (Value::F32(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32(x % y))
            }
            (Value::F32(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32(x % *y as f32))
            }
            (Value::I64(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32(*x as f32 % y))
            }
            // F32 <-> F64 mixed mod promotes to F64
            (Value::F32(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(*x as f64 % y))
            }
            (Value::F64(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(x % *y as f64))
            }
            // F32 <-> Bool mod
            (Value::F32(x), Value::Bool(y)) => {
                let y_val = if *y { 1.0f32 } else { 0.0f32 };
                if y_val == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32(x % y_val))
            }
            (Value::Bool(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32(if *x { 1.0f32 } else { 0.0f32 } % y))
            }
            // F16 mod operations (type preservation, Issue #1972)
            (Value::F16(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(f32::from(*x) % yf)))
            }
            (Value::F16(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(f32::from(*x) % *y as f32)))
            }
            (Value::I64(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(*x as f32 % yf)))
            }
            // F16 <-> F64 mixed mod promotes to F64
            (Value::F16(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(f64::from(*x) % y))
            }
            (Value::F64(x), Value::F16(y)) => {
                let yf = f64::from(*y);
                if yf == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64(x % yf))
            }
            // F16 <-> F32 mixed mod promotes to F32
            (Value::F16(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32(f32::from(*x) % y))
            }
            (Value::F32(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32(x % yf))
            }
            // F16 <-> Bool mod
            (Value::F16(x), Value::Bool(y)) => {
                let y_val = if *y {
                    half::f16::from_f32(1.0)
                } else {
                    half::f16::from_f32(0.0)
                };
                if y_val == half::f16::from_f32(0.0) {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(
                    f32::from(*x) % f32::from(y_val),
                )))
            }
            (Value::Bool(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(
                    if *x { 1.0f32 } else { 0.0f32 } % yf,
                )))
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot compute modulo of {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    /// Dynamic integer division (div/÷) with type preservation (Issue #1970).
    /// In Julia, `div(Float32(x), Float32(y))` returns `Float32(floor(x/y))`.
    #[inline]
    pub(super) fn dynamic_int_div(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            (Value::I64(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(x.div_euclid(*y)))
            }
            (Value::F64(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((x / y).floor()))
            }
            (Value::I64(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((*x as f64 / y).floor()))
            }
            (Value::F64(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((x / *y as f64).floor()))
            }
            // Bool as Int64
            (Value::Bool(x), Value::Bool(y)) => {
                let y_int = if *y { 1 } else { 0 };
                if y_int == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(if *x { 1 } else { 0 } / y_int))
            }
            (Value::Bool(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(if *x { 1 } else { 0 } / y))
            }
            (Value::I64(x), Value::Bool(y)) => {
                let y_int = if *y { 1 } else { 0 };
                if y_int == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::I64(x / y_int))
            }
            (Value::Bool(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((if *x { 1.0 } else { 0.0f64 } / y).floor()))
            }
            (Value::F64(x), Value::Bool(y)) => {
                let y_val = if *y { 1.0 } else { 0.0 };
                if y_val == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((x / y_val).floor()))
            }
            // F32 int div operations (type preservation)
            (Value::F32(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32((x / y).floor()))
            }
            (Value::F32(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32((x / *y as f32).floor()))
            }
            (Value::I64(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32((*x as f32 / y).floor()))
            }
            // F32 <-> F64 mixed int div promotes to F64
            (Value::F32(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((*x as f64 / y).floor()))
            }
            (Value::F64(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((x / *y as f64).floor()))
            }
            // F32 <-> Bool int div
            (Value::F32(x), Value::Bool(y)) => {
                let y_val = if *y { 1.0f32 } else { 0.0f32 };
                if y_val == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32((x / y_val).floor()))
            }
            (Value::Bool(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32((if *x { 1.0f32 } else { 0.0f32 } / y).floor()))
            }
            // F16 int div operations (type preservation, Issue #1972)
            (Value::F16(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(
                    (f32::from(*x) / yf).floor(),
                )))
            }
            (Value::F16(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(
                    (f32::from(*x) / *y as f32).floor(),
                )))
            }
            (Value::I64(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32((*x as f32 / yf).floor())))
            }
            // F16 <-> F64 mixed int div promotes to F64
            (Value::F16(x), Value::F64(y)) => {
                if *y == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((f64::from(*x) / y).floor()))
            }
            (Value::F64(x), Value::F16(y)) => {
                let yf = f64::from(*y);
                if yf == 0.0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F64((x / yf).floor()))
            }
            // F16 <-> F32 mixed int div promotes to F32
            (Value::F16(x), Value::F32(y)) => {
                if *y == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32((f32::from(*x) / y).floor()))
            }
            (Value::F32(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F32((x / yf).floor()))
            }
            // F16 <-> Bool int div
            (Value::F16(x), Value::Bool(y)) => {
                let y_val = if *y { 1.0f32 } else { 0.0f32 };
                if y_val == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(
                    (f32::from(*x) / y_val).floor(),
                )))
            }
            (Value::Bool(x), Value::F16(y)) => {
                let yf = f32::from(*y);
                if yf == 0.0f32 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::F16(half::f16::from_f32(
                    (if *x { 1.0f32 } else { 0.0f32 } / yf).floor(),
                )))
            }
            // BigInt integer division (Issue #2383)
            (Value::BigInt(x), Value::BigInt(y)) => {
                let zero = num_bigint::BigInt::from(0);
                if *y == zero {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::BigInt(x / y))
            }
            (Value::BigInt(x), Value::I64(y)) => {
                if *y == 0 {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::BigInt(x / num_bigint::BigInt::from(*y)))
            }
            (Value::I64(x), Value::BigInt(y)) => {
                let zero = num_bigint::BigInt::from(0);
                if y.as_inner() == &zero {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::BigInt(
                    (num_bigint::BigInt::from(*x) / y.as_inner().clone()).into(),
                ))
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot compute integer division of {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    /// Dynamic negation with type preservation.
    ///
    /// Issue #4789: narrow signed integers (`I8`/`I16`/`I32`/`I128`) use
    /// `wrapping_neg` to match upstream Julia's two's-complement
    /// semantics where `-typemin(IntN) == typemin(IntN)`. Without this,
    /// `abs(Int8(-128))` (which lowers to `-x` via the generic
    /// `abs(x) = -x if signbit(x) else x` in base/number.jl) crashed
    /// because the narrow variants fell through to the error arm.
    /// `I64` already wraps via Rust's debug/release behavior on a plain
    /// `-x`, but using `wrapping_neg` everywhere makes the intent
    /// explicit and aligns release/debug behavior.
    #[inline]
    pub(super) fn dynamic_neg(&mut self, a: &Value) -> Result<Value, VmError> {
        if let Some(arr_ref) = native_array_value_ref(a) {
            return self.dynamic_neg_array_value(arr_ref.borrow().clone());
        }
        if let Some(arr) = array_wrapper_value_to_array_value(a, &self.struct_heap)? {
            return self.dynamic_neg_array_value(arr);
        }
        if let Value::Memory(mem) = a {
            let values = {
                let mem = mem.borrow();
                (1..=mem.len())
                    .map(|idx| mem.get(idx))
                    .collect::<Result<Vec<_>, _>>()?
            };
            let shape = vec![values.len()];
            let negated = values
                .iter()
                .map(|value| self.dynamic_neg(value))
                .collect::<Result<Vec<_>, _>>()?;
            let mut result =
                ArrayValue::memory_first_collect_values(negated, ArrayElementType::Any)?;
            result.shape = shape;
            return self.array_value_to_wrapper(result);
        }

        match a {
            Value::I8(x) => Ok(Value::I8(x.wrapping_neg())),
            Value::I16(x) => Ok(Value::I16(x.wrapping_neg())),
            Value::I32(x) => Ok(Value::I32(x.wrapping_neg())),
            Value::I64(x) => Ok(Value::I64(x.wrapping_neg())),
            Value::I128(x) => Ok(Value::I128(x.wrapping_neg())),
            Value::F64(x) => Ok(Value::F64(-x)),
            Value::F32(x) => Ok(Value::F32(-x)),
            Value::F16(x) => Ok(Value::F16(-*x)),
            // -Bool -> Int64 (Julia semantics: -true == -1, -false == 0)
            Value::Bool(x) => Ok(Value::I64(if *x { -1 } else { 0 })),
            Value::BigInt(x) => Ok(Value::BigInt(-x.clone())),
            Value::BigFloat(x) => Ok(Value::BigFloat(-x.clone())),
            // Complex/Rational negation is handled by Julia dispatch (Issue #2433)
            _ => Err(VmError::TypeError(format!(
                "Cannot negate {:?}",
                self.value_type_name(a)
            ))),
        }
    }

    fn dynamic_neg_array_value(&mut self, arr: ArrayValue) -> Result<Value, VmError> {
        let shape = arr.shape.clone();
        let empty_element_type = negated_array_empty_element_type(&arr.element_type());
        let values = (0..arr.element_count())
            .map(|idx| {
                arr.get_linear(idx)
                    .and_then(|value| self.dynamic_neg(&value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut result = ArrayValue::memory_first_collect_values(values, empty_element_type)?;
        result.shape = shape;
        self.array_value_to_wrapper(result)
    }

    /// Dynamic power with type promotion.
    #[inline]
    pub(super) fn dynamic_pow(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        if let Some(result) = dynamic_integer_pow(a, b)? {
            return Ok(result);
        }
        if let Some(result) = dynamic_bigint_pow(a, b)? {
            return Ok(result);
        }
        if let Some(result) = dynamic_float_bigint_pow(a, b) {
            return Ok(result);
        }

        match (a, b) {
            (Value::F64(x), Value::F64(y)) => Ok(Value::F64(pow_f64(*x, *y))),
            (Value::I64(x), Value::F64(y)) => Ok(Value::F64(pow_f64(*x as f64, *y))),
            (Value::F64(x), Value::I64(y)) => Ok(Value::F64(pow_f64(*x, *y as f64))),
            // F32 ^ F32 → F32 (type preservation)
            (Value::F32(x), Value::F32(y)) => Ok(Value::F32((*x as f64).powf(*y as f64) as f32)),
            // F32 <-> I64 → F32 (follows promotion rules)
            (Value::F32(x), Value::I64(y)) => Ok(Value::F32((*x as f64).powf(*y as f64) as f32)),
            (Value::I64(x), Value::F32(y)) => Ok(Value::F32((*x as f64).powf(*y as f64) as f32)),
            // F32 <-> F64 → F64 (mixed promotion to F64)
            (Value::F32(x), Value::F64(y)) => Ok(Value::F64(pow_f64(*x as f64, *y))),
            (Value::F64(x), Value::F32(y)) => Ok(Value::F64(pow_f64(*x, *y as f64))),
            // F32 <-> Bool → F32
            (Value::F32(x), Value::Bool(y)) => {
                let e: f64 = if *y { 1.0 } else { 0.0 };
                Ok(Value::F32((*x as f64).powf(e) as f32))
            }
            (Value::Bool(x), Value::F32(y)) => {
                let b: f64 = if *x { 1.0 } else { 0.0 };
                Ok(Value::F32(b.powf(*y as f64) as f32))
            }
            // F16 ^ F16 → F16 (type preservation, Issue #1972)
            (Value::F16(x), Value::F16(y)) => {
                let result = (f64::from(*x)).powf(f64::from(*y));
                Ok(Value::F16(half::f16::from_f64(result)))
            }
            // F16 <-> I64 → F16
            (Value::F16(x), Value::I64(y)) => {
                let result = (f64::from(*x)).powf(*y as f64);
                Ok(Value::F16(half::f16::from_f64(result)))
            }
            (Value::I64(x), Value::F16(y)) => {
                let result = (*x as f64).powf(f64::from(*y));
                Ok(Value::F16(half::f16::from_f64(result)))
            }
            // F16 <-> F64 → F64 (mixed promotion)
            (Value::F16(x), Value::F64(y)) => Ok(Value::F64(pow_f64(f64::from(*x), *y))),
            (Value::F64(x), Value::F16(y)) => Ok(Value::F64(pow_f64(*x, f64::from(*y)))),
            // F16 <-> F32 → F32 (mixed promotion)
            (Value::F16(x), Value::F32(y)) => {
                Ok(Value::F32((f64::from(*x)).powf(*y as f64) as f32))
            }
            (Value::F32(x), Value::F16(y)) => {
                Ok(Value::F32((*x as f64).powf(f64::from(*y)) as f32))
            }
            // F16 <-> Bool → F16
            (Value::F16(x), Value::Bool(y)) => {
                let e: f64 = if *y { 1.0 } else { 0.0 };
                Ok(Value::F16(half::f16::from_f64((f64::from(*x)).powf(e))))
            }
            (Value::Bool(x), Value::F16(y)) => {
                let b: f64 = if *x { 1.0 } else { 0.0 };
                Ok(Value::F16(half::f16::from_f64(b.powf(f64::from(*y)))))
            }
            (Value::Bool(base), Value::F64(exp)) => {
                let b: f64 = if *base { 1.0 } else { 0.0 };
                Ok(Value::F64(pow_f64(b, *exp)))
            }
            (Value::F64(base), Value::Bool(exp)) => {
                let e: f64 = if *exp { 1.0 } else { 0.0 };
                Ok(Value::F64(pow_f64(*base, e)))
            }
            _ if irrational_f64_from_value(self, a).is_some()
                || irrational_f64_from_value(self, b).is_some() =>
            {
                Ok(Value::F64(pow_f64(
                    self.convert_to_f64(a)?,
                    self.convert_to_f64(b)?,
                )))
            }
            // BigFloat ^ <real numeric> → BigFloat (Issue #6790). Computed
            // inline here because runtime `^` dispatch has no terminating
            // BigFloat method and infinite-recurses; `should_use_inline_dynamic_pow`
            // routes these here. Complex/Rational exponents are excluded by
            // `is_bigfloat_pow` and keep going through Julia dispatch.
            _ if is_bigfloat_pow(a, b) => {
                let mut consts = astro_float::Consts::new().map_err(|e| {
                    VmError::InternalError(format!("Failed to initialize BigFloat constants: {e}"))
                })?;
                let (Some(base), Some(exp)) = (
                    value_to_bigfloat_for_pow(a, &mut consts),
                    value_to_bigfloat_for_pow(b, &mut consts),
                ) else {
                    return Err(VmError::TypeError(format!(
                        "Cannot compute power of {:?} and {:?}",
                        self.value_type_name(a),
                        self.value_type_name(b)
                    )));
                };
                Ok(Value::BigFloat(base.pow(
                    &exp,
                    crate::vm::value::BIGFLOAT_PRECISION,
                    crate::vm::value::BigFloatRoundingMode::ToEven,
                    &mut consts,
                )))
            }
            // Complex and Rational power is handled by Julia dispatch
            _ => Err(VmError::TypeError(format!(
                "Cannot compute power of {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    // === Helper methods for Complex number operations ===

    /// Check if a struct instance is a Complex number.
    pub(super) fn is_complex(&self, s: &StructInstance) -> bool {
        if let Some(def) = self.struct_defs.get(s.type_id) {
            is_complex_type_name(&def.name)
        } else {
            false
        }
    }

    /// Get a human-readable type name for error messages.
    fn value_type_name(&self, v: &Value) -> String {
        match v {
            Value::I64(_) => "Int64".to_string(),
            Value::F64(_) => "Float64".to_string(),
            Value::Bool(_) => "Bool".to_string(),
            Value::Str(_) => "String".to_string(),
            Value::Struct(s) => {
                if let Some(def) = self.struct_defs.get(s.type_id) {
                    def.name.clone()
                } else {
                    "Struct".to_string()
                }
            }
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    if let Some(def) = self.struct_defs.get(s.type_id) {
                        def.name.clone()
                    } else if !s.struct_name.is_empty() {
                        s.struct_name.to_string()
                    } else {
                        "Struct".to_string()
                    }
                } else {
                    "Struct".to_string()
                }
            }
            _ => format!("{:?}", v),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rng::StableRng;
    use crate::vm::value::{
        array_wrapper_value_to_array_value, new_memory_ref, ArrayElementType, MemoryRefValue,
        MemoryValue, StructInstance, TupleValue, Value,
    };
    use crate::vm::Vm;

    #[test]
    fn real_scalar_mul_complex_array_wrapper_preserves_complex_elements() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.struct_heap.push(StructInstance::complex(7, 1.0, 2.0));
        vm.struct_heap.push(StructInstance::complex(7, 3.0, 4.0));

        let mut memory = MemoryValue::undef_typed(&ArrayElementType::ComplexF64, 2);
        memory.set(1, Value::StructRef(0)).unwrap();
        memory.set(2, Value::StructRef(1)).unwrap();
        let mem_ref = Value::MemoryRef(Box::new(MemoryRefValue::first(new_memory_ref(memory))));
        let size = Value::Tuple(TupleValue::new(vec![Value::I64(2)]));
        let wrapper_idx = vm.struct_heap.len();
        vm.struct_heap.push(StructInstance::with_name(
            0,
            "Array{Complex{Float64},1}".to_string(),
            vec![mem_ref, size],
        ));
        let broadcastable =
            super::helpers::broadcastable_array_like(&vm, &Value::StructRef(wrapper_idx)).unwrap();
        assert!(broadcastable.is_complex());

        let result = vm
            .dynamic_mul(&Value::F64(2.0), &Value::StructRef(wrapper_idx))
            .unwrap();
        // `dynamic_mul` now produces the `Array{T,N}` wrapper (Issue #6806);
        // materialize it back to an `ArrayValue` for the asserts.
        let arr = array_wrapper_value_to_array_value(&result, &vm.struct_heap)
            .unwrap()
            .expect("dynamic_mul of a complex array should produce an Array wrapper");

        assert_eq!(arr.element_type(), ArrayElementType::ComplexF64);
        assert_eq!(arr.len(), 4);
        assert_eq!(arr.element_count(), 2);
        assert_eq!(
            arr.element_type_override,
            Some(ArrayElementType::ComplexF64)
        );
    }
}
