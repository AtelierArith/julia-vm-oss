//! Jump/control flow operations for the VM.
//!
//! This module handles jump instructions:
//! - Jump: Unconditional jump
//! - JumpIfZero: Jump if condition is zero/false
//! - JumpIfNeI64, JumpIfEqI64: Jump based on I64 equality
//! - JumpIfLtI64, JumpIfGtI64, JumpIfLeI64, JumpIfGeI64: Jump based on I64 comparison

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::Vm;

/// Result of executing a jump instruction.
pub(super) enum JumpResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled, no jump taken
    NoJump,
    /// Jump to the specified IP
    Jump(usize),
}

impl<R: RngLike> Vm<R> {
    /// Execute jump instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_jump(&mut self, instr: &Instr) -> Result<JumpResult, VmError> {
        match instr {
            Instr::Jump(target) => Ok(JumpResult::Jump(*target)),

            Instr::JumpIfZero(target) => {
                // Use explicit match instead of `?` so TypeError from a non-boolean
                // condition (e.g., `if 42`) can be caught by a surrounding try/catch.
                match self.execute_jump_if_zero(*target) {
                    Ok(Some(new_ip)) => Ok(JumpResult::Jump(new_ip)),
                    Ok(None) => Ok(JumpResult::NoJump),
                    Err(err) => {
                        self.raise(err)?;
                        // handle_error has already set self.ip to the catch handler.
                        Ok(JumpResult::NoJump)
                    }
                }
            }

            Instr::JumpIfNeI64(target) => {
                // Compare top 2 I64s, jump if not equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a != b {
                    Ok(JumpResult::Jump(*target))
                } else {
                    Ok(JumpResult::NoJump)
                }
            }

            Instr::JumpIfEqI64(target) => {
                // Compare top 2 I64s, jump if equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a == b {
                    Ok(JumpResult::Jump(*target))
                } else {
                    Ok(JumpResult::NoJump)
                }
            }

            Instr::JumpIfLtI64(target) => {
                // Compare top 2 I64s, jump if less than
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a < b {
                    Ok(JumpResult::Jump(*target))
                } else {
                    Ok(JumpResult::NoJump)
                }
            }

            Instr::JumpIfGtI64(target) => {
                // Compare top 2 I64s, jump if greater than
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a > b {
                    Ok(JumpResult::Jump(*target))
                } else {
                    Ok(JumpResult::NoJump)
                }
            }

            Instr::JumpIfLeI64(target) => {
                // Compare top 2 I64s, jump if less or equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a <= b {
                    Ok(JumpResult::Jump(*target))
                } else {
                    Ok(JumpResult::NoJump)
                }
            }

            Instr::JumpIfGeI64(target) => {
                // Compare top 2 I64s, jump if greater or equal
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                if a >= b {
                    Ok(JumpResult::Jump(*target))
                } else {
                    Ok(JumpResult::NoJump)
                }
            }

            _ => Ok(JumpResult::NotHandled),
        }
    }
}
