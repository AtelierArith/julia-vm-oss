//! String operations for the VM.
//!
//! This module handles string-related instructions including:
//! - ToString: Convert value to string using format_value
//! - StringConcat: Concatenate multiple values using format_value
//! - ConcatStrings: Concatenate multiple values using value_to_string
//! - ToStr: Convert a value to string using value_to_string

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;
use super::{format_value, value_to_string};

impl<R: RngLike> Vm<R> {
    /// Execute string operation instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_string_ops(&mut self, instr: &Instr) -> Result<Option<()>, VmError> {
        match instr {
            Instr::ToString => {
                if let Some(v) = self.stack.pop() {
                    let s = format_value(&v);
                    self.stack.push(Value::Str(s));
                }
                Ok(Some(()))
            }

            Instr::StringConcat(count) => {
                // Pop count values, convert each to string, concatenate
                let mut parts: Vec<String> = Vec::with_capacity(*count);
                for _ in 0..*count {
                    if let Some(v) = self.stack.pop() {
                        parts.push(format_value(&v));
                    }
                }
                // Reverse because we popped in reverse order
                parts.reverse();
                let result = parts.join("");
                self.stack.push(Value::Str(result));
                Ok(Some(()))
            }

            Instr::ConcatStrings(n) => {
                // Pop n values, convert to strings, concatenate
                let mut parts: Vec<String> = Vec::with_capacity(*n);
                for _ in 0..*n {
                    let val = self.stack.pop_value()?;
                    parts.push(value_to_string(&val));
                }
                // Reverse because we popped in reverse order
                parts.reverse();
                let result = parts.join("");
                self.stack.push(Value::Str(result));
                Ok(Some(()))
            }

            Instr::ToStr => {
                let val = self.stack.pop_value()?;
                self.stack.push(Value::Str(value_to_string(&val)));
                Ok(Some(()))
            }

            _ => Ok(None),
        }
    }
}
