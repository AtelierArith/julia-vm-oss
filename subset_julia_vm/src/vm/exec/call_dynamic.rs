//! Dynamic dispatch call instructions.
//!
//! This module serves as the entry point for all dynamic dispatch operations.
//! The main dispatcher delegates to specialized submodules:
//!
//! - `call_dynamic_binary`: Binary operator dispatch (CallDynamicBinary, CallDynamicBinaryBoth, etc.)
//! - `call_dynamic_typed`: Typed dispatch (CallTypedDispatch, CallTypeConstructor)
//! - `call_function_variable`: Function variable calls (CallGlobalRef, CallFunctionVariable, etc.)
//!
//! ## Debug Logging
//!
//! Set `SJULIA_DISPATCH_DEBUG=1` to enable dispatch tracing for binary operations.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::util::{
    bind_value_to_slot, extract_base_type, is_rust_dict_parametric_mismatch, score_type_match,
};
use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use crate::vm::intrinsics_exec::apply_unary_float_op;

/// Check if dispatch debug logging is enabled via `SJULIA_DISPATCH_DEBUG` env var.
/// Only available in debug builds to avoid performance impact in release.
#[cfg(debug_assertions)]
pub(super) fn dispatch_debug_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("SJULIA_DISPATCH_DEBUG").is_ok())
}

/// Emit dispatch debug logs in debug builds without relying on `eprintln!`.
#[cfg(debug_assertions)]
pub(super) fn dispatch_debug_log(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{args}");
}

/// Result type for dynamic call operations
pub(super) enum CallDynamicResult {
    /// Instruction was not handled by this module
    NotHandled,
    /// Instruction was handled successfully
    Handled,
    /// Instruction handled, but need to continue (e.g., after raise)
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute dynamic dispatch call instructions.
    ///
    /// Returns `CallDynamicResult::NotHandled` if the instruction is not a dynamic call operation.
    /// Delegates to specialized handlers for binary, typed, and function variable dispatch.
    #[inline]
    pub(super) fn execute_call_dynamic(
        &mut self,
        instr: &Instr,
    ) -> Result<CallDynamicResult, VmError> {
        // Try specialized handlers first
        match instr {
            Instr::CallDynamicBinary(..)
            | Instr::CallDynamicBinaryBoth(..)
            | Instr::CallDynamicBinaryNoFallback(..) => {
                return self.execute_call_dynamic_binary(instr);
            }
            Instr::CallTypedDispatch(..) | Instr::CallTypeConstructor => {
                return self.execute_call_dynamic_typed(instr);
            }
            Instr::CallGlobalRef(..)
            | Instr::CallFunctionVariable(..)
            | Instr::CallFunctionVariableWithSplat(..) => {
                return self.execute_call_function_variable(instr);
            }
            _ => {}
        }

        match instr {
            Instr::CallDynamic(fallback_func_index, arg_count, ref candidates) => {
                // Runtime method dispatch: check argument types and select best match
                #[cfg(debug_assertions)]
                if dispatch_debug_enabled() {
                    dispatch_debug_log(format_args!(
                        "[DISPATCH] CallDynamic: arg_count={}, candidates={}, fallback=#{}",
                        arg_count,
                        candidates.len(),
                        fallback_func_index
                    ));
                }
                // Pop arguments to inspect their types
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Dispatch based on first argument type (Issue #2517, #2691).
                // For multi-arg functions, we dispatch on the first argument's type,
                // which covers critical cases like _broadcast_getindex(x::Number, I)
                // where Complex{Bool} must match ::Number, not fall through to ::Array.
                let selected_func_index = if *arg_count >= 1 {
                    let arg_type_name = self.get_type_name(&args[0]);

                    // Check dispatch cache first (Issue #2943, #3355)
                    let call_site_ip = self.ip - 1;
                    let type_hash = hash_type_name(&arg_type_name);
                    if let Some(cached) = self
                        .dispatch_cache
                        .get(&call_site_ip)
                        .and_then(|m| m.get(&type_hash))
                    {
                        *cached
                    } else {
                        let arg_base = extract_base_type(&arg_type_name);

                        // Scored dispatch: find the most specific match (Issue #2517).
                        // Uses shared score_type_match() for consistent scoring across all handlers.
                        let mut best_idx: Option<usize> = None;
                        let mut best_score: u32 = 0;
                        for (idx, expected_type) in candidates.iter() {
                            // Value::Dict (Rust-backed) must not match parametric Dict{K,V}
                            // Pure Julia methods that expect StructRef (Issue #2748).
                            if is_rust_dict_parametric_mismatch(&args[0], expected_type) {
                                continue;
                            }
                            let mut score =
                                score_type_match(expected_type, &arg_type_name, arg_base);
                            if score == 0 && self.check_subtype(&arg_type_name, expected_type) {
                                score = 1; // Subtype match (lowest priority)
                            }
                            if score > best_score {
                                best_score = score;
                                best_idx = Some(*idx);
                                if score == 4 {
                                    break; // Exact match can't be beaten
                                }
                            }
                        }
                        let result = best_idx.unwrap_or(*fallback_func_index);
                        // Store in cache using hashed key (Issue #3355)
                        self.dispatch_cache
                            .entry(call_site_ip)
                            .or_default()
                            .insert(type_hash, result);
                        result
                    }
                } else {
                    *fallback_func_index
                };

                let func = match self.get_function_cloned_or_raise(selected_func_index)? {
                    Some(f) => f,
                    None => return Ok(CallDynamicResult::Continue),
                };

                let mut frame =
                    Frame::new_with_slots(func.local_slot_count, Some(selected_func_index));

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &args, &mut frame);

                // Bind arguments (with varargs support), consuming args to avoid cloning
                if let Some(vararg_idx) = func.vararg_param_index {
                    let vararg_values: Vec<Value> = args.drain(vararg_idx..).collect();
                    for (slot, val) in func.param_slots[..vararg_idx].iter().zip(args) {
                        bind_value_to_slot(&mut frame, *slot, val, &mut self.struct_heap);
                    }
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1, consuming args
                    for (slot, val) in func.param_slots.iter().zip(args) {
                        bind_value_to_slot(&mut frame, *slot, val, &mut self.struct_heap);
                    }
                }

                for kwparam in &func.kwparams {
                    if kwparam.required {
                        return Err(VmError::UndefKeywordError(kwparam.name.clone()));
                    }
                    bind_value_to_slot(
                        &mut frame,
                        kwparam.slot,
                        kwparam.default.clone(),
                        &mut self.struct_heap,
                    );
                }

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = func.entry;
                Ok(CallDynamicResult::Handled)
            }

            Instr::CallDynamicOrBuiltin(builtin_id, ref candidates) => {
                // Runtime dispatch for unary functions with builtin fallback.
                // Pop the argument to inspect its type
                let arg = self.stack.pop_value()?;
                let arg_type_name = self.get_type_name(&arg);

                // Check dispatch cache first (Issue #2943, #3355)
                let call_site_ip = self.ip - 1;
                let type_hash = hash_type_name(&arg_type_name);
                let matched = if let Some(&cached) = self
                    .dispatch_cache
                    .get(&call_site_ip)
                    .and_then(|m| m.get(&type_hash))
                {
                    // Cache stores usize::MAX as sentinel for "no match" (use builtin)
                    if cached == usize::MAX {
                        None
                    } else {
                        Some(cached)
                    }
                } else {
                    // Scored dispatch: find the most specific match (Issue #2517).
                    let arg_base = extract_base_type(&arg_type_name);
                    let mut best_idx: Option<usize> = None;
                    let mut best_score: u32 = 0;
                    for (idx, expected_type) in candidates.iter() {
                        // Value::Dict (Rust-backed) must not match parametric Dict{K,V}
                        // Pure Julia methods that expect StructRef (Issue #2748).
                        if is_rust_dict_parametric_mismatch(&arg, expected_type) {
                            continue;
                        }
                        let mut score =
                            score_type_match(expected_type, &arg_type_name, arg_base);
                        if score == 0 && self.check_subtype(&arg_type_name, expected_type) {
                            score = 1;
                        }
                        if score > best_score {
                            best_score = score;
                            best_idx = Some(*idx);
                            if score == 4 {
                                break; // Exact match can't be beaten
                            }
                        }
                    }
                    // Store in cache using hashed key (Issue #3355)
                    let cache_val = best_idx.unwrap_or(usize::MAX);
                    self.dispatch_cache
                        .entry(call_site_ip)
                        .or_default()
                        .insert(type_hash, cache_val);
                    best_idx
                };

                if let Some(func_index) = matched {
                    // Call the user-defined method
                    let func = match self.get_function_cloned_or_raise(func_index)? {
                        Some(f) => f,
                        None => return Ok(CallDynamicResult::Continue),
                    };

                    let mut frame = Frame::new_with_slots(func.local_slot_count, Some(func_index));

                    // Bind type parameters from where clauses (Issue #2468)
                    self.bind_type_params(&func, std::slice::from_ref(&arg), &mut frame);

                    if let Some(slot) = func.param_slots.first() {
                        bind_value_to_slot(&mut frame, *slot, arg, &mut self.struct_heap);
                    }

                    for kwparam in &func.kwparams {
                        if kwparam.required {
                            return Err(VmError::UndefKeywordError(kwparam.name.clone()));
                        }
                        bind_value_to_slot(
                            &mut frame,
                            kwparam.slot,
                            kwparam.default.clone(),
                            &mut self.struct_heap,
                        );
                    }

                    self.return_ips.push(self.ip);
                    self.frames.push(frame);
                    self.ip = func.entry;
                } else {
                    // No matching struct method - fall back to builtin
                    // Special case for NegAny: preserve type
                    if matches!(builtin_id, BuiltinId::NegAny) {
                        let result = match arg {
                            Value::I64(v) => Value::I64(-v),
                            Value::F64(v) => Value::F64(-v),
                            Value::I8(v) => Value::I8(-v),
                            Value::I16(v) => Value::I16(-v),
                            Value::I32(v) => Value::I32(-v),
                            Value::I128(v) => Value::I128(-v),
                            Value::F16(v) => Value::F16(-v),
                            Value::F32(v) => Value::F32(-v),
                            _ => {
                                let arg_type = self.get_type_name(&arg);
                                self.raise(VmError::TypeError(format!(
                                    "expected numeric for NegAny, got {}",
                                    arg_type
                                )))?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        };
                        self.stack.push(result);
                    } else {
                        // Resolve builtin to f64 operation, then use
                        // apply_unary_float_op for type-preserving dispatch (Issue #2284)
                        let op: fn(f64) -> f64 = match builtin_id {
                            // Note: Exp, Log, Sin, Cos, Tan removed — now Pure Julia (base/math.jl)
                            BuiltinId::Floor => f64::floor,
                            BuiltinId::Ceil => f64::ceil,
                            _ => {
                                self.raise(VmError::MethodError(format!(
                                    "unsupported builtin for CallDynamicOrBuiltin: {:?}",
                                    builtin_id
                                )))?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        };
                        let result = apply_unary_float_op(arg, op)?;
                        self.stack.push(result);
                    }
                }
                Ok(CallDynamicResult::Handled)
            }

            Instr::IterateDynamic(argc, ref candidates) => {
                // Dynamic dispatch for iterate() when collection type is Any at compile time.
                // Supports both 1-arg (initial) and 2-arg (subsequent) forms.
                let (coll, state_opt) = if *argc == 2 {
                    let state = self.stack.pop_value()?;
                    let coll = self.stack.pop_value()?;
                    (coll, Some(state))
                } else {
                    let coll = self.stack.pop_value()?;
                    (coll, None)
                };

                // Check if collection is a struct type
                let is_struct = matches!(&coll, Value::StructRef(_) | Value::Struct(_));

                // Special case: CartesianIndices uses VM builtin iterate
                let is_cartesian_indices = match &coll {
                    Value::Struct(s) => s.struct_name == "CartesianIndices",
                    Value::StructRef(idx) => self
                        .struct_heap
                        .get(*idx)
                        .is_some_and(|s| s.struct_name == "CartesianIndices"),
                    _ => false,
                };

                if is_struct && !is_cartesian_indices {
                    // Get struct type name and find matching iterate method
                    let coll_type_name = self.get_type_name(&coll);

                    // Check dispatch cache first (Issue #2943, #3355)
                    let call_site_ip = self.ip - 1;
                    let type_hash = hash_type_name(&coll_type_name);
                    let matched = if let Some(&cached) = self
                        .dispatch_cache
                        .get(&call_site_ip)
                        .and_then(|m| m.get(&type_hash))
                    {
                        if cached == usize::MAX {
                            None
                        } else {
                            // Find the candidate with this func_index for type binding
                            candidates.iter().find(|(idx, _)| *idx == cached)
                        }
                    } else {
                        // Extract base type name (e.g., "Drop{Array}" -> "Drop")
                        let base_type = extract_base_type(&coll_type_name);

                        // Find matching candidate using scored dispatch (Issue #2517)
                        let mut best: Option<&(usize, String)> = None;
                        let mut best_score: u32 = 0;
                        for candidate in candidates {
                            let (_, expected) = candidate;
                            let score = score_type_match(expected, &coll_type_name, base_type);
                            if score > best_score {
                                best_score = score;
                                best = Some(candidate);
                                if score == 4 {
                                    break; // Exact match can't be beaten
                                }
                            }
                        }
                        // Store in cache using hashed key (Issue #3355)
                        let cache_val = best.map(|(idx, _)| *idx).unwrap_or(usize::MAX);
                        self.dispatch_cache
                            .entry(call_site_ip)
                            .or_default()
                            .insert(type_hash, cache_val);
                        best
                    };

                    if let Some((func_index, _)) = matched {
                        // Call the user-defined iterate method
                        let func = match self.get_function_cloned_or_raise(*func_index)? {
                            Some(f) => f,
                            None => return Ok(CallDynamicResult::Continue),
                        };

                        let mut frame =
                            Frame::new_with_slots(func.local_slot_count, Some(*func_index));

                        // Bind type parameters from where clauses (Issue #2468)
                        {
                            let type_bind_args: Vec<Value> = if let Some(ref state) = state_opt {
                                vec![coll.clone(), state.clone()]
                            } else {
                                vec![coll.clone()]
                            };
                            self.bind_type_params(&func, &type_bind_args, &mut frame);
                        }

                        // Bind arguments to parameter slots
                        if let Some(slot) = func.param_slots.first() {
                            bind_value_to_slot(&mut frame, *slot, coll, &mut self.struct_heap);
                        }
                        if let Some(state) = state_opt {
                            if let Some(slot) = func.param_slots.get(1) {
                                bind_value_to_slot(&mut frame, *slot, state, &mut self.struct_heap);
                            }
                        }

                        for kwparam in &func.kwparams {
                            if kwparam.required {
                                return Err(VmError::UndefKeywordError(kwparam.name.clone()));
                            }
                            bind_value_to_slot(
                                &mut frame,
                                kwparam.slot,
                                kwparam.default.clone(),
                                &mut self.struct_heap,
                            );
                        }

                        self.return_ips.push(self.ip);
                        self.frames.push(frame);
                        self.ip = func.entry;
                    } else {
                        // No matching method found - error
                        // User-visible: user's struct type has no iterate method — triggered by for-loops over custom types
                        return Err(VmError::TypeError(format!(
                            "iterate: no method matching iterate(::{}{})",
                            coll_type_name,
                            if *argc == 2 { ", ...)" } else { ")" }
                        )));
                    }
                } else {
                    // Not a struct or CartesianIndices - use builtin iterate
                    let result = if let Some(state) = state_opt {
                        self.iterate_next(&coll, &state)?
                    } else {
                        self.iterate_first(&coll)?
                    };
                    self.stack.push(result);
                }
                Ok(CallDynamicResult::Handled)
            }

            _ => Ok(CallDynamicResult::NotHandled),
        }
    }
}
