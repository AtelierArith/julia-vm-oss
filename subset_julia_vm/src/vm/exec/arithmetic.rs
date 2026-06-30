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
use super::super::intrinsics_exec::{
    apply_unary_float_op_with_heap, apply_unary_rounding_op_with_heap, pow_f64,
    value_to_f64_with_heap,
};
use super::super::stack_ops::StackOps;
use super::super::value::{RustBigFloat, Value};
use super::super::BinaryDispatchOp;
use super::super::Vm;
use super::DispatchAction;

impl<R: RngLike> Vm<R> {
    /// Execute arithmetic instructions.
    /// Returns the execution result.
    // Hot dispatch handler: front-loaded in `dispatch_instr` (Issue #5175).
    #[inline(always)]
    pub(super) fn execute_arithmetic(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
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
                // Issue #7964 Phase 2+3: Rust-level StaticArray add (inline variant is zero-alloc).
                {
                    let r = match (&a, &b) {
                        (Value::StaticArrayInline(sa), Value::StaticArrayInline(sb)) => {
                            sa.inline_add(sb)
                        }
                        (Value::StaticArray(sa), Value::StaticArray(sb)) => {
                            crate::vm::value::static_add(sa, sb)
                        }
                        _ => None,
                    };
                    if let Some(result) = r {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                }
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
                Ok(DispatchAction::Continue)
            }
            Instr::DynamicSub => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                // Issue #7964 Phase 2+3: Rust-level StaticArray sub (inline variant is zero-alloc).
                {
                    let r = match (&a, &b) {
                        (Value::StaticArrayInline(sa), Value::StaticArrayInline(sb)) => {
                            sa.inline_sub(sb)
                        }
                        (Value::StaticArray(sa), Value::StaticArray(sb)) => {
                            crate::vm::value::static_sub(sa, sb)
                        }
                        _ => None,
                    };
                    if let Some(result) = r {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                }
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
                Ok(DispatchAction::Continue)
            }
            Instr::DynamicMul => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                // Issue #7964 Phase 2+3: Rust-level StaticArray mul/matvec/matmat.
                // Inline variant path is zero-allocation (pure stack copy).
                {
                    use crate::vm::value::{static_matmat, static_matvec, static_scalar_mul};
                    let handled = match (&a, &b) {
                        // Phase 3 fast path: both inline → zero allocation.
                        (Value::StaticArrayInline(sa), Value::StaticArrayInline(sb)) => {
                            if !sa.is_vector() && sb.is_vector() {
                                sa.inline_matvec(sb)
                            } else if !sa.is_vector() && !sb.is_vector() {
                                sa.inline_matmat(sb)
                            } else {
                                None
                            }
                        }
                        (scalar, Value::StaticArrayInline(sv)) => sv.inline_scalar_mul(scalar),
                        (Value::StaticArrayInline(sv), scalar) => sv.inline_scalar_mul(scalar),
                        // Phase 2 boxed path (fallback for larger arrays).
                        (Value::StaticArray(sa), Value::StaticArray(sb)) => {
                            if !sa.is_vector() && sb.is_vector() {
                                static_matvec(sa, sb)
                            } else if !sa.is_vector() && !sb.is_vector() {
                                static_matmat(sa, sb)
                            } else {
                                None
                            }
                        }
                        (scalar, Value::StaticArray(sv)) => static_scalar_mul(scalar, sv),
                        (Value::StaticArray(sv), scalar) => static_scalar_mul(scalar, sv),
                        _ => None,
                    };
                    if let Some(result) = handled {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                }
                if let Some(result) = super::binary_both::try_matrix_diagonal_mul(self, &a, &b)? {
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }
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
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
            }
            Instr::DynamicNeg => {
                let a = self.stack.pop_value()?;
                // Primitives (including BigInt/BigFloat): use inline negation
                if matches!(
                    a,
                    Value::I8(_)
                        | Value::I16(_)
                        | Value::I32(_)
                        | Value::I64(_)
                        | Value::I128(_)
                        | Value::U8(_)
                        | Value::U16(_)
                        | Value::U32(_)
                        | Value::U64(_)
                        | Value::U128(_)
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
                Ok(DispatchAction::Continue)
            }
            Instr::DynamicPow => {
                let b = self.stack.pop_value()?;
                let a = self.stack.pop_value()?;
                if self.value_as_irrational_f64(&a).is_some()
                    || self.value_as_irrational_f64(&b).is_some()
                    || self.should_use_inline_dynamic_pow(&a, &b)
                {
                    let result = self.dynamic_pow(&a, &b);
                    match self.try_or_handle(result)? {
                        Some(result) => self.stack.push(result),
                        None => return Ok(DispatchAction::Continue),
                    }
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
                        let result = self.dynamic_pow(&values[0], &values[1]);
                        match self.try_or_handle(result)? {
                            Some(result) => self.stack.push(result),
                            None => return Ok(DispatchAction::Continue),
                        }
                    }
                }
                Ok(DispatchAction::Continue)
            }

            // === I64 arithmetic ===
            Instr::AddI64 => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_add(b)));
                Ok(DispatchAction::Continue)
            }
            Instr::SubI64 => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_sub(b)));
                Ok(DispatchAction::Continue)
            }
            Instr::MulI64 => {
                let b = self.stack.pop_i64()?;
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_mul(b)));
                Ok(DispatchAction::Continue)
            }
            Instr::ModI64 => {
                let b = self.stack.pop_i64()?;
                if b == 0 {
                    self.raise(VmError::DivisionByZero)?;
                    return Ok(DispatchAction::Continue);
                }
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a % b));
                Ok(DispatchAction::Continue)
            }
            Instr::IncI64 => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_add(1)));
                Ok(DispatchAction::Continue)
            }
            Instr::NegI64 => {
                let a = self.stack.pop_i64()?;
                self.stack.push(Value::I64(a.wrapping_neg()));
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
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
                Ok(DispatchAction::Continue)
            }
            Instr::Dup => {
                let v = self.stack.last().cloned().ok_or(VmError::StackUnderflow)?;
                self.stack.push(v);
                Ok(DispatchAction::Continue)
            }

            // === F64 basic arithmetic ===
            Instr::AddF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(a + b));
                Ok(DispatchAction::Continue)
            }
            Instr::SubF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(a - b));
                Ok(DispatchAction::Continue)
            }
            Instr::MulF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(a * b));
                Ok(DispatchAction::Continue)
            }
            Instr::DivF64 => {
                let b = self.pop_f64_or_i64()?;
                let a = self.pop_f64_or_i64()?;
                // IEEE 754: 0.0/0.0 = NaN, x/0.0 = ±Inf
                self.stack.push(Value::F64(a / b));
                Ok(DispatchAction::Continue)
            }
            Instr::NegF64 => {
                let a = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(-a));
                Ok(DispatchAction::Continue)
            }
            Instr::PowF64 => {
                let exp = self.pop_f64_or_i64()?;
                let base = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(pow_f64(base, exp)));
                Ok(DispatchAction::Continue)
            }

            // === F64 math functions ===
            Instr::SqrtF64 => {
                let value = self.stack.pop_value()?;
                let x = value_to_f64_with_heap(&value, &self.struct_heap)?;
                // Julia throws DomainError for negative real arguments
                if x < 0.0 {
                    self.raise(VmError::DomainError(format!(
                        "sqrt was called with a negative real argument ({}) but will only return a complex result if called with a complex argument. Try sqrt(Complex(x)).",
                        x
                    )))?;
                    return Ok(DispatchAction::Continue);
                }
                self.stack.push(apply_unary_float_op_with_heap(
                    value,
                    &self.struct_heap,
                    f64::sqrt,
                )?);
                Ok(DispatchAction::Continue)
            }
            Instr::FloorF64 => {
                let value = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    value,
                    &self.struct_heap,
                    f64::floor,
                    RustBigFloat::floor,
                )?);
                Ok(DispatchAction::Continue)
            }
            Instr::CeilF64 => {
                let value = self.stack.pop_value()?;
                self.stack.push(apply_unary_rounding_op_with_heap(
                    value,
                    &self.struct_heap,
                    f64::ceil,
                    RustBigFloat::ceil,
                )?);
                Ok(DispatchAction::Continue)
            }
            Instr::AbsF64 => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x.abs()));
                Ok(DispatchAction::Continue)
            }
            Instr::Abs2F64 => {
                let x = self.pop_f64_or_i64()?;
                self.stack.push(Value::F64(x * x));
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
