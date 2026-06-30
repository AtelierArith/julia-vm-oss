//! Array mutation instructions.
//!
//! Handles: Zero, ArrayPush, ArrayPop, ArrayPushFirst, ArrayPopFirst,
//!          ArrayInsert, ArrayDeleteAt

#![deny(clippy::unwrap_used)]
// SAFETY: i64→usize casts for insert/delete indices are VM-internal values
// that are valid array indices by construction.
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::stack_ops::StackOps;
use super::DispatchAction;
use crate::rng::RngLike;
use crate::vm::value::{
    array_wrapper_value_to_array_value, new_memory_ref, ArrayElementType, MemoryRefValue,
    MemoryValue,
};

enum ArrayMutationTarget {
    Array(ArrayRef),
    Memory(&'static str),
    Other(Value),
}

fn array_mutation_target(value: Value) -> ArrayMutationTarget {
    match try_consume_array_value(value) {
        Ok(arr) => ArrayMutationTarget::Array(arr),
        Err(Value::Memory(mem)) => {
            ArrayMutationTarget::Memory(util::value_type_name(&Value::Memory(mem)))
        }
        Err(other) => ArrayMutationTarget::Other(other),
    }
}

/// File-local alias for the shared
/// [`super::super::value::native_array_ref_from_value`] destructure helper so the
/// existing `try_consume_array_value` call sites in this file remain
/// unchanged (Issue #3908).
#[inline]
fn try_consume_array_value(value: Value) -> Result<ArrayRef, Value> {
    super::super::value::native_array_ref_from_value(value)
}

/// Push an existing Pure Julia Array wrapper carrier (`ArrayRef`) back onto
/// the operand stack. Delegates to the shared
/// [`super::super::value::native_array_ref_value`] constructor so the native-Array
/// construction lives in a single source of truth across the VM
/// (Issue #3908).
#[inline]
fn push_array_ref<R: RngLike>(vm: &mut Vm<R>, arr: ArrayRef) {
    vm.stack
        .push(super::super::value::native_array_ref_value(arr));
}

/// Wrap an `ArrayValue` (freshly constructed via a Memory-first helper) in
/// a fresh `ArrayRef` and push it onto the operand stack. Companion to
/// [`push_array_ref`] for the cases that allocate the carrier here.
#[inline]
fn push_array_value<R: RngLike>(vm: &mut Vm<R>, arr: ArrayValue) {
    push_array_ref(vm, new_array_ref(arr));
}

fn array_wrapper_vector_len_and_offset(size: &Value) -> Option<(usize, usize)> {
    let Value::Tuple(size_tuple) = size else {
        return None;
    };

    if let Some(Value::Tuple(dims_tuple)) = size_tuple.elements.first() {
        if dims_tuple.elements.len() != 1 {
            return None;
        }
        let len = match dims_tuple.elements.first() {
            Some(Value::I64(n)) if *n >= 0 => usize::try_from(*n).ok()?,
            _ => return None,
        };
        let offset = match size_tuple.elements.get(1) {
            Some(Value::I64(offset)) if *offset >= 1 => usize::try_from(*offset).ok()?,
            _ => return None,
        };
        return Some((len, offset));
    }

    if size_tuple.elements.len() != 1 {
        return None;
    }
    let len = match size_tuple.elements.first() {
        Some(Value::I64(n)) if *n >= 0 => usize::try_from(*n).ok()?,
        _ => return None,
    };
    Some((len, 1))
}

fn array_wrapper_vector_size(len: usize) -> Value {
    Value::Tuple(TupleValue::new(vec![Value::I64(len as i64)]))
}

/// Whether an append into a backing `Memory` of this element type can grow it in
/// place via `MemoryValue::push` (amortized O(1)) with semantics identical to the
/// previous exact-size realloc + `set` path (Issue #6873).
///
/// Interleaved `Complex` and array-of-struct (`Tuple` / isbits-struct / heap
/// struct-ref) layouts pack a *resolved* element — a `StructRef` item must first
/// be materialized into an inline `Struct` (or an inline `Struct` interned into
/// the heap) the way `PushElemTyped` does (Issue #5775); `push` alone would reject
/// the raw item ("Cannot push Any to Complex{Float64} array"). Those layouts keep
/// the conservative realloc path, which is correct (it widens to boxed `Any`
/// storage exactly as before). Every other element type — primitives, `Bool`,
/// `String`, `Char`, `Symbol`, and boxed `Any`/`Union`/abstract — stores the item
/// verbatim, so `push` matches `set` and the fast path is safe.
fn elem_type_supports_inplace_growth(elem_type: &ArrayElementType) -> bool {
    !matches!(
        elem_type,
        ArrayElementType::ComplexF64
            | ArrayElementType::ComplexF32
            | ArrayElementType::TupleOf(_)
            | ArrayElementType::StructInlineOf(_, _)
            | ArrayElementType::StructOf(_)
    )
}

fn array_wrapper_vector_size_with_offset(len: usize, offset: usize) -> Value {
    Value::Tuple(TupleValue::new(vec![
        Value::Tuple(TupleValue::new(vec![Value::I64(len as i64)])),
        Value::I64(offset as i64),
    ]))
}

fn array_wrapper_index_error(index: usize, len: usize) -> VmError {
    VmError::IndexOutOfBounds {
        indices: vec![index as i64],
        shape: vec![len],
    }
}

impl<R: RngLike> Vm<R> {
    fn pop_array_wrapper(
        &mut self,
        value: Value,
    ) -> Result<Result<(Value, Value), Value>, VmError> {
        let Value::StructRef(idx) = value else {
            return Ok(Err(value));
        };
        let Some(instance) = self.struct_heap.get(idx) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if !(&*instance.struct_name == "Array" || instance.struct_name.starts_with("Array{")) {
            return Ok(Err(Value::StructRef(idx)));
        }

        let Some(mem_value) = instance.values.first().cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some(size_value) = instance.values.get(1).cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some((len, offset)) = array_wrapper_vector_len_and_offset(&size_value) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if len == 0 {
            return Err(VmError::EmptyArrayPop);
        }

        let popped = match mem_value {
            Value::Memory(mem_ref) => {
                let popped = mem_ref.borrow().get(offset + len - 1)?;
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(
                        1,
                        if offset == 1 {
                            array_wrapper_vector_size(len - 1)
                        } else {
                            array_wrapper_vector_size_with_offset(len - 1, offset)
                        },
                    )?;
                }
                popped
            }
            Value::MemoryRef(memref) => {
                let popped = memref.get(len)?;
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(1, array_wrapper_vector_size(len - 1))?;
                }
                popped
            }
            other if is_native_array_value(&other) => {
                let arr_ref = native_array_value_ref(&other).ok_or_else(|| {
                    VmError::InternalError(
                        "native_array_value_ref returned None after is_some()".to_string(),
                    )
                })?;
                if offset != 1 {
                    return Ok(Err(Value::StructRef(idx)));
                }
                let popped = arr_ref.borrow_mut().pop()?;
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(
                        1,
                        Value::Tuple(TupleValue::new(vec![Value::I64((len - 1) as i64)])),
                    )?;
                }
                popped
            }
            _ => return Ok(Err(Value::StructRef(idx))),
        };

        Ok(Ok((Value::StructRef(idx), popped)))
    }

    fn pop_first_array_wrapper(
        &mut self,
        value: Value,
    ) -> Result<Result<(Value, Value), Value>, VmError> {
        let Value::StructRef(idx) = value else {
            return Ok(Err(value));
        };
        let Some(instance) = self.struct_heap.get(idx) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if !(&*instance.struct_name == "Array" || instance.struct_name.starts_with("Array{")) {
            return Ok(Err(Value::StructRef(idx)));
        }

        let Some(mem_value) = instance.values.first().cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some(size_value) = instance.values.get(1).cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some((len, offset)) = array_wrapper_vector_len_and_offset(&size_value) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if len == 0 {
            return Err(VmError::EmptyArrayPop);
        }

        let popped = match mem_value {
            Value::Memory(mem_ref) => {
                let popped = mem_ref.borrow().get(offset)?;
                let new_offset = offset + 1;
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(
                        1,
                        if new_offset == 1 {
                            array_wrapper_vector_size(len - 1)
                        } else {
                            array_wrapper_vector_size_with_offset(len - 1, new_offset)
                        },
                    )?;
                }
                popped
            }
            Value::MemoryRef(memref) => {
                let popped = memref.get(1)?;
                let new_ref = MemoryRefValue::new(memref.parent(), memref.memory_index() + 1)?;
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(0, Value::MemoryRef(Box::new(new_ref)))?;
                    instance.set_field(1, array_wrapper_vector_size(len - 1))?;
                }
                popped
            }
            other if is_native_array_value(&other) => {
                let arr_ref = native_array_value_ref(&other).ok_or_else(|| {
                    VmError::InternalError(
                        "native_array_value_ref returned None after is_some()".to_string(),
                    )
                })?;
                if offset != 1 {
                    return Ok(Err(Value::StructRef(idx)));
                }
                let popped = arr_ref.borrow_mut().pop_first()?;
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(
                        1,
                        Value::Tuple(TupleValue::new(vec![Value::I64((len - 1) as i64)])),
                    )?;
                }
                popped
            }
            _ => return Ok(Err(Value::StructRef(idx))),
        };

        Ok(Ok((Value::StructRef(idx), popped)))
    }

    pub(in crate::vm) fn push_array_wrapper(
        &mut self,
        value: Value,
        item: Value,
        typejoin: bool,
    ) -> Result<Result<Value, Value>, VmError> {
        let Value::StructRef(idx) = value else {
            return Ok(Err(value));
        };
        let Some(instance) = self.struct_heap.get(idx) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if !(&*instance.struct_name == "Array" || instance.struct_name.starts_with("Array{")) {
            return Ok(Err(Value::StructRef(idx)));
        }

        let Some(mem_value) = instance.values.first().cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some(size_value) = instance.values.get(1).cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some((len, offset)) = array_wrapper_vector_len_and_offset(&size_value) else {
            return Ok(Err(Value::StructRef(idx)));
        };

        if typejoin {
            return self.push_array_wrapper_typejoin(idx, item);
        }

        let new_len = len + 1;

        match mem_value {
            Value::Memory(mem_ref) => {
                // Fast path (Issue #6873): the wrapper owns the whole backing
                // `Memory` contiguously from the front (offset 1, logical length ==
                // Memory length). Grow it in place via the Memory's amortized
                // `push` (the underlying Vec grows geometrically) instead of
                // reallocating an exact-size buffer and copying every prior element
                // on each append. The exact-realloc-per-push path is O(n) per
                // element → O(n^2) for an n-element comprehension / `push!` loop
                // (the Issue #6846 surface-plot hot path); amortized growth is O(n).
                let owns_whole_front = offset == 1 && {
                    let m = mem_ref.borrow();
                    m.len() == len && elem_type_supports_inplace_growth(m.element_type())
                };
                if owns_whole_front {
                    mem_ref.borrow_mut().push(item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else {
                    let capacity = mem_ref.borrow().len() + 1 - offset;
                    if new_len <= capacity {
                        mem_ref.borrow_mut().set(offset + len, item)?;
                        if let Some(instance) = self.struct_heap.get_mut(idx) {
                            instance.set_field(
                                1,
                                if offset == 1 {
                                    array_wrapper_vector_size(new_len)
                                } else {
                                    array_wrapper_vector_size_with_offset(new_len, offset)
                                },
                            )?;
                        }
                    } else {
                        let mem = mem_ref.borrow();
                        let element_type = mem.element_type().clone();
                        let mut new_mem = MemoryValue::undef_typed(&element_type, new_len);
                        for linear in 0..len {
                            new_mem.set(linear + 1, mem.get(offset + linear)?)?;
                        }
                        new_mem.set(new_len, item)?;
                        let new_ref = new_memory_ref(new_mem);
                        if let Some(instance) = self.struct_heap.get_mut(idx) {
                            instance.set_field(0, Value::Memory(new_ref))?;
                            instance.set_field(1, array_wrapper_vector_size(new_len))?;
                        }
                    }
                }
            }
            Value::MemoryRef(memref) => {
                // Fast path (Issue #6873): the ref starts at the parent's first
                // element and the parent holds exactly `len` logical elements, i.e.
                // the wrapper owns the whole backing `Memory` contiguously (the
                // freshly-built comprehension / `push!` accumulation case). Grow the
                // parent in place via amortized `push` rather than reallocating an
                // exact-size `Memory` + copying every element each append (O(n^2)).
                let owns_whole_front = memref.offset == 0 && {
                    let m = memref.memory.borrow();
                    m.len() == len && elem_type_supports_inplace_growth(m.element_type())
                };
                if owns_whole_front {
                    memref.memory.borrow_mut().push(item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else if new_len <= memref.len() {
                    memref.set(new_len, item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else {
                    let parent = memref.parent();
                    let mem = parent.borrow();
                    let element_type = mem.element_type().clone();
                    let mut new_mem = MemoryValue::undef_typed(&element_type, new_len);
                    for linear in 0..len {
                        new_mem.set(linear + 1, memref.get(linear + 1)?)?;
                    }
                    new_mem.set(new_len, item)?;
                    let new_ref = new_memory_ref(new_mem);
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(
                            0,
                            Value::MemoryRef(Box::new(MemoryRefValue::first(new_ref))),
                        )?;
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                }
            }
            other if is_native_array_value(&other) => {
                let arr_ref = native_array_value_ref(&other).ok_or_else(|| {
                    VmError::InternalError(
                        "native_array_value_ref returned None after is_some()".to_string(),
                    )
                })?;
                if offset != 1 {
                    return Ok(Err(Value::StructRef(idx)));
                }
                arr_ref.borrow_mut().push(item)?;
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(
                        1,
                        Value::Tuple(TupleValue::new(vec![Value::I64(new_len as i64)])),
                    )?;
                }
            }
            _ => return Ok(Err(Value::StructRef(idx))),
        }

        Ok(Ok(Value::StructRef(idx)))
    }

    fn push_array_wrapper_typejoin(
        &mut self,
        idx: usize,
        item: Value,
    ) -> Result<Result<Value, Value>, VmError> {
        let wrapper = Value::StructRef(idx);
        let Some(mut arr) = array_wrapper_value_to_array_value(&wrapper, &self.struct_heap)? else {
            return Ok(Err(wrapper));
        };
        if arr.shape.len() != 1 {
            return Ok(Err(wrapper));
        }

        arr.push_typejoin(item)?;
        let elem_type = arr.element_type();
        let new_len = arr.element_count();
        let mut new_mem = MemoryValue::undef_typed(&elem_type, new_len);
        for linear in 0..new_len {
            new_mem.set(linear + 1, arr.get_linear(linear)?)?;
        }
        let new_ref = new_memory_ref(new_mem);
        if let Some(instance) = self.struct_heap.get_mut(idx) {
            instance.set_field(
                0,
                Value::MemoryRef(Box::new(MemoryRefValue::first(new_ref))),
            )?;
            instance.set_field(
                1,
                Value::Tuple(TupleValue::new(vec![Value::I64(new_len as i64)])),
            )?;
        }

        Ok(Ok(Value::StructRef(idx)))
    }

    fn push_first_array_wrapper(
        &mut self,
        value: Value,
        item: Value,
    ) -> Result<Result<Value, Value>, VmError> {
        let Value::StructRef(idx) = value else {
            return Ok(Err(value));
        };
        let Some(instance) = self.struct_heap.get(idx) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if !(&*instance.struct_name == "Array" || instance.struct_name.starts_with("Array{")) {
            return Ok(Err(Value::StructRef(idx)));
        }

        let Some(mem_value) = instance.values.first().cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some(size_value) = instance.values.get(1).cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some((len, offset)) = array_wrapper_vector_len_and_offset(&size_value) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let new_len = len + 1;

        match mem_value {
            Value::Memory(mem_ref) => {
                let capacity = mem_ref.borrow().len() + 1 - offset;
                if offset > 1 {
                    let new_offset = offset - 1;
                    mem_ref.borrow_mut().set(new_offset, item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(
                            1,
                            array_wrapper_vector_size_with_offset(new_len, new_offset),
                        )?;
                    }
                } else if new_len <= capacity {
                    for pos in (0..len).rev() {
                        let existing = mem_ref.borrow().get(offset + pos)?;
                        mem_ref.borrow_mut().set(offset + pos + 1, existing)?;
                    }
                    mem_ref.borrow_mut().set(offset, item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else {
                    let mem = mem_ref.borrow();
                    let element_type = mem.element_type().clone();
                    let mut new_mem = MemoryValue::undef_typed(&element_type, new_len);
                    new_mem.set(1, item)?;
                    for linear in 0..len {
                        new_mem.set(linear + 2, mem.get(offset + linear)?)?;
                    }
                    let new_ref = new_memory_ref(new_mem);
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(0, Value::Memory(new_ref))?;
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                }
            }
            Value::MemoryRef(memref) => {
                let memory_index = memref.memory_index();
                if memory_index > 1 {
                    let new_ref = MemoryRefValue::new(memref.parent(), memory_index - 1)?;
                    new_ref.set(1, item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(0, Value::MemoryRef(Box::new(new_ref)))?;
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else if new_len <= memref.len() {
                    for pos in (1..=len).rev() {
                        let existing = memref.get(pos)?;
                        memref.set(pos + 1, existing)?;
                    }
                    memref.set(1, item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else {
                    let element_type = memref.element_type();
                    let mut new_mem = MemoryValue::undef_typed(&element_type, new_len);
                    new_mem.set(1, item)?;
                    for linear in 0..len {
                        new_mem.set(linear + 2, memref.get(linear + 1)?)?;
                    }
                    let new_ref = new_memory_ref(new_mem);
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(
                            0,
                            Value::MemoryRef(Box::new(MemoryRefValue::first(new_ref))),
                        )?;
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                }
            }
            _ => return Ok(Err(Value::StructRef(idx))),
        }

        Ok(Ok(Value::StructRef(idx)))
    }

    fn insert_array_wrapper(
        &mut self,
        value: Value,
        index: usize,
        item: Value,
    ) -> Result<Result<Value, Value>, VmError> {
        let Value::StructRef(idx) = value else {
            return Ok(Err(value));
        };
        let Some(instance) = self.struct_heap.get(idx) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if !(&*instance.struct_name == "Array" || instance.struct_name.starts_with("Array{")) {
            return Ok(Err(Value::StructRef(idx)));
        }

        let Some(mem_value) = instance.values.first().cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some(size_value) = instance.values.get(1).cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some((len, offset)) = array_wrapper_vector_len_and_offset(&size_value) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if index < 1 || index > len + 1 {
            return Err(array_wrapper_index_error(index, len));
        }
        if index == 1 {
            return self.push_first_array_wrapper(Value::StructRef(idx), item);
        }
        if index == len + 1 {
            return self.push_array_wrapper(Value::StructRef(idx), item, false);
        }
        let new_len = len + 1;

        match mem_value {
            Value::Memory(mem_ref) => {
                let capacity = mem_ref.borrow().len() + 1 - offset;
                if new_len <= capacity {
                    for pos in (index..=len).rev() {
                        let existing = mem_ref.borrow().get(offset + pos - 1)?;
                        mem_ref.borrow_mut().set(offset + pos, existing)?;
                    }
                    mem_ref.borrow_mut().set(offset + index - 1, item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(
                            1,
                            if offset == 1 {
                                array_wrapper_vector_size(new_len)
                            } else {
                                array_wrapper_vector_size_with_offset(new_len, offset)
                            },
                        )?;
                    }
                } else {
                    let mem = mem_ref.borrow();
                    let element_type = mem.element_type().clone();
                    let mut new_mem = MemoryValue::undef_typed(&element_type, new_len);
                    for linear in 0..(index - 1) {
                        new_mem.set(linear + 1, mem.get(offset + linear)?)?;
                    }
                    new_mem.set(index, item)?;
                    for linear in (index - 1)..len {
                        new_mem.set(linear + 2, mem.get(offset + linear)?)?;
                    }
                    let new_ref = new_memory_ref(new_mem);
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(0, Value::Memory(new_ref))?;
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                }
            }
            Value::MemoryRef(memref) => {
                if new_len <= memref.len() {
                    for pos in (index..=len).rev() {
                        let existing = memref.get(pos)?;
                        memref.set(pos + 1, existing)?;
                    }
                    memref.set(index, item)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else {
                    let element_type = memref.element_type();
                    let mut new_mem = MemoryValue::undef_typed(&element_type, new_len);
                    for linear in 0..(index - 1) {
                        new_mem.set(linear + 1, memref.get(linear + 1)?)?;
                    }
                    new_mem.set(index, item)?;
                    for linear in (index - 1)..len {
                        new_mem.set(linear + 2, memref.get(linear + 1)?)?;
                    }
                    let new_ref = new_memory_ref(new_mem);
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(
                            0,
                            Value::MemoryRef(Box::new(MemoryRefValue::first(new_ref))),
                        )?;
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                }
            }
            _ => return Ok(Err(Value::StructRef(idx))),
        }

        Ok(Ok(Value::StructRef(idx)))
    }

    fn delete_at_array_wrapper(
        &mut self,
        value: Value,
        index: usize,
    ) -> Result<Result<Value, Value>, VmError> {
        let Value::StructRef(idx) = value else {
            return Ok(Err(value));
        };
        let Some(instance) = self.struct_heap.get(idx) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if !(&*instance.struct_name == "Array" || instance.struct_name.starts_with("Array{")) {
            return Ok(Err(Value::StructRef(idx)));
        }

        let Some(mem_value) = instance.values.first().cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some(size_value) = instance.values.get(1).cloned() else {
            return Ok(Err(Value::StructRef(idx)));
        };
        let Some((len, offset)) = array_wrapper_vector_len_and_offset(&size_value) else {
            return Ok(Err(Value::StructRef(idx)));
        };
        if index < 1 || index > len {
            return Err(array_wrapper_index_error(index, len));
        }
        let new_len = len - 1;

        match mem_value {
            Value::Memory(mem_ref) => {
                let new_offset = if index == 1 { offset + 1 } else { offset };
                if index > 1 && index < len {
                    for pos in (index + 1)..=len {
                        let existing = mem_ref.borrow().get(offset + pos - 1)?;
                        mem_ref.borrow_mut().set(offset + pos - 2, existing)?;
                    }
                }
                if let Some(instance) = self.struct_heap.get_mut(idx) {
                    instance.set_field(
                        1,
                        if new_offset == 1 {
                            array_wrapper_vector_size(new_len)
                        } else {
                            array_wrapper_vector_size_with_offset(new_len, new_offset)
                        },
                    )?;
                }
            }
            Value::MemoryRef(memref) => {
                if index == 1 {
                    let new_ref = MemoryRefValue::new(memref.parent(), memref.memory_index() + 1)?;
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(0, Value::MemoryRef(Box::new(new_ref)))?;
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                } else {
                    if index < len {
                        for pos in (index + 1)..=len {
                            let existing = memref.get(pos)?;
                            memref.set(pos - 1, existing)?;
                        }
                    }
                    if let Some(instance) = self.struct_heap.get_mut(idx) {
                        instance.set_field(1, array_wrapper_vector_size(new_len))?;
                    }
                }
            }
            _ => return Ok(Err(Value::StructRef(idx))),
        }

        Ok(Ok(Value::StructRef(idx)))
    }

    /// Execute array mutation instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not an array mutation operation.
    #[inline]
    pub(super) fn execute_array_mutate(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::Zero => {
                // zero(x) - return zero of the same type as x
                // zero(T) - return zero of type T (Issue #2181)
                let val = self.stack.pop_value()?;
                // Check if value is Complex (inline or heap reference)
                let is_complex_val = match &val {
                    Value::Struct(s) => s.is_complex(),
                    Value::StructRef(idx) => {
                        self.struct_heap.get(*idx).is_some_and(|s| s.is_complex())
                    }
                    _ => false,
                };
                if is_complex_val {
                    // Return zero Complex as heap-allocated struct
                    let complex_val = self.create_complex(self.get_complex_type_id(), 0.0, 0.0);
                    self.stack.push(complex_val);
                } else {
                    match try_consume_array_value(val) {
                        Ok(arr) => {
                            // Return zero array of same shape and type
                            let arr = arr.borrow();
                            let zero_arr = ArrayValue::memory_first_undef(
                                &arr.element_type(),
                                arr.shape.clone(),
                            );
                            push_array_value(self, zero_arr);
                        }
                        Err(other) => {
                            if let Some(arr) =
                                array_wrapper_value_to_array_value(&other, &self.struct_heap)?
                            {
                                let zero_arr = ArrayValue::memory_first_undef(
                                    &arr.element_type(),
                                    arr.shape.clone(),
                                );
                                push_array_value(self, zero_arr);
                                return Ok(DispatchAction::Continue);
                            }

                            match other {
                                // Value-based: zero(x) where x is a value
                                Value::F64(_) => self.stack.push(Value::F64(0.0)),
                                Value::F32(_) => self.stack.push(Value::F32(0.0)),
                                Value::F16(_) => self.stack.push(Value::F16(half::f16::ZERO)),
                                Value::I64(_) => self.stack.push(Value::I64(0)),
                                Value::I32(_) => self.stack.push(Value::I32(0)),
                                Value::I16(_) => self.stack.push(Value::I16(0)),
                                Value::I8(_) => self.stack.push(Value::I8(0)),
                                Value::I128(_) => self.stack.push(Value::I128(0)),
                                Value::U8(_) => self.stack.push(Value::U8(0)),
                                Value::U16(_) => self.stack.push(Value::U16(0)),
                                Value::U32(_) => self.stack.push(Value::U32(0)),
                                Value::U64(_) => self.stack.push(Value::U64(0)),
                                Value::U128(_) => self.stack.push(Value::U128(0)),
                                Value::Bool(_) => self.stack.push(Value::Bool(false)),
                                // Type-based: zero(Int64), zero(Float32), etc.
                                Value::DataType(ref jt) => {
                                    use crate::types::JuliaType;
                                    let result = match jt.as_ref() {
                                        JuliaType::Int64 => Value::I64(0),
                                        JuliaType::Int32 => Value::I32(0),
                                        JuliaType::Int16 => Value::I16(0),
                                        JuliaType::Int8 => Value::I8(0),
                                        JuliaType::Int128 => Value::I128(0),
                                        JuliaType::UInt8 => Value::U8(0),
                                        JuliaType::UInt16 => Value::U16(0),
                                        JuliaType::UInt32 => Value::U32(0),
                                        JuliaType::UInt64 => Value::U64(0),
                                        JuliaType::UInt128 => Value::U128(0),
                                        JuliaType::Float64 => Value::F64(0.0),
                                        JuliaType::Float32 => Value::F32(0.0),
                                        JuliaType::Float16 => Value::F16(half::f16::ZERO),
                                        JuliaType::Bool => Value::Bool(false),
                                        _ => Value::F64(0.0), // Fallback for unknown types
                                    };
                                    self.stack.push(result);
                                }
                                // Memory → zero array of same shape (Issue #2764)
                                Value::Memory(mem) => {
                                    let mem = mem.borrow();
                                    let zero_arr = ArrayValue::memory_first_undef(
                                        mem.element_type(),
                                        vec![mem.len()],
                                    );
                                    push_array_value(self, zero_arr);
                                }
                                _ => self.stack.push(Value::F64(0.0)), // Default to 0.0
                            }
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ArrayPush | Instr::ArrayPushTypejoin => {
                // Pop value first (as f64 for legacy Array, or keep as Value for TypedArray)
                let val = self.stack.pop_value()?;
                let arr_val = self.stack.pop_value()?;

                match try_consume_array_value(arr_val) {
                    Ok(arr) => {
                        // Push value in-place
                        let push_result = if matches!(instr, Instr::ArrayPushTypejoin) {
                            arr.borrow_mut().push_typejoin(val)
                        } else {
                            arr.borrow_mut().push(val)
                        };
                        if self.try_or_handle(push_result)?.is_none() {
                            return Ok(DispatchAction::Continue);
                        }
                        push_array_ref(self, arr);
                    }
                    Err(other) => {
                        if let Ok(array_value) = self.push_array_wrapper(
                            other.clone(),
                            val.clone(),
                            matches!(instr, Instr::ArrayPushTypejoin),
                        )? {
                            self.stack.push(array_value);
                            return Ok(DispatchAction::Continue);
                        }

                        match other {
                            Value::Memory(mem) => {
                                self.raise(VmError::MethodError(format!(
                                    "no method matching resize!({}, {})",
                                    util::value_type_name(&Value::Memory(mem)),
                                    util::value_type_name(&val)
                                )))?;
                                return Ok(DispatchAction::Continue);
                            }
                            other => {
                                let args = vec![other.clone(), val.clone()];
                                if let Some(func_index) =
                                    self.find_best_method_index(&["push!", "Base.push!"], &args)
                                {
                                    self.start_function_call(func_index, args)?;
                                    return Ok(DispatchAction::Continue);
                                }
                                // User-visible: push! on a non-array/set is a runtime type mismatch — user can trigger via Any-typed dispatch
                                return Err(VmError::TypeError(format!(
                                    "ArrayPush: expected Array or Set, got {:?}",
                                    util::value_type_name(&other)
                                )));
                            }
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ArrayPop => {
                let arr_val = self.stack.pop_value()?;

                match try_consume_array_value(arr_val) {
                    Ok(arr) => {
                        let val = {
                            let pop_result = arr.borrow_mut().pop();
                            match self.try_or_handle(pop_result)? {
                                Some(val) => val,
                                None => return Ok(DispatchAction::Continue),
                            }
                        };
                        push_array_ref(self, arr);
                        self.stack.push(val);
                    }
                    Err(other) => match other {
                        other @ Value::StructRef(_) => match self.pop_array_wrapper(other)? {
                            Ok((array_value, popped)) => {
                                self.stack.push(array_value);
                                self.stack.push(popped);
                            }
                            Err(other) => {
                                return Err(VmError::TypeError(format!(
                                    "ArrayPop: expected Array or Set, got {:?}",
                                    util::value_type_name(&other)
                                )));
                            }
                        },
                        Value::Memory(_) => {
                            self.raise(VmError::MethodError(
                                "no method matching pop!(Memory)".to_string(),
                            ))?;
                            return Ok(DispatchAction::Continue);
                        }
                        other => {
                            // User-visible: pop! on a non-array/set is a runtime type mismatch — user can trigger via Any-typed dispatch
                            return Err(VmError::TypeError(format!(
                                "ArrayPop: expected Array or Set, got {:?}",
                                util::value_type_name(&other)
                            )));
                        }
                    },
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ArrayPushFirst => {
                let val = self.stack.pop_value()?;
                let arr_val = self.stack.pop_value()?;

                match try_consume_array_value(arr_val) {
                    Ok(arr) => {
                        {
                            let mut arr = arr.borrow_mut();
                            let result = arr.push_first(val);
                            if self.try_or_handle(result)?.is_none() {
                                return Ok(DispatchAction::Continue);
                            }
                        }
                        push_array_ref(self, arr);
                    }
                    Err(Value::Memory(mem)) => {
                        self.raise(VmError::MethodError(format!(
                            "no method matching _prepend!({}, Base.HasLength, Tuple{{Float64}})",
                            util::value_type_name(&Value::Memory(mem))
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(other) => {
                        if let Ok(array_value) =
                            self.push_first_array_wrapper(other.clone(), val.clone())?
                        {
                            self.stack.push(array_value);
                            return Ok(DispatchAction::Continue);
                        }

                        // A `copy`/`collect` result is a Memory-backed `Array`
                        // wrapper (StructRef), not a native array. Fall back to the
                        // pure-Julia `pushfirst!(a::Array, item)` method, mirroring
                        // `push!` above (Issue #5721).
                        let args = vec![other.clone(), val.clone()];
                        if let Some(func_index) =
                            self.find_best_method_index(&["pushfirst!", "Base.pushfirst!"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(DispatchAction::Continue);
                        }
                        return Err(VmError::TypeError(format!(
                            "ArrayPushFirst: expected Array, got {:?}",
                            util::value_type_name(&other)
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ArrayPopFirst => {
                let arr_val = self.stack.pop_value()?;

                match array_mutation_target(arr_val) {
                    ArrayMutationTarget::Array(arr) => {
                        let val = {
                            let mut arr = arr.borrow_mut();
                            let result = arr.pop_first();
                            match self.try_or_handle(result)? {
                                Some(val) => val,
                                None => return Ok(DispatchAction::Continue),
                            }
                        };
                        push_array_ref(self, arr);
                        self.stack.push(val);
                    }
                    ArrayMutationTarget::Memory(mem_type) => {
                        self.raise(VmError::MethodError(format!(
                            "no method matching popfirst!({})",
                            mem_type
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                    ArrayMutationTarget::Other(other) => {
                        match self.pop_first_array_wrapper(other)? {
                            Ok((array_value, popped)) => {
                                self.stack.push(array_value);
                                self.stack.push(popped);
                            }
                            Err(other) => {
                                return Err(VmError::TypeError(format!(
                                    "ArrayPopFirst: expected Array, got {:?}",
                                    util::value_type_name(&other)
                                )));
                            }
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ArrayInsert => {
                let val = self.stack.pop_value()?;
                let index = self.stack.pop_i64()?;
                let arr_val = self.stack.pop_value()?;

                match try_consume_array_value(arr_val) {
                    Ok(arr) => {
                        {
                            let mut arr = arr.borrow_mut();
                            let result = arr.insert_at(index as usize, val);
                            if self.try_or_handle(result)?.is_none() {
                                return Ok(DispatchAction::Continue);
                            }
                        }
                        push_array_ref(self, arr);
                    }
                    Err(Value::Memory(mem)) => {
                        self.raise(VmError::MethodError(format!(
                            "no method matching insert!({}, Int64, Float64)",
                            util::value_type_name(&Value::Memory(mem))
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(other) => {
                        if let Ok(array_value) =
                            self.insert_array_wrapper(other.clone(), index as usize, val.clone())?
                        {
                            self.stack.push(array_value);
                            return Ok(DispatchAction::Continue);
                        }

                        // A `copy`/`collect` result is a Memory-backed `Array`
                        // wrapper (StructRef), not a native array. Fall back to the
                        // pure-Julia `insert!(a::Array, index, item)` method,
                        // mirroring `push!` above (Issue #5721).
                        let args = vec![other.clone(), Value::I64(index), val.clone()];
                        if let Some(func_index) =
                            self.find_best_method_index(&["insert!", "Base.insert!"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(DispatchAction::Continue);
                        }
                        return Err(VmError::TypeError(format!(
                            "ArrayInsert: expected Array, got {:?}",
                            util::value_type_name(&other)
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ArrayDeleteAt => {
                let index = self.stack.pop_i64()?;
                let arr_val = self.stack.pop_value()?;

                match try_consume_array_value(arr_val) {
                    Ok(arr) => {
                        {
                            let mut arr = arr.borrow_mut();
                            let result = arr.delete_at(index as usize);
                            if self.try_or_handle(result)?.is_none() {
                                return Ok(DispatchAction::Continue);
                            }
                        }
                        push_array_ref(self, arr);
                    }
                    Err(Value::Memory(mem)) => {
                        self.raise(VmError::MethodError(format!(
                            "no method matching deleteat!({}, Int64)",
                            util::value_type_name(&Value::Memory(mem))
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(other) => {
                        if let Ok(array_value) =
                            self.delete_at_array_wrapper(other.clone(), index as usize)?
                        {
                            self.stack.push(array_value);
                            return Ok(DispatchAction::Continue);
                        }

                        // A `copy`/`collect` result is a Memory-backed `Array`
                        // wrapper (StructRef), not a native array. Fall back to the
                        // pure-Julia `deleteat!(a::Array, i::Int64)` method,
                        // mirroring `push!` above (Issue #5721).
                        let args = vec![other.clone(), Value::I64(index)];
                        if let Some(func_index) =
                            self.find_best_method_index(&["deleteat!", "Base.deleteat!"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(DispatchAction::Continue);
                        }
                        return Err(VmError::TypeError(format!(
                            "ArrayDeleteAt: expected Array, got {:?}",
                            util::value_type_name(&other)
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ArrayDeleteAtIndices => {
                // deleteat!(arr, inds) where `inds` is a Vector/Range of (1-based)
                // indices (Issue #5738).
                let inds_val = self.stack.pop_value()?;
                let arr_val = self.stack.pop_value()?;

                // Collect the 1-based indices to delete from the index collection.
                let mut indices: Vec<usize> = match &inds_val {
                    Value::Range(r) => r.to_vec().into_iter().map(|f| f as usize).collect(),
                    other => {
                        let index_array = if let Some(arr) = native_array_value_ref(other) {
                            Some(arr.borrow().clone())
                        } else {
                            array_wrapper_value_to_array_value(other, &self.struct_heap)?
                        };

                        match index_array {
                            Some(arr_ref) => {
                                let count = arr_ref.element_count();
                                let mut idxs = Vec::with_capacity(count);
                                for i in 0..count {
                                    match arr_ref.get_linear(i)? {
                                        Value::I64(n) => idxs.push(n as usize),
                                        _ => {
                                            return Err(VmError::TypeError(
                                                "deleteat!: indices must be integers".to_string(),
                                            ));
                                        }
                                    }
                                }
                                idxs
                            }
                            None => {
                                return Err(VmError::TypeError(format!(
                                    "deleteat!: expected a collection of indices, got {:?}",
                                    other
                                )));
                            }
                        }
                    }
                };
                // Delete in descending order so earlier deletions don't shift the
                // indices that are deleted later. Sorting also lets the largest
                // (most likely out-of-bounds) index be validated first.
                indices.sort_unstable();
                indices.dedup();

                match try_consume_array_value(arr_val) {
                    Ok(arr) => {
                        {
                            let mut arr = arr.borrow_mut();
                            for &idx in indices.iter().rev() {
                                let result = arr.delete_at(idx);
                                if self.try_or_handle(result)?.is_none() {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                        }
                        push_array_ref(self, arr);
                    }
                    Err(Value::Memory(mem)) => {
                        self.raise(VmError::MethodError(format!(
                            "no method matching deleteat!({}, indices)",
                            util::value_type_name(&Value::Memory(mem))
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(other) => {
                        // A `copy`/`collect` result is a Memory-backed `Array`
                        // wrapper (`StructRef`), not a native array, so the
                        // multi-index `deleteat!(arr, inds)` couldn't operate on it
                        // directly. Fall back to the pure-Julia
                        // `deleteat!(a::Array, inds)` method, mirroring the scalar
                        // `ArrayDeleteAt` path (Issue #5744, follows #5721).
                        let args = vec![other.clone(), inds_val];
                        if let Some(func_index) =
                            self.find_best_method_index(&["deleteat!", "Base.deleteat!"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(DispatchAction::Continue);
                        }
                        return Err(VmError::TypeError(format!(
                            "ArrayDeleteAtIndices: expected Array, got {:?}",
                            util::value_type_name(&other)
                        )));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rng::StableRng;
    use crate::vm::value::{new_memory_ref, ArrayElementType, MemoryValue};
    use crate::vm::{Instr, Value, Vm, VmError};

    fn memory_with_i64(values: &[i64]) -> Value {
        let mut mem = MemoryValue::undef_typed(&ArrayElementType::I64, values.len());
        for (idx, value) in values.iter().enumerate() {
            assert!(mem.set(idx + 1, Value::I64(*value)).is_ok());
        }
        Value::Memory(new_memory_ref(mem))
    }

    #[test]
    fn array_push_grows_wrapper_in_place_amortized() {
        // Regression for Issue #6873: appending to an `Array{T}` wrapper must
        // grow the backing `Memory` in place (amortized O(1) via the Vec's
        // geometric growth) instead of reallocating an exact-size `Memory` and
        // copying every prior element on each push (O(n) per push, O(n^2) for an
        // n-element comprehension / `push!` loop — the Issue #6846 surface-plot
        // hot path). Observable signature: the parent `Memory` `Rc` is preserved
        // across pushes (only the inner Vec buffer may move), whereas the old
        // realloc-every-push path swapped in a fresh `Rc` each time.
        use super::array_wrapper_vector_len_and_offset;
        use crate::vm::stack_ops::StackOps;
        use crate::vm::value::{MemoryRefValue, StructInstance, TupleValue};
        use std::rc::Rc;

        let mut vm = Vm::new(Vec::new(), StableRng::new(0));

        // Empty `Vector{Float64}` wrapper: field0 = MemoryRef@first(parent),
        // field1 = (0,) (a length-1 size tuple, offset implicitly 1).
        let parent = new_memory_ref(MemoryValue::undef_typed(&ArrayElementType::F64, 0));
        let parent_ptr = Rc::as_ptr(&parent);
        let instance = StructInstance::with_name(
            0,
            "Array{Float64, 1}".to_string(),
            vec![
                Value::MemoryRef(Box::new(MemoryRefValue::first(parent))),
                Value::Tuple(TupleValue::new(vec![Value::I64(0)])),
            ],
        );
        vm.struct_heap.push(instance);
        let idx = vm.struct_heap.len() - 1;

        let n = 64usize;
        for i in 0..n {
            vm.stack.push(Value::StructRef(idx));
            vm.stack.push(Value::F64(i as f64));
            if let Err(e) = vm.execute_array_mutate(&Instr::ArrayPush) {
                panic!("ArrayPush should succeed: {e:?}");
            }
            // ArrayPush leaves the wrapper back on the stack; drop it.
            let _ = vm.stack.pop_value();
        }

        // 1. Parent `Memory` identity preserved across all pushes (in-place growth).
        let Value::MemoryRef(memref) = &vm.struct_heap[idx].values[0] else {
            panic!("field0 should still be a MemoryRef");
        };
        assert_eq!(
            Rc::as_ptr(&memref.memory),
            parent_ptr,
            "parent Memory Rc must be preserved (in-place growth, not realloc-per-push)"
        );

        // 2. Logical length tracked in the size field (offset stays 1).
        let Some((len, offset)) =
            array_wrapper_vector_len_and_offset(&vm.struct_heap[idx].values[1])
        else {
            panic!("size field should parse");
        };
        assert_eq!(len, n, "size field must reflect the new length");
        assert_eq!(offset, 1, "offset must stay 1");

        // 3. All elements landed at the right positions, in order.
        for i in 0..n {
            match memref.get(i + 1) {
                Ok(Value::F64(v)) => assert_eq!(v, i as f64, "element {i} should round-trip"),
                Ok(other) => panic!("element {i} should be F64, got {other:?}"),
                Err(e) => panic!("element {i} should be in range: {e:?}"),
            }
        }
    }

    #[test]
    fn array_push_rejects_memory_without_array_bridge() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(memory_with_i64(&[1, 2]));
        vm.stack.push(Value::I64(3));

        let err = match vm.execute_array_mutate(&Instr::ArrayPush) {
            Ok(_) => panic!("ArrayPush on Memory should be unsupported"),
            Err(err) => err,
        };

        match err {
            VmError::MethodError(msg) => {
                assert!(msg.contains("resize!"), "unexpected MethodError: {msg}");
                assert!(msg.contains("Memory"), "unexpected MethodError: {msg}");
            }
            other => panic!("expected MethodError, got {other:?}"),
        }
    }

    #[test]
    fn array_pop_rejects_memory_without_array_bridge() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(memory_with_i64(&[1, 2]));

        let err = match vm.execute_array_mutate(&Instr::ArrayPop) {
            Ok(_) => panic!("ArrayPop on Memory should be unsupported"),
            Err(err) => err,
        };

        match err {
            VmError::MethodError(msg) => {
                assert!(msg.contains("pop!"), "unexpected MethodError: {msg}");
                assert!(msg.contains("Memory"), "unexpected MethodError: {msg}");
            }
            other => panic!("expected MethodError, got {other:?}"),
        }
    }

    #[test]
    fn array_push_first_rejects_memory_with_method_error() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(memory_with_i64(&[1, 2]));
        vm.stack.push(Value::I64(0));

        let err = match vm.execute_array_mutate(&Instr::ArrayPushFirst) {
            Ok(_) => panic!("ArrayPushFirst on Memory should be unsupported"),
            Err(err) => err,
        };

        match err {
            VmError::MethodError(msg) => {
                assert!(msg.contains("_prepend!"), "unexpected MethodError: {msg}");
                assert!(msg.contains("Memory"), "unexpected MethodError: {msg}");
            }
            other => panic!("expected MethodError, got {other:?}"),
        }
    }

    #[test]
    fn array_pop_first_rejects_memory_with_method_error() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(memory_with_i64(&[1, 2]));

        let err = match vm.execute_array_mutate(&Instr::ArrayPopFirst) {
            Ok(_) => panic!("ArrayPopFirst on Memory should be unsupported"),
            Err(err) => err,
        };

        match err {
            VmError::MethodError(msg) => {
                assert!(msg.contains("popfirst!"), "unexpected MethodError: {msg}");
                assert!(msg.contains("Memory"), "unexpected MethodError: {msg}");
            }
            other => panic!("expected MethodError, got {other:?}"),
        }
    }

    #[test]
    fn array_insert_rejects_memory_with_method_error() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(memory_with_i64(&[1, 2]));
        vm.stack.push(Value::I64(1));
        vm.stack.push(Value::I64(0));

        let err = match vm.execute_array_mutate(&Instr::ArrayInsert) {
            Ok(_) => panic!("ArrayInsert on Memory should be unsupported"),
            Err(err) => err,
        };

        match err {
            VmError::MethodError(msg) => {
                assert!(msg.contains("insert!"), "unexpected MethodError: {msg}");
                assert!(msg.contains("Memory"), "unexpected MethodError: {msg}");
            }
            other => panic!("expected MethodError, got {other:?}"),
        }
    }

    #[test]
    fn array_delete_at_rejects_memory_with_method_error() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.stack.push(memory_with_i64(&[1, 2]));
        vm.stack.push(Value::I64(1));

        let err = match vm.execute_array_mutate(&Instr::ArrayDeleteAt) {
            Ok(_) => panic!("ArrayDeleteAt on Memory should be unsupported"),
            Err(err) => err,
        };

        match err {
            VmError::MethodError(msg) => {
                assert!(msg.contains("deleteat!"), "unexpected MethodError: {msg}");
                assert!(msg.contains("Memory"), "unexpected MethodError: {msg}");
            }
            other => panic!("expected MethodError, got {other:?}"),
        }
    }
}
