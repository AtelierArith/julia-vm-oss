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
use super::value::{BigFloatRoundingMode, RustBigFloat, RustBigInt, Value, BIGFLOAT_PRECISION};
use super::Vm;

use num_traits::{Signed, ToPrimitive, Zero};

/// Apply a unary float operation preserving the input type (Issue #2221).
/// F16 → F16, F32 → F32, other → F64.
/// This replaces the repeated match pattern in SqrtLlvm, FloorLlvm, etc.
/// Also used by CallDynamicOrBuiltin in call_dynamic.rs (Issue #2284).
pub(crate) fn apply_unary_float_op(val: Value, op: fn(f64) -> f64) -> Result<Value, VmError> {
    match val {
        Value::F16(a) => Ok(Value::F16(half::f16::from_f64(op(a.to_f64())))),
        Value::F32(a) => Ok(Value::F32(op(a as f64) as f32)),
        other => {
            let a = value_to_f64(&other)?;
            Ok(Value::F64(op(a)))
        }
    }
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

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_intrinsic(&mut self, intrinsic: Intrinsic) -> Result<(), VmError> {
        match intrinsic {
            // === Integer Arithmetic ===
            Intrinsic::NegInt => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(-a));
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
                let b = self.stack.pop_i64()?;
                if b == 0 {
                    return Err(VmError::DivisionByZero);
                }
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a / b));
            }
            Intrinsic::SremInt => {
                // Check for Float32 operands to preserve type (same pattern as AddFloat etc.)
                let b_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let a_val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match (&a_val, &b_val) {
                    (Value::F32(a), Value::F32(b)) => {
                        if *b == 0.0f32 {
                            return Err(VmError::DivisionByZero);
                        }
                        self.stack.push(Value::F32(a % b));
                    }
                    (Value::F64(_) | Value::F32(_), _) | (_, Value::F64(_) | Value::F32(_)) => {
                        let a = value_to_f64(&a_val)?;
                        let b = value_to_f64(&b_val)?;
                        if b == 0.0 {
                            return Err(VmError::DivisionByZero);
                        }
                        self.stack.push(Value::F64(a % b));
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
                        self.stack.push(Value::I64(a % b));
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
                // Negate value preserving its type (I64 stays I64, F16 stays F16, F32 stays F32, F64 stays F64, BigInt stays BigInt)
                match self.stack.pop_value()? {
                    Value::I64(a) => self.stack.push(Value::I64(-a)),
                    Value::F16(a) => self.stack.push(Value::F16(-a)),
                    Value::F32(a) => self.stack.push(Value::F32(-a)),
                    Value::F64(a) => self.stack.push(Value::F64(-a)),
                    Value::BigInt(a) => self.stack.push(Value::BigInt(-a)),
                    Value::BigFloat(a) => self.stack.push(Value::BigFloat(-a)),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "neg_any: expected numeric, got {:?}",
                            other
                        )))
                    }
                }
            }
            Intrinsic::AddFloat => {
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
            Intrinsic::SubFloat => {
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
            Intrinsic::MulFloat => {
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
            Intrinsic::DivFloat => {
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
            Intrinsic::PowFloat => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(a.powf(b)));
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
            Intrinsic::AndInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a & b));
            }
            Intrinsic::OrInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a | b));
            }
            Intrinsic::XorInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a ^ b));
            }
            Intrinsic::NotInt => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(!a));
            }
            Intrinsic::ShlInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a << (b as u32)));
            }
            Intrinsic::LshrInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()? as u64;
                self.stack.push(Value::I64((a >> (b as u32)) as i64));
            }
            Intrinsic::AshrInt => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a >> (b as u32)));
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
                self.stack.push(apply_unary_float_op(val, f64::sqrt)?);
            }
            Intrinsic::FloorLlvm => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_float_op(val, f64::floor)?);
            }
            Intrinsic::CeilLlvm => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_float_op(val, f64::ceil)?);
            }
            Intrinsic::TruncLlvm => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_float_op(val, f64::trunc)?);
            }
            Intrinsic::AbsFloat => {
                let val = self.stack.pop_value()?;
                self.stack.push(apply_unary_float_op(val, f64::abs)?);
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
                // BigInt power: base^exp where exp is Int64
                // Pop exponent first (stack order is reversed)
                let exp = self.stack.pop_i64()?;
                let base = self.stack.pop_bigint()?;
                if exp < 0 {
                    // Negative exponents would result in fractions, not supported for integers
                    return Err(VmError::DomainError(
                        "BigInt power with negative exponent not supported".to_string(),
                    ));
                }
                let result = base.pow(exp as u32);
                self.stack.push(Value::BigInt(result));
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
                self.stack.push(Value::Str(a.to_string()));
            }

            // === BigFloat Arithmetic ===
            Intrinsic::NegBigFloat => {
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::BigFloat(a.neg()));
            }
            Intrinsic::AddBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::BigFloat(a.add(
                    &b,
                    BIGFLOAT_PRECISION,
                    BigFloatRoundingMode::ToEven,
                )));
            }
            Intrinsic::SubBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::BigFloat(a.sub(
                    &b,
                    BIGFLOAT_PRECISION,
                    BigFloatRoundingMode::ToEven,
                )));
            }
            Intrinsic::MulBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::BigFloat(a.mul(
                    &b,
                    BIGFLOAT_PRECISION,
                    BigFloatRoundingMode::ToEven,
                )));
            }
            Intrinsic::DivBigFloat => {
                let b = self.stack.pop_bigfloat()?;
                if b.is_zero() {
                    return Err(VmError::DivisionByZero);
                }
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::BigFloat(a.div(
                    &b,
                    BIGFLOAT_PRECISION,
                    BigFloatRoundingMode::ToEven,
                )));
            }
            Intrinsic::AbsBigFloat => {
                let a = self.stack.pop_bigfloat()?;
                self.stack.push(Value::BigFloat(a.abs()));
            }

            // === BigFloat Comparisons - return Bool (Julia semantics) ===
            // cmp() returns Option<i128>: positive if a > b, negative if a < b, 0 if equal, None if NaN
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
                    BIGFLOAT_PRECISION,
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
                    BIGFLOAT_PRECISION,
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
                self.stack.push(Value::Str(a.to_string()));
            }
        }
        Ok(())
    }
}
