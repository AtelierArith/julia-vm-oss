//! IndexSlice handling for array/string slicing.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// SAFETY: i64→usize casts for slice start/end are from range_indices which are
// 1-based Julia indices verified to be non-empty before use.
#![allow(clippy::cast_sign_loss)]

use super::super::*;
use super::array_index::{create_sliced_array, ArrayIndexResult};
use crate::rng::RngLike;

// Convert Value indices to internal slice representation.
enum SliceIndex {
    Scalar(i64),
    Range(Vec<i64>),
    All,
}

fn value_to_slice_index(v: &Value) -> SliceIndex {
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
        Value::Array(arr_ref) => {
            let arr = arr_ref.borrow();

            match &arr.data {
                value::ArrayData::Bool(bool_vec) => {
                    let mut indices = Vec::new();
                    for (i, &b) in bool_vec.iter().enumerate() {
                        if b {
                            indices.push((i + 1) as i64);
                        }
                    }
                    return SliceIndex::Range(indices);
                }
                value::ArrayData::F64(f64_vec) => {
                    let is_boolean_like = f64_vec.iter().all(|&f| f == 0.0 || f == 1.0);
                    if is_boolean_like && !f64_vec.is_empty() {
                        let mut indices = Vec::new();
                        for (i, &f) in f64_vec.iter().enumerate() {
                            if f == 1.0 {
                                indices.push((i + 1) as i64);
                            }
                        }
                        return SliceIndex::Range(indices);
                    }
                }
                _ => {}
            }

            let len = arr.data.raw_len();
            let mut indices = Vec::with_capacity(len);
            for i in 0..len {
                if let Some(val) = arr.data.get_value(i) {
                    match val {
                        Value::F64(f) => indices.push(f as i64),
                        Value::I64(n) => indices.push(n),
                        Value::F32(f) => indices.push(f as i64),
                        Value::I32(n) => indices.push(n as i64),
                        Value::I16(n) => indices.push(n as i64),
                        Value::I8(n) => indices.push(n as i64),
                        _ => indices.push(0),
                    }
                }
            }
            SliceIndex::Range(indices)
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

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_index_slice(&mut self, n: usize) -> Result<ArrayIndexResult, VmError> {
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
                return Ok(ArrayIndexResult::Continue);
            }

            let range_indices: Vec<i64> = match &indices[0] {
                Value::Array(arr_ref) => {
                    let arr = arr_ref.borrow();
                    let len = arr.data.raw_len();
                    let mut idxs = Vec::with_capacity(len);
                    for i in 0..len {
                        if let Some(val) = arr.data.get_value(i) {
                            match val {
                                Value::I64(n) => idxs.push(n),
                                Value::F64(f) => idxs.push(f as i64),
                                _ => {}
                            }
                        }
                    }
                    idxs
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
                        return Ok(ArrayIndexResult::Continue);
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
                    return Ok(ArrayIndexResult::Continue);
                }
            };

            if range_indices.is_empty() {
                self.stack.push(Value::Str(String::new()));
            } else {
                let start = (range_indices[0] - 1) as usize;
                let end = match range_indices.last() {
                    Some(&v) => v as usize,
                    None => {
                        // INTERNAL: unreachable — guarded by is_empty() check above (compiler invariant).
                        return Err(VmError::InternalError(
                            "range_indices is empty after non-empty check".to_string(),
                        ))
                    }
                };
                if start > s.len() || end > s.len() {
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![start as i64 + 1, end as i64],
                        shape: vec![s.len()],
                    })?;
                    return Ok(ArrayIndexResult::Continue);
                }
                if !s.is_char_boundary(start) || !s.is_char_boundary(end) {
                    // User-visible: Julia raises StringIndexError for invalid byte positions — catchable.
                    self.raise(VmError::TypeError(
                        "StringIndexError: byte index is not valid char boundary".to_string(),
                    ))?;
                    return Ok(ArrayIndexResult::Continue);
                }
                let substring = s[start..end].to_string();
                self.stack.push(Value::Str(substring));
            }
            return Ok(ArrayIndexResult::Continue);
        }

        let arr = match target {
            Value::Array(arr) => arr,
            other => {
                // User-visible: indexing non-array with slice → Julia MethodError catchable.
                self.raise(VmError::TypeError(format!(
                    "MethodError: no getindex method for {:?} with range index",
                    other
                )))?;
                return Ok(ArrayIndexResult::Continue);
            }
        };

        let slice_indices: Vec<SliceIndex> = indices.iter().map(value_to_slice_index).collect();
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
                None => return Ok(ArrayIndexResult::Continue),
            };
            self.stack.push(val);
            return Ok(ArrayIndexResult::Handled);
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
            let result_arr = create_sliced_array(&arr_borrow, values, vec![row_indices.len()]);
            self.stack.push(Value::Array(new_array_ref(result_arr)));
            return Ok(ArrayIndexResult::Handled);
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
            let result_arr = create_sliced_array(&arr_borrow, values, result_shape);
            self.stack.push(Value::Array(new_array_ref(result_arr)));
            return Ok(ArrayIndexResult::Handled);
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
        let result_arr = create_sliced_array(&arr_borrow, values, final_shape);
        self.stack.push(Value::Array(new_array_ref(result_arr)));
        Ok(ArrayIndexResult::Handled)
    }
}
