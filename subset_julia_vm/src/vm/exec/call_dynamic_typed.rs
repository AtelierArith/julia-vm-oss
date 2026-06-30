//! Typed dispatch call instructions.
//!
//! Handles: CallTypedDispatch, CallTypeConstructor
//!
//! These instructions handle method dispatch when parameter types are
//! declared in function signatures, and type constructor calls.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::hof_exec::state::RuntimeCallableResult;
use super::super::*;
use super::call::bind_kwargs_defaults;
use super::call_dynamic::generator_iter_type_name;
use super::util::{bind_value_to_slot, is_struct_dict_bare_mismatch};
use super::DispatchAction;
use crate::builtins::BuiltinId;
use crate::inference_core::dispatch_resolver::{
    resolve_typed_runtime_core_candidates_with_subtype_fallback, RuntimeTypedCoreCandidate,
};
use crate::inference_core::selection;
use crate::inference_core::CoreType;
use crate::rng::RngLike;
use crate::vm::dispatch_binding::{
    build_runtime_candidate_core_signature, RuntimeCandidateCoreSignature,
};

fn ldiv_builtin_fallback_applies_to_value(value: &Value) -> bool {
    matches!(value, Value::Memory(_))
        || matches!(
            value.runtime_type(),
            crate::types::JuliaType::Array
                | crate::types::JuliaType::AbstractArray
                | crate::types::JuliaType::VectorOf(_)
                | crate::types::JuliaType::MatrixOf(_)
        )
}

fn runtime_typed_core_candidate<'a>(
    idx: usize,
    signature: &'a RuntimeCandidateCoreSignature,
) -> RuntimeTypedCoreCandidate<'a> {
    RuntimeTypedCoreCandidate {
        idx,
        rendered: signature.rendered.as_slice(),
        slots: signature.slots.as_slice(),
        signature: signature.signature.as_ref(),
    }
}

impl<R: RngLike> Vm<R> {
    /// Resolve the structured `CallTypedDispatch[OrBuiltin*]` candidate
    /// payload (function indices, Issue #6496) into per-arity runtime
    /// signatures.
    ///
    /// The per-arity signature is derived from each candidate's `FunctionInfo`;
    /// equality with the canonical `MethodSig` projection is pinned by
    /// `base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495`
    /// in `compile/cache.rs`. Results are memoized in
    /// `Vm::typed_signature_cache` because this family has no call-site
    /// dispatch cache. Candidates whose signature cannot be derived for the
    /// arity are dropped — the historical emit-time
    /// `runtime_type_names_for_arity` gate never baked them.
    fn typed_candidates_with_signatures(
        &mut self,
        candidates: &[usize],
        arity: usize,
    ) -> Vec<(
        usize,
        std::rc::Rc<crate::vm::dispatch_binding::RuntimeCandidateCoreSignature>,
    )> {
        for &func_index in candidates {
            if self
                .typed_signature_cache
                .contains_key(&(func_index, arity))
            {
                continue;
            }
            let derived = self
                .functions
                .get(func_index)
                .and_then(|func| {
                    let param_types =
                        crate::vm::dispatch_binding::expanded_param_types_for_call(func, arity)?;
                    Some(
                        crate::vm::dispatch_binding::build_runtime_candidate_core_signature(
                            &param_types,
                            &func.type_params,
                        ),
                    )
                })
                .map(std::rc::Rc::new);
            self.typed_signature_cache
                .insert((func_index, arity), derived);
        }
        candidates
            .iter()
            .filter_map(
                |&func_index| match self.typed_signature_cache.get(&(func_index, arity)) {
                    Some(Some(signature)) => Some((func_index, std::rc::Rc::clone(signature))),
                    _ => None,
                },
            )
            .collect()
    }

    /// Execute typed dispatch call instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not a typed dispatch operation.
    #[inline]
    pub(super) fn execute_call_dynamic_typed(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::CallTypedDispatchOrBuiltin(
                builtin_id,
                ref func_name,
                arg_count,
                ref candidates,
            ) => self.execute_call_typed_dispatch_or_builtin(
                *builtin_id,
                func_name,
                *arg_count,
                candidates,
                None,
                false,
            ),
            Instr::CallTypedDispatchOrBuiltinResult(
                builtin_id,
                ref func_name,
                arg_count,
                ref candidates,
            ) => self.execute_call_typed_dispatch_or_builtin(
                *builtin_id,
                func_name,
                *arg_count,
                candidates,
                None,
                true,
            ),
            Instr::CallTypedDispatchOrBuiltinStoreDict(ref operands) => self
                .execute_call_typed_dispatch_or_builtin(
                    operands.builtin,
                    &operands.function_name,
                    operands.arg_count,
                    &operands.candidates,
                    Some(operands.store_local.as_str()),
                    false,
                ),
            Instr::CallTypedDispatchOrBuiltinStoreDictResult(ref operands) => self
                .execute_call_typed_dispatch_or_builtin(
                    operands.builtin,
                    &operands.function_name,
                    operands.arg_count,
                    &operands.candidates,
                    Some(operands.store_local.as_str()),
                    true,
                ),

            Instr::CallTypedDispatch(ref _func_name, arg_count, fallback_index, ref candidates) => {
                // Runtime dispatch for Type{T} patterns
                // Pop arguments (expected to be DataType values)
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Issue #6496: the payload carries candidate function indices
                // only; reproduce the historical (index, type names) pairs.
                let candidates = self.typed_candidates_with_signatures(candidates, *arg_count);

                // Encode runtime values as dispatch types. A DataType value has
                // singleton type `Type{T}` in Julia dispatch, while ordinary
                // values use their runtime value type.
                let arg_cores: Vec<CoreType> = args
                    .iter()
                    .map(|arg| {
                        let ty = self.dispatch_julia_type_for_value(arg);
                        crate::vm::dispatch_binding::runtime_actual_core_type(&ty)
                    })
                    .collect();
                let filtered_candidates: Vec<_> = candidates
                    .iter()
                    .filter_map(|(idx, signature)| {
                        let has_dict_mismatch =
                            args.iter()
                                .zip(signature.rendered.iter())
                                .any(|(arg, exp)| {
                                    is_struct_dict_bare_mismatch(arg, exp, &self.struct_heap)
                                });
                        (!has_dict_mismatch).then_some((*idx, signature.as_ref()))
                    })
                    .collect();
                let metadata_candidate_indices: Vec<_> = filtered_candidates
                    .iter()
                    .filter_map(|(idx, _)| (*idx != usize::MAX).then_some(*idx))
                    .collect();

                // Find the best matching candidate:
                // 1. First try exact match (all concrete types)
                // 2. Then try pattern match with TypeVars, preferring more specific patterns
                let best_match = resolve_typed_runtime_core_candidates_with_subtype_fallback(
                    &self.struct_hierarchy,
                    filtered_candidates
                        .iter()
                        .map(|(idx, signature)| runtime_typed_core_candidate(*idx, signature)),
                    &arg_cores,
                    |actual, bound| self.check_subtype_core(actual, bound),
                );
                let metadata_best =
                    self.find_best_method_index_from_candidates(&metadata_candidate_indices, &args);
                let metadata_best = match metadata_best {
                    Ok(Some(idx)) => {
                        let candidate_signature_matches =
                            filtered_candidates
                                .iter()
                                .any(|(candidate_idx, signature)| {
                                    *candidate_idx == idx
                                    && resolve_typed_runtime_core_candidates_with_subtype_fallback(
                                        &self.struct_hierarchy,
                                        std::iter::once(runtime_typed_core_candidate(
                                            *candidate_idx,
                                            signature,
                                        )),
                                        &arg_cores,
                                        |actual, bound| self.check_subtype_core(actual, bound),
                                    )
                                    .is_some()
                                });
                        if candidate_signature_matches {
                            Some(idx)
                        } else {
                            None
                        }
                    }
                    Ok(None) => None,
                    Err(err) => {
                        if args.iter().any(|arg| matches!(arg, Value::DataType(_)))
                            && best_match.as_ref().is_some_and(|(_, score)| *score > 0)
                        {
                            None
                        } else {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                };

                // If no specific match found (only TypeVar fallback with negative specificity),
                // try runtime method search to find user-defined methods not in the
                // frozen candidate list (Issue #2557).
                //
                // The native-array wrapper fence (Issue #6595) is a selection-core
                // POLICY: when the value channel's `metadata_best` winner is an
                // over-broad catch-all the fence let through
                // (`signature_is_broad_wrapper_fence`), the structured repair
                // re-resolves the name channel over the non-broad candidate subset
                // so a broad `::Function`/`Any` method cannot overwrite a typed
                // specialization the fence excludes from the value channel
                // (hazard #6528: empty narrow-int/Bool reduce must not throw).
                let non_broad_best_match = selection::wrapper_fence_name_channel_repair(
                    metadata_best,
                    |idx| {
                        filtered_candidates
                            .iter()
                            .any(|(candidate_idx, signature)| {
                                *candidate_idx == idx
                                    && selection::signature_is_broad_wrapper_fence(
                                        &signature.rendered,
                                    )
                            })
                    },
                    || {
                        resolve_typed_runtime_core_candidates_with_subtype_fallback(
                            &self.struct_hierarchy,
                            filtered_candidates
                                .iter()
                                .filter(|(_, signature)| {
                                    !selection::signature_is_broad_wrapper_fence(
                                        &signature.rendered,
                                    )
                                })
                                .map(|(idx, signature)| {
                                    runtime_typed_core_candidate(*idx, signature)
                                }),
                            &arg_cores,
                            |actual, bound| self.check_subtype_core(actual, bound),
                        )
                    },
                );
                let selected_func_index = selection::select_typed_dispatch_candidate(
                    *fallback_index,
                    best_match,
                    metadata_best,
                    non_broad_best_match,
                    // Search functions by name at runtime for a better match (Issue #3361).
                    // Uses function_name_index for O(1) name lookup instead of O(f) scan.
                    // Each candidate is scored independently through the shared
                    // resolver (per-candidate `once(..)` keeps its two-stage
                    // non-`<:`-first policy out of the cross-candidate ranking);
                    // the first-best max-score winnow is owned by the shared
                    // selection core (`selection::pick_max_score`, Issue #6502).
                    || {
                        selection::pick_max_score(
                            self.get_function_indices_by_name(_func_name)
                                .iter()
                                .filter_map(|&idx| {
                                    let func = &self.functions[idx];
                                    if func.param_julia_types.len() != args.len() {
                                        return None;
                                    }
                                    let signature = build_runtime_candidate_core_signature(
                                        &func.param_julia_types,
                                        &func.type_params,
                                    );
                                    // Dict carrier mismatch is a no-op after
                                    // Value::Dict removal, but the shared
                                    // filter remains for older candidate shapes.
                                    let has_dict_mismatch = args
                                        .iter()
                                        .zip(signature.rendered.iter())
                                        .any(|(arg, exp)| {
                                            is_struct_dict_bare_mismatch(
                                                arg,
                                                exp,
                                                &self.struct_heap,
                                            )
                                        });
                                    if has_dict_mismatch {
                                        return None;
                                    }
                                    resolve_typed_runtime_core_candidates_with_subtype_fallback(
                                        &self.struct_hierarchy,
                                        std::iter::once(runtime_typed_core_candidate(
                                            idx, &signature,
                                        )),
                                        &arg_cores,
                                        |actual, bound| self.check_subtype_core(actual, bound),
                                    )
                                    .map(|(_, specificity)| (idx, specificity))
                                }),
                        )
                    },
                );

                let func = match self.get_function_cloned_or_raise(selected_func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };

                if args.len() == 1 {
                    if let Value::DataType(julia_type) = &args[0] {
                        if let Some(iter_type) = generator_iter_type_name(julia_type) {
                            match _func_name.as_str() {
                                "IteratorSize" => {
                                    let result = self
                                        .iterator_size_value_for_generator_iter_type_name(
                                            &iter_type,
                                        )?;
                                    self.stack.push(result);
                                    return Ok(DispatchAction::Continue);
                                }
                                "IteratorEltype" => {
                                    let result = self.zero_field_struct_value("EltypeUnknown")?;
                                    self.stack.push(result);
                                    return Ok(DispatchAction::Continue);
                                }
                                _ => {}
                            }
                        }
                    }
                }

                let mut frame =
                    self.acquire_frame(func.local_slot_count, Some(selected_func_index));
                if let Some(vararg_idx) = func.vararg_param_index {
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
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: args[vararg_idx..].to_vec(),
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    for (idx, slot) in func.param_slots.iter().enumerate() {
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

                // Bind type parameters from where clauses using the full parameter
                // JuliaType patterns. This is required for signatures like
                // `g(mem, ::Type{S}, n) where S`, where the type parameter is
                // not aligned with the argument index.
                self.bind_type_params(&func, &args, &mut frame);

                bind_kwargs_defaults(
                    &func,
                    &mut frame,
                    &mut self.struct_heap,
                    &self.code,
                    &self.functions,
                    self.frames.first(),
                    &self.global_slot_map,
                )?;

                self.return_ips.push(self.ip);
                self.try_push_call_frame(frame)?;
                self.ip = func.entry;
                Ok(DispatchAction::Continue)
            }

            Instr::CallTypeConstructor => {
                // Dynamic call: T(x) where T can be:
                // - DataType: type conversion
                // - Function: call the function
                // - ComposedFunction: call inner, then outer
                // Stack: [value, callable] -> [result]
                let callable = self.stack.pop_value()?;
                let value = self.stack.pop_value()?;

                match self.call_runtime_callable_value(callable, vec![value])? {
                    RuntimeCallableResult::Immediate(value) => {
                        self.stack.push(value);
                    }
                    RuntimeCallableResult::StartedFrame => {}
                    RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    fn execute_call_typed_dispatch_or_builtin(
        &mut self,
        builtin_id: BuiltinId,
        func_name: &str,
        arg_count: usize,
        candidates: &[usize],
        store_dict_name: Option<&str>,
        keep_builtin_result: bool,
    ) -> Result<DispatchAction, VmError> {
        let mut args = Vec::with_capacity(arg_count);
        for _ in 0..arg_count {
            args.push(self.stack.pop_value()?);
        }
        args.reverse();

        // Issue #6496: the payload carries candidate function indices only;
        // reproduce the historical (index, type names) pairs.
        let candidates = self.typed_candidates_with_signatures(candidates, arg_count);

        let arg_cores: Vec<CoreType> = args
            .iter()
            .map(|arg| {
                let ty = self.dispatch_julia_type_for_value(arg);
                crate::vm::dispatch_binding::runtime_actual_core_type(&ty)
            })
            .collect();
        let best_match = resolve_typed_runtime_core_candidates_with_subtype_fallback(
            &self.struct_hierarchy,
            candidates.iter().filter_map(|(idx, signature)| {
                let has_dict_mismatch = args
                    .iter()
                    .zip(signature.rendered.iter())
                    .any(|(arg, exp)| is_struct_dict_bare_mismatch(arg, exp, &self.struct_heap));
                (!has_dict_mismatch).then_some(runtime_typed_core_candidate(*idx, signature))
            }),
            &arg_cores,
            |actual, bound| self.check_subtype_core(actual, bound),
        );

        let Some((selected_func_index, _)) = best_match else {
            if builtin_id == BuiltinId::Ldiv
                && arg_count == 2
                && store_dict_name.is_none()
                && !ldiv_builtin_fallback_applies_to_value(&args[0])
            {
                let result = self.dynamic_div(&args[1], &args[0])?;
                self.stack.push(result);
                return Ok(DispatchAction::Continue);
            }

            for arg in args {
                self.stack.push(arg);
            }
            self.execute_builtin(builtin_id, arg_count)?;
            if keep_builtin_result && store_dict_name.is_none() {
                let result = self.stack.pop_value()?;
                let _modified_collection = self.stack.pop_value()?;
                self.stack.push(result);
                return Ok(DispatchAction::Continue);
            }
            if store_dict_name.is_some() {
                // `Value::Dict`/`Value::Set` carriers removed (Issues #6731/#6732);
                // a store-dict-result fallback can no longer produce a carrier here.
                let _result = if keep_builtin_result {
                    Some(self.stack.pop_value()?)
                } else {
                    None
                };
                let other = self.stack.pop_value()?;
                return Err(VmError::InternalError(format!(
                    "store-dict fallback unreachable after carrier removal for {}, got {:?}",
                    func_name,
                    crate::vm::util::value_type_name(&other)
                )));
            }
            return Ok(DispatchAction::Continue);
        };

        let func = match self.get_function_cloned_or_raise(selected_func_index)? {
            Some(f) => f,
            None => return Ok(DispatchAction::Continue),
        };

        let mut frame = self.acquire_frame(func.local_slot_count, Some(selected_func_index));
        if let Some(vararg_idx) = func.vararg_param_index {
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: args[vararg_idx..].to_vec(),
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            for (idx, slot) in func.param_slots.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        self.bind_type_params(&func, &args, &mut frame);

        bind_kwargs_defaults(
            &func,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = func.entry;
        Ok(DispatchAction::Continue)
    }

    fn call_function_by_name_with_args(
        &mut self,
        func_name: &str,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let arg_type_names = self.callable_dispatch_type_names(&args);

        // Use function_name_index for O(1) lookup (Issue #3361)
        let indices = self.get_function_indices_by_name(func_name);
        if indices.is_empty() {
            // Fallback: try to dispatch as a builtin function (Issue #2070)
            if let Some(builtin_id) = BuiltinId::from_name(func_name) {
                for arg in args {
                    self.stack.push(arg);
                }
                self.execute_builtin(builtin_id, arg_type_names.len())?;
                return Ok(());
            }
            if let Some(result) = self.try_call_intrinsic(func_name, &args)? {
                self.stack.push(result);
                return Ok(());
            }
            // INTERNAL: function names in composed calls are compiler-assigned or already resolved
            return Err(VmError::InternalError(format!(
                "Function '{}' not found",
                func_name
            )));
        }

        let candidates: Vec<(usize, &FunctionInfo)> = indices
            .iter()
            .map(|&idx| (idx, self.functions[idx].as_ref()))
            .collect();

        let func_index =
            match self.dispatch_function_variable(func_name, &candidates, &arg_type_names) {
                Ok(idx) => idx,
                Err(_) => {
                    if let Some(builtin_id) = BuiltinId::from_name(func_name) {
                        for arg in args {
                            self.stack.push(arg);
                        }
                        self.execute_builtin(builtin_id, arg_type_names.len())?;
                        return Ok(());
                    }
                    if let Some(result) = self.try_call_intrinsic(func_name, &args)? {
                        self.stack.push(result);
                        return Ok(());
                    }
                    return Err(VmError::MethodError(format!(
                        "no method matching {}({})",
                        func_name,
                        arg_type_names.join(", ")
                    )));
                }
            };

        let func = self.get_function_checked(func_index)?.clone();
        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));
        self.bind_args_to_frame(&func, &args, &mut frame)?;

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = func.entry;
        Ok(())
    }

    fn call_closure_with_args(
        &mut self,
        func_name: &str,
        // Issue #5189: borrow the closure's frozen capture set (shared behind an
        // `Rc`) instead of taking it by value, so the per-call path does not
        // deep-clone the whole `Vec<(String, Value)>`.
        captures: &[(String, Value)],
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let arg_type_names = self.callable_dispatch_type_names(&args);

        // Use function_name_index for O(1) lookup (Issue #3361)
        let indices = self.get_function_indices_by_name(func_name);
        if indices.is_empty() {
            // INTERNAL: closure function name is compiler-assigned; function not found is a compiler bug
            return Err(VmError::InternalError(format!(
                "Function '{}' not found",
                func_name
            )));
        }

        let candidates: Vec<(usize, &FunctionInfo)> = indices
            .iter()
            .map(|&idx| (idx, self.functions[idx].as_ref()))
            .collect();

        let func_index =
            self.dispatch_function_variable(func_name, &candidates, &arg_type_names)?;

        let func = self.get_function_checked(func_index)?.clone();
        let mut frame =
            self.acquire_frame_with_captures(func.local_slot_count, Some(func_index), captures);
        self.bind_args_to_frame(&func, &args, &mut frame)?;

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = func.entry;
        Ok(())
    }

    fn bind_args_to_frame(
        &mut self,
        func: &FunctionInfo,
        args: &[Value],
        frame: &mut Frame,
    ) -> Result<(), VmError> {
        self.bind_type_params(func, args, frame);

        if let Some(vararg_idx) = func.vararg_param_index {
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }

            let vararg_tuple = Value::Tuple(TupleValue {
                elements: args[vararg_idx..].to_vec(),
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            for (idx, slot) in func.param_slots.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    bind_value_to_slot(frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        bind_kwargs_defaults(
            func,
            frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        Ok(())
    }

    /// Set up a composed function call: (f ∘ g)(x...) = f(g(x...))
    /// Calls inner function first, saves outer for after inner returns
    /// Supports nested composition: (a ∘ b ∘ c)(x) = a(b(c(x)))
    pub(in crate::vm::exec) fn setup_composed_call(
        &mut self,
        outer: Value,
        inner: Value,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        use super::super::hof_exec::state::ComposedCallState;

        // Flatten the entire composition: collect all pending outers and find the innermost function
        // This handles both right-associative (a ∘ (b ∘ c)) and left-associative ((a ∘ b) ∘ c) forms
        let mut pending_outers = Vec::new();

        // Helper to flatten a Value, adding callable values to pending_outers in reverse call order
        fn flatten_composition(val: Value, outers: &mut Vec<Value>) -> Result<Value, VmError> {
            match val {
                Value::Function(_) | Value::Closure(_) => Ok(val),
                Value::ComposedFunction(cf) => {
                    // First flatten the outer (it will be called after inner)
                    let flattened_outer = flatten_composition(*cf.outer, outers)?;
                    outers.push(flattened_outer);
                    // Then recursively process inner
                    flatten_composition(*cf.inner, outers)
                }
                _ => Err(VmError::TypeError(format!(
                    "Expected Function or Closure in composition, got {:?}",
                    val
                ))),
            }
        }

        // Flatten outer first
        let flattened_outer = flatten_composition(outer, &mut pending_outers)?;
        pending_outers.push(flattened_outer);

        // Flatten inner to get the innermost callable
        let innermost = flatten_composition(inner, &mut pending_outers)?;

        // Save state with all pending outers
        self.composed_call_state = Some(ComposedCallState {
            pending_outers,
            return_ip: self.ip,
            call_frame_depth: self.frames.len(),
        });

        // Call the innermost function/closure with the argument
        match innermost {
            Value::Function(fv) => {
                self.call_function_by_name_with_args(&fv.name, args)?;
            }
            Value::Closure(cv) => {
                self.call_closure_with_args(&cv.name, &cv.captures, args)?;
            }
            _ => {
                // INTERNAL: composed call innermost must be Function or Closure; other type is a compiler bug
                return Err(VmError::InternalError(
                    "Expected Function or Closure as innermost".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Dispatch a function variable call based on runtime argument types.
    /// This fixes Issue #1658 where abstract types like Number incorrectly matched
    /// concrete types like Array because no type checking was done.
    ///
    /// Returns the index of the best matching function, or an error if no method matches.
    pub(super) fn check_type_match(
        &self,
        arg_type_name: &str,
        param_jt: &crate::types::JuliaType,
    ) -> bool {
        // Thin adapter over the shared matcher (Issue #5915): the VM supplies
        // only the declared-struct lookup (Issue #5314 leaf-struct guard) and
        // the engine-backed runtime `<:` authority.
        crate::inference_core::dispatch_resolver::runtime_type_name_matches_param(
            arg_type_name,
            param_jt,
            |param_base| {
                self.struct_defs
                    .iter()
                    .any(|d| d.name.rsplit('.').next().unwrap_or(&d.name) == param_base)
            },
            |arg, param| self.check_subtype(arg, param),
        )
    }

    /// Check if argument type is an exact match (not just a subtype)
    pub(super) fn is_exact_type_match(
        &self,
        arg_type_name: &str,
        param_jt: &crate::types::JuliaType,
    ) -> bool {
        let param_type_name = param_jt.name();
        arg_type_name == param_type_name.as_ref()
    }
}
