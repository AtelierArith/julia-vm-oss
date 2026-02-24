//! Dict operations for the VM.
//!
//! This module handles dict instructions:
//! - NewDict: Create an empty dict
//! - NewDictTyped: Create an empty dict with type parameters
//! - NewDictWithPairs: Create a dict from key-value pairs
//! - DictSet: Set a key-value pair
//! - DictLen: Get the number of entries
//! - LoadDict, StoreDict: Load/store dict variables
//! - ReturnDict: Return dict from function

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{DictKey, DictValue, Value};
use super::super::frame::VarTypeTag;
use super::super::Vm;

/// Result of executing a dict instruction.
pub(super) enum DictResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Return with value (exit run loop)
    Return(Value),
}

impl<R: RngLike> Vm<R> {
    /// Execute dict instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_dict(&mut self, instr: &Instr) -> Result<DictResult, VmError> {
        match instr {
            Instr::NewDict => {
                self.stack.push(Value::Dict(Box::default()));
                Ok(DictResult::Handled)
            }

            Instr::NewDictTyped(key_type, value_type) => {
                self.stack.push(Value::Dict(Box::new(DictValue::with_type_params(
                    key_type.clone(),
                    value_type.clone(),
                ))));
                Ok(DictResult::Handled)
            }

            Instr::NewDictWithPairs(n) => {
                // Pop n pairs (value, key) from stack (in reverse order)
                let mut entries = Vec::with_capacity(*n);
                for _ in 0..*n {
                    let value = self.stack.pop_value()?;
                    let key_val = self.stack.pop_value()?;
                    let key = DictKey::from_value(&key_val)?;
                    entries.push((key, value));
                }
                entries.reverse(); // Restore original order
                self.stack
                    .push(Value::Dict(Box::new(DictValue::with_entries(entries))));
                Ok(DictResult::Handled)
            }

            Instr::DictSet => {
                let value = self.stack.pop_value()?;
                let key_val = self.stack.pop_value()?;
                let mut dict = match self.stack.pop_value()? {
                    Value::Dict(d) => d,
                    other => {
                        // INTERNAL: DictSet is emitted only when the compiler typed the target as Dict
                        return Err(VmError::InternalError(format!(
                            "Expected Dict, got {:?}",
                            other
                        )))
                    }
                };
                let key = DictKey::from_value(&key_val)?;
                dict.insert(key, value);
                self.stack.push(Value::Dict(dict));
                Ok(DictResult::Handled)
            }

            Instr::DictLen => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::Dict(d) => {
                        self.stack.push(Value::I64(d.len() as i64));
                    }
                    Value::Set(s) => {
                        // Runtime fallback: length on Set through Dict path (Issue #1832)
                        self.stack.push(Value::I64(s.len() as i64));
                    }
                    other => {
                        // INTERNAL: DictLen is emitted only when the compiler typed the target as Dict or Set
                        return Err(VmError::InternalError(format!(
                            "Expected Dict or Set, got {:?}",
                            crate::vm::util::value_type_name(&other)
                        )))
                    }
                }
                Ok(DictResult::Handled)
            }

            Instr::LoadDict(name) => {
                // Try to load Dict or Set value (runtime fallback for Issue #1832)
                // When delete!/pop! is called on untyped params, the compiler may emit
                // LoadDict even when the actual value is a Set.
                let val = self
                    .frames
                    .last()
                    .and_then(|frame| match self.load_slot_value_by_name(frame, name) {
                        Some(val @ Value::Dict(_)) | Some(val @ Value::Set(_)) => Some(val),
                        _ => {
                            if let Some(d) = frame.locals_dict.get(name) {
                                Some(Value::Dict(d.clone()))
                            } else if let Some(val @ Value::Set(_)) =
                                frame.locals_any.get(name).cloned()
                            {
                                Some(val)
                            } else {
                                None
                            }
                        }
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                match self.load_slot_value_by_name(frame, name) {
                                    Some(val @ Value::Dict(_)) | Some(val @ Value::Set(_)) => {
                                        Some(val)
                                    }
                                    _ => {
                                        if let Some(d) = frame.locals_dict.get(name) {
                                            Some(Value::Dict(d.clone()))
                                        } else if let Some(val @ Value::Set(_)) =
                                            frame.locals_any.get(name).cloned()
                                        {
                                            Some(val)
                                        } else {
                                            None
                                        }
                                    }
                                }
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Value::Dict(Box::default()));
                self.stack.push(val);
                Ok(DictResult::Handled)
            }

            Instr::StoreDict(name) => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::Dict(d) => {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.locals_dict.insert(name.clone(), d);
                            frame.var_types.insert(name.clone(), VarTypeTag::Dict);
                        }
                    }
                    Value::Set(_) => {
                        // Runtime fallback: store Set in locals_any (Issue #1832)
                        if let Some(frame) = self.frames.last_mut() {
                            frame.locals_any.insert(name.clone(), val);
                            frame.var_types.insert(name.clone(), VarTypeTag::Any);
                        }
                    }
                    other => {
                        // INTERNAL: StoreDict is emitted only when the compiler typed the value as Dict or Set
                        return Err(VmError::InternalError(format!(
                            "Expected Dict or Set for StoreDict, got {:?}",
                            crate::vm::util::value_type_name(&other)
                        )))
                    }
                }
                Ok(DictResult::Handled)
            }

            Instr::ReturnDict => {
                let val = self.stack.pop().unwrap_or(Value::Nothing);
                if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(val);
                    Ok(DictResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(DictResult::Return(val))
                }
            }

            _ => Ok(DictResult::NotHandled),
        }
    }
}
