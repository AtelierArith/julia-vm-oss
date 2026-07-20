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
use super::string_index::string_char_byte_span;
use super::DispatchAction;
use crate::rng::RngLike;
use crate::vm::value::{
    array_wrapper_shape_and_offset, array_wrapper_value_to_array_value,
    native_array_value_from_array as array_value, native_array_value_ref, new_array_ref,
    ArrayValue, RangeElementType, RangeValue, StructInstance, TupleValue,
};
use subset_julia_vm_bytecode::ArrayElementType;

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

fn tuple_numeric_index_value(v: &Value) -> Option<i64> {
    match v {
        Value::Bool(value) => Some(if *value { 1 } else { 0 }),
        Value::F64(value) => Some(*value as i64),
        Value::F32(value) => Some(*value as i64),
        Value::I64(value) => Some(*value),
        Value::I128(value) => i64::try_from(*value).ok(),
        Value::I32(value) => Some(*value as i64),
        Value::I16(value) => Some(*value as i64),
        Value::I8(value) => Some(*value as i64),
        Value::U128(value) => i64::try_from(*value).ok(),
        Value::U64(value) => i64::try_from(*value).ok(),
        Value::U32(value) => Some(*value as i64),
        Value::U16(value) => Some(*value as i64),
        Value::U8(value) => Some(*value as i64),
        _ => None,
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
    let unparameterized = name.split('{').next().unwrap_or(name);
    let base = unparameterized
        .rsplit('.')
        .next()
        .unwrap_or(unparameterized);
    base == "Array"
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

/// Best-effort shape of an array-like receiver `Value`, used only to fill
/// the `BoundsError` diagnostic in [`Vm::struct_backed_range_indices`]
/// (Issue #11010). Returns an empty shape for anything that is not a
/// recognized array carrier, which only degrades the printed shape, not the
/// exception class.
pub(super) fn receiver_array_shape(value: &Value, struct_heap: &[StructInstance]) -> Vec<usize> {
    if let Some(array_ref) = native_array_value_ref(value) {
        return array_ref.borrow().shape.clone();
    }
    if let Ok(Some(array)) = array_wrapper_value_to_array_value(value, struct_heap) {
        return array.shape;
    }
    Vec::new()
}

impl<R: RngLike> Vm<R> {
    /// Materialize a struct-backed `AbstractRange` through Julia's iterator
    /// protocol. This keeps dynamic array indexing representation-independent:
    /// VM-native ranges retain their existing path, while any Julia struct whose
    /// declared supertype is `AbstractRange` supplies indices through `iterate`.
    ///
    /// `receiver_shape` is the shape of the array being indexed, used only to
    /// populate a `BoundsError` when an element is an integer that is simply
    /// out of `i64` range (Issue #11010); pass `&[]` when unavailable, which
    /// only degrades the diagnostic's shape display, not its exception class.
    pub(super) fn struct_backed_range_indices(
        &mut self,
        index: &Value,
        receiver_shape: &[usize],
    ) -> Result<Option<Vec<i64>>, VmError> {
        if !matches!(index, Value::Struct(_) | Value::StructRef(_)) {
            return Ok(None);
        }
        let runtime_type = self.get_type_name(index);
        if !self.check_subtype(&runtime_type, "AbstractRange") {
            return Ok(None);
        }

        let collected = self.collect_iterator(index)?;
        let array = if let Some(array_ref) = native_array_value_ref(&collected) {
            array_ref.borrow().clone()
        } else if let Some(array) =
            array_wrapper_value_to_array_value(&collected, &self.struct_heap)?
        {
            array
        } else {
            return Err(VmError::TypeError(format!(
                "collect({runtime_type}) did not produce an Array"
            )));
        };

        let mut indices = Vec::with_capacity(array.element_count());
        for position in 0..array.element_count() {
            let value = array.get_linear(position)?;
            // Issue #11010: a `BigInt` element that is out of `i64` range is
            // still an *integer* index — upstream's `checkbounds` performs
            // the container membership check without ever attempting an
            // `Int` conversion, so this element raises `BoundsError`, not the
            // `TypeError` a genuinely non-integer element (e.g. a `Float64`
            // with a fractional part) gets below. Clamp toward the nearest
            // `i64` bound (mirrors `util::regexmatch_integer_index`), which
            // is guaranteed out of bounds for any real array shape, so the
            // raised `BoundsError` is correct even though the exact BigInt
            // value cannot be carried in `VmError::IndexOutOfBounds`.
            if let Value::BigInt(n) = &value {
                if n.to_i64().is_none() {
                    let clamped = if n.as_inner().sign() == num_bigint::Sign::Minus {
                        i64::MIN
                    } else {
                        i64::MAX
                    };
                    return Err(VmError::IndexOutOfBounds {
                        indices: vec![clamped],
                        shape: receiver_shape.to_vec(),
                    });
                }
            }
            let value_type = self.get_type_name(&value);
            let integer = integer_index_value(value).ok_or_else(|| {
                VmError::TypeError(format!("array indices must be integers, got {value_type}"))
            })?;
            indices.push(integer);
        }
        Ok(Some(indices))
    }

    fn value_to_slice_index(
        &mut self,
        v: &Value,
        receiver_shape: &[usize],
    ) -> Result<SliceIndex, VmError> {
        Ok(match v {
            Value::I64(i) => SliceIndex::Scalar(*i),
            Value::F64(f) => SliceIndex::Scalar(*f as i64),
            Value::SliceAll => SliceIndex::All,
            Value::Range(range) => {
                let start = range.start as i64;
                let step = range.step as i64;
                let stop = range.stop as i64;
                if step == 0 {
                    return Ok(SliceIndex::Range(vec![]));
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
                    None => return Ok(SliceIndex::Scalar(0)),
                };
                let arr = arr_ref.borrow();
                match arr.to_logical_value_vec() {
                    Ok(values) => logical_values_to_slice_index(&values),
                    Err(_) => SliceIndex::Range(Vec::new()),
                }
            }
            Value::StructRef(idx) => {
                if let Some(indices) = self.struct_backed_range_indices(v, receiver_shape)? {
                    SliceIndex::Range(indices)
                } else if let Some(range) = value_as_range(v, &self.struct_heap) {
                    match range_value_to_indices(&range) {
                        Ok(indices) => SliceIndex::Range(indices),
                        Err(_) => SliceIndex::Range(Vec::new()),
                    }
                } else if let Some(values) = self
                    .struct_heap
                    .get(*idx)
                    .and_then(array_wrapper_logical_values)
                {
                    logical_values_to_slice_index(&values)
                } else {
                    SliceIndex::Scalar(0)
                }
            }
            Value::Struct(instance) => {
                if let Some(indices) = self.struct_backed_range_indices(v, receiver_shape)? {
                    SliceIndex::Range(indices)
                } else if let Some(range) = value_as_range(v, &self.struct_heap) {
                    match range_value_to_indices(&range) {
                        Ok(indices) => SliceIndex::Range(indices),
                        Err(_) => SliceIndex::Range(Vec::new()),
                    }
                } else if let Some(values) = array_wrapper_logical_values(instance) {
                    logical_values_to_slice_index(&values)
                } else {
                    SliceIndex::Scalar(0)
                }
            }
            _ => SliceIndex::Scalar(0),
        })
    }
}

fn resolve_dimension(idx: &SliceIndex, dim_size: usize) -> Vec<i64> {
    match idx {
        SliceIndex::Scalar(i) => vec![*i],
        SliceIndex::Range(r) => r.clone(),
        SliceIndex::All => (1..=dim_size as i64).collect(),
    }
}

pub(super) fn integer_index_value(v: Value) -> Option<i64> {
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
        Value::BigInt(n) => n.to_i64(),
        _ => None,
    }
}

fn range_struct_name_base(name: &str) -> &str {
    let unqualified = name.rsplit('.').next().unwrap_or(name);
    unqualified.split('{').next().unwrap_or(unqualified)
}

fn range_struct_field_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Char(c) => Some(f64::from(u32::from(*c))),
        Value::I8(v) => Some(f64::from(*v)),
        Value::I16(v) => Some(f64::from(*v)),
        Value::I32(v) => Some(f64::from(*v)),
        Value::I64(v) => Some(*v as f64),
        Value::U8(v) => Some(f64::from(*v)),
        Value::U16(v) => Some(f64::from(*v)),
        Value::U32(v) => Some(f64::from(*v)),
        Value::U64(v) => Some(*v as f64),
        _ => None,
    }
}

fn struct_instance_as_range(instance: &StructInstance) -> Option<RangeValue> {
    match range_struct_name_base(&instance.struct_name) {
        "OneTo" => match instance.values.first()? {
            Value::I64(stop) => Some(RangeValue::unit_range(1.0, *stop as f64)),
            _ => None,
        },
        "UnitRange" => Some(RangeValue {
            start: range_struct_field_to_f64(instance.values.first()?)?,
            step: 1.0,
            stop: range_struct_field_to_f64(instance.values.get(1)?)?,
            is_float: false,
            element_type: value::RangeElementType::Default,
            step_type: value::RangeElementType::Default,
            is_step_range: false,
            linspace_len: None,
            step_defined: false,
            bigint: None,
        }),
        "StepRange" => Some(RangeValue {
            start: range_struct_field_to_f64(instance.values.first()?)?,
            step: range_struct_field_to_f64(instance.values.get(1)?)?,
            stop: range_struct_field_to_f64(instance.values.get(2)?)?,
            is_float: false,
            element_type: value::RangeElementType::Default,
            step_type: value::RangeElementType::Default,
            is_step_range: true,
            linspace_len: None,
            step_defined: false,
            bigint: None,
        }),
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

fn range_value_to_indices(range: &RangeValue) -> Result<Vec<i64>, VmError> {
    let len = range.length();
    if len <= 0 {
        return Ok(Vec::new());
    }
    let mut indices = Vec::with_capacity(len as usize);
    for position in 1..=len {
        indices.push(range_position_to_index(range, position)?);
    }
    Ok(indices)
}

fn range_position_to_index(range: &RangeValue, position: i64) -> Result<i64, VmError> {
    let value = range.get(position)?;
    if !value.is_finite()
        || value.fract() != 0.0
        || value < i64::MIN as f64
        || value > i64::MAX as f64
    {
        return Err(VmError::InexactError(format!("Int64({value})")));
    }
    Ok(value as i64)
}

fn tuple_indices_from_slice_value(
    index: &Value,
    len: usize,
    struct_heap: &[StructInstance],
) -> Result<Option<Vec<i64>>, VmError> {
    if matches!(index, Value::SliceAll) {
        return Ok(Some((1..=len as i64).collect()));
    }
    if let Some(indices) = tuple_indices_from_index_array(index, len, struct_heap)? {
        return Ok(Some(indices));
    }
    match value_as_range(index, struct_heap) {
        Some(range) => range_value_to_indices(&range).map(Some),
        None => Ok(None),
    }
}

fn tuple_indices_from_index_array(
    index: &Value,
    tuple_len: usize,
    struct_heap: &[StructInstance],
) -> Result<Option<Vec<i64>>, VmError> {
    if let Some(arr_ref) = native_array_value_ref(index) {
        return tuple_indices_from_array_value(&arr_ref.borrow(), tuple_len).map(Some);
    }
    if let Some(arr) = array_wrapper_value_to_array_value(index, struct_heap)? {
        return tuple_indices_from_array_value(&arr, tuple_len).map(Some);
    }
    Ok(None)
}

fn tuple_indices_from_array_value(
    index_array: &ArrayValue,
    tuple_len: usize,
) -> Result<Vec<i64>, VmError> {
    let len = index_array.element_count();
    if matches!(index_array.element_type(), ArrayElementType::Bool) {
        if len != tuple_len {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![len as i64],
                shape: vec![tuple_len],
            });
        }
        let mut selected = Vec::new();
        for i in 0..len {
            if matches!(index_array.get_linear(i)?, Value::Bool(true)) {
                selected.push((i + 1) as i64);
            }
        }
        return Ok(selected);
    }

    let mut indices = Vec::with_capacity(len);
    for i in 0..len {
        let value = index_array.get_linear(i)?;
        let Some(index) = tuple_numeric_index_value(&value) else {
            // Upstream: a non-integer index element is a dispatch miss ->
            // MethodError. Raised through the real variant since Issue #11146
            // (it used to be a TypeError with a "MethodError: " text prefix).
            return Err(VmError::MethodError(
                "no method matching getindex(Tuple, non-integer index array element)".to_string(),
            ));
        };
        indices.push(index);
    }
    Ok(indices)
}

fn tuple_slice_from_index(
    tuple: &TupleValue,
    index: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<TupleValue>, VmError> {
    let Some(indices) = tuple_indices_from_slice_value(index, tuple.elements.len(), struct_heap)?
    else {
        return Ok(None);
    };
    let mut values = Vec::with_capacity(indices.len());
    for idx in indices {
        if idx < 1 || idx > tuple.elements.len() as i64 {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![idx],
                shape: vec![tuple.elements.len()],
            });
        }
        values.push(tuple.elements[(idx - 1) as usize].clone());
    }
    Ok(Some(TupleValue::new(values)))
}

fn range_index_slice(range: &RangeValue, inds: &RangeValue) -> Result<RangeValue, VmError> {
    let n = inds.length();
    let step = range.step * inds.step;
    let (first, stop) = if n == 0 {
        let first = range.start + (inds.start - 1.0) * range.step;
        let stop = if step < 0.0 { first + 1.0 } else { first - 1.0 };
        (first, stop)
    } else {
        // Materialize the selected endpoints through `RangeValue::get` so a
        // float parent range contributes TwicePrecision-exact values
        // (Issue #9421), instead of re-accumulating `start + k*step`.
        let first_index = inds.start as i64;
        let first = range.get(first_index)?;
        (first, first + ((n - 1) as f64) * step)
    };
    Ok(RangeValue {
        start: first,
        step,
        stop,
        is_float: range.is_float,
        element_type: range.element_type,
        step_type: crate::vm::value::RangeElementType::Default,
        is_step_range: step != 1.0,
        linspace_len: None,
        step_defined: false,
        bigint: None,
    })
}

fn string_bytes_at_indices(bytes: &[u8], indices: &[i64]) -> Result<Vec<u8>, VmError> {
    let mut out = Vec::new();
    for &idx in indices {
        let (start, end) = string_char_byte_span(bytes, idx)?;
        out.extend_from_slice(&bytes[start..end]);
    }
    Ok(out)
}

/// Select a String range without first materializing every numeric index.
/// UnitRange endpoints can be validated directly; StepRange indices are
/// streamed until the first invalid code-unit start (Issue #11640).
fn string_bytes_at_range(bytes: &[u8], range: &RangeValue) -> Result<Vec<u8>, VmError> {
    if range.is_float
        || matches!(
            range.element_type,
            RangeElementType::Float16
                | RangeElementType::Float32
                | RangeElementType::Float64
                | RangeElementType::Char
        )
    {
        return Err(VmError::MethodError(
            "getindex(::String, ::AbstractRange{<:NonInteger})".to_string(),
        ));
    }
    let len = range.length();
    if len <= 0 {
        return Ok(Vec::new());
    }

    if !range.is_step_range && range.step == 1.0 {
        let first = range_position_to_index(range, 1)?;
        let last = range_position_to_index(range, len)?;
        let (start, _) = string_char_byte_span(bytes, first)?;
        let (_, end) = string_char_byte_span(bytes, last)?;
        return Ok(bytes[start..end].to_vec());
    }

    let mut out = Vec::new();
    for position in 1..=len {
        let index = range_position_to_index(range, position)?;
        let (start, end) = string_char_byte_span(bytes, index)?;
        out.extend_from_slice(&bytes[start..end]);
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

        let string_bytes = match &target {
            Value::Str(s) => Some(s.as_bytes()),
            Value::StrBytes(bytes) => Some(bytes.as_ref()),
            _ => None,
        };
        if let Some(bytes) = string_bytes {
            if n != 1 {
                // User-visible: Julia throws MethodError for multi-dim string indexing — catchable.
                self.raise(VmError::ArgumentError(
                    "string slice requires exactly one range index".to_string(),
                ))?;
                return Ok(DispatchAction::Continue);
            }

            if let Some(range) = value_as_range(&indices[0], &self.struct_heap) {
                match string_bytes_at_range(bytes, &range) {
                    Ok(selected) => self.stack.push(Value::str_from_bytes(selected)),
                    Err(VmError::StringIndexError {
                        index,
                        valid_indices,
                    }) => {
                        let err = self.string_index_error_with_string(
                            target.clone(),
                            index,
                            valid_indices,
                        );
                        self.raise(err)?;
                    }
                    Err(err) => self.raise(err)?,
                }
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
                        let value_type = self.get_type_name(&val);
                        let Some(n) = integer_index_value(val.clone()) else {
                            let err = if matches!(val, Value::Bool(_)) {
                                VmError::ArgumentError(
                                    "invalid index: Bool values are not integer indices"
                                        .to_string(),
                                )
                            } else {
                                VmError::MethodError(format!(
                                    "getindex(::String, ::Vector{{{value_type}}})"
                                ))
                            };
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        };
                        idxs.push(n);
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
                            let value_type = self.get_type_name(&val);
                            let Some(n) = integer_index_value(val.clone()) else {
                                let err = if matches!(val, Value::Bool(_)) {
                                    VmError::ArgumentError(
                                        "invalid index: Bool values are not integer indices"
                                            .to_string(),
                                    )
                                } else {
                                    VmError::MethodError(format!(
                                        "getindex(::String, ::Vector{{{value_type}}})"
                                    ))
                                };
                                self.raise(err)?;
                                return Ok(DispatchAction::Continue);
                            };
                            idxs.push(n);
                        }
                        idxs
                    } else if let Some(range) = value_as_range(idx_val, &self.struct_heap) {
                        match range_value_to_indices(&range) {
                            Ok(indices) => indices,
                            Err(err) => {
                                self.raise(err)?;
                                return Ok(DispatchAction::Continue);
                            }
                        }
                    } else {
                        self.raise(VmError::ArgumentError(
                            "string slicing requires a range index".to_string(),
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
                        self.raise(VmError::ArgumentError(
                            "range step cannot be zero".to_string(),
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
                Value::SliceAll => {
                    self.stack.push(target.clone());
                    return Ok(DispatchAction::Continue);
                }
                Value::Function(function) if function.name == ":" => {
                    self.stack.push(target.clone());
                    return Ok(DispatchAction::Continue);
                }
                Value::Symbol(symbol) if symbol.as_str() == ":" => {
                    self.stack.push(target.clone());
                    return Ok(DispatchAction::Continue);
                }
                _ => {
                    self.raise(VmError::MethodError(
                        "getindex(::String, unsupported index)".to_string(),
                    ))?;
                    return Ok(DispatchAction::Continue);
                }
            };

            let select_individual_chars = is_index_array
                || range_indices
                    .windows(2)
                    .any(|pair| pair[1].checked_sub(pair[0]) != Some(1));
            if range_indices.is_empty() {
                self.stack.push(Value::str_new(String::new()));
            } else if select_individual_chars {
                match string_bytes_at_indices(bytes, &range_indices) {
                    Ok(selected) => self.stack.push(Value::str_from_bytes(selected)),
                    Err(VmError::StringIndexError {
                        index,
                        valid_indices,
                    }) => {
                        let err = self.string_index_error_with_string(
                            target.clone(),
                            index,
                            valid_indices,
                        );
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(err) => {
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                }
            } else {
                let first_index = range_indices[0];
                let last_index = match range_indices.last() {
                    Some(&value) => value,
                    None => {
                        // INTERNAL: unreachable — guarded by is_empty() check above (compiler invariant).
                        return Err(VmError::InternalError(
                            "range_indices is empty after non-empty check".to_string(),
                        ));
                    }
                };
                // Validate the caller's one-based ENDPOINTS through the shared
                // byte-aware segmentation, then use the final character's
                // exclusive byte end for the Rust slice (Issues #11618/#11621/#11627).
                let (validated_start, _) = match string_char_byte_span(bytes, first_index) {
                    Ok(span) => span,
                    Err(VmError::StringIndexError {
                        index,
                        valid_indices,
                    }) => {
                        let err = self.string_index_error_with_string(
                            target.clone(),
                            index,
                            valid_indices,
                        );
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(err) => {
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                let (_, end) = match string_char_byte_span(bytes, last_index) {
                    Ok(span) => span,
                    Err(VmError::StringIndexError {
                        index,
                        valid_indices,
                    }) => {
                        let err = self.string_index_error_with_string(
                            target.clone(),
                            index,
                            valid_indices,
                        );
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(err) => {
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                };
                self.stack
                    .push(Value::str_from_bytes(bytes[validated_start..end].to_vec()));
            }
            return Ok(DispatchAction::Continue);
        }

        if n == 1 {
            let tuple_slice = match &target {
                Value::Tuple(tuple) => Some((false, tuple)),
                Value::SimpleVector(tuple) => Some((true, tuple)),
                _ => None,
            };
            if let Some((is_simple_vector, tuple)) = tuple_slice {
                match tuple_slice_from_index(tuple, &indices[0], &self.struct_heap) {
                    Ok(Some(slice)) => {
                        let value = if is_simple_vector {
                            Value::SimpleVector(slice)
                        } else {
                            Value::Tuple(slice)
                        };
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                    Ok(None) => {}
                    Err(err) => {
                        self.raise(err)?;
                        return Ok(DispatchAction::Continue);
                    }
                }
            }
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
            self.raise(VmError::MethodError(format!(
                "no method matching getindex({}) with range index",
                type_name
            )))?;
            return Ok(DispatchAction::Continue);
        };

        let receiver_shape = arr.borrow().shape.clone();
        let mut slice_indices = Vec::with_capacity(indices.len());
        for value in &indices {
            slice_indices.push(self.value_to_slice_index(value, &receiver_shape)?);
        }
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
