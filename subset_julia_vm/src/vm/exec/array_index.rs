//! Array indexing instructions.
//!
//! Handles: IndexLoad, IndexLoadTyped, IndexSlice, IndexStore, IndexStoreTyped

#![deny(clippy::unwrap_used)]
// SAFETY: All i64→usize casts for array/tuple/string indexing are guarded by
// prior bounds checks (e.g. `idx < 1 || idx as usize > len`).
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::expect_used)]

use super::super::value::{
    array_wrapper_shape_and_offset, array_wrapper_value_to_array_value, is_scalar_carrier,
    native_array_ref_from_value, native_array_ref_value as array_value, native_array_value_ref,
    new_array_ref,
};
use super::super::*;
use super::stack_ops::StackOps;
use super::util::extract_cartesian_index_indices;
use super::DispatchAction;
use crate::rng::RngLike;

// Issue #4814: scalar `getindex` was rejected with `TypeError`. The
// `Number ∪ AbstractChar` carrier predicate now lives in the shared
// `vm/value/predicates.rs` module (Issue #4875), so the same
// boundary is reused by `Length` and any future scalar-aware
// builtin instead of being re-enumerated per call site.

/// Create a sliced array from collected values, preserving the source array's element type.
/// This ensures that Bool, String, Complex, Struct, and other types are correctly preserved
/// during slicing operations.
pub(super) fn create_sliced_array(
    source: &ArrayValue,
    values: Vec<Value>,
    shape: Vec<usize>,
) -> Result<ArrayValue, VmError> {
    ArrayValue::memory_first_slice_from_values(source, values, shape)
}

fn typed_array_from_datatype_values(
    element_type: &crate::types::JuliaType,
    values: Vec<Value>,
    struct_heap: &[StructInstance],
) -> Result<Value, VmError> {
    let array_element_type = super::array_basic::array_element_type_from_julia_type(element_type);
    let len = values.len();
    let mut array = ArrayValue::memory_first_with_capacity(array_element_type.clone(), len);
    for value in values {
        let value =
            match (&array_element_type, value) {
                (
                    ArrayElementType::ComplexF64 | ArrayElementType::ComplexF32,
                    Value::StructRef(idx),
                ) => Value::Struct(struct_heap.get(idx).cloned().ok_or_else(|| {
                    VmError::TypeError(format!("Invalid StructRef index {}", idx))
                })?),
                (_, value) => value,
            };
        array.push(value)?;
    }
    array.shape = vec![len];
    Ok(array_value(new_typed_array_ref(array)))
}

fn typed_array_from_datatype_indices(
    element_type: &crate::types::JuliaType,
    values: &[i64],
) -> Result<Value, VmError> {
    typed_array_from_datatype_values(
        element_type,
        values.iter().map(|value| Value::I64(*value)).collect(),
        &[],
    )
}

fn is_struct_ref_dict(value: &Value, struct_heap: &[StructInstance]) -> bool {
    matches!(value, Value::StructRef(idx) if {
        struct_heap
            .get(*idx)
            .map(|s| util::is_dict_type_name(&s.struct_name))
            .unwrap_or(false)
    })
}

pub(super) fn selected_indices_from_index_array(idx_arr: &ArrayValue) -> Result<Vec<i64>, VmError> {
    let len = idx_arr.element_count();
    match idx_arr.element_type() {
        ArrayElementType::Bool => {
            let mut selected = Vec::new();
            for i in 0..len {
                if matches!(idx_arr.get_linear(i)?, Value::Bool(true)) {
                    selected.push((i + 1) as i64);
                }
            }
            Ok(selected)
        }
        ArrayElementType::F64 => {
            let mut values = Vec::with_capacity(len);
            for i in 0..len {
                if let Value::F64(value) = idx_arr.get_linear(i)? {
                    values.push(value);
                }
            }
            if values.iter().all(|&f| f == 0.0 || f == 1.0) {
                Ok(values
                    .iter()
                    .enumerate()
                    .filter(|(_, &f)| f == 1.0)
                    .map(|(i, _)| (i + 1) as i64)
                    .collect())
            } else {
                Ok(values.iter().map(|&f| f as i64).collect())
            }
        }
        ArrayElementType::I64 => {
            let mut indices = Vec::with_capacity(len);
            for i in 0..len {
                if let Value::I64(n) = idx_arr.get_linear(i)? {
                    indices.push(n);
                }
            }
            Ok(indices)
        }
        _ => {
            let mut indices = Vec::with_capacity(len);
            for i in 0..len {
                if let Value::I64(n) = idx_arr.get_linear(i)? {
                    indices.push(n);
                }
            }
            Ok(indices)
        }
    }
}

fn boxed_struct_indexstore_target(value: &Value) -> bool {
    let accepts_boxed_struct = |element_type: ArrayElementType| {
        matches!(
            element_type,
            ArrayElementType::Any
                | ArrayElementType::UnionOf(_)
                | ArrayElementType::Abstract(_)
                | ArrayElementType::Struct
                | ArrayElementType::StructOf(_)
                | ArrayElementType::StructInlineOf(_, _)
        )
    };

    if let Some(arr) = native_array_value_ref(value) {
        return accepts_boxed_struct(arr.borrow().element_type());
    }

    match value {
        Value::Memory(mem) => accepts_boxed_struct(mem.borrow().element_type().clone()),
        _ => false,
    }
}

fn boxed_numeric_indexstore_target(value: &Value, struct_heap: &[StructInstance]) -> bool {
    let accepts_boxed_numeric = |element_type: ArrayElementType| {
        matches!(
            element_type,
            ArrayElementType::Any | ArrayElementType::UnionOf(_) | ArrayElementType::Abstract(_)
        )
    };

    if let Ok(arr) = native_array_ref_from_value(value.clone()) {
        return accepts_boxed_numeric(arr.borrow().element_type());
    }

    match value {
        Value::Memory(mem) => accepts_boxed_numeric(mem.borrow().element_type().clone()),
        Value::Struct(s) => &*s.struct_name == "Array" || s.struct_name.starts_with("Array{"),
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .is_some_and(|s| &*s.struct_name == "Array" || s.struct_name.starts_with("Array{")),
        _ => false,
    }
}

fn selected_indices_from_logical_values(values: &[Value]) -> Vec<i64> {
    if values.iter().all(|value| matches!(value, Value::Bool(_))) {
        return values
            .iter()
            .enumerate()
            .filter_map(|(i, value)| match value {
                Value::Bool(true) => Some((i + 1) as i64),
                _ => None,
            })
            .collect();
    }

    let is_boolean_like_f64 = !values.is_empty()
        && values
            .iter()
            .all(|value| matches!(value, Value::F64(f) if *f == 0.0 || *f == 1.0));
    if is_boolean_like_f64 {
        return values
            .iter()
            .enumerate()
            .filter_map(|(i, value)| match value {
                Value::F64(1.0) => Some((i + 1) as i64),
                _ => None,
            })
            .collect();
    }

    values
        .iter()
        .filter_map(|value| match value {
            Value::I64(n) => Some(*n),
            Value::F64(n) => Some(*n as i64),
            Value::F32(n) => Some(*n as i64),
            Value::I32(n) => Some(*n as i64),
            Value::I16(n) => Some(*n as i64),
            Value::I8(n) => Some(*n as i64),
            _ => None,
        })
        .collect()
}

fn is_array_wrapper_name(name: &str) -> bool {
    let short = name.rsplit('.').next().unwrap_or(name);
    let base = short.split('{').next().unwrap_or(short);
    matches!(base, "Array" | "Vector" | "Matrix")
}

/// Fast path for `IndexLoad` on a MemoryRef-backed `Array{T,N}` wrapper indexed
/// by integer indices: read the element directly from the wrapper's `Memory`
/// instead of dispatching `getindex` per index (Issue #6806, PR B).
///
/// Handles the two index modes that [`ArrayValue::linear_index`] accepts — a
/// single linear index into an array of any rank, or one index per dimension
/// (column-major) — and defers any other arity (e.g. trailing-singleton
/// `v[i, 1]`) to dispatch by returning `None`. Behaviour-identical to the native
/// `ArrayValue::get` path: the wrapper's `Memory` stores exactly what
/// `ArrayValue::get` would return (the materializer copies `memref.get(i)`
/// element-by-element) and the linear-index computation mirrors
/// `ArrayValue::linear_index`, so this only removes the per-index method
/// dispatch. Returns `None` (deferring to dispatch) for inline structs,
/// non-`Array` wrappers, and non-`MemoryRef` storage. Bounds are checked against
/// the logical shape, which for a view is shorter than the parent `Memory` tail.
fn memoryref_wrapper_element(
    value: &Value,
    indices: &[i64],
    struct_heap: &[value::StructInstance],
) -> Option<Result<Value, VmError>> {
    let Value::StructRef(idx) = value else {
        return None;
    };
    let instance = struct_heap.get(*idx)?;
    if !is_array_wrapper_name(&instance.struct_name) {
        return None;
    }
    let size = instance.values.get(1)?;
    let (shape, _offset) = array_wrapper_shape_and_offset(size)?;
    let Value::MemoryRef(memref) = instance.values.first()? else {
        return None;
    };

    // Mirror `ArrayValue::linear_index`: a single index is linear into the whole
    // array; `shape.len()` indices are column-major per-dimension. Other arities
    // defer to dispatch.
    let linear = if indices.len() == 1 {
        let i = indices[0];
        let total: usize = shape.iter().product();
        if i < 1 || i as usize > total {
            return Some(Err(VmError::IndexOutOfBounds {
                indices: indices.to_vec(),
                shape,
            }));
        }
        (i - 1) as usize
    } else if indices.len() == shape.len() {
        let mut linear = 0usize;
        let mut stride = 1usize;
        for (dim, &ix) in indices.iter().enumerate() {
            if ix < 1 || ix as usize > shape[dim] {
                return Some(Err(VmError::IndexOutOfBounds {
                    indices: indices.to_vec(),
                    shape,
                }));
            }
            linear += ((ix - 1) as usize) * stride;
            stride *= shape[dim];
        }
        linear
    } else {
        return None;
    };

    Some(memref.get(linear + 1))
}

/// Whether a value of the given runtime type can be stored verbatim or by a
/// numeric coercion into the given element storage such that `ArrayData`'s
/// `set_value` is provably equivalent to `setindex!`'s `convert(T, v)`
/// (Issue #6806). Two safe cases:
///   * the value's type exactly matches the element storage type — a verbatim
///     store, equal to `convert(T, v::T) == v`; and
///   * an integer value into a floating-point array — `set_value` performs the
///     same numeric float cast as `convert(Float, int)`.
///
/// Every other pairing (notably a float value into an integer array, which
/// `convert` rounds with an `InexactError` check but `set_value` rejects, and
/// integer narrowing) is excluded and falls through to `setindex!` dispatch.
/// `Bool`, `Char`, `Complex`, boxed `Int128`/`UInt128`, and aggregate values are
/// excluded too.
fn fast_store_value_matches(value: &Value, elem: &value::ArrayElementType) -> bool {
    use value::ArrayElementType as E;
    matches!(
        (value, elem),
        (Value::F64(_), E::F64)
            | (Value::F32(_), E::F32)
            | (Value::I64(_), E::I64)
            | (Value::I32(_), E::I32)
            | (Value::I16(_), E::I16)
            | (Value::I8(_), E::I8)
            | (Value::U64(_), E::U64)
            | (Value::U32(_), E::U32)
            | (Value::U16(_), E::U16)
            | (Value::U8(_), E::U8)
            | (
                Value::I64(_)
                    | Value::I32(_)
                    | Value::I16(_)
                    | Value::I8(_)
                    | Value::U64(_)
                    | Value::U32(_)
                    | Value::U16(_)
                    | Value::U8(_),
                E::F64 | E::F32,
            )
    )
}

/// Whether the `IndexStore` write fast path applies: `target` is a
/// MemoryRef-backed `Array{T}` wrapper and `value` can be stored into its
/// element type with `set_value` == `setindex!`'s `convert` (Issue #6806).
fn fast_store_applies(
    target: &Value,
    value: &Value,
    struct_heap: &[value::StructInstance],
) -> bool {
    let Value::StructRef(idx) = target else {
        return false;
    };
    let Some(instance) = struct_heap.get(*idx) else {
        return false;
    };
    if !is_array_wrapper_name(&instance.struct_name) {
        return false;
    }
    matches!(
        instance.values.first(),
        Some(Value::MemoryRef(m)) if fast_store_value_matches(value, &m.element_type())
    )
}

fn selected_indices_from_array_wrapper(
    instance: &value::StructInstance,
) -> Result<Option<Vec<i64>>, VmError> {
    if !is_array_wrapper_name(&instance.struct_name) {
        return Ok(None);
    }

    let Some(mem) = instance.values.first() else {
        return Ok(None);
    };
    let Some(size) = instance.values.get(1) else {
        return Ok(None);
    };
    let Some((shape, offset)) = array_wrapper_shape_and_offset(size) else {
        return Ok(None);
    };
    let len: usize = shape.iter().product();
    let mut values = Vec::with_capacity(len);

    match native_array_ref_from_value(mem.clone()) {
        Ok(array_ref) => {
            let array_borrow = array_ref.borrow();
            for linear in 0..len {
                values.push(array_borrow.get_linear(offset - 1 + linear)?);
            }
        }
        Err(Value::MemoryRef(memref)) => {
            for linear in 0..len {
                values.push(memref.get(linear + 1)?);
            }
        }
        Err(Value::Memory(mem_ref)) => {
            let mem_borrow = mem_ref.borrow();
            for linear in 0..len {
                values.push(mem_borrow.get(offset + linear)?);
            }
        }
        Err(_) => return Ok(None),
    }

    Ok(Some(selected_indices_from_logical_values(&values)))
}

pub(super) fn load_selected_array_elements(
    target: Value,
    selected_indices: &[i64],
    struct_heap: &[StructInstance],
) -> Result<Value, VmError> {
    match native_array_ref_from_value(target) {
        Ok(arr_ref) => {
            let arr = arr_ref.borrow();
            let mut elements = Vec::with_capacity(selected_indices.len());
            for &idx in selected_indices {
                // Issue #3908: route the per-index read through ArrayValue's
                // logical helper so reshaped/Complex/struct-ref/tuple-element
                // arrays follow the same shared-backing semantics as the rest
                // of the Memory-first migration.
                if idx < 1 || (idx as usize) > arr.element_count() {
                    return Err(VmError::IndexOutOfBounds {
                        indices: vec![idx],
                        shape: arr.shape.clone(),
                    });
                }
                let val =
                    arr.get_linear((idx - 1) as usize)
                        .map_err(|_| VmError::IndexOutOfBounds {
                            indices: vec![idx],
                            shape: arr.shape.clone(),
                        })?;
                elements.push(val);
            }
            let result_shape = vec![selected_indices.len()];
            let result = create_sliced_array(&arr, elements, result_shape)?;
            Ok(array_value(new_array_ref(result)))
        }
        Err(other) => {
            if let Some(arr_value) = array_wrapper_value_to_array_value(&other, struct_heap)? {
                let arr_ref = new_array_ref(arr_value);
                let arr = arr_ref.borrow();
                let mut elements = Vec::with_capacity(selected_indices.len());
                for &idx in selected_indices {
                    if idx < 1 || (idx as usize) > arr.element_count() {
                        return Err(VmError::IndexOutOfBounds {
                            indices: vec![idx],
                            shape: arr.shape.clone(),
                        });
                    }
                    let val = arr.get_linear((idx - 1) as usize).map_err(|_| {
                        VmError::IndexOutOfBounds {
                            indices: vec![idx],
                            shape: arr.shape.clone(),
                        }
                    })?;
                    elements.push(val);
                }
                let result_shape = vec![selected_indices.len()];
                let result = create_sliced_array(&arr, elements, result_shape)?;
                return Ok(array_value(new_array_ref(result)));
            }

            Err(VmError::TypeError(
                "logical indexing requires an Array target".to_string(),
            ))
        }
    }
}

/// Wrap a logical [`ArrayRef`] as a stack-pushable [`Value`].
///
/// Extract the parent [`ArrayRef`] from a `SubArray` struct's field slice.
///
/// Issue #3908: The IndexStore SubArray paths (`StructRef` and inline `Struct`)
/// share the "parent must be a legacy native array" check. Delegating to
/// [`native_array_ref_from_value`] keeps the legacy-array destructure centralized in
/// a single helper so the audit allowlist (see
/// `scripts/check_value_array_allowlist.sh`) keeps shrinking as the runtime
/// moves toward Memory-first storage.
#[inline]
fn sub_array_parent_array_ref(values: &[Value]) -> Result<ArrayRef, VmError> {
    let first = values
        .first()
        .ok_or_else(|| VmError::InternalError("SubArray parent must be an Array".to_string()))?;
    native_array_ref_from_value(first.clone())
        .map_err(|_| VmError::InternalError("SubArray parent must be an Array".to_string()))
}

fn sub_array_offset_len(values: &[Value]) -> Result<(i64, i64), VmError> {
    let (offset_idx, len_idx) = if values.len() >= 4 { (2, 3) } else { (1, 2) };
    let offset = match values.get(offset_idx) {
        Some(Value::I64(o)) => *o,
        _ => {
            return Err(VmError::InternalError(
                "SubArray offset must be Int64".to_string(),
            ))
        }
    };
    let len = match values.get(len_idx) {
        Some(Value::I64(l)) => *l,
        _ => {
            return Err(VmError::InternalError(
                "SubArray len must be Int64".to_string(),
            ))
        }
    };
    Ok((offset, len))
}

fn scalar_indexstore_value_for_element_type(
    val: f64,
    element_type: value::ArrayElementType,
) -> Value {
    match element_type {
        value::ArrayElementType::I8 => Value::I8(val as i8),
        value::ArrayElementType::I16 => Value::I16(val as i16),
        value::ArrayElementType::I32 => Value::I32(val as i32),
        value::ArrayElementType::I64 => Value::I64(val as i64),
        value::ArrayElementType::U8 => Value::U8(val as u8),
        value::ArrayElementType::U16 => Value::U16(val as u16),
        value::ArrayElementType::U32 => Value::U32(val as u32),
        value::ArrayElementType::U64 => Value::U64(val as u64),
        value::ArrayElementType::F32 => Value::F32(val as f32),
        value::ArrayElementType::Bool => Value::Bool(val != 0.0),
        value::ArrayElementType::ComplexF64 | value::ArrayElementType::ComplexF32 => {
            Value::Struct(value::StructInstance::complex(0, val, 0.0))
        }
        _ => Value::F64(val),
    }
}

/// Outcome of resolving a single integer index against a Generator's
/// underlying iterator. Returned by [`generator_iter_index`] so the caller
/// can route each case through the right VM mechanism without re-borrowing
/// the iterator.
#[derive(Debug)]
enum GeneratorIndexOutcome {
    /// Element read from an Array-backed iterator. The inner `Result` may be a
    /// catchable [`VmError::IndexOutOfBounds`], so the caller funnels it
    /// through `try_or_handle` (preserving `try`/`catch` semantics).
    ArrayElement(Result<Value, VmError>),
    /// A catchable out-of-bounds error for Range/Tuple iterators; the caller
    /// must hand it to `raise`.
    RaiseOutOfBounds(VmError),
    /// A fully-resolved value to push directly onto the stack.
    Push(Value),
}

/// Resolve `gen_idx` (1-based) against a Generator's underlying `iter`,
/// borrowing it instead of deep-cloning.
///
/// Issue #5088: the previous implementation did `(*g.iter).clone()` on every
/// access, which deep-cloned the entire iterator state (e.g. a Tuple's whole
/// element `Vec`) just to read a single element. This helper inspects `iter`
/// by reference and only performs the minimal clone required: a cheap `Rc`
/// bump on the inner `ArrayRef` for arrays, scalar field copies for ranges,
/// and a single-element clone for tuples.
fn generator_iter_index(iter: &Value, gen_idx: i64) -> Result<GeneratorIndexOutcome, VmError> {
    // Native-array carrier path (Issue #6806): destructure through the shared
    // `native_array_value_ref` helper rather than matching the carrier variant
    // directly, so the carrier stays centralized as the migration proceeds.
    if let Some(arr_ref) = native_array_value_ref(iter) {
        // Clone only the `Rc<RefCell<…>>` handle (refcount bump), never the
        // backing storage. Borrow it in a tight scope so the `Ref` guard is
        // released before the caller takes `&mut self`.
        let arr = arr_ref.clone();
        let item = {
            let arr = arr.borrow();
            if gen_idx < 1 {
                Err(VmError::IndexOutOfBounds {
                    indices: vec![gen_idx],
                    shape: arr.shape.clone(),
                })
            } else {
                arr.get_linear((gen_idx - 1) as usize)
            }
        };
        return Ok(GeneratorIndexOutcome::ArrayElement(item));
    }
    match iter {
        Value::Range(r) => {
            let len = r.length();
            if gen_idx < 1 || gen_idx > len {
                return Ok(GeneratorIndexOutcome::RaiseOutOfBounds(
                    VmError::IndexOutOfBounds {
                        indices: vec![gen_idx],
                        shape: vec![len as usize],
                    },
                ));
            }
            let element = r.start + ((gen_idx - 1) as f64) * r.step;
            let value = if r.is_unit_range() && element.fract() == 0.0 {
                Value::I64(element as i64)
            } else {
                Value::F64(element)
            };
            Ok(GeneratorIndexOutcome::Push(value))
        }
        Value::Tuple(t) => {
            if gen_idx < 1 || gen_idx as usize > t.elements.len() {
                return Ok(GeneratorIndexOutcome::RaiseOutOfBounds(
                    VmError::IndexOutOfBounds {
                        indices: vec![gen_idx],
                        shape: vec![t.elements.len()],
                    },
                ));
            }
            Ok(GeneratorIndexOutcome::Push(
                t.elements[(gen_idx - 1) as usize].clone(),
            ))
        }
        Value::Struct(instance) => {
            if let Some(outcome) = generator_array_wrapper_index(instance, gen_idx, &[])? {
                return Ok(outcome);
            }
            Err(unsupported_generator_index_iter(iter))
        }
        Value::StructRef(_) => Err(unsupported_generator_index_iter(iter)),
        other => {
            // INTERNAL: Generator underlying type is compiler-assigned; unsupported type is a compiler bug
            Err(unsupported_generator_index_iter(other))
        }
    }
}

fn generator_iter_index_with_heap(
    iter: &Value,
    gen_idx: i64,
    struct_heap: &[StructInstance],
) -> Result<GeneratorIndexOutcome, VmError> {
    if let Value::StructRef(idx) = iter {
        let instance = struct_heap
            .get(*idx)
            .ok_or_else(|| VmError::TypeError(format!("Invalid StructRef index {}", idx)))?;
        if let Some(outcome) = generator_array_wrapper_index(instance, gen_idx, struct_heap)? {
            return Ok(outcome);
        }
    }

    match iter {
        Value::Struct(instance) => {
            if let Some(outcome) = generator_array_wrapper_index(instance, gen_idx, struct_heap)? {
                return Ok(outcome);
            }
            generator_iter_index(iter, gen_idx)
        }
        _ => generator_iter_index(iter, gen_idx),
    }
}

fn generator_array_wrapper_index(
    instance: &value::StructInstance,
    gen_idx: i64,
    struct_heap: &[StructInstance],
) -> Result<Option<GeneratorIndexOutcome>, VmError> {
    if !is_array_wrapper_name(&instance.struct_name) {
        return Ok(None);
    }

    let Some(storage) = instance.values.first() else {
        return Ok(None);
    };
    let Some(size) = instance.values.get(1) else {
        return Ok(None);
    };
    let Some((shape, offset)) = array_wrapper_shape_and_offset(size) else {
        return Ok(None);
    };
    let len: usize = shape.iter().product();
    let linear = if gen_idx < 1 || gen_idx as usize > len {
        None
    } else {
        Some((gen_idx - 1) as usize)
    };

    let item = match native_array_ref_from_value(storage.clone()) {
        Ok(array_ref) => {
            let array = array_ref.borrow();
            match linear {
                Some(linear) => array.get_linear(offset - 1 + linear).and_then(|value| {
                    generator_materialize_array_wrapper_element(
                        &array.element_type(),
                        value,
                        struct_heap,
                    )
                }),
                None => Err(VmError::IndexOutOfBounds {
                    indices: vec![gen_idx],
                    shape,
                }),
            }
        }
        Err(Value::MemoryRef(memref)) => {
            let element_type = memref.element_type();
            match linear {
                Some(linear) => memref.get(linear + 1).and_then(|value| {
                    generator_materialize_array_wrapper_element(&element_type, value, struct_heap)
                }),
                None => Err(VmError::IndexOutOfBounds {
                    indices: vec![gen_idx],
                    shape,
                }),
            }
        }
        Err(Value::Memory(mem_ref)) => {
            let mem = mem_ref.borrow();
            let element_type = mem.element_type().clone();
            match linear {
                Some(linear) => mem.get(offset + linear).and_then(|value| {
                    generator_materialize_array_wrapper_element(&element_type, value, struct_heap)
                }),
                None => Err(VmError::IndexOutOfBounds {
                    indices: vec![gen_idx],
                    shape,
                }),
            }
        }
        Err(_) => return Ok(None),
    };

    Ok(Some(GeneratorIndexOutcome::ArrayElement(item)))
}

fn generator_materialize_array_wrapper_element(
    element_type: &ArrayElementType,
    value: Value,
    struct_heap: &[StructInstance],
) -> Result<Value, VmError> {
    if !element_type.is_complex() {
        return Ok(value);
    }

    let Value::StructRef(idx) = value else {
        return Ok(value);
    };
    let instance = struct_heap
        .get(idx)
        .ok_or_else(|| VmError::TypeError(format!("Invalid StructRef index {}", idx)))?;
    Ok(Value::Struct(instance.clone()))
}

fn unsupported_generator_index_iter(iter: &Value) -> VmError {
    VmError::InternalError(format!(
        "indexing not supported for Generator with underlying {:?}",
        iter
    ))
}

#[cfg(test)]
mod tests {
    use crate::vm::value::{
        native_array_ref_value as array_value, new_array_ref, new_memory_ref, ArrayElementType,
        ArrayValue, MemoryRefValue, MemoryValue, RangeValue, StructInstance, TupleValue,
    };

    use super::*;

    #[test]
    fn generator_iter_index_array_reads_element_via_rc_bump() -> Result<(), VmError> {
        // The Array carrier must be resolved by borrowing `iter` (Rc bump on
        // the inner `ArrayRef`), never by deep-cloning the whole iter value.
        let arr_ref = new_array_ref(ArrayValue::memory_first_from_i64(vec![10, 20, 30], vec![3]));
        let iter = array_value(arr_ref);

        match generator_iter_index(&iter, 2)? {
            GeneratorIndexOutcome::ArrayElement(result) => {
                assert!(matches!(result?, Value::I64(20)));
            }
            other => panic!("expected ArrayElement, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn generator_iter_index_array_out_of_bounds_is_catchable() -> Result<(), VmError> {
        let arr_ref = new_array_ref(ArrayValue::memory_first_from_i64(vec![1, 2], vec![2]));
        let iter = array_value(arr_ref);

        // gen_idx < 1 and gen_idx > len both surface as a catchable Array read.
        match generator_iter_index(&iter, 0)? {
            GeneratorIndexOutcome::ArrayElement(result) => {
                assert!(matches!(result, Err(VmError::IndexOutOfBounds { .. })));
            }
            other => panic!("expected ArrayElement, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn generator_iter_index_array_wrapper_structref_reads_element() -> Result<(), VmError> {
        let mut memory = MemoryValue::undef_typed(&ArrayElementType::I64, 3);
        memory.set(1, Value::I64(10))?;
        memory.set(2, Value::I64(20))?;
        memory.set(3, Value::I64(30))?;
        let wrapper = StructInstance::with_name(
            0,
            "Array{Int64,1}".to_string(),
            vec![
                Value::MemoryRef(Box::new(MemoryRefValue::first(new_memory_ref(memory)))),
                Value::Tuple(TupleValue::new(vec![Value::I64(3)])),
            ],
        );
        let struct_heap = vec![wrapper];
        let iter = Value::StructRef(0);

        match generator_iter_index_with_heap(&iter, 2, &struct_heap)? {
            GeneratorIndexOutcome::ArrayElement(result) => {
                assert!(matches!(result?, Value::I64(20)));
            }
            other => panic!("expected ArrayElement, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn generator_iter_index_unit_range_narrows_to_i64() -> Result<(), VmError> {
        let iter = Value::Range(RangeValue::unit_range(1.0, 5.0));

        match generator_iter_index(&iter, 3)? {
            GeneratorIndexOutcome::Push(value) => assert!(matches!(value, Value::I64(3))),
            other => panic!("expected Push, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn generator_iter_index_range_out_of_bounds_raises() -> Result<(), VmError> {
        let iter = Value::Range(RangeValue::unit_range(1.0, 3.0));

        match generator_iter_index(&iter, 9)? {
            GeneratorIndexOutcome::RaiseOutOfBounds(VmError::IndexOutOfBounds {
                indices, ..
            }) => {
                assert_eq!(indices, vec![9]);
            }
            other => panic!("expected RaiseOutOfBounds, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn generator_iter_index_tuple_clones_single_element() -> Result<(), VmError> {
        let iter = Value::Tuple(TupleValue::new(vec![
            Value::I64(7),
            Value::Str("b".into()),
            Value::F64(2.5),
        ]));

        match generator_iter_index(&iter, 2)? {
            GeneratorIndexOutcome::Push(Value::Str(s)) => assert_eq!(s, "b"),
            other => panic!("expected Push(Str), got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn generator_iter_index_tuple_out_of_bounds_raises() -> Result<(), VmError> {
        let iter = Value::Tuple(TupleValue::new(vec![Value::I64(1)]));

        match generator_iter_index(&iter, 5)? {
            GeneratorIndexOutcome::RaiseOutOfBounds(VmError::IndexOutOfBounds {
                indices, ..
            }) => {
                assert_eq!(indices, vec![5]);
            }
            other => panic!("expected RaiseOutOfBounds, got {:?}", other),
        }
        Ok(())
    }

    #[test]
    fn generator_iter_index_unsupported_iter_is_internal_error() {
        let iter = Value::Str("nope".into());
        assert!(matches!(
            generator_iter_index(&iter, 1),
            Err(VmError::InternalError(_))
        ));
    }

    #[test]
    fn selected_indices_follow_reshaped_bool_parent() -> Result<(), VmError> {
        let source = new_array_ref(ArrayValue::memory_first_from_bool(
            vec![true, false, true],
            vec![3],
        ));
        let reshaped = ArrayValue::reshaped_from_ref(&source, vec![3])?;

        let indices = selected_indices_from_index_array(&reshaped)?;

        assert_eq!(indices, vec![1, 3]);
        Ok(())
    }

    #[test]
    fn selected_indices_preserve_f64_boolean_like_mode() -> Result<(), VmError> {
        let idx_arr = ArrayValue::memory_first_from_f64(vec![0.0, 1.0, 1.0], vec![3]);

        let indices = selected_indices_from_index_array(&idx_arr)?;

        assert_eq!(indices, vec![2, 3]);
        Ok(())
    }

    #[test]
    fn selected_indices_preserve_i64_index_mode() -> Result<(), VmError> {
        let idx_arr = ArrayValue::memory_first_from_i64(vec![3, 1], vec![2]);

        let indices = selected_indices_from_index_array(&idx_arr)?;

        assert_eq!(indices, vec![3, 1]);
        Ok(())
    }
}

impl<R: RngLike> Vm<R> {
    fn stack_top_is_struct_dict(&self) -> bool {
        self.stack
            .last()
            .is_some_and(|target| is_struct_ref_dict(target, &self.struct_heap))
    }

    fn dispatch_dict_getindex_with_popped_key(
        &mut self,
        key: Value,
    ) -> Result<DispatchAction, VmError> {
        let target = self.stack.pop_value()?;
        let args = vec![target, key];
        if let Some(func_index) = self.find_best_method_index(&["getindex", "Base.getindex"], &args)
        {
            self.start_function_call(func_index, args)?;
            return Ok(DispatchAction::Continue);
        }
        let type_name = self.get_type_name(&args[0]);
        Err(VmError::MethodError(format!(
            "no method matching getindex({})",
            type_name
        )))
    }

    /// Execute array indexing instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not an array index operation.
    #[inline]
    pub(in crate::vm) fn execute_array_index(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::IndexLoadTypedInbounds(n) => {
                if *n != 1 {
                    return Err(VmError::InternalError(
                        "IndexLoadTypedInbounds currently supports one index".to_string(),
                    ));
                }
                let idx = self.stack.pop_i64()?;
                if idx < 1 {
                    let target = self.stack.pop_value()?;
                    let shape = native_array_ref_from_value(target)
                        .ok()
                        .map(|arr| arr.borrow().shape.clone())
                        .unwrap_or_default();
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![idx],
                        shape,
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                let linear = (idx - 1) as usize;
                match self.stack.pop() {
                    Some(val) => match native_array_ref_from_value(val) {
                        Ok(arr) => {
                            let arr_borrow = arr.borrow();
                            let val =
                                match self.try_or_handle(arr_borrow.get_linear_inbounds(linear))? {
                                    Some(v) => v,
                                    None => {
                                        drop(arr_borrow);
                                        return Ok(DispatchAction::Continue);
                                    }
                                };
                            self.stack.push(val);
                        }
                        Err(target) => {
                            self.stack.push(target);
                            self.stack.push(Value::I64(idx));
                            return self.execute_array_index(&Instr::IndexLoad(1));
                        }
                    },
                    None => {
                        return Err(VmError::InternalError(
                            "IndexLoadTypedInbounds requires TypedArray".to_string(),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IndexLoadTyped(n) => {
                // Support CartesianIndex: A[CartesianIndex((i, j))] == A[i, j]
                let indices = if *n == 1 {
                    let val = self.stack.pop_value()?;
                    match val {
                        Value::I64(v) => vec![v],
                        Value::Struct(s) if &*s.struct_name == "CartesianIndex" => {
                            extract_cartesian_index_indices(&s)?
                        }
                        Value::StructRef(idx) => {
                            let s = self.struct_heap.get(idx).ok_or_else(|| {
                                VmError::TypeError("Invalid struct ref".to_string())
                            })?;
                            if &*s.struct_name == "CartesianIndex" {
                                extract_cartesian_index_indices(s)?
                            } else if let Some(selected_indices) =
                                selected_indices_from_array_wrapper(s)?
                            {
                                let target = self.stack.pop_value()?;
                                self.stack.push(load_selected_array_elements(
                                    target,
                                    &selected_indices,
                                    &self.struct_heap,
                                )?);
                                return Ok(DispatchAction::Continue);
                            } else {
                                // INTERNAL: StructRef index in IndexLoadTyped is compiler-generated; invalid ref means heap corruption
                                return Err(VmError::InternalError(format!(
                                    "expected I64 or CartesianIndex, got {}",
                                    s.struct_name
                                )));
                            }
                        }
                        other => match native_array_ref_from_value(other) {
                            Ok(idx_arr_ref) => {
                                // Boolean/logical array indexing: arr[bool_array] (Issue #2694)
                                // Handle Array indices at runtime by extracting true-indices
                                // from boolean arrays, or using integer arrays directly.
                                let idx_arr = idx_arr_ref.borrow();
                                let selected_indices = selected_indices_from_index_array(&idx_arr)?;
                                drop(idx_arr);

                                let target = self.stack.pop_value()?;
                                self.stack.push(load_selected_array_elements(
                                    target,
                                    &selected_indices,
                                    &self.struct_heap,
                                )?);
                                return Ok(DispatchAction::Continue);
                            }
                            Err(other) => {
                                // INTERNAL: IndexLoadTyped index type is compiler-typed; non-CartesianIndex struct is a compiler bug
                                return Err(VmError::InternalError(format!(
                                    "expected I64 or CartesianIndex, got {:?}",
                                    util::value_type_name(&other)
                                )));
                            }
                        },
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
                    Some(val) => match native_array_ref_from_value(val) {
                        Ok(arr) => {
                            let arr_borrow = arr.borrow();
                            // Use try_or_handle so out-of-bounds errors can be caught by try/catch
                            let val = match self.try_or_handle(arr_borrow.get(&indices))? {
                                Some(v) => v,
                                None => {
                                    drop(arr_borrow);
                                    return Ok(DispatchAction::Continue);
                                }
                            };
                            self.stack.push(val);
                        }
                        Err(target) => {
                            // Graceful fallback (Issue #8132): IndexLoadTyped was
                            // emitted because compile-time inference saw the *generic*
                            // method's return type (e.g. `LinearAlgebra.diag(A)` ->
                            // `Vector{Float64}`), but the value loaded at runtime is a
                            // package/user override's result whose concrete type differs
                            // (e.g. an `SVector`/`StaticArray` or a user struct). Such a
                            // value is not a native typed array, so dispatch the element
                            // read through the generic `getindex` method, which handles
                            // StaticArray flat reprs, Array struct wrappers, and user
                            // structs alike — mirroring `IndexLoadTypedInbounds`, which
                            // already delegates non-array targets to the generic path.
                            let mut args = Vec::with_capacity(indices.len() + 1);
                            args.push(target);
                            for idx in indices {
                                args.push(Value::I64(idx));
                            }
                            if let Some(func_index) =
                                self.find_best_method_index(&["getindex", "Base.getindex"], &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching getindex({})",
                                type_name
                            )));
                        }
                    },
                    None => {
                        // INTERNAL: IndexLoadTyped requires an Array target; wrong type is a compiler bug
                        return Err(VmError::InternalError(
                            "IndexLoadTyped requires TypedArray".to_string(),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IndexStoreTyped(n) => {
                let val = self.stack.pop_value()?;
                let mut indices = Vec::with_capacity(*n);
                for _ in 0..*n {
                    indices.push(self.stack.pop_i64()?);
                }
                indices.reverse();
                match self.stack.pop() {
                    Some(popped) => match native_array_ref_from_value(popped) {
                        Ok(arr) => {
                            // Special handling for struct arrays: convert Value::Struct to StructRef
                            let mut arr_mut = arr.borrow_mut();
                            if arr_mut.is_struct_ref_array() {
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
                                        )));
                                    }
                                }
                            } else {
                                arr_mut.set(&indices, val)?;
                            }
                            drop(arr_mut);
                            self.stack.push(array_value(arr));
                        }
                        Err(target @ Value::Struct(_)) | Err(target @ Value::StructRef(_)) => {
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
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                        Err(target) => {
                            self.stack.push(target);
                            for idx in indices {
                                self.stack.push(Value::I64(idx));
                            }
                            self.stack.push(val);
                            return self.execute_array_index(&Instr::IndexStore(*n));
                        }
                    },
                    None => {
                        // INTERNAL: IndexStoreTyped requires an Array target; wrong type is a compiler bug
                        return Err(VmError::InternalError(
                            "IndexStoreTyped requires TypedArray".to_string(),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IndexLoad(n) => {
                let datatype_target_idx = self.stack.len().checked_sub(*n + 1);
                if let Some(target_idx) = datatype_target_idx {
                    if let Some(Value::DataType(_)) = self.stack.get(target_idx) {
                        let mut values = Vec::with_capacity(*n);
                        for _ in 0..*n {
                            values.push(self.stack.pop_value()?);
                        }
                        values.reverse();
                        let target = self.stack.pop_value()?;
                        if let Value::DataType(jt) = target {
                            self.stack.push(typed_array_from_datatype_values(
                                &jt,
                                values,
                                &self.struct_heap,
                            )?);
                            return Ok(DispatchAction::Continue);
                        }
                        return Err(VmError::InternalError(
                            "IndexLoad DataType target changed while popping values".to_string(),
                        ));
                    }
                }

                // Support CartesianIndex: A[CartesianIndex((i, j))] == A[i, j]
                // Also supports Dict indexing with non-integer keys (Issue #1814)
                let indices = if *n == 1 {
                    // When n==1, check if it's a CartesianIndex (which expands to multiple indices)
                    // or a non-integer key for Dict indexing
                    let val = self.stack.pop_value()?;
                    match val {
                        Value::I64(v) => vec![v],
                        Value::Struct(s) if &*s.struct_name == "CartesianIndex" => {
                            extract_cartesian_index_indices(&s)?
                        }
                        Value::Struct(s) => {
                            let struct_name = s.struct_name.to_string();
                            if let Some(selected_indices) = selected_indices_from_array_wrapper(&s)?
                            {
                                let target = self.stack.pop_value()?;
                                self.stack.push(load_selected_array_elements(
                                    target,
                                    &selected_indices,
                                    &self.struct_heap,
                                )?);
                                return Ok(DispatchAction::Continue);
                            }
                            if self.stack_top_is_struct_dict() {
                                return self
                                    .dispatch_dict_getindex_with_popped_key(Value::Struct(s));
                            }
                            return Err(VmError::TypeError(format!(
                                "expected I64 or CartesianIndex, got {}",
                                struct_name
                            )));
                        }
                        Value::StructRef(idx) => {
                            let s = self.struct_heap.get(idx).ok_or_else(|| {
                                VmError::TypeError("Invalid struct ref".to_string())
                            })?;
                            let struct_name = s.struct_name.to_string();
                            if &*s.struct_name == "CartesianIndex" {
                                extract_cartesian_index_indices(s)?
                            } else if let Some(selected_indices) =
                                selected_indices_from_array_wrapper(s)?
                            {
                                let target = self.stack.pop_value()?;
                                self.stack.push(load_selected_array_elements(
                                    target,
                                    &selected_indices,
                                    &self.struct_heap,
                                )?);
                                return Ok(DispatchAction::Continue);
                            } else {
                                // Dict lookup through an Any-typed receiver reaches
                                // IndexLoad. Struct keys must use the same Dict
                                // fallback as primitive non-integer keys instead of
                                // being rejected as array indices (Issue #8397).
                                if self.stack_top_is_struct_dict() {
                                    return self.dispatch_dict_getindex_with_popped_key(
                                        Value::StructRef(idx),
                                    );
                                }
                                // User-visible: user can index an array with a non-CartesianIndex struct at runtime
                                return Err(VmError::TypeError(format!(
                                    "expected I64 or CartesianIndex, got {}",
                                    struct_name
                                )));
                            }
                        }
                        other => match native_array_ref_from_value(other) {
                            Ok(idx_arr_ref) => {
                                // Boolean/logical array indexing: arr[bool_array] (Issue #2694)
                                // When the compiler cannot determine the index is an Array at
                                // compile time, IndexLoad is emitted instead of IndexSlice.
                                // Handle Array indices at runtime by extracting true-indices
                                // from boolean arrays, or using integer arrays directly.
                                let idx_arr = idx_arr_ref.borrow();
                                let selected_indices = selected_indices_from_index_array(&idx_arr)?;
                                drop(idx_arr);

                                let target = self.stack.pop_value()?;
                                self.stack.push(load_selected_array_elements(
                                    target,
                                    &selected_indices,
                                    &self.struct_heap,
                                )?);
                                return Ok(DispatchAction::Continue);
                            }
                            Err(other) => {
                                // Non-integer key: check if target is a Dict (Issue #1814)
                                // When a Dict is passed to a function as Any-typed parameter,
                                // the compiler emits IndexLoad instead of CallBuiltin(DictGet).
                                // Handle Dict lookup at runtime.
                                let target = self.stack.pop_value()?;
                                // StructRef Dict dispatch: when a Pure Julia Dict struct
                                // is indexed with non-integer keys, dispatch to getindex
                                // method via find_best_method_index. (Issue #2748)
                                if is_struct_ref_dict(&target, &self.struct_heap) {
                                    let args = vec![target, other];
                                    if let Some(func_index) = self.find_best_method_index(
                                        &["getindex", "Base.getindex"],
                                        &args,
                                    ) {
                                        self.start_function_call(func_index, args)?;
                                        return Ok(DispatchAction::Continue);
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
                        },
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
                match native_array_ref_from_value(target) {
                    Ok(arr) => {
                        let arr_borrow = arr.borrow();
                        let val = match self.try_or_handle(arr_borrow.get(&indices))? {
                            Some(val) => val,
                            None => return Ok(DispatchAction::Continue),
                        };
                        self.stack.push(val);
                    }
                    Err(target) => match target {
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
                                return Ok(DispatchAction::Continue);
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
                                return Ok(DispatchAction::Continue);
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
                        // Tuple and Core.SimpleVector share 1-based linear
                        // indexing semantics (Issue #4722).
                        Value::Tuple(tuple) | Value::SimpleVector(tuple) => {
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
                                return Ok(DispatchAction::Continue);
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
                                return Ok(DispatchAction::Continue);
                            }
                            let element = named.values[(idx - 1) as usize].clone();
                            self.stack.push(element);
                        }
                        Value::Pairs(pairs) => {
                            if indices.len() != 1 {
                                return Err(VmError::TypeError(
                                    "Pairs indexing requires exactly one index".to_string(),
                                ));
                            }
                            let idx = indices[0];
                            if idx < 1 || idx > pairs.data.values.len() as i64 {
                                self.raise(VmError::IndexOutOfBounds {
                                    indices: vec![idx],
                                    shape: vec![pairs.data.values.len()],
                                })?;
                                return Ok(DispatchAction::Continue);
                            }
                            self.stack
                                .push(pairs.data.values[(idx - 1) as usize].clone());
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
                            let range_len = range.length();
                            if range.step == 0.0 {
                                // User-visible: user can create a Range with step=0 and then index it
                                return Err(VmError::TypeError(
                                    "Range step cannot be zero".to_string(),
                                ));
                            }
                            if idx < 1 || idx > range_len {
                                self.raise(VmError::IndexOutOfBounds {
                                    indices: vec![idx],
                                    shape: vec![range_len as usize],
                                })?;
                                return Ok(DispatchAction::Continue);
                            }
                            // Calculate element: start + (idx - 1) * step.
                            // Issue #4760: honor `is_float` so a Float64
                            // whole-number range like `(0.0:1.0:5.0)[1]`
                            // returns `Float64(0.0)`, not `Int64(0)`.
                            // For non-float ranges, keep the legacy
                            // "narrow to I64 when integer-valued" path
                            // to preserve Int element type for
                            // `(1:5)[1] === Int64(1)`.
                            // Issue #4795: also exclude Char ranges from
                            // the Int64 narrowing path so `('a':'e')[1]`
                            // returns `Char('a')`, not `Int64(97)`.
                            let element = range.start + ((idx - 1) as f64) * range.step;
                            if !range.is_float
                                && !matches!(
                                    range.element_type,
                                    crate::vm::value::RangeElementType::Char
                                )
                                && range.is_unit_range()
                                && element.fract() == 0.0
                            {
                                self.stack.push(Value::I64(element as i64));
                            } else {
                                self.stack.push(range.typed_element(element));
                            }
                        }
                        Value::DataType(jt) => {
                            self.stack
                                .push(typed_array_from_datatype_indices(&jt, &indices)?);
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
                            // Issue #5088: resolve the index against the boxed
                            // iter *by reference* via `generator_iter_index`
                            // instead of deep-cloning `(*g.iter)` on every
                            // access. The helper performs only the minimal clone
                            // each carrier needs (Rc bump for arrays, scalar
                            // copy for ranges, single-element clone for tuples),
                            // keeping this fallback path allocation-light.
                            match generator_iter_index_with_heap(
                                &g.iter,
                                gen_idx,
                                &self.struct_heap,
                            )? {
                                GeneratorIndexOutcome::ArrayElement(item) => {
                                    let v = match self.try_or_handle(item)? {
                                        Some(v) => v,
                                        None => return Ok(DispatchAction::Continue),
                                    };
                                    self.stack.push(v);
                                }
                                GeneratorIndexOutcome::RaiseOutOfBounds(err) => {
                                    self.raise(err)?;
                                    return Ok(DispatchAction::Continue);
                                }
                                GeneratorIndexOutcome::Push(value) => {
                                    self.stack.push(value);
                                }
                            }
                        }
                        target @ Value::Struct(_) | target @ Value::StructRef(_) => {
                            // Fast path (#6806 PR B): a MemoryRef-backed Array{T,N}
                            // wrapper indexed by integers reads the element directly
                            // from its Memory, skipping the per-index getindex method
                            // dispatch. Inline structs, non-MemoryRef storage, user
                            // structs, and trailing-singleton arities defer to
                            // dispatch below. Gated on the shared #6657 flag: when
                            // the program defines a user `getindex` array override
                            // the fast path is refused so the override is reached via
                            // dispatch (matching the runtime specializer's behavior).
                            if !self.disable_array_getindex_specialization() {
                                if let Some(result) =
                                    memoryref_wrapper_element(&target, &indices, &self.struct_heap)
                                {
                                    let val = match self.try_or_handle(result)? {
                                        Some(val) => val,
                                        None => return Ok(DispatchAction::Continue),
                                    };
                                    self.stack.push(val);
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            let mut args = Vec::with_capacity(indices.len() + 1);
                            args.push(target);
                            for idx in &indices {
                                args.push(Value::I64(*idx));
                            }
                            if let Some(func_index) =
                                self.find_best_method_index(&["getindex", "Base.getindex"], &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            // No user/Base `getindex` method is loaded (e.g. a bare
                            // VM constructed without Base): index an `Array{T,N}`
                            // wrapper directly so raw `IndexLoad` keeps working on the
                            // MemoryRef-backed representation now emitted by array
                            // producers (Issue #6806). Non-wrapper structs return
                            // `None` here and fall through to the MethodError below
                            // unchanged; programs with Base loaded never reach this
                            // fallback because dispatch resolves `getindex` first.
                            if let Some(arr) = crate::vm::value::array_wrapper_value_to_array_value(
                                &args[0],
                                &self.struct_heap,
                            )? {
                                let val = match self.try_or_handle(arr.get(&indices))? {
                                    Some(val) => val,
                                    None => return Ok(DispatchAction::Continue),
                                };
                                self.stack.push(val);
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching getindex({})",
                                type_name
                            )));
                        }
                        Value::Ref(inner) => {
                            // Ref indexing: r[] or r[1] unwraps the contained value (Issue #2687)
                            // In Julia, getindex(r::Ref) = r.x (returns the wrapped scalar)
                            let v = inner.borrow().clone();
                            self.stack.push(v);
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
                                return Ok(DispatchAction::Continue);
                            }
                            let val = mem
                                .borrow()
                                .get(idx as usize)
                                .map_err(|e| VmError::TypeError(format!("BoundsError: {}", e)))?;
                            self.stack.push(val);
                        }
                        // Issue #7964: StaticArray flat representations — support
                        // 1-D (linear) and 2-D (row, col) integer indexing.
                        // 2-D uses the column-major formula matching upstream
                        // StaticArrays / Julia: element (i,j) of an M×N matrix is at
                        // data[(j-1)*M + i] (1-based), i.e. 0-based (col-1)*rows +
                        // (row-1). Linear `[i]` reads the column-major backing tuple
                        // directly (Issue #8084).
                        Value::StaticArray(sv) => {
                            let idx = match indices.as_slice() {
                                [i] => {
                                    let n = sv.len() as i64;
                                    if *i < 1 || *i > n {
                                        self.raise(VmError::IndexOutOfBounds {
                                            indices: vec![*i],
                                            shape: vec![n as usize],
                                        })?;
                                        return Ok(DispatchAction::Continue);
                                    }
                                    (*i - 1) as usize
                                }
                                [row, col] => {
                                    let (r, c) = (sv.rows as i64, sv.cols as i64);
                                    if *row < 1 || *row > r || *col < 1 || *col > c {
                                        self.raise(VmError::IndexOutOfBounds {
                                            indices: vec![*row, *col],
                                            shape: vec![r as usize, c as usize],
                                        })?;
                                        return Ok(DispatchAction::Continue);
                                    }
                                    ((*col - 1) * r + (*row - 1)) as usize
                                }
                                _ => {
                                    return Err(VmError::TypeError(format!(
                                        "StaticArray: unsupported index arity {}",
                                        indices.len()
                                    )))
                                }
                            };
                            let val = sv.elems.get_value(idx).ok_or_else(|| {
                                VmError::IndexOutOfBounds {
                                    indices: vec![(idx + 1) as i64],
                                    shape: vec![sv.len()],
                                }
                            })?;
                            self.stack.push(val);
                        }
                        Value::StaticArrayInline(sv) => {
                            let idx = match indices.as_slice() {
                                [i] => {
                                    let n = sv.len() as i64;
                                    if *i < 1 || *i > n {
                                        self.raise(VmError::IndexOutOfBounds {
                                            indices: vec![*i],
                                            shape: vec![n as usize],
                                        })?;
                                        return Ok(DispatchAction::Continue);
                                    }
                                    (*i - 1) as usize
                                }
                                [row, col] => {
                                    let (r, c) = (sv.rows() as i64, sv.cols() as i64);
                                    if *row < 1 || *row > r || *col < 1 || *col > c {
                                        self.raise(VmError::IndexOutOfBounds {
                                            indices: vec![*row, *col],
                                            shape: vec![r as usize, c as usize],
                                        })?;
                                        return Ok(DispatchAction::Continue);
                                    }
                                    ((*col - 1) * r + (*row - 1)) as usize
                                }
                                _ => {
                                    return Err(VmError::TypeError(format!(
                                        "StaticArrayInline: unsupported index arity {}",
                                        indices.len()
                                    )))
                                }
                            };
                            self.stack.push(sv.get_0indexed(idx));
                        }
                        other if is_scalar_carrier(&other) => {
                            // Issue #4814: scalars in upstream Julia behave as
                            // 0-dimensional collections of length 1. `x[1] == x`,
                            // `x[i]` for any `i != 1` raises `BoundsError`. The
                            // `Number ∪ AbstractChar` predicate lives in
                            // `vm/value/predicates.rs` (Issue #4875) so the
                            // boundary stays in lock-step with `Length` and any
                            // future scalar-aware builtin.
                            if indices.len() != 1 {
                                return Err(VmError::TypeError(format!(
                                    "scalar indexing requires exactly one index (got {})",
                                    indices.len()
                                )));
                            }
                            let idx = indices[0];
                            if idx != 1 {
                                self.raise(VmError::IndexOutOfBounds {
                                    indices: vec![idx],
                                    shape: vec![1],
                                })?;
                                return Ok(DispatchAction::Continue);
                            }
                            self.stack.push(other);
                        }
                        other => {
                            // User-visible: user can attempt to index an unsupported type
                            return Err(VmError::TypeError(format!(
                                "indexing not supported for {:?}",
                                other
                            )));
                        }
                    },
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IndexSlice(n) => self.execute_index_slice(*n),

            Instr::IndexLoadInbounds(n) => {
                if *n != 1 {
                    return Err(VmError::InternalError(
                        "IndexLoadInbounds currently supports one index".to_string(),
                    ));
                }
                let idx = self.stack.pop_i64()?;
                if idx < 1 {
                    let target = self.stack.pop_value()?;
                    let shape = native_array_ref_from_value(target)
                        .ok()
                        .map(|arr| arr.borrow().shape.clone())
                        .unwrap_or_default();
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![idx],
                        shape,
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                let linear = (idx - 1) as usize;
                let target = self.stack.pop_value()?;
                match native_array_ref_from_value(target) {
                    Ok(arr) => {
                        let arr_borrow = arr.borrow();
                        let val =
                            match self.try_or_handle(arr_borrow.get_linear_inbounds(linear))? {
                                Some(v) => v,
                                None => {
                                    drop(arr_borrow);
                                    return Ok(DispatchAction::Continue);
                                }
                            };
                        self.stack.push(val);
                    }
                    Err(target) => {
                        self.stack.push(target);
                        self.stack.push(Value::I64(idx));
                        return self.execute_array_index(&Instr::IndexLoad(1));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IndexStoreInbounds(n) => {
                if *n != 1 {
                    return Err(VmError::InternalError(
                        "IndexStoreInbounds currently supports one index".to_string(),
                    ));
                }
                let val = self.stack.pop_value()?;
                let idx = self.stack.pop_i64()?;
                if idx < 1 {
                    let target = self.stack.pop_value()?;
                    let shape = native_array_ref_from_value(target)
                        .ok()
                        .map(|arr| arr.borrow().shape.clone())
                        .unwrap_or_default();
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![idx],
                        shape,
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                let linear = (idx - 1) as usize;
                match self.stack.pop() {
                    Some(popped) => match native_array_ref_from_value(popped) {
                        Ok(arr) => {
                            {
                                let mut arr_mut = arr.borrow_mut();
                                if self
                                    .try_or_handle(arr_mut.set_linear_inbounds(linear, val))?
                                    .is_none()
                                {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            self.stack.push(array_value(arr));
                        }
                        Err(target) => {
                            self.stack.push(target);
                            self.stack.push(Value::I64(idx));
                            self.stack.push(val);
                            return self.execute_array_index(&Instr::IndexStore(1));
                        }
                    },
                    None => {
                        return Err(VmError::InternalError(
                            "IndexStoreInbounds requires Array".to_string(),
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IndexStore(n) => {
                // Ref mutation: r[] = v  (setindex!(r::RefValue, v)) (Issue #5130).
                // Stack: [ref, value] when n == 0. Interior mutability means aliases
                // observe the write, matching upstream Base.RefValue semantics.
                if *n == 0 {
                    let stack_len = self.stack.len();
                    if stack_len >= 2 && matches!(&self.stack[stack_len - 2], Value::Ref(_)) {
                        let value = self.stack.pop_value()?;
                        let target = self.stack.pop_value()?;
                        if let Value::Ref(cell) = target {
                            *cell.borrow_mut() = value;
                            // IndexStore leaves the modified collection on the stack
                            // (compiler emits Pop/StoreArray afterward).
                            self.stack.push(Value::Ref(cell));
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                // Early Dict handling for non-integer keys (Issue #1814)
                // Stack: [collection, index, value] when n==1
                // When a Dict is passed as Any-typed parameter, the compiler emits
                // IndexStore instead of CallBuiltin(DictSet). Handle at runtime.
                if *n == 1 {
                    let stack_len = self.stack.len();
                    if stack_len >= 3 {
                        // StructRef Dict dispatch: when a Pure Julia Dict struct is
                        // indexed with non-integer keys, dispatch to setindex! method.
                        // The compiler emits IndexStore for Any-typed collections,
                        // but pop_i64 fails on non-integer keys. (Issue #2748)
                        let is_struct_dict =
                            is_struct_ref_dict(&self.stack[stack_len - 3], &self.struct_heap);
                        if is_struct_dict {
                            let value = self.stack.pop_value()?;
                            let key = self.stack.pop_value()?;
                            let target = self.stack.pop_value()?;
                            let args = vec![target, value, key];
                            if let Some(func_index) =
                                self.find_best_method_index(&["setindex!", "Base.setindex!"], &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                    }
                }

                // Fast path (#6806 PR B): native write into a MemoryRef-backed
                // numeric Array{T} wrapper for a single integer index and a plain
                // numeric value, skipping setindex! dispatch. Stack is
                // [target, index, value]. Gated on the user-`setindex!`-override
                // flag so an override is still reached via dispatch. Restricted to
                // numeric storage + numeric value, where the `ArrayData` numeric
                // coercion is equivalent to `setindex!`'s `convert(T, v)`; every
                // other shape (Bool/complex/struct/Any storage, non-numeric value,
                // multi-index) falls through to the existing handling unchanged.
                if *n == 1 && !self.disable_array_setindex_specialization() {
                    let len = self.stack.len();
                    if len >= 3
                        && matches!(&self.stack[len - 2], Value::I64(_))
                        && fast_store_applies(
                            &self.stack[len - 3],
                            &self.stack[len - 1],
                            &self.struct_heap,
                        )
                    {
                        let value = self.stack.pop_value()?;
                        let index = self.stack.pop_i64()?;
                        let target = self.stack.pop_value()?;
                        let write = if let Value::StructRef(sidx) = &target {
                            match self.struct_heap.get(*sidx) {
                                Some(instance) => {
                                    let shape = instance
                                        .values
                                        .get(1)
                                        .and_then(array_wrapper_shape_and_offset)
                                        .map(|(s, _)| s)
                                        .unwrap_or_default();
                                    let total: usize = shape.iter().product();
                                    if index < 1 || index as usize > total {
                                        Err(VmError::IndexOutOfBounds {
                                            indices: vec![index],
                                            shape,
                                        })
                                    } else if let Some(Value::MemoryRef(memref)) =
                                        instance.values.first()
                                    {
                                        memref.set(index as usize, value)
                                    } else {
                                        // Unreachable: the peek confirmed MemoryRef storage.
                                        Ok(())
                                    }
                                }
                                None => Ok(()),
                            }
                        } else {
                            Ok(())
                        };
                        match self.try_or_handle(write)? {
                            Some(()) => {}
                            None => return Ok(DispatchAction::Continue),
                        }
                        // IndexStore leaves the (mutated) collection on the stack
                        // for the compiler's subsequent StoreBack.
                        self.stack.push(target);
                        return Ok(DispatchAction::Continue);
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
                let is_symbol_val = matches!(self.stack.last(), Some(Value::Symbol(_)));
                let is_nothing_val = matches!(self.stack.last(), Some(Value::Nothing));
                let is_bigint_val = matches!(self.stack.last(), Some(Value::BigInt(_)));
                let is_macro_ast_val = matches!(
                    self.stack.last(),
                    Some(
                        Value::Expr(_)
                            | Value::QuoteNode(_)
                            | Value::LineNumberNode(_)
                            | Value::GlobalRef(_)
                    )
                );
                // Issue #5196: type-system value objects (DataType, TypeVar,
                // SimpleVector) must be stored verbatim into Any/abstract
                // containers. Without this they fell through to the numeric
                // `else` arm, which calls `pop_f64_or_i64` and fails with
                // "expected numeric value, got DataType" (e.g. storing the
                // elements of `Tuple{Int,String}.parameters` into a Vector{Any}
                // during `collect`).
                let is_type_object_val = matches!(
                    self.stack.last(),
                    Some(Value::DataType(_))
                        | Some(Value::RuntimeTypeVar(_))
                        | Some(Value::SimpleVector(_))
                );
                // Issue #5233: heap container values (Dict, Set) returned from a
                // `collect`/`map`/comprehension element store must be kept
                // verbatim when written into an `Any`/abstract result array
                // (e.g. `collect(::Dict)` over a Dict-of-Dicts, or
                // `map(x -> Dict(...), v)`). Without this they fell through to
                // the numeric `else` arm, which calls `pop_f64_or_i64` and
                // fails with "expected numeric value, got Dict". Pair values are
                // already `Value::Struct`/`Value::StructRef` and are handled by
                // the `is_struct_val` branch below.
                // `Value::Set`/`Value::Dict` carriers removed (Issues #6731/#6732);
                // heap containers are StructRef values handled by the struct branch.
                let is_container_heap_val = false;
                let is_struct_val = matches!(
                    self.stack.last(),
                    Some(Value::Struct(_)) | Some(Value::StructRef(_))
                );
                // Issue #3648: Vector{Any} container assigned an Array element
                // (e.g. nested arrays). The default numeric-only IndexStore path
                // would reject the Array. Capture it here so the value is stored
                // verbatim, mirroring the Tuple/String/Char branches below.
                let is_array_val = self
                    .stack
                    .last()
                    .is_some_and(|value| value.value_type() == ValueType::Array);
                let is_numeric_val = self.stack.last().is_some_and(|value| {
                    matches!(
                        value,
                        Value::I8(_)
                            | Value::I16(_)
                            | Value::I32(_)
                            | Value::I64(_)
                            | Value::I128(_)
                            | Value::U8(_)
                            | Value::U16(_)
                            | Value::U32(_)
                            | Value::U64(_)
                            | Value::U128(_)
                            | Value::F16(_)
                            | Value::F32(_)
                            | Value::F64(_)
                            | Value::Bool(_)
                    )
                });
                let is_boxed_numeric_target = if is_numeric_val {
                    let target_index = self.stack.len().checked_sub(*n + 2);
                    target_index.is_some_and(|idx| {
                        boxed_numeric_indexstore_target(&self.stack[idx], &self.struct_heap)
                    })
                } else {
                    false
                };
                let is_boxed_struct_target = if is_struct_val {
                    let target_index = self.stack.len().checked_sub(*n + 2);
                    target_index.is_some_and(|idx| {
                        boxed_struct_indexstore_target(&self.stack[idx])
                            || matches!(self.stack[idx], Value::Struct(_) | Value::StructRef(_))
                    })
                } else {
                    false
                };
                let mut indices = Vec::with_capacity(*n);

                if is_complex_val {
                    let complex_val = self.stack.pop_value()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();
                    let arr_val = self.stack.pop_value()?;
                    match native_array_ref_from_value(arr_val) {
                        Ok(arr) => {
                            let (re, im) = match &complex_val {
                                Value::Struct(s) => s.as_complex_parts().ok_or_else(|| {
                                    VmError::TypeError(
                                        "Invalid Complex struct for IndexStore".to_string(),
                                    )
                                })?,
                                Value::StructRef(idx) => self
                                    .struct_heap
                                    .get(*idx)
                                    .ok_or_else(|| {
                                        VmError::TypeError(format!(
                                            "Invalid StructRef index {}",
                                            idx
                                        ))
                                    })?
                                    .as_complex_parts()
                                    .ok_or_else(|| {
                                        VmError::TypeError(
                                            "Invalid Complex struct for IndexStore".to_string(),
                                        )
                                    })?,
                                _ => {
                                    return Err(VmError::TypeError(
                                        "IndexStore expected Complex value".to_string(),
                                    ));
                                }
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
                                    return Ok(DispatchAction::Continue);
                                }
                            } else {
                                let stored_val = match complex_val {
                                    Value::StructRef(_) => complex_val,
                                    Value::Struct(s) => {
                                        let struct_idx = self.struct_heap.len();
                                        self.struct_heap.push(s);
                                        Value::StructRef(struct_idx)
                                    }
                                    _ => {
                                        return Err(VmError::TypeError(
                                            "IndexStore expected Complex value".to_string(),
                                        ));
                                    }
                                };
                                let mut arr_mut = arr.borrow_mut();
                                let set_result = arr_mut.set(&indices, stored_val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            self.stack.push(array_value(arr));
                        }
                        Err(Value::Memory(mem)) => {
                            if indices.len() != 1 {
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
                                return Ok(DispatchAction::Continue);
                            }
                            mem.borrow_mut()
                                .set(idx as usize, complex_val)
                                .map_err(|e| VmError::TypeError(format!("BoundsError: {}", e)))?;
                            self.stack.push(Value::Memory(mem));
                        }
                        Err(target @ Value::Struct(_)) | Err(target @ Value::StructRef(_)) => {
                            let mut args = Vec::with_capacity(indices.len() + 2);
                            args.push(target);
                            args.push(complex_val);
                            for idx in indices {
                                args.push(Value::I64(idx));
                            }
                            if let Some(func_index) =
                                self.find_best_method_index(&["setindex!", "Base.setindex!"], &args)
                            {
                                self.start_function_call(func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                        Err(other) => {
                            return Err(VmError::TypeError(format!(
                                "IndexStore: expected Array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else if is_tuple_val {
                    // Handle Tuple value - store directly into array
                    let tuple_val = self.stack.pop_value()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();

                    let arr_val = self.stack.pop_value()?;
                    match native_array_ref_from_value(arr_val) {
                        Ok(arr) => {
                            {
                                let mut arr_borrow = arr.borrow_mut();
                                let set_result = arr_borrow.set(&indices, tuple_val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            self.stack.push(array_value(arr));
                        }
                        Err(Value::Memory(mem)) => {
                            // Tuple-typed Array wrappers store through their backing Memory.
                            // Keep this aligned with the other Memory IndexStore branches
                            // so Pure Julia setindex!(::Array{Tuple{...}}) can write via
                            // `a._mem[i] = tuple` (Issue #4578).
                            if indices.len() != 1 {
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
                                return Ok(DispatchAction::Continue);
                            }
                            mem.borrow_mut()
                                .set(idx as usize, tuple_val)
                                .map_err(|e| VmError::TypeError(format!("BoundsError: {}", e)))?;
                            self.stack.push(Value::Memory(mem));
                        }
                        Err(target @ Value::Struct(_)) | Err(target @ Value::StructRef(_)) => {
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
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                        Err(other) => {
                            // User-visible: user can IndexStore a Tuple into an unsupported collection type
                            return Err(VmError::TypeError(format!(
                                "IndexStore: expected Array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else if is_array_val {
                    // Issue #3648: Array element stored into a heterogeneous container
                    // (e.g., result[i] = arr[i] where arr[i] is itself an Array).
                    let val = self.stack.pop_value()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();

                    let arr_val = self.stack.pop_value()?;
                    match native_array_ref_from_value(arr_val) {
                        Ok(arr) => {
                            {
                                let mut arr_borrow = arr.borrow_mut();
                                let set_result = arr_borrow.set(&indices, val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            self.stack.push(array_value(arr));
                        }
                        Err(Value::Memory(mem)) => {
                            if indices.len() != 1 {
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
                                return Ok(DispatchAction::Continue);
                            }
                            mem.borrow_mut()
                                .set(idx as usize, val)
                                .map_err(|e| VmError::TypeError(format!("BoundsError: {}", e)))?;
                            self.stack.push(Value::Memory(mem));
                        }
                        Err(target @ Value::Struct(_)) | Err(target @ Value::StructRef(_)) => {
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
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                        Err(other) => {
                            return Err(VmError::TypeError(format!(
                                "IndexStore: expected Array, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    }
                } else if is_string_val
                    || is_char_val
                    || is_symbol_val
                    || is_nothing_val
                    || is_bigint_val
                    || is_macro_ast_val
                    || is_type_object_val
                    || is_container_heap_val
                    || is_boxed_numeric_target
                    || is_boxed_struct_target
                {
                    // Handle boxed scalar values directly. Julia's fill!/setindex!
                    // stores convert(T, x); Symbol is represented as boxed Any
                    // storage for Symbol arrays and Memory{Symbol} (Issues #4027/#4034).
                    // Macro expansion helpers also store Expr/QuoteNode/etc. into
                    // heterogeneous arrays, e.g. MacroTools @capture's generated
                    // assignment list (Issue #7538).
                    // BigInt is boxed arbitrary-precision storage; Any/abstract
                    // array slots must keep it verbatim instead of routing through
                    // the f64 scalar fallback (Issue #8262).
                    // Abstract/Any/Union Memory numeric targets also use this
                    // path so `Memory{Real}` preserves boxed Int64 values
                    // instead of routing through the f64 storage fast path.
                    let val = self.stack.pop_value()?;
                    for _ in 0..*n {
                        indices.push(self.stack.pop_i64()?);
                    }
                    indices.reverse();

                    let arr_val = self.stack.pop_value()?;
                    match native_array_ref_from_value(arr_val) {
                        Ok(arr) => {
                            {
                                let mut arr_borrow = arr.borrow_mut();
                                let set_result = arr_borrow.set(&indices, val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            self.stack.push(array_value(arr));
                        }
                        Err(target @ Value::Struct(_)) | Err(target @ Value::StructRef(_)) => {
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
                                return Ok(DispatchAction::Continue);
                            }
                            let type_name = self.get_type_name(&args[0]);
                            return Err(VmError::MethodError(format!(
                                "no method matching setindex!({})",
                                type_name
                            )));
                        }
                        Err(Value::Memory(mem)) => {
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
                                return Ok(DispatchAction::Continue);
                            }
                            mem.borrow_mut()
                                .set(idx as usize, val)
                                .map_err(|e| VmError::TypeError(format!("BoundsError: {}", e)))?;
                            self.stack.push(Value::Memory(mem));
                        }
                        Err(other) => {
                            // User-visible: user can IndexStore a boxed scalar into an unsupported collection type
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
                    match native_array_ref_from_value(arr_val) {
                        Ok(arr) => {
                            {
                                let mut arr_borrow = arr.borrow_mut();
                                let typed_val = scalar_indexstore_value_for_element_type(
                                    val,
                                    arr_borrow.element_type(),
                                );
                                let set_result = arr_borrow.set(&indices, typed_val);
                                if self.try_or_handle(set_result)?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            self.stack.push(array_value(arr));
                        }
                        Err(arr_val) => match arr_val {
                            Value::StructRef(struct_idx) => {
                                // Handle SubArray by inline implementation
                                // SubArray struct: old {parent, offset, len}
                                // or new {parent, indices, offset, len}.
                                let struct_val =
                                    self.struct_heap.get(struct_idx).ok_or_else(|| {
                                        VmError::TypeError("Invalid struct ref".to_string())
                                    })?;

                                // Check if this is a SubArray
                                if struct_val.struct_name.starts_with("SubArray") {
                                    // Route every single-index view `setindex!(v, x, i)`
                                    // to the pure-Julia method (#5816). Two reasons the
                                    // inline `offset + i` path below is wrong here:
                                    //  - a 2D view's linear index is column-major over
                                    //    its per-dimension `indices`, not 1D-contiguous;
                                    //  - the parent stored in a SubArray field is a boxed
                                    //    `StructRef`, not a bare native array, so the
                                    //    inline parent extraction fails for 1D AND 2D.
                                    // The pure-Julia method returns the SubArray (Julia
                                    // `setindex!` returns the collection), so the
                                    // `StoreBack` the compiler emits after `IndexStore`
                                    // writes the view back unchanged rather than clobbering
                                    // it with the stored value.
                                    if indices.len() == 1 {
                                        let target = Value::StructRef(struct_idx);
                                        let args = vec![
                                            target.clone(),
                                            Value::F64(val),
                                            Value::I64(indices[0]),
                                        ];
                                        if let Some(func_index) = self.find_best_method_index(
                                            &["setindex!", "Base.setindex!"],
                                            &args,
                                        ) {
                                            self.start_function_call(func_index, args)?;
                                            return Ok(DispatchAction::Continue);
                                        }
                                        let type_name = self.get_type_name(&target);
                                        return Err(VmError::MethodError(format!(
                                            "no method matching setindex!({})",
                                            type_name
                                        )));
                                    }
                                    if indices.len() != 1 {
                                        let target = Value::StructRef(struct_idx);
                                        let mut args = Vec::with_capacity(indices.len() + 2);
                                        args.push(target.clone());
                                        args.push(Value::F64(val));
                                        for idx in indices {
                                            args.push(Value::I64(idx));
                                        }
                                        if let Some(func_index) = self.find_best_method_index(
                                            &["setindex!", "Base.setindex!"],
                                            &args,
                                        ) {
                                            self.start_function_call(func_index, args)?;
                                            return Ok(DispatchAction::Continue);
                                        }
                                        let type_name = self.get_type_name(&target);
                                        return Err(VmError::MethodError(format!(
                                            "no method matching setindex!({})",
                                            type_name
                                        )));
                                    }
                                    // INTERNAL: SubArray parent field is compiler-assigned; non-Array parent is a compiler bug
                                    let parent_arr =
                                        sub_array_parent_array_ref(&struct_val.values)?;
                                    let (offset, len) = sub_array_offset_len(&struct_val.values)?;

                                    // Bounds check on view index
                                    let view_idx = indices[0];
                                    if view_idx < 1 || view_idx > len {
                                        self.raise(VmError::IndexOutOfBounds {
                                            indices: vec![view_idx],
                                            shape: vec![len as usize],
                                        })?;
                                        return Ok(DispatchAction::Continue);
                                    }

                                    // Calculate parent index: offset + view_idx (1-indexed)
                                    let parent_idx = offset + view_idx;

                                    // Set value in parent array
                                    {
                                        let mut arr_borrow = parent_arr.borrow_mut();
                                        let typed_val = scalar_indexstore_value_for_element_type(
                                            val,
                                            arr_borrow.element_type(),
                                        );
                                        let set_result = arr_borrow.set(&[parent_idx], typed_val);
                                        if self.try_or_handle(set_result)?.is_none() {
                                            return Ok(DispatchAction::Continue);
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
                                    if let Some(func_index) = self.find_best_method_index(
                                        &["setindex!", "Base.setindex!"],
                                        &args,
                                    ) {
                                        // Save the struct ref to push after function returns
                                        // For now, we call the function but note that the return value
                                        // will be the stored value, not the collection
                                        self.start_function_call(func_index, args)?;
                                        return Ok(DispatchAction::Continue);
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
                                    // INTERNAL: SubArray parent field is compiler-assigned; non-Array parent is a compiler bug
                                    let parent_arr = sub_array_parent_array_ref(&s.values)?;
                                    let (offset, len) = sub_array_offset_len(&s.values)?;

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
                                        return Ok(DispatchAction::Continue);
                                    }

                                    // Calculate parent index: offset + view_idx (1-indexed)
                                    let parent_idx = offset + view_idx;

                                    // Set value in parent array
                                    {
                                        let mut arr_borrow = parent_arr.borrow_mut();
                                        let typed_val = scalar_indexstore_value_for_element_type(
                                            val,
                                            arr_borrow.element_type(),
                                        );
                                        let set_result = arr_borrow.set(&[parent_idx], typed_val);
                                        if self.try_or_handle(set_result)?.is_none() {
                                            return Ok(DispatchAction::Continue);
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
                                    if let Some(func_index) = self.find_best_method_index(
                                        &["setindex!", "Base.setindex!"],
                                        &args,
                                    ) {
                                        self.start_function_call(func_index, args)?;
                                        return Ok(DispatchAction::Continue);
                                    }
                                    let type_name = self.get_type_name(&arr_val);
                                    return Err(VmError::MethodError(format!(
                                        "no method matching setindex!({})",
                                        type_name
                                    )));
                                }
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
                                    return Ok(DispatchAction::Continue);
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
                                        value::ArrayElementType::Bool => Value::Bool(val != 0.0),
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
                        },
                    }
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
