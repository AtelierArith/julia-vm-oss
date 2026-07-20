//! Helpers for the Pure Julia `Array{T,N}` wrapper runtime representation.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::array_data::ArrayData;
use super::array_element::ArrayElementType;
use super::array_value::{native_array_value_ref, ArrayValue};
use super::memory_value::{new_memory_ref, MemoryRefValue, MemoryValue};
use super::struct_instance::StructInstance;
use super::tuple::TupleValue;
use super::value_enum::Value;
use crate::error::VmError;

/// Whether a module qualifier can own the genuine `Array` wrapper. The
/// faithful wrapper is Base/Core-owned; a USER module's `Array`/`Vector`/
/// `Matrix` struct is an unrelated nominal type and must never be recognized
/// as native array storage (Issues #11388/#11395).
pub(crate) fn array_wrapper_owner_is_builtin(owner: &str) -> bool {
    owner
        .split('.')
        .all(|seg| matches!(seg, "Base" | "Core" | "Main"))
}

/// Whether `name` is the faithful `Array{T,N}` wrapper struct name (or one of
/// its `Vector`/`Matrix` aliases), ignoring type parameters. A module
/// qualifier is accepted only when it is Base/Core-owned (`Base.Array{..}`);
/// a user module's qualified `Faux.Array` is NOT a wrapper (Issue #11388).
/// Single source of truth for the byte-identical copies that used to live in
/// `compile/`, `vm/exec/`, and `vm/specialize/` (Issue #6830).
pub fn is_array_wrapper_struct_name(name: &str) -> bool {
    let base_with_module = name.split('{').next().unwrap_or(name);
    let (owner, base) = match base_with_module.rsplit_once('.') {
        Some((owner, base)) => (owner, base),
        None => ("", base_with_module),
    };
    if !owner.is_empty() && !array_wrapper_owner_is_builtin(owner) {
        return false;
    }
    matches!(base, "Array" | "Vector" | "Matrix")
}

/// Decode an `Array{T,N}` wrapper's `_size` field into its logical `(shape,
/// offset)`. The `_size` field is either a bare dims `NTuple` (offset `1`) or a
/// `(dims, offset)` pair. Returns `None` for any non-conforming value so callers
/// can fall through to other handling. Single source of truth for the
/// `Option`-returning copies that used to live in `repl/`, `vm/formatting.rs`,
/// and `vm/exec/` (Issue #6830). The `Result`-returning variants in
/// `builtins_strings.rs`/`builtins_linalg.rs` intentionally differ: they report
/// op-name-contextual `VmError`s instead of `None`.
pub fn array_wrapper_shape_and_offset(size: &Value) -> Option<(Vec<usize>, usize)> {
    let Value::Tuple(size_tuple) = size else {
        return None;
    };

    if let Some(Value::Tuple(dims_tuple)) = size_tuple.elements.first() {
        let shape = array_wrapper_shape_from_tuple(dims_tuple)?;
        let offset = match size_tuple.elements.get(1) {
            Some(Value::I64(i)) if *i >= 1 => usize::try_from(*i).ok()?,
            _ => return None,
        };
        return Some((shape, offset));
    }

    Some((array_wrapper_shape_from_tuple(size_tuple)?, 1))
}

/// Decode a dims `NTuple` into a `Vec<usize>` shape, returning `None` if any
/// element is not a non-negative `Int64`. Single source of truth for the
/// `Option`-returning copies (Issue #6830).
pub fn array_wrapper_shape_from_tuple(dims_tuple: &TupleValue) -> Option<Vec<usize>> {
    dims_tuple
        .elements
        .iter()
        .map(|dim| match dim {
            Value::I64(i) if *i >= 0 => usize::try_from(*i).ok(),
            _ => None,
        })
        .collect()
}

fn array_value_from_values(
    element_type: ArrayElementType,
    shape: Vec<usize>,
    values: Vec<Value>,
    struct_heap: &[StructInstance],
) -> Result<ArrayValue, VmError> {
    let mut arr = ArrayValue::memory_first_with_capacity(element_type.clone(), values.len());
    for value in values {
        arr.push(array_wrapper_materialize_element(
            &element_type,
            value,
            struct_heap,
        )?)?;
    }
    arr.shape = shape;
    Ok(arr)
}

fn array_wrapper_materialize_element(
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

fn array_wrapper_instance_to_array_value(
    instance: &StructInstance,
    struct_heap: &[StructInstance],
) -> Result<Option<ArrayValue>, VmError> {
    if !is_array_wrapper_struct_name(&instance.struct_name) {
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

    match storage {
        Value::MemoryRef(memref) => {
            let mut values = Vec::with_capacity(len);
            for linear in 0..len {
                values.push(memref.get(linear + 1)?);
            }
            array_value_from_values(memref.element_type(), shape, values, struct_heap).map(Some)
        }
        Value::Memory(mem_ref) => {
            let mem = mem_ref.borrow();
            let element_type = mem.element_type().clone();
            let mut values = Vec::with_capacity(len);
            for linear in 0..len {
                values.push(mem.get(offset + linear)?);
            }
            array_value_from_values(element_type, shape, values, struct_heap).map(Some)
        }
        other => {
            let Some(array_ref) = native_array_value_ref(other) else {
                return Ok(None);
            };
            let array = array_ref.borrow();
            let element_type = array.element_type();
            let mut values = Vec::with_capacity(len);
            for linear in 0..len {
                values.push(array.get_linear(offset - 1 + linear)?);
            }
            array_value_from_values(element_type, shape, values, struct_heap).map(Some)
        }
    }
}

pub fn array_wrapper_value_to_array_value(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<ArrayValue>, VmError> {
    match value {
        Value::Struct(instance) => array_wrapper_instance_to_array_value(instance, struct_heap),
        Value::StructRef(idx) => {
            let instance = struct_heap
                .get(*idx)
                .ok_or_else(|| VmError::TypeError(format!("Invalid StructRef index {}", idx)))?;
            array_wrapper_instance_to_array_value(instance, struct_heap)
        }
        _ => Ok(None),
    }
}

/// Build the `Array{T,N}` wrapper `StructInstance` (storage `MemoryRef` + size
/// `NTuple`) from an [`ArrayValue`], shared by the heap-backed and the
/// self-contained inline wrapper constructors.
fn build_array_wrapper_instance(
    arr: ArrayValue,
    array_type_id: usize,
) -> Result<StructInstance, VmError> {
    let element_type = arr.element_type();
    let len = arr.element_count();
    let ndims = arr.shape.len();
    let shape = arr
        .shape
        .iter()
        .map(|dim| {
            i64::try_from(*dim)
                .map(Value::I64)
                .map_err(|_| VmError::TypeError("Array dimension exceeds Int64 range".to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Fast path (Issue #6807): a simple, contiguous array already holds exactly
    // the wrapper's `Memory` storage, so move the `ArrayData` in instead of
    // copying it element-by-element. This is only valid when materializing a
    // fresh `MemoryValue::undef_typed(element_type)` would pick the SAME storage
    // variant — i.e. no interleaved/AoS element-type override (Complex, Tuple,
    // isbits struct, Symbol/Union/...), no shared-parent view storage, a raw
    // length matching the logical element count, and a directly-typed primitive
    // backing (not BitPackedBool, which `undef_typed` unpacks to `Bool`, nor
    // `StructRefs`, which it boxes to `Any`). Everything else takes the
    // materializing copy below, preserving byte-identical wrapper storage.
    let can_move_storage = arr.element_type_override.is_none()
        && arr.array_type_override.is_none()
        && arr.shared_parent.is_none()
        && arr.data.raw_len() == len
        && matches!(
            arr.data,
            ArrayData::F32(_)
                | ArrayData::F64(_)
                | ArrayData::I8(_)
                | ArrayData::I16(_)
                | ArrayData::I32(_)
                | ArrayData::I64(_)
                | ArrayData::U8(_)
                | ArrayData::U16(_)
                | ArrayData::U32(_)
                | ArrayData::U64(_)
                | ArrayData::Bool(_)
                | ArrayData::String(_)
                | ArrayData::Char(_)
        );
    let memory = if can_move_storage {
        MemoryValue::new(arr.data, element_type.clone(), len)
    } else {
        // The materializing copy reconstructs each logical element via
        // `arr.get_linear` and stores it with `MemoryValue::set`, which packs
        // the multi-slot layouts (interleaved Complex, AoS isbits struct incl.
        // the contiguous `StructInlineF64` — Issue #9198 S4) field-for-field.
        let mut memory = MemoryValue::undef_typed(&element_type, len);
        for linear in 0..len {
            memory.set(linear + 1, arr.get_linear(linear)?)?;
        }
        memory
    };
    let storage = Value::MemoryRef(Box::new(MemoryRefValue::first(new_memory_ref(memory))));
    let size = Value::Tuple(TupleValue::new(shape));
    let struct_name = format!("Array{{{}, {}}}", element_type.julia_type_name(), ndims);
    Ok(StructInstance::with_name(
        array_type_id,
        struct_name,
        vec![storage, size],
    ))
}

pub fn array_wrapper_value_from_array_value(
    arr: ArrayValue,
    array_type_id: usize,
    struct_heap: &mut Vec<StructInstance>,
) -> Result<Value, VmError> {
    let instance = build_array_wrapper_instance(arr, array_type_id)?;
    let idx = struct_heap.len();
    struct_heap.push(instance);
    Ok(Value::StructRef(idx))
}

/// Self-contained inline variant for the host-return boundary (Issue #6864):
/// returns `Value::Struct` (the wrapper struct held inline, its `MemoryRef`
/// backed by an `Rc<Memory>`), so the returned array survives the drop of the
/// `Vm`/`struct_heap` after `run()` — a heap `StructRef` would dangle.
pub fn array_wrapper_value_from_array_value_inline(
    arr: ArrayValue,
    array_type_id: usize,
) -> Result<Value, VmError> {
    Ok(Value::Struct(build_array_wrapper_instance(
        arr,
        array_type_id,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{new_memory_ref, ArrayElementType, MemoryRefValue, MemoryValue, TupleValue};

    #[test]
    fn complex_memoryref_wrapper_materializes_interleaved_array() -> Result<(), VmError> {
        let mut struct_heap = vec![
            StructInstance::complex(7, 1.0, 2.0),
            StructInstance::complex(7, 3.0, 4.0),
        ];
        let mut memory = MemoryValue::undef_typed(&ArrayElementType::ComplexF64, 2);
        memory.set(1, Value::Struct(struct_heap[0].clone()))?;
        memory.set(2, Value::Struct(struct_heap[1].clone()))?;
        let mem_ref = Value::MemoryRef(Box::new(MemoryRefValue::first(new_memory_ref(memory))));
        let size = Value::Tuple(TupleValue::new(vec![Value::I64(2)]));
        let wrapper_instance = StructInstance::with_name(
            0,
            "Array{Complex{Float64},1}".to_string(),
            vec![mem_ref, size],
        );
        let wrapper = Value::Struct(wrapper_instance.clone());

        let arr = array_wrapper_value_to_array_value(&wrapper, &struct_heap)?
            .ok_or_else(|| VmError::TypeError("Expected Array wrapper".into()))?;

        assert_eq!(arr.element_type(), ArrayElementType::ComplexF64);
        assert_eq!(arr.len(), 4);
        assert_eq!(arr.element_count(), 2);
        assert_eq!(
            arr.element_type_override,
            Some(ArrayElementType::ComplexF64)
        );

        let wrapper_ref_idx = struct_heap.len();
        struct_heap.push(wrapper_instance);
        let arr =
            array_wrapper_value_to_array_value(&Value::StructRef(wrapper_ref_idx), &struct_heap)?
                .ok_or_else(|| VmError::TypeError("Expected Array wrapper ref".into()))?;

        assert_eq!(arr.element_type(), ArrayElementType::ComplexF64);
        assert_eq!(arr.len(), 4);
        assert_eq!(arr.element_count(), 2);
        assert_eq!(
            arr.element_type_override,
            Some(ArrayElementType::ComplexF64)
        );

        Ok(())
    }

    #[test]
    fn is_array_wrapper_struct_name_matches_aliases_and_qualifiers() {
        // Bare names and parametric forms of the wrapper and its aliases match.
        for name in [
            "Array",
            "Array{Float64, 2}",
            "Vector",
            "Vector{Int64}",
            "Matrix",
            "Matrix{Float64}",
            "Matrix{Symbolics.Num}",
            "Array{Symbolics.Num, 2}",
            "Base.Array{Int64, 1}",
            "Base.Array{M.T, 2}",
            "Core.Array",
        ] {
            assert!(is_array_wrapper_struct_name(name), "{name} should match");
        }
        // Unrelated struct names do not match — including a USER module's
        // struct named Array/Vector/Matrix (Issues #11388/#11395).
        for name in [
            "Dict",
            "Tuple",
            "MyArrayWrapper",
            "ArrayLike",
            "Set{Int64}",
            "Faux11388.Array",
            "Faux11388.Array{Int64, 1}",
            "M.Vector{Int64}",
            "Outer.Inner.Matrix",
        ] {
            assert!(
                !is_array_wrapper_struct_name(name),
                "{name} should not match"
            );
        }
    }

    #[test]
    fn shape_and_offset_decodes_bare_dims_and_offset_pair() {
        // Bare dims tuple → offset defaults to 1.
        let bare = Value::Tuple(TupleValue::new(vec![Value::I64(2), Value::I64(3)]));
        assert_eq!(array_wrapper_shape_and_offset(&bare), Some((vec![2, 3], 1)));

        // (dims, offset) pair → explicit offset.
        let with_offset = Value::Tuple(TupleValue::new(vec![
            Value::Tuple(TupleValue::new(vec![Value::I64(4)])),
            Value::I64(5),
        ]));
        assert_eq!(
            array_wrapper_shape_and_offset(&with_offset),
            Some((vec![4], 5))
        );

        // Non-tuple, negative dim, and non-positive offset are rejected.
        assert_eq!(array_wrapper_shape_and_offset(&Value::I64(1)), None);
        let bad_dim = Value::Tuple(TupleValue::new(vec![Value::I64(-1)]));
        assert_eq!(array_wrapper_shape_and_offset(&bad_dim), None);
        let bad_offset = Value::Tuple(TupleValue::new(vec![
            Value::Tuple(TupleValue::new(vec![Value::I64(2)])),
            Value::I64(0),
        ]));
        assert_eq!(array_wrapper_shape_and_offset(&bad_offset), None);
    }

    #[test]
    fn shape_from_tuple_requires_non_negative_int64() {
        let ok = TupleValue::new(vec![Value::I64(0), Value::I64(7)]);
        assert_eq!(array_wrapper_shape_from_tuple(&ok), Some(vec![0, 7]));

        let bad = TupleValue::new(vec![Value::I64(2), Value::F64(1.5)]);
        assert_eq!(array_wrapper_shape_from_tuple(&bad), None);
    }
}
