//! IndexSlice handling for array/string slicing.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// SAFETY: i64→usize casts for slice start/end are from range_indices which are
// 1-based Julia indices verified to be non-empty before use.
#![allow(clippy::cast_sign_loss)]

use super::super::*;
use super::array_index::{
    create_sliced_array, load_selected_array_elements, selected_indices_from_index_array,
};
use super::DispatchAction;
use crate::rng::RngLike;
use crate::vm::value::{
    array_wrapper_shape_and_offset, array_wrapper_value_to_array_value,
    native_array_value_from_array as array_value, native_array_value_ref, new_array_ref,
    RangeValue, StructInstance,
};

// Convert Value indices to internal slice representation.
enum SliceIndex {
    Scalar(i64),
    Range(Vec<i64>),
    All,
}

fn numeric_slice_index_value(v: &Value) -> i64 {
    match v {
        Value::F64(f) => *f as i64,
        Value::I64(n) => *n,
        Value::F32(f) => *f as i64,
        Value::I32(n) => *n as i64,
        Value::I16(n) => *n as i64,
        Value::I8(n) => *n as i64,
        _ => 0,
    }
}

fn logical_values_to_slice_index(values: &[Value]) -> SliceIndex {
    if values.iter().all(|v| matches!(v, Value::Bool(_))) {
        let indices = values
            .iter()
            .enumerate()
            .filter_map(|(i, value)| match value {
                Value::Bool(true) => Some((i + 1) as i64),
                _ => None,
            })
            .collect();
        return SliceIndex::Range(indices);
    }

    let is_boolean_like_f64 = !values.is_empty()
        && values
            .iter()
            .all(|value| matches!(value, Value::F64(f) if *f == 0.0 || *f == 1.0));
    if is_boolean_like_f64 {
        let indices = values
            .iter()
            .enumerate()
            .filter_map(|(i, value)| match value {
                Value::F64(1.0) => Some((i + 1) as i64),
                _ => None,
            })
            .collect();
        return SliceIndex::Range(indices);
    }

    SliceIndex::Range(values.iter().map(numeric_slice_index_value).collect())
}

fn is_array_wrapper_name(name: &str) -> bool {
    let base = name.rsplit('.').next().unwrap_or(name);
    base == "Array" || base.starts_with("Array{")
}

fn array_wrapper_logical_values(instance: &StructInstance) -> Option<Vec<Value>> {
    if !is_array_wrapper_name(&instance.struct_name) {
        return None;
    }

    let mem = instance.values.first()?;
    let size = instance.values.get(1)?;
    let (shape, offset) = array_wrapper_shape_and_offset(size)?;
    let len: usize = shape.iter().product();
    let mut values = Vec::with_capacity(len);

    match mem {
        Value::MemoryRef(memref) => {
            for linear in 0..len {
                values.push(memref.get(linear + 1).ok()?);
            }
        }
        Value::Memory(mem_ref) => {
            let mem_borrow = mem_ref.borrow();
            for linear in 0..len {
                values.push(mem_borrow.get(offset + linear).ok()?);
            }
        }
        // Route the legacy native-Array `_mem` carrier through the
        // file-local helper so the unwrap stays centralized while #3908
        // retires the transitional native container.
        _ if is_native_array_value(mem) => {
            let array_ref = native_array_value_ref(mem)?;
            let array_borrow = array_ref.borrow();
            for linear in 0..len {
                values.push(array_borrow.get_linear(offset - 1 + linear).ok()?);
            }
        }
        _ => return None,
    }

    Some(values)
}

fn value_to_slice_index(v: &Value, struct_heap: &[StructInstance]) -> SliceIndex {
    match v {
        Value::I64(i) => SliceIndex::Scalar(*i),
        Value::F64(f) => SliceIndex::Scalar(*f as i64),
        Value::SliceAll => SliceIndex::All,
        Value::Range(range) => {
            let start = range.start as i64;
            let step = range.step as i64;
            let stop = range.stop as i64;
            if step == 0 {
                return SliceIndex::Range(vec![]);
            }
            let cap = ((stop - start).unsigned_abs() / step.unsigned_abs() + 1) as usize;
            let mut indices = Vec::with_capacity(cap);
            let mut i = start;
            while (step > 0 && i <= stop) || (step < 0 && i >= stop) {
                indices.push(i);
                i += step;
            }
            SliceIndex::Range(indices)
        }
        // Route the legacy native-Array index source through the file-local
        // helper so the unwrap stays centralized while #3908 retires the
        // transitional native container.
        _ if is_native_array_value(v) => {
            let arr_ref = match native_array_value_ref(v) {
                Some(arr_ref) => arr_ref,
                None => return SliceIndex::Scalar(0),
            };
            let arr = arr_ref.borrow();
            match arr.to_logical_value_vec() {
                Ok(values) => logical_values_to_slice_index(&values),
                Err(_) => SliceIndex::Range(Vec::new()),
            }
        }
        Value::StructRef(idx) => {
            if let Some(values) = struct_heap.get(*idx).and_then(array_wrapper_logical_values) {
                logical_values_to_slice_index(&values)
            } else {
                SliceIndex::Scalar(0)
            }
        }
        Value::Struct(instance) => {
            if let Some(values) = array_wrapper_logical_values(instance) {
                logical_values_to_slice_index(&values)
            } else {
                SliceIndex::Scalar(0)
            }
        }
        _ => SliceIndex::Scalar(0),
    }
}

fn resolve_dimension(idx: &SliceIndex, dim_size: usize) -> Vec<i64> {
    match idx {
        SliceIndex::Scalar(i) => vec![*i],
        SliceIndex::Range(r) => r.clone(),
        SliceIndex::All => (1..=dim_size as i64).collect(),
    }
}

fn integer_index_value(v: Value) -> Option<i64> {
    match v {
        Value::I64(n) => Some(n),
        Value::I128(n) => i64::try_from(n).ok(),
        Value::I32(n) => Some(n as i64),
        Value::I16(n) => Some(n as i64),
        Value::I8(n) => Some(n as i64),
        Value::U128(n) => i64::try_from(n).ok(),
        Value::U64(n) => i64::try_from(n).ok(),
        Value::U32(n) => Some(n as i64),
        Value::U16(n) => Some(n as i64),
        Value::U8(n) => Some(n as i64),
        _ => None,
    }
}

fn range_struct_name_base(name: &str) -> &str {
    let unqualified = name.rsplit('.').next().unwrap_or(name);
    unqualified.split('{').next().unwrap_or(unqualified)
}

fn struct_instance_as_range(instance: &StructInstance) -> Option<RangeValue> {
    match range_struct_name_base(&instance.struct_name) {
        "OneTo" => match instance.values.first()? {
            Value::I64(stop) => Some(RangeValue::unit_range(1.0, *stop as f64)),
            _ => None,
        },
        _ => None,
    }
}

fn value_as_range(value: &Value, struct_heap: &[StructInstance]) -> Option<RangeValue> {
    match value {
        Value::Range(range) => Some(range.clone()),
        Value::Struct(instance) => struct_instance_as_range(instance),
        Value::StructRef(idx) => struct_heap.get(*idx).and_then(struct_instance_as_range),
        _ => None,
    }
}

fn range_index_slice(range: &RangeValue, inds: &RangeValue) -> Result<RangeValue, VmError> {
    let n = inds.length();
    let step = range.step * inds.step;
    let first = range.start + (inds.start - 1.0) * range.step;
    let stop = if n == 0 {
        if step < 0.0 {
            first + 1.0
        } else {
            first - 1.0
        }
    } else {
        let first_index = inds.start as i64;
        let first = range.get(first_index)?;
        first + ((n - 1) as f64) * step
    };
    Ok(RangeValue {
        start: first,
        step,
        stop,
        is_float: range.is_float,
        element_type: range.element_type,
        is_step_range: step != 1.0,
    })
}

fn string_chars_at_indices(s: &str, indices: &[i64]) -> Result<String, VmError> {
    let mut out = String::new();
    for &idx in indices {
        if idx < 1 {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![idx],
                shape: vec![s.len()],
            });
        }
        let start = (idx - 1) as usize;
        if start >= s.len() || !s.is_char_boundary(start) {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![idx],
                shape: vec![s.len()],
            });
        }
        let ch = s[start..]
            .chars()
            .next()
            .ok_or_else(|| VmError::IndexOutOfBounds {
                indices: vec![idx],
                shape: vec![s.len()],
            })?;
        out.push(ch);
    }
    Ok(out)
}

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_index_slice(&mut self, n: usize) -> Result<DispatchAction, VmError> {
        let mut indices = Vec::with_capacity(n);
        for _ in 0..n {
            indices.push(self.stack.pop_value()?);
        }
        indices.reverse();

        let target = self.stack.pop_value()?;

        if let Value::Str(s) = target {
            if n != 1 {
                // User-visible: Julia throws MethodError for multi-dim string indexing — catchable.
                self.raise(VmError::TypeError(
                    "ArgumentError: string slice requires exactly one range index".to_string(),
                ))?;
                return Ok(DispatchAction::Continue);
            }

            let mut is_index_array = false;
            let range_indices: Vec<i64> = match &indices[0] {
                // Route the legacy native-Array index carrier through the
                // file-local helper so the unwrap stays centralized while
                // #3908 retires the transitional native container.
                idx_val if is_native_array_value(idx_val) => {
                    is_index_array = true;
                    let arr_ref = match native_array_value_ref(idx_val) {
                        Some(arr_ref) => arr_ref,
                        None => {
                            return Err(VmError::InternalError(
                                "native_array_value_ref returned None after is_some()".to_string(),
                            ));
                        }
                    };
                    let arr = arr_ref.borrow();
                    let len = arr.element_count();
                    let mut idxs = Vec::with_capacity(len);
                    for i in 0..len {
                        let val = arr.get_linear(i)?;
                        if let Some(n) = integer_index_value(val) {
                            idxs.push(n);
                        }
                    }
                    idxs
                }
                idx_val @ (Value::Struct(_) | Value::StructRef(_)) => {
                    if let Some(arr) =
                        array_wrapper_value_to_array_value(idx_val, &self.struct_heap)?
                    {
                        is_index_array = true;
                        let len = arr.element_count();
                        let mut idxs = Vec::with_capacity(len);
                        for i in 0..len {
                            let val = arr.get_linear(i)?;
                            if let Some(n) = integer_index_value(val) {
                                idxs.push(n);
                            }
                        }
                        idxs
                    } else {
                        self.raise(VmError::TypeError(
                            "ArgumentError: string slicing requires a range index".to_string(),
                        ))?;
                        return Ok(DispatchAction::Continue);
                    }
                }
                Value::Range(range) => {
                    let start = range.start as i64;
                    let step = range.step as i64;
                    let stop = range.stop as i64;
                    if step == 0 {
                        // User-visible: Julia throws ArgumentError for zero-step range — catchable.
                        self.raise(VmError::TypeError(
                            "ArgumentError: range step cannot be zero".to_string(),
                        ))?;
                        return Ok(DispatchAction::Continue);
                    }
                    let cap = ((stop - start).unsigned_abs() / step.unsigned_abs() + 1) as usize;
                    let mut idxs = Vec::with_capacity(cap);
                    let mut i = start;
                    while (step > 0 && i <= stop) || (step < 0 && i >= stop) {
                        idxs.push(i);
                        i += step;
                    }
                    idxs
                }
                Value::SliceAll => (1..=s.len() as i64).collect(),
                _ => {
                    // User-visible: indexing string with unsupported type → Julia MethodError catchable.
                    self.raise(VmError::TypeError(
                        "ArgumentError: string slicing requires a range index".to_string(),
                    ))?;
                    return Ok(DispatchAction::Continue);
                }
            };

            if range_indices.is_empty() {
                self.stack.push(Value::Str(String::new()));
            } else if is_index_array {
                match string_chars_at_indices(&s, &range_indices) {
                    Ok(selected) => self.stack.push(Value::Str(selected)),
                    Err(err) => {
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                }
            } else {
                let start = (range_indices[0] - 1) as usize;
                let end = match range_indices.last() {
                    Some(&v) => v as usize,
                    None => {
                        // INTERNAL: unreachable — guarded by is_empty() check above (compiler invariant).
                        return Err(VmError::InternalError(
                            "range_indices is empty after non-empty check".to_string(),
                        ));
                    }
                };
                if start > s.len() || end > s.len() {
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![start as i64 + 1, end as i64],
                        shape: vec![s.len()],
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
                    // User-visible: Julia raises StringIndexError for invalid byte positions — catchable.
                    self.raise(VmError::TypeError(
                        "StringIndexError: byte index is not valid char boundary".to_string(),
                    ))?;
                    return Ok(DispatchAction::Continue);
                }
                let substring = s[start..end].to_string();
                self.stack.push(Value::Str(substring));
            }
            return Ok(DispatchAction::Continue);
        }

        // A range indexed by a VECTOR of indices / Bool mask materializes the
        // selected elements into a Vector (Issue #5754). The runtime dispatch
        // matcher does not route a native-array index to the pure-Julia
        // `getindex(::AbstractRange, ::AbstractVector)`, so handle it here. A
        // range INDEX (slice) — `indices[0]` is itself a range, not a native
        // array — takes the lazy range-slice path below, preserving
        // `(1:10)[2:4] === 2:4`.
        if indices.len() == 1 {
            if let (Some(range), Some(inds)) = (
                value_as_range(&target, &self.struct_heap),
                value_as_range(&indices[0], &self.struct_heap),
            ) {
                match range_index_slice(&range, &inds) {
                    Ok(result) => self.stack.push(Value::Range(result)),
                    Err(err) => self.raise(err)?,
                }
                return Ok(DispatchAction::Continue);
            }
        }

        if let Value::Range(range) = &target {
            if indices.len() == 1 {
                if let Some(idx_ref) = native_array_value_ref(&indices[0]) {
                    let selected = selected_indices_from_index_array(&idx_ref.borrow())?;
                    let materialized = array_value(range.collect());
                    let result =
                        load_selected_array_elements(materialized, &selected, &self.struct_heap)?;
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }
            }
        }

        // Route the legacy native-Array slicing target through the file-local
        // helper so the unwrap stays centralized while #3908 retires the
        // transitional native container. Clone the `ArrayRef` (cheap Rc
        // bump) to keep `target` available for the dispatch fallback arm.
        let arr = if let Some(arr_ref) = native_array_value_ref(&target) {
            arr_ref.clone()
        } else if let Some(arr_value) =
            array_wrapper_value_to_array_value(&target, &self.struct_heap)?
        {
            new_array_ref(arr_value)
        } else {
            let other = target;
            let mut args = Vec::with_capacity(indices.len() + 1);
            args.push(other);
            args.extend(indices);
            if let Some(func_index) =
                self.find_best_method_index(&["getindex", "Base.getindex"], &args)
            {
                self.start_function_call(func_index, args)?;
                return Ok(DispatchAction::Continue);
            }
            let type_name = self.get_type_name(&args[0]);
            // User-visible: indexing non-array with slice → Julia MethodError catchable.
            self.raise(VmError::TypeError(format!(
                "MethodError: no method matching getindex({}) with range index",
                type_name
            )))?;
            return Ok(DispatchAction::Continue);
        };

        let slice_indices: Vec<SliceIndex> = indices
            .iter()
            .map(|value| value_to_slice_index(value, &self.struct_heap))
            .collect();
        let all_scalars = slice_indices
            .iter()
            .all(|idx| matches!(idx, SliceIndex::Scalar(_)));

        if all_scalars {
            let i64_indices: Vec<i64> = slice_indices
                .iter()
                .map(|idx| match idx {
                    SliceIndex::Scalar(i) => *i,
                    _ => 0,
                })
                .collect();
            let arr_borrow = arr.borrow();
            let val = match self.try_or_handle(arr_borrow.get(&i64_indices))? {
                Some(val) => val,
                None => return Ok(DispatchAction::Continue),
            };
            self.stack.push(val);
            return Ok(DispatchAction::Continue);
        }

        let arr_borrow = arr.borrow();
        let shape = &arr_borrow.shape;

        if slice_indices.len() == 1 {
            let dim_size = if shape.is_empty() { 0 } else { shape[0] };
            let row_indices = resolve_dimension(&slice_indices[0], dim_size);
            let mut values: Vec<Value> = Vec::with_capacity(row_indices.len());
            for &idx in &row_indices {
                if let Ok(val) = arr_borrow.get(&[idx]) {
                    values.push(val);
                }
            }
            let result_arr = create_sliced_array(&arr_borrow, values, vec![row_indices.len()])?;
            // Emit the MemoryRef-backed `Array{T,N}` wrapper for the slice
            // result instead of the legacy native carrier (Issue #6807). `arr`
            // is a local owned `ArrayRef` (distinct from `self`), so the
            // outstanding `arr_borrow` does not conflict with the `&mut self`
            // wrapper construction.
            let wrapped = self.array_value_to_wrapper(result_arr)?;
            self.stack.push(wrapped);
            return Ok(DispatchAction::Continue);
        }

        if slice_indices.len() == 2 {
            let rows = if !shape.is_empty() { shape[0] } else { 0 };
            let cols = if shape.len() >= 2 { shape[1] } else { 0 };
            let row_indices = resolve_dimension(&slice_indices[0], rows);
            let col_indices = resolve_dimension(&slice_indices[1], cols);

            let mut values: Vec<Value> = Vec::with_capacity(row_indices.len() * col_indices.len());
            for &c in &col_indices {
                for &r in &row_indices {
                    if let Ok(val) = arr_borrow.get(&[r, c]) {
                        values.push(val);
                    }
                }
            }

            let row_is_scalar = matches!(slice_indices[0], SliceIndex::Scalar(_));
            let col_is_scalar = matches!(slice_indices[1], SliceIndex::Scalar(_));
            let result_shape = match (row_is_scalar, col_is_scalar) {
                (true, true) => vec![1],
                (true, false) => vec![col_indices.len()],
                (false, true) => vec![row_indices.len()],
                (false, false) => vec![row_indices.len(), col_indices.len()],
            };
            let result_arr = create_sliced_array(&arr_borrow, values, result_shape)?;
            let wrapped = self.array_value_to_wrapper(result_arr)?;
            self.stack.push(wrapped);
            return Ok(DispatchAction::Continue);
        }

        let n_dims = slice_indices.len();
        let mut dim_indices: Vec<Vec<i64>> = Vec::with_capacity(n_dims);
        for (i, idx) in slice_indices.iter().enumerate() {
            let dim_size = if i < shape.len() { shape[i] } else { 0 };
            dim_indices.push(resolve_dimension(idx, dim_size));
        }

        let total_elements: usize = dim_indices.iter().map(|d| d.len()).product();
        let mut values: Vec<Value> = Vec::with_capacity(total_elements);
        let mut current_indices: Vec<usize> = vec![0; n_dims];
        for _ in 0..total_elements {
            let actual_indices: Vec<i64> = current_indices
                .iter()
                .enumerate()
                .map(|(dim, &idx)| dim_indices[dim][idx])
                .collect();
            if let Ok(val) = arr_borrow.get(&actual_indices) {
                values.push(val);
            }
            for dim in 0..n_dims {
                current_indices[dim] += 1;
                if current_indices[dim] < dim_indices[dim].len() {
                    break;
                }
                current_indices[dim] = 0;
            }
        }

        let result_shape: Vec<usize> = slice_indices
            .iter()
            .enumerate()
            .filter_map(|(i, idx)| {
                if matches!(idx, SliceIndex::Scalar(_)) {
                    None
                } else {
                    Some(dim_indices[i].len())
                }
            })
            .collect();
        let final_shape = if result_shape.is_empty() {
            vec![1]
        } else {
            result_shape
        };
        let result_arr = create_sliced_array(&arr_borrow, values, final_shape)?;
        let wrapped = self.array_value_to_wrapper(result_arr)?;
        self.stack.push(wrapped);
        Ok(DispatchAction::Continue)
    }
}
