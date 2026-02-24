//! Stack operations trait for the VM.
//!
//! This module provides the `StackOps` trait that consolidates common stack
//! pop operations, reducing code duplication across builtin implementations.

// SAFETY: i64/i128/i32/i16/i8→usize casts in pop_usize and pop_array are
// all guarded by `if v >= 0` match guards before the cast.
#![allow(clippy::cast_sign_loss)]

use super::error::VmError;
use super::util::value_type_name;
use super::value::{
    new_array_ref, ArrayRef, ArrayValue, DictValue, RangeValue, RustBigFloat, RustBigInt,
    StructInstance, Value, BIGFLOAT_PRECISION,
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

    /// Pop a Dict value from the stack.
    fn pop_dict(&mut self) -> Result<Box<DictValue>, VmError>;

    /// Pop a Range value from the stack.
    fn pop_range(&mut self) -> Result<RangeValue, VmError>;

    /// Pop a BigInt from the stack, promoting I64/I128 if needed.
    fn pop_bigint(&mut self) -> Result<RustBigInt, VmError>;

    /// Pop a BigFloat from the stack, promoting F64/I64 if needed.
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
            Value::Str(s) => Ok(s),
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
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::Array(arr) => Ok(arr),
            Value::Range(r) => {
                // Automatically collect Range to Array (needed for transpose, etc.)
                let len = r.length() as usize;
                let mut data = Vec::with_capacity(len);
                for i in 0..len {
                    data.push(r.start + (i as f64) * r.step);
                }
                Ok(new_array_ref(ArrayValue::from_f64(data, vec![len])))
            }
            // Memory → Array conversion (Issue #2764)
            Value::Memory(mem) => Ok(super::util::memory_to_array_ref(&mem)),
            other => Err(VmError::TypeError(format!(
                "expected Array, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_dict(&mut self) -> Result<Box<DictValue>, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::Dict(d) => Ok(d),
            other => Err(VmError::TypeError(format!(
                "expected Dict, got {:?}",
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
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::BigInt(v) => Ok(v),
            Value::I64(v) => Ok(RustBigInt::from(v)), // Promote I64 to BigInt
            Value::I128(v) => Ok(RustBigInt::from(v)), // Promote I128 to BigInt
            other => Err(VmError::TypeError(format!(
                "expected BigInt, got {:?}",
                value_type_name(&other)
            ))),
        }
    }

    #[inline]
    fn pop_bigfloat(&mut self) -> Result<RustBigFloat, VmError> {
        match self.pop().ok_or(VmError::StackUnderflow)? {
            Value::BigFloat(v) => Ok(v),
            Value::F64(v) => Ok(RustBigFloat::from_f64(v, BIGFLOAT_PRECISION)), // Promote F64 to BigFloat
            Value::I64(v) => Ok(RustBigFloat::from_f64(v as f64, BIGFLOAT_PRECISION)), // Promote I64 to BigFloat
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
            Value::Struct(s) => Self::rational_to_f64(&s).ok_or_else(|| {
                VmError::TypeError(format!(
                    "expected numeric value, got {:?}",
                    value_type_name(&Value::Struct(s.clone()))
                ))
            }),
            Value::StructRef(idx) => {
                let s = struct_heap.get(idx).ok_or_else(|| {
                    VmError::TypeError(format!("invalid struct reference: {}", idx))
                })?;
                Self::rational_to_f64(s).ok_or_else(|| {
                    VmError::TypeError(format!("expected numeric value, got {:?}", s.struct_name))
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

    /// Convert a Rational struct to f64.
    fn rational_to_f64(s: &StructInstance) -> Option<f64> {
        if !(s.struct_name == "Rational" || s.struct_name.starts_with("Rational{")) {
            return None;
        }
        let num = match s.values.first() {
            Some(Value::I64(v)) => *v as f64,
            Some(Value::I32(v)) => *v as f64,
            Some(Value::I16(v)) => *v as f64,
            Some(Value::I8(v)) => *v as f64,
            Some(Value::Bool(v)) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return None,
        };
        let den = match s.values.get(1) {
            Some(Value::I64(v)) => *v as f64,
            Some(Value::I32(v)) => *v as f64,
            Some(Value::I16(v)) => *v as f64,
            Some(Value::I8(v)) => *v as f64,
            Some(Value::Bool(v)) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return None,
        };
        Some(num / den)
    }
}

#[cfg(test)]
mod tests {
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
        let mut stack = vec![Value::Str("hello".to_string())];
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
}
