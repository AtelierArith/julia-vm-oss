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
use half::f16;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{ArrayData, ArrayElementType, ArrayValue, Value};
use super::super::Vm;
use super::randn;
use super::DispatchAction;
use crate::types::JuliaType;
use subset_julia_vm_bytecode::ScalarRandType;

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

            Instr::RngRandArrayTyped(sty, n) => {
                // rand(rng, T, dims...) - N-dimensional array of a concrete
                // scalar type from an explicit RNG, materialized with the
                // faithful element type instead of a Float64 backing
                // (Issue #9328). Stack layout: Rng below, then n dims on top.
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let arr = if matches!(rng, RngInstance::Global) {
                        sample_typed_array(*sty, dims, &mut self.rng)
                    } else {
                        sample_typed_array(*sty, dims, &mut rng)
                    };
                    self.push_array_value_as_wrapper(arr)?;
                    self.stack.push(Value::Rng(rng));
                }
                Ok(DispatchAction::Continue)
            }

            Instr::RandArrayTyped(sty, n) => {
                // rand(T, dims...) - N-dimensional array of a concrete scalar
                // type from the global RNG, materialized with the faithful
                // element type (Issue #9328).
                let mut dims = Vec::with_capacity(*n);
                for _ in 0..*n {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let arr = sample_typed_array(*sty, dims, &mut self.rng);
                self.push_array_value_as_wrapper(arr)?;
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
                        // rand(T) where T resolved to a concrete scalar type at
                        // runtime: scalar typed draw from the global RNG
                        // (Issue #9265). Precedes the user-method / dimension
                        // fallbacks so a `Type` value is not misread as a length.
                        if let Value::DataType(jt) = &other {
                            if let Some(sty) = scalar_rand_type_from_julia_type(jt) {
                                let val = sample_scalar_typed(sty, &mut self.rng);
                                self.stack.push(val);
                                return Ok(DispatchAction::Continue);
                            }
                        }
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

            Instr::RandScalarTyped(ty) => {
                // rand(T) - scalar draw of a concrete bits-numeric / Bool type
                // from the VM's global RNG (Issue #9265).
                let val = sample_scalar_typed(*ty, &mut self.rng);
                self.stack.push(val);
                Ok(DispatchAction::Continue)
            }

            Instr::RngRandScalarTyped(ty) => {
                // rand(rng, T) - scalar draw of a concrete bits-numeric / Bool
                // type from an explicit RNG (Issue #9265). The global handle
                // routes to the VM's own RNG so it shares the bare-rand stream
                // (Issue #7230); the advanced RNG is pushed back so the caller's
                // store_rng_back persists the state.
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let val = if matches!(rng, RngInstance::Global) {
                        sample_scalar_typed(*ty, &mut self.rng)
                    } else {
                        sample_scalar_typed(*ty, &mut rng)
                    };
                    self.stack.push(val);
                    self.stack.push(Value::Rng(rng));
                }
                Ok(DispatchAction::Continue)
            }

            Instr::RngRandArg => {
                // rand(rng, x) where x is statically untyped (Issue #9265). The
                // stack holds [Rng, x]; x may be a concrete scalar `DataType`
                // (scalar typed draw) or a dimension (length-n vector). The
                // advanced Rng is pushed back for the caller's store_rng_back.
                let x = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let mut rng = match self.stack.pop() {
                    Some(Value::Rng(rng)) => rng,
                    _ => {
                        return Err(VmError::TypeError(
                            "rand(rng, x): expected an RNG below the argument".to_string(),
                        ))
                    }
                };
                let is_global = matches!(rng, RngInstance::Global);
                if let Value::DataType(jt) = &x {
                    if let Some(sty) = scalar_rand_type_from_julia_type(jt) {
                        let val = if is_global {
                            sample_scalar_typed(sty, &mut self.rng)
                        } else {
                            sample_scalar_typed(sty, &mut rng)
                        };
                        self.stack.push(val);
                        self.stack.push(Value::Rng(rng));
                        return Ok(DispatchAction::Continue);
                    }
                }
                // Not a scalar type: treat x as a dimension -> vector of floats.
                let n = rng_value_to_dim(&x)?;
                let data: Vec<f64> = (0..n)
                    .map(|_| {
                        if is_global {
                            self.rng.next_f64()
                        } else {
                            rng.next_f64()
                        }
                    })
                    .collect();
                let arr = ArrayValue::memory_first_from_f64(data, vec![n]);
                self.push_array_value_as_wrapper(arr)?;
                self.stack.push(Value::Rng(rng));
                Ok(DispatchAction::Continue)
            }

            Instr::RandMaybeRng { argc, is_randn } => {
                // rand(a, dims...) / randn(a, dims...) where a is statically
                // untyped (Issue #9285). The stack holds a (lowest) with the
                // trailing (argc-1) args pushed above it; a may be an explicit Rng
                // (e.g. captured in a generator/comprehension body) or a leading
                // array dimension. The handler always leaves [result, extra] so the
                // caller's store_rng_back consumes `extra` uniformly.
                let argc = *argc;
                let is_randn = *is_randn;
                let mut rest = Vec::with_capacity(argc.saturating_sub(1));
                for _ in 1..argc {
                    rest.push(self.stack.pop().ok_or(VmError::StackUnderflow)?);
                }
                rest.reverse();
                let a = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                match a {
                    Value::Rng(mut rng) => {
                        let is_global = matches!(rng, RngInstance::Global);
                        // rand(rng, T) / rand(rng, T, dims...): a leading concrete
                        // scalar type. Only `rand` has typed forms; `randn` treats
                        // every trailing arg as a dimension (matching the static
                        // RngRandnArrayF64 path).
                        if !is_randn {
                            if let Some(Value::DataType(jt)) = rest.first() {
                                if let Some(sty) = scalar_rand_type_from_julia_type(jt) {
                                    let dims_rest = &rest[1..];
                                    if dims_rest.is_empty() {
                                        // rand(rng, T) - scalar typed draw.
                                        let val = if is_global {
                                            sample_scalar_typed(sty, &mut self.rng)
                                        } else {
                                            sample_scalar_typed(sty, &mut rng)
                                        };
                                        self.stack.push(val);
                                        self.stack.push(Value::Rng(rng));
                                        return Ok(DispatchAction::Continue);
                                    }
                                    // rand(rng, T, dims...) - typed array,
                                    // materialized with the faithful element type
                                    // via the same helper as the static
                                    // RngRandArrayTyped path so both agree
                                    // (Issue #9328).
                                    let dims = rng_values_to_dims(dims_rest)?;
                                    let arr = if is_global {
                                        sample_typed_array(sty, dims, &mut self.rng)
                                    } else {
                                        sample_typed_array(sty, dims, &mut rng)
                                    };
                                    self.push_array_value_as_wrapper(arr)?;
                                    self.stack.push(Value::Rng(rng));
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                        }
                        // rand(rng, dims...) / randn(rng, dims...) - N-dimensional
                        // Float64 array from the explicit RNG.
                        let dims = rng_values_to_dims(&rest)?;
                        let size: usize = dims.iter().product();
                        let data: Vec<f64> = (0..size)
                            .map(|_| {
                                if is_randn {
                                    if is_global {
                                        randn(&mut self.rng)
                                    } else {
                                        randn(&mut rng)
                                    }
                                } else if is_global {
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
                    other => {
                        // a is not an Rng: every argument is an array dimension.
                        // rand(dims...) / randn(dims...) -> N-dimensional Float64
                        // array from the global RNG. A trailing type (e.g.
                        // rand(m, T)) has no dimension reading and raises a
                        // TypeError here via rng_value_to_dim (upstream also errors,
                        // as a MethodError).
                        let mut dims = Vec::with_capacity(argc);
                        dims.push(rng_value_to_dim(&other)?);
                        dims.extend(rng_values_to_dims(&rest)?);
                        let size: usize = dims.iter().product();
                        let data: Vec<f64> = (0..size)
                            .map(|_| {
                                if is_randn {
                                    randn(&mut self.rng)
                                } else {
                                    self.rng.next_f64()
                                }
                            })
                            .collect();
                        let arr = ArrayValue::memory_first_from_f64(data, dims);
                        self.push_array_value_as_wrapper(arr)?;
                        // No RNG to persist; push a placeholder so the caller's
                        // store_rng_back / Pop keeps the stack balanced.
                        self.stack.push(Value::Nothing);
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

/// Map a runtime `DataType` value's [`JuliaType`] to its [`ScalarRandType`] for
/// a scalar typed `rand(rng, T)` / `rand(T)` draw where `T` is only known at
/// runtime (e.g. a loop variable over a tuple of types). Returns `None` for any
/// non-scalar type so the caller falls back to the dimension / error path
/// (Issue #9265).
pub(crate) fn scalar_rand_type_from_julia_type(jt: &JuliaType) -> Option<ScalarRandType> {
    Some(match jt {
        JuliaType::Int8 => ScalarRandType::I8,
        JuliaType::Int16 => ScalarRandType::I16,
        JuliaType::Int32 => ScalarRandType::I32,
        JuliaType::Int64 => ScalarRandType::I64,
        JuliaType::Int128 => ScalarRandType::I128,
        JuliaType::UInt8 => ScalarRandType::U8,
        JuliaType::UInt16 => ScalarRandType::U16,
        JuliaType::UInt32 => ScalarRandType::U32,
        JuliaType::UInt64 => ScalarRandType::U64,
        JuliaType::UInt128 => ScalarRandType::U128,
        JuliaType::Bool => ScalarRandType::Bool,
        JuliaType::Float16 => ScalarRandType::F16,
        JuliaType::Float32 => ScalarRandType::F32,
        JuliaType::Float64 => ScalarRandType::F64,
        _ => return None,
    })
}

/// Draw one scalar random `Value` of the requested concrete type from `rng`
/// (Issue #9265). Bits-integer and `Bool` types sample the full range from the
/// engine's raw 64-bit words (`Int128`/`UInt128` consume two words); float types
/// draw a value in `[0, 1)`. The exact bit stream is NOT upstream-parity for the
/// `MersenneTwister` backend (Issue #8998 keeps the dSFMT port deferred), but
/// every draw is deterministic for a fixed seed and correctly typed.
pub(crate) fn sample_scalar_typed<Rg: RngLike>(ty: ScalarRandType, rng: &mut Rg) -> Value {
    match ty {
        ScalarRandType::I8 => Value::I8(rng.next_u64() as i8),
        ScalarRandType::I16 => Value::I16(rng.next_u64() as i16),
        ScalarRandType::I32 => Value::I32(rng.next_u64() as i32),
        ScalarRandType::I64 => Value::I64(rng.next_u64() as i64),
        ScalarRandType::I128 => {
            let hi = u128::from(rng.next_u64());
            let lo = u128::from(rng.next_u64());
            Value::I128(((hi << 64) | lo) as i128)
        }
        ScalarRandType::U8 => Value::U8(rng.next_u64() as u8),
        ScalarRandType::U16 => Value::U16(rng.next_u64() as u16),
        ScalarRandType::U32 => Value::U32(rng.next_u64() as u32),
        ScalarRandType::U64 => Value::U64(rng.next_u64()),
        ScalarRandType::U128 => {
            let hi = u128::from(rng.next_u64());
            let lo = u128::from(rng.next_u64());
            Value::U128((hi << 64) | lo)
        }
        ScalarRandType::Bool => Value::Bool(rng.next_u64() & 1 == 1),
        ScalarRandType::F16 => Value::F16(sample_f16_close_open01(rng)),
        ScalarRandType::F32 => Value::F32(sample_f32_close_open01(rng)),
        ScalarRandType::F64 => Value::F64(rng.next_f64()),
    }
}

/// Draw a `Float32` uniformly in `[0, 1)` by constructing the float from random
/// mantissa bits, mirroring upstream Julia's
/// `rand(r, ::SamplerTrivial{CloseOpen01{Float32}})`
/// (`reinterpret(Float32, rand(UInt23) | 0x3f800000) - 1`,
/// julia/stdlib/Random/src/generation.jl). Rounding a `f64` from `next_f64()`
/// down to `f32` (round-to-nearest) can land f64 values just below 1.0 exactly
/// on 1.0, breaking the `[0, 1)` contract (Issue #9275); building the value from
/// 23 random mantissa bits cannot exceed `1 - 2^-23`, so the result is always
/// `< 1.0` at full `Float32` granularity.
#[inline]
fn sample_f32_close_open01<Rg: RngLike>(rng: &mut Rg) -> f32 {
    // 23 random mantissa bits placed in a [1, 2) float, then shifted to [0, 1).
    let mantissa = (rng.next_u64() as u32) & 0x007f_ffff;
    f32::from_bits(mantissa | 0x3f80_0000) - 1.0
}

/// Draw a `Float16` uniformly in `[0, 1)` by constructing the float from random
/// mantissa bits, mirroring upstream Julia's
/// `rand(r, ::SamplerTrivial{CloseOpen01{Float16}})`
/// (`Float16(reinterpret(Float32, (rand(UInt10) << 13) | 0x3f800000) - 1)`,
/// julia/stdlib/Random/src/generation.jl). 10 random mantissa bits give a value
/// in `[0, 1)` at full `Float16` granularity whose maximum (`1023/1024`) is
/// exactly representable and strictly `< 1.0`, avoiding the round-to-nearest
/// overflow to 1.0 that a `f16::from_f64(next_f64())` cast suffers (Issue #9275).
#[inline]
fn sample_f16_close_open01<Rg: RngLike>(rng: &mut Rg) -> f16 {
    // 10 random mantissa bits (Float16 mantissa width) via the Float32 [1, 2)
    // construction, then shifted to [0, 1) and narrowed to Float16.
    let mantissa10 = (rng.next_u64() as u32) & 0x0000_03ff;
    let f = f32::from_bits((mantissa10 << 13) | 0x3f80_0000) - 1.0;
    f16::from_f32(f)
}

/// Convert a slice of runtime values (all array dimensions) into a dimension
/// vector, erroring on any non-dimension value. Used by the N-dimensional
/// `RandMaybeRng` array forms (Issue #9285).
fn rng_values_to_dims(vals: &[Value]) -> Result<Vec<usize>, VmError> {
    vals.iter().map(rng_value_to_dim).collect()
}

/// Draw a `size`-element (`size = product(dims)`) random array of scalar type
/// `ty` from `rng`, materialized with the faithful element type (Issue #9328).
///
/// Every element is drawn via the same [`sample_scalar_typed`] path as the
/// scalar `rand(rng, T)` form, so an N-element array consumes exactly the RNG
/// words that N successive scalar `rand(rng, T)` draws would — array and scalar
/// draws never disagree on the stream contract.
///
/// The result carries the faithful element type for every type sjulia can
/// represent: the native-storage bits integers / `Bool` / `Float32` / `Float64`
/// and the `Any`-backed `Int128` / `UInt128` (tagged via an element-type
/// override, Issue #3557 convention). `Float16` has no native array storage in
/// sjulia — `Float16[...]` and `zeros(Float16, n)` degrade to `Vector{Any}`
/// (see `compile/expr/collection.rs`) — so its elements are stored `Any`-backed
/// to match; the faithful `Vector{Float16}` representation is a separate
/// pre-existing gap tracked outside this fix.
fn sample_typed_array<Rg: RngLike>(
    ty: ScalarRandType,
    dims: Vec<usize>,
    rng: &mut Rg,
) -> ArrayValue {
    let size: usize = dims.iter().product();

    // Collect `size` typed scalars into the narrowest ArrayData that faithfully
    // represents `ty`. `sample_scalar_typed(ty, ..)` always returns the matching
    // `Value` variant, so the fallback arm is unreachable.
    macro_rules! collect_as {
        ($arm:pat => $val:expr, $data:ident, $elem:ident) => {{
            let mut buf = Vec::with_capacity(size);
            for _ in 0..size {
                match sample_scalar_typed(ty, rng) {
                    $arm => buf.push($val),
                    _ => unreachable!("sample_scalar_typed produced a mismatched variant"),
                }
            }
            (ArrayData::$data(buf), ArrayElementType::$elem)
        }};
    }

    let (data, element_type) = match ty {
        ScalarRandType::I8 => collect_as!(Value::I8(x) => x, I8, I8),
        ScalarRandType::I16 => collect_as!(Value::I16(x) => x, I16, I16),
        ScalarRandType::I32 => collect_as!(Value::I32(x) => x, I32, I32),
        ScalarRandType::I64 => collect_as!(Value::I64(x) => x, I64, I64),
        ScalarRandType::U8 => collect_as!(Value::U8(x) => x, U8, U8),
        ScalarRandType::U16 => collect_as!(Value::U16(x) => x, U16, U16),
        ScalarRandType::U32 => collect_as!(Value::U32(x) => x, U32, U32),
        ScalarRandType::U64 => collect_as!(Value::U64(x) => x, U64, U64),
        ScalarRandType::Bool => collect_as!(Value::Bool(x) => x, Bool, Bool),
        ScalarRandType::F32 => collect_as!(Value::F32(x) => x, F32, F32),
        ScalarRandType::F64 => collect_as!(Value::F64(x) => x, F64, F64),
        // Int128/UInt128 use boxed `Any` storage tagged with an element-type
        // override so `typeof` reports `Vector{Int128}` / `Vector{UInt128}`.
        ScalarRandType::I128 => collect_as!(v @ Value::I128(_) => v, Any, I128),
        ScalarRandType::U128 => collect_as!(v @ Value::U128(_) => v, Any, U128),
        // Float16: no native array storage — stored `Any`-backed exactly like a
        // `Float16[...]` literal (displays `Vector{Any}` today).
        ScalarRandType::F16 => collect_as!(v @ Value::F16(_) => v, Any, Any),
    };

    ArrayValue::memory_first_from_array_data_with_element_type(data, dims, element_type)
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

#[cfg(test)]
mod tests {
    use super::{sample_scalar_typed, ScalarRandType};
    use crate::rng::{MersenneTwister, StableRng, Xoshiro};
    use crate::vm::value::Value;

    /// Every `rand(rng, Float16)` draw must satisfy the `[0, 1)` contract.
    /// Regression for Issue #9275: the previous `f16::from_f64(next_f64())`
    /// implementation rounded f64 values just below 1.0 up to exactly 1.0 at a
    /// ~2^-12 rate, violating the upper bound. Draw many samples from several
    /// deterministic engines and assert none reaches 1.0.
    #[test]
    fn sample_scalar_typed_float16_stays_in_unit_interval() {
        const DRAWS: usize = 300_000;
        for (engine, mut draw) in float16_engines() {
            let mut max = f64::NEG_INFINITY;
            for i in 0..DRAWS {
                let v = match draw() {
                    Value::F16(f) => f.to_f64(),
                    other => panic!("{engine}: expected Value::F16, got {other:?}"),
                };
                assert!(
                    (0.0..1.0).contains(&v),
                    "{engine}: draw {i} = {v} is outside [0, 1)"
                );
                if v > max {
                    max = v;
                }
            }
            // Full Float16 granularity: the maximum over many draws should reach
            // near-1.0 (1023/1024) yet never touch 1.0.
            assert!(
                max < 1.0,
                "{engine}: observed maximum {max} must stay below 1.0"
            );
            assert!(
                max > 0.9,
                "{engine}: observed maximum {max} is implausibly low (lost granularity)"
            );
        }
    }

    /// Every `rand(rng, Float32)` draw must satisfy the `[0, 1)` contract.
    /// Regression for Issue #9275: `next_f64() as f32` rounded f64 values just
    /// below 1.0 up to 1.0 at a ~2^-25 rate.
    #[test]
    fn sample_scalar_typed_float32_stays_in_unit_interval() {
        const DRAWS: usize = 300_000;
        for (engine, mut draw) in float32_engines() {
            let mut max = f64::NEG_INFINITY;
            for i in 0..DRAWS {
                let v = match draw() {
                    Value::F32(f) => f64::from(f),
                    other => panic!("{engine}: expected Value::F32, got {other:?}"),
                };
                assert!(
                    (0.0..1.0).contains(&v),
                    "{engine}: draw {i} = {v} is outside [0, 1)"
                );
                if v > max {
                    max = v;
                }
            }
            assert!(
                max < 1.0,
                "{engine}: observed maximum {max} must stay below 1.0"
            );
            assert!(
                max > 0.99,
                "{engine}: observed maximum {max} is implausibly low (lost granularity)"
            );
        }
    }

    type Draw = Box<dyn FnMut() -> Value>;

    fn float16_engines() -> Vec<(&'static str, Draw)> {
        let mut mt = MersenneTwister::new(12345);
        let mut xo = Xoshiro::new(0xABCD);
        let mut st = StableRng::new(42);
        vec![
            (
                "MersenneTwister",
                Box::new(move || sample_scalar_typed(ScalarRandType::F16, &mut mt)) as Draw,
            ),
            (
                "Xoshiro",
                Box::new(move || sample_scalar_typed(ScalarRandType::F16, &mut xo)) as Draw,
            ),
            (
                "StableRng",
                Box::new(move || sample_scalar_typed(ScalarRandType::F16, &mut st)) as Draw,
            ),
        ]
    }

    fn float32_engines() -> Vec<(&'static str, Draw)> {
        let mut mt = MersenneTwister::new(12345);
        let mut xo = Xoshiro::new(0xABCD);
        let mut st = StableRng::new(42);
        vec![
            (
                "MersenneTwister",
                Box::new(move || sample_scalar_typed(ScalarRandType::F32, &mut mt)) as Draw,
            ),
            (
                "Xoshiro",
                Box::new(move || sample_scalar_typed(ScalarRandType::F32, &mut xo)) as Draw,
            ),
            (
                "StableRng",
                Box::new(move || sample_scalar_typed(ScalarRandType::F32, &mut st)) as Draw,
            ),
        ]
    }
}
