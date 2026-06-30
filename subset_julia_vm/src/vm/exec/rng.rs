//! Random number generation operations for the VM.
//!
//! This module handles RNG instructions including:
//! - RandF64: single random float
//! - RandArray: array of random floats
//! - RandIntArray: array of random integers

// SAFETY: i64→u64 cast for RNG seed reinterprets the bit pattern; negative seeds
// are valid and result in a different (but well-defined) seed value.
#![allow(clippy::cast_sign_loss)]
//! - RandnF64: single standard normal value
//! - RandnArray: array of standard normal values
//! - SeedGlobalRng: reseed the global RNG

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::{RngInstance, RngLike};

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{ArrayValue, Value};
use super::super::Vm;
use super::randn;
use super::DispatchAction;

impl<R: RngLike> Vm<R> {
    /// Execute RNG instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_rng(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::RandF64 => {
                self.stack.push(Value::F64(self.rng.next_f64()));
                Ok(DispatchAction::Continue)
            }

            Instr::RandArray(n) => {
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let size: usize = dims.iter().product();
                let data: Vec<f64> = (0..size).map(|_| self.rng.next_f64()).collect();
                let arr = ArrayValue::memory_first_from_f64(data, dims);
                self.push_array_value_as_wrapper(arr)?;
                Ok(DispatchAction::Continue)
            }

            Instr::RandIntArray(n) => {
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let size: usize = dims.iter().product();
                // Generate random integers in a reasonable range (0 to i64::MAX)
                let data: Vec<f64> = (0..size)
                    .map(|_| (self.rng.next_u64() as i64).abs() as f64)
                    .collect();
                let arr = ArrayValue::memory_first_from_f64(data, dims);
                self.push_array_value_as_wrapper(arr)?;
                Ok(DispatchAction::Continue)
            }

            Instr::RandnF64 => {
                // randn() - standard normal distribution using global RNG
                let val = randn(&mut self.rng);
                self.stack.push(Value::F64(val));
                Ok(DispatchAction::Continue)
            }

            Instr::RandnArray(n) => {
                // randn(dims...) - array of standard normal values using global RNG
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let size: usize = dims.iter().product();
                let data: Vec<f64> = (0..size).map(|_| randn(&mut self.rng)).collect();
                let arr = ArrayValue::memory_first_from_f64(data, dims);
                self.push_array_value_as_wrapper(arr)?;
                Ok(DispatchAction::Continue)
            }

            Instr::RngRandArrayF64(n) => {
                // rand(rng, dims...) - array of random floats from an explicit RNG.
                // Stack layout: Rng below, then n dims on top.
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let size: usize = dims.iter().product();
                    let is_global = matches!(rng, RngInstance::Global);
                    let data: Vec<f64> = (0..size)
                        .map(|_| {
                            if is_global {
                                self.rng.next_f64()
                            } else {
                                rng.next_f64()
                            }
                        })
                        .collect();
                    let arr = ArrayValue::memory_first_from_f64(data, dims);
                    self.push_array_value_as_wrapper(arr)?;
                    self.stack.push(Value::Rng(rng));
                }
                Ok(DispatchAction::Continue)
            }

            Instr::RngRandArrayI64(n) => {
                // rand(rng, Int, dims...) - array of random integers from an explicit RNG.
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let size: usize = dims.iter().product();
                    let is_global = matches!(rng, RngInstance::Global);
                    let data: Vec<f64> = (0..size)
                        .map(|_| {
                            let u = if is_global {
                                self.rng.next_u64()
                            } else {
                                rng.next_u64()
                            };
                            (u as i64).abs() as f64
                        })
                        .collect();
                    let arr = ArrayValue::memory_first_from_f64(data, dims);
                    self.push_array_value_as_wrapper(arr)?;
                    self.stack.push(Value::Rng(rng));
                }
                Ok(DispatchAction::Continue)
            }

            Instr::RngRandnArrayF64(n) => {
                // randn(rng, dims...) - array of standard normal values from an explicit RNG.
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let size: usize = dims.iter().product();
                    let is_global = matches!(rng, RngInstance::Global);
                    let data: Vec<f64> = (0..size)
                        .map(|_| {
                            if is_global {
                                randn(&mut self.rng)
                            } else {
                                randn(&mut rng)
                            }
                        })
                        .collect();
                    let arr = ArrayValue::memory_first_from_f64(data, dims);
                    self.push_array_value_as_wrapper(arr)?;
                    self.stack.push(Value::Rng(rng));
                }
                Ok(DispatchAction::Continue)
            }

            Instr::PushGlobalRng => {
                // Random.default_rng() / GLOBAL_RNG -> a handle to the VM's
                // global RNG. Drawing through it advances the SAME stream as
                // bare rand()/randn() (Issue #7230).
                self.stack.push(Value::Rng(RngInstance::Global));
                Ok(DispatchAction::Continue)
            }

            Instr::RandnArg(write_back) => {
                // Single-arg randn(x) where x is statically untyped (Issue #7231).
                match self.stack.pop().ok_or(VmError::StackUnderflow)? {
                    Value::Rng(mut rng) => {
                        let val = if matches!(rng, RngInstance::Global) {
                            randn(&mut self.rng)
                        } else {
                            randn(&mut rng)
                        };
                        self.write_back_rng_local(write_back.as_deref(), rng);
                        self.stack.push(Value::F64(val));
                    }
                    other => {
                        // A struct argument has no numeric/RNG interpretation:
                        // defer to a user/library `randn` method on its type
                        // before treating it as a dimension (Issue #7901),
                        // mirroring the builtin-defer-through-`Any` precedent
                        // (#6657 getindex/first/last, #6610 haskey/isempty,
                        // #6638 iterate). The concrete-arg case never reaches
                        // this builtin because dispatch resolves the user
                        // method at compile time.
                        if self.try_defer_rand_to_user_method(
                            &["randn", "Base.randn", "Random.randn"],
                            &other,
                        )? {
                            return Ok(DispatchAction::Continue);
                        }
                        // randn(n) - vector of n standard normal values.
                        let n = rng_value_to_dim(&other)?;
                        let data: Vec<f64> = (0..n).map(|_| randn(&mut self.rng)).collect();
                        let arr = ArrayValue::memory_first_from_f64(data, vec![n]);
                        self.push_array_value_as_wrapper(arr)?;
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::RandArg(write_back) => {
                // Single-arg rand(x) where x is statically untyped (Issue #7231).
                match self.stack.pop().ok_or(VmError::StackUnderflow)? {
                    Value::Rng(mut rng) => {
                        let val = if matches!(rng, RngInstance::Global) {
                            self.rng.next_f64()
                        } else {
                            rng.next_f64()
                        };
                        self.write_back_rng_local(write_back.as_deref(), rng);
                        self.stack.push(Value::F64(val));
                    }
                    other => {
                        // A struct argument has no numeric/RNG interpretation:
                        // defer to a user/library `rand` method on its type
                        // before treating it as a dimension (Issue #7901),
                        // mirroring the builtin-defer-through-`Any` precedent
                        // (#6657 getindex/first/last, #6610 haskey/isempty,
                        // #6638 iterate). The concrete-arg case never reaches
                        // this builtin because dispatch resolves the user
                        // method at compile time.
                        if self.try_defer_rand_to_user_method(
                            &["rand", "Base.rand", "Random.rand"],
                            &other,
                        )? {
                            return Ok(DispatchAction::Continue);
                        }
                        // rand(n) - vector of n random floats in [0, 1).
                        let n = rng_value_to_dim(&other)?;
                        let data: Vec<f64> = (0..n).map(|_| self.rng.next_f64()).collect();
                        let arr = ArrayValue::memory_first_from_f64(data, vec![n]);
                        self.push_array_value_as_wrapper(arr)?;
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::SeedGlobalRng => {
                // seed!(n) - reseed global RNG
                let seed = self.stack.pop_i64()? as u64;
                self.rng.reseed(seed);
                self.stack.push(Value::Nothing);
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    /// Single-arg `rand(x)` / `randn(x)` where `x` is statically `Any` compiles
    /// to the Rust builtin, which only interprets an RNG or a non-negative
    /// integer dimension. A struct argument has neither interpretation, so
    /// before erroring, re-dispatch to a user/library `rand`/`randn` method on
    /// the struct's type (Issue #7901). Returns `true` when such a method was
    /// found and its call was started (the caller must `return Continue`), or
    /// `false` when no method matches (the caller falls through to the
    /// dimension/error path unchanged). Only struct receivers are considered so
    /// numeric dimension arguments keep their existing behavior.
    fn try_defer_rand_to_user_method(
        &mut self,
        names: &[&str],
        value: &Value,
    ) -> Result<bool, VmError> {
        if !matches!(value, Value::Struct(_) | Value::StructRef(_)) {
            return Ok(false);
        }
        let args = vec![value.clone()];
        if let Some(func_index) = self.find_best_method_index(names, &args) {
            self.start_function_call(func_index, args)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Write the advanced RNG state back to a named local, when the explicit-RNG
    /// argument was a plain variable. Mirrors `Instr::StoreRng` so repeated
    /// `rand(rng)` / `randn(rng)` on a local keep progressing the stream. The
    /// global handle (`RngInstance::Global`) carries no state, so writing it back
    /// is harmless (the next draw still routes to `self.rng`).
    fn write_back_rng_local(&mut self, name: Option<&str>, rng: RngInstance) {
        if let Some(name) = name {
            if let Some(frame) = self.frames.last_mut() {
                frame.locals_any.insert(name.to_string(), Value::Rng(rng));
                frame
                    .var_types
                    .insert(name.to_string(), super::super::frame::VarTypeTag::Rng);
            }
        }
    }
}

/// Convert a runtime value used as the single `rand(x)` / `randn(x)` argument
/// (when it turned out NOT to be an RNG) into an array dimension count.
fn rng_value_to_dim(value: &Value) -> Result<usize, VmError> {
    match value {
        Value::I64(v) if *v >= 0 => Ok(*v as usize),
        Value::I128(v) if *v >= 0 => Ok(*v as usize),
        Value::I32(v) if *v >= 0 => Ok(*v as usize),
        Value::I16(v) if *v >= 0 => Ok(*v as usize),
        Value::I8(v) if *v >= 0 => Ok(*v as usize),
        Value::U64(v) => Ok(*v as usize),
        Value::U128(v) => Ok(*v as usize),
        Value::U32(v) => Ok(*v as usize),
        Value::U16(v) => Ok(*v as usize),
        Value::U8(v) => Ok(*v as usize),
        other => Err(VmError::TypeError(format!(
            "rand/randn expected an RNG or a non-negative integer dimension, got {:?}",
            other
        ))),
    }
}
