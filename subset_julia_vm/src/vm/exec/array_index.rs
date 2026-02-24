//! Array indexing instructions.
//!
//! Handles: IndexLoad, IndexLoadTyped, IndexSlice, IndexStore, IndexStoreTyped

#![deny(clippy::unwrap_used)]
// SAFETY: All i64→usize casts for array/tuple/string indexing are guarded by
// prior bounds checks (e.g. `idx < 1 || idx as usize > len`).
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::stack_ops::StackOps;
use super::util::extract_cartesian_index_indices;
use crate::rng::RngLike;

/// Result type for array index operations
pub(super) enum ArrayIndexResult {
    /// Instruction was not handled by this module
    NotHandled,
    /// Instruction was handled successfully
    Handled,
    /// Instruction handled, but need to continue (e.g., after raise)
    Continue,
}

/// Macro to reduce duplication for integer array type conversion in slicing.
/// Extracts the primary Value variant or falls back to converting from I64.
macro_rules! convert_int_array_data {
    ($values:expr, $variant:ident, $val_variant:ident, $ty:ty) => {{
        let data: Vec<$ty> = $values
            .iter()
            .map(|v| match v {
                Value::$val_variant(i) => *i,
                Value::I64(i) => *i as $ty,
                _ => 0 as $ty,
            })
            .collect();
        ArrayData::$variant(data)
    }};
}

/// Create a sliced array from collected values, preserving the source array's element type.
/// This ensures that Bool, String, Complex, Struct, and other types are correctly preserved
/// during slicing operations.
pub(super) fn create_sliced_array(
    source: &ArrayValue,
    values: Vec<Value>,
    shape: Vec<usize>,
) -> ArrayValue {
    use crate::vm::ArrayData;
    use crate::vm::ArrayElementType;

    // Check for complex arrays with interleaved storage
    if let Some(ref elem_type) = source.element_type_override {
        match elem_type {
            ArrayElementType::ComplexF64 => {
                // Complex array: values are Complex structs, need to convert back to interleaved F64
                let mut f64_data: Vec<f64> = Vec::with_capacity(values.len() * 2);
                for v in &values {
                    let (re, im) = match v {
                        Value::Struct(s) if s.struct_name.starts_with("Complex") => {
                            let re = match s.values.first() {
                                Some(Value::F64(x)) => *x,
                                Some(Value::I64(x)) => *x as f64,
                                _ => 0.0,
                            };
                            let im = match s.values.get(1) {
                                Some(Value::F64(x)) => *x,
                                Some(Value::I64(x)) => *x as f64,
                                _ => 0.0,
                            };
                            (re, im)
                        }
                        _ => (0.0, 0.0),
                    };
                    f64_data.push(re);
                    f64_data.push(im);
                }
                let mut result = ArrayValue::new(ArrayData::F64(f64_data), shape);
                result.element_type_override = Some(ArrayElementType::ComplexF64);
                // Preserve struct_type_id for correct Complex type_id lookup (Issue #1804)
                result.struct_type_id = source.struct_type_id;
                return result;
            }
            ArrayElementType::ComplexF32 => {
                // Complex F32 array: values are Complex structs, need to convert back to interleaved F32
                let mut f32_data: Vec<f32> = Vec::with_capacity(values.len() * 2);
                for v in &values {
                    let (re, im) = match v {
                        Value::Struct(s) if s.struct_name.starts_with("Complex") => {
                            let re = match s.values.first() {
                                Some(Value::F32(x)) => *x,
                                Some(Value::F64(x)) => *x as f32,
                                Some(Value::I64(x)) => *x as f32,
                                _ => 0.0,
                            };
                            let im = match s.values.get(1) {
                                Some(Value::F32(x)) => *x,
                                Some(Value::F64(x)) => *x as f32,
                                Some(Value::I64(x)) => *x as f32,
                                _ => 0.0,
                            };
                            (re, im)
                        }
                        _ => (0.0, 0.0),
                    };
                    f32_data.push(re);
                    f32_data.push(im);
                }
                let mut result = ArrayValue::new(ArrayData::F32(f32_data), shape);
                result.element_type_override = Some(ArrayElementType::ComplexF32);
                // Preserve struct_type_id for correct Complex type_id lookup (Issue #1804)
                result.struct_type_id = source.struct_type_id;
                return result;
            }
            _ => {} // Fall through to normal handling
        }
    }

    // Try to create a homogeneous array based on the source type
    let data = match &source.data {
        ArrayData::F64(_) => {
            let f64_data: Vec<f64> = values
                .iter()
                .map(|v| match v {
                    Value::F64(f) => *f,
                    Value::I64(i) => *i as f64,
                    _ => 0.0,
                })
                .collect();
            ArrayData::F64(f64_data)
        }
        ArrayData::I64(_) => {
            let i64_data: Vec<i64> = values
                .iter()
                .map(|v| match v {
                    Value::I64(i) => *i,
                    Value::F64(f) => *f as i64,
                    _ => 0,
                })
                .collect();
            ArrayData::I64(i64_data)
        }
        ArrayData::Bool(_) => {
            let bool_data: Vec<bool> = values
                .iter()
                .map(|v| match v {
                    Value::Bool(b) => *b,
                    _ => false,
                })
                .collect();
            ArrayData::Bool(bool_data)
        }
        ArrayData::String(_) => {
            let string_data: Vec<String> = values
                .iter()
                .map(|v| match v {
                    Value::Str(s) => s.clone(),
                    _ => String::new(),
                })
                .collect();
            ArrayData::String(string_data)
        }
        ArrayData::Char(_) => {
            let char_data: Vec<char> = values
                .iter()
                .map(|v| match v {
                    Value::Char(c) => *c,
                    _ => '\0',
                })
                .collect();
            ArrayData::Char(char_data)
        }
        ArrayData::F32(_) => {
            let f32_data: Vec<f32> = values
                .iter()
                .map(|v| match v {
                    Value::F32(f) => *f,
                    Value::F64(f) => *f as f32,
                    Value::I64(i) => *i as f32,
                    _ => 0.0,
                })
                .collect();
            ArrayData::F32(f32_data)
        }
        // Integer types: extract primary variant or convert from I64
        ArrayData::I32(_) => convert_int_array_data!(values, I32, I32, i32),
        ArrayData::I16(_) => convert_int_array_data!(values, I16, I16, i16),
        ArrayData::I8(_) => convert_int_array_data!(values, I8, I8, i8),
        ArrayData::U64(_) => convert_int_array_data!(values, U64, U64, u64),
        ArrayData::U32(_) => convert_int_array_data!(values, U32, U32, u32),
        ArrayData::U16(_) => convert_int_array_data!(values, U16, U16, u16),
        ArrayData::U8(_) => convert_int_array_data!(values, U8, U8, u8),
        ArrayData::StructRefs(_) => {
            // For struct refs, extract the indices from StructRef values
            let refs: Vec<usize> = values
                .iter()
                .map(|v| match v {
                    Value::StructRef(idx) => *idx,
                    _ => 0,
                })
                .collect();
            ArrayData::StructRefs(refs)
        }
        ArrayData::Any(_) => {
            // Any array: just use the values directly
            ArrayData::Any(values)
        }
    };

    let mut result = ArrayValue::new(data, shape);

    // Preserve struct_type_id for struct arrays
    if source.struct_type_id.is_some() {
        result.struct_type_id = source.struct_type_id;
    }

    // Preserve element_type_override for complex/tuple arrays
    if source.element_type_override.is_some() {
        result.element_type_override = source.element_type_override.clone();
    }

    result
}

impl<R: RngLike> Vm<R> {
    /// Execute array indexing instructions.
    ///
    /// Returns `ArrayIndexResult::NotHandled` if the instruction is not an array index operation.
    #[inline]
    pub(super) fn execute_array_index(
        &mut self,
        instr: &Instr,
    ) -> Result<ArrayIndexResult, VmError> {
        match instr {
            Instr::IndexLoadTyped(n) => {
                // Support CartesianIndex: A[CartesianIndex((i, j))] == A[i, j]
                let indices = if *n == 1 {
                    let val = self.stack.pop_value()?;
                    match val {
                        Value::I64(v) => vec![v],
                        Value::Struct(s) if s.struct_name == "CartesianIndex" => {
                            extract_cartesian_index_indices(&s)?
                        }
                        Value::StructRef(idx) => {
                            let s = self.struct_heap.get(idx).ok_or_else(|| {
                                VmError::TypeError("Invalid struct ref".to_string())
                            })?;
                            if s.struct_name == "CartesianIndex" {
                                extract_cartesian_index_indices(s)?
                            } else {
                                // INTERNAL: StructRef index in IndexLoadTyped is compiler-generated; invalid ref means heap corruption
                                return Err(VmError::InternalError(format!(
                                    "expected I64 or CartesianIndex, got {}",
                                    s.struct_name
                                )));
                            }
                        }
                        Value::Array(idx_arr_ref) => {
                            // Boolean/logical array indexing: arr[bool_array] (Issue #2694)
                            // Handle Array indices at runtime by extracting true-indices
                            // from boolean arrays, or using integer arrays directly.
                            let idx_arr = idx_arr_ref.borrow();
                            let selected_indices: Vec<i64> = match &idx_arr.data {
                                value::ArrayData::Bool(bool_vec) => bool_vec
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, &b)| b)
                                    .map(|(i, _)| (i + 1) as i64)
                                    .collect(),
                                value::ArrayData::F64(f64_vec) => {
                                    let is_boolean_like =
                                        f64_vec.iter().all(|&f| f == 0.0 || f == 1.0);
                                    if is_boolean_like {
                                        f64_vec
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, &f)| f == 1.0)
                                            .map(|(i, _)| (i + 1) as i64)
                                            .collect()
                                    } else {
                                        f64_vec.iter().map(|&f| f as i64).collect()
                                    }
                                }
                                value::ArrayData::I64(i64_vec) => i64_vec.clone(),
                                _ => {
                                    let len = idx_arr.data.raw_len();
                                    let mut indices = Vec::with_capacity(len);
                                    for i in 0..len {
                                        if let Some(Value::I64(n)) = idx_arr.data.get_value(i) {
                                            indices.push(n);
                                        }
                                    }
                                    indices
                                }
                            };
                            drop(idx_arr);

                            // Pop the target array and gather selected elements
                            let target = self.stack.pop_value()?;
                            match target {
                                Value::Array(arr_ref) => {
                                    let arr = arr_ref.borrow();
                                    let mut elements = Vec::with_capacity(selected_indices.len());
                                    for &idx in &selected_indices {
                                        let val = arr.get(&[idx]).map_err(|_| {
                                            VmError::IndexOutOfBounds {
                                                indices: vec![idx],
                                                shape: arr.shape.clone(),
                                            }
                                        })?;
                                        elements.push(val);
                                    }
                                    let result_shape = vec![selected_indices.len()];
                                    let result = create_sliced_array(&arr, elements, result_shape);
                                    drop(arr);
                                    self.stack.push(Value::Array(new_array_ref(result)));
                                    return Ok(ArrayIndexResult::Handled);
                                }
                                _ => {
                                    // INTERNAL: IndexLoadTyped target is compiler-typed as Array; wrong type is a compiler bug
                                    return Err(VmError::InternalError(
                                        "logical indexing requires an Array target".to_string(),
                                    ));
                                }
                            }
                        }
                        other => {
                            // INTERNAL: IndexLoadTyped index type is compiler-typed; non-CartesianIndex struct is a compiler bug
                            return Err(VmError::InternalError(format!(
                                "expected I64 or CartesianIndex, got {:?}",
                                util::value_type_name(&other)
                            )))
                        }
                    }
                } else {
                    let mut idx = Vec::with_capacity(*n);
                    for _ in 0..*n {
                        idx.push(self.stack.pop_i64()?);
                    }
                    idx.reverse();
                    idx
                };
                match self.stack.pop() {
                    Some(Value::Array(arr)) => {
                        let arr_borrow = arr.borrow();
                        // Use try_or_handle so out-of-bounds errors can be caught by try/catch
                        let val = match self.try_or_handle(arr_borrow.get(&indices))? {
                            Some(v) => v,
                            None => {
                                drop(arr_borrow);
                                return Ok(ArrayIndexResult::Continue);
                            }
                        };
                        self.stack.push(val);
                    }
                    _ => {
                        // INTERNAL: IndexLoadTyped requires an Array target; wrong type is a compiler bug
                        return Err(VmError::InternalError(
                            "IndexLoadTyped requires TypedArray".to_string(),
                        ))
                    }
                }
                Ok(ArrayIndexResult::Handled)
            }

            Instr::IndexStoreTyped(n) => {
                let val = self.stack.pop_value()?;
                let mut indices = Vec::with_capacity(*n);
                for _ in 0..*n {
                    indices.push(self.stack.pop_i64()?);
                }
                indices.reverse();
                match self.stack.pop() {
                    Some(Value::Array(arr)) => {
                        // Special handling for struct arrays: convert Value::Struct to StructRef
                        let mut arr_mut = arr.borrow_mut();
                        if let ArrayData::StructRefs(_) = &arr_mut.data {
                            match &val {
                                Value::Struct(s) => {
                                    let idx = self.struct_heap.len();
                                    self.struct_heap.push(s.clone());
                                    arr_mut.set(&indices, Value::StructRef(idx))?;
                                }
                                Value::StructRef(_) => {
                                    arr_mut.set(&indices, val)?;
                                }
                                _ => {
                                    // INTERNAL: IndexStoreTyped value type is compiler-typed; wrong value type is a compiler bug
                                    return Err(VmError::InternalError(format!(
                                        "Cannot store {:?} in struct array",
                                        val.value_type()
                                    )))
                                }
                            }
                        } else {
                            arr_mut.set(&indices, val)?;
                        }
                        drop(arr_mut);
                        self.stack.push(Value::Array(arr));
                    }
                    _ => {
                        // INTERNAL: IndexStoreTyped requires an Array target; wrong type is a compiler bug
                        return Err(VmError::InternalError(
                            "IndexStoreTyped requires TypedArray".to_string(),
                        ))
                    }
                }
                Ok(ArrayIndexResult::Handled)
            }

            Instr::IndexLoad(n) => {
                // Support CartesianIndex: A[CartesianIndex((i, j))] == A[i, j]
                // Also supports Dict indexing with non-integer keys (Issue #1814)
                let indices = if *n == 1 {
                    // When n==1, check if it's a CartesianIndex (which expands to multiple indices)
                    // or a non-integer key for Dict indexing
                    let val = self.stack.pop_value()?;
                    match val {
                        Value::I64(v) => vec![v],
                        Value::Struct(s) if s.struct_name == "CartesianIndex" => {
                            extract_cartesian_index_indices(&s)?
                        }
                        Value::StructRef(idx) => {
                            let s = self.struct_heap.get(idx).ok_or_else(|| {
                                VmError::TypeError("Invalid struct ref".to_string())
                            })?;
                            if s.struct_name == "CartesianIndex" {
                                extract_cartesian_index_indices(s)?
                            } else {
                                // User-visible: user can index an array with a non-CartesianIndex struct at runtime
                                return Err(VmError::TypeError(format!(
                                    "expected I64 or CartesianIndex, got {}",
                                    s.struct_name
                                )));
                            }
                        }
                        Value::Array(idx_arr_ref) => {
                            // Boolean/logical array indexing: arr[bool_array] (Issue #2694)
                            // When the compiler cannot determine the index is an Array at
                            // compile time, IndexLoad is emitted instead of IndexSlice.
                            // Handle Array indices at runtime by extracting true-indices
                            // from boolean arrays, or using integer arrays directly.
                            let idx_arr = idx_arr_ref.borrow();
                            let selected_indices: Vec<i64> = match &idx_arr.data {
                                value::ArrayData::Bool(bool_vec) => bool_vec
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, &b)| b)
                                    .map(|(i, _)| (i + 1) as i64)
                                    .collect(),
                                value::ArrayData::F64(f64_vec) => {
                                    // Broadcast comparison may produce F64 with 0.0/1.0
                                    let is_boolean_like =
                                        f64_vec.iter().all(|&f| f == 0.0 || f == 1.0);
                                    if is_boolean_like {
                                        f64_vec
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, &f)| f == 1.0)
                                            .map(|(i, _)| (i + 1) as i64)
                                            .collect()
                                    } else {
                                        // Regular integer-like index array
                                        f64_vec.iter().map(|&f| f as i64).collect()
                                    }
                                }
                                value::ArrayData::I64(i64_vec) => i64_vec.clone(),
                                _ => {
                                    let len = idx_arr.data.raw_len();
                                    let mut indices = Vec::with_capacity(len);
                                    for i in 0..len {
                                        if let Some(Value::I64(n)) = idx_arr.data.get_value(i) {
                                            indices.push(n);
                                        }
                                    }
                                    indices
                                }
                            };
                            drop(idx_arr);

                            // Pop the target array and gather selected elements
                            let target = self.stack.pop_value()?;
                            match target {
                                Value::Array(arr_ref) => {
                                    let arr = arr_ref.borrow();
                                    let mut elements = Vec::with_capacity(selected_indices.len());
                                    for &idx in &selected_indices {
                                        let val = arr.get(&[idx]).map_err(|_| {
                                            VmError::IndexOutOfBounds {
                                                indices: vec![idx],
                                                shape: arr.shape.clone(),
                                            }
                                        })?;
                                        elements.push(val);
                                    }
                                    let result_shape = vec![selected_indices.len()];
                                    let result = create_sliced_array(&arr, elements, result_shape);
                                    drop(arr);
                                    self.stack.push(Value::Array(new_array_ref(result)));
                                    return Ok(ArrayIndexResult::Handled);
                                }
                                _ => {
                                    // User-visible: user can apply boolean-array indexing to a non-array target
                                    return Err(VmError::TypeError(
                                        "logical indexing requires an Array target".to_string(),
                                    ));
                                }
                            }
                        }
                        other => {
                            // Non-integer key: check if target is a Dict (Issue #1814)
                            // When a Dict is passed to a function as Any-typed parameter,
                            // the compiler emits IndexLoad instead of CallBuiltin(DictGet).
                            // Handle Dict lookup at runtime.
                            let target = self.stack.pop_value()?;
                            if let Value::Dict(dict) = &target {
                                let key = DictKey::from_value(&other).map_err(|_| {
                                    VmError::TypeError(format!(
                                        "invalid Dict key type: {:?}",
                                        util::value_type_name(&other)
                                    ))
                                })?;
                                let result = dict
                                    .get(&key)
                                    .ok_or_else(|| VmError::DictKeyNotFound(format!("{}", key)))?;
                                self.stack.push(result.clone());
                                return Ok(ArrayIndexResult::Handled);
                            }
                            // StructRef Dict dispatch: when a Pure Julia Dict struct
                            // is indexed with non-integer keys, dispatch to getindex
                            // method via find_best_method_index. (Issue #2748)
                            if matches!(&target, Value::StructRef(idx) if {
                                self.struct_heap.get(*idx)
                                    .map(|s| s.struct_name == "Dict" || s.struct_name.starts_with("Dict{"))
                                    .unwrap_or(false)
                            }) {
                                let args = vec![target, other];
                                if let Some(func_index) = self.find_best_method_index(
                                    &["getindex", "Base.getindex"],
                                    &args,
                                ) {
                                    self.start_function_call(func_index, args)?;
                                    return Ok(ArrayIndexResult::Continue);
                                }
                                let type_name = self.get_type_name(&args[0]);
                                return Err(VmError::MethodError(format!(
                                    "no method matching getindex({})",
                                    type_name
                                )));
                            }
                            // User-visible: user can index a collection with an unsupported key type
                            return Err(VmError::TypeError(format!(
                                "expected I64 or CartesianIndex, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else {
                    let mut idx = Vec::with_capacity(*n);
                    for _ in 0..*n {
                        idx.push(self.stack.pop_i64()?);
                    }
                    idx.reverse();
                    idx
                };

                // Check if we're indexing a String (returns Char) or Array
                let target = self.stack.pop_value()?;
                match target {
                    Value::Str(s) => {
                        // String indexing: s[i] returns Char (1-indexed, by byte position like Julia)
                        // Julia uses byte indexing, so accessing a byte in the middle of a
                        // multi-byte UTF-8 character is an error.
                        if indices.len() != 1 {
                            // User-visible: user can attempt multi-dimensional indexing on a String
                            return Err(VmError::TypeError(
                                "String indexing requires exactly one index".to_string(),
                            ));
                        }
                        let idx = indices[0];
                        let byte_idx = (idx - 1) as usize; // Convert to 0-indexed byte position

                        // Check bounds
                        if idx < 1 || byte_idx >= s.len() {
                            self.raise(VmError::IndexOutOfBounds {
                                indices: vec![idx],
                                shape: vec![s.len()],
                            })?;
                            return Ok(ArrayIndexResult::Continue);
                        }

                        // Check if this is a valid character boundary (Julia-compliant)
                        if !s.is_char_boundary(byte_idx) {
                            // Find the nearest valid indices
                            let prev_valid = {
                                let mut i = byte_idx;
                                while i > 0 && !s.is_char_boundary(i) {
                                    i -= 1;
                                }
                                (i + 1) as i64 // Convert back to 1-indexed
                            };
                            let next_valid = {
                                let mut i = byte_idx + 1;
                                while i < s.len() && !s.is_char_boundary(i) {
                                    i += 1;
                                }
                                if i >= s.len() {
                                    -1
                                } else {
                                    (i + 1) as i64
                                } // Convert back to 1-indexed
                            };
                            self.raise(VmError::StringIndexError {
                                index: idx,
                                valid_indices: (prev_valid, next_valid),
                            })?;
                            return Ok(ArrayIndexResult::Continue);
                        }

                        // Get the character at this byte position
                        let ch = s[byte_idx..].chars().next().ok_or_else(|| {
                            VmError::TypeError(format!(
                                "StringIndexError: no character at byte index {}",
                                byte_idx
                            ))
                        })?;
                        self.stack.push(Value::Char(ch));
                    }
                    Value::Array(arr) => {
                        let arr_borrow = arr.borrow();
                        // Check if this is an interleaved complex array
                        let element_count = arr_borrow.element_count();
                        if arr_borrow.len() == element_count * 2 {
                            // Interleaved complex array - return Complex struct
                            let linear =
                                match self.try_or_handle(arr_borrow.linear_index(&indices))? {
                                    Some(idx) => idx,
                                    None => return Ok(ArrayIndexResult::Continue),
                                };
                            let re = arr_borrow.try_data_f64()?[linear * 2];
                            let im = arr_borrow.try_data_f64()?[linear * 2 + 1];
                            drop(arr_borrow);

                            // Create Complex struct with correct name from struct_defs
                            let complex_val =
                                self.create_complex(self.get_complex_type_id(), re, im);
                            self.stack.push(complex_val);
                        } else {
                            // Regular array
                            let val = match self.try_or_handle(arr_borrow.get(&indices))? {
                                Some(val) => val,
                                None => return Ok(ArrayIndexResult::Continue),
                            };
                            self.stack.push(val);
                        }
                    }
                    Value::Tuple(tuple) => {
                        // Tuple indexing: t[i] where i is 1-indexed
                        if indices.len() != 1 {
                            // User-visible: user can attempt multi-dimensional indexing on a Tuple
                            return Err(VmError::TypeError(
                                "Tuple indexing requires exactly one index".to_string(),
                            ));
                        }
                        let idx = indices[0];
                        if idx < 1 || idx > tuple.elements.len() as i64 {
                            self.raise(VmError::IndexOutOfBounds {
                                indices: vec![idx],
                                shape: vec![tuple.elements.len()],
                            })?;
                            return Ok(ArrayIndexResult::Continue);
                        }
                        let element = tuple.elements[(idx - 1) as usize].clone();
                        self.stack.push(element);
                    }
                    Value::NamedTuple(named) => {
                        // NamedTuple indexing: nt[i] where i is 1-indexed
                        if indices.len() != 1 {
                            // User-visible: user can attempt multi-dimensional indexing on a NamedTuple
                            return Err(VmError::TypeError(
                                "NamedTuple indexing requires exactly one index".to_string(),
                            ));
                        }
                        let idx = indices[0];
                        if idx < 1 || idx > named.values.len() as i64 {
                            self.raise(VmError::IndexOutOfBounds {
                                indices: vec![idx],
                                shape: vec![named.values.len()],
                            })?;
                            return Ok(ArrayIndexResult::Continue);
                        }
                        let element = named.values[(idx - 1) as usize].clone();
                        self.stack.push(element);
                    }
                    Value::Range(range) => {
                        // Range indexing: r[i] where i is 1-indexed
                        if indices.len() != 1 {
                            // User-visible: user can attempt multi-dimensional indexing on a Range
                            return Err(VmError::TypeError(
                                "Range indexing requires exactly one index".to_string(),
                            ));
                        }
                        let idx = indices[0];
                        // Calculate range length
                        let range_len = if range.step > 0.0 {
                            ((range.stop - range.start) / range.step).floor() as i64 + 1
                        } else if range.step < 0.0 {
                            ((range.start - range.stop) / (-range.step)).floor() as i64 + 1
                        } else {
                            // User-visible: user can create a Range with step=0 and then index it
                            return Err(VmError::TypeError(
                                "Range step cannot be zero".to_string(),
                            ));
                        };
                        if idx < 1 || idx > range_len {
                            self.raise(VmError::IndexOutOfBounds {
                                indices: vec![idx],
                                shape: vec![range_len as usize],
                            })?;
                            return Ok(ArrayIndexResult::Continue);
                        }
                        // Calculate element: start + (idx - 1) * step
                        let element = range.start + ((idx - 1) as f64) * range.step;
                        // Return as I64 if it's a unit range with integer values
                        if range.is_unit_range() && element.fract() == 0.0 {
                            self.stack.push(Value::I64(element as i64));
                        } else {
                            self.stack.push(Value::F64(element));
                        }
                    }
                    // Generator: delegate indexing to the underlying iterator
                    Value::Generator(g) => {
                        // Generator only supports single-index access
                        if indices.len() != 1 {
                            // User-visible: user can attempt multi-dimensional indexing on a Generator
                            return Err(VmError::TypeError(
                                "Generator indexing requires exactly one index".to_string(),
                            ));
                        }
                        let gen_idx = indices[0];
                        match g.iter.as_ref() {
                            Value::Array(arr) => {
                                // Borrow arr only in this block so the Ref guard is
                                // dropped before calling try_or_handle (&mut self).
                                let (item, shape) = {
                                    let arr = arr.borrow();
                                    let item = arr.data.get_value((gen_idx - 1) as usize);
                                    let shape = arr.shape.clone();
                                    (item, shape)
                                };
                                let v = match self.try_or_handle(item.ok_or_else(|| {
                                    VmError::IndexOutOfBounds {
                                        indices: vec![gen_idx],
                                        shape,
                                    }
                                }))? {
                                    Some(v) => v,
                                    None => return Ok(ArrayIndexResult::Continue),
                                };
                                self.stack.push(v);
                            }
                            Value::Range(r) => {
                                let len = r.length();
                                if gen_idx < 1 || gen_idx > len {
                                    self.raise(VmError::IndexOutOfBounds {
                                        indices: vec![gen_idx],
                                        shape: vec![len as usize],
                                    })?;
                                    return Ok(ArrayIndexResult::Continue);
                                }
                                let element = r.start + ((gen_idx - 1) as f64) * r.step;
                                if r.is_unit_range() && element.fract() == 0.0 {
                                    self.stack.push(Value::I64(element as i64));
                                } else {
                                    self.stack.push(Value::F64(element));
                                }
                            }
                            Value::Tuple(t) => {
                                if gen_idx < 1 || gen_idx as usize > t.elements.len() {
                                    self.raise(VmError::IndexOutOfBounds {
                                        indices: vec![gen_idx],
                                        shape: vec![t.elements.len()],
                                    })?;
                                    return Ok(ArrayIndexResult::Continue);
                                }
                                self.stack.push(t.elements[(gen_idx - 1) as usize].clone());
                            }
                            other => {
                                // INTERNAL: Generator underlying type is compiler-assigned; unsupported type is a compiler bug
                                return Err(VmError::InternalError(format!(
                                    "indexing not supported for Generator with underlying {:?}",
                                    other
                                )));
                            }
                        }
                    }
                    target @ Value::Struct(_) | target @ Value::StructRef(_) => {
                        let mut args = Vec::with_capacity(indices.len() + 1);
                        args.push(target);
                        for idx in indices {
                            args.push(Value::I64(idx));
                        }
                        if let Some(func_index) =
                            self.find_best_method_index(&["getindex", "Base.getindex"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(ArrayIndexResult::Continue);
                        }
                        let type_name = self.get_type_name(&args[0]);
                        return Err(VmError::MethodError(format!(
                            "no method matching getindex({})",
                            type_name
                        )));
                    }
                    Value::Dict(dict) => {
                        // Dict indexing with integer key (Issue #1814)
                        // When Dict has integer keys (e.g., Dict(1 => "one")),
                        // the IndexLoad path receives i64 indices.
                        if indices.len() != 1 {
                            // User-visible: user can attempt multi-key integer indexing on a Dict
                            return Err(VmError::TypeError(
                                "Dict indexing requires exactly one key".to_string(),
                            ));
                        }
                        let key = DictKey::I64(indices[0]);
                        let result = dict
                            .get(&key)
                            .ok_or_else(|| VmError::DictKeyNotFound(format!("{}", key)))?;
                        self.stack.push(result.clone());
                    }
                    Value::Ref(inner) => {
                        // Ref indexing: r[] or r[1] unwraps the contained value (Issue #2687)
                        // In Julia, getindex(r::Ref) = r.x (returns the wrapped scalar)
                        self.stack.push(*inner);
                    }
                    Value::Memory(mem) => {
                        // Memory indexing: m[i] where i is 1-indexed
                        if indices.len() != 1 {
                            // INTERNAL: Memory only supports single-index access; multi-index is a compiler bug
                            return Err(VmError::InternalError(
                                "Memory indexing requires exactly one index".to_string(),
                            ));
                        }
                        let idx = indices[0];
                        let mem_len = mem.borrow().len();
                        if idx < 1 || idx as usize > mem_len {
                            self.raise(VmError::IndexOutOfBounds {
                                indices: vec![idx],
                                shape: vec![mem_len],
                            })?;
                            return Ok(ArrayIndexResult::Continue);
                        }
                        let val = mem.borrow().get(idx as usize).map_err(|e| {
                            VmError::TypeError(format!("BoundsError: {}", e))
                        })?;
                        self.stack.push(val);
                    }
                    other => {
                        // User-visible: user can attempt to index an unsupported type
                        return Err(VmError::TypeError(format!(
                            "indexing not supported for {:?}",
                            other
                        )));
                    }
                }
                Ok(ArrayIndexResult::Handled)
            }

            Instr::IndexSlice(n) => self.execute_index_slice(*n),

            Instr::IndexStore(n) => {
                // Early Dict handling for non-integer keys (Issue #1814)
                // Stack: [collection, index, value] when n==1
                // When a Dict is passed as Any-typed parameter, the compiler emits
                // IndexStore instead of CallBuiltin(DictSet). Handle at runtime.
                if *n == 1 {
                    let stack_len = self.stack.len();
                    if stack_len >= 3 {
                        let is_dict_with_non_int_key = {
                            let idx = &self.stack[stack_len - 2];
                            let col = &self.stack[stack_len - 3];
                            matches!(col, Value::Dict(_))
                                && !matches!(idx, Value::I64(_) | Value::F64(_))
                        };
                        if is_dict_with_non_int_key {
                            let value = self.stack.pop_value()?;
                            let index = self.stack.pop_value()?;
                            let collection = self.stack.pop_value()?;
                            if let Value::Dict(mut dict) = collection {
                                let key = DictKey::from_value(&index)?;
                                dict.insert(key, value);
                                // Push modified Dict (compiler emits Pop + PushNothing after)
                                self.stack.push(Value::Dict(dict));
                                return Ok(ArrayIndexResult::Handled);
                            }
                        }
                        // StructRef Dict dispatch: when a Pure Julia Dict struct is
                        // indexed with non-integer keys, dispatch to setindex! method.
                        // The compiler emits IndexStore for Any-typed collections,
                        // but pop_i64 fails on non-integer keys. (Issue #2748)
                        let is_struct_dict = {
                            let col = &self.stack[stack_len - 3];
                            if let Value::StructRef(struct_idx) = col {
                                self.struct_heap
                                    .get(*struct_idx)
                                    .map(|s| {
                                        s.struct_name == "Dict"
                                            || s.struct_name.starts_with("Dict{")
                                    })
                                    .unwrap_or(false)
                            } else {
                                false
                            }
                        };
                        if is_struct_dict {
                            let value = self.stack.pop_value()?;
                            let key = self.stack.pop_value()?;
                            let target = self.stack.pop_value()?;
                            let args = vec![target, value, key];
                            if let Some(func_index) = self.find_best_method_index(
                                &["setindex!", "Base.setindex!"],
                                &args,
                            ) {
                                self.start_function_call(func_index, args)?;
                                return Ok(ArrayIndexResult::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                    }
                }

                // Check top of stack to determine the value type
                // Need to check both Value::Struct and Value::StructRef for Complex
                let is_complex_val = match self.stack.last() {
                    Some(Value::Struct(s)) => s.is_complex(),
                    Some(Value::StructRef(idx)) => self
                        .struct_heap
                        .get(*idx)
                        .map(|s| s.is_complex())
                        .unwrap_or(false),
                    _ => false,
                };
                let is_tuple_val = matches!(self.stack.last(), Some(Value::Tuple(_)));
                let is_string_val = matches!(self.stack.last(), Some(Value::Str(_)));
                let is_char_val = matches!(self.stack.last(), Some(Value::Char(_)));
                let mut indices = Vec::with_capacity(*n);

                if is_complex_val {
                    let (re, im) = self.pop_complex()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();
                    let arr_result = self.stack.pop_array();
                    let arr = match self.try_or_handle(arr_result)? {
                        Some(arr) => arr,
                        None => return Ok(ArrayIndexResult::Continue),
                    };
                    // Check if the array uses interleaved complex storage (Issue #2691)
                    let uses_interleaved = {
                        let arr_ref = arr.borrow();
                        matches!(
                            arr_ref.element_type_override,
                            Some(value::ArrayElementType::ComplexF64)
                                | Some(value::ArrayElementType::ComplexF32)
                        )
                    };
                    if uses_interleaved {
                        let mut arr_mut = arr.borrow_mut();
                        let set_result = arr_mut.set_complex(&indices, re, im);
                        if self.try_or_handle(set_result)?.is_none() {
                            return Ok(ArrayIndexResult::Continue);
                        }
                    } else {
                        // For non-interleaved arrays (Any, StructRefs), reconstruct
                        // the Complex struct and store via generic set (Issue #2691)
                        let complex_struct = value::StructInstance {
                            type_id: 0,
                            struct_name: "Complex{Float64}".to_string(),
                            values: vec![Value::F64(re), Value::F64(im)],
                        };
                        let struct_idx = self.struct_heap.len();
                        self.struct_heap.push(complex_struct);
                        let mut arr_mut = arr.borrow_mut();
                        let set_result = arr_mut.set(&indices, Value::StructRef(struct_idx));
                        if self.try_or_handle(set_result)?.is_none() {
                            return Ok(ArrayIndexResult::Continue);
                        }
                    }
                    self.stack.push(Value::Array(arr));
                } else if is_tuple_val {
                    // Handle Tuple value - store directly into array
                    let tuple_val = self.stack.pop_value()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();

                    let arr_val = self.stack.pop_value()?;
                    match arr_val {
                        Value::Array(arr) => {
                            {
                                let mut arr_borrow = arr.borrow_mut();
                                let set_result = arr_borrow.set(&indices, tuple_val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(ArrayIndexResult::Continue);
                                }
                            }
                            self.stack.push(Value::Array(arr));
                        }
                        Value::Dict(mut dict) => {
                            // Dict integer-key assignment with Tuple value (Issue #1814)
                            if indices.len() == 1 {
                                let key = DictKey::I64(indices[0]);
                                dict.insert(key, tuple_val);
                                self.stack.push(Value::Dict(dict));
                            } else {
                                // User-visible: user can attempt multi-key Dict indexing when storing a Tuple value
                                return Err(VmError::TypeError(
                                    "Dict indexing requires exactly one key".to_string(),
                                ));
                            }
                        }
                        target @ Value::Struct(_) | target @ Value::StructRef(_) => {
                            // Handle struct types (e.g., SubArray) by calling setindex!
                            let mut args = Vec::with_capacity(indices.len() + 2);
                            args.push(target);
                            args.push(tuple_val);
                            for idx in indices {
                                args.push(Value::I64(idx));
                            }
                            if let Some(func_index) =
                                self.find_best_method_index(&["setindex!", "Base.setindex!"], &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(ArrayIndexResult::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                        other => {
                            // User-visible: user can IndexStore a Tuple into an unsupported collection type
                            return Err(VmError::TypeError(format!(
                                "IndexStore: expected Array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else if is_string_val || is_char_val {
                    // Handle String or Char value - store directly into array
                    let val = self.stack.pop_value()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();

                    let arr_val = self.stack.pop_value()?;
                    match arr_val {
                        Value::Array(arr) => {
                            {
                                let mut arr_borrow = arr.borrow_mut();
                                let set_result = arr_borrow.set(&indices, val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(ArrayIndexResult::Continue);
                                }
                            }
                            self.stack.push(Value::Array(arr));
                        }
                        Value::Dict(mut dict) => {
                            // Dict integer-key assignment with String/Char value (Issue #1814)
                            if indices.len() == 1 {
                                let key = DictKey::I64(indices[0]);
                                dict.insert(key, val);
                                self.stack.push(Value::Dict(dict));
                            } else {
                                // User-visible: user can attempt multi-key Dict indexing when storing a String/Char value
                                return Err(VmError::TypeError(
                                    "Dict indexing requires exactly one key".to_string(),
                                ));
                            }
                        }
                        target @ Value::Struct(_) | target @ Value::StructRef(_) => {
                            // Handle struct types (e.g., SubArray) by calling setindex!
                            let mut args = Vec::with_capacity(indices.len() + 2);
                            args.push(target);
                            args.push(val);
                            for idx in indices {
                                args.push(Value::I64(idx));
                            }
                            if let Some(func_index) =
                                self.find_best_method_index(&["setindex!", "Base.setindex!"], &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(ArrayIndexResult::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                        Value::Memory(mem) => {
                            // Memory setindex!: m[i] = val
                            if indices.len() != 1 {
                                // INTERNAL: Memory only supports single-index access; multi-index is a compiler bug
                                return Err(VmError::InternalError(
                                    "Memory indexing requires exactly one index".to_string(),
                                ));
                            }
                            let idx = indices[0];
                            let mem_len = mem.borrow().len();
                            if idx < 1 || idx as usize > mem_len {
                                self.raise(VmError::IndexOutOfBounds {
                                    indices: vec![idx],
                                    shape: vec![mem_len],
                                })?;
                                return Ok(ArrayIndexResult::Continue);
                            }
                            mem.borrow_mut().set(idx as usize, val).map_err(|e| {
                                VmError::TypeError(format!("BoundsError: {}", e))
                            })?;
                            self.stack.push(Value::Memory(mem));
                        }
                        other => {
                            // User-visible: user can IndexStore a String/Char into an unsupported collection type
                            return Err(VmError::TypeError(format!(
                                "IndexStore: expected Array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else {
                    let val = self.pop_f64_or_i64()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();

                    // Handle Array or StructRef (e.g., SubArray)
                    let arr_val = self.stack.pop_value()?;
                    match arr_val {
                        Value::Array(arr) => {
                            {
                                let mut arr_borrow = arr.borrow_mut();
                                // Convert f64 to appropriate type based on array element type
                                let typed_val = match &arr_borrow.data {
                                    value::ArrayData::I8(_) => Value::I8(val as i8),
                                    value::ArrayData::I16(_) => Value::I16(val as i16),
                                    value::ArrayData::I32(_) => Value::I32(val as i32),
                                    value::ArrayData::I64(_) => Value::I64(val as i64),
                                    value::ArrayData::U8(_) => Value::U8(val as u8),
                                    value::ArrayData::U16(_) => Value::U16(val as u16),
                                    value::ArrayData::U32(_) => Value::U32(val as u32),
                                    value::ArrayData::U64(_) => Value::U64(val as u64),
                                    value::ArrayData::F32(_) => Value::F32(val as f32),
                                    value::ArrayData::F64(_) => Value::F64(val),
                                    value::ArrayData::Bool(_) => {
                                        Value::I64(if val != 0.0 { 1 } else { 0 })
                                    }
                                    _ => Value::F64(val),
                                };
                                let set_result = arr_borrow.set(&indices, typed_val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(ArrayIndexResult::Continue);
                                }
                            }
                            self.stack.push(Value::Array(arr));
                        }
                        Value::StructRef(struct_idx) => {
                            // Handle SubArray by inline implementation
                            // SubArray struct: { parent: Vector, offset: Int64, len: Int64 }
                            let struct_val = self.struct_heap.get(struct_idx).ok_or_else(|| {
                                VmError::TypeError("Invalid struct ref".to_string())
                            })?;

                            // Check if this is a SubArray
                            if struct_val.struct_name.starts_with("SubArray") {
                                // Extract parent array (field 0), offset (field 1), len (field 2)
                                let parent_arr = match struct_val.values.first() {
                                    Some(Value::Array(arr)) => arr.clone(),
                                    _ => {
                                        // INTERNAL: SubArray parent field is compiler-assigned; non-Array parent is a compiler bug
                                        return Err(VmError::InternalError(
                                            "SubArray parent must be an Array".to_string(),
                                        ))
                                    }
                                };
                                let offset = match struct_val.values.get(1) {
                                    Some(Value::I64(o)) => *o,
                                    _ => {
                                        // INTERNAL: SubArray offset field is compiler-assigned; non-Int64 offset is a compiler bug
                                        return Err(VmError::InternalError(
                                            "SubArray offset must be Int64".to_string(),
                                        ))
                                    }
                                };
                                let len = match struct_val.values.get(2) {
                                    Some(Value::I64(l)) => *l,
                                    _ => {
                                        // INTERNAL: SubArray len field is compiler-assigned; non-Int64 len is a compiler bug
                                        return Err(VmError::InternalError(
                                            "SubArray len must be Int64".to_string(),
                                        ))
                                    }
                                };

                                // Bounds check on view index
                                if indices.len() != 1 {
                                    // INTERNAL: SubArray supports only 1D indexing; multi-index is a compiler bug
                                    return Err(VmError::InternalError(
                                        "SubArray only supports 1D indexing".to_string(),
                                    ));
                                }
                                let view_idx = indices[0];
                                if view_idx < 1 || view_idx > len {
                                    self.raise(VmError::IndexOutOfBounds {
                                        indices: vec![view_idx],
                                        shape: vec![len as usize],
                                    })?;
                                    return Ok(ArrayIndexResult::Continue);
                                }

                                // Calculate parent index: offset + view_idx (1-indexed)
                                let parent_idx = offset + view_idx;

                                // Set value in parent array
                                {
                                    let mut arr_borrow = parent_arr.borrow_mut();
                                    let typed_val = match &arr_borrow.data {
                                        value::ArrayData::I64(_) => Value::I64(val as i64),
                                        value::ArrayData::F64(_) => Value::F64(val),
                                        value::ArrayData::Bool(_) => {
                                            Value::I64(if val != 0.0 { 1 } else { 0 })
                                        }
                                        _ => Value::F64(val),
                                    };
                                    let set_result = arr_borrow.set(&[parent_idx], typed_val);
                                    if self.try_or_handle(set_result)?.is_none() {
                                        return Ok(ArrayIndexResult::Continue);
                                    }
                                }

                                // Push SubArray back onto stack (collection returned after IndexStore)
                                self.stack.push(Value::StructRef(struct_idx));
                            } else {
                                // For other struct types, call setindex! method
                                let target = Value::StructRef(struct_idx);
                                let mut args = Vec::with_capacity(indices.len() + 2);
                                args.push(target.clone());
                                args.push(Value::F64(val));
                                for idx in indices {
                                    args.push(Value::I64(idx));
                                }
                                if let Some(func_index) = self
                                    .find_best_method_index(&["setindex!", "Base.setindex!"], &args)
                                {
                                    // Save the struct ref to push after function returns
                                    // For now, we call the function but note that the return value
                                    // will be the stored value, not the collection
                                    self.start_function_call(func_index, args)?;
                                    return Ok(ArrayIndexResult::Continue);
                                }
                                let type_name = self.get_type_name(&target);
                                return Err(VmError::MethodError(format!(
                                    "no method matching setindex!({})",
                                    type_name
                                )));
                            }
                        }
                        Value::Struct(ref s) => {
                            // Handle inline Struct (less common for SubArray, but possible)
                            if s.struct_name.starts_with("SubArray") {
                                // Extract parent array (field 0), offset (field 1), len (field 2)
                                let parent_arr = match s.values.first() {
                                    Some(Value::Array(arr)) => arr.clone(),
                                    _ => {
                                        // INTERNAL: SubArray parent field is compiler-assigned; non-Array parent is a compiler bug
                                        return Err(VmError::InternalError(
                                            "SubArray parent must be an Array".to_string(),
                                        ))
                                    }
                                };
                                let offset = match s.values.get(1) {
                                    Some(Value::I64(o)) => *o,
                                    _ => {
                                        // INTERNAL: SubArray offset field is compiler-assigned; non-Int64 offset is a compiler bug
                                        return Err(VmError::InternalError(
                                            "SubArray offset must be Int64".to_string(),
                                        ))
                                    }
                                };
                                let len = match s.values.get(2) {
                                    Some(Value::I64(l)) => *l,
                                    _ => {
                                        // INTERNAL: SubArray len field is compiler-assigned; non-Int64 len is a compiler bug
                                        return Err(VmError::InternalError(
                                            "SubArray len must be Int64".to_string(),
                                        ))
                                    }
                                };

                                // Bounds check on view index
                                if indices.len() != 1 {
                                    // INTERNAL: SubArray supports only 1D indexing; multi-index is a compiler bug
                                    return Err(VmError::InternalError(
                                        "SubArray only supports 1D indexing".to_string(),
                                    ));
                                }
                                let view_idx = indices[0];
                                if view_idx < 1 || view_idx > len {
                                    self.raise(VmError::IndexOutOfBounds {
                                        indices: vec![view_idx],
                                        shape: vec![len as usize],
                                    })?;
                                    return Ok(ArrayIndexResult::Continue);
                                }

                                // Calculate parent index: offset + view_idx (1-indexed)
                                let parent_idx = offset + view_idx;

                                // Set value in parent array
                                {
                                    let mut arr_borrow = parent_arr.borrow_mut();
                                    let typed_val = match &arr_borrow.data {
                                        value::ArrayData::I64(_) => Value::I64(val as i64),
                                        value::ArrayData::F64(_) => Value::F64(val),
                                        value::ArrayData::Bool(_) => {
                                            Value::I64(if val != 0.0 { 1 } else { 0 })
                                        }
                                        _ => Value::F64(val),
                                    };
                                    let set_result = arr_borrow.set(&[parent_idx], typed_val);
                                    if self.try_or_handle(set_result)?.is_none() {
                                        return Ok(ArrayIndexResult::Continue);
                                    }
                                }

                                // Push Struct back onto stack
                                self.stack.push(arr_val);
                            } else {
                                // For other struct types, call setindex! method
                                let mut args = Vec::with_capacity(indices.len() + 2);
                                args.push(arr_val.clone());
                                args.push(Value::F64(val));
                                for idx in indices {
                                    args.push(Value::I64(idx));
                                }
                                if let Some(func_index) = self
                                    .find_best_method_index(&["setindex!", "Base.setindex!"], &args)
                                {
                                    self.start_function_call(func_index, args)?;
                                    return Ok(ArrayIndexResult::Continue);
                                }
                                let type_name = self.get_type_name(&arr_val);
                                return Err(VmError::MethodError(format!(
                                    "no method matching setindex!({})",
                                    type_name
                                )));
                            }
                        }
                        Value::Dict(mut dict) => {
                            // Dict integer-key assignment (Issue #1814)
                            // When Dict has integer keys, IndexStore receives i64 indices.
                            if indices.len() != 1 {
                                // User-visible: user can attempt multi-key Dict indexing when storing a numeric value
                                return Err(VmError::TypeError(
                                    "Dict indexing requires exactly one key".to_string(),
                                ));
                            }
                            let key = DictKey::I64(indices[0]);
                            dict.insert(key, Value::F64(val));
                            self.stack.push(Value::Dict(dict));
                        }
                        Value::Memory(mem) => {
                            // Memory setindex!: m[i] = val (f64/i64 path)
                            if indices.len() != 1 {
                                // INTERNAL: Memory only supports single-index access; multi-index is a compiler bug
                                return Err(VmError::InternalError(
                                    "Memory indexing requires exactly one index".to_string(),
                                ));
                            }
                            let idx = indices[0];
                            let mem_len = mem.borrow().len();
                            if idx < 1 || idx as usize > mem_len {
                                self.raise(VmError::IndexOutOfBounds {
                                    indices: vec![idx],
                                    shape: vec![mem_len],
                                })?;
                                return Ok(ArrayIndexResult::Continue);
                            }
                            // Convert f64 val to appropriate type based on Memory element type
                            let typed_val = {
                                let mem_borrow = mem.borrow();
                                match mem_borrow.element_type() {
                                    value::ArrayElementType::I64 => Value::I64(val as i64),
                                    value::ArrayElementType::I32 => Value::I32(val as i32),
                                    value::ArrayElementType::I16 => Value::I16(val as i16),
                                    value::ArrayElementType::I8 => Value::I8(val as i8),
                                    value::ArrayElementType::U64 => Value::U64(val as u64),
                                    value::ArrayElementType::U32 => Value::U32(val as u32),
                                    value::ArrayElementType::U16 => Value::U16(val as u16),
                                    value::ArrayElementType::U8 => Value::U8(val as u8),
                                    value::ArrayElementType::F32 => Value::F32(val as f32),
                                    value::ArrayElementType::Bool => {
                                        Value::Bool(val != 0.0)
                                    }
                                    _ => Value::F64(val),
                                }
                            };
                            mem.borrow_mut().set(idx as usize, typed_val).map_err(|e| {
                                VmError::TypeError(format!("BoundsError: {}", e))
                            })?;
                            self.stack.push(Value::Memory(mem));
                        }
                        other => {
                            // User-visible: user can IndexStore a numeric value into an unsupported collection type
                            return Err(VmError::TypeError(format!(
                                "IndexStore: expected Array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                }
                Ok(ArrayIndexResult::Handled)
            }

            _ => Ok(ArrayIndexResult::NotHandled),
        }
    }
}
