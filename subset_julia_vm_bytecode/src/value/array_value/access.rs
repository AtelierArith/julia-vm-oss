//! Element access, slicing, and type-checked data accessors for ArrayValue.

// SAFETY: i64→usize casts for array element access are guarded by bounds checks
// (`idx < 1 || idx as usize > len`) that ensure values are positive before the cast.
#![allow(clippy::cast_sign_loss)]

use super::super::super::error::VmError;
use super::super::array_data::ArrayData;
use super::super::array_element::ArrayElementType;
use super::super::struct_instance::StructInstance;
use super::super::tuple::TupleValue;
use super::super::Value;
use super::ArrayValue;

impl ArrayValue {
    /// Get element at indices (1-indexed), returns Value
    /// For Complex arrays, unpacks interleaved storage into Complex struct
    /// For Tuple arrays, unpacks AoS storage into Tuple
    pub fn get(&self, indices: &[i64]) -> Result<Value, VmError> {
        let linear = self.linear_index(indices)?;
        if let Some(parent) = &self.shared_parent {
            return parent
                .borrow()
                .get_linear_value(linear, indices, &self.shape);
        }

        self.get_linear_value(linear, indices, &self.shape)
    }

    /// Get element by zero-based Julia linear storage index.
    ///
    /// This preserves reshape shared-backing semantics for VM consumers that
    /// need elementwise reads but do not have multidimensional indices.
    pub fn get_linear(&self, linear: usize) -> Result<Value, VmError> {
        if linear >= self.element_count() {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![(linear + 1) as i64],
                shape: self.shape.clone(),
            });
        }
        let indices = [(linear + 1) as i64];
        if let Some(parent) = &self.shared_parent {
            return parent
                .borrow()
                .get_linear_value(linear, &indices, &self.shape);
        }

        self.get_linear_value(linear, &indices, &self.shape)
    }

    /// Get a logical linear element after the compiler has proved the index is
    /// within the array axes. This skips the explicit `element_count` guard but
    /// still lets the storage accessor report a BoundsError if an upstream proof
    /// is invalid, keeping Rust memory access safe.
    pub fn get_linear_inbounds(&self, linear: usize) -> Result<Value, VmError> {
        let indices = [(linear + 1) as i64];
        if let Some(parent) = &self.shared_parent {
            return parent
                .borrow()
                .get_linear_value(linear, &indices, &self.shape);
        }

        self.get_linear_value(linear, &indices, &self.shape)
    }

    /// Read a logical linear element as f64, following reshape shared backing.
    pub fn get_linear_f64(&self, linear: usize) -> Result<f64, VmError> {
        match self.get_linear(linear)? {
            Value::F64(v) => Ok(v),
            Value::F32(v) => Ok(v as f64),
            Value::F16(v) => Ok(v.to_f64()),
            Value::I64(v) => Ok(v as f64),
            Value::I32(v) => Ok(v as f64),
            Value::I16(v) => Ok(v as f64),
            Value::I8(v) => Ok(v as f64),
            Value::U64(v) => Ok(v as f64),
            Value::U32(v) => Ok(v as f64),
            Value::U16(v) => Ok(v as f64),
            Value::U8(v) => Ok(v as f64),
            Value::Bool(v) => Ok(if v { 1.0 } else { 0.0 }),
            other => Err(VmError::TypeError(format!(
                "expected numeric array element, got {:?}",
                other.value_type()
            ))),
        }
    }

    /// Convert logical elements to Values, following reshape shared backing.
    pub fn to_logical_value_vec(&self) -> Result<Vec<Value>, VmError> {
        (0..self.element_count())
            .map(|i| self.get_linear(i))
            .collect()
    }

    /// Convert logical elements to f64 values, following reshape shared backing.
    pub fn to_logical_f64_vec(&self) -> Result<Vec<f64>, VmError> {
        (0..self.element_count())
            .map(|i| self.get_linear_f64(i))
            .collect()
    }

    fn get_linear_value(
        &self,
        linear: usize,
        indices: &[i64],
        error_shape: &[usize],
    ) -> Result<Value, VmError> {
        // Handle complex/tuple arrays with special storage
        if let Some(ref elem_type) = self.element_type_override {
            match elem_type {
                ArrayElementType::ComplexF64 => {
                    // Read two consecutive f64 values (re, im) from the general
                    // contiguous-isbits `StructF64` buffer. `Complex{Float64}` IS
                    // the 2×f64 isbits struct, so it shares the S4 `StructF64`
                    // storage with user all-`Float64` structs rather than the
                    // scalar-`Float64` `ArrayData::F64` variant (Issue #9198 S5).
                    let raw_idx = linear * 2;
                    let re = match &self.data {
                        ArrayData::StructF64(v) => v.get(raw_idx).copied(),
                        _ => None,
                    };
                    let im = match &self.data {
                        ArrayData::StructF64(v) => v.get(raw_idx + 1).copied(),
                        _ => None,
                    };
                    if let (Some(re), Some(im)) = (re, im) {
                        // Construct a Complex{Float64} struct (fields [re, im] in
                        // order). Use the stored struct_type_id for the correct
                        // runtime type_id; derive the struct name from the element
                        // type's own Julia name instead of a string literal so the
                        // name has a single source of truth (Issue #5152).
                        return Ok(Value::Struct(StructInstance::complex_from_storage(
                            self.complex_type_id(),
                            elem_type.julia_type_name(),
                            Value::F64(re),
                            Value::F64(im),
                        )));
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: error_shape.to_vec(),
                    });
                }
                ArrayElementType::ComplexF32 => {
                    // Read two consecutive f32 values (re, im)
                    let raw_idx = linear * 2;
                    let re = match &self.data {
                        ArrayData::F32(v) => v.get(raw_idx).copied(),
                        _ => None,
                    };
                    let im = match &self.data {
                        ArrayData::F32(v) => v.get(raw_idx + 1).copied(),
                        _ => None,
                    };
                    if let (Some(re), Some(im)) = (re, im) {
                        // Construct a Complex{Float32} struct. Use the stored
                        // struct_type_id for the correct runtime type_id; derive
                        // the struct name from the element type's own Julia name
                        // instead of a string literal (Issue #5152).
                        return Ok(Value::Struct(StructInstance::complex_from_storage(
                            self.complex_type_id(),
                            elem_type.julia_type_name(),
                            Value::F32(re),
                            Value::F32(im),
                        )));
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: error_shape.to_vec(),
                    });
                }
                ArrayElementType::TupleOf(ref field_types) => {
                    // Read arity consecutive values for tuple fields
                    let arity = field_types.len();
                    let raw_idx = linear * arity;

                    if let ArrayData::Any(v) = &self.data {
                        if raw_idx + arity <= v.len() {
                            let tuple_values: Vec<Value> = v[raw_idx..raw_idx + arity].to_vec();
                            return Ok(Value::Tuple(TupleValue::new(tuple_values)));
                        }
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: error_shape.to_vec(),
                    });
                }
                ArrayElementType::StructInlineOf(type_id, field_count) => {
                    // Read field_count consecutive values for struct fields
                    let raw_idx = linear * field_count;

                    if let ArrayData::Any(v) = &self.data {
                        if raw_idx + field_count <= v.len() {
                            let field_values: Vec<Value> =
                                v[raw_idx..raw_idx + field_count].to_vec();
                            return Ok(Value::Struct(StructInstance::new(*type_id, field_values)));
                        }
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: error_shape.to_vec(),
                    });
                }
                ArrayElementType::StructInlineF64(type_id, field_count) => {
                    // Read field_count consecutive raw f64 slots and reconstruct
                    // the all-`Float64` isbits struct (Issue #9198 S4). The
                    // struct name is resolved from `type_id` at the VM layer, so
                    // `StructInstance::new` leaves it empty (like `StructInlineOf`).
                    let raw_idx = linear * field_count;
                    if let ArrayData::StructF64(v) = &self.data {
                        if raw_idx + field_count <= v.len() {
                            let field_values: Vec<Value> = v[raw_idx..raw_idx + field_count]
                                .iter()
                                .map(|&x| Value::F64(x))
                                .collect();
                            return Ok(Value::Struct(StructInstance::new_inline(
                                *type_id,
                                field_values,
                            )));
                        }
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: error_shape.to_vec(),
                    });
                }
                _ => {} // Fall through to normal get
            }
        }

        self.data
            .get_value(linear)
            .ok_or(VmError::IndexOutOfBounds {
                indices: indices.to_vec(),
                shape: error_shape.to_vec(),
            })
    }

    /// Get element at indices (1-indexed), returns f64 (for F64 arrays)
    pub fn get_f64(&self, indices: &[i64]) -> Result<f64, VmError> {
        match self.get(indices)? {
            Value::F64(v) => Ok(v),
            Value::I64(v) => Ok(v as f64),
            other => Err(VmError::TypeError(format!(
                "Expected F64, got {:?}",
                other.value_type()
            ))),
        }
    }

    /// Convert all elements to a Vec<Value>, resolving StructRefs if needed
    /// Note: For StructRefs, this returns Value::StructRef(idx) - the caller must resolve
    pub fn to_value_vec(&self) -> Vec<Value> {
        let len = self.data.raw_len();
        (0..len).filter_map(|i| self.data.get_value(i)).collect()
    }

    /// Check if this array contains struct references
    pub fn is_struct_array(&self) -> bool {
        matches!(self.data, ArrayData::StructRefs(_))
    }

    /// Get a reference to the underlying f64 data (for F64 arrays only)
    pub fn as_f64_slice(&self) -> Option<&[f64]> {
        self.data.as_f64_slice()
    }

    /// Get a mutable reference to the underlying f64 data (for F64 arrays only)
    pub fn as_f64_slice_mut(&mut self) -> Option<&mut Vec<f64>> {
        self.data.as_f64_slice_mut()
    }

    /// Raw interleaved `f64` buffer of a `Complex{Float64}` array — the
    /// contiguous `[re0, im0, re1, im1, ...]` store (2 raw slots per element).
    ///
    /// Issue #9198 S5: `Complex{Float64}` arrays back their interleaved data with
    /// the general contiguous-isbits `ArrayData::StructF64` variant (shared with
    /// user all-`Float64` structs), NOT the scalar-`Float64` `ArrayData::F64`.
    /// This accessor returns the buffer for a complex array regardless of which
    /// f64 variant currently backs it, so the complex fast paths (broadcast) do
    /// not depend on the storage-variant choice. Returns `Err` for any other
    /// storage.
    pub fn complex_interleaved_f64(&self) -> Result<&Vec<f64>, VmError> {
        match &self.data {
            ArrayData::StructF64(v) | ArrayData::F64(v) => Ok(v),
            other => Err(VmError::TypeError(format!(
                "expected complex f64 interleaved array, got {:?}",
                other.element_type()
            ))),
        }
    }

    /// Get a reference to the underlying f64 Vec (for F64 arrays only).
    /// Returns Err if not an F64 array.
    pub fn try_data_f64(&self) -> Result<&Vec<f64>, VmError> {
        match &self.data {
            ArrayData::F64(v) => Ok(v),
            _ => Err(VmError::TypeError(format!(
                "expected F64 array, got {:?}",
                self.data.element_type()
            ))),
        }
    }

    /// Get a mutable reference to the underlying f64 Vec (for F64 arrays only).
    /// Returns Err if not an F64 array.
    pub fn try_data_f64_mut(&mut self) -> Result<&mut Vec<f64>, VmError> {
        let elem_type = self.data.element_type();
        match &mut self.data {
            ArrayData::F64(v) => Ok(v),
            _ => Err(VmError::TypeError(format!(
                "expected F64 array, got {:?}",
                elem_type
            ))),
        }
    }

    /// Check if this is an F64 array
    pub fn is_f64_array(&self) -> bool {
        matches!(self.data, ArrayData::F64(_))
    }

    /// Get a reference to the underlying i64 Vec (for I64 arrays only).
    /// Returns Err if not an I64 array.
    pub fn try_data_i64(&self) -> Result<&Vec<i64>, VmError> {
        match &self.data {
            ArrayData::I64(v) => Ok(v),
            _ => Err(VmError::TypeError(format!(
                "expected I64 array, got {:?}",
                self.data.element_type()
            ))),
        }
    }

    /// Get a mutable reference to the underlying i64 Vec (for I64 arrays only).
    /// Returns Err if not an I64 array.
    pub fn try_data_i64_mut(&mut self) -> Result<&mut Vec<i64>, VmError> {
        let elem_type = self.data.element_type();
        match &mut self.data {
            ArrayData::I64(v) => Ok(v),
            _ => Err(VmError::TypeError(format!(
                "expected I64 array, got {:?}",
                elem_type
            ))),
        }
    }

    /// Check if this is an I64 array
    pub fn is_i64_array(&self) -> bool {
        matches!(self.data, ArrayData::I64(_))
    }

    /// Get a reference to the underlying bool Vec (for Bool arrays only).
    /// Returns Err if not a Bool array.
    pub fn try_data_bool(&self) -> Result<&Vec<bool>, VmError> {
        match &self.data {
            ArrayData::Bool(v) => Ok(v),
            _ => Err(VmError::TypeError(format!(
                "expected Bool array, got {:?}",
                self.data.element_type()
            ))),
        }
    }

    /// Convert any numeric array to Vec<f64>.
    /// Returns Err if the array contains non-numeric data.
    pub fn try_as_f64_vec(&self) -> Result<Vec<f64>, VmError> {
        match &self.data {
            ArrayData::F64(v) => Ok(v.clone()),
            ArrayData::F32(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::I64(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::I32(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::I16(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::I8(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::U64(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::U32(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::U16(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::U8(v) => Ok(v.iter().map(|&x| x as f64).collect()),
            ArrayData::Bool(v) => Ok(v.iter().map(|&x| if x { 1.0 } else { 0.0 }).collect()),
            ArrayData::BitPackedBool(v) => {
                Ok(v.iter().map(|x| if x { 1.0 } else { 0.0 }).collect())
            }
            _ => Err(VmError::TypeError(format!(
                "cannot convert {:?} array to f64",
                self.data.element_type()
            ))),
        }
    }

    /// Get a slice of the array, returning a new array with selected elements
    pub fn slice(&self, indices: &[i64]) -> Result<Value, VmError> {
        // For 1D array with single index, return the element
        if self.shape.len() == 1 && indices.len() == 1 {
            return self.get(indices);
        }
        // For multi-dimensional slicing, we need more complex logic
        // For now, handle 1D linear indexing
        if indices.len() == 1 {
            let idx = indices[0];
            if idx < 1 || (idx as usize) > self.len() {
                return Err(VmError::IndexOutOfBounds {
                    indices: indices.to_vec(),
                    shape: self.shape.clone(),
                });
            }
            return self
                .data
                .get_value((idx - 1) as usize)
                .ok_or(VmError::IndexOutOfBounds {
                    indices: indices.to_vec(),
                    shape: self.shape.clone(),
                });
        }
        // For N-D indexing
        self.get(indices)
    }
}

#[cfg(test)]
mod tests {
    use super::ArrayValue;
    use super::Value;

    // ── is_f64_array ─────────────────────────────────────────────────────────

    #[test]
    fn test_access_is_f64_array_true_for_f64() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        assert!(arr.is_f64_array());
    }

    #[test]
    fn test_access_is_f64_array_false_for_i64() {
        let arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        assert!(!arr.is_f64_array());
    }

    #[test]
    fn test_access_is_f64_array_false_for_bool() {
        let arr = ArrayValue::from_bool(vec![true], vec![1]);
        assert!(!arr.is_f64_array());
    }

    // ── is_i64_array ─────────────────────────────────────────────────────────

    #[test]
    fn test_access_is_i64_array_true_for_i64() {
        let arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        assert!(arr.is_i64_array());
    }

    #[test]
    fn test_access_is_i64_array_false_for_f64() {
        let arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        assert!(!arr.is_i64_array());
    }

    #[test]
    fn test_access_is_i64_array_false_for_bool() {
        let arr = ArrayValue::from_bool(vec![false], vec![1]);
        assert!(!arr.is_i64_array());
    }

    // ── is_struct_array ───────────────────────────────────────────────────────

    #[test]
    fn test_access_is_struct_array_true_for_struct_refs() {
        let arr = ArrayValue::with_struct_type(0, 0);
        assert!(arr.is_struct_array());
    }

    #[test]
    fn test_access_is_struct_array_false_for_f64() {
        let arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        assert!(!arr.is_struct_array());
    }

    // ── as_f64_slice ──────────────────────────────────────────────────────────

    #[test]
    fn test_access_as_f64_slice_some_for_f64_array() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0], vec![3]);
        let slice = arr.as_f64_slice();
        assert!(slice.is_some());
        assert_eq!(slice.unwrap(), &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_access_as_f64_slice_none_for_i64_array() {
        let arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        assert!(arr.as_f64_slice().is_none());
    }

    // ── get (basic element access, 1-indexed) ─────────────────────────────────

    #[test]
    fn test_access_get_first_f64_element() {
        let arr = ArrayValue::from_f64(vec![10.0, 20.0, 30.0], vec![3]);
        let val = arr.get(&[1]).unwrap();
        assert!(matches!(val, Value::F64(v) if (v - 10.0).abs() < 1e-12));
    }

    #[test]
    fn test_access_get_second_i64_element() {
        let arr = ArrayValue::from_i64(vec![10, 20, 30], vec![3]);
        let val = arr.get(&[2]).unwrap();
        assert!(matches!(val, Value::I64(20)));
    }

    #[test]
    fn test_access_get_out_of_bounds_returns_err() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        assert!(arr.get(&[5]).is_err());
    }

    #[test]
    fn test_access_get_zero_index_returns_err() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        assert!(arr.get(&[0]).is_err());
    }

    // ── get_f64 ───────────────────────────────────────────────────────────────

    #[test]
    fn test_access_get_f64_returns_float_value() {
        let arr = ArrayValue::from_f64(vec![1.25], vec![1]);
        let v = arr.get_f64(&[1]).unwrap();
        assert!((v - 1.25).abs() < 1e-12);
    }

    #[test]
    fn test_access_get_f64_converts_i64_to_f64() {
        let arr = ArrayValue::from_i64(vec![7], vec![1]);
        let v = arr.get_f64(&[1]).unwrap();
        assert!((v - 7.0).abs() < 1e-12);
    }

    // ── to_value_vec ──────────────────────────────────────────────────────────

    #[test]
    fn test_access_to_value_vec_f64_array_elements() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        let v = arr.to_value_vec();
        assert_eq!(v.len(), 2);
        assert!(matches!(v[0], Value::F64(x) if (x - 1.0).abs() < 1e-12));
        assert!(matches!(v[1], Value::F64(x) if (x - 2.0).abs() < 1e-12));
    }

    #[test]
    fn test_access_to_value_vec_i64_array_elements() {
        let arr = ArrayValue::from_i64(vec![5, 6], vec![2]);
        let v = arr.to_value_vec();
        assert_eq!(v.len(), 2);
        assert!(matches!(v[0], Value::I64(5)));
        assert!(matches!(v[1], Value::I64(6)));
    }

    #[test]
    fn test_access_to_value_vec_empty_array() {
        let arr = ArrayValue::from_f64(vec![], vec![0]);
        let v = arr.to_value_vec();
        assert!(v.is_empty());
    }
}
