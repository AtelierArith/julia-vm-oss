//! Stack operations trait for the VM.
//!
//! This module provides the `StackOps` trait that consolidates common stack
//! pop operations, reducing code duplication across builtin implementations.

// SAFETY: i64/i128/i32/i16/i8→usize casts in pop_usize and pop_array are
// all guarded by `if v >= 0` match guards before the cast.
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::error::VmError;
use super::util::value_type_name;
use super::value::{
    get_bigfloat_precision, native_array_ref_from_value, new_array_ref, ArrayRef, RangeValue,
    RustBigFloat, RustBigInt, StructInstance, Value,
};

/// Trait for stack operations, providing typed pop methods.
///
/// This trait is implemented for `Vec<Value>` to provide convenient,
/// type-checked stack operations that reduce boilerplate in builtin
/// implementations.
///
/// # Example
/// ```
/// use subset_julia_vm::vm::stack_ops::StackOps;
/// use subset_julia_vm::vm::value::Value;
///
/// let mut stack: Vec<Value> = vec![Value::I64(42)];
/// let val = stack.pop_i64().unwrap();
/// assert_eq!(val, 42);
/// ```
pub trait StackOps {
    /// Pop an i64 value from the stack.
    fn pop_i64(&mut self) -> Result<i64, VmError>;

    /// Pop an f64 value from the stack.
    fn pop_f64(&mut self) -> Result<f64, VmError>;

    /// Pop a string value from the stack.
    fn pop_str(&mut self) -> Result<String, VmError>;

    /// Pop a boolean condition value from the stack.
    /// Accepts Bool or I64 (0 = false, non-zero = true).
    fn pop_condition(&mut self) -> Result<bool, VmError>;

    /// Pop a boolean value from the stack.
    fn pop_bool(&mut self) -> Result<bool, VmError>;

    /// Pop a char value from the stack.
    fn pop_char(&mut self) -> Result<char, VmError>;

    /// Pop an array reference from the stack.
    /// Also handles automatic Range -> Array conversion.
    fn pop_array(&mut self) -> Result<ArrayRef, VmError>;

    /// Pop a Range value from the stack.
    fn pop_range(&mut self) -> Result<RangeValue, VmError>;

    /// Pop a BigInt from the stack, promoting any primitive integer
    /// (Bool, I8/16/32/64/128, U8/16/32/64/128) to BigInt.
    fn pop_bigint(&mut self) -> Result<RustBigInt, VmError>;

    /// Pop a BigFloat from the stack, promoting any primitive numeric value
    /// (Bool, all integer widths, F16/F32/F64, BigInt) to BigFloat.
    fn pop_bigfloat(&mut self) -> Result<RustBigFloat, VmError>;

    /// Pop any value from the stack.
    fn pop_value(&mut self) -> Result<Value, VmError>;

    /// Pop a numeric value as f64 (accepts F64, F32, I64, I32).
    fn pop_numeric_as_f64(&mut self) -> Result<f64, VmError>;

    /// Pop an unsigned integer, accepting U8, U16, U32, U64, or I64 (if non-negative).
    fn pop_usize(&mut self) -> Result<usize, VmError>;
}

impl StackOps for Vec<Value> {
    #[inline]
    fn pop_i64(&mut self) -> Result<i64, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::I64(v) => Ok(v),
            // Bool is a subtype of Integer in Julia, so accept it as i64
            // (false = 0, true = 1)
            Value::Bool(v) => Ok(if v { 1 } else { 0 }),
            // Accept narrow integer types, widening to i64
            // This is needed because the compiler may emit StoreI64/LoadI64 for variables
            // that hold narrow integer values (Int8, Int16, Int32, etc.)
            Value::I32(v) => Ok(v as i64),
            Value::I16(v) => Ok(v as i64),
            Value::I8(v) => Ok(v as i64),
            Value::I128(v) => Ok(v as i64),
            Value::U8(v) => Ok(v as i64),
            Value::U16(v) => Ok(v as i64),
            Value::U32(v) => Ok(v as i64),
            Value::U64(v) => Ok(v as i64),
            Value::U128(v) => Ok(v as i64),
            other => Err(VmError::TypeError(format!(
                "expected I64, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_f64(&mut self) -> Result<f64, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::F64(v) => Ok(v),
            Value::F32(v) => Ok(v as f64),
            Value::I64(v) => Ok(v as f64),
            other => Err(VmError::TypeError(format!(
                "expected F64, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_str(&mut self) -> Result<String, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::Str(s) => Ok(s.to_string()),
            Value::StrBytes(bytes) => Ok(String::from_utf8_lossy(bytes.as_ref()).into_owned()),
            other => Err(VmError::TypeError(format!(
                "expected String, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_condition(&mut self) -> Result<bool, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::Bool(v) => Ok(v),
            Value::I64(v) => Ok(v != 0),
            other => Err(VmError::TypeError(format!(
                "expected Bool or I64 for condition, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_bool(&mut self) -> Result<bool, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::Bool(v) => Ok(v),
            other => Err(VmError::TypeError(format!(
                "expected Bool, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_char(&mut self) -> Result<char, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::Char(c) => Ok(c),
            other => Err(VmError::TypeError(format!(
                "expected Char, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_array(&mut self) -> Result<ArrayRef, VmError> {
        let popped = self.pop().ok_or(VmError::StackUnderflow)?;
        match native_array_ref_from_value(popped) {
            Ok(arr) => Ok(arr),
            Err(Value::Range(r)) => {
                // Automatically collect Range to Array (needed for transpose,
                // etc.) through the same Memory-first range materialization
                // path used by public collect.
                Ok(new_array_ref(r.collect()))
            }
            Err(other) => Err(VmError::TypeError(format!(
                "expected Array, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_range(&mut self) -> Result<RangeValue, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::Range(r) => Ok(r),
            other => Err(VmError::TypeError(format!(
                "expected Range, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_bigint(&mut self) -> Result<RustBigInt, VmError> {
        // Issue #3748: accept every primitive integer Value variant. In Julia,
        // any integer + BigInt promotes to BigInt, so the runtime coercion
        // helper must mirror that — Bool, all signed widths (I8/16/32/64/128),
        // and all unsigned widths (U8/16/32/64/128).
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::BigInt(v) => Ok(v),
            Value::Bool(v) => Ok(RustBigInt::from(if v { 1u8 } else { 0u8 })),
            Value::I8(v) => Ok(RustBigInt::from(v)),
            Value::I16(v) => Ok(RustBigInt::from(v)),
            Value::I32(v) => Ok(RustBigInt::from(v)),
            Value::I64(v) => Ok(RustBigInt::from(v)),
            Value::I128(v) => Ok(RustBigInt::from(v)),
            Value::U8(v) => Ok(RustBigInt::from(v)),
            Value::U16(v) => Ok(RustBigInt::from(v)),
            Value::U32(v) => Ok(RustBigInt::from(v)),
            Value::U64(v) => Ok(RustBigInt::from(v)),
            Value::U128(v) => Ok(RustBigInt::from(v)),
            other => Err(VmError::TypeError(format!(
                "expected BigInt, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_bigfloat(&mut self) -> Result<RustBigFloat, VmError> {
        // Issue #3749: accept every primitive numeric Value variant. In Julia,
        // any numeric + BigFloat promotes to BigFloat, so the runtime coercion
        // helper must accept Bool, all integer widths, F16/F32/F64 and BigInt.
        // Float/Bool coercions allocate at the CURRENT default precision (the
        // active `setprecision` context), like the `BigFloat(x)` constructor
        // (Issue #9332). Integer operands are handled separately below: mixed
        // BigFloat×Integer operations keep the integer exact and round only the
        // final destination.
        let p = get_bigfloat_precision();
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::BigFloat(v) => Ok(v),
            Value::F64(v) => Ok(RustBigFloat::from_f64(v, p)),
            Value::F32(v) => Ok(RustBigFloat::from_f64(v as f64, p)),
            Value::F16(v) => Ok(RustBigFloat::from_f64(v.to_f64(), p)),
            Value::Bool(v) => Ok(RustBigFloat::from_f64(if v { 1.0 } else { 0.0 }, p)),
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
            other => Err(VmError::TypeError(format!(
                "expected BigFloat, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_value(&mut self) -> Result<Value, VmError> {
        self.pop().ok_or(VmError::StackUnderflow)
    }

    #[inline]
    fn pop_numeric_as_f64(&mut self) -> Result<f64, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::F64(v) => Ok(v),
            Value::F32(v) => Ok(v as f64),
            Value::F16(v) => Ok(v.to_f64()),
            Value::I64(v) => Ok(v as f64),
            Value::I128(v) => Ok(v as f64),
            Value::I32(v) => Ok(v as f64),
            Value::I16(v) => Ok(v as f64),
            Value::I8(v) => Ok(v as f64),
            Value::U64(v) => Ok(v as f64),
            Value::U128(v) => Ok(v as f64),
            Value::U32(v) => Ok(v as f64),
            Value::U16(v) => Ok(v as f64),
            Value::U8(v) => Ok(v as f64),
            Value::Bool(b) => Ok(if b { 1.0 } else { 0.0 }),
            other => Err(VmError::TypeError(format!(
                "expected numeric value, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_usize(&mut self) -> Result<usize, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::I64(v) if v >= 0 => Ok(v as usize),
            Value::I128(v) if v >= 0 => Ok(v as usize),
            Value::I32(v) if v >= 0 => Ok(v as usize),
            Value::I16(v) if v >= 0 => Ok(v as usize),
            Value::I8(v) if v >= 0 => Ok(v as usize),
            Value::U64(v) => Ok(v as usize),
            Value::U128(v) => Ok(v as usize),
            Value::U32(v) => Ok(v as usize),
            Value::U16(v) => Ok(v as usize),
            Value::U8(v) => Ok(v as usize),
            Value::I64(v) => Err(VmError::TypeError(format!(
                "expected non-negative integer, got {}",
                v
            ))),
            other => Err(VmError::TypeError(format!(
                "expected integer, got {:?}",
                value_type_name(&other)
            ))),
        }
    }
}

/// Convert an integer's decimal string to a `BigFloat` without pre-rounding the
/// integer operand. Mixed BigFloat×Integer operations pass the integer exactly
/// to MPFR and round only the final destination (Issues #9332, #9450).
fn bigfloat_from_integer_decimal_exact(
    s: &str,
    min_precision: usize,
) -> Result<RustBigFloat, VmError> {
    let mut consts = astro_float::Consts::new().map_err(|e| {
        VmError::InternalError(format!("Failed to initialize BigFloat constants: {}", e))
    })?;
    Ok(RustBigFloat::parse_integer_exact_decimal(
        s,
        min_precision,
        &mut consts,
    ))
}

/// Extended stack operations that require additional context (like struct_heap).
/// These are provided as associated functions rather than trait methods.
#[derive(Debug)]
pub struct StackOpsExt;

impl StackOpsExt {
    /// Pop a numeric value as f64, handling Rational structs and BigInt as well.
    #[inline]
    pub fn pop_f64_or_i64(
        st: &mut Vec<Value>,
        struct_heap: &[StructInstance],
    ) -> Result<f64, VmError> {
        match st.pop().ok_or(VmError::StackUnderflow)? {
            Value::F64(v) => Ok(v),
            Value::F32(v) => Ok(v as f64),
            Value::F16(v) => Ok(v.to_f64()),
            Value::I64(v) => Ok(v as f64),
            Value::I128(v) => Ok(v as f64),
            Value::I32(v) => Ok(v as f64),
            Value::I16(v) => Ok(v as f64),
            Value::I8(v) => Ok(v as f64),
            Value::U64(v) => Ok(v as f64),
            Value::U128(v) => Ok(v as f64),
            Value::U32(v) => Ok(v as f64),
            Value::U16(v) => Ok(v as f64),
            Value::U8(v) => Ok(v as f64),
            Value::Bool(b) => Ok(if b { 1.0 } else { 0.0 }),
            Value::BigInt(ref b) => {
                // Convert BigInt to F64 (may lose precision for large values)
                use num_traits::ToPrimitive;
                Ok(b.to_f64().unwrap_or(f64::INFINITY))
            }
            Value::Struct(s) => {
                if let Some(v) = s.as_irrational_f64() {
                    return Ok(v);
                }
                s.as_rational_parts_f64()
                    .map(|(num, den)| num / den)
                    .ok_or_else(|| {
                        VmError::TypeError(format!(
                            "expected numeric value, got {:?}",
                            value_type_name(&Value::Struct(s.clone()))
                        ))
                    })
            }
            Value::StructRef(idx) => {
                let s = struct_heap.get(idx).ok_or_else(|| {
                    VmError::TypeError(format!("invalid struct reference: {}", idx))
                })?;
                if let Some(v) = s.as_irrational_f64() {
                    return Ok(v);
                }
                s.as_rational_parts_f64()
                    .map(|(num, den)| num / den)
                    .ok_or_else(|| {
                        VmError::TypeError(format!(
                            "expected numeric value, got {:?}",
                            s.struct_name
                        ))
                    })
            }
            other => Err(VmError::TypeError(format!(
                "expected numeric value, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    /// Pop a complex number from the stack, handling promotion from real numbers.
    #[inline]
    pub fn pop_complex(
        st: &mut Vec<Value>,
        struct_heap: &[StructInstance],
    ) -> Result<(f64, f64), VmError> {
        match st.pop().ok_or(VmError::StackUnderflow)? {
            Value::F64(v) => Ok((v, 0.0)), // promote real to complex
            Value::I64(v) => Ok((v as f64, 0.0)),
            Value::Struct(s) if s.is_complex() => {
                // Handle inline Complex struct
                s.as_complex_parts().ok_or_else(|| {
                    VmError::TypeError("Complex struct has invalid fields".to_string())
                })
            }
            Value::StructRef(idx) => {
                // Handle Complex struct reference (from heap)
                let s = struct_heap.get(idx).ok_or_else(|| {
                    VmError::TypeError(format!("invalid struct reference: {}", idx))
                })?;
                if s.is_complex() {
                    s.as_complex_parts().ok_or_else(|| {
                        VmError::TypeError("Complex struct has invalid fields".to_string())
                    })
                } else {
                    Err(VmError::TypeError(format!(
                        "expected Complex, got {:?}",
                        s.struct_name
                    )))
                }
            }
            other => Err(VmError::TypeError(format!(
                "expected Complex, got {:?}",
                value_type_name(&other)
            ))),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::value::{new_memory_ref, ArrayElementType, MemoryValue};
    use super::*;

    #[test]
    fn test_pop_i64() {
        let mut stack = vec![Value::I64(42)];
        assert_eq!(stack.pop_i64().unwrap(), 42);
        assert!(stack.pop_i64().is_err()); // StackUnderflow
    }

    #[test]
    fn test_pop_i64_accepts_bool() {
        // Bool is a subtype of Integer in Julia, so pop_i64 should accept Bool
        // This is a regression test for Issue #1612
        let mut stack = vec![Value::Bool(true), Value::Bool(false)];
        assert_eq!(stack.pop_i64().unwrap(), 0); // false -> 0
        assert_eq!(stack.pop_i64().unwrap(), 1); // true -> 1
    }

    #[test]
    fn test_pop_f64() {
        let mut stack = vec![Value::F64(std::f64::consts::PI), Value::I64(42)];
        assert_eq!(stack.pop_f64().unwrap(), 42.0); // I64 promoted to f64
        assert_eq!(stack.pop_f64().unwrap(), std::f64::consts::PI);
    }

    #[test]
    fn test_pop_str() {
        let mut stack = vec![Value::str_new("hello".to_string())];
        assert_eq!(stack.pop_str().unwrap(), "hello");
    }

    #[test]
    fn test_pop_condition() {
        let mut stack = vec![Value::Bool(true), Value::I64(0), Value::I64(1)];
        assert!(stack.pop_condition().unwrap()); // I64(1) -> true
        assert!(!stack.pop_condition().unwrap()); // I64(0) -> false
        assert!(stack.pop_condition().unwrap()); // Bool(true) -> true
    }

    #[test]
    fn test_pop_numeric_as_f64() {
        let mut stack = vec![
            Value::F64(1.5),
            Value::F32(2.5),
            Value::I64(3),
            Value::U8(4),
        ];
        assert_eq!(stack.pop_numeric_as_f64().unwrap(), 4.0);
        assert_eq!(stack.pop_numeric_as_f64().unwrap(), 3.0);
        assert_eq!(stack.pop_numeric_as_f64().unwrap(), 2.5);
        assert_eq!(stack.pop_numeric_as_f64().unwrap(), 1.5);
    }

    #[test]
    fn test_pop_usize() {
        let mut stack = vec![Value::I64(10), Value::I64(-1)];
        assert!(stack.pop_usize().is_err()); // negative number
        assert_eq!(stack.pop_usize().unwrap(), 10);
    }

    #[test]
    fn test_pop_array_rejects_memory_without_array_bridge() {
        let mem = MemoryValue::undef_typed(&ArrayElementType::I64, 2);
        let mut stack = vec![Value::Memory(new_memory_ref(mem))];

        let err = stack
            .pop_array()
            .expect_err("Memory must not be converted to Array by pop_array");

        match err {
            VmError::TypeError(msg) => {
                assert!(
                    msg.contains("expected Array"),
                    "unexpected TypeError: {msg}"
                );
                assert!(msg.contains("Memory"), "unexpected TypeError: {msg}");
            }
            other => panic!("expected TypeError, got {other:?}"),
        }
    }

    #[test]
    fn test_pop_array_collects_integer_range_with_memory_first_element_type() {
        let range = RangeValue::unit_range(1.0, 3.0);
        let mut stack = vec![Value::Range(range)];

        let arr = stack.pop_array().unwrap();
        let arr = arr.borrow();

        assert_eq!(arr.shape, vec![3]);
        assert_eq!(arr.element_type(), ArrayElementType::I64);
        let values = arr.to_logical_value_vec().unwrap();
        let ints = values
            .into_iter()
            .map(|value| match value {
                Value::I64(n) => n,
                other => panic!("expected Int64 range element, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(ints, vec![1, 2, 3]);
    }
}
