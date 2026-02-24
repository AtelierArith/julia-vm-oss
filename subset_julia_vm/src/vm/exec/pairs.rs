//! Base.Pairs operations for the VM.
//!
//! This module handles Pairs instructions for kwargs:
//! - NewPairs: Create a Pairs from stack values
//! - PairsGetBySymbol: Get element by symbol name
//! - PairsLength: Get length
//! - PairsKeys: Get keys as tuple of symbols
//! - PairsValues: Get values as NamedTuple

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{PairsValue, TupleValue, Value};
use super::super::Vm;

/// Result of executing a Pairs instruction.
pub(super) enum PairsResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute Pairs instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_pairs(&mut self, instr: &Instr) -> Result<PairsResult, VmError> {
        match instr {
            Instr::NewPairs(names) => {
                let mut values = Vec::with_capacity(names.len());
                for _ in 0..names.len() {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();
                let pairs = match self.try_or_handle(PairsValue::new(names.clone(), values))? {
                    Some(p) => p,
                    None => return Ok(PairsResult::Continue),
                };
                self.stack.push(Value::Pairs(pairs));
                Ok(PairsResult::Handled)
            }

            Instr::PairsGetBySymbol => {
                // Pop symbol from stack
                let symbol_name = match self.stack.pop_value()? {
                    Value::Symbol(s) => s.as_str().to_string(),
                    other => {
                        // INTERNAL: compiler always emits a Symbol before PairsGetBySymbol.
                        return Err(VmError::InternalError(format!(
                            "PairsGetBySymbol: expected Symbol on stack, got {:?}",
                            other
                        )))
                    }
                };
                // Pop pairs from stack
                let pairs = match self.stack.pop_value()? {
                    Value::Pairs(p) => p,
                    other => {
                        // INTERNAL: compiler always emits a Pairs value before PairsGetBySymbol.
                        return Err(VmError::InternalError(format!(
                            "PairsGetBySymbol: expected Pairs on stack, got {:?}",
                            other
                        )))
                    }
                };
                // Get field by name
                let value = match self.try_or_handle(pairs.get_by_symbol(&symbol_name).cloned())? {
                    Some(v) => v,
                    None => return Ok(PairsResult::Continue),
                };
                self.stack.push(value);
                Ok(PairsResult::Handled)
            }

            Instr::PairsLength => {
                let pairs = match self.stack.pop_value()? {
                    Value::Pairs(p) => p,
                    other => {
                        // INTERNAL: compiler always emits a Pairs value before PairsLength.
                        return Err(VmError::InternalError(format!(
                            "PairsLength: expected Pairs on stack, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(Value::I64(pairs.len() as i64));
                Ok(PairsResult::Handled)
            }

            Instr::PairsKeys => {
                let pairs = match self.stack.pop_value()? {
                    Value::Pairs(p) => p,
                    other => {
                        // INTERNAL: compiler always emits a Pairs value before PairsKeys.
                        return Err(VmError::InternalError(format!(
                            "PairsKeys: expected Pairs on stack, got {:?}",
                            other
                        )))
                    }
                };
                // Return keys as a tuple of symbols
                let keys: Vec<Value> = pairs.keys().into_iter().map(Value::Symbol).collect();
                self.stack.push(Value::Tuple(TupleValue { elements: keys }));
                Ok(PairsResult::Handled)
            }

            Instr::PairsValues => {
                let pairs = match self.stack.pop_value()? {
                    Value::Pairs(p) => p,
                    other => {
                        // INTERNAL: compiler always emits a Pairs value before PairsValues.
                        return Err(VmError::InternalError(format!(
                            "PairsValues: expected Pairs on stack, got {:?}",
                            other
                        )))
                    }
                };
                // Return values as the underlying NamedTuple
                self.stack.push(Value::NamedTuple(pairs.data));
                Ok(PairsResult::Handled)
            }

            _ => Ok(PairsResult::NotHandled),
        }
    }
}
