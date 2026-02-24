//! Sleep/timing operations for the VM.
//!
//! This module handles sleep instructions:
//! - SleepF64: Sleep for a float number of seconds
//! - SleepI64: Sleep for an integer number of seconds

// SAFETY: i64→u64 cast for sleep duration is guarded by a prior check that
// rejects negative values with an error before the cast occurs.
#![allow(clippy::cast_sign_loss)]

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;

/// Result of executing a sleep instruction.
pub(super) enum SleepResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute sleep instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_sleep(&mut self, instr: &Instr) -> Result<SleepResult, VmError> {
        match instr {
            Instr::SleepF64 => {
                let secs = self.pop_f64_or_i64()?;

                // Validate: reject negative, NaN, and infinite durations
                if secs < 0.0 {
                    self.raise(VmError::DomainError(
                        "sleep() duration cannot be negative".to_string(),
                    ))?;
                    return Ok(SleepResult::Continue);
                }
                if !secs.is_finite() {
                    self.raise(VmError::DomainError(
                        "sleep() duration must be finite".to_string(),
                    ))?;
                    return Ok(SleepResult::Continue);
                }

                // Sleep for the specified duration
                let duration = std::time::Duration::from_secs_f64(secs);
                std::thread::sleep(duration);

                // Push nothing (Julia's sleep() returns nothing)
                self.stack.push(Value::Nothing);
                Ok(SleepResult::Handled)
            }

            Instr::SleepI64 => {
                let secs = self.stack.pop_i64()?;

                // Validate: reject negative durations
                if secs < 0 {
                    self.raise(VmError::DomainError(
                        "sleep() duration cannot be negative".to_string(),
                    ))?;
                    return Ok(SleepResult::Continue);
                }

                // Sleep for the specified duration
                let duration = std::time::Duration::from_secs(secs as u64);
                std::thread::sleep(duration);

                // Push nothing
                self.stack.push(Value::Nothing);
                Ok(SleepResult::Handled)
            }

            _ => Ok(SleepResult::NotHandled),
        }
    }
}
