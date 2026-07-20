//! Memory{T} operations for the VM.
//!
//! This module handles Memory instructions:
//! - NewMemory: Create a new undef-initialized Memory{T}
//! - MemoryGet: Get element at index (1-indexed)
//! - MemorySet: Set element at index (1-indexed)

// SAFETY: i64→usize casts for Memory indices match the Value::I64 arm,
// which only fires for I64 variants whose sign is not checked here since
// negative indices would fail the subsequent bounds check.
#![allow(clippy::cast_sign_loss)]
//! - MemoryLength: Get the number of elements
//! - LoadMemory, StoreMemory: Load/store Memory variables
//! - ReturnMemory: Return Memory from function

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{new_memory_ref, ArrayElementType, MemoryValue, Value};
use super::super::Vm;
use super::array_basic::array_element_type_from_julia_type_resolved;

impl<R: RngLike> Vm<R> {
    pub(in crate::vm) fn byte_allocation_exceeds_budget(&self, bytes: usize) -> bool {
        self.memory_budget_bytes.is_some_and(|limit| bytes > limit)
    }

    /// When storing into layouts that pack struct fields, the packer needs the
    /// struct's field values. If the incoming value is a heap `Value::StructRef`,
    /// resolve it to an inline `Value::Struct` using `struct_heap` (which the
    /// bytecode-crate `MemoryValue::set` / `ArrayValue::push` cannot see). For
    /// every other element type / value shape this is an identity pass, so it is
    /// safe to call on generic array and memory mutation paths.
    pub(in crate::vm) fn resolve_struct_ref_for_array_element_type(
        &self,
        elem_type: &ArrayElementType,
        value: Value,
    ) -> Value {
        if let Value::StructRef(idx) = value {
            let needs_inline_struct = matches!(
                elem_type,
                ArrayElementType::ComplexF64
                    | ArrayElementType::ComplexF32
                    | ArrayElementType::StructInlineF64(_, _)
            );
            if needs_inline_struct {
                if let Some(instance) = self.struct_heap.get(idx) {
                    return Value::Struct(instance.clone());
                }
            }
            return Value::StructRef(idx);
        }
        value
    }

    pub(in crate::vm) fn resolve_struct_ref_for_inline_store(
        &self,
        mem: &crate::vm::value::MemoryRef,
        value: Value,
    ) -> Value {
        let elem_type = mem.borrow().element_type().clone();
        self.resolve_struct_ref_for_array_element_type(&elem_type, value)
    }

    pub(in crate::vm) fn memory_allocation_exceeds_budget(
        &self,
        elem_type: &ArrayElementType,
        length: usize,
    ) -> bool {
        if self.memory_budget_bytes.is_none() {
            return false;
        };
        let Some(bytes) = estimated_memory_allocation_bytes(elem_type, length) else {
            return true;
        };
        self.byte_allocation_exceeds_budget(bytes)
    }

    /// Execute Memory instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_memory(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::NewMemory(elem_type, length) => {
                if self.memory_allocation_exceeds_budget(elem_type, *length) {
                    self.raise(VmError::OutOfMemory)?;
                    return Ok(DispatchAction::Continue);
                }
                let mem = MemoryValue::undef_typed(elem_type, *length);
                self.stack.push(Value::Memory(new_memory_ref(mem)));
                Ok(DispatchAction::Continue)
            }

            Instr::NewMemoryDynamic(elem_type) => {
                let length = match self.stack.pop_value()? {
                    Value::I64(n) if n >= 0 => n as usize,
                    Value::I64(_n) => {
                        // Upstream (`julia 1.12.6`): `Memory{Int64}(undef, -1)` raises
                        // ArgumentError with this exact text. Previously raised as a
                        // `VmError::TypeError` carrying an `"ArgumentError: "` text
                        // prefix — the message named one class, the variant was
                        // another (Issue #11146; also fixed independently in the
                        // in-flight PR #11163 for Issue #10354).
                        self.raise(VmError::ArgumentError(
                            "invalid GenericMemory size: the number of elements is either \
                             negative or too large for system address width"
                                .to_string(),
                        ))?;
                        return Ok(DispatchAction::Continue);
                    }
                    Value::U64(n) => n as usize,
                    other => {
                        // INTERNAL: NewMemoryDynamic size is compiler-emitted; integer type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Memory size must be an integer, got {:?}",
                            other
                        )));
                    }
                };
                if self.memory_allocation_exceeds_budget(elem_type, length) {
                    self.raise(VmError::OutOfMemory)?;
                    return Ok(DispatchAction::Continue);
                }
                let mem = MemoryValue::undef_typed(elem_type, length);
                self.stack.push(Value::Memory(new_memory_ref(mem)));
                Ok(DispatchAction::Continue)
            }

            Instr::NewMemoryDynamicTyped => {
                let length = match self.stack.pop_value()? {
                    Value::I64(n) if n >= 0 => n as usize,
                    Value::I64(_n) => {
                        // Upstream (`julia 1.12.6`): `Memory{Int64}(undef, -1)` raises
                        // ArgumentError with this exact text. Previously raised as a
                        // `VmError::TypeError` carrying an `"ArgumentError: "` text
                        // prefix — the message named one class, the variant was
                        // another (Issue #11146; also fixed independently in the
                        // in-flight PR #11163 for Issue #10354).
                        self.raise(VmError::ArgumentError(
                            "invalid GenericMemory size: the number of elements is either \
                             negative or too large for system address width"
                                .to_string(),
                        ))?;
                        return Ok(DispatchAction::Continue);
                    }
                    Value::U64(n) => n as usize,
                    other => {
                        // INTERNAL: NewMemoryDynamicTyped size is compiler-emitted; integer type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Memory size must be an integer, got {:?}",
                            other
                        )));
                    }
                };
                let type_val = self.stack.pop_value()?;
                let elem_type = match type_val {
                    // Resolve a user-struct element type to a `StructOf` tag so
                    // `Memory{T}(n)` (and thus `Vector{T}(undef, n)` /
                    // `similar(Array{T}, dims)`) keeps the concrete eltype
                    // instead of widening to `Any` (Issue #7304).
                    Value::DataType(jt) => {
                        array_element_type_from_julia_type_resolved(&jt, &self.struct_defs)
                    }
                    _ => ArrayElementType::Any,
                };
                if self.memory_allocation_exceeds_budget(&elem_type, length) {
                    self.raise(VmError::OutOfMemory)?;
                    return Ok(DispatchAction::Continue);
                }
                let mem = MemoryValue::undef_typed(&elem_type, length);
                self.stack.push(Value::Memory(new_memory_ref(mem)));
                Ok(DispatchAction::Continue)
            }

            Instr::MemoryGet => {
                let index = match self.stack.pop_value()? {
                    Value::I64(i) => i as usize,
                    Value::U64(i) => i as usize,
                    other => {
                        // INTERNAL: MemoryGet index is compiler-emitted; integer type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Memory index must be an integer, got {:?}",
                            other
                        )));
                    }
                };
                let mem = match self.stack.pop_value()? {
                    Value::Memory(m) => m,
                    other => {
                        // INTERNAL: MemoryGet target is compiler-emitted; Memory type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Expected Memory, got {:?}",
                            other
                        )));
                    }
                };
                // `MemoryValue::get` already raises `IndexOutOfBounds`, which the
                // taxonomy funnel maps to `BoundsError`. Re-wrapping it in a
                // `TypeError` whose message merely *said* "BoundsError: " made
                // `typeof(caught)` a TypeError (Issue #11146).
                let value = mem.borrow().get(index)?;
                self.stack.push(value);
                Ok(DispatchAction::Continue)
            }

            Instr::MemorySet => {
                let value = self.stack.pop_value()?;
                let index = match self.stack.pop_value()? {
                    Value::I64(i) => i as usize,
                    Value::U64(i) => i as usize,
                    other => {
                        // INTERNAL: MemorySet index is compiler-emitted; integer type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Memory index must be an integer, got {:?}",
                            other
                        )));
                    }
                };
                let mem = match self.stack.pop_value()? {
                    Value::Memory(m) => m,
                    other => {
                        // INTERNAL: MemorySet target is compiler-emitted; Memory type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Expected Memory, got {:?}",
                            other
                        )));
                    }
                };
                // Issue #9198 S4: a contiguous `StructInlineF64` Memory needs the
                // struct's field values to pack; resolve a heap `StructRef` to an
                // inline `Value::Struct` here, where `struct_heap` is available
                // (`MemoryValue::set` in the bytecode crate cannot).
                let value = self.resolve_struct_ref_for_inline_store(&mem, value);
                // Same as `MemoryGet` above: propagate the real `IndexOutOfBounds`
                // (-> `BoundsError`) instead of a mislabeled `TypeError` (Issue #11146).
                mem.borrow_mut().set(index, value)?;
                self.stack.push(Value::Memory(mem));
                Ok(DispatchAction::Continue)
            }

            Instr::MemoryLength => {
                let mem = match self.stack.pop_value()? {
                    Value::Memory(m) => m,
                    other => {
                        // INTERNAL: MemoryLength target is compiler-emitted; Memory type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Expected Memory, got {:?}",
                            other
                        )));
                    }
                };
                let len = mem.borrow().len();
                self.stack.push(Value::I64(len as i64));
                Ok(DispatchAction::Continue)
            }

            Instr::LoadMemory(ref name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Memory(m)) = self.load_slot_value_by_name(frame, name) {
                        self.stack.push(Value::Memory(m));
                        return Ok(DispatchAction::Continue);
                    }
                    if let Some(Value::Memory(m)) = frame.locals_any.get(name) {
                        self.stack.push(Value::Memory(m.clone()));
                        return Ok(DispatchAction::Continue);
                    }
                }
                // Search global frame
                if self.frames.len() > 1 {
                    if let Some(frame) = self.frames.first() {
                        if let Some(Value::Memory(m)) = self.load_slot_value_by_name(frame, name) {
                            self.stack.push(Value::Memory(m));
                            return Ok(DispatchAction::Continue);
                        }
                        if let Some(Value::Memory(m)) = frame.locals_any.get(name) {
                            self.stack.push(Value::Memory(m.clone()));
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                Err(VmError::TypeError(format!(
                    "Memory variable not found: {}",
                    name
                )))
            }

            Instr::StoreMemory(name) => {
                if let Some(Value::Memory(m)) = self.stack.pop() {
                    if let Some(frame) = self.frames.last_mut() {
                        frame.locals_any.insert(name.clone(), Value::Memory(m));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ReturnMemory => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::Memory(_) => Ok(DispatchAction::Exit(val)),
                    other => Err(VmError::TypeError(format!(
                        "Expected Memory for return, got {:?}",
                        other
                    ))),
                }
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}

fn estimated_memory_allocation_bytes(elem_type: &ArrayElementType, length: usize) -> Option<usize> {
    let bytes_per_element = match elem_type {
        ArrayElementType::F64
        | ArrayElementType::I64
        | ArrayElementType::U64
        | ArrayElementType::ComplexF32 => 8,
        ArrayElementType::F32
        | ArrayElementType::I32
        | ArrayElementType::U32
        | ArrayElementType::Char => 4,
        ArrayElementType::I16 | ArrayElementType::U16 => 2,
        ArrayElementType::I8 | ArrayElementType::U8 | ArrayElementType::Bool => 1,
        ArrayElementType::ComplexF64 => 16,
        ArrayElementType::TupleOf(field_types) => field_types
            .len()
            .max(1)
            .checked_mul(std::mem::size_of::<Value>())?,
        ArrayElementType::StructInlineOf(_, field_count) => (*field_count)
            .max(1)
            .checked_mul(std::mem::size_of::<Value>())?,
        // Contiguous all-`Float64` isbits struct: `field_count` unboxed f64
        // (8 B each), not boxed `Value`s (Issue #9198 S4).
        ArrayElementType::StructInlineF64(_, field_count) => {
            (*field_count).max(1).checked_mul(8)?
        }
        _ => std::mem::size_of::<Value>(),
    };
    length.checked_mul(bytes_per_element)
}
