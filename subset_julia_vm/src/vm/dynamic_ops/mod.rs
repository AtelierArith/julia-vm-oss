//! Dynamic arithmetic operations for runtime type dispatch.
//!
//! These operations implement Julia's type promotion rules at runtime,
//! used when parameter types are not known at compile time.

// SAFETY: i64→u32 casts in pow operations are guarded by `if *exp < 0` checks.
#![allow(clippy::cast_sign_loss)]

mod dispatch;
mod helpers;

use crate::rng::RngLike;

use super::error::VmError;
use super::value::{StructInstance, Value};
use super::Vm;
use helpers::normalize_memory;

impl<R: RngLike> Vm<R> {
    /// Convert a StructRefs-backed complex array to interleaved ComplexF64 format.
    /// Returns None if the array is not a StructRefs complex array.
    /// Used to bridge StructRefs complex arrays to the interleaved broadcast path (Issue #2690).
    fn to_interleaved_complex(
        &self,
        arr: &super::value::ArrayValue,
    ) -> Option<super::value::ArrayValue> {
        use super::value::{ArrayData, ArrayElementType, ArrayValue};
        match &arr.data {
            ArrayData::StructRefs(refs) => {
                let mut data = Vec::with_capacity(refs.len() * 2);
                for &idx in refs {
                    let s = self.struct_heap.get(idx)?;
                    let (re, im) = s.as_complex_parts()?;
                    data.push(re);
                    data.push(im);
                }
                Some(ArrayValue {
                    data: ArrayData::F64(data),
                    shape: arr.shape.clone(),
                    struct_type_id: None,
                    element_type_override: Some(ArrayElementType::ComplexF64),
                })
            }
            _ => None,
        }
    }

    /// Dynamic addition with type promotion.
    /// Follows Julia semantics: Int64 + Int64 → Int64, Int64 + Float64 → Float64
    #[inline]
    pub(super) fn dynamic_add(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        // Normalize Memory → Array (Issue #2764)
        let a = normalize_memory(a);
        let b = normalize_memory(b);
        let (a, b) = (&*a, &*b);
        // Complex and Rational arithmetic is handled by Julia dispatch
        match (a, b) {
            // Int64 + Int64 → Int64
            (Value::I64(x), Value::I64(y)) => Ok(Value::I64(x.wrapping_add(*y))),
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
            (Value::Array(arr_a), Value::Array(arr_b)) => {
                use super::broadcast::{
                    broadcast_op_complex, broadcast_op_f64, complex_add, Broadcastable,
                };
                let a_ref = arr_a.borrow();
                let b_ref = arr_b.borrow();
                // Convert StructRefs complex arrays to interleaved format (Issue #2690)
                let a_conv = self.to_interleaved_complex(&a_ref);
                let b_conv = self.to_interleaved_complex(&b_ref);
                let a_val = a_conv.as_ref().unwrap_or(&*a_ref);
                let b_val = b_conv.as_ref().unwrap_or(&*b_ref);
                let broadcastable_a = Broadcastable::Array(a_val.clone());
                let broadcastable_b = Broadcastable::Array(b_val.clone());
                // Use complex broadcast if either operand is complex
                let result = if broadcastable_a.is_complex() || broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_add)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x + y)?
                };
                Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(
                    result,
                ))))
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
    pub(super) fn dynamic_sub(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        // Normalize Memory → Array (Issue #2764)
        let a = normalize_memory(a);
        let b = normalize_memory(b);
        let (a, b) = (&*a, &*b);
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
            (Value::Array(arr_a), Value::Array(arr_b)) => {
                use super::broadcast::{
                    broadcast_op_complex, broadcast_op_f64, complex_sub, Broadcastable,
                };
                let a_ref = arr_a.borrow();
                let b_ref = arr_b.borrow();
                // Convert StructRefs complex arrays to interleaved format (Issue #2690)
                let a_conv = self.to_interleaved_complex(&a_ref);
                let b_conv = self.to_interleaved_complex(&b_ref);
                let a_val = a_conv.as_ref().unwrap_or(&*a_ref);
                let b_val = b_conv.as_ref().unwrap_or(&*b_ref);
                let broadcastable_a = Broadcastable::Array(a_val.clone());
                let broadcastable_b = Broadcastable::Array(b_val.clone());
                // Use complex broadcast if either operand is complex
                let result = if broadcastable_a.is_complex() || broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_sub)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x - y)?
                };
                Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(
                    result,
                ))))
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
    pub(super) fn dynamic_mul(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        // Normalize Memory → Array (Issue #2764)
        let a = normalize_memory(a);
        let b = normalize_memory(b);
        let (a, b) = (&*a, &*b);
        // Complex and Rational arithmetic is handled by Julia dispatch
        match (a, b) {
            (Value::I64(x), Value::I64(y)) => Ok(Value::I64(x.wrapping_mul(*y))),
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
            // Array * Array → element-wise multiplication
            (Value::Array(arr_a), Value::Array(arr_b)) => {
                use super::broadcast::{
                    broadcast_op_complex, broadcast_op_f64, complex_mul, Broadcastable,
                };
                let a_ref = arr_a.borrow();
                let b_ref = arr_b.borrow();
                // Convert StructRefs complex arrays to interleaved format (Issue #2690)
                let a_conv = self.to_interleaved_complex(&a_ref);
                let b_conv = self.to_interleaved_complex(&b_ref);
                let a_val = a_conv.as_ref().unwrap_or(&*a_ref);
                let b_val = b_conv.as_ref().unwrap_or(&*b_ref);
                let broadcastable_a = Broadcastable::Array(a_val.clone());
                let broadcastable_b = Broadcastable::Array(b_val.clone());
                // Use complex broadcast if either operand is complex
                let result = if broadcastable_a.is_complex() || broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_mul)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x * y)?
                };
                Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(
                    result,
                ))))
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
    pub(super) fn dynamic_div(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        // Normalize Memory → Array (Issue #2764)
        let a = normalize_memory(a);
        let b = normalize_memory(b);
        let (a, b) = (&*a, &*b);
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
                Ok(Value::BigInt(BigInt::from(*x) / y))
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
            (Value::Array(arr), scalar) => {
                use super::broadcast::{
                    broadcast_op_complex, broadcast_op_f64, complex_div, Broadcastable,
                };
                let arr_ref = arr.borrow();
                let broadcastable_a = Broadcastable::Array(arr_ref.clone());
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
                Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(
                    result,
                ))))
            }
            // Scalar / Array → element-wise division
            (scalar, Value::Array(arr)) => {
                use super::broadcast::{
                    broadcast_op_complex, broadcast_op_f64, complex_div, Broadcastable,
                };
                let arr_ref = arr.borrow();
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
                let broadcastable_b = Broadcastable::Array(arr_ref.clone());
                let result = if broadcastable_b.is_complex() {
                    broadcast_op_complex(&broadcastable_a, &broadcastable_b, complex_div)?
                } else {
                    broadcast_op_f64(&broadcastable_a, &broadcastable_b, |x, y| x / y)?
                };
                Ok(Value::Array(std::rc::Rc::new(std::cell::RefCell::new(
                    result,
                ))))
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
                if *y == zero {
                    return Err(VmError::DivisionByZero);
                }
                Ok(Value::BigInt(num_bigint::BigInt::from(*x) / y))
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot compute integer division of {:?} and {:?}",
                self.value_type_name(a),
                self.value_type_name(b)
            ))),
        }
    }

    /// Dynamic negation with type preservation.
    #[inline]
    pub(super) fn dynamic_neg(&self, a: &Value) -> Result<Value, VmError> {
        match a {
            Value::I64(x) => Ok(Value::I64(-x)),
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

    /// Dynamic power with type promotion.
    #[inline]
    pub(super) fn dynamic_pow(&self, a: &Value, b: &Value) -> Result<Value, VmError> {
        match (a, b) {
            // Int ^ Int → Int (for non-negative exponent)
            (Value::I64(base), Value::I64(exp)) => {
                if *exp < 0 {
                    // Negative exponent → Float64
                    Ok(Value::F64((*base as f64).powf(*exp as f64)))
                } else {
                    Ok(Value::I64(base.wrapping_pow(*exp as u32)))
                }
            }
            (Value::F64(x), Value::F64(y)) => Ok(Value::F64(x.powf(*y))),
            (Value::I64(x), Value::F64(y)) => Ok(Value::F64((*x as f64).powf(*y))),
            (Value::F64(x), Value::I64(y)) => Ok(Value::F64(x.powf(*y as f64))),
            // F32 ^ F32 → F32 (type preservation)
            (Value::F32(x), Value::F32(y)) => Ok(Value::F32((*x as f64).powf(*y as f64) as f32)),
            // F32 <-> I64 → F32 (follows promotion rules)
            (Value::F32(x), Value::I64(y)) => Ok(Value::F32((*x as f64).powf(*y as f64) as f32)),
            (Value::I64(x), Value::F32(y)) => Ok(Value::F32((*x as f64).powf(*y as f64) as f32)),
            // F32 <-> F64 → F64 (mixed promotion to F64)
            (Value::F32(x), Value::F64(y)) => Ok(Value::F64((*x as f64).powf(*y))),
            (Value::F64(x), Value::F32(y)) => Ok(Value::F64(x.powf(*y as f64))),
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
            (Value::F16(x), Value::F64(y)) => Ok(Value::F64((f64::from(*x)).powf(*y))),
            (Value::F64(x), Value::F16(y)) => Ok(Value::F64(x.powf(f64::from(*y)))),
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
            // Bool ^ Bool -> Bool (Julia semantics: x || !y, see base/bool.jl)
            (Value::Bool(x), Value::Bool(y)) => Ok(Value::Bool(*x || !*y)),
            (Value::Bool(base), Value::I64(exp)) => {
                let b = if *base { 1i64 } else { 0 };
                if *exp < 0 {
                    Ok(Value::F64((b as f64).powf(*exp as f64)))
                } else {
                    Ok(Value::I64(b.wrapping_pow(*exp as u32)))
                }
            }
            (Value::I64(base), Value::Bool(exp)) => {
                let e = if *exp { 1u32 } else { 0 };
                Ok(Value::I64(base.wrapping_pow(e)))
            }
            (Value::Bool(base), Value::F64(exp)) => {
                let b: f64 = if *base { 1.0 } else { 0.0 };
                Ok(Value::F64(b.powf(*exp)))
            }
            (Value::F64(base), Value::Bool(exp)) => {
                let e: f64 = if *exp { 1.0 } else { 0.0 };
                Ok(Value::F64(base.powf(e)))
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
            def.name == "Complex" || def.name.starts_with("Complex{")
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
                        s.struct_name.clone()
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
