//! Tuple operations for the VM.
//!
//! This module handles tuple instructions:
//! - NewTuple: Create a tuple from stack values
//! - LoadTuple, StoreTuple: Load/store tuple variables
//! - TupleGet: Get element by index

// SAFETY: i64→usize casts for tuple element access are guarded by bounds checks
// that ensure the index is in [1, len].
#![allow(clippy::cast_sign_loss)]
//! - TupleUnpack: Destructure tuple into stack values
//! - TupleFirst, TupleSecond: Get first/second element
//! - ReturnTuple: Return tuple from function

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{TupleValue, Value};
use super::super::frame::VarTypeTag;
use super::super::Vm;

/// Result of executing a tuple instruction.
pub(super) enum TupleResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
    /// Return with value (exit run loop)
    Return(Value),
}

impl<R: RngLike> Vm<R> {
    /// Execute tuple instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_tuple(&mut self, instr: &Instr) -> Result<TupleResult, VmError> {
        match instr {
            Instr::NewTuple(n) => {
                let mut elements = Vec::with_capacity(*n);
                for _ in 0..*n {
                    elements.push(self.stack.pop_value()?);
                }
                elements.reverse();
                self.stack.push(Value::Tuple(TupleValue::new(elements)));
                Ok(TupleResult::Handled)
            }

            Instr::LoadTuple(name) => {
                let tuple = self
                    .frames
                    .last()
                    .and_then(|frame| match self.load_slot_value_by_name(frame, name) {
                        Some(Value::Tuple(t)) => Some(t),
                        _ => frame.locals_tuple.get(name).cloned(),
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                match self.load_slot_value_by_name(frame, name) {
                                    Some(Value::Tuple(t)) => Some(t),
                                    _ => frame.locals_tuple.get(name).cloned(),
                                }
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| TupleValue::new(Vec::new()));
                self.stack.push(Value::Tuple(tuple));
                Ok(TupleResult::Handled)
            }

            Instr::StoreTuple(name) => {
                let tuple = match self.stack.pop_value()? {
                    Value::Tuple(t) => t,
                    other => {
                        // INTERNAL: StoreTuple is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )))
                    }
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_tuple.insert(name.clone(), tuple);
                    frame.var_types.insert(name.clone(), VarTypeTag::Tuple);
                }
                Ok(TupleResult::Handled)
            }

            Instr::TupleGet => {
                let index = self.stack.pop_i64()?;
                let val = self.stack.pop_value()?;

                let value = match &val {
                    Value::Tuple(t) => match self.try_or_handle(t.get(index).cloned())? {
                        Some(v) => v,
                        None => return Ok(TupleResult::Continue),
                    },
                    // Handle Pair struct as a 2-element tuple (for Dict iteration)
                    Value::Struct(s) if s.struct_name == "Pair" && s.values.len() == 2 => {
                        // Julia uses 1-based indexing, so index 1 = first element, index 2 = second element
                        if !(1..=2).contains(&index) {
                            self.raise(VmError::IndexOutOfBounds {
                                indices: vec![index],
                                shape: vec![2],
                            })?;
                            return Ok(TupleResult::Continue);
                        }
                        s.values[(index - 1) as usize].clone()
                    }
                    // Handle StructRef - dereference it first
                    Value::StructRef(idx) => {
                        // Build result while heap is borrowed, then release borrow before
                        // calling try_or_handle (which needs &mut self).
                        let heap_idx = *idx;
                        let result = {
                            let s = self.struct_heap.get(heap_idx);
                            match s {
                                Some(s) if s.struct_name == "Pair" && s.values.len() == 2 => {
                                    if (1..=2).contains(&index) {
                                        Ok(s.values[(index - 1) as usize].clone())
                                    } else {
                                        Err(VmError::IndexOutOfBounds {
                                            indices: vec![index],
                                            shape: vec![2],
                                        })
                                    }
                                }
                                Some(s) => Err(VmError::TypeError(format!(
                                    "TupleGet: Expected Tuple or Pair, got struct {}",
                                    s.struct_name
                                ))),
                                None => Err(VmError::TypeError(format!(
                                    "Invalid struct reference: {}",
                                    heap_idx
                                ))),
                            }
                        };
                        match self.try_or_handle(result)? {
                            Some(v) => v,
                            None => return Ok(TupleResult::Continue),
                        }
                    }
                    other => {
                        // INTERNAL: TupleGet is emitted only when the compiler typed the collection as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )))
                    }
                };

                self.stack.push(value);
                Ok(TupleResult::Handled)
            }

            Instr::TupleUnpack(n) => {
                let val = self.stack.pop_value()?;
                let elements = match &val {
                    Value::Tuple(t) => t.elements.clone(),
                    // Handle Pair struct as a 2-element tuple (for Dict iteration destructuring)
                    Value::Struct(s) if s.struct_name == "Pair" && s.values.len() == 2 => {
                        s.values.clone()
                    }
                    // Handle StructRef - dereference it first
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            if s.struct_name == "Pair" && s.values.len() == 2 {
                                s.values.clone()
                            } else {
                                // INTERNAL: TupleUnpack non-Pair struct is a compiler bug; only Pair structs can be treated as 2-tuples
                                return Err(VmError::InternalError(format!(
                                    "Expected Tuple or Pair, got struct {}",
                                    s.struct_name
                                )));
                            }
                        } else {
                            // INTERNAL: TupleUnpack StructRef index is compiler-generated; invalid ref means heap corruption
                            return Err(VmError::InternalError(format!(
                                "Invalid struct reference: {}",
                                idx
                            )));
                        }
                    }
                    other => {
                        // INTERNAL: TupleUnpack is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )))
                    }
                };
                if elements.len() != *n {
                    self.raise(VmError::TupleDestructuringMismatch {
                        expected: *n,
                        got: elements.len(),
                    })?;
                    return Ok(TupleResult::Continue);
                }
                for elem in elements {
                    self.stack.push(elem);
                }
                Ok(TupleResult::Handled)
            }

            Instr::TupleFirst => {
                let tuple = match self.stack.pop_value()? {
                    Value::Tuple(t) => t,
                    other => {
                        // INTERNAL: TupleFirst is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "TupleFirst: expected Tuple, got {:?}",
                            other
                        )))
                    }
                };
                if tuple.elements.is_empty() {
                    // User-visible: first(()) throws BoundsError in Julia — must be catchable.
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![0],
                        shape: vec![0],
                    })?;
                    return Ok(TupleResult::Continue);
                }
                self.stack.push(tuple.elements[0].clone());
                Ok(TupleResult::Handled)
            }

            Instr::TupleSecond => {
                let tuple = match self.stack.pop_value()? {
                    Value::Tuple(t) => t,
                    other => {
                        // INTERNAL: TupleSecond is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "TupleSecond: expected Tuple, got {:?}",
                            other
                        )))
                    }
                };
                if tuple.elements.len() < 2 {
                    // User-visible: tuple[2] on single-element tuple throws BoundsError — catchable.
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![1],
                        shape: vec![tuple.elements.len()],
                    })?;
                    return Ok(TupleResult::Continue);
                }
                self.stack.push(tuple.elements[1].clone());
                Ok(TupleResult::Handled)
            }

            Instr::ReturnTuple => {
                let tuple = match self.stack.pop_value()? {
                    Value::Tuple(t) => t,
                    other => {
                        // INTERNAL: ReturnTuple is emitted only when the compiler typed the return value as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )))
                    }
                };
                if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(Value::Tuple(tuple));
                    Ok(TupleResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(TupleResult::Return(Value::Tuple(tuple)))
                }
            }

            _ => Ok(TupleResult::NotHandled),
        }
    }
}
