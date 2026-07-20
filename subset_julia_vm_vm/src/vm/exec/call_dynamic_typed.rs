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
use super::call::{bind_kwargs_defaults, runtime_type_binding_display};
use super::call_dynamic::generator_iter_type_name;
use super::util::{bind_value_to_slot, is_struct_dict_bare_mismatch, strip_module_prefix};
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

fn native_range_unary_accessor_value(func_name: &str, args: &[Value]) -> Option<Value> {
    if args.len() != 1 {
        return None;
    }
    let Value::Range(range) = &args[0] else {
        return None;
    };
    match strip_module_prefix(func_name) {
        "first" => range.first_value(),
        "last" => range.last_value(),
        "step" => Some(range.typed_step()),
        "length" => Some(range.length_value()),
        _ => None,
    }
}

fn is_native_range_iterate(func_name: &str, args: &[Value]) -> bool {
    matches!(strip_module_prefix(func_name), "iterate")
        && (args.len() == 1 || args.len() == 2)
        && matches!(args.first(), Some(Value::Range(_)))
}

impl<R: RngLike> Vm<R> {
    fn typed_dispatch_has_user_function_name(
        &self,
        fallback_index: Option<usize>,
        candidates: &[usize],
        target: &str,
    ) -> bool {
        let target = strip_module_prefix(target);
        let user_name_matches = |idx: usize| {
            idx >= self.base_function_count
                && self
                    .functions
                    .get(idx)
                    .is_some_and(|func| strip_module_prefix(func.name.as_str()) == target)
        };
        fallback_index.is_some_and(user_name_matches)
            || candidates.iter().any(|idx| user_name_matches(*idx))
    }

    /// Resolve the structured `CallTypedDispatch[OrBuiltin*]` candidate
    /// payload (function indices, Issue #6496) into per-arity runtime
    /// signatures.
    ///
    /// The per-arity signature is derived from each candidate's `FunctionInfo`;
    /// equality with the canonical `MethodSig` projection is pinned by
    /// `base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495`
    /// in `compile/cache.rs`. Results are memoized in
    /// `Vm::typed_signature_cache`: the #8561 call-site inline cache only
    /// covers exact-scalar argument tuples, so every other argument shape
    /// still derives signatures per resolution. Candidates whose signature
    /// cannot be derived for the arity are dropped — the historical
    /// emit-time `runtime_type_names_for_arity` gate never baked them.
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
                    let param_types = crate::vm::expanded_param_types_for_call(func, arity)?;
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

    /// Enter a resolved typed-dispatch target through a cached call-site
    /// method index (Issue #8561).
    ///
    /// Hit-path counterpart of the shared frame-entry tail: fetches the
    /// function and enters its frame. The `Value::DataType` generator
    /// `IteratorSize`/`IteratorEltype` special case is deliberately absent —
    /// `Type{T}` singleton arguments are excluded from the exact scalar
    /// fingerprint, so a cached target can never be reached with a
    /// `DataType` argument.
    fn enter_typed_dispatch_target(
        &mut self,
        selected_func_index: usize,
        args: Vec<Value>,
    ) -> Result<DispatchAction, VmError> {
        let func = match self.get_function_cloned_or_raise(selected_func_index)? {
            Some(f) => f,
            None => return Ok(DispatchAction::Continue),
        };
        self.enter_typed_dispatch_frame(selected_func_index, &func, &args)
    }

    /// Shared frame-entry tail of the `CallTypedDispatch[OrBuiltin*]` family:
    /// bind positional/vararg arguments, `where`-clause type parameters, and
    /// keyword defaults, then push the callee frame.
    fn enter_typed_dispatch_frame(
        &mut self,
        selected_func_index: usize,
        func: &FunctionInfo,
        args: &[Value],
    ) -> Result<DispatchAction, VmError> {
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

        // Bind type parameters from where clauses using the full parameter
        // JuliaType patterns. This is required for signatures like
        // `g(mem, ::Type{S}, n) where S`, where the type parameter is
        // not aligned with the argument index.
        self.bind_type_params(func, args, &mut frame);

        bind_kwargs_defaults(
            func,
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

                let has_user_target = self.typed_dispatch_has_user_function_name(
                    Some(*fallback_index),
                    candidates,
                    _func_name,
                );
                if !has_user_target && is_native_range_iterate(_func_name, &args) {
                    let result = if let Some(state) = args.get(1) {
                        self.iterate_next(&args[0], state)?
                    } else {
                        self.iterate_first(&args[0])?
                    };
                    self.stack.push(result);
                    return Ok(DispatchAction::Continue);
                }

                if !has_user_target {
                    if let Some(value) = native_range_unary_accessor_value(_func_name, &args) {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                }

                // Per-call-site inline cache over the exact scalar argument
                // fingerprint (Issue #8561). Only argument tuples whose
                // dispatch identity is fully captured by the value tags
                // participate (see `exact_call_site_fingerprint`); parametric
                // structs, `Type{T}` singletons, functions, and containers
                // fingerprint to `None` and always take the resolver below,
                // so the post-selection `Value::DataType` generator special
                // case can never be reached through a cached target. Stale
                // generations (method-table mutations) miss; see
                // `note_method_table_mutation`.
                let call_site_ip = self.ip - 1;
                let arg_fingerprint = {
                    let arg_refs: Vec<&Value> = args.iter().collect();
                    self.call_site_arg_fingerprints(&arg_refs)
                };
                if let Some(cached) = arg_fingerprint
                    .as_deref()
                    .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp))
                {
                    if self.typed_dispatch_candidate_origin_compatible(cached, &args) {
                        return self.enter_typed_dispatch_target(cached, args);
                    }
                }

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
                        if *idx != usize::MAX
                            && !self.typed_dispatch_candidate_origin_compatible(*idx, &args)
                        {
                            return None;
                        }
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
                                    if !self.typed_dispatch_candidate_origin_compatible(idx, &args)
                                    {
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
                if !self.typed_dispatch_candidate_origin_compatible(selected_func_index, &args) {
                    let arg_types = args
                        .iter()
                        .map(|arg| self.get_type_name(arg))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.raise(VmError::MethodError(format!(
                        "no method matching {}({})",
                        _func_name, arg_types
                    )))?;
                    return Ok(DispatchAction::Continue);
                }

                // Cache only a target whose full Julia signature, including
                // nominal type origin, accepts the runtime arguments.
                self.store_call_site_inline_cache(
                    call_site_ip,
                    arg_fingerprint.as_deref(),
                    selected_func_index,
                );

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

                self.enter_typed_dispatch_frame(selected_func_index, &func, &args)
            }

            Instr::CallParametricConstructorDispatch(ref operands) => {
                self.execute_call_parametric_constructor_dispatch(operands)
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

    /// Execute `Instr::CallParametricConstructorDispatch` (Issue #10971):
    /// runtime candidate selection over an explicit-parametric constructor
    /// family by value signature, then per-candidate `where`-binder
    /// installation into the selected frame.
    ///
    /// Stack layout: `[args..., type_arg_values...]` — the (possibly empty)
    /// runtime type-argument values are popped first (they were pushed last,
    /// mirroring `CallStaticParametric::runtime_binding_names`), then the
    /// positional arguments select the candidate by value signature (the same
    /// value-based multiple-dispatch resolver every other runtime-callable
    /// path uses), and finally the *selected candidate's own* binder
    /// names/bindings are installed into its frame — different candidates may
    /// name their self type parameter differently (`where T` vs `where S`).
    fn execute_call_parametric_constructor_dispatch(
        &mut self,
        operands: &crate::bytecode::ParametricConstructorDispatchCall,
    ) -> Result<DispatchAction, VmError> {
        // Runtime type-argument values are pushed above the positional args,
        // so they pop first (mirrors `CallStaticParametric`, Issue #10998).
        let mut type_arg_values = Vec::with_capacity(operands.type_arg_value_count);
        for _ in 0..operands.type_arg_value_count {
            type_arg_values.push(self.stack.pop_value()?);
        }
        type_arg_values.reverse();

        let mut args = Vec::with_capacity(operands.arg_count);
        for _ in 0..operands.arg_count {
            args.push(self.stack.pop_value()?);
        }
        args.reverse();

        let candidate_indices: Vec<usize> =
            operands.candidates.iter().map(|c| c.func_index).collect();
        let selected_func_index =
            match self.find_best_method_index_from_candidates(&candidate_indices, &args)? {
                Some(idx) => idx,
                None => {
                    let arg_types = args
                        .iter()
                        .map(|arg| self.dispatch_julia_type_for_value(arg).to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let type_arg_display = if type_arg_values.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "{{{}}}",
                            type_arg_values
                                .iter()
                                .map(runtime_type_binding_display)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    self.raise(VmError::MethodError(format!(
                        "no method matching {}{}({})",
                        operands.base_name, type_arg_display, arg_types
                    )))?;
                    return Ok(DispatchAction::Continue);
                }
            };

        let Some(candidate) = operands
            .candidates
            .iter()
            .find(|c| c.func_index == selected_func_index)
        else {
            // INTERNAL: the selected index always comes from `candidate_indices`,
            // which is derived directly from `operands.candidates`.
            return Err(VmError::InternalError(format!(
                "CallParametricConstructorDispatch selected function index {} \
                 not present in its own candidate list",
                selected_func_index
            )));
        };

        // Zip this candidate's own binder names against the shared runtime
        // type-argument values positionally — the values are the same for
        // every candidate (the source type application is evaluated once);
        // only the binder NAME each candidate installs them under differs.
        let runtime_bindings: Vec<(String, Value)> = candidate
            .runtime_binding_names
            .iter()
            .cloned()
            .zip(type_arg_values.iter().cloned())
            .collect();

        let func = match self.get_function_cloned_or_raise(selected_func_index)? {
            Some(f) => f,
            None => return Ok(DispatchAction::Continue),
        };

        self.execute_direct_call_with_func_args_and_static_bindings(
            selected_func_index,
            func,
            &args,
            false,
            &candidate.bindings,
            false,
            true,
            None,
            &runtime_bindings,
        )
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

        let has_user_target =
            self.typed_dispatch_has_user_function_name(None, candidates, func_name);
        if !has_user_target && is_native_range_iterate(func_name, &args) {
            let result = if let Some(state) = args.get(1) {
                self.iterate_next(&args[0], state)?
            } else {
                self.iterate_first(&args[0])?
            };
            self.stack.push(result);
            return Ok(DispatchAction::Continue);
        }

        if !has_user_target {
            if let Some(value) = native_range_unary_accessor_value(func_name, &args) {
                self.stack.push(value);
                return Ok(DispatchAction::Continue);
            }
        }

        // Per-call-site inline cache over the exact scalar argument
        // fingerprint (Issue #8561). `usize::MAX` is the negative sentinel:
        // "no method candidate matched, take the builtin fallback below" —
        // that branch depends only on the (fingerprinted) argument types and
        // the per-instruction operands, so replaying it from a cached
        // sentinel is equivalent to re-resolving. Non-taggable arguments
        // (structs, `Type{T}`, containers, functions) fingerprint to `None`
        // and always re-resolve.
        let call_site_ip = self.ip - 1;
        let arg_fingerprint = {
            let arg_refs: Vec<&Value> = args.iter().collect();
            self.call_site_arg_fingerprints(&arg_refs)
        };
        let cached_index = arg_fingerprint
            .as_deref()
            .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp));

        let resolved_index = if let Some(cached) = cached_index {
            (cached != usize::MAX).then_some(cached)
        } else {
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
                    if *idx != usize::MAX
                        && !self.typed_dispatch_candidate_origin_compatible(*idx, &args)
                    {
                        return None;
                    }
                    let has_dict_mismatch =
                        args.iter()
                            .zip(signature.rendered.iter())
                            .any(|(arg, exp)| {
                                is_struct_dict_bare_mismatch(arg, exp, &self.struct_heap)
                            });
                    (!has_dict_mismatch).then_some(runtime_typed_core_candidate(*idx, signature))
                }),
                &arg_cores,
                |actual, bound| self.check_subtype_core(actual, bound),
            );
            let resolved = best_match.map(|(idx, _)| idx);
            self.store_call_site_inline_cache(
                call_site_ip,
                arg_fingerprint.as_deref(),
                resolved.unwrap_or(usize::MAX),
            );
            resolved
        };

        let Some(selected_func_index) = resolved_index else {
            return self.execute_typed_dispatch_builtin_fallback(
                builtin_id,
                func_name,
                arg_count,
                args,
                store_dict_name,
                keep_builtin_result,
            );
        };

        let func = match self.get_function_cloned_or_raise(selected_func_index)? {
            Some(f) => f,
            None => return Ok(DispatchAction::Continue),
        };
        if let Some(param_types) = crate::vm::expanded_param_types_for_call(&func, arg_count) {
            if self
                .function_candidate_binding_count(
                    selected_func_index,
                    &args,
                    &param_types,
                    &func.type_params,
                )
                .is_none()
            {
                // Fail closed for `CallTypedDispatchOrBuiltin`: if the structured
                // resolver or a cached candidate payload picks a method whose full
                // Julia signature does not match the runtime values, the builtin
                // fallback remains authoritative (Issue #10782).
                return self.execute_typed_dispatch_builtin_fallback(
                    builtin_id,
                    func_name,
                    arg_count,
                    args,
                    store_dict_name,
                    keep_builtin_result,
                );
            }
        } else {
            return self.execute_typed_dispatch_builtin_fallback(
                builtin_id,
                func_name,
                arg_count,
                args,
                store_dict_name,
                keep_builtin_result,
            );
        }

        self.enter_typed_dispatch_frame(selected_func_index, &func, &args)
    }

    fn typed_dispatch_candidate_origin_compatible(
        &self,
        function_index: usize,
        args: &[Value],
    ) -> bool {
        let Some(func) = self.functions.get(function_index) else {
            return false;
        };
        crate::vm::expanded_param_types_for_call(func, args.len()).is_some_and(|param_types| {
            !self.function_candidate_has_nominal_origin_conflict(
                function_index,
                args,
                &param_types,
                &func.type_params,
            )
        })
    }

    fn execute_typed_dispatch_builtin_fallback(
        &mut self,
        builtin_id: BuiltinId,
        func_name: &str,
        arg_count: usize,
        args: Vec<Value>,
        store_dict_name: Option<&str>,
        keep_builtin_result: bool,
    ) -> Result<DispatchAction, VmError> {
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
        Ok(DispatchAction::Continue)
    }

    fn call_function_value_with_args(
        &mut self,
        function: &FunctionValue,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let func_name = &function.name;
        let arg_type_names = self.callable_dispatch_type_names(&args);

        // A FunctionValue may carry a frozen private-helper candidate family.
        // Preserve that authority boundary instead of rediscovering only the
        // public generic by spelling (#9784).
        let function_value = Value::Function(function.clone());
        let candidates = self.collect_runtime_callable_candidates(&function_value, func_name)?;
        if candidates.is_empty() {
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

        let func_index = match self.dispatch_function_variable_for_values(
            func_name,
            &candidates,
            &arg_type_names,
            &args,
        ) {
            Ok(Some(idx)) => idx,
            Ok(None) => {
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
            Err(error) => return Err(error),
        };

        let func = self.get_function_checked(func_index)?.clone();
        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));
        self.bind_args_to_frame(&func, &args, &mut frame)?;

        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = func.entry;
        Ok(())
    }

    fn call_closure_value_with_args(
        &mut self,
        closure: &ClosureValue,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        let func_name = &closure.name;
        let arg_type_names = self.callable_dispatch_type_names(&args);

        let closure_value = Value::Closure(closure.clone());
        let candidates = self.collect_runtime_callable_candidates(&closure_value, func_name)?;
        if candidates.is_empty() {
            // INTERNAL: closure function name is compiler-assigned; function not found is a compiler bug
            return Err(VmError::InternalError(format!(
                "Function '{}' not found",
                func_name
            )));
        }

        let func_index = self
            .dispatch_function_variable_for_values(func_name, &candidates, &arg_type_names, &args)?
            .ok_or_else(|| {
                VmError::MethodError(format!(
                    "no method matching {}({})",
                    func_name,
                    arg_type_names.join(", ")
                ))
            })?;

        let func = self.get_function_checked(func_index)?.clone();
        let mut frame = self.acquire_frame_with_captures(
            func.local_slot_count,
            Some(func_index),
            &closure.captures,
        );
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
                // Upstream: composing with a non-callable value raises the same
                // `MethodError: objects of type T are not callable` as calling
                // it directly — not a TypeError (Issue #11146). This is a
                // nested `fn` with no `&self`, so it uses the static value type
                // name rather than `Vm::get_type_name`.
                _ => Err(VmError::MethodError(format!(
                    "objects of type {} are not callable",
                    crate::vm::util::value_type_name(&val)
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
                self.call_function_value_with_args(&fv, args)?;
            }
            Value::Closure(cv) => {
                self.call_closure_value_with_args(&cv, args)?;
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
