//! Function call instructions.
//!
//! Handles: Call, CallWithKwargs, CallWithSplat, CallSpecialize, CallIntrinsic, CallBuiltin
//!
//! ## Kwargs Binding Pattern (Issue #2397)
//!
//! Kwargs binding is centralized in two helper functions to avoid divergence:
//! - `bind_kwargs_defaults()`: Binds all kwargs to defaults (no kwargs provided at call site)
//! - `bind_kwargs_with_map()`: Binds kwargs using provided map (kwargs provided at call site)
//!
//! All call instruction handlers MUST use these helpers instead of inline kwargs logic.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::slot::slotize_code;
use super::util::bind_value_to_slot;
use crate::rng::RngLike;
use std::collections::HashMap;

/// Bind all keyword arguments to their defaults (no kwargs provided at call site).
///
/// Used by: `Call`, `CallWithSplat`
///
/// This function handles:
/// - Required kwargs: Returns error if any required kwarg has no value
/// - kwargs varargs: Binds to empty `Pairs` (NOT `Nothing`)
/// - Regular kwargs: Binds to their default values
pub(super) fn bind_kwargs_defaults(
    func: &FunctionInfo,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
) -> Result<(), VmError> {
    for kwparam in &func.kwparams {
        if kwparam.required {
            return Err(VmError::UndefKeywordError(kwparam.name.clone()));
        }
        let default_value = if kwparam.is_varargs {
            // kwargs varargs with no kwargs passed: bind empty Pairs (not Nothing)
            Value::Pairs(PairsValue {
                data: NamedTupleValue {
                    names: vec![],
                    values: vec![],
                },
            })
        } else {
            kwparam.default.clone()
        };
        bind_value_to_slot(frame, kwparam.slot, default_value, struct_heap);
    }
    Ok(())
}

/// Bind keyword arguments using provided kwargs map (kwargs provided at call site).
///
/// Used by: `CallWithKwargs`, `CallWithKwargsSplat`
///
/// This function handles:
/// - kwargs varargs: Collects remaining kwargs not matched to named kwparams
/// - Provided kwargs: Uses value from map
/// - Required kwargs: Returns error if not provided
/// - Regular kwargs: Falls back to default value
fn bind_kwargs_with_map(
    func: &FunctionInfo,
    kwargs_map: &HashMap<String, Value>,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
) -> Result<(), VmError> {
    for kwparam in &func.kwparams {
        if kwparam.is_varargs {
            // This is a kwargs varargs parameter (kwargs...)
            // Collect remaining kwargs that weren't matched to specific kwparams
            let mut remaining: Vec<(String, Value)> = Vec::with_capacity(kwargs_map.len());
            for (k, v) in kwargs_map.iter() {
                // Check if this key is a named kwparam (not the varargs one)
                let is_named_kwparam = func
                    .kwparams
                    .iter()
                    .any(|kp| !kp.is_varargs && &kp.name == k);
                if !is_named_kwparam {
                    remaining.push((k.clone(), v.clone()));
                }
            }
            // Create a Pairs from remaining kwargs (Julia's Base.Pairs type)
            let names: Vec<String> = remaining.iter().map(|(k, _)| k.clone()).collect();
            let values: Vec<Value> = remaining.into_iter().map(|(_, v)| v).collect();
            let pairs = Value::Pairs(PairsValue {
                data: NamedTupleValue { names, values },
            });
            bind_value_to_slot(frame, kwparam.slot, pairs, struct_heap);
        } else if let Some(val) = kwargs_map.get(&kwparam.name) {
            bind_value_to_slot(frame, kwparam.slot, val.clone(), struct_heap);
        } else if kwparam.required {
            return Err(VmError::UndefKeywordError(kwparam.name.clone()));
        } else {
            bind_value_to_slot(frame, kwparam.slot, kwparam.default.clone(), struct_heap);
        }
    }
    Ok(())
}

/// Result type for call operations
pub(super) enum CallResult {
    /// Instruction was not handled by this module
    NotHandled,
    /// Instruction was handled successfully
    Handled,
    /// Instruction handled, but need to continue (e.g., after raise)
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute function call instructions.
    ///
    /// Returns `CallResult::NotHandled` if the instruction is not a call operation.
    #[inline]
    pub(super) fn execute_call(&mut self, instr: &Instr) -> Result<CallResult, VmError> {
        match instr {
            Instr::Call(func_index, arg_count) => {
                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(CallResult::Continue),
                };
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();
                let mut frame = Frame::new_with_slots(func.local_slot_count, Some(*func_index));

                // Extract type parameter bindings from arguments (Issue #2468)
                self.bind_type_params(&func, &args, &mut frame);

                // Bind positional arguments
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs: bind args[0..vararg_idx] normally,
                    // then collect remaining args into a Tuple for the vararg param
                    for idx in 0..vararg_idx {
                        if let Some(val) = args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining args into a Tuple
                    let vararg_values: Vec<Value> = args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                        if let Some(val) = args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                }

                // Bind keyword arguments with their defaults (no kwargs provided)
                bind_kwargs_defaults(&func, &mut frame, &mut self.struct_heap)?;

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = func.entry;
                Ok(CallResult::Handled)
            }

            Instr::CallWithKwargs(func_index, pos_arg_count, ref kwarg_names) => {
                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(CallResult::Continue),
                };

                // Pop kwarg values from stack (they were pushed last)
                let mut kwarg_values: Vec<Value> = Vec::with_capacity(kwarg_names.len());
                for _ in 0..kwarg_names.len() {
                    kwarg_values.push(self.stack.pop_value()?);
                }
                kwarg_values.reverse();

                // Build kwargs map
                let mut kwargs_map: HashMap<String, Value> = HashMap::new();
                for (name, value) in kwarg_names.iter().zip(kwarg_values.into_iter()) {
                    kwargs_map.insert(name.clone(), value);
                }

                // Pop positional args
                let mut pos_args = Vec::with_capacity(*pos_arg_count);
                for _ in 0..*pos_arg_count {
                    pos_args.push(self.stack.pop_value()?);
                }
                pos_args.reverse();

                let mut frame = Frame::new_with_slots(func.local_slot_count, Some(*func_index));

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &pos_args, &mut frame);

                // Bind positional args (with varargs support)
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining args into a Tuple
                    let vararg_values: Vec<Value> = pos_args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                }

                // Bind keyword args (use provided value or default)
                bind_kwargs_with_map(&func, &kwargs_map, &mut frame, &mut self.struct_heap)?;

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = func.entry;
                Ok(CallResult::Handled)
            }

            Instr::CallWithKwargsSplat(
                func_index,
                pos_arg_count,
                ref kwarg_names,
                ref kwargs_splat_mask,
            ) => {
                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(CallResult::Continue),
                };

                // Pop kwarg values from stack (they were pushed last)
                let mut kwarg_values: Vec<Value> = Vec::with_capacity(kwarg_names.len());
                for _ in 0..kwarg_names.len() {
                    kwarg_values.push(self.stack.pop_value()?);
                }
                kwarg_values.reverse();

                // Build kwargs map, expanding splatted values
                let mut kwargs_map: HashMap<String, Value> = HashMap::new();
                for (idx, (name, value)) in
                    kwarg_names.iter().zip(kwarg_values.into_iter()).enumerate()
                {
                    if kwargs_splat_mask.get(idx).copied().unwrap_or(false) {
                        // This is a splatted kwarg - expand NamedTuple/Dict into key-value pairs
                        match &value {
                            Value::NamedTuple(named_tuple) => {
                                for (k, v) in
                                    named_tuple.names.iter().zip(named_tuple.values.iter())
                                {
                                    kwargs_map.insert(k.clone(), v.clone());
                                }
                            }
                            Value::Tuple(tuple) => {
                                // Tuple of pairs: ((k1, v1), (k2, v2), ...)
                                for elem in &tuple.elements {
                                    if let Value::Tuple(pair) = elem {
                                        if pair.elements.len() == 2 {
                                            if let Value::Symbol(key) = &pair.elements[0] {
                                                kwargs_map.insert(
                                                    key.as_str().to_string(),
                                                    pair.elements[1].clone(),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Value::Dict(dict) => {
                                // Dict{String/Symbol, Any} -> expand to kwargs
                                for (k, v) in dict.iter() {
                                    match k {
                                        DictKey::Str(key) => {
                                            kwargs_map.insert(key.clone(), v.clone());
                                        }
                                        DictKey::Symbol(key) => {
                                            kwargs_map.insert(key.clone(), v.clone());
                                        }
                                        DictKey::I64(_) => {
                                            // Integer keys are not valid kwargs - skip
                                        }
                                    }
                                }
                            }
                            _ => {
                                // Unknown type - ignore silently
                            }
                        }
                    } else {
                        // Regular kwarg - add directly
                        kwargs_map.insert(name.clone(), value);
                    }
                }

                // Pop positional args
                let mut pos_args = Vec::with_capacity(*pos_arg_count);
                for _ in 0..*pos_arg_count {
                    pos_args.push(self.stack.pop_value()?);
                }
                pos_args.reverse();

                let mut frame = Frame::new_with_slots(func.local_slot_count, Some(*func_index));

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &pos_args, &mut frame);

                // Bind positional args (with varargs support)
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining args into a Tuple
                    let vararg_values: Vec<Value> = pos_args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                        if let Some(val) = pos_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                }

                // Bind keyword args (use provided value or default)
                bind_kwargs_with_map(&func, &kwargs_map, &mut frame, &mut self.struct_heap)?;

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = func.entry;
                Ok(CallResult::Handled)
            }

            Instr::CallWithSplat(func_index, arg_count, ref splat_mask) => {
                let func = match self.get_function_cloned_or_raise(*func_index)? {
                    Some(f) => f,
                    None => return Ok(CallResult::Continue),
                };

                // Pop arguments from stack
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Expand splatted arguments
                let expanded_args = super::super::splat::expand_splat_arguments(args, splat_mask);

                let mut frame = Frame::new_with_slots(func.local_slot_count, Some(*func_index));

                // Bind expanded arguments to parameters (with varargs support)
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = expanded_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining expanded args into a Tuple
                    let vararg_values: Vec<Value> = expanded_args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, (_name, _ty)) in func.params.iter().enumerate() {
                        if let Some(val) = expanded_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                }

                // Bind keyword arguments with their defaults (no kwargs provided via splat)
                bind_kwargs_defaults(&func, &mut frame, &mut self.struct_heap)?;

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = func.entry;
                Ok(CallResult::Handled)
            }

            // Lazy AoT call: specialize function based on runtime argument types
            Instr::CallSpecialize(spec_func_index, arg_count) => {
                // 1. Pop arguments from stack
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Get spec_func for fallback
                let spec_func = match self.specializable_functions.get(*spec_func_index) {
                    Some(f) => f.clone(),
                    None => {
                        self.raise(VmError::InternalError(format!(
                            "unknown specializable function index: {}",
                            spec_func_index
                        )))?;
                        return Ok(CallResult::Continue);
                    }
                };
                let fallback_func =
                    match self.get_function_cloned_or_raise(spec_func.fallback_index)? {
                        Some(f) => f,
                        None => return Ok(CallResult::Continue),
                    };

                // 2. Extract actual types from arguments (typeof(x))
                let arg_types: Vec<ValueType> =
                    args.iter().map(|v| self.get_value_type(v)).collect();

                // 3. Look up in specialization cache
                let key = SpecializationKey {
                    func_index: *spec_func_index,
                    arg_types: arg_types.clone(),
                };

                let entry = if let Some(cached) = self.specialization_cache.get(&key) {
                    Some(cached.entry)
                } else {
                    // 4. Cache miss: try to specialize now
                    if self.compile_context.is_some() {
                        match specialize::specialize_function(&spec_func.ir, &arg_types) {
                            Ok(result) => {
                                // 5. Append bytecode to code vector
                                // IMPORTANT: Relocate jump targets by adding entry_point offset
                                let entry_point = self.code.len();
                                let mut specialized_code = result.code;
                                let slot_map: HashMap<String, usize> = fallback_func
                                    .slot_names
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, name)| (name.clone(), idx))
                                    .collect();
                                slotize_code(&mut specialized_code, &slot_map);
                                for instr in specialized_code {
                                    let relocated = match instr {
                                        Instr::Jump(target) => Instr::Jump(target + entry_point),
                                        Instr::JumpIfZero(target) => {
                                            Instr::JumpIfZero(target + entry_point)
                                        }
                                        // All other instructions pass through unchanged
                                        // Only jump targets need relocation
                                        other => other,
                                    };
                                    self.code.push(relocated);
                                }

                                // 6. Cache the specialized code
                                self.specialization_cache.insert(
                                    key,
                                    SpecializedCode {
                                        entry: entry_point,
                                        return_type: result.return_type,
                                        code_len: self.code.len() - entry_point,
                                    },
                                );
                                Some(entry_point)
                            }
                            Err(_) => {
                                // Specialization failed, fall back to generic version
                                None
                            }
                        }
                    } else {
                        // No compile context, fall back
                        None
                    }
                };

                // 7. Set up frame and jump to code
                let mut frame = Frame::new_with_slots(
                    fallback_func.local_slot_count,
                    Some(spec_func.fallback_index),
                );
                let target_entry = if let Some(specialized_entry) = entry {
                    specialized_entry
                } else {
                    fallback_func.entry
                };

                // Bind arguments (with varargs support)
                if let Some(vararg_idx) = fallback_func.vararg_param_index {
                    // Function has varargs: bind fixed params, then pack rest into tuple
                    for idx in 0..vararg_idx {
                        if let Some(val) = args.get(idx) {
                            if let Some(slot) = fallback_func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining args into a Tuple
                    let vararg_values: Vec<Value> = args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = fallback_func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, slot) in fallback_func.param_slots.iter().enumerate() {
                        if let Some(val) = args.get(idx) {
                            bind_value_to_slot(
                                &mut frame,
                                *slot,
                                val.clone(),
                                &mut self.struct_heap,
                            );
                        }
                    }
                }

                if entry.is_none() {
                    // No specialized entry, use fallback - check for required kwargs
                    for kwparam in &fallback_func.kwparams {
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
                }

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = target_entry;
                Ok(CallResult::Handled)
            }

            Instr::CallIntrinsic(intrinsic) => {
                if let Err(err) = self.execute_intrinsic(*intrinsic) {
                    self.raise(err)?;
                    return Ok(CallResult::Continue);
                }
                Ok(CallResult::Handled)
            }

            Instr::CallBuiltin(builtin_id, argc) => {
                if let Err(err) = self.execute_builtin(*builtin_id, *argc) {
                    self.raise(err)?;
                    return Ok(CallResult::Continue);
                }
                Ok(CallResult::Handled)
            }

            _ => Ok(CallResult::NotHandled),
        }
    }
}
