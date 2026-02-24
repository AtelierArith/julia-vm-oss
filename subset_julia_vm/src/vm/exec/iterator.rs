//! Iterator operations for the VM.
//!
//! This module handles iteration instructions:
//! - IterateFirst: Get first element and state from collection
//! - IterateNext: Get next element given state

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute iterator instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_iterator(&mut self, instr: &Instr) -> Result<Option<()>, VmError> {
        match instr {
            Instr::IterateFirst => {
                let coll = self.stack.pop_value()?;
                let result = self.iterate_first(&coll)?;
                self.stack.push(result);
                Ok(Some(()))
            }

            Instr::IterateNext => {
                let state = self.stack.pop_value()?;
                let coll = self.stack.pop_value()?;
                let result = self.iterate_next(&coll, &state)?;
                self.stack.push(result);
                Ok(Some(()))
            }

            _ => Ok(None),
        }
    }
}
