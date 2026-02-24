//! Comparison operations for the VM.
//!
//! This module handles comparison instructions including:
//! - Integer comparisons (Gt, Lt, Le, Ge, Eq, Ne for I64)
//! - Float comparisons (Gt, Lt, Le, Ge, Eq, Ne for F64)
//! - Struct and string equality
//! - Conditional selection (Select)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute comparison instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_comparison(&mut self, instr: &Instr) -> Result<Option<()>, VmError> {
        match instr {
            // Integer comparisons - return Bool (Julia semantics)
            Instr::GtI64 => {
                self.cmp_i64(|a, b| a > b)?;
                Ok(Some(()))
            }
            Instr::LtI64 => {
                self.cmp_i64(|a, b| a < b)?;
                Ok(Some(()))
            }
            Instr::LeI64 => {
                self.cmp_i64(|a, b| a <= b)?;
                Ok(Some(()))
            }
            Instr::GeI64 => {
                self.cmp_i64(|a, b| a >= b)?;
                Ok(Some(()))
            }
            Instr::EqI64 => {
                self.cmp_i64(|a, b| a == b)?;
                Ok(Some(()))
            }
            Instr::NeI64 => {
                self.cmp_i64(|a, b| a != b)?;
                Ok(Some(()))
            }

            // Float comparisons - return Bool (Julia semantics)
            Instr::LtF64 => {
                self.cmp_f64(|a, b| a < b)?;
                Ok(Some(()))
            }
            Instr::GtF64 => {
                self.cmp_f64(|a, b| a > b)?;
                Ok(Some(()))
            }
            Instr::LeF64 => {
                self.cmp_f64(|a, b| a <= b)?;
                Ok(Some(()))
            }
            Instr::GeF64 => {
                self.cmp_f64(|a, b| a >= b)?;
                Ok(Some(()))
            }
            Instr::EqF64 => {
                self.cmp_f64(|a, b| a == b)?;
                Ok(Some(()))
            }
            Instr::NeF64 => {
                self.cmp_f64(|a, b| a != b)?;
                Ok(Some(()))
            }

            // Struct field comparison (default == for structs without custom ==)
            Instr::EqStruct => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = self.compare_struct_fields(&left, &right);
                self.stack.push(Value::Bool(result));
                Ok(Some(()))
            }

            // String equality comparison
            Instr::EqStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a == b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(Some(()))
            }

            // String ordered comparisons (lexicographic, Issue #2025)
            Instr::LtStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a < b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(Some(()))
            }
            Instr::LeStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a <= b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(Some(()))
            }
            Instr::GtStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a > b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(Some(()))
            }
            Instr::GeStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a >= b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(Some(()))
            }

            // Conditional selection
            Instr::SelectI64 => {
                let else_v = self.stack.pop_i64()?;
                let then_v = self.stack.pop_i64()?;
                let cond = self.stack.pop_condition()?;
                self.stack
                    .push(Value::I64(if cond { then_v } else { else_v }));
                Ok(Some(()))
            }
            Instr::SelectF64 => {
                let else_v = self.pop_f64_or_i64()?;
                let then_v = self.pop_f64_or_i64()?;
                let cond = self.stack.pop_condition()?;
                self.stack
                    .push(Value::F64(if cond { then_v } else { else_v }));
                Ok(Some(()))
            }

            _ => Ok(None),
        }
    }
}
