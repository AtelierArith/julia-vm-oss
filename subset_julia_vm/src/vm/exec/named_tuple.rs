//! NamedTuple operations for the VM.
//!
//! This module handles named tuple instructions:
//! - NewNamedTuple: Create a named tuple from stack values
//! - LoadNamedTuple, StoreNamedTuple: Load/store named tuple variables
//! - NamedTupleGetField: Get element by field name
//! - NamedTupleGetIndex: Get element by numeric index
//! - ReturnNamedTuple: Return named tuple from function

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{NamedTupleValue, Value};
use super::super::frame::VarTypeTag;
use super::super::Vm;

/// Result of executing a named tuple instruction.
pub(super) enum NamedTupleResult {
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
    /// Execute named tuple instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_named_tuple(
        &mut self,
        instr: &Instr,
    ) -> Result<NamedTupleResult, VmError> {
        match instr {
            Instr::NewNamedTuple(names) => {
                let mut values = Vec::with_capacity(names.len());
                for _ in 0..names.len() {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();
                let named = match self.try_or_handle(NamedTupleValue::new(names.clone(), values))? {
                    Some(nt) => nt,
                    None => return Ok(NamedTupleResult::Continue),
                };
                self.stack.push(Value::NamedTuple(named));
                Ok(NamedTupleResult::Handled)
            }

            Instr::LoadNamedTuple(name) => {
                let named = self
                    .frames
                    .last()
                    .and_then(|frame| match self.load_slot_value_by_name(frame, name) {
                        Some(Value::NamedTuple(nt)) => Some(nt),
                        _ => frame.locals_named_tuple.get(name).cloned(),
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                match self.load_slot_value_by_name(frame, name) {
                                    Some(Value::NamedTuple(nt)) => Some(nt),
                                    _ => frame.locals_named_tuple.get(name).cloned(),
                                }
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| NamedTupleValue {
                        names: Vec::new(),
                        values: Vec::new(),
                    });
                self.stack.push(Value::NamedTuple(named));
                Ok(NamedTupleResult::Handled)
            }

            Instr::StoreNamedTuple(name) => {
                let named = match self.stack.pop_value()? {
                    Value::NamedTuple(nt) => nt,
                    other => {
                        // INTERNAL: StoreNamedTuple is emitted only when the compiler typed the value as NamedTuple
                        return Err(VmError::InternalError(format!(
                            "Expected NamedTuple, got {:?}",
                            other
                        )))
                    }
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_named_tuple.insert(name.clone(), named);
                    frame.var_types.insert(name.clone(), VarTypeTag::NamedTuple);
                }
                Ok(NamedTupleResult::Handled)
            }

            Instr::NamedTupleGetField(field) => {
                let named = match self.stack.pop_value()? {
                    Value::NamedTuple(nt) => nt,
                    other => {
                        // INTERNAL: NamedTupleGetField is emitted only for NamedTuple-typed values
                        return Err(VmError::InternalError(format!(
                            "Expected NamedTuple, got {:?}",
                            other
                        )))
                    }
                };
                let value = match self.try_or_handle(named.get_by_name(field).cloned())? {
                    Some(v) => v,
                    None => return Ok(NamedTupleResult::Continue),
                };
                self.stack.push(value);
                Ok(NamedTupleResult::Handled)
            }

            Instr::NamedTupleGetIndex => {
                let index = self.stack.pop_i64()?;
                let named = match self.stack.pop_value()? {
                    Value::NamedTuple(nt) => nt,
                    other => {
                        // INTERNAL: NamedTupleGetIndex is emitted only for NamedTuple-typed values
                        return Err(VmError::InternalError(format!(
                            "Expected NamedTuple, got {:?}",
                            other
                        )))
                    }
                };
                let value = match self.try_or_handle(named.get_by_index(index).cloned())? {
                    Some(v) => v,
                    None => return Ok(NamedTupleResult::Continue),
                };
                self.stack.push(value);
                Ok(NamedTupleResult::Handled)
            }

            Instr::NamedTupleGetBySymbol => {
                // Pop symbol from stack
                let symbol_name = match self.stack.pop_value()? {
                    Value::Symbol(s) => s.as_str().to_string(),
                    other => {
                        // INTERNAL: NamedTupleGetBySymbol symbol is compiler-emitted; wrong type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "Expected Symbol for NamedTuple index, got {:?}",
                            other
                        )))
                    }
                };
                // Pop named tuple from stack
                let named = match self.stack.pop_value()? {
                    Value::NamedTuple(nt) => nt,
                    other => {
                        // INTERNAL: NamedTupleGetBySymbol target is compiler-emitted; wrong type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "Expected NamedTuple, got {:?}",
                            other
                        )))
                    }
                };
                // Get field by name
                let value = match self.try_or_handle(named.get_by_name(&symbol_name).cloned())? {
                    Some(v) => v,
                    None => return Ok(NamedTupleResult::Continue),
                };
                self.stack.push(value);
                Ok(NamedTupleResult::Handled)
            }

            Instr::ReturnNamedTuple => {
                let named = match self.stack.pop_value()? {
                    Value::NamedTuple(nt) => nt,
                    other => {
                        // INTERNAL: ReturnNamedTuple is emitted only when the compiler typed the return value as NamedTuple
                        return Err(VmError::InternalError(format!(
                            "Expected NamedTuple, got {:?}",
                            other
                        )))
                    }
                };
                if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(Value::NamedTuple(named));
                    Ok(NamedTupleResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(NamedTupleResult::Return(Value::NamedTuple(named)))
                }
            }

            _ => Ok(NamedTupleResult::NotHandled),
        }
    }
}
