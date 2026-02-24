//! Set execution - handles Set operations
//!
//! Instructions:
//! - NewSet: Create an empty set
//! - SetAdd: Add an element to a set
//! - StoreSet, LoadSet: Store/load set variables
//! - ReturnSet: Return set from function

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{DictKey, SetValue, Value};
use super::super::frame::VarTypeTag;
use super::super::Vm;

/// Result of executing a set instruction.
pub(super) enum SetResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Return with value (exit run loop)
    Return(Value),
}

impl<R: RngLike> Vm<R> {
    /// Execute set instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_set(&mut self, instr: &Instr) -> Result<SetResult, VmError> {
        match instr {
            Instr::NewSet => {
                self.stack.push(Value::Set(SetValue::new()));
                Ok(SetResult::Handled)
            }

            Instr::SetAdd => {
                // Pop element, pop set, add element, push set
                let element = self.stack.pop_value()?;
                let set_val = self.stack.pop_value()?;

                // Convert element to DictKey
                let key = DictKey::from_value(&element)?;

                match set_val {
                    Value::Set(mut s) => {
                        s.insert(key);
                        self.stack.push(Value::Set(s));
                        Ok(SetResult::Handled)
                    }
                    other => Err(VmError::TypeError(format!(
                        "expected Set for SetAdd, got {:?}",
                        crate::vm::util::value_type_name(&other)
                    ))),
                }
            }

            Instr::StoreSet(name) => {
                // Pop set and store to locals_any
                let val = self.stack.pop_value()?;
                match &val {
                    Value::Set(_) => {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.locals_any.insert(name.clone(), val);
                            frame.var_types.insert(name.clone(), VarTypeTag::Any);
                        }
                        Ok(SetResult::Handled)
                    }
                    other => Err(VmError::TypeError(format!(
                        "expected Set for StoreSet, got {:?}",
                        crate::vm::util::value_type_name(other)
                    ))),
                }
            }

            Instr::LoadSet(name) => {
                // Load set from locals_any, checking all frames
                for frame in self.frames.iter().rev() {
                    if let Some(Value::Set(s)) = frame.locals_any.get(name) {
                        self.stack.push(Value::Set(s.clone()));
                        return Ok(SetResult::Handled);
                    }
                }
                Err(VmError::TypeError(format!(
                    "Set variable '{}' not found",
                    name
                )))
            }

            Instr::ReturnSet => {
                // Pop set and return it
                let val = self.stack.pop_value()?;
                match val {
                    Value::Set(_) => Ok(SetResult::Return(val)),
                    other => Err(VmError::TypeError(format!(
                        "expected Set for ReturnSet, got {:?}",
                        crate::vm::util::value_type_name(&other)
                    ))),
                }
            }

            _ => Ok(SetResult::NotHandled),
        }
    }
}
