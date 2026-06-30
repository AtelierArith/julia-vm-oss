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
use super::DispatchAction;

impl<R: RngLike> Vm<R> {
    /// Execute comparison instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    // Hot dispatch handler: front-loaded in `dispatch_instr` (Issue #5175).
    #[inline(always)]
    pub(super) fn execute_comparison(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            // Integer comparisons - return Bool (Julia semantics)
            Instr::GtI64 => {
                self.cmp_i64(|a, b| a > b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::LtI64 => {
                self.cmp_i64(|a, b| a < b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::LeI64 => {
                self.cmp_i64(|a, b| a <= b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::GeI64 => {
                self.cmp_i64(|a, b| a >= b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::EqI64 => {
                self.cmp_i64(|a, b| a == b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::NeI64 => {
                self.cmp_i64(|a, b| a != b)?;
                Ok(DispatchAction::Continue)
            }

            // Float comparisons - return Bool (Julia semantics)
            Instr::LtF64 => {
                self.cmp_f64(|a, b| a < b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::GtF64 => {
                self.cmp_f64(|a, b| a > b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::LeF64 => {
                self.cmp_f64(|a, b| a <= b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::GeF64 => {
                self.cmp_f64(|a, b| a >= b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::EqF64 => {
                self.cmp_f64(|a, b| a == b)?;
                Ok(DispatchAction::Continue)
            }
            Instr::NeF64 => {
                self.cmp_f64(|a, b| a != b)?;
                Ok(DispatchAction::Continue)
            }

            // Struct field comparison (default == for structs without custom ==)
            Instr::EqStruct => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = self.compare_struct_fields(&left, &right);
                self.stack.push(Value::Bool(result));
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
            }
            Instr::LeStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a <= b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(DispatchAction::Continue)
            }
            Instr::GtStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a > b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(DispatchAction::Continue)
            }
            Instr::GeStr => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                let result = match (left, right) {
                    (Value::Str(a), Value::Str(b)) => a >= b,
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
                Ok(DispatchAction::Continue)
            }

            // Conditional selection
            Instr::SelectI64 => {
                let else_v = self.stack.pop_i64()?;
                let then_v = self.stack.pop_i64()?;
                let cond = self.stack.pop_condition()?;
                self.stack
                    .push(Value::I64(if cond { then_v } else { else_v }));
                Ok(DispatchAction::Continue)
            }
            Instr::SelectF64 => {
                let else_v = self.pop_f64_or_i64()?;
                let then_v = self.pop_f64_or_i64()?;
                let cond = self.stack.pop_condition()?;
                self.stack
                    .push(Value::F64(if cond { then_v } else { else_v }));
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
