//! Iteration operations for values.

// SAFETY: i64→usize casts for range/array/struct iteration are from `r.length()` (≥ 0)
// or from iteration state indices that are non-negative by construction.
#![allow(clippy::cast_sign_loss)]

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::field_indices::{
    ARRAY_FIRST_DIM_INDEX, ARRAY_SECOND_DIM_INDEX, FIRST_FIELD_INDEX, FOURTH_FIELD_INDEX,
    SECOND_FIELD_INDEX, THIRD_FIELD_INDEX,
};
use crate::vm::value::{
    new_array_ref, ArrayData, ArrayValue, StructInstance, TupleValue, Value,
};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Get the type_id for CartesianIndex from struct_defs (cached).
    /// Falls back to 0 if not found (for backwards compatibility).
    fn get_cartesian_index_type_id(&self) -> usize {
        if let Some(id) = self.cached_cartesian_index_type_id.get() {
            return id;
        }
        let id = self
            .struct_defs
            .iter()
            .position(|def| def.name == "CartesianIndex")
            .unwrap_or(0);
        self.cached_cartesian_index_type_id.set(Some(id));
        id
    }

    /// Get the type_id for Pair from struct_defs (cached).
    /// Falls back to 0 if not found (for backwards compatibility).
    fn get_pair_type_id(&self) -> usize {
        if let Some(id) = self.cached_pair_type_id.get() {
            return id;
        }
        let id = self
            .struct_defs
            .iter()
            .position(|def| def.name == "Pair")
            .unwrap_or(0);
        self.cached_pair_type_id.set(Some(id));
        id
    }

    /// Check if a value is missing.
    fn is_missing(&self, val: &Value) -> bool {
        matches!(val, Value::Missing)
    }

    pub(in crate::vm) fn iterate_first(&self, coll: &Value) -> Result<Value, VmError> {
        match coll {
            Value::Array(arr) => {
                let arr_borrow = arr.borrow();
                let len = arr_borrow.data.raw_len();
                if len == 0 {
                    Ok(Value::Nothing)
                } else {
                    // Return first element
                    let elem =
                        arr_borrow
                            .data
                            .get_value(0)
                            .ok_or_else(|| VmError::IndexOutOfBounds {
                                indices: vec![1],
                                shape: vec![len],
                            })?;
                    let state = Value::I64(1);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
            Value::Range(r) => {
                let len = r.length() as usize;
                if len == 0 {
                    Ok(Value::Nothing)
                } else {
                    let elem = if r.is_integer_range() {
                        Value::I64(r.start as i64)
                    } else {
                        Value::F64(r.start)
                    };
                    let state = Value::I64(1); // next index
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
            Value::Tuple(t) => {
                if t.elements.is_empty() {
                    Ok(Value::Nothing)
                } else {
                    let elem = t.elements[0].clone();
                    let state = Value::I64(1);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
            Value::Str(s) => {
                if s.is_empty() {
                    Ok(Value::Nothing)
                } else {
                    // Return first character as Char (Julia's behavior)
                    // Safety: guarded by !s.is_empty() above
                    let first_char = match s.chars().next() {
                        Some(c) => c,
                        None => return Ok(Value::Nothing),
                    };
                    let elem = Value::Char(first_char);
                    let state = Value::I64(1);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
            // CartesianIndices iteration support
            Value::Struct(s) if s.struct_name == "CartesianIndices" => {
                // Get dims from the struct
                let dims = s
                    .values
                    .first()
                    .cloned()
                    .unwrap_or(Value::Tuple(TupleValue { elements: vec![] }));
                if let Value::Tuple(dims_tuple) = dims {
                    if dims_tuple.elements.is_empty() {
                        // 0-dimensional: return single CartesianIndex(()) then done
                        let ci = StructInstance {
                            type_id: self.get_cartesian_index_type_id(),
                            struct_name: "CartesianIndex".to_string(),
                            values: vec![Value::Tuple(TupleValue { elements: vec![] })],
                        };
                        let state = Value::Bool(true); // Signal done after first
                        Ok(Value::Tuple(TupleValue {
                            elements: vec![Value::Struct(ci), state],
                        }))
                    } else {
                        // Check if any dimension is 0
                        for d in &dims_tuple.elements {
                            if let Value::I64(v) = d {
                                if *v <= 0 {
                                    return Ok(Value::Nothing);
                                }
                            }
                        }
                        // Start at (1, 1, ..., 1)
                        let n = dims_tuple.elements.len();
                        let first_idx: Vec<Value> = (0..n).map(|_| Value::I64(1)).collect();
                        let ci = StructInstance {
                            type_id: self.get_cartesian_index_type_id(),
                            struct_name: "CartesianIndex".to_string(),
                            values: vec![Value::Tuple(TupleValue {
                                elements: first_idx.clone(),
                            })],
                        };
                        // State is the current index tuple
                        let state = Value::Tuple(TupleValue {
                            elements: first_idx,
                        });
                        Ok(Value::Tuple(TupleValue {
                            elements: vec![Value::Struct(ci), state],
                        }))
                    }
                } else {
                    Err(VmError::TypeError(
                        "CartesianIndices: dims must be a Tuple".to_string(),
                    ))
                }
            }
            Value::StructRef(idx) => {
                // Handle StructRef for struct types that should delegate to Julia iterate methods
                if let Some(s) = self.struct_heap.get(*idx) {
                    match s.struct_name.as_str() {
                        "CartesianIndices" => {
                            let dims = s
                                .values
                                .first()
                                .cloned()
                                .unwrap_or(Value::Tuple(TupleValue { elements: vec![] }));
                            if let Value::Tuple(dims_tuple) = dims {
                                if dims_tuple.elements.is_empty() {
                                    let ci = StructInstance {
                                        type_id: self.get_cartesian_index_type_id(),
                                        struct_name: "CartesianIndex".to_string(),
                                        values: vec![Value::Tuple(TupleValue { elements: vec![] })],
                                    };
                                    let state = Value::Bool(true);
                                    return Ok(Value::Tuple(TupleValue {
                                        elements: vec![Value::Struct(ci), state],
                                    }));
                                } else {
                                    for d in &dims_tuple.elements {
                                        if let Value::I64(v) = d {
                                            if *v <= 0 {
                                                return Ok(Value::Nothing);
                                            }
                                        }
                                    }
                                    let n = dims_tuple.elements.len();
                                    let first_idx: Vec<Value> =
                                        (0..n).map(|_| Value::I64(1)).collect();
                                    let ci = StructInstance {
                                        type_id: self.get_cartesian_index_type_id(),
                                        struct_name: "CartesianIndex".to_string(),
                                        values: vec![Value::Tuple(TupleValue {
                                            elements: first_idx.clone(),
                                        })],
                                    };
                                    let state = Value::Tuple(TupleValue {
                                        elements: first_idx,
                                    });
                                    return Ok(Value::Tuple(TupleValue {
                                        elements: vec![Value::Struct(ci), state],
                                    }));
                                }
                            }
                        }
                        "OneTo" => {
                            // OneTo iteration: simple range iteration
                            if let Some(stop_val) = s.values.first() {
                                if let Value::I64(stop) = stop_val {
                                    if *stop <= 0 {
                                        return Ok(Value::Nothing);
                                    }
                                    // Return first element (1) and state (2)
                                    return Ok(Value::Tuple(TupleValue {
                                        elements: vec![Value::I64(1), Value::I64(2)],
                                    }));
                                } else {
                                    return Err(VmError::TypeError(
                                        "OneTo: stop must be an integer".to_string(),
                                    ));
                                }
                            }
                        }
                        "EachCol" => {
                            // EachCol iteration: iterate over columns of a matrix
                            if let Some(mat) = s.values.first() {
                                match mat {
                                    Value::Array(arr) => {
                                        let arr_borrow = arr.borrow();
                                        let shape = &arr_borrow.shape;
                                        if shape.len() < 2 {
                                            // 1D array: treat as single column
                                            let col = mat.clone();
                                            return Ok(Value::Tuple(TupleValue {
                                                elements: vec![col, Value::I64(2)],
                                            }));
                                        }
                                        let ncols = shape[ARRAY_SECOND_DIM_INDEX];
                                        if ncols == 0 {
                                            return Ok(Value::Nothing);
                                        }
                                        // Return first column by extracting column slice
                                        let nrows = shape[ARRAY_FIRST_DIM_INDEX];
                                        let mut column_data = Vec::new();
                                        for row in 0..nrows {
                                            let indices = vec![row as i64 + 1, 1]; // Julia 1-indexed
                                            let elem = arr_borrow.get(&indices)?;
                                            column_data.push(elem);
                                        }
                                        let col_arr = ArrayValue::any_vector(column_data);
                                        let col = Value::Array(new_array_ref(col_arr));
                                        return Ok(Value::Tuple(TupleValue {
                                            elements: vec![col, Value::I64(2)],
                                        }));
                                    }
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "EachCol: matrix must be an Array".to_string(),
                                        ))
                                    }
                                }
                            }
                        }
                        "EachRow" => {
                            // EachRow iteration: iterate over rows of a matrix
                            if let Some(mat) = s.values.first() {
                                match mat {
                                    Value::Array(arr) => {
                                        let arr_borrow = arr.borrow();
                                        let shape = &arr_borrow.shape;
                                        if shape.len() < 2 {
                                            // 1D array: each element is a "row"
                                            return self.iterate_first(mat);
                                        }
                                        let nrows = shape[ARRAY_FIRST_DIM_INDEX];
                                        if nrows == 0 {
                                            return Ok(Value::Nothing);
                                        }
                                        // Return first row by extracting row slice
                                        let ncols = shape[ARRAY_SECOND_DIM_INDEX];
                                        let mut row_data = Vec::new();
                                        for col in 0..ncols {
                                            let indices = vec![1, col as i64 + 1]; // Julia 1-indexed
                                            let elem = arr_borrow.get(&indices)?;
                                            row_data.push(elem);
                                        }
                                        let row_arr = ArrayValue::any_vector(row_data);
                                        let row = Value::Array(new_array_ref(row_arr));
                                        return Ok(Value::Tuple(TupleValue {
                                            elements: vec![row, Value::I64(2)],
                                        }));
                                    }
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "EachRow: matrix must be an Array".to_string(),
                                        ))
                                    }
                                }
                            }
                        }
                        "EachSlice" => {
                            // EachSlice iteration: iterate over slices along a specified dimension
                            if let (Some(mat), Some(dim_val)) = (s.values.first(), s.values.get(1))
                            {
                                let dim = match dim_val {
                                    Value::I64(d) => *d as usize,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "EachSlice: dim must be an integer".to_string(),
                                        ))
                                    }
                                };
                                match mat {
                                    Value::Array(arr) => {
                                        let arr_borrow = arr.borrow();
                                        let shape = &arr_borrow.shape;
                                        if shape.len() < 2 {
                                            if dim == 1 {
                                                return self.iterate_first(mat);
                                            } else {
                                                let col = mat.clone();
                                                return Ok(Value::Tuple(TupleValue {
                                                    elements: vec![col, Value::I64(2)],
                                                }));
                                            }
                                        }
                                        let n = shape[dim - 1];
                                        if n == 0 {
                                            return Ok(Value::Nothing);
                                        }
                                        if dim == 1 {
                                            // Return first row
                                            let ncols = shape[ARRAY_SECOND_DIM_INDEX];
                                            let mut row_data = Vec::new();
                                            for col in 0..ncols {
                                                let indices = vec![1, col as i64 + 1];
                                                let elem = arr_borrow.get(&indices)?;
                                                row_data.push(elem);
                                            }
                                            let row_arr = ArrayValue::any_vector(row_data);
                                            let row = Value::Array(new_array_ref(row_arr));
                                            return Ok(Value::Tuple(TupleValue {
                                                elements: vec![row, Value::I64(2)],
                                            }));
                                        } else {
                                            // Return first column
                                            let nrows = shape[ARRAY_FIRST_DIM_INDEX];
                                            let mut column_data = Vec::new();
                                            for row in 0..nrows {
                                                let indices = vec![row as i64 + 1, 1];
                                                let elem = arr_borrow.get(&indices)?;
                                                column_data.push(elem);
                                            }
                                            let col_arr = ArrayValue::any_vector(column_data);
                                            let col = Value::Array(new_array_ref(col_arr));
                                            return Ok(Value::Tuple(TupleValue {
                                                elements: vec![col, Value::I64(2)],
                                            }));
                                        }
                                    }
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "EachSlice: matrix must be an Array".to_string(),
                                        ))
                                    }
                                }
                            }
                        }
                        "SkipMissing" => {
                            // SkipMissing iteration: iterate over the underlying collection, skipping missing values
                            // The Julia-defined iterate methods handle the logic
                            if let Some(inner_coll) = s.values.first() {
                                // Start by iterating the inner collection
                                let next = self.iterate_first(inner_coll)?;
                                if matches!(next, Value::Nothing) {
                                    return Ok(Value::Nothing);
                                }
                                // Extract (val, state) from the result
                                if let Value::Tuple(t) = &next {
                                    if t.elements.len() == 2 {
                                        let val = &t.elements[0];
                                        let state = &t.elements[1];
                                        // Check if value is missing
                                        if self.is_missing(val) {
                                            // Skip this missing value and continue to next
                                            return self.iterate_next(coll, state);
                                        }
                                        // Return the non-missing value with state
                                        return Ok(next);
                                    }
                                }
                                return Ok(next);
                            }
                        }
                        name if name.starts_with("LinRange") => {
                            // LinRange iteration: linearly spaced range
                            // Fields: start, stop, len, lendiv
                            if s.values.len() >= 3 {
                                let len = match &s.values[THIRD_FIELD_INDEX] {
                                    Value::I64(n) => *n,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "LinRange: len must be I64".to_string(),
                                        ))
                                    }
                                };
                                if len <= 0 {
                                    return Ok(Value::Nothing);
                                }
                                // Return first element using lerp formula: start (when i=1)
                                let start = match &s.values[FIRST_FIELD_INDEX] {
                                    Value::F64(f) => *f,
                                    Value::I64(i) => *i as f64,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "LinRange: start must be numeric".to_string(),
                                        ))
                                    }
                                };
                                return Ok(Value::Tuple(TupleValue {
                                    elements: vec![Value::F64(start), Value::I64(1)],
                                }));
                            }
                        }
                        name if name.starts_with("StepRangeLen") => {
                            // StepRangeLen iteration: range with reference, step, length, offset
                            // Fields: ref, step, len, offset
                            if s.values.len() >= 4 {
                                let len = match &s.values[THIRD_FIELD_INDEX] {
                                    Value::I64(n) => *n,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: len must be I64".to_string(),
                                        ))
                                    }
                                };
                                if len <= 0 {
                                    return Ok(Value::Nothing);
                                }
                                let ref_val = match &s.values[FIRST_FIELD_INDEX] {
                                    Value::F64(f) => *f,
                                    Value::I64(i) => *i as f64,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: ref must be numeric".to_string(),
                                        ))
                                    }
                                };
                                let step_val = match &s.values[SECOND_FIELD_INDEX] {
                                    Value::F64(f) => *f,
                                    Value::I64(i) => *i as f64,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: step must be numeric".to_string(),
                                        ))
                                    }
                                };
                                let offset = match &s.values[FOURTH_FIELD_INDEX] {
                                    Value::I64(n) => *n,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: offset must be I64".to_string(),
                                        ))
                                    }
                                };
                                // First element: ref + (1 - offset) * step
                                let first_val = ref_val + (1.0 - offset as f64) * step_val;
                                return Ok(Value::Tuple(TupleValue {
                                    elements: vec![Value::F64(first_val), Value::I64(1)],
                                }));
                            }
                        }
                        _ => {
                            // Other struct types are unsupported
                        }
                    }
                }
                Err(VmError::TypeError(format!(
                    "iterate: unsupported struct type for StructRef({})",
                    idx
                )))
            }
            // Generator iteration: iterate over the underlying iterator
            Value::Generator(g) => self.iterate_first(g.iter.as_ref()),
            // Dict iteration: returns Pair(key, value) for each entry
            Value::Dict(d) => match d.next_filled_slot(0) {
                Some((idx, key, val)) => {
                    let pair = StructInstance {
                        type_id: self.get_pair_type_id(),
                        struct_name: "Pair".to_string(),
                        values: vec![key.to_value(), val.clone()],
                    };
                    let state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![Value::Struct(pair), state],
                    }))
                }
                None => Ok(Value::Nothing),
            },
            // Set iteration: returns each element directly
            Value::Set(s) => {
                if s.elements.is_empty() {
                    Ok(Value::Nothing)
                } else {
                    let elem = s.elements[0].to_value();
                    let state = Value::I64(1);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, state],
                    }))
                }
            }
            // Scalar number iteration (Julia: iterate(x::Number) = (x, nothing))
            // Numbers iterate exactly once, yielding themselves.
            Value::I64(_) | Value::F64(_) | Value::F32(_) | Value::Bool(_) => {
                Ok(Value::Tuple(TupleValue {
                    elements: vec![coll.clone(), Value::Nothing],
                }))
            }
            _ => Err(VmError::TypeError(format!(
                "iterate: unsupported collection type {:?}",
                coll
            ))),
        }
    }

    /// Subsequent iteration: iterate(collection, state) -> (element, state) or nothing
    /// State is 0-indexed - it represents the next index to fetch
    pub(in crate::vm) fn iterate_next(&self, coll: &Value, state: &Value) -> Result<Value, VmError> {
        // Scalar number iteration (Julia: iterate(x::Number, ::Nothing) = nothing)
        // After yielding once, scalar iteration is done.
        if matches!(
            coll,
            Value::I64(_) | Value::F64(_) | Value::F32(_) | Value::Bool(_)
        ) && matches!(state, Value::Nothing)
        {
            return Ok(Value::Nothing);
        }

        // Handle CartesianIndices with Tuple or Bool state
        if let Some(dims) = self.get_cartesian_indices_dims(coll) {
            return self.iterate_next_cartesian_indices(&dims, state);
        }

        let idx = match state {
            Value::I64(i) => *i as usize,
            _ => return Err(VmError::TypeError("iterate: state must be I64".to_string())),
        };

        match coll {
            Value::Array(arr) => {
                let arr_borrow = arr.borrow();
                if idx >= arr_borrow.len() {
                    Ok(Value::Nothing)
                } else {
                    let elem = arr_borrow.data.get_value(idx).ok_or_else(|| {
                        VmError::IndexOutOfBounds {
                            indices: vec![idx as i64],
                            shape: vec![arr_borrow.len()],
                        }
                    })?;
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            Value::Range(r) => {
                let len = r.length() as usize;
                if idx >= len {
                    Ok(Value::Nothing)
                } else {
                    let val = r.start + (idx as f64) * r.step;
                    let elem = if r.is_integer_range() {
                        Value::I64(val as i64)
                    } else {
                        Value::F64(val)
                    };
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            Value::Tuple(t) => {
                if idx >= t.elements.len() {
                    Ok(Value::Nothing)
                } else {
                    let elem = t.elements[idx].clone();
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            Value::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                if idx >= chars.len() {
                    Ok(Value::Nothing)
                } else {
                    // Return character as Char (Julia's behavior)
                    let elem = Value::Char(chars[idx]);
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            // Generator iteration: iterate over the underlying iterator
            Value::Generator(g) => self.iterate_next(g.iter.as_ref(), state),
            Value::StructRef(struct_idx) => {
                // Handle StructRef iteration for various struct types
                if let Some(s) = self.struct_heap.get(*struct_idx) {
                    match s.struct_name.as_str() {
                        "OneTo" => {
                            if let Some(Value::I64(stop)) = s.values.first() {
                                if idx as i64 > *stop {
                                    Ok(Value::Nothing)
                                } else {
                                    let elem = Value::I64(idx as i64);
                                    let next_state = Value::I64((idx + 1) as i64);
                                    Ok(Value::Tuple(TupleValue {
                                        elements: vec![elem, next_state],
                                    }))
                                }
                            } else {
                                Err(VmError::TypeError(
                                    "OneTo: stop must be an integer".to_string(),
                                ))
                            }
                        }
                        "EachCol" => {
                            // EachCol iteration: subsequent column access
                            if let Some(mat) = s.values.first() {
                                match mat {
                                    Value::Array(arr) => {
                                        let arr_borrow = arr.borrow();
                                        let shape = &arr_borrow.shape;
                                        if shape.len() < 2 {
                                            // 1D array: only one column
                                            return Ok(Value::Nothing);
                                        }
                                        let ncols = shape[ARRAY_SECOND_DIM_INDEX];
                                        if idx > ncols {
                                            return Ok(Value::Nothing);
                                        }
                                        // Return column at index idx (1-indexed)
                                        let nrows = shape[ARRAY_FIRST_DIM_INDEX];
                                        let mut column_data = Vec::new();
                                        for row in 0..nrows {
                                            let indices = vec![row as i64 + 1, idx as i64];
                                            let elem = arr_borrow.get(&indices)?;
                                            column_data.push(elem);
                                        }
                                        let col_arr = ArrayValue::any_vector(column_data);
                                        let col = Value::Array(new_array_ref(col_arr));
                                        let next_state = Value::I64((idx + 1) as i64);
                                        Ok(Value::Tuple(TupleValue {
                                            elements: vec![col, next_state],
                                        }))
                                    }
                                    _ => Err(VmError::TypeError(
                                        "EachCol: matrix must be an Array".to_string(),
                                    )),
                                }
                            } else {
                                Err(VmError::TypeError(
                                    "EachCol: missing matrix value".to_string(),
                                ))
                            }
                        }
                        "EachRow" => {
                            // EachRow iteration: subsequent row access
                            if let Some(mat) = s.values.first() {
                                match mat {
                                    Value::Array(arr) => {
                                        let arr_borrow = arr.borrow();
                                        let shape = &arr_borrow.shape;
                                        if shape.len() < 2 {
                                            // 1D array: delegate to array iteration
                                            return self.iterate_next(mat, state);
                                        }
                                        let nrows = shape[ARRAY_FIRST_DIM_INDEX];
                                        if idx > nrows {
                                            return Ok(Value::Nothing);
                                        }
                                        // Return row at index idx (1-indexed)
                                        let ncols = shape[ARRAY_SECOND_DIM_INDEX];
                                        let mut row_data = Vec::new();
                                        for col in 0..ncols {
                                            let indices = vec![idx as i64, col as i64 + 1];
                                            let elem = arr_borrow.get(&indices)?;
                                            row_data.push(elem);
                                        }
                                        let row_arr = ArrayValue::any_vector(row_data);
                                        let row = Value::Array(new_array_ref(row_arr));
                                        let next_state = Value::I64((idx + 1) as i64);
                                        Ok(Value::Tuple(TupleValue {
                                            elements: vec![row, next_state],
                                        }))
                                    }
                                    _ => Err(VmError::TypeError(
                                        "EachRow: matrix must be an Array".to_string(),
                                    )),
                                }
                            } else {
                                Err(VmError::TypeError(
                                    "EachRow: missing matrix value".to_string(),
                                ))
                            }
                        }
                        "EachSlice" => {
                            // EachSlice iteration: subsequent slice access
                            if let (Some(mat), Some(dim_val)) = (s.values.first(), s.values.get(1))
                            {
                                let dim = match dim_val {
                                    Value::I64(d) => *d as usize,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "EachSlice: dim must be an integer".to_string(),
                                        ))
                                    }
                                };
                                match mat {
                                    Value::Array(arr) => {
                                        let arr_borrow = arr.borrow();
                                        let shape = &arr_borrow.shape;
                                        if shape.len() < 2 {
                                            if dim == 1 {
                                                return self.iterate_next(mat, state);
                                            } else {
                                                return Ok(Value::Nothing);
                                            }
                                        }
                                        let n = shape[dim - 1];
                                        if idx > n {
                                            return Ok(Value::Nothing);
                                        }
                                        if dim == 1 {
                                            // Return row at index idx
                                            let ncols = shape[ARRAY_SECOND_DIM_INDEX];
                                            let mut row_data = Vec::new();
                                            for col in 0..ncols {
                                                let indices = vec![idx as i64, col as i64 + 1];
                                                let elem = arr_borrow.get(&indices)?;
                                                row_data.push(elem);
                                            }
                                            let row_arr = ArrayValue::any_vector(row_data);
                                            let row = Value::Array(new_array_ref(row_arr));
                                            let next_state = Value::I64((idx + 1) as i64);
                                            Ok(Value::Tuple(TupleValue {
                                                elements: vec![row, next_state],
                                            }))
                                        } else {
                                            // Return column at index idx
                                            let nrows = shape[ARRAY_FIRST_DIM_INDEX];
                                            let mut column_data = Vec::new();
                                            for row in 0..nrows {
                                                let indices = vec![row as i64 + 1, idx as i64];
                                                let elem = arr_borrow.get(&indices)?;
                                                column_data.push(elem);
                                            }
                                            let col_arr = ArrayValue::any_vector(column_data);
                                            let col = Value::Array(new_array_ref(col_arr));
                                            let next_state = Value::I64((idx + 1) as i64);
                                            Ok(Value::Tuple(TupleValue {
                                                elements: vec![col, next_state],
                                            }))
                                        }
                                    }
                                    _ => Err(VmError::TypeError(
                                        "EachSlice: matrix must be an Array".to_string(),
                                    )),
                                }
                            } else {
                                Err(VmError::TypeError("EachSlice: missing values".to_string()))
                            }
                        }
                        "SkipMissing" => {
                            // SkipMissing iteration: continue iterating the inner collection, skipping missing values
                            if let Some(inner_coll) = s.values.first() {
                                // Continue iterating the inner collection with the given state
                                let next = self.iterate_next(inner_coll, state)?;
                                if matches!(next, Value::Nothing) {
                                    return Ok(Value::Nothing);
                                }
                                // Extract (val, newstate) from the result
                                if let Value::Tuple(t) = &next {
                                    if t.elements.len() == 2 {
                                        let val = &t.elements[0];
                                        let newstate = &t.elements[1];
                                        // Check if value is missing
                                        if self.is_missing(val) {
                                            // Skip this missing value and continue to next
                                            return self.iterate_next(coll, newstate);
                                        }
                                        // Return the non-missing value with new state
                                        return Ok(next);
                                    }
                                }
                                Ok(next)
                            } else {
                                Err(VmError::TypeError(
                                    "SkipMissing: missing inner collection".to_string(),
                                ))
                            }
                        }
                        name if name.starts_with("LinRange") => {
                            // LinRange iteration: next element
                            // Fields: start, stop, len, lendiv
                            // State (idx) is 1-based index of the PREVIOUS element returned
                            // We need to compute element at idx+1 (next element)
                            if s.values.len() >= 4 {
                                let len = match &s.values[THIRD_FIELD_INDEX] {
                                    Value::I64(n) => *n,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "LinRange: len must be I64".to_string(),
                                        ))
                                    }
                                };
                                let next_idx = idx + 1; // Compute the next element's 1-based index
                                if next_idx as i64 > len {
                                    return Ok(Value::Nothing);
                                }
                                let start = match &s.values[FIRST_FIELD_INDEX] {
                                    Value::F64(f) => *f,
                                    Value::I64(i) => *i as f64,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "LinRange: start must be numeric".to_string(),
                                        ))
                                    }
                                };
                                let stop = match &s.values[SECOND_FIELD_INDEX] {
                                    Value::F64(f) => *f,
                                    Value::I64(i) => *i as f64,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "LinRange: stop must be numeric".to_string(),
                                        ))
                                    }
                                };
                                let lendiv = match &s.values[FOURTH_FIELD_INDEX] {
                                    Value::I64(n) => *n,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "LinRange: lendiv must be I64".to_string(),
                                        ))
                                    }
                                };
                                // lerp formula: (1 - t) * start + t * stop where t = (next_idx - 1) / lendiv
                                let t = (next_idx as f64 - 1.0) / lendiv as f64;
                                let elem = (1.0 - t) * start + t * stop;
                                let next_state = Value::I64(next_idx as i64);
                                Ok(Value::Tuple(TupleValue {
                                    elements: vec![Value::F64(elem), next_state],
                                }))
                            } else {
                                Err(VmError::TypeError(
                                    "LinRange: invalid struct fields".to_string(),
                                ))
                            }
                        }
                        name if name.starts_with("StepRangeLen") => {
                            // StepRangeLen iteration: next element
                            // Fields: ref, step, len, offset
                            // State (idx) is 1-based index of the PREVIOUS element returned
                            // We need to compute element at idx+1 (next element)
                            if s.values.len() >= 4 {
                                let len = match &s.values[THIRD_FIELD_INDEX] {
                                    Value::I64(n) => *n,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: len must be I64".to_string(),
                                        ))
                                    }
                                };
                                let next_idx = idx + 1; // Compute the next element's 1-based index
                                if next_idx as i64 > len {
                                    return Ok(Value::Nothing);
                                }
                                let ref_val = match &s.values[FIRST_FIELD_INDEX] {
                                    Value::F64(f) => *f,
                                    Value::I64(i) => *i as f64,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: ref must be numeric".to_string(),
                                        ))
                                    }
                                };
                                let step_val = match &s.values[SECOND_FIELD_INDEX] {
                                    Value::F64(f) => *f,
                                    Value::I64(i) => *i as f64,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: step must be numeric".to_string(),
                                        ))
                                    }
                                };
                                let offset = match &s.values[FOURTH_FIELD_INDEX] {
                                    Value::I64(n) => *n,
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "StepRangeLen: offset must be I64".to_string(),
                                        ))
                                    }
                                };
                                // Element at index next_idx: ref + (next_idx - offset) * step
                                let elem = ref_val + (next_idx as f64 - offset as f64) * step_val;
                                let next_state = Value::I64(next_idx as i64);
                                Ok(Value::Tuple(TupleValue {
                                    elements: vec![Value::F64(elem), next_state],
                                }))
                            } else {
                                Err(VmError::TypeError(
                                    "StepRangeLen: invalid struct fields".to_string(),
                                ))
                            }
                        }
                        _ => Err(VmError::TypeError(format!(
                            "iterate: unsupported struct type for StructRef({})",
                            struct_idx
                        ))),
                    }
                } else {
                    Err(VmError::TypeError(format!(
                        "iterate: invalid StructRef({})",
                        struct_idx
                    )))
                }
            }
            // Dict iteration: returns Pair(key, value) for each entry
            // State (idx) is the next slot index to scan from (0-based).
            Value::Dict(d) => match d.next_filled_slot(idx) {
                Some((slot_idx, key, val)) => {
                    let pair = StructInstance {
                        type_id: self.get_pair_type_id(),
                        struct_name: "Pair".to_string(),
                        values: vec![key.to_value(), val.clone()],
                    };
                    let next_state = Value::I64((slot_idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![Value::Struct(pair), next_state],
                    }))
                }
                None => Ok(Value::Nothing),
            },
            // Set iteration: returns each element directly
            Value::Set(s) => {
                if idx >= s.elements.len() {
                    Ok(Value::Nothing)
                } else {
                    let elem = s.elements[idx].to_value();
                    let next_state = Value::I64((idx + 1) as i64);
                    Ok(Value::Tuple(TupleValue {
                        elements: vec![elem, next_state],
                    }))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "iterate: unsupported collection type {:?}",
                coll
            ))),
        }
    }

    /// Helper: extract dims from CartesianIndices (Value::Struct or Value::StructRef)
    fn get_cartesian_indices_dims(&self, coll: &Value) -> Option<Vec<i64>> {
        match coll {
            Value::Struct(s) if s.struct_name == "CartesianIndices" => {
                if let Some(Value::Tuple(dims_tuple)) = s.values.first() {
                    Some(
                        dims_tuple
                            .elements
                            .iter()
                            .filter_map(|d| {
                                if let Value::I64(v) = d {
                                    Some(*v)
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            }
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    if s.struct_name == "CartesianIndices" {
                        if let Some(Value::Tuple(dims_tuple)) = s.values.first() {
                            return Some(
                                dims_tuple
                                    .elements
                                    .iter()
                                    .filter_map(|d| {
                                        if let Value::I64(v) = d {
                                            Some(*v)
                                        } else {
                                            None
                                        }
                                    })
                                    .collect(),
                            );
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Helper: iterate_next for CartesianIndices
    fn iterate_next_cartesian_indices(
        &self,
        dims: &[i64],
        state: &Value,
    ) -> Result<Value, VmError> {
        // Handle 0-dimensional case (state is Bool)
        if let Value::Bool(true) = state {
            // Already iterated over the single element
            return Ok(Value::Nothing);
        }

        // State is a Tuple of current indices
        let current_indices: Vec<i64> = match state {
            Value::Tuple(t) => t
                .elements
                .iter()
                .filter_map(|e| {
                    if let Value::I64(v) = e {
                        Some(*v)
                    } else {
                        None
                    }
                })
                .collect(),
            _ => {
                return Err(VmError::TypeError(
                    "CartesianIndices state must be Tuple or Bool".to_string(),
                ))
            }
        };

        if current_indices.len() != dims.len() {
            return Err(VmError::TypeError(
                "CartesianIndices state dimension mismatch".to_string(),
            ));
        }

        // Increment indices in column-major order (first index varies fastest)
        let mut next_indices = current_indices.clone();
        let mut carry = true;

        for i in 0..dims.len() {
            if carry {
                next_indices[i] += 1;
                if next_indices[i] > dims[i] {
                    next_indices[i] = 1;
                    // carry remains true
                } else {
                    carry = false;
                }
            }
        }

        // If carry is still true, we've exhausted all indices
        if carry {
            return Ok(Value::Nothing);
        }

        // Create the CartesianIndex with the new indices
        let idx_values: Vec<Value> = next_indices.iter().map(|&v| Value::I64(v)).collect();
        let ci = StructInstance {
            type_id: self.get_cartesian_index_type_id(),
            struct_name: "CartesianIndex".to_string(),
            values: vec![Value::Tuple(TupleValue {
                elements: idx_values.clone(),
            })],
        };
        let new_state = Value::Tuple(TupleValue {
            elements: idx_values,
        });

        Ok(Value::Tuple(TupleValue {
            elements: vec![Value::Struct(ci), new_state],
        }))
    }

    /// Collect iterator into Array
    /// Type-preserving: returns Int64 array for integer ranges/tuples, Float64 otherwise.
    /// For arrays, creates a shallow copy (new array with same element type).
    pub(in crate::vm) fn collect_iterator(&self, iter: &Value) -> Result<Value, VmError> {
        match iter {
            // Array - create a type-preserving copy (not just clone the reference)
            Value::Array(arr) => {
                let borrowed = arr.borrow();
                Ok(Value::Array(new_array_ref(ArrayValue::new(
                    borrowed.data.clone(),
                    borrowed.shape.clone(),
                ))))
            }
            // Range -> Array (type-preserving via RangeValue::collect())
            Value::Range(r) => Ok(Value::Array(new_array_ref(r.collect()))),
            // Tuple -> Array (type-preserving)
            Value::Tuple(t) => {
                if t.elements.is_empty() {
                    // Empty tuple -> empty Float64 array
                    return Ok(Value::Array(new_array_ref(ArrayValue::vector(vec![]))));
                }
                // Check if all elements are Int64
                let all_i64 = t.elements.iter().all(|e| matches!(e, Value::I64(_)));
                if all_i64 {
                    // All Int64 -> Int64 array
                    let data: Vec<i64> = t
                        .elements
                        .iter()
                        .map(|e| match e {
                            Value::I64(i) => Ok(*i),
                            _ => Err(VmError::InternalError(
                                "collect: all_i64 invariant violated for tuple collection"
                                    .to_string(),
                            )),
                        })
                        .collect::<Result<_, _>>()?;
                    let len = data.len();
                    Ok(Value::Array(new_array_ref(ArrayValue::from_i64(
                        data,
                        vec![len],
                    ))))
                } else {
                    // Mixed or Float64 -> Float64 array
                    let mut data = Vec::with_capacity(t.elements.len());
                    for elem in &t.elements {
                        match elem {
                            Value::F64(f) => data.push(*f),
                            Value::I64(i) => data.push(*i as f64),
                            _ => {
                                return Err(VmError::TypeError(
                                    "collect: tuple elements must be numeric".to_string(),
                                ))
                            }
                        }
                    }
                    let len = data.len();
                    Ok(Value::Array(new_array_ref(ArrayValue::from_f64(
                        data,
                        vec![len],
                    ))))
                }
            }
            // String -> Char array (Issue #2027)
            Value::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                let len = chars.len();
                Ok(Value::Array(new_array_ref(ArrayValue::new(
                    ArrayData::Char(chars),
                    vec![len],
                ))))
            }
            _ => Err(VmError::TypeError(format!(
                "collect: unsupported iterator type {:?}",
                iter
            ))),
        }
    }

    /// Collect Generator by applying function to each element.
    /// If func_index is usize::MAX, the generator was eager-evaluated and iter is already the result.
    /// Otherwise, we need to apply the function to each element (for lazy generators).
    pub(in crate::vm) fn collect_generator(
        &self,
        func_index: usize,
        iter: &Value,
    ) -> Result<Value, VmError> {
        if func_index == usize::MAX {
            // Eager-evaluated generator: iter is already the collected array
            // Just return a copy of it
            self.collect_iterator(iter)
        } else {
            // Lazy generator: we need to apply the function to each element
            // For now, just return the underlying iterator collected
            // Full lazy generator support requires more VM infrastructure
            self.collect_iterator(iter)
        }
    }
}
