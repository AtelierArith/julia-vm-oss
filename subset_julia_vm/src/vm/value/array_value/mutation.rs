//! Element mutation operations for ArrayValue.
//!
//! Includes set, push, pop, insert, delete, and complex value operations.

use super::super::super::error::VmError;
use super::super::array_data::ArrayData;
use super::super::array_element::ArrayElementType;
use super::super::struct_instance::StructInstance;
use super::super::tuple::TupleValue;
use super::super::Value;
use super::ArrayValue;

impl ArrayValue {
    /// Set element at indices (1-indexed)
    /// For Complex arrays, packs Complex struct into interleaved storage
    /// For Tuple arrays, packs Tuple into AoS storage
    pub fn set(&mut self, indices: &[i64], value: Value) -> Result<(), VmError> {
        let linear = self.linear_index(indices)?;

        // Handle complex/tuple arrays with special storage
        if let Some(ref elem_type) = self.element_type_override {
            match elem_type {
                ArrayElementType::ComplexF64 => {
                    // Extract re and im from the Complex struct
                    // Complex struct has values in order: [re, im]
                    let (re, im) = match &value {
                        Value::Struct(s)
                            if s.struct_name.starts_with("Complex") && s.values.len() >= 2 =>
                        {
                            let re = match &s.values[0] {
                                Value::F64(x) => *x,
                                Value::I64(x) => *x as f64,
                                Value::F32(x) => *x as f64,
                                _ => 0.0,
                            };
                            let im = match &s.values[1] {
                                Value::F64(x) => *x,
                                Value::I64(x) => *x as f64,
                                Value::F32(x) => *x as f64,
                                _ => 0.0,
                            };
                            (re, im)
                        }
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "Cannot store {:?} in Complex{{Float64}} array",
                                value.value_type()
                            )))
                        }
                    };
                    // Write to interleaved storage
                    let raw_idx = linear * 2;
                    if let ArrayData::F64(v) = &mut self.data {
                        if raw_idx + 1 < v.len() {
                            v[raw_idx] = re;
                            v[raw_idx + 1] = im;
                            return Ok(());
                        }
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: self.shape.clone(),
                    });
                }
                ArrayElementType::ComplexF32 => {
                    // Extract re and im from the Complex struct
                    let (re, im) = match &value {
                        Value::Struct(s)
                            if s.struct_name.starts_with("Complex") && s.values.len() >= 2 =>
                        {
                            let re = match &s.values[0] {
                                Value::F32(x) => *x,
                                Value::F64(x) => *x as f32,
                                Value::I64(x) => *x as f32,
                                _ => 0.0,
                            };
                            let im = match &s.values[1] {
                                Value::F32(x) => *x,
                                Value::F64(x) => *x as f32,
                                Value::I64(x) => *x as f32,
                                _ => 0.0,
                            };
                            (re, im)
                        }
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "Cannot store {:?} in Complex{{Float32}} array",
                                value.value_type()
                            )))
                        }
                    };
                    // Write to interleaved storage
                    let raw_idx = linear * 2;
                    if let ArrayData::F32(v) = &mut self.data {
                        if raw_idx + 1 < v.len() {
                            v[raw_idx] = re;
                            v[raw_idx + 1] = im;
                            return Ok(());
                        }
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: self.shape.clone(),
                    });
                }
                ArrayElementType::TupleOf(ref field_types) => {
                    // Extract tuple elements
                    let arity = field_types.len();
                    let elements = match value {
                        Value::Tuple(t) if t.len() == arity => t.elements,
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "Cannot store {:?} in Tuple array (expected {}-element tuple)",
                                value.value_type(),
                                arity
                            )))
                        }
                    };

                    // Write to AoS storage
                    let raw_idx = linear * arity;
                    if let ArrayData::Any(v) = &mut self.data {
                        if raw_idx + arity <= v.len() {
                            for (i, elem) in elements.into_iter().enumerate() {
                                v[raw_idx + i] = elem;
                            }
                            return Ok(());
                        }
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: self.shape.clone(),
                    });
                }
                ArrayElementType::StructInlineOf(_, field_count) => {
                    // Extract struct fields
                    let fields = match &value {
                        Value::Struct(s) if s.values.len() == *field_count => s.values.clone(),
                        _ => {
                            return Err(VmError::TypeError(format!(
                            "Cannot store {:?} in isbits struct array (expected {}-field struct)",
                            value.value_type(),
                            field_count
                        )))
                        }
                    };

                    // Write to AoS storage
                    let raw_idx = linear * field_count;
                    if let ArrayData::Any(v) = &mut self.data {
                        if raw_idx + field_count <= v.len() {
                            for (i, field) in fields.into_iter().enumerate() {
                                v[raw_idx + i] = field;
                            }
                            return Ok(());
                        }
                    }
                    return Err(VmError::IndexOutOfBounds {
                        indices: indices.to_vec(),
                        shape: self.shape.clone(),
                    });
                }
                _ => {} // Fall through to normal set
            }
        }

        self.data.set_value(linear, value)
    }

    /// Set element at indices (1-indexed), f64 version
    pub fn set_f64(&mut self, indices: &[i64], value: f64) -> Result<(), VmError> {
        self.set(indices, Value::F64(value))
    }

    /// Push a value to the end (1D arrays only)
    /// For Complex arrays, pushes the re and im parts as two consecutive values
    /// For Tuple arrays, pushes each field as consecutive values
    pub fn push(&mut self, value: Value) -> Result<(), VmError> {
        if self.shape.len() != 1 {
            return Err(VmError::DimensionMismatch {
                expected: 1,
                got: self.shape.len(),
            });
        }

        // Handle complex/tuple arrays with special storage
        if let Some(ref elem_type) = self.element_type_override {
            match elem_type {
                ArrayElementType::ComplexF64 => {
                    let (re, im) = match &value {
                        Value::Struct(s)
                            if s.struct_name.starts_with("Complex") && s.values.len() >= 2 =>
                        {
                            let re = match &s.values[0] {
                                Value::F64(x) => *x,
                                Value::I64(x) => *x as f64,
                                Value::F32(x) => *x as f64,
                                _ => 0.0,
                            };
                            let im = match &s.values[1] {
                                Value::F64(x) => *x,
                                Value::I64(x) => *x as f64,
                                Value::F32(x) => *x as f64,
                                _ => 0.0,
                            };
                            (re, im)
                        }
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "Cannot push {:?} to Complex{{Float64}} array",
                                value.value_type()
                            )))
                        }
                    };
                    if let ArrayData::F64(v) = &mut self.data {
                        v.push(re);
                        v.push(im);
                        self.shape[0] = v.len() / 2;
                        return Ok(());
                    }
                    return Err(VmError::TypeError(
                        "Invalid array data for Complex{Float64}".to_string(),
                    ));
                }
                ArrayElementType::ComplexF32 => {
                    let (re, im) = match &value {
                        Value::Struct(s)
                            if s.struct_name.starts_with("Complex") && s.values.len() >= 2 =>
                        {
                            let re = match &s.values[0] {
                                Value::F32(x) => *x,
                                Value::F64(x) => *x as f32,
                                Value::I64(x) => *x as f32,
                                _ => 0.0,
                            };
                            let im = match &s.values[1] {
                                Value::F32(x) => *x,
                                Value::F64(x) => *x as f32,
                                Value::I64(x) => *x as f32,
                                _ => 0.0,
                            };
                            (re, im)
                        }
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "Cannot push {:?} to Complex{{Float32}} array",
                                value.value_type()
                            )))
                        }
                    };
                    if let ArrayData::F32(v) = &mut self.data {
                        v.push(re);
                        v.push(im);
                        self.shape[0] = v.len() / 2;
                        return Ok(());
                    }
                    return Err(VmError::TypeError(
                        "Invalid array data for Complex{Float32}".to_string(),
                    ));
                }
                ArrayElementType::TupleOf(ref field_types) => {
                    let arity = field_types.len();
                    let elements = match value {
                        Value::Tuple(t) if t.len() == arity => t.elements,
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "Cannot push {:?} to Tuple array (expected {}-element tuple)",
                                value.value_type(),
                                arity
                            )))
                        }
                    };

                    if let ArrayData::Any(v) = &mut self.data {
                        for elem in elements {
                            v.push(elem);
                        }
                        self.shape[0] = v.len() / arity;
                        return Ok(());
                    }
                    return Err(VmError::TypeError(
                        "Invalid array data for Tuple array".to_string(),
                    ));
                }
                ArrayElementType::StructInlineOf(_, field_count) => {
                    // Extract struct fields
                    let fields = match &value {
                        Value::Struct(s) if s.values.len() == *field_count => s.values.clone(),
                        _ => {
                            return Err(VmError::TypeError(format!(
                            "Cannot push {:?} to isbits struct array (expected {}-field struct)",
                            value.value_type(),
                            field_count
                        )))
                        }
                    };

                    if let ArrayData::Any(v) = &mut self.data {
                        for field in fields {
                            v.push(field);
                        }
                        self.shape[0] = v.len() / field_count;
                        return Ok(());
                    }
                    return Err(VmError::TypeError(
                        "Invalid array data for isbits struct array".to_string(),
                    ));
                }
                _ => {} // Fall through to normal push
            }
        }

        self.data.push_value(value)?;
        self.shape[0] = self.data.raw_len();
        Ok(())
    }

    /// Push f64 value to the end (1D F64 arrays only)
    pub fn push_f64(&mut self, value: f64) -> Result<(), VmError> {
        self.push(Value::F64(value))
    }

    /// Pop a value from the end (1D arrays only)
    /// For Complex arrays, pops two values and reconstructs the Complex struct
    /// For Tuple arrays, pops arity values and reconstructs the Tuple
    /// For isbits struct arrays, pops field_count values and reconstructs the struct
    pub fn pop(&mut self) -> Result<Value, VmError> {
        if self.shape.len() != 1 {
            return Err(VmError::DimensionMismatch {
                expected: 1,
                got: self.shape.len(),
            });
        }

        // Handle complex/tuple arrays with special storage
        if let Some(ref elem_type) = self.element_type_override {
            match elem_type {
                ArrayElementType::ComplexF64 => {
                    if let ArrayData::F64(v) = &mut self.data {
                        if v.len() >= 2 {
                            let im = v.pop().ok_or(VmError::EmptyArrayPop)?;
                            let re = v.pop().ok_or(VmError::EmptyArrayPop)?;
                            self.shape[0] = v.len() / 2;
                            // Use stored struct_type_id for correct runtime type_id lookup
                            return Ok(Value::Struct(StructInstance {
                                type_id: self.complex_type_id(),
                                struct_name: "Complex{Float64}".to_string(),
                                values: vec![Value::F64(re), Value::F64(im)],
                            }));
                        }
                    }
                    return Err(VmError::EmptyArrayPop);
                }
                ArrayElementType::ComplexF32 => {
                    if let ArrayData::F32(v) = &mut self.data {
                        if v.len() >= 2 {
                            let im = v.pop().ok_or(VmError::EmptyArrayPop)?;
                            let re = v.pop().ok_or(VmError::EmptyArrayPop)?;
                            self.shape[0] = v.len() / 2;
                            // Use stored struct_type_id for correct runtime type_id lookup
                            return Ok(Value::Struct(StructInstance {
                                type_id: self.complex_type_id(),
                                struct_name: "Complex{Float32}".to_string(),
                                values: vec![Value::F32(re), Value::F32(im)],
                            }));
                        }
                    }
                    return Err(VmError::EmptyArrayPop);
                }
                ArrayElementType::TupleOf(ref field_types) => {
                    let arity = field_types.len();
                    if let ArrayData::Any(v) = &mut self.data {
                        if v.len() >= arity {
                            let mut elements = Vec::with_capacity(arity);
                            // Pop in reverse order to get correct tuple order
                            for _ in 0..arity {
                                elements.push(v.pop().ok_or(VmError::EmptyArrayPop)?);
                            }
                            elements.reverse();
                            self.shape[0] = v.len() / arity;
                            return Ok(Value::Tuple(TupleValue::new(elements)));
                        }
                    }
                    return Err(VmError::EmptyArrayPop);
                }
                ArrayElementType::StructInlineOf(type_id, field_count) => {
                    if let ArrayData::Any(v) = &mut self.data {
                        if v.len() >= *field_count {
                            let mut fields = Vec::with_capacity(*field_count);
                            // Pop in reverse order to get correct field order
                            for _ in 0..*field_count {
                                fields.push(v.pop().ok_or(VmError::EmptyArrayPop)?);
                            }
                            fields.reverse();
                            self.shape[0] = v.len() / field_count;
                            return Ok(Value::Struct(StructInstance::new(*type_id, fields)));
                        }
                    }
                    return Err(VmError::EmptyArrayPop);
                }
                _ => {} // Fall through to normal pop
            }
        }

        let val = self.data.pop_value()?;
        self.shape[0] = self.data.raw_len();
        Ok(val)
    }

    /// Pop f64 value from the end (1D F64 arrays only)
    pub fn pop_f64(&mut self) -> Result<f64, VmError> {
        match self.pop()? {
            Value::F64(v) => Ok(v),
            other => Err(VmError::TypeError(format!(
                "Expected F64, got {:?}",
                other.value_type()
            ))),
        }
    }

    /// Push value to the front (1D arrays only)
    pub fn push_first(&mut self, value: Value) -> Result<(), VmError> {
        if self.shape.len() != 1 {
            return Err(VmError::DimensionMismatch {
                expected: 1,
                got: self.shape.len(),
            });
        }
        match (&mut self.data, value) {
            (ArrayData::F64(v), Value::F64(f)) => v.insert(0, f),
            (ArrayData::F64(v), Value::I64(i)) => v.insert(0, i as f64),
            (ArrayData::I64(v), Value::I64(i)) => v.insert(0, i),
            (ArrayData::Bool(v), Value::Bool(b)) => v.insert(0, b),
            (ArrayData::Bool(v), Value::I64(b)) => v.insert(0, b != 0),
            (ArrayData::String(v), Value::Str(s)) => v.insert(0, s),
            (ArrayData::Char(v), Value::Char(c)) => v.insert(0, c),
            (ArrayData::StructRefs(v), Value::StructRef(idx)) => v.insert(0, idx),
            (ArrayData::Any(v), val) => v.insert(0, val),
            (data, val) => {
                return Err(VmError::TypeError(format!(
                    "Cannot push_first {:?} into {:?} array",
                    val.value_type(),
                    data.element_type()
                )));
            }
        }
        self.shape[0] = self.data.raw_len();
        Ok(())
    }

    /// Pop value from the front (1D arrays only)
    pub fn pop_first(&mut self) -> Result<Value, VmError> {
        if self.shape.len() != 1 {
            return Err(VmError::DimensionMismatch {
                expected: 1,
                got: self.shape.len(),
            });
        }
        if self.data.raw_len() == 0 {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![1],
                shape: self.shape.clone(),
            });
        }
        let val = match &mut self.data {
            ArrayData::F32(v) => Value::F64(v.remove(0) as f64),
            ArrayData::F64(v) => Value::F64(v.remove(0)),
            ArrayData::I8(v) => Value::I64(v.remove(0) as i64),
            ArrayData::I16(v) => Value::I64(v.remove(0) as i64),
            ArrayData::I32(v) => Value::I64(v.remove(0) as i64),
            ArrayData::I64(v) => Value::I64(v.remove(0)),
            ArrayData::U8(v) => Value::I64(v.remove(0) as i64),
            ArrayData::U16(v) => Value::I64(v.remove(0) as i64),
            ArrayData::U32(v) => Value::I64(v.remove(0) as i64),
            ArrayData::U64(v) => Value::I64(v.remove(0) as i64),
            ArrayData::Bool(v) => Value::Bool(v.remove(0)),
            ArrayData::String(v) => Value::Str(v.remove(0)),
            ArrayData::Char(v) => Value::Char(v.remove(0)),
            ArrayData::StructRefs(v) => Value::StructRef(v.remove(0)),
            ArrayData::Any(v) => v.remove(0),
        };
        self.shape[0] = self.data.raw_len();
        Ok(val)
    }

    /// Insert value at specific 1-indexed position (1D arrays only)
    pub fn insert_at(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        if self.shape.len() != 1 {
            return Err(VmError::DimensionMismatch {
                expected: 1,
                got: self.shape.len(),
            });
        }
        // Julia uses 1-indexed, so index 1 means position 0 in Rust
        let rust_index = index.saturating_sub(1);
        if rust_index > self.data.raw_len() {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: self.shape.clone(),
            });
        }
        match (&mut self.data, value) {
            (ArrayData::F64(v), Value::F64(f)) => v.insert(rust_index, f),
            (ArrayData::F64(v), Value::I64(i)) => v.insert(rust_index, i as f64),
            (ArrayData::I64(v), Value::I64(i)) => v.insert(rust_index, i),
            (ArrayData::Bool(v), Value::Bool(b)) => v.insert(rust_index, b),
            (ArrayData::Bool(v), Value::I64(b)) => v.insert(rust_index, b != 0),
            (ArrayData::String(v), Value::Str(s)) => v.insert(rust_index, s),
            (ArrayData::Char(v), Value::Char(c)) => v.insert(rust_index, c),
            (ArrayData::StructRefs(v), Value::StructRef(idx)) => v.insert(rust_index, idx),
            (ArrayData::Any(v), val) => v.insert(rust_index, val),
            (data, val) => {
                return Err(VmError::TypeError(format!(
                    "Cannot insert {:?} into {:?} array",
                    val.value_type(),
                    data.element_type()
                )));
            }
        }
        self.shape[0] = self.data.raw_len();
        Ok(())
    }

    /// Delete value at specific 1-indexed position (1D arrays only)
    pub fn delete_at(&mut self, index: usize) -> Result<Value, VmError> {
        if self.shape.len() != 1 {
            return Err(VmError::DimensionMismatch {
                expected: 1,
                got: self.shape.len(),
            });
        }
        let rust_index = index.saturating_sub(1);
        if rust_index >= self.data.raw_len() {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: self.shape.clone(),
            });
        }
        let val = match &mut self.data {
            ArrayData::F32(v) => Value::F64(v.remove(rust_index) as f64),
            ArrayData::F64(v) => Value::F64(v.remove(rust_index)),
            ArrayData::I8(v) => Value::I64(v.remove(rust_index) as i64),
            ArrayData::I16(v) => Value::I64(v.remove(rust_index) as i64),
            ArrayData::I32(v) => Value::I64(v.remove(rust_index) as i64),
            ArrayData::I64(v) => Value::I64(v.remove(rust_index)),
            ArrayData::U8(v) => Value::I64(v.remove(rust_index) as i64),
            ArrayData::U16(v) => Value::I64(v.remove(rust_index) as i64),
            ArrayData::U32(v) => Value::I64(v.remove(rust_index) as i64),
            ArrayData::U64(v) => Value::I64(v.remove(rust_index) as i64),
            ArrayData::Bool(v) => Value::Bool(v.remove(rust_index)),
            ArrayData::String(v) => Value::Str(v.remove(rust_index)),
            ArrayData::Char(v) => Value::Char(v.remove(rust_index)),
            ArrayData::StructRefs(v) => Value::StructRef(v.remove(rust_index)),
            ArrayData::Any(v) => v.remove(rust_index),
        };
        self.shape[0] = self.data.raw_len();
        Ok(val)
    }

    /// Set complex value (re, im) at indices (for complex arrays with interleaved storage)
    pub fn set_complex(&mut self, indices: &[i64], re: f64, im: f64) -> Result<(), VmError> {
        let linear = self.linear_index(indices)?;

        // Handle interleaved complex storage
        if let Some(ref elem_type) = self.element_type_override {
            match elem_type {
                ArrayElementType::ComplexF64 => {
                    if let ArrayData::F64(ref mut data) = self.data {
                        let offset = linear * 2;
                        if offset + 1 < data.len() {
                            data[offset] = re;
                            data[offset + 1] = im;
                            return Ok(());
                        }
                        return Err(VmError::IndexOutOfBounds {
                            indices: indices.to_vec(),
                            shape: self.shape.clone(),
                        });
                    }
                }
                ArrayElementType::ComplexF32 => {
                    if let ArrayData::F32(ref mut data) = self.data {
                        let offset = linear * 2;
                        if offset + 1 < data.len() {
                            data[offset] = re as f32;
                            data[offset + 1] = im as f32;
                            return Ok(());
                        }
                        return Err(VmError::IndexOutOfBounds {
                            indices: indices.to_vec(),
                            shape: self.shape.clone(),
                        });
                    }
                }
                _ => {}
            }
        }

        Err(VmError::TypeError(format!(
            "set_complex: unsupported storage format for complex array (linear index: {}), re={}, im={}",
            linear, re, im
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::ArrayValue;
    use super::Value;

    // ── set / set_f64 ────────────────────────────────────────────────────────

    #[test]
    fn test_mutation_set_f64_element() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0], vec![3]);
        arr.set(&[2], Value::F64(99.0)).unwrap();
        let val = arr.get(&[2]).unwrap();
        assert!(matches!(val, Value::F64(v) if (v - 99.0).abs() < 1e-12));
    }

    #[test]
    fn test_mutation_set_i64_element() {
        let mut arr = ArrayValue::from_i64(vec![10, 20, 30], vec![3]);
        arr.set(&[1], Value::I64(55)).unwrap();
        let val = arr.get(&[1]).unwrap();
        assert!(matches!(val, Value::I64(55)));
    }

    #[test]
    fn test_mutation_set_out_of_bounds_returns_err() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        assert!(arr.set(&[5], Value::F64(0.0)).is_err());
    }

    #[test]
    fn test_mutation_set_f64_helper() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        arr.set_f64(&[1], 42.0).unwrap();
        assert!(matches!(arr.get(&[1]).unwrap(), Value::F64(v) if (v - 42.0).abs() < 1e-12));
    }

    // ── push / push_f64 ──────────────────────────────────────────────────────

    #[test]
    fn test_mutation_push_f64_increases_len() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        arr.push(Value::F64(3.0)).unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.shape, vec![3]);
    }

    #[test]
    fn test_mutation_push_i64_increases_len() {
        let mut arr = ArrayValue::from_i64(vec![1, 2], vec![2]);
        arr.push(Value::I64(3)).unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn test_mutation_push_2d_array_returns_err() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]);
        assert!(arr.push(Value::F64(5.0)).is_err());
    }

    #[test]
    fn test_mutation_push_f64_helper() {
        let mut arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        arr.push_f64(7.0).unwrap();
        assert_eq!(arr.len(), 2);
        assert!(matches!(arr.get(&[2]).unwrap(), Value::F64(v) if (v - 7.0).abs() < 1e-12));
    }

    // ── pop / pop_f64 ────────────────────────────────────────────────────────

    #[test]
    fn test_mutation_pop_f64_returns_last_element() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0], vec![3]);
        let val = arr.pop().unwrap();
        assert!(matches!(val, Value::F64(v) if (v - 3.0).abs() < 1e-12));
        assert_eq!(arr.len(), 2);
        assert_eq!(arr.shape, vec![2]);
    }

    #[test]
    fn test_mutation_pop_i64_returns_last_element() {
        let mut arr = ArrayValue::from_i64(vec![10, 20, 30], vec![3]);
        let val = arr.pop().unwrap();
        assert!(matches!(val, Value::I64(30)));
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_mutation_pop_empty_returns_err() {
        let mut arr = ArrayValue::from_f64(vec![], vec![0]);
        assert!(arr.pop().is_err());
    }

    #[test]
    fn test_mutation_pop_f64_helper() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 5.5], vec![2]);
        let v = arr.pop_f64().unwrap();
        assert!((v - 5.5).abs() < 1e-12);
        assert_eq!(arr.len(), 1);
    }

    // ── push_first ───────────────────────────────────────────────────────────

    #[test]
    fn test_mutation_push_first_prepends_element() {
        let mut arr = ArrayValue::from_f64(vec![2.0, 3.0], vec![2]);
        arr.push_first(Value::F64(1.0)).unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.shape, vec![3]);
        assert!(matches!(arr.get(&[1]).unwrap(), Value::F64(v) if (v - 1.0).abs() < 1e-12));
    }

    #[test]
    fn test_mutation_push_first_i64_prepends() {
        let mut arr = ArrayValue::from_i64(vec![2, 3], vec![2]);
        arr.push_first(Value::I64(1)).unwrap();
        assert!(matches!(arr.get(&[1]).unwrap(), Value::I64(1)));
    }

    // ── pop_first ────────────────────────────────────────────────────────────

    #[test]
    fn test_mutation_pop_first_removes_first_element() {
        let mut arr = ArrayValue::from_f64(vec![10.0, 20.0, 30.0], vec![3]);
        let val = arr.pop_first().unwrap();
        assert!(matches!(val, Value::F64(v) if (v - 10.0).abs() < 1e-12));
        assert_eq!(arr.len(), 2);
        assert_eq!(arr.shape, vec![2]);
    }

    #[test]
    fn test_mutation_pop_first_empty_returns_err() {
        let mut arr = ArrayValue::from_f64(vec![], vec![0]);
        assert!(arr.pop_first().is_err());
    }

    // ── insert_at ────────────────────────────────────────────────────────────

    #[test]
    fn test_mutation_insert_at_middle() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 3.0], vec![2]);
        arr.insert_at(2, Value::F64(2.0)).unwrap(); // insert at 1-indexed position 2
        assert_eq!(arr.len(), 3);
        assert!(matches!(arr.get(&[2]).unwrap(), Value::F64(v) if (v - 2.0).abs() < 1e-12));
    }

    #[test]
    fn test_mutation_insert_at_out_of_bounds_returns_err() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        assert!(arr.insert_at(10, Value::F64(0.0)).is_err());
    }

    // ── delete_at ────────────────────────────────────────────────────────────

    #[test]
    fn test_mutation_delete_at_removes_element() {
        let mut arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0], vec![3]);
        let val = arr.delete_at(2).unwrap(); // delete 1-indexed position 2
        assert!(matches!(val, Value::F64(v) if (v - 2.0).abs() < 1e-12));
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_mutation_delete_at_out_of_bounds_returns_err() {
        let mut arr = ArrayValue::from_f64(vec![1.0], vec![1]);
        assert!(arr.delete_at(5).is_err());
    }
}
