//! ArrayData - Type-segregated array storage for efficient operations.
//!
//! This module contains the `ArrayData` enum which holds homogeneous vectors
//! for each supported element type.

// SAFETY: i64→u8/u16/u32/u64 casts are all guarded by `if x >= 0` match guards
// (pattern `Value::I64(x) if x >= 0`) before the cast occurs.
#![allow(clippy::cast_sign_loss)]

use serde::{Deserialize, Serialize};

use super::super::error::VmError;
use super::array_element::ArrayElementType;
use super::Value;

/// Type-segregated array storage for efficient operations
/// Each variant holds a homogeneous vector of the corresponding type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArrayData {
    // Floating point types
    F32(Vec<f32>),
    F64(Vec<f64>),
    // Signed integer types
    I8(Vec<i8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    // Unsigned integer types
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    U64(Vec<u64>),
    // Other types
    Bool(Vec<bool>),
    String(Vec<std::string::String>),
    Char(Vec<char>),
    StructRefs(Vec<usize>),
    Any(Vec<Value>),
}

impl ArrayData {
    /// Get the element type of this data
    pub fn element_type(&self) -> ArrayElementType {
        match self {
            ArrayData::F32(_) => ArrayElementType::F32,
            ArrayData::F64(_) => ArrayElementType::F64,
            ArrayData::I8(_) => ArrayElementType::I8,
            ArrayData::I16(_) => ArrayElementType::I16,
            ArrayData::I32(_) => ArrayElementType::I32,
            ArrayData::I64(_) => ArrayElementType::I64,
            ArrayData::U8(_) => ArrayElementType::U8,
            ArrayData::U16(_) => ArrayElementType::U16,
            ArrayData::U32(_) => ArrayElementType::U32,
            ArrayData::U64(_) => ArrayElementType::U64,
            ArrayData::Bool(_) => ArrayElementType::Bool,
            ArrayData::String(_) => ArrayElementType::String,
            ArrayData::Char(_) => ArrayElementType::Char,
            ArrayData::StructRefs(_) => ArrayElementType::Struct,
            ArrayData::Any(_) => ArrayElementType::Any,
        }
    }

    /// Get the raw length (number of stored elements)
    pub fn raw_len(&self) -> usize {
        match self {
            ArrayData::F32(v) => v.len(),
            ArrayData::F64(v) => v.len(),
            ArrayData::I8(v) => v.len(),
            ArrayData::I16(v) => v.len(),
            ArrayData::I32(v) => v.len(),
            ArrayData::I64(v) => v.len(),
            ArrayData::U8(v) => v.len(),
            ArrayData::U16(v) => v.len(),
            ArrayData::U32(v) => v.len(),
            ArrayData::U64(v) => v.len(),
            ArrayData::Bool(v) => v.len(),
            ArrayData::String(v) => v.len(),
            ArrayData::Char(v) => v.len(),
            ArrayData::StructRefs(v) => v.len(),
            ArrayData::Any(v) => v.len(),
        }
    }

    /// Check if the data is empty
    pub fn is_empty(&self) -> bool {
        match self {
            ArrayData::F32(v) => v.is_empty(),
            ArrayData::F64(v) => v.is_empty(),
            ArrayData::I8(v) => v.is_empty(),
            ArrayData::I16(v) => v.is_empty(),
            ArrayData::I32(v) => v.is_empty(),
            ArrayData::I64(v) => v.is_empty(),
            ArrayData::U8(v) => v.is_empty(),
            ArrayData::U16(v) => v.is_empty(),
            ArrayData::U32(v) => v.is_empty(),
            ArrayData::U64(v) => v.is_empty(),
            ArrayData::Bool(v) => v.is_empty(),
            ArrayData::String(v) => v.is_empty(),
            ArrayData::Char(v) => v.is_empty(),
            ArrayData::StructRefs(v) => v.is_empty(),
            ArrayData::Any(v) => v.is_empty(),
        }
    }

    /// Sum all numeric elements as f64
    pub fn sum_as_f64(&self) -> f64 {
        match self {
            ArrayData::F32(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::F64(v) => v.iter().sum(),
            ArrayData::I8(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::I16(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::I32(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::I64(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U8(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U16(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U32(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::U64(v) => v.iter().map(|&x| x as f64).sum(),
            ArrayData::Bool(v) => v.iter().map(|&x| if x { 1.0 } else { 0.0 }).sum(),
            ArrayData::Any(v) => {
                // Sum boxed numeric values (from collect(map(...)))
                v.iter()
                    .map(|val| match val {
                        Value::I8(x) => *x as f64,
                        Value::I16(x) => *x as f64,
                        Value::I32(x) => *x as f64,
                        Value::I64(x) => *x as f64,
                        Value::U8(x) => *x as f64,
                        Value::U16(x) => *x as f64,
                        Value::U32(x) => *x as f64,
                        Value::U64(x) => *x as f64,
                        Value::F32(x) => *x as f64,
                        Value::F64(x) => *x,
                        Value::Bool(b) => {
                            if *b {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        _ => 0.0, // Non-numeric values contribute 0
                    })
                    .sum()
            }
            ArrayData::String(_) | ArrayData::Char(_) | ArrayData::StructRefs(_) => 0.0,
        }
    }

    /// Get a string representation of the type for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            ArrayData::F32(_) => "F32",
            ArrayData::F64(_) => "F64",
            ArrayData::I8(_) => "I8",
            ArrayData::I16(_) => "I16",
            ArrayData::I32(_) => "I32",
            ArrayData::I64(_) => "I64",
            ArrayData::U8(_) => "U8",
            ArrayData::U16(_) => "U16",
            ArrayData::U32(_) => "U32",
            ArrayData::U64(_) => "U64",
            ArrayData::Bool(_) => "Bool",
            ArrayData::String(_) => "String",
            ArrayData::Char(_) => "Char",
            ArrayData::StructRefs(_) => "StructRefs",
            ArrayData::Any(_) => "Any",
        }
    }

    /// Get a value at a linear index, converting to Value
    /// For StructRefs, returns Value::StructRef(heap_index)
    pub fn get_value(&self, index: usize) -> Option<Value> {
        match self {
            ArrayData::F32(v) => v.get(index).map(|&x| Value::F32(x)),
            ArrayData::F64(v) => v.get(index).map(|&x| Value::F64(x)),
            ArrayData::I8(v) => v.get(index).map(|&x| Value::I8(x)),
            ArrayData::I16(v) => v.get(index).map(|&x| Value::I16(x)),
            ArrayData::I32(v) => v.get(index).map(|&x| Value::I32(x)),
            ArrayData::I64(v) => v.get(index).map(|&x| Value::I64(x)),
            ArrayData::U8(v) => v.get(index).map(|&x| Value::U8(x)),
            ArrayData::U16(v) => v.get(index).map(|&x| Value::U16(x)),
            ArrayData::U32(v) => v.get(index).map(|&x| Value::U32(x)),
            ArrayData::U64(v) => v.get(index).map(|&x| Value::U64(x)),
            ArrayData::Bool(v) => v.get(index).map(|&x| Value::Bool(x)),
            ArrayData::String(v) => v.get(index).map(|x| Value::Str(x.clone())),
            ArrayData::Char(v) => v.get(index).map(|&x| Value::Char(x)),
            ArrayData::StructRefs(v) => v.get(index).map(|&idx| Value::StructRef(idx)),
            ArrayData::Any(v) => v.get(index).cloned(),
        }
    }

    /// Set a value at a linear index
    pub fn set_value(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        macro_rules! check_bounds {
            ($v:expr) => {
                if index >= $v.len() {
                    return Err(VmError::IndexOutOfBounds {
                        indices: vec![index as i64 + 1],
                        shape: vec![$v.len()],
                    });
                }
            };
        }
        match self {
            ArrayData::F32(v) => {
                check_bounds!(v);
                match value {
                    Value::F32(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::F64(x) => {
                        v[index] = x as f32;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as f32;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in F32 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::F64(v) => {
                check_bounds!(v);
                match value {
                    Value::F64(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::F32(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as f64;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in F64 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I8(v) => {
                check_bounds!(v);
                match value {
                    Value::I8(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as i8;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I8 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I16(v) => {
                check_bounds!(v);
                match value {
                    Value::I16(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as i16;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I16 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I32(v) => {
                check_bounds!(v);
                match value {
                    Value::I32(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x as i32;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I32 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::I64(v) => {
                check_bounds!(v);
                match value {
                    Value::I64(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::F64(x) if x.fract() == 0.0 => {
                        v[index] = x as i64;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in I64 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U8(v) => {
                check_bounds!(v);
                match value {
                    Value::U8(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u8;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U8 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U16(v) => {
                check_bounds!(v);
                match value {
                    Value::U16(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u16;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U16 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U32(v) => {
                check_bounds!(v);
                match value {
                    Value::U32(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u32;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U32 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::U64(v) => {
                check_bounds!(v);
                match value {
                    Value::U64(x) => {
                        v[index] = x;
                        Ok(())
                    }
                    Value::I64(x) if x >= 0 => {
                        v[index] = x as u64;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in U64 array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::Bool(v) => {
                check_bounds!(v);
                match value {
                    Value::Bool(b) => {
                        v[index] = b;
                        Ok(())
                    }
                    Value::I64(x) => {
                        v[index] = x != 0;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in Bool array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::String(v) => {
                check_bounds!(v);
                match value {
                    Value::Str(s) => {
                        v[index] = s;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in String array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::Char(v) => {
                check_bounds!(v);
                match value {
                    Value::Char(c) => {
                        v[index] = c;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in Char array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::StructRefs(v) => {
                check_bounds!(v);
                match value {
                    Value::StructRef(idx) => {
                        v[index] = idx;
                        Ok(())
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Cannot store {:?} in StructRefs array",
                        value.value_type()
                    ))),
                }
            }
            ArrayData::Any(v) => {
                check_bounds!(v);
                v[index] = value;
                Ok(())
            }
        }
    }

    /// Push a value to the end (for 1D arrays)
    pub fn push_value(&mut self, value: Value) -> Result<(), VmError> {
        match self {
            ArrayData::F32(v) => match value {
                Value::F32(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::F64(x) => {
                    v.push(x as f32);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as f32);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to F32 array",
                    value.value_type()
                ))),
            },
            ArrayData::F64(v) => match value {
                Value::F64(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::F32(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as f64);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to F64 array",
                    value.value_type()
                ))),
            },
            ArrayData::I8(v) => match value {
                Value::I8(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as i8);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I8 array",
                    value.value_type()
                ))),
            },
            ArrayData::I16(v) => match value {
                Value::I16(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as i16);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I16 array",
                    value.value_type()
                ))),
            },
            ArrayData::I32(v) => match value {
                Value::I32(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x as i32);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I32 array",
                    value.value_type()
                ))),
            },
            ArrayData::I64(v) => match value {
                Value::I64(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::F64(x) if x.fract() == 0.0 => {
                    v.push(x as i64);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to I64 array",
                    value.value_type()
                ))),
            },
            ArrayData::U8(v) => match value {
                Value::U8(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u8);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U8 array",
                    value.value_type()
                ))),
            },
            ArrayData::U16(v) => match value {
                Value::U16(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u16);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U16 array",
                    value.value_type()
                ))),
            },
            ArrayData::U32(v) => match value {
                Value::U32(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u32);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U32 array",
                    value.value_type()
                ))),
            },
            ArrayData::U64(v) => match value {
                Value::U64(x) => {
                    v.push(x);
                    Ok(())
                }
                Value::I64(x) if x >= 0 => {
                    v.push(x as u64);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to U64 array",
                    value.value_type()
                ))),
            },
            ArrayData::Bool(v) => match value {
                Value::Bool(b) => {
                    v.push(b);
                    Ok(())
                }
                Value::I64(x) => {
                    v.push(x != 0);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to Bool array",
                    value.value_type()
                ))),
            },
            ArrayData::String(v) => match value {
                Value::Str(s) => {
                    v.push(s);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to String array",
                    value.value_type()
                ))),
            },
            ArrayData::Char(v) => match value {
                Value::Char(c) => {
                    v.push(c);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to Char array",
                    value.value_type()
                ))),
            },
            ArrayData::StructRefs(v) => match value {
                Value::StructRef(idx) => {
                    v.push(idx);
                    Ok(())
                }
                _ => Err(VmError::TypeError(format!(
                    "Cannot push {:?} to StructRefs array",
                    value.value_type()
                ))),
            },
            ArrayData::Any(v) => {
                v.push(value);
                Ok(())
            }
        }
    }

    /// Pop a value from the end (for 1D arrays)
    pub fn pop_value(&mut self) -> Result<Value, VmError> {
        match self {
            ArrayData::F32(v) => v.pop().map(Value::F32).ok_or(VmError::EmptyArrayPop),
            ArrayData::F64(v) => v.pop().map(Value::F64).ok_or(VmError::EmptyArrayPop),
            ArrayData::I8(v) => v.pop().map(Value::I8).ok_or(VmError::EmptyArrayPop),
            ArrayData::I16(v) => v.pop().map(Value::I16).ok_or(VmError::EmptyArrayPop),
            ArrayData::I32(v) => v.pop().map(Value::I32).ok_or(VmError::EmptyArrayPop),
            ArrayData::I64(v) => v.pop().map(Value::I64).ok_or(VmError::EmptyArrayPop),
            ArrayData::U8(v) => v.pop().map(Value::U8).ok_or(VmError::EmptyArrayPop),
            ArrayData::U16(v) => v.pop().map(Value::U16).ok_or(VmError::EmptyArrayPop),
            ArrayData::U32(v) => v.pop().map(Value::U32).ok_or(VmError::EmptyArrayPop),
            ArrayData::U64(v) => v.pop().map(Value::U64).ok_or(VmError::EmptyArrayPop),
            ArrayData::Bool(v) => v.pop().map(Value::Bool).ok_or(VmError::EmptyArrayPop),
            ArrayData::String(v) => v.pop().map(Value::Str).ok_or(VmError::EmptyArrayPop),
            ArrayData::Char(v) => v.pop().map(Value::Char).ok_or(VmError::EmptyArrayPop),
            ArrayData::StructRefs(v) => v.pop().map(Value::StructRef).ok_or(VmError::EmptyArrayPop),
            ArrayData::Any(v) => v.pop().ok_or(VmError::EmptyArrayPop),
        }
    }

    /// Get a reference to the underlying f64 data (for backward compatibility)
    /// Returns None if not an F64 array
    pub fn as_f64_slice(&self) -> Option<&[f64]> {
        match self {
            ArrayData::F64(v) => Some(v),
            _ => None,
        }
    }

    /// Get a mutable reference to the underlying f64 data (for backward compatibility)
    pub fn as_f64_slice_mut(&mut self) -> Option<&mut Vec<f64>> {
        match self {
            ArrayData::F64(v) => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::array_element::ArrayElementType;

    // ── element_type ──────────────────────────────────────────────────────────

    #[test]
    fn test_element_type_f64() {
        assert_eq!(ArrayData::F64(vec![1.0]).element_type(), ArrayElementType::F64);
    }

    #[test]
    fn test_element_type_i64() {
        assert_eq!(ArrayData::I64(vec![1]).element_type(), ArrayElementType::I64);
    }

    #[test]
    fn test_element_type_bool() {
        assert_eq!(ArrayData::Bool(vec![true]).element_type(), ArrayElementType::Bool);
    }

    #[test]
    fn test_element_type_any() {
        assert_eq!(ArrayData::Any(vec![]).element_type(), ArrayElementType::Any);
    }

    // ── raw_len / is_empty ────────────────────────────────────────────────────

    #[test]
    fn test_raw_len_f64() {
        assert_eq!(ArrayData::F64(vec![1.0, 2.0, 3.0]).raw_len(), 3);
    }

    #[test]
    fn test_raw_len_empty() {
        assert_eq!(ArrayData::I64(vec![]).raw_len(), 0);
    }

    #[test]
    fn test_is_empty_true() {
        assert!(ArrayData::F64(vec![]).is_empty());
    }

    #[test]
    fn test_is_empty_false() {
        assert!(!ArrayData::I64(vec![42]).is_empty());
    }

    // ── type_name ─────────────────────────────────────────────────────────────

    #[test]
    fn test_type_name_f64() {
        assert_eq!(ArrayData::F64(vec![]).type_name(), "F64");
    }

    #[test]
    fn test_type_name_i64() {
        assert_eq!(ArrayData::I64(vec![]).type_name(), "I64");
    }

    #[test]
    fn test_type_name_bool() {
        assert_eq!(ArrayData::Bool(vec![]).type_name(), "Bool");
    }

    #[test]
    fn test_type_name_any() {
        assert_eq!(ArrayData::Any(vec![]).type_name(), "Any");
    }

    // ── sum_as_f64 ────────────────────────────────────────────────────────────

    #[test]
    fn test_sum_as_f64_integers() {
        let result = ArrayData::I64(vec![1, 2, 3]).sum_as_f64();
        assert!((result - 6.0).abs() < 1e-15, "sum should be 6.0, got {}", result);
    }

    #[test]
    fn test_sum_as_f64_floats() {
        let result = ArrayData::F64(vec![1.5, 2.5]).sum_as_f64();
        assert!((result - 4.0).abs() < 1e-15);
    }

    #[test]
    fn test_sum_as_f64_booleans_count_true() {
        // true=1.0, false=0.0
        let result = ArrayData::Bool(vec![true, false, true, true]).sum_as_f64();
        assert!((result - 3.0).abs() < 1e-15);
    }

    #[test]
    fn test_sum_as_f64_empty_is_zero() {
        assert_eq!(ArrayData::F64(vec![]).sum_as_f64(), 0.0);
    }

    #[test]
    fn test_sum_as_f64_string_is_zero() {
        // Non-numeric types contribute 0
        let result = ArrayData::String(vec!["a".to_string()]).sum_as_f64();
        assert_eq!(result, 0.0);
    }

    // ── get_value ─────────────────────────────────────────────────────────────

    #[test]
    fn test_get_value_f64_valid() {
        let data = ArrayData::F64(vec![1.25, 6.78]);
        assert!(matches!(data.get_value(0), Some(Value::F64(x)) if (x - 1.25).abs() < 1e-10));
        assert!(matches!(data.get_value(1), Some(Value::F64(x)) if (x - 6.78).abs() < 1e-10));
    }

    #[test]
    fn test_get_value_i64_valid() {
        let data = ArrayData::I64(vec![42, -7]);
        assert!(matches!(data.get_value(0), Some(Value::I64(42))));
        assert!(matches!(data.get_value(1), Some(Value::I64(-7))));
    }

    #[test]
    fn test_get_value_out_of_bounds_returns_none() {
        let data = ArrayData::I64(vec![1, 2]);
        assert!(data.get_value(10).is_none(), "out-of-bounds should return None");
    }

    // ── as_f64_slice ──────────────────────────────────────────────────────────

    #[test]
    fn test_as_f64_slice_for_f64_data() {
        let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
        let slice = data.as_f64_slice().unwrap();
        assert_eq!(slice, &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_as_f64_slice_for_non_f64_returns_none() {
        let data = ArrayData::I64(vec![1, 2]);
        assert!(data.as_f64_slice().is_none());
    }
}
