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

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{new_memory_ref, MemoryValue, Value};
use super::super::Vm;

/// Result of executing a Memory instruction.
pub(super) enum MemoryResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Return with value (exit run loop)
    Return(Value),
}

impl<R: RngLike> Vm<R> {
    /// Execute Memory instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_memory(&mut self, instr: &Instr) -> Result<MemoryResult, VmError> {
        match instr {
            Instr::NewMemory(elem_type, length) => {
                let mem = MemoryValue::undef_typed(elem_type, *length);
                self.stack.push(Value::Memory(new_memory_ref(mem)));
                Ok(MemoryResult::Handled)
            }

            Instr::NewMemoryDynamic(elem_type) => {
                let length = match self.stack.pop_value()? {
                    Value::I64(n) => n as usize,
                    Value::U64(n) => n as usize,
                    other => {
                        // INTERNAL: NewMemoryDynamic size is compiler-emitted; integer type on stack is a compiler invariant
                        return Err(VmError::InternalError(format!(
                            "Memory size must be an integer, got {:?}",
                            other
                        )));
                    }
                };
                let mem = MemoryValue::undef_typed(elem_type, length);
                self.stack.push(Value::Memory(new_memory_ref(mem)));
                Ok(MemoryResult::Handled)
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
                let value = mem.borrow().get(index).map_err(|e| {
                    VmError::TypeError(format!("BoundsError: Memory access error: {}", e))
                })?;
                self.stack.push(value);
                Ok(MemoryResult::Handled)
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
                mem.borrow_mut().set(index, value).map_err(|e| {
                    VmError::TypeError(format!("BoundsError: Memory access error: {}", e))
                })?;
                self.stack.push(Value::Memory(mem));
                Ok(MemoryResult::Handled)
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
                Ok(MemoryResult::Handled)
            }

            Instr::LoadMemory(ref name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Memory(m)) = self.load_slot_value_by_name(frame, name) {
                        self.stack.push(Value::Memory(m));
                        return Ok(MemoryResult::Handled);
                    }
                    if let Some(Value::Memory(m)) = frame.locals_any.get(name) {
                        self.stack.push(Value::Memory(m.clone()));
                        return Ok(MemoryResult::Handled);
                    }
                }
                // Search global frame
                if self.frames.len() > 1 {
                    if let Some(frame) = self.frames.first() {
                        if let Some(Value::Memory(m)) = self.load_slot_value_by_name(frame, name) {
                            self.stack.push(Value::Memory(m));
                            return Ok(MemoryResult::Handled);
                        }
                        if let Some(Value::Memory(m)) = frame.locals_any.get(name) {
                            self.stack.push(Value::Memory(m.clone()));
                            return Ok(MemoryResult::Handled);
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
                        frame
                            .locals_any
                            .insert(name.clone(), Value::Memory(m));
                    }
                }
                Ok(MemoryResult::Handled)
            }

            Instr::ReturnMemory => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::Memory(_) => Ok(MemoryResult::Return(val)),
                    other => Err(VmError::TypeError(format!(
                        "Expected Memory for return, got {:?}",
                        other
                    ))),
                }
            }

            _ => Ok(MemoryResult::NotHandled),
        }
    }
}
