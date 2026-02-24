//! ArrayValue - N-dimensional array with type-segregated storage.
//!
//! This module contains the `ArrayValue` struct for representing Julia arrays
//! with efficient homogeneous storage using `ArrayData`.
//!
//! # Sub-modules
//!
//! - `access`: Element access, slicing, type-checked data accessors
//! - `mutation`: Element mutation, push/pop, insert/delete operations

// SAFETY: i64→usize casts for linear/multi-dimensional index computation are
// guarded by `index < 1 || index as usize > total_size` and `dim_idx < 1 || dim_idx as usize > shape[i]`.
#![allow(clippy::cast_sign_loss)]

mod access;
mod mutation;

use std::cell::RefCell;
use std::rc::Rc;

use super::super::error::VmError;
use super::array_data::ArrayData;
use super::array_element::ArrayElementType;
use super::Value;

/// N-dimensional array value with type-segregated storage (column-major order like Julia)
/// Supports all Value types using ArrayData for efficient homogeneous storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArrayValue {
    /// Type-segregated storage for efficient operations
    pub data: ArrayData,
    /// Shape: [dim1, dim2, ...] for N-D arrays
    pub shape: Vec<usize>,
    /// Optional concrete struct type for StructRefs arrays
    pub struct_type_id: Option<usize>,
    /// Optional element type override (for complex arrays that use F32/F64 storage)
    /// When Some, this takes precedence over data.element_type()
    pub element_type_override: Option<ArrayElementType>,
}

pub type ArrayRef = Rc<RefCell<ArrayValue>>;

pub fn new_array_ref(arr: ArrayValue) -> ArrayRef {
    Rc::new(RefCell::new(arr))
}

// Backward compatibility aliases (to be removed after full migration)
pub type TypedArrayValue = ArrayValue;
pub type TypedArrayRef = ArrayRef;

pub fn new_typed_array_ref(arr: ArrayValue) -> ArrayRef {
    new_array_ref(arr)
}

impl ArrayValue {
    /// Create a new array with given data and shape
    pub fn new(data: ArrayData, shape: Vec<usize>) -> Self {
        Self {
            data,
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create an ArrayValue from a MemoryValue by extracting the data and adding a
    /// shape wrapper. Used by zeros/ones/similar builtins internally.
    pub fn from_memory(mem: super::memory_value::MemoryValue, shape: Vec<usize>) -> Self {
        Self {
            data: mem.data,
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create an ArrayValue from a MemoryValue with an element type override.
    ///
    /// Used for complex arrays where the underlying storage (F64) needs to be
    /// tagged with ComplexF64 element type.
    pub fn from_memory_with_override(
        mem: super::memory_value::MemoryValue,
        shape: Vec<usize>,
        element_type_override: ArrayElementType,
    ) -> Self {
        Self {
            data: mem.data,
            shape,
            struct_type_id: None,
            element_type_override: Some(element_type_override),
        }
    }

    /// Create a new f64 array from raw data
    pub fn from_f64(data: Vec<f64>, shape: Vec<usize>) -> Self {
        Self {
            data: ArrayData::F64(data),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a new i64 array from raw data
    pub fn from_i64(data: Vec<i64>, shape: Vec<usize>) -> Self {
        Self {
            data: ArrayData::I64(data),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a 1D f64 vector
    pub fn vector(data: Vec<f64>) -> Self {
        let len = data.len();
        Self {
            data: ArrayData::F64(data),
            shape: vec![len],
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a 1D i64 vector
    pub fn i64_vector(data: Vec<i64>) -> Self {
        let len = data.len();
        Self {
            data: ArrayData::I64(data),
            shape: vec![len],
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a bool array from raw data
    pub fn from_bool(data: Vec<bool>, shape: Vec<usize>) -> Self {
        Self {
            data: ArrayData::Bool(data),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a 1D bool vector
    pub fn bool_vector(data: Vec<bool>) -> Self {
        let len = data.len();
        Self {
            data: ArrayData::Bool(data),
            shape: vec![len],
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a zeros array (f64)
    pub fn zeros(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: ArrayData::F64(vec![0.0; total]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a ones array (f64)
    pub fn ones(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: ArrayData::F64(vec![1.0; total]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a zeros array (f64) - explicit type version
    pub fn zeros_f64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: ArrayData::F64(vec![0.0; total]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a zeros array (i64) - explicit type version
    pub fn zeros_i64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: ArrayData::I64(vec![0; total]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a ones array (f64) - explicit type version
    pub fn ones_f64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: ArrayData::F64(vec![1.0; total]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a ones array (i64) - explicit type version
    pub fn ones_i64(shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: ArrayData::I64(vec![1; total]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a filled array with a specific f64 value
    pub fn fill(value: f64, shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        Self {
            data: ArrayData::F64(vec![value; total]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a filled array with a specific Value
    pub fn fill_value(value: Value, shape: Vec<usize>) -> Self {
        let total: usize = shape.iter().product();
        let data = match &value {
            Value::F64(v) => ArrayData::F64(vec![*v; total]),
            Value::F32(v) => ArrayData::F32(vec![*v; total]),
            Value::I64(v) => ArrayData::I64(vec![*v; total]),
            Value::Bool(v) => ArrayData::Bool(vec![*v; total]),
            Value::Str(s) => ArrayData::String(vec![s.clone(); total]),
            Value::Char(c) => ArrayData::Char(vec![*c; total]),
            _ => ArrayData::Any(vec![value; total]),
        };
        Self {
            data,
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a new empty array with given element type and capacity
    pub fn with_capacity(element_type: ArrayElementType, capacity: usize) -> Self {
        let data = match &element_type {
            ArrayElementType::F32 => ArrayData::F32(Vec::with_capacity(capacity)),
            ArrayElementType::F64 => ArrayData::F64(Vec::with_capacity(capacity)),
            // Complex types use interleaved storage: [re1, im1, re2, im2, ...]
            // Each complex number takes 2 slots in the underlying storage
            ArrayElementType::ComplexF32 => ArrayData::F32(Vec::with_capacity(capacity * 2)),
            ArrayElementType::ComplexF64 => ArrayData::F64(Vec::with_capacity(capacity * 2)),
            ArrayElementType::I8 => ArrayData::I8(Vec::with_capacity(capacity)),
            ArrayElementType::I16 => ArrayData::I16(Vec::with_capacity(capacity)),
            ArrayElementType::I32 => ArrayData::I32(Vec::with_capacity(capacity)),
            ArrayElementType::I64 => ArrayData::I64(Vec::with_capacity(capacity)),
            ArrayElementType::U8 => ArrayData::U8(Vec::with_capacity(capacity)),
            ArrayElementType::U16 => ArrayData::U16(Vec::with_capacity(capacity)),
            ArrayElementType::U32 => ArrayData::U32(Vec::with_capacity(capacity)),
            ArrayElementType::U64 => ArrayData::U64(Vec::with_capacity(capacity)),
            ArrayElementType::Bool => ArrayData::Bool(Vec::with_capacity(capacity)),
            ArrayElementType::String => ArrayData::String(Vec::with_capacity(capacity)),
            ArrayElementType::Char => ArrayData::Char(Vec::with_capacity(capacity)),
            ArrayElementType::StructOf(_) => ArrayData::StructRefs(Vec::with_capacity(capacity)),
            // isbits struct inline storage: AoS format
            ArrayElementType::StructInlineOf(_, field_count) => {
                ArrayData::Any(Vec::with_capacity(capacity * field_count))
            }
            ArrayElementType::Struct | ArrayElementType::Any => {
                ArrayData::Any(Vec::with_capacity(capacity))
            }
            // Tuple arrays use AoS storage in ArrayData::Any
            ArrayElementType::TupleOf(ref field_types) => {
                ArrayData::Any(Vec::with_capacity(capacity * field_types.len()))
            }
        };
        let struct_type_id = match &element_type {
            ArrayElementType::StructOf(type_id) | ArrayElementType::StructInlineOf(type_id, _) => {
                Some(*type_id)
            }
            _ => None,
        };
        // For complex, tuple, and isbits struct types, we need to override the element type since
        // the underlying storage (F32/F64/Any) doesn't distinguish between real/complex or plain/tuple/struct
        let element_type_override = if element_type.is_complex()
            || element_type.is_tuple()
            || element_type.is_struct_inline()
        {
            Some(element_type)
        } else {
            None
        };
        Self {
            data,
            shape: vec![0],
            struct_type_id,
            element_type_override,
        }
    }

    /// Create a struct array with given type_id and capacity
    pub fn with_struct_type(type_id: usize, capacity: usize) -> Self {
        Self {
            data: ArrayData::StructRefs(Vec::with_capacity(capacity)),
            shape: vec![0],
            struct_type_id: Some(type_id),
            element_type_override: None,
        }
    }

    /// Create a heterogeneous (Any) array from a Vec<Value>
    pub fn any_vector(values: Vec<Value>) -> Self {
        let len = values.len();
        Self {
            data: ArrayData::Any(values),
            shape: vec![len],
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a complex F64 array with interleaved storage
    /// data should be [re1, im1, re2, im2, ...] with shape indicating logical dimensions
    pub fn complex_f64(data: Vec<f64>, shape: Vec<usize>) -> Self {
        Self {
            data: ArrayData::F64(data),
            shape,
            struct_type_id: None,
            element_type_override: Some(ArrayElementType::ComplexF64),
        }
    }

    /// Create a complex F32 array with interleaved storage
    /// data should be [re1, im1, re2, im2, ...] with shape indicating logical dimensions
    pub fn complex_f32(data: Vec<f32>, shape: Vec<usize>) -> Self {
        Self {
            data: ArrayData::F32(data),
            shape,
            struct_type_id: None,
            element_type_override: Some(ArrayElementType::ComplexF32),
        }
    }

    /// Create a complex F64 array filled with zeros
    pub fn zeros_complex_f64(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        // Each complex number needs 2 f64 values (re, im)
        Self {
            data: ArrayData::F64(vec![0.0; size * 2]),
            shape,
            struct_type_id: None,
            element_type_override: Some(ArrayElementType::ComplexF64),
        }
    }

    /// Create a complex F32 array filled with zeros
    pub fn zeros_complex_f32(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        // Each complex number needs 2 f32 values (re, im)
        Self {
            data: ArrayData::F32(vec![0.0; size * 2]),
            shape,
            struct_type_id: None,
            element_type_override: Some(ArrayElementType::ComplexF32),
        }
    }

    /// Create an uninitialized Float64 array (for Vector{Float64}(undef, n))
    /// Values are initialized to 0.0 for safety (Rust doesn't have true undef)
    pub fn undef_f64(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        Self {
            data: ArrayData::F64(vec![0.0; size]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create an uninitialized Int64 array (for Vector{Int64}(undef, n))
    /// Values are initialized to 0 for safety (Rust doesn't have true undef)
    pub fn undef_i64(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        Self {
            data: ArrayData::I64(vec![0; size]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create an uninitialized Bool array (for Vector{Bool}(undef, n))
    /// Values are initialized to false for safety
    pub fn undef_bool(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        Self {
            data: ArrayData::Bool(vec![false; size]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create an uninitialized Complex{Float64} array (for Vector{Complex{Float64}}(undef, n))
    /// Values are initialized to 0.0+0.0im for safety
    pub fn undef_complex_f64(shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        // Each complex number needs 2 f64 values (re, im)
        Self {
            data: ArrayData::F64(vec![0.0; size * 2]),
            shape,
            struct_type_id: None,
            element_type_override: Some(ArrayElementType::ComplexF64),
        }
    }

    /// Create an uninitialized array for any supported element type (Issue #2218).
    /// This is the generic version that handles all types including small integers and floats.
    pub fn undef_typed(elem_type: &ArrayElementType, shape: Vec<usize>) -> Self {
        let size: usize = shape.iter().product();
        match elem_type {
            ArrayElementType::F64 => Self::undef_f64(shape),
            ArrayElementType::I64 => Self::undef_i64(shape),
            ArrayElementType::Bool => Self::undef_bool(shape),
            ArrayElementType::ComplexF64 => Self::undef_complex_f64(shape),
            ArrayElementType::ComplexF32 => Self {
                data: ArrayData::F32(vec![0.0; size * 2]),
                shape,
                struct_type_id: None,
                element_type_override: Some(ArrayElementType::ComplexF32),
            },
            ArrayElementType::F32 => Self {
                data: ArrayData::F32(vec![0.0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::I8 => Self {
                data: ArrayData::I8(vec![0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::I16 => Self {
                data: ArrayData::I16(vec![0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::I32 => Self {
                data: ArrayData::I32(vec![0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::U8 => Self {
                data: ArrayData::U8(vec![0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::U16 => Self {
                data: ArrayData::U16(vec![0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::U32 => Self {
                data: ArrayData::U32(vec![0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::U64 => Self {
                data: ArrayData::U64(vec![0; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::String => Self {
                data: ArrayData::String(vec![std::string::String::new(); size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            ArrayElementType::Char => Self {
                data: ArrayData::Char(vec!['\0'; size]),
                shape,
                struct_type_id: None,
                element_type_override: None,
            },
            _ => Self::undef_any(shape),
        }
    }

    /// Create an uninitialized Any array (for Vector{Any}(undef, n))
    /// Values are initialized to nothing for safety
    pub fn undef_any(shape: Vec<usize>) -> Self {
        use super::Value;
        let size: usize = shape.iter().product();
        Self {
            data: ArrayData::Any(vec![Value::Nothing; size]),
            shape,
            struct_type_id: None,
            element_type_override: None,
        }
    }

    /// Create a tuple array with AoS (Array of Structs) storage
    /// data should be [a1, b1, a2, b2, ...] for Tuple{A, B} tuples
    /// shape indicates logical dimensions (number of tuples)
    pub fn tuple_array(
        data: Vec<Value>,
        shape: Vec<usize>,
        field_types: Vec<ArrayElementType>,
    ) -> Self {
        Self {
            data: ArrayData::Any(data),
            shape,
            struct_type_id: None,
            element_type_override: Some(ArrayElementType::TupleOf(field_types)),
        }
    }

    /// Create an empty tuple array with given field types and capacity
    pub fn with_tuple_capacity(field_types: Vec<ArrayElementType>, capacity: usize) -> Self {
        Self {
            data: ArrayData::Any(Vec::with_capacity(capacity * field_types.len())),
            shape: vec![0],
            struct_type_id: None,
            element_type_override: Some(ArrayElementType::TupleOf(field_types)),
        }
    }

    /// Create an isbits struct array with inline AoS storage
    /// data should be [f1_1, f2_1, f1_2, f2_2, ...] for Point{x, y} structs
    /// shape indicates logical dimensions (number of structs)
    pub fn isbits_struct_array(
        type_id: usize,
        field_count: usize,
        data: Vec<Value>,
        shape: Vec<usize>,
    ) -> Self {
        Self {
            data: ArrayData::Any(data),
            shape,
            struct_type_id: Some(type_id),
            element_type_override: Some(ArrayElementType::StructInlineOf(type_id, field_count)),
        }
    }

    /// Create an empty isbits struct array with capacity
    pub fn with_isbits_struct_capacity(
        type_id: usize,
        field_count: usize,
        capacity: usize,
    ) -> Self {
        Self {
            data: ArrayData::Any(Vec::with_capacity(capacity * field_count)),
            shape: vec![0],
            struct_type_id: Some(type_id),
            element_type_override: Some(ArrayElementType::StructInlineOf(type_id, field_count)),
        }
    }

    /// Check if this is an isbits struct array
    pub fn is_isbits_struct_array(&self) -> bool {
        matches!(
            self.element_type_override,
            Some(ArrayElementType::StructInlineOf(_, _))
        )
    }

    /// Get the element type
    /// Returns element_type_override if set, otherwise infers from data
    pub fn element_type(&self) -> ArrayElementType {
        self.element_type_override
            .clone()
            .unwrap_or_else(|| self.data.element_type())
    }

    /// Get the number of logical elements
    pub fn element_count(&self) -> usize {
        self.shape.iter().product()
    }

    /// Total number of elements (alias for element_count)
    pub fn len(&self) -> usize {
        self.data.raw_len()
    }

    /// Check if array is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Number of dimensions
    pub fn ndims(&self) -> usize {
        self.shape.len()
    }

    /// Size in a specific dimension (1-indexed like Julia)
    pub fn size(&self, dim: usize) -> Option<usize> {
        if dim >= 1 && dim <= self.shape.len() {
            Some(self.shape[dim - 1])
        } else {
            None
        }
    }

    /// Convert N-dimensional indices to linear index (column-major, 1-indexed)
    /// Supports both:
    /// - Full indexing: arr[i, j, k] for 3D array (indices.len() == shape.len())
    /// - Linear indexing: arr[i] for any dimension array (indices.len() == 1)
    pub fn linear_index(&self, indices: &[i64]) -> Result<usize, VmError> {
        // Linear indexing: single index for any dimension array
        if indices.len() == 1 {
            let index = indices[0];
            let total_size = self.element_count();

            // Bounds check (1-indexed)
            if index < 1 || index as usize > total_size {
                return Err(VmError::IndexOutOfBounds {
                    indices: indices.to_vec(),
                    shape: self.shape.clone(),
                });
            }

            // Convert to 0-indexed
            return Ok((index - 1) as usize);
        }

        // Full indexing: indices count must match dimensions
        if indices.len() != self.shape.len() {
            return Err(VmError::DimensionMismatch {
                expected: self.shape.len(),
                got: indices.len(),
            });
        }

        let mut linear = 0;
        let mut stride = 1;
        for (i, &dim_idx) in indices.iter().enumerate() {
            if dim_idx < 1 || dim_idx as usize > self.shape[i] {
                return Err(VmError::IndexOutOfBounds {
                    indices: indices.to_vec(),
                    shape: self.shape.clone(),
                });
            }
            linear += ((dim_idx - 1) as usize) * stride;
            stride *= self.shape[i];
        }
        Ok(linear)
    }

    /// Get the type_id for Complex structs returned from this array.
    /// Uses the stored struct_type_id if available, falls back to 0.
    /// The struct_type_id is set when the array is created (e.g., in AllocUndefComplexF64)
    /// to match the runtime struct_defs ordering.
    fn complex_type_id(&self) -> usize {
        self.struct_type_id.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_data_f64_ok_on_f64_array() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0], vec![3]);
        let data = arr.try_data_f64().unwrap();
        assert_eq!(data, &vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn try_data_f64_err_on_i64_array() {
        let arr = ArrayValue::from_i64(vec![1, 2, 3], vec![3]);
        assert!(arr.try_data_f64().is_err());
    }

    #[test]
    fn try_data_f64_err_on_bool_array() {
        let arr = ArrayValue::from_bool(vec![true, false], vec![2]);
        assert!(arr.try_data_f64().is_err());
    }

    #[test]
    fn try_data_f64_mut_ok_on_f64_array() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        let data = arr.try_data_f64_mut().unwrap();
        data[0] = 42.0;
        assert_eq!(arr.try_data_f64().unwrap()[0], 42.0);
    }

    #[test]
    fn try_data_f64_mut_err_on_i64_array() {
        let mut arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        assert!(arr.try_data_f64_mut().is_err());
    }

    #[test]
    fn try_data_i64_ok_on_i64_array() {
        let arr = ArrayValue::from_i64(vec![10, 20, 30], vec![3]);
        let data = arr.try_data_i64().unwrap();
        assert_eq!(data, &vec![10, 20, 30]);
    }

    #[test]
    fn try_data_i64_err_on_f64_array() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        assert!(arr.try_data_i64().is_err());
    }

    #[test]
    fn try_data_i64_mut_ok_on_i64_array() {
        let mut arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        let data = arr.try_data_i64_mut().unwrap();
        data[0] = 99;
        assert_eq!(arr.try_data_i64().unwrap()[0], 99);
    }

    #[test]
    fn try_data_i64_mut_err_on_f64_array() {
        let mut arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        assert!(arr.try_data_i64_mut().is_err());
    }

    #[test]
    fn try_data_bool_ok_on_bool_array() {
        let arr = ArrayValue::from_bool(vec![true, false, true], vec![3]);
        let data = arr.try_data_bool().unwrap();
        assert_eq!(data, &vec![true, false, true]);
    }

    #[test]
    fn try_data_bool_err_on_f64_array() {
        let arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        assert!(arr.try_data_bool().is_err());
    }

    #[test]
    fn try_as_f64_vec_ok_on_f64() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.5], vec![2]);
        let v = arr.try_as_f64_vec().unwrap();
        assert_eq!(v, vec![1.0, 2.5]);
    }

    #[test]
    fn try_as_f64_vec_ok_on_i64() {
        let arr = ArrayValue::from_i64(vec![3, 4], vec![2]);
        let v = arr.try_as_f64_vec().unwrap();
        assert_eq!(v, vec![3.0, 4.0]);
    }

    #[test]
    fn try_as_f64_vec_ok_on_bool() {
        let arr = ArrayValue::from_bool(vec![true, false], vec![2]);
        let v = arr.try_as_f64_vec().unwrap();
        assert_eq!(v, vec![1.0, 0.0]);
    }

    #[test]
    fn try_as_f64_vec_err_on_string_array() {
        let arr = ArrayValue::new(ArrayData::String(vec!["hello".to_string()]), vec![1]);
        assert!(arr.try_as_f64_vec().is_err());
    }

    #[test]
    fn try_as_f64_vec_err_on_any_array() {
        let arr = ArrayValue::any_vector(vec![Value::Nothing]);
        assert!(arr.try_as_f64_vec().is_err());
    }
}
