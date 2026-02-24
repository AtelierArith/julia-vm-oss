//! Arithmetic operations for the VM.
//!
//! This module handles arithmetic instructions including:
//! - Dynamic arithmetic (runtime type dispatch)
//! - I64 arithmetic
//! - F64 arithmetic and math functions

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::BinaryDispatchOp;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;

/// Result of executing an arithmetic instruction.
pub(super) enum ArithmeticResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute arithmetic instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_arithmetic(
        &mut self,
        instr: &Instr,
    ) -> Result<ArithmeticResult, VmError> {
        match instr {
            // === Dynamic arithmetic operations (runtime type dispatch) ===
            //
            // For mixed-type primitive operations (e.g., Int64 + Float64), we dispatch
            // to Julia's promotion.jl path, matching official Julia behavior:
            //   +(x::Number, y::Number) → promote(x, y) → convert → same-type op
            //
            // Same-type primitives, Rational, Array, and BigInt operations
            // stay on the fast inline path. Complex goes through Julia dispatch (Issue #2422).
            Instr::DynamicAdd => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.should_use_inline_dynamic_op(&a, &b) {
                    let result = self.dynamic_add(&a, &b)?;
                    self.stack.push(result);
                } else {
                    let values = vec![a, b];
                    if let Some(func_index) = self.find_cached_binary_method_index(
                        BinaryDispatchOp::Add,
                        &["+"],
                        &values[0],
                        &values[1],
                    ) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_add(&values[0], &values[1])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DynamicSub => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.should_use_inline_dynamic_op(&a, &b) {
                    let result = self.dynamic_sub(&a, &b)?;
                    self.stack.push(result);
                } else {
                    let values = vec![a, b];
                    if let Some(func_index) = self.find_cached_binary_method_index(
                        BinaryDispatchOp::Sub,
                        &["-"],
                        &values[0],
                        &values[1],
                    ) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_sub(&values[0], &values[1])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DynamicMul => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.should_use_inline_dynamic_op(&a, &b) {
                    let result = self.dynamic_mul(&a, &b)?;
                    self.stack.push(result);
                } else {
                    let values = vec![a, b];
                    if let Some(func_index) = self.find_cached_binary_method_index(
                        BinaryDispatchOp::Mul,
                        &["*"],
                        &values[0],
                        &values[1],
                    ) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_mul(&values[0], &values[1])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DynamicDiv => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.should_use_inline_dynamic_op(&a, &b) {
                    let result = self.dynamic_div(&a, &b)?;
                    self.stack.push(result);
                } else {
                    let values = vec![a, b];
                    if let Some(func_index) = self.find_cached_binary_method_index(
                        BinaryDispatchOp::Div,
                        &["/"],
                        &values[0],
                        &values[1],
                    ) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_div(&values[0], &values[1])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DynamicMod => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.should_use_inline_dynamic_op(&a, &b) {
                    let result = self.dynamic_mod(&a, &b)?;
                    self.stack.push(result);
                } else {
                    let values = vec![a, b];
                    if let Some(func_index) = self.find_cached_binary_method_index(
                        BinaryDispatchOp::Mod,
                        &["%"],
                        &values[0],
                        &values[1],
                    ) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_mod(&values[0], &values[1])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DynamicIntDiv => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.should_use_inline_dynamic_op(&a, &b) {
                    let result = self.dynamic_int_div(&a, &b)?;
                    self.stack.push(result);
                } else {
                    let values = vec![a, b];
                    if let Some(func_index) = self.find_cached_binary_method_index(
                        BinaryDispatchOp::IntDiv,
                        &["÷", "div"],
                        &values[0],
                        &values[1],
                    ) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_int_div(&values[0], &values[1])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DynamicNeg => {
                let a = self.stack.pop_value()?;
                // Primitives (including BigInt/BigFloat): use inline negation
                if matches!(
                    a,
                    Value::I64(_)
                        | Value::F64(_)
                        | Value::F32(_)
                        | Value::F16(_)
                        | Value::Bool(_)
                        | Value::BigInt(_)
                        | Value::BigFloat(_)
                ) {
                    let result = self.dynamic_neg(&a)?;
                    self.stack.push(result);
                } else {
                    // Struct (Complex, etc.): try Julia dispatch first (Issue #2433)
                    let values = vec![a];
                    if let Some(func_index) = self.find_best_method_index(&["-"], &values) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_neg(&values[0])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DynamicPow => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.should_use_inline_dynamic_op(&a, &b) {
                    let result = self.dynamic_pow(&a, &b)?;
                    self.stack.push(result);
                } else {
                    let values = vec![a, b];
                    if let Some(func_index) = self.find_cached_binary_method_index(
                        BinaryDispatchOp::Pow,
                        &["^"],
                        &values[0],
                        &values[1],
                    ) {
                        self.start_function_call(func_index, values)?;
                    } else {
                        let result = self.dynamic_pow(&values[0], &values[1])?;
                        self.stack.push(result);
                    }
                }
                Ok(ArithmeticResult::Handled)
            }

            // === I64 arithmetic ===
            Instr::AddI64 => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a + b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::SubI64 => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a - b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::MulI64 => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a * b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::ModI64 => {
                let b = self.stack.pop_i64()?;
                if b == 0 {
                    self.raise(VmError::DivisionByZero)?;
                    return Ok(ArithmeticResult::Continue);
                }
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a % b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::IncI64 => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a + 1));
                Ok(ArithmeticResult::Handled)
            }
            Instr::NegI64 => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(-a));
                Ok(ArithmeticResult::Handled)
            }

            // === Stack duplication (related to arithmetic operand handling) ===
            Instr::DupI64 => {
                let top = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| VmError::TypeError("DupI64: stack underflow".to_string()))?;
                match top {
                    Value::I64(x) => self.stack.push(Value::I64(x)),
                    other => {
                        // INTERNAL: DupI64 is emitted only after I64 operations; mismatched type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "DupI64: expected I64, got {:?}",
                            other
                        )));
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::DupF64 => {
                let top = self
                    .stack
                    .last()
                    .cloned()
                    .ok_or_else(|| VmError::TypeError("DupF64: stack underflow".to_string()))?;
                match top {
                    Value::F64(x) => self.stack.push(Value::F64(x)),
                    other => {
                        // INTERNAL: DupF64 is emitted only after F64 operations; mismatched type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "DupF64: expected F64, got {:?}",
                            other
                        )));
                    }
                }
                Ok(ArithmeticResult::Handled)
            }
            Instr::Dup => {
                let v = self.stack.last().cloned().ok_or(VmError::StackUnderflow)?;
                self.stack.push(v);
                Ok(ArithmeticResult::Handled)
            }

            // === F64 basic arithmetic ===
            Instr::AddF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(a + b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::SubF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(a - b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::MulF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(a * b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::DivF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                // IEEE 754: 0.0/0.0 = NaN, x/0.0 = ±Inf
                self.stack.push(Value::F64(a / b));
                Ok(ArithmeticResult::Handled)
            }
            Instr::NegF64 => {
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(-a));
                Ok(ArithmeticResult::Handled)
            }
            Instr::PowF64 => {
                let exp = self.pop_f64_or_i64()?;
                let base = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(base.powf(exp)));
                Ok(ArithmeticResult::Handled)
            }

            // === F64 math functions ===
            Instr::SqrtF64 => {
                let x = self.pop_f64_or_i64()?;
                // Julia throws DomainError for negative real arguments
                if x < 0.0 {
                    self.raise(VmError::DomainError(format!(
                        "sqrt was called with a negative real argument ({}) but will only return a complex result if called with a complex argument. Try sqrt(Complex(x)).",
                        x
                    )))?;
                    return Ok(ArithmeticResult::Continue);
                }
                self.stack.push(Value::F64(x.sqrt()));
                Ok(ArithmeticResult::Handled)
            }
            Instr::FloorF64 => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.floor()));
                Ok(ArithmeticResult::Handled)
            }
            Instr::CeilF64 => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.ceil()));
                Ok(ArithmeticResult::Handled)
            }
            Instr::AbsF64 => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.abs()));
                Ok(ArithmeticResult::Handled)
            }
            Instr::Abs2F64 => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x * x));
                Ok(ArithmeticResult::Handled)
            }

            _ => Ok(ArithmeticResult::NotHandled),
        }
    }
}
