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
use super::call::bind_kwargs_defaults;
use super::util::{
    bind_value_to_slot, extract_base_type, is_rust_dict_parametric_mismatch,
    is_struct_dict_bare_mismatch, strip_module_prefix,
};
use super::DispatchAction;
use crate::builtins::BuiltinId;
use crate::inference_core::dispatch_resolver::{
    resolve_runtime_core_signature_candidates,
    resolve_runtime_core_signature_slice_candidates_with_family_fallback, RuntimeCoreCandidate,
    RuntimeCoreSliceCandidate,
};
use crate::inference_core::selection;
use crate::inference_core::CoreType;
use crate::rng::RngLike;
use crate::types::JuliaType;
use crate::vm::intrinsics_exec::apply_unary_rounding_op_with_heap;
use crate::vm::value::{
    native_array_value_ref, GeneratorCallable, GeneratorValue, RustBigFloat, StructInstance,
};

fn matches_native_collect_iterator(struct_name: &str) -> bool {
    matches!(
        struct_name,
        "Zip" | "Zip3" | "Zip4" | "Zip5" | "Zip6" | "Zip7"
    ) || struct_name.starts_with("Zip{")
        || struct_name.starts_with("Zip3{")
        || struct_name.starts_with("Zip4{")
        || struct_name.starts_with("Zip5{")
        || struct_name.starts_with("Zip6{")
        || struct_name.starts_with("Zip7{")
}

fn uses_builtin_iterate_for_struct(struct_name: &str) -> bool {
    struct_name == "CartesianIndices" || struct_name == "Array" || struct_name.starts_with("Array{")
}

fn is_array_wrapper_value(value: &Value, struct_heap: &[StructInstance]) -> bool {
    match value {
        Value::Struct(s) => s.array_wrapper_julia_type().is_some(),
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .and_then(StructInstance::array_wrapper_julia_type)
            .is_some(),
        _ => false,
    }
}

fn can_score_iterate_dynamic_candidates(value: &Value) -> bool {
    // Structs are always scored against the full candidate set so Base
    // iterators (Zip, SubArray, ...) and user iterators both win on merit.
    //
    // Native arrays have a dedicated VM iterator, so historically they bypassed
    // candidate scoring entirely. But that made `iterate(::Any)` unable to reach
    // a user-defined `iterate(::Vector{Int64})` method, unlike the `collect`
    // CallDynamic path (Issue #6638). Native arrays are now scored too, but only
    // against *user-defined* candidates (see `scored_iterate_candidates`): there
    // are no Base `iterate` methods over Array/Vector, so the VM builtin iterator
    // still runs whenever the user has not explicitly overridden it (Issue #5584).
    // Native-array carrier check goes through the shared destructure helper
    // rather than a direct carrier-variant match (Issue #6806).
    matches!(value, Value::Struct(_) | Value::StructRef(_)) || is_native_array_value(value)
}

/// The candidate index subset to score for an `IterateDynamic` collection.
///
/// Structs score against the full candidate set; native arrays score only
/// against user-defined candidates (`idx >= base_function_count`) so the
/// dedicated VM array iterator is overridden only by an explicit user
/// `iterate(::Vector{...})` method, never by loosely matching a Base struct
/// iterator (Issue #6638 / #5584).
fn scored_iterate_candidates(
    coll: &Value,
    candidates: &[usize],
    base_function_count: usize,
    struct_heap: &[StructInstance],
) -> Vec<usize> {
    if is_native_array_value(coll) || is_array_wrapper_value(coll, struct_heap) {
        candidates
            .iter()
            .copied()
            .filter(|idx| *idx >= base_function_count)
            .collect()
    } else {
        candidates.to_vec()
    }
}

fn is_native_range_candidate_mismatch(arg: &Value, expected_type: &str) -> bool {
    let Value::Range(range) = arg else {
        return false;
    };

    // Char ranges are always `StepRange{Char, Int64}` in upstream
    // Julia regardless of step value (`:` over non-numeric types
    // defaults to the explicit-step form). Force StepRange to match
    // and UnitRange to mismatch so `show`/`isa`/method dispatch over
    // a Char range routes to the StepRange arm (Issue #4830, paired
    // with the typeof guard in `value_enum.rs::runtime_type`).
    let is_char_range = matches!(range.element_type, crate::vm::value::RangeElementType::Char);

    let base_name = strip_module_prefix(extract_base_type(expected_type));
    match base_name {
        "AbstractRange" => false,
        "AbstractUnitRange" | "UnitRange" => is_char_range || !range.is_unit_range(),
        "StepRange" => !is_char_range && range.is_unit_range(),
        "StepRangeLen" | "LinRange" | "OneTo" | "LogRange" => true,
        _ => false,
    }
}

fn dynamic_dispatch_type_name(value: &Value, fallback: &str) -> String {
    match value {
        Value::DataType(jt) => format!("Type{{{}}}", jt.name()),
        _ => fallback.to_string(),
    }
}

/// Same-family fallback for the CallDynamic / IterateDynamic structured
/// resolvers: a legacy wrapper sentinel (e.g. native-iterator `Generator`) and
/// the actual wrapper struct share a family when their bare nominal names match
/// (module prefix + parametric `{...}` stripped). This compares the structured
/// `core_signature` family name directly via [`CoreType::nominal_family_name`]
/// instead of rendering each type back to a Julia name string and re-parsing it
/// (Issue #6593). The `expected` side is always a bare `Struct`/`Named`
/// candidate (gated by `core_type_allows_family_fallback`), so a non-nominal
/// `actual` simply has no family to match.
fn runtime_core_family_fallback_matches(actual: &CoreType, expected: &CoreType) -> bool {
    match (actual.nominal_family_name(), expected.nominal_family_name()) {
        (Some(actual_family), Some(expected_family)) => actual_family == expected_family,
        _ => false,
    }
}

fn native_array_rank_count(iter: &Value) -> Option<(usize, usize, bool)> {
    let arr_ref = native_array_value_ref(iter)?;
    let arr = arr_ref.borrow();
    Some((arr.shape.len(), arr.element_count(), arr.shape.is_empty()))
}

fn generator_iter_known_nonempty(iter: &Value) -> bool {
    if let Some((_rank, count, shape_is_empty)) = native_array_rank_count(iter) {
        return !shape_is_empty && count > 0;
    }

    match iter {
        Value::Memory(mem) => !mem.borrow().is_empty(),
        Value::Range(r) => {
            if r.step == 0.0 {
                false
            } else if r.step > 0.0 {
                r.stop >= r.start
            } else {
                r.start >= r.stop
            }
        }
        _ => false,
    }
}

fn top_level_generic_args(type_name: &str, prefix: &str) -> Option<Vec<String>> {
    let inner = type_name
        .strip_prefix(prefix)?
        .strip_prefix('{')?
        .strip_suffix('}')?;
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => depth = depth.saturating_add(1),
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(inner[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    Some(args)
}

pub(super) fn generator_iter_type_name(julia_type: &JuliaType) -> Option<String> {
    let JuliaType::Struct(name) = julia_type else {
        return None;
    };
    top_level_generic_args(name, "Base.Generator")
        .or_else(|| top_level_generic_args(name, "Generator"))
        .and_then(|args| args.into_iter().next())
}

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

impl<R: RngLike> Vm<R> {
    /// Expected first-parameter type name for a `CallDynamic` method
    /// candidate, derived from the candidate's `FunctionInfo` (Issue #6496).
    /// Candidates are emitted only for single-parameter methods, and the
    /// Issue #6496 parity gates pin this rendering against the historical
    /// compile-time baked string.
    fn dynamic_candidate_expected_type_name(&self, func_index: usize) -> String {
        self.functions
            .get(func_index)
            .and_then(|func| func.param_julia_types.first())
            .map(ToString::to_string)
            .unwrap_or_else(|| "Any".to_string())
    }

    /// Structured counterpart of
    /// [`Self::dynamic_candidate_expected_type_name`] (Issue #6502 slice 2):
    /// the rendered first-parameter name (kept for the VM representation
    /// fences) plus the per-slot `core_signature` projection and, for
    /// `where`-parametric methods, the full signature gate (Issue #6536).
    fn dynamic_candidate_expected_signature(
        &self,
        func_index: usize,
    ) -> (String, CoreType, Option<CoreType>) {
        let Some(jt) = self
            .functions
            .get(func_index)
            .and_then(|func| func.param_julia_types.first())
        else {
            return ("Any".to_string(), CoreType::Any, None);
        };
        let type_params = self
            .functions
            .get(func_index)
            .map(|func| func.type_params.as_slice())
            .unwrap_or(&[]);
        let signature = crate::vm::dispatch_binding::build_runtime_candidate_core_signature(
            std::slice::from_ref(jt),
            type_params,
        );
        let rendered = signature
            .rendered
            .into_iter()
            .next()
            .unwrap_or_else(|| "Any".to_string());
        let slot = signature.slots.into_iter().next().unwrap_or(CoreType::Any);
        (rendered, slot, signature.signature)
    }

    /// Project a structured [`DynamicCallCandidate`] onto the runtime
    /// `core_signature` fallback shape; native-iterator sentinels keep their
    /// `usize::MAX` index and carry their legacy family name as a `CoreType`.
    fn resolve_dynamic_call_candidate_signature(
        &self,
        candidate: DynamicCallCandidate,
    ) -> (usize, String, CoreType, Option<CoreType>) {
        match candidate {
            DynamicCallCandidate::Method(idx) => {
                let (rendered, slot, gate) = self.dynamic_candidate_expected_signature(idx);
                (idx, rendered, slot, gate)
            }
            DynamicCallCandidate::NativeIterator(kind) => {
                let rendered = kind.type_name().to_string();
                let slot = CoreType::from_julia_name(&rendered);
                (usize::MAX, rendered, slot, None)
            }
        }
    }

    fn has_range_collect_candidate(&self, candidates: &[DynamicCallCandidate]) -> bool {
        candidates.iter().any(|candidate| {
            let DynamicCallCandidate::Method(idx) = candidate else {
                return false;
            };
            let name = self.dynamic_candidate_expected_type_name(*idx);
            let base_name = strip_module_prefix(extract_base_type(&name));
            matches!(base_name, "UnitRange" | "StepRange" | "AbstractRange")
        })
    }

    fn has_generator_collect_candidate(&self, candidates: &[DynamicCallCandidate]) -> bool {
        candidates.iter().any(|candidate| {
            let DynamicCallCandidate::Method(idx) = candidate else {
                return false;
            };
            let name = self.dynamic_candidate_expected_type_name(*idx);
            strip_module_prefix(extract_base_type(&name)) == "Generator"
        })
    }

    fn generator_can_use_generic_collect(
        &self,
        generator: &GeneratorValue,
    ) -> Result<bool, VmError> {
        let GeneratorCallable::FunctionIndex(func_index) = &generator.callable else {
            return Ok(false);
        };
        let func = self.get_function_checked(*func_index)?;
        let first = func.name.chars().next();
        Ok(first.is_some_and(|ch| ch.is_lowercase())
            && generator_iter_known_nonempty(generator.iter.as_ref()))
    }

    pub(super) fn zero_field_struct_value(&mut self, struct_name: &str) -> Result<Value, VmError> {
        let base_name = strip_module_prefix(extract_base_type(struct_name));
        let type_id = self
            .struct_defs
            .iter()
            .position(|def| {
                def.name == struct_name
                    || strip_module_prefix(extract_base_type(&def.name)) == base_name
            })
            .ok_or_else(|| VmError::TypeError(format!("{base_name} type is not loaded")))?;
        let idx = self.struct_heap.len();
        self.struct_heap.push(StructInstance::with_name(
            type_id,
            struct_name.to_string(),
            Vec::new(),
        ));
        Ok(Value::StructRef(idx))
    }

    fn iterator_size_value_for_native_generator_iter(
        &mut self,
        iter: &Value,
    ) -> Result<Value, VmError> {
        if let Some((rank, _count, _shape_is_empty)) = native_array_rank_count(iter) {
            return if (1..=8).contains(&rank) {
                self.zero_field_struct_value(&format!("HasShape{{{rank}}}"))
            } else {
                self.zero_field_struct_value("HasLength")
            };
        }

        match iter {
            Value::Memory(_) | Value::Range(_) => self.zero_field_struct_value("HasShape{1}"),
            Value::Tuple(_) | Value::Str(_) => self.zero_field_struct_value("HasLength"),
            Value::Generator(g) => {
                let iter = (*g.iter).clone();
                self.iterator_size_value_for_native_generator_iter(&iter)
            }
            _ => self.zero_field_struct_value("HasLength"),
        }
    }

    pub(super) fn iterator_size_value_for_generator_iter_type_name(
        &mut self,
        iter_type: &str,
    ) -> Result<Value, VmError> {
        let base_name = strip_module_prefix(extract_base_type(iter_type));
        match base_name {
            "Vector" | "UnitRange" | "StepRange" | "Memory" => {
                self.zero_field_struct_value("HasShape{1}")
            }
            "Matrix" => self.zero_field_struct_value("HasShape{2}"),
            "Array" => {
                let rank = top_level_generic_args(iter_type, "Array")
                    .and_then(|args| args.get(1).and_then(|rank| rank.parse::<usize>().ok()))
                    .or_else(|| {
                        top_level_generic_args(iter_type, "Base.Array").and_then(|args| {
                            args.get(1).and_then(|rank| rank.parse::<usize>().ok())
                        })
                    });
                if let Some(rank) = rank.filter(|rank| (1..=8).contains(rank)) {
                    self.zero_field_struct_value(&format!("HasShape{{{rank}}}"))
                } else {
                    self.zero_field_struct_value("HasLength")
                }
            }
            "Tuple" | "String" => self.zero_field_struct_value("HasLength"),
            "Generator" => {
                if let Some(inner_iter) = top_level_generic_args(iter_type, "Base.Generator")
                    .or_else(|| top_level_generic_args(iter_type, "Generator"))
                    .and_then(|args| args.into_iter().next())
                {
                    self.iterator_size_value_for_generator_iter_type_name(&inner_iter)
                } else {
                    self.zero_field_struct_value("SizeUnknown")
                }
            }
            _ => self.zero_field_struct_value("HasLength"),
        }
    }

    /// Whether `arg` cannot satisfy a candidate's `expected_type` slot because of
    /// a container-shape mismatch. Extracted from the dynamic-dispatch candidate
    /// filter to keep it flat (Issue #6833).
    fn dynamic_candidate_arg_mismatch(&self, arg: &Value, expected_type: &str) -> bool {
        is_rust_dict_parametric_mismatch(arg, expected_type)
            || is_native_range_candidate_mismatch(arg, expected_type)
            || is_struct_dict_bare_mismatch(arg, expected_type, &self.struct_heap)
    }

    /// User-defined subset of `indices` (function index past the Base allotment).
    /// Tier-1 of the metadata-backed dynamic selection (Issue #6833 flatten).
    fn user_metadata_candidate_indices(&self, indices: &[usize]) -> Vec<usize> {
        indices
            .iter()
            .copied()
            .filter(|idx| *idx >= self.base_function_count)
            .collect()
    }

    /// Base-only allowlist subset of `indices`: Base-program functions named
    /// `empty`. Tier-2 of the metadata-backed dynamic selection (Issue #6833).
    fn base_empty_metadata_candidate_indices(&self, indices: &[usize]) -> Vec<usize> {
        indices
            .iter()
            .copied()
            .filter(|idx| {
                self.is_base_program_function_index(*idx)
                    && self.functions.get(*idx).is_some_and(|func| {
                        matches!(
                            func.name.strip_prefix("Base.").unwrap_or(&func.name),
                            "empty"
                        )
                    })
            })
            .collect()
    }

    /// Tier-dispatch fallback over the metadata-filtered candidates: resolve the
    /// `(slot, gate)` signatures via the family-fallback matcher, falling back to
    /// `fallback_func_index`. Extracted from `execute_call_dynamic` (Issue #6833).
    fn resolve_tier_filtered_fallback(
        &self,
        filtered_candidates: &[(usize, &str, &CoreType, Option<&CoreType>)],
        actual_cores: &[CoreType],
        fallback_func_index: usize,
    ) -> usize {
        resolve_runtime_core_signature_slice_candidates_with_family_fallback(
            &self.struct_hierarchy,
            filtered_candidates
                .iter()
                .map(|(idx, _, slot, gate)| RuntimeCoreSliceCandidate {
                    idx: *idx,
                    slots: std::slice::from_ref(*slot),
                    signature: *gate,
                }),
            actual_cores,
            runtime_core_family_fallback_matches,
            |actual, expected| self.check_subtype_core(actual, expected),
        )
        .map(|(idx, _score)| idx)
        .unwrap_or(fallback_func_index)
    }

    /// Scored-dispatch fallback: derive each scored candidate's per-arity core
    /// signature from its `FunctionInfo` and resolve via the family-fallback
    /// matcher (Issues #6336/#6502). Extracted from `execute_call_dynamic` to
    /// keep it flat (Issue #6833).
    fn resolve_scored_family_fallback(
        &self,
        scored: &[usize],
        actual_cores: &[CoreType],
    ) -> Option<usize> {
        let derived_signatures: Vec<(
            usize,
            crate::vm::dispatch_binding::RuntimeCandidateCoreSignature,
        )> = scored
            .iter()
            .filter_map(|&idx| {
                let func = self.functions.get(idx)?;
                let param_types = expanded_param_types_for_call(func, actual_cores.len())?;
                let signature = crate::vm::dispatch_binding::build_runtime_candidate_core_signature(
                    &param_types,
                    &func.type_params,
                );
                Some((idx, signature))
            })
            .collect();
        resolve_runtime_core_signature_slice_candidates_with_family_fallback(
            &self.struct_hierarchy,
            derived_signatures
                .iter()
                .map(|(idx, signature)| RuntimeCoreSliceCandidate {
                    idx: *idx,
                    slots: signature.slots.as_slice(),
                    signature: signature.signature.as_ref(),
                }),
            actual_cores,
            runtime_core_family_fallback_matches,
            |actual, expected| self.check_subtype_core(actual, expected),
        )
        .map(|(func_index, _score)| func_index)
    }

    /// Execute dynamic dispatch call instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not a dynamic call operation.
    /// Delegates to specialized handlers for binary, typed, and function variable dispatch.
    #[inline]
    pub(super) fn execute_call_dynamic(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        // Try specialized handlers first
        match instr {
            Instr::CallDynamicBinary(..)
            | Instr::CallDynamicBinaryBoth(..)
            | Instr::CallDynamicBinaryNoFallback(..) => {
                return self.execute_call_dynamic_binary(instr);
            }
            Instr::CallTypedDispatch(..)
            | Instr::CallTypedDispatchOrBuiltin(..)
            | Instr::CallTypedDispatchOrBuiltinResult(..)
            | Instr::CallTypedDispatchOrBuiltinStoreDict(..)
            | Instr::CallTypedDispatchOrBuiltinStoreDictResult(..)
            | Instr::CallTypeConstructor => {
                return self.execute_call_dynamic_typed(instr);
            }
            Instr::CallGlobalRef(..)
            | Instr::CallFunctionVariable(..)
            | Instr::InvokeFunctionVariable(..)
            | Instr::InvokeFunctionVariableWithKwargs(..)
            | Instr::InvokeFunctionVariableDynamicSignature(..)
            | Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(..)
            | Instr::CallFunctionVariableWithSplat(..)
            | Instr::CallFunctionVariableWithKwargsSplat(..) => {
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

                // `collect(x::Any)` uses this CallDynamic path with native
                // candidates for VM-backed containers. Generators are not Pure
                // Julia structs, so keep their representation boundary before
                // normal candidate scoring. Struct-backed iterators such as Zip
                // are scored first so user/Pure Julia methods can win before
                // the native collect compatibility sentinel.
                if *arg_count == 1
                    && candidates.contains(&DynamicCallCandidate::NativeIterator(
                        NativeIteratorKind::Generator,
                    ))
                {
                    if !self.has_generator_collect_candidate(candidates) {
                        if let Value::Generator(g) = &args[0] {
                            if self.generator_can_use_generic_collect(g)? {
                                self.start_function_call(*fallback_func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            // CollectFallback: runtime-generator-pre-score-boundary
                            let iter = (*g.iter).clone();
                            if let Some(result) = self.collect_generator(
                                g.callable.clone(),
                                &iter,
                                g.result_element_type.clone(),
                            )? {
                                self.stack.push(result);
                            }
                            return Ok(DispatchAction::Continue);
                        }
                    }

                    if matches!(args[0], Value::Range(_))
                        && !self.has_range_collect_candidate(candidates)
                    {
                        // CollectFallback: runtime-range-pre-score-boundary
                        let result = self.collect_iterator(&args[0])?;
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }

                    // Issue #5196: `Core.SimpleVector` (svec) collects to a
                    // `Vector{Any}` preserving heterogeneous elements. Route it
                    // through the native `collect_iterator` boundary directly so
                    // it never enters the Pure Julia `_collect` element-type
                    // widening path (which mis-coerces type-object elements such
                    // as `Tuple{Int,String}.parameters`).
                    if matches!(args[0], Value::SimpleVector(_)) {
                        // CollectFallback: runtime-simplevector-pre-score-boundary
                        let result = self.collect_iterator(&args[0])?;
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                }

                // Dispatch based on the full argument type tuple. Candidate
                // pre-filtering still uses the first slot for representation
                // boundary checks, but the cache key must include every
                // dispatch argument so one call site can alternate between
                // `f(dest, ::Broadcasted)` and `f(dest, ::Float64)` safely
                // (Issue #8368).
                let selected_func_index = if *arg_count >= 1 {
                    let call_site_ip = self.ip - 1;
                    let arg_refs: Vec<&Value> = args.iter().collect();
                    let arg_fingerprint = self.call_site_arg_fingerprints(&arg_refs);

                    if let Some(cached) = arg_fingerprint
                        .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp))
                    {
                        cached
                    } else {
                        let actual_type_names_owned: Vec<_> = args
                            .iter()
                            .map(|arg| {
                                let raw_arg_type_name = self.get_type_name(arg);
                                dynamic_dispatch_type_name(arg, &raw_arg_type_name)
                            })
                            .collect();

                        // Check dispatch cache first (Issue #2943, #3355)
                        let type_hash = hash_type_name(&actual_type_names_owned.join("\u{1f}"));
                        if let Some(cached) =
                            self.lookup_call_site_dispatch_cache(call_site_ip, type_hash)
                        {
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint,
                                cached,
                            );
                            cached
                        } else {
                            let actual_cores: Vec<_> = args
                                .iter()
                                .map(|arg| {
                                    let ty = self.dispatch_julia_type_for_value(arg);
                                    crate::vm::dispatch_binding::runtime_actual_core_type(&ty)
                                })
                                .collect();
                            let fallback_actual_core =
                                [actual_cores.first().cloned().unwrap_or(CoreType::Any)];
                            // Scored dispatch: prefer the FunctionInfo-backed VM
                            // resolver so runtime `where` bounds remain available
                            // for cases such as `Type{T}` / `Vector{T}` methods
                            // reached through an `Any` container (Issue #6202).
                            // Keep the string-pattern resolver as fallback for
                            // sentinel/native candidates and legacy projections.
                            // The payload carries only structured candidates
                            // (Issue #6496); the expected type name pairs are
                            // derived here, once per call site + argument type
                            // (the result is dispatch-cached below).
                            let named_candidates: Vec<(usize, String, CoreType, Option<CoreType>)> =
                                candidates
                                    .iter()
                                    .map(|candidate| {
                                        self.resolve_dynamic_call_candidate_signature(*candidate)
                                    })
                                    .collect();
                            let filtered_candidates: Vec<_> = named_candidates
                                .iter()
                                .filter_map(|(idx, expected_type, slot, gate)| {
                                    if self.dynamic_candidate_arg_mismatch(&args[0], expected_type)
                                    {
                                        return None;
                                    }
                                    Some((*idx, expected_type.as_str(), slot, gate.as_ref()))
                                })
                                .collect();
                            let metadata_candidate_indices: Vec<_> = filtered_candidates
                                .iter()
                                .filter_map(|(idx, _, _, _)| (*idx != usize::MAX).then_some(*idx))
                                .collect();
                            // Metadata-backed selection tiers, narrowing the
                            // candidate index list: all candidates → user-defined
                            // only → Base-only allowlist (`empty`). The ordered
                            // first-winner control flow is owned by the shared
                            // selection core (`selection::pick_first_tier`,
                            // Issue #6502); each tier's index list is still built
                            // lazily only when the previous tier found nothing.
                            let tier_pick = selection::pick_first_tier(3, |tier| match tier {
                                0 => self.find_best_method_index_from_candidates(
                                    &metadata_candidate_indices,
                                    &args,
                                ),
                                1 => {
                                    let user = self.user_metadata_candidate_indices(
                                        &metadata_candidate_indices,
                                    );
                                    self.find_best_method_index_from_candidates(&user, &args)
                                }
                                _ => {
                                    let base = self.base_empty_metadata_candidate_indices(
                                        &metadata_candidate_indices,
                                    );
                                    self.find_best_method_index_from_candidates(&base, &args)
                                }
                            });
                            let result = match tier_pick {
                                Ok(Some(idx)) => idx,
                                Ok(None) => self.resolve_tier_filtered_fallback(
                                    &filtered_candidates,
                                    &fallback_actual_core,
                                    *fallback_func_index,
                                ),
                                Err(err) => {
                                    self.raise(err)?;
                                    return Ok(DispatchAction::Continue);
                                }
                            };
                            // Store in cache using hashed key (Issue #3355)
                            self.store_call_site_dispatch_cache(call_site_ip, type_hash, result);
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint,
                                result,
                            );
                            result
                        }
                    }
                } else {
                    *fallback_func_index
                };

                if selected_func_index == usize::MAX {
                    let has_native_collect_sentinel = candidates
                        .iter()
                        .any(|c| matches!(c, DynamicCallCandidate::NativeIterator(_)));
                    if has_native_collect_sentinel && args.len() == 1 {
                        if let Value::Generator(g) = &args[0] {
                            if self.generator_can_use_generic_collect(g)? {
                                self.start_function_call(*fallback_func_index, args)?;
                                return Ok(DispatchAction::Continue);
                            }
                            let iter = (*g.iter).clone();
                            if let Some(result) = self.collect_generator(
                                g.callable.clone(),
                                &iter,
                                g.result_element_type.clone(),
                            )? {
                                self.stack.push(result);
                            }
                            return Ok(DispatchAction::Continue);
                        }

                        let route_to_native_collect = match &args[0] {
                            Value::Range(_) => true,
                            Value::Struct(s) => matches_native_collect_iterator(&s.struct_name),
                            Value::StructRef(idx) => self
                                .struct_heap
                                .get(*idx)
                                .map(|s| matches_native_collect_iterator(&s.struct_name))
                                .unwrap_or(false),
                            _ => false,
                        };
                        if route_to_native_collect {
                            // CollectFallback: native-collect-sentinel-boundary
                            let result = self.collect_iterator(&args[0])?;
                            self.stack.push(result);
                            return Ok(DispatchAction::Continue);
                        }
                    }
                    // A runtime dispatch failure must raise a CATCHABLE
                    // `MethodError`, like upstream Julia — route it through
                    // `self.raise` so an enclosing `try/catch` can intercept it
                    // (Issue #5648). `return Err(..)` aborted the VM uncatchably.
                    // With no handler, `raise` re-propagates the error (still
                    // aborts), preserving the prior top-level behavior.
                    self.raise(VmError::MethodError(
                        "no matching runtime method candidate".to_string(),
                    ))?;
                    return Ok(DispatchAction::Continue);
                }

                let func = match self.get_function_cloned_or_raise(selected_func_index)? {
                    Some(f) => f,
                    None => return Ok(DispatchAction::Continue),
                };

                if args.len() == 1
                    && strip_module_prefix(&func.name) == "collect"
                    && self.is_base_program_function_index(selected_func_index)
                {
                    if let Value::Generator(g) = &args[0] {
                        let iter = (*g.iter).clone();
                        if let Some(result) = self.collect_generator(
                            g.callable.clone(),
                            &iter,
                            g.result_element_type.clone(),
                        )? {
                            self.stack.push(result);
                        }
                        return Ok(DispatchAction::Continue);
                    }
                }

                if args.len() == 1 {
                    if let Value::Generator(g) = &args[0] {
                        match func.name.as_str() {
                            "IteratorSize" => {
                                let iter = (*g.iter).clone();
                                let result =
                                    self.iterator_size_value_for_native_generator_iter(&iter)?;
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
                    let generator_iter_type = match &args[0] {
                        Value::DataType(julia_type) => generator_iter_type_name(julia_type),
                        _ => None,
                    };
                    if let Some(iter_type) = generator_iter_type {
                        match func.name.as_str() {
                            "IteratorSize" => {
                                let result = self
                                    .iterator_size_value_for_generator_iter_type_name(&iter_type)?;
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
                if args.len() == 2 && func.name == "collect_similar" {
                    if let (container, Value::Generator(g)) = (&args[0], &args[1]) {
                        if matches!(container, Value::Memory(_)) {
                            // Let Pure Julia dispatch choose the container-aware
                            // collect_similar(::Memory, ::Generator) method.
                        } else {
                            // CollectFallback: collect-similar-generator-runtime-boundary
                            let iter = (*g.iter).clone();
                            if let Some(result) = self.collect_generator(
                                g.callable.clone(),
                                &iter,
                                g.result_element_type.clone(),
                            )? {
                                self.stack.push(result);
                            }
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }

                let mut frame =
                    self.acquire_frame(func.local_slot_count, Some(selected_func_index));

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

            Instr::CallDynamicOrBuiltin(builtin_id, ref candidates) => {
                // Runtime dispatch for unary functions with builtin fallback.
                // Pop the argument to inspect its type
                let arg = self.stack.pop_value()?;
                let call_site_ip = self.ip - 1;
                let arg_fingerprint = self.call_site_arg_fingerprint(&arg);

                let matched = if let Some(cached) = arg_fingerprint
                    .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp))
                {
                    // Cache stores usize::MAX as sentinel for "no match" (use builtin)
                    if cached == usize::MAX {
                        None
                    } else {
                        Some(cached)
                    }
                } else {
                    let arg_type_name = self.get_type_name(&arg);

                    // Check dispatch cache first (Issue #2943, #3355)
                    let type_hash = hash_type_name(&arg_type_name);
                    if let Some(cached) =
                        self.lookup_call_site_dispatch_cache(call_site_ip, type_hash)
                    {
                        self.store_call_site_inline_cache(call_site_ip, arg_fingerprint, cached);
                        // Cache stores usize::MAX as sentinel for "no match" (use builtin)
                        if cached == usize::MAX {
                            None
                        } else {
                            Some(cached)
                        }
                    } else {
                        // Scored dispatch before builtin fallback (Issue #3910).
                        // VM representation filters remain local; candidate score
                        // ordering is shared with other migrated dynamic calls.
                        // Issue #6496: the payload carries only candidate function
                        // indices; the expected first-parameter signatures are
                        // derived here, once per call site + argument type (the
                        // result is dispatch-cached below). Issue #6502 slice 2:
                        // matching runs on the structured `core_signature`
                        // projection.
                        let named_candidates: Vec<(usize, String, CoreType, Option<CoreType>)> =
                            candidates
                                .iter()
                                .map(|&idx| {
                                    let (rendered, slot, gate) =
                                        self.dynamic_candidate_expected_signature(idx);
                                    (idx, rendered, slot, gate)
                                })
                                .collect();
                        let actual_core_ty = self.dispatch_julia_type_for_value(&arg);
                        let actual_cores = [crate::vm::dispatch_binding::runtime_actual_core_type(
                            &actual_core_ty,
                        )];
                        let best_match = resolve_runtime_core_signature_candidates(
                            &self.struct_hierarchy,
                            named_candidates.iter().filter_map(
                                |(idx, expected_type, slot, gate)| {
                                    if self.dynamic_candidate_arg_mismatch(&arg, expected_type) {
                                        return None;
                                    }
                                    Some(RuntimeCoreCandidate {
                                        idx: *idx,
                                        slots: [slot],
                                        signature: gate.as_ref(),
                                    })
                                },
                            ),
                            &actual_cores,
                            |actual, expected| self.check_subtype_core(actual, expected),
                        );
                        let best_idx = best_match.map(|(idx, _score)| idx);
                        // Store in cache using hashed key (Issue #3355)
                        let cache_val = best_idx.unwrap_or(usize::MAX);
                        self.store_call_site_dispatch_cache(call_site_ip, type_hash, cache_val);
                        self.store_call_site_inline_cache(call_site_ip, arg_fingerprint, cache_val);
                        best_idx
                    }
                };

                if let Some(func_index) = matched {
                    // Call the user-defined method
                    let func = match self.get_function_cloned_or_raise(func_index)? {
                        Some(f) => f,
                        None => return Ok(DispatchAction::Continue),
                    };

                    let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

                    // Bind type parameters from where clauses (Issue #2468)
                    self.bind_type_params(&func, std::slice::from_ref(&arg), &mut frame);

                    if let Some(slot) = func.param_slots.first() {
                        bind_value_to_slot(&mut frame, *slot, arg, &mut self.struct_heap);
                    }

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
                } else {
                    // No matching struct method - fall back to builtin
                    if matches!(
                        builtin_id,
                        BuiltinId::Length
                            | BuiltinId::Size
                            | BuiltinId::Ndims
                            | BuiltinId::Eltype
                            | BuiltinId::Similar
                    ) {
                        self.stack.push(arg);
                        self.execute_builtin(*builtin_id, 1)?;
                        return Ok(DispatchAction::Continue);
                    }
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
                                return Ok(DispatchAction::Continue);
                            }
                        };
                        self.stack.push(result);
                    } else {
                        // Resolve builtin to an f64 operation plus a matching
                        // BigFloat op, then preserve primitive float width while
                        // allowing heap-backed numeric structs to coerce to
                        // Float64 and BigFloat to keep arbitrary precision
                        // (Issue #6801).
                        type F64Op = fn(f64) -> f64;
                        type BfOp = fn(&RustBigFloat) -> RustBigFloat;
                        let (f64_op, bf_op): (F64Op, BfOp) = match builtin_id {
                            // Note: Exp, Log, Sin, Cos, Tan removed — now Pure Julia (base/math.jl)
                            BuiltinId::Floor => (f64::floor, RustBigFloat::floor),
                            BuiltinId::Ceil => (f64::ceil, RustBigFloat::ceil),
                            // Julia's default RoundNearest is round-half-to-even
                            // (banker's rounding): round(2.5)==2.0, round(0.5)==0.0.
                            // f64::round rounds half away from zero, so use
                            // round_ties_even to match the direct builtin handler
                            // and upstream (Issue #6742).
                            BuiltinId::Round => {
                                (f64::round_ties_even, RustBigFloat::round_nearest_even)
                            }
                            BuiltinId::Trunc => (f64::trunc, RustBigFloat::trunc),
                            _ => {
                                self.raise(VmError::MethodError(format!(
                                    "unsupported builtin for CallDynamicOrBuiltin: {:?}",
                                    builtin_id
                                )))?;
                                return Ok(DispatchAction::Continue);
                            }
                        };
                        let result = apply_unary_rounding_op_with_heap(
                            arg,
                            &self.struct_heap,
                            f64_op,
                            bf_op,
                        )?;
                        self.stack.push(result);
                    }
                }
                Ok(DispatchAction::Continue)
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

                // Some VM-backed Pure Julia wrappers use builtin iteration.
                let uses_builtin_iterate = match &coll {
                    Value::Struct(s) => uses_builtin_iterate_for_struct(&s.struct_name),
                    Value::StructRef(idx) => self
                        .struct_heap
                        .get(*idx)
                        .is_some_and(|s| uses_builtin_iterate_for_struct(&s.struct_name)),
                    _ => false,
                } && !is_array_wrapper_value(&coll, &self.struct_heap);

                if !uses_builtin_iterate && can_score_iterate_dynamic_candidates(&coll) {
                    let call_site_ip = self.ip - 1;
                    let arg_fingerprint = if let Some(state) = &state_opt {
                        self.call_site_arg_fingerprints(&[&coll, state])
                    } else {
                        self.call_site_arg_fingerprint(&coll)
                    };

                    let matched = if let Some(cached) = arg_fingerprint
                        .and_then(|fp| self.lookup_call_site_inline_cache(call_site_ip, fp))
                    {
                        if cached == usize::MAX {
                            None
                        } else {
                            // Find the candidate with this func_index for type binding
                            candidates.iter().copied().find(|idx| *idx == cached)
                        }
                    } else {
                        // Get struct type name and find matching iterate method
                        let coll_type_name = self.get_type_name(&coll);
                        let mut actual_type_names_owned = vec![coll_type_name.clone()];
                        if let Some(state) = &state_opt {
                            actual_type_names_owned.push(self.get_type_name(state));
                        }
                        let mut dispatch_args = vec![coll.clone()];
                        if let Some(state) = &state_opt {
                            dispatch_args.push(state.clone());
                        }
                        let actual_cores: Vec<CoreType> = dispatch_args
                            .iter()
                            .map(|arg| {
                                let ty = self.dispatch_julia_type_for_value(arg);
                                crate::vm::dispatch_binding::runtime_actual_core_type(&ty)
                            })
                            .collect();

                        // Check dispatch cache first (Issue #2943, #3355)
                        let type_hash = hash_type_name(&actual_type_names_owned.join("\u{1f}"));
                        if let Some(cached) =
                            self.lookup_call_site_dispatch_cache(call_site_ip, type_hash)
                        {
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint,
                                cached,
                            );
                            if cached == usize::MAX {
                                None
                            } else {
                                // Find the candidate with this func_index for type binding
                                candidates.iter().copied().find(|idx| *idx == cached)
                            }
                        } else {
                            let dispatch_args: Vec<Value> = if let Some(ref state) = state_opt {
                                vec![coll.clone(), state.clone()]
                            } else {
                                vec![coll.clone()]
                            };
                            // Native arrays only score against user-defined
                            // candidates so the VM builtin iterator stays the
                            // default; structs score the full set (Issue #6638).
                            let scored = scored_iterate_candidates(
                                &coll,
                                candidates,
                                self.base_function_count,
                                &self.struct_heap,
                            );
                            let best = match self
                                .find_best_method_index_from_candidates(&scored, &dispatch_args)
                            {
                                Ok(Some(func_index)) => Some(func_index),
                                Ok(None) => {
                                    // Shared structured scored dispatch fallback
                                    // (Issues #6336/#6502).
                                    self.resolve_scored_family_fallback(&scored, &actual_cores)
                                }
                                Err(err) => {
                                    self.raise(err)?;
                                    return Ok(DispatchAction::Continue);
                                }
                            };
                            // Store in cache using hashed key (Issue #3355)
                            let cache_val = best.unwrap_or(usize::MAX);
                            self.store_call_site_dispatch_cache(call_site_ip, type_hash, cache_val);
                            self.store_call_site_inline_cache(
                                call_site_ip,
                                arg_fingerprint,
                                cache_val,
                            );
                            best
                        }
                    };

                    if let Some(func_index) = matched {
                        // Call the user-defined iterate method
                        let func = match self.get_function_cloned_or_raise(func_index)? {
                            Some(f) => f,
                            None => return Ok(DispatchAction::Continue),
                        };

                        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

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
                    } else {
                        if is_struct && !is_array_wrapper_value(&coll, &self.struct_heap) {
                            // No matching method found - error
                            // User-visible: user's struct type has no iterate method — triggered by for-loops over custom types
                            return Err(VmError::TypeError(format!(
                                "iterate: no method matching iterate(::{}{})",
                                self.get_type_name(&coll),
                                if *argc == 2 { ", ...)" } else { ")" }
                            )));
                        }

                        // Native VM collections can still use builtin iteration
                        // when no user/runtime method candidate matches.
                        if let Value::Generator(generator) = &coll {
                            if self
                                .start_lazy_generator_iterate_call(generator, state_opt.as_ref())?
                            {
                                return Ok(DispatchAction::Continue);
                            }
                        }
                        let result = if let Some(state) = state_opt {
                            self.iterate_next(&coll, &state)?
                        } else {
                            self.iterate_first(&coll)?
                        };
                        self.stack.push(result);
                    }
                } else {
                    // Not a struct or CartesianIndices - use builtin iterate
                    if let Value::Generator(generator) = &coll {
                        if self.start_lazy_generator_iterate_call(generator, state_opt.as_ref())? {
                            return Ok(DispatchAction::Continue);
                        }
                    }
                    let result = if let Some(state) = state_opt {
                        self.iterate_next(&coll, &state)?
                    } else {
                        self.iterate_first(&coll)?
                    };
                    self.stack.push(result);
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
