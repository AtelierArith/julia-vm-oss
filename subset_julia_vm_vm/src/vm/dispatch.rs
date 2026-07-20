//! Runtime method dispatch and parameter type matching for the VM.
//!
//! Split out of `vm/mod.rs` (Issue #6826). These `impl Vm<R>` methods implement
//! runtime method selection (`find_best_method_index` + the dominance/specificity
//! pre-checks), runtime type matching (`type_matches`, `value_matches_param*`),
//! and type-parameter binding (`bind_type_params`, `bind_ntuple_params`).

use super::*;
use crate::types::JuliaType;

fn type_binding_structure_score(ty: &crate::types::JuliaType) -> u8 {
    u8::from(ty.contains_unionall()) + 2 * u8::from(ty.contains_runtime_typevar())
}

fn insert_frame_type_binding(frame: &mut Frame, name: String, bound_type: crate::types::JuliaType) {
    match frame.type_bindings.entry(name) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(bound_type);
        }
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            let existing = entry.get();
            let semantically_equal =
                crate::inference_core::CoreType::from_julia_type_preserving_owner(existing)
                    .is_semantically_equal(
                        &crate::inference_core::CoreType::from_julia_type_preserving_owner(
                            &bound_type,
                        ),
                    );
            if !semantically_equal
                || type_binding_structure_score(&bound_type)
                    > type_binding_structure_score(existing)
            {
                entry.insert(bound_type);
            }
        }
    }
}

fn strip_runtime_module_prefix(name: &str) -> &str {
    name.rfind('.').map_or(name, |idx| &name[idx + 1..])
}

// Runtime type-name string parser on the dispatch RESOLVE path (candidate
// matching / type-var binding, reached only on an L1/L2 cache miss). It operates
// on `JuliaType::Struct(name)` opaque spellings — the parametric parameters live
// only in the rendered string (TYPE_REPRESENTATIONS §1.1), so it cannot be retired
// to structured interned ids until the resolver matches struct/abstract-argument
// candidates by `ConcreteTypeId`. Issue #9197 S5 landed the primitive-argument
// first-arg typemap gather (`FirstArgIndex`, method_table.rs); this
// struct-argument resolve path is deferred to S6/S7 (see TYPE_INTERNING.md
// "Slice 5 deliverable"). `TypeInternTable::intern_type_name` is the structural
// extractor S6/S7 will consume here.
fn split_runtime_parametric_name(name: &str) -> (&str, Vec<String>) {
    let Some(open) = name.find('{') else {
        return (name, Vec::new());
    };
    if !name.ends_with('}') {
        return (name, Vec::new());
    }
    let inner = &name[open + 1..name.len() - 1];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(inner[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        args.push(last.to_string());
    }
    (name[..open].trim(), args)
}

fn static_core_mentions_type_param(
    ty: &crate::inference_core::CoreType,
    type_params: &[crate::types::TypeParam],
) -> bool {
    type_params
        .iter()
        .any(|type_param| static_core_mentions_named_type_param(ty, &type_param.name))
}

fn static_core_mentions_named_type_param(ty: &crate::inference_core::CoreType, name: &str) -> bool {
    match ty {
        crate::inference_core::CoreType::Named(ty_name) => ty_name == name,
        crate::inference_core::CoreType::TypeVar(var) => var.name == name,
        crate::inference_core::CoreType::AbstractUser { parent, .. } => parent
            .as_deref()
            .is_some_and(|parent| static_core_mentions_named_type_param(parent, name)),
        crate::inference_core::CoreType::Struct { params, .. }
        | crate::inference_core::CoreType::Tuple(params)
        | crate::inference_core::CoreType::Union(params) => params
            .iter()
            .any(|param| static_core_mentions_named_type_param(param, name)),
        crate::inference_core::CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, field_ty)| static_core_mentions_named_type_param(field_ty, name)),
        crate::inference_core::CoreType::TypeOf(inner)
        | crate::inference_core::CoreType::Vararg(inner)
        | crate::inference_core::CoreType::UnionAll { body: inner, .. } => {
            static_core_mentions_named_type_param(inner, name)
        }
        crate::inference_core::CoreType::VarargLen { element, len } => {
            static_core_mentions_named_type_param(element, name)
                || static_core_mentions_named_type_param(len, name)
        }
        _ => false,
    }
}

impl<R: RngLike> Vm<R> {
    fn julia_type_is_range_family(ty: &crate::types::JuliaType) -> bool {
        let name = ty.name();
        let unqualified = name.rsplit('.').next().unwrap_or(&name);
        let base = unqualified.split('{').next().unwrap_or(unqualified);
        matches!(
            base,
            "AbstractRange"
                | "AbstractUnitRange"
                | "UnitRange"
                | "StepRange"
                | "StepRangeLen"
                | "LinRange"
                | "OneTo"
        )
    }

    /// Check if a runtime type matches a function parameter type.
    ///
    /// Dispatch RESOLVE path (Issue #9197 S6/S7): `runtime_type` is a rendered
    /// type-name string and the parametric matching below re-parses its braces at
    /// runtime. This is reached only on a struct/abstract-argument cache miss —
    /// S5's `FirstArgIndex`→typemap gather covers the sealed-primitive argument
    /// path (method_table.rs), and S2/S3 already moved the cached L1/L2 hot path
    /// off strings; retiring this struct-argument re-parse to structural
    /// interned-id matching is deferred to S6/S7 (see TYPE_INTERNING.md).
    pub(super) fn type_matches(
        &self,
        runtime_type: &str,
        param_type: &crate::types::JuliaType,
    ) -> bool {
        use crate::types::JuliaType;

        fn strip_module(name: &str) -> &str {
            if let Some(idx) = name.rfind('.') {
                &name[idx + 1..]
            } else {
                name
            }
        }

        // Extract base name from runtime type (e.g., "Complex{Float64}" -> "Complex")
        let runtime_base_raw = if let Some(idx) = runtime_type.find('{') {
            &runtime_type[..idx]
        } else {
            runtime_type
        };
        let runtime_base = strip_module(runtime_base_raw);

        match param_type {
            JuliaType::Any => true,
            JuliaType::Union(members) => members
                .iter()
                .any(|member| self.type_matches(runtime_type, member)),
            JuliaType::TupleOf(expected_elems) => {
                if runtime_base != "Tuple" {
                    return false;
                }
                // No type variables to bind: this is a pure subtype question
                // (`Tuple{...} <: Tuple{...}` is covariant in Julia), so route
                // it through the shared engine (Issue #5915). Elements that
                // are TypeVar wildcards keep the permissive local matching
                // used by the bindings-driven dispatcher below.
                if !expected_elems
                    .iter()
                    .any(runtime_julia_type_contains_type_var)
                {
                    return self.check_subtype(runtime_type, param_type.name().as_ref());
                }
                // TypeVar elements keep the permissive local wildcard match
                // (bindings are extracted by the bindings-driven dispatcher);
                // every concrete element leg is a covariant subtype question
                // routed through the shared engine (Issue #5915), matching
                // upstream `Tuple{Int64, Int64} <: Tuple{T, Real} where T`.
                let params = subset_julia_vm_bytecode::parse_parametric_params(runtime_type);
                let elem_matches = |actual: &str, expected: &JuliaType| -> bool {
                    match expected {
                        JuliaType::TypeVar(_, Some(bound)) if bound.starts_with(">:") => {
                            self.check_subtype(bound.trim_start_matches(">:").trim(), actual)
                        }
                        JuliaType::TypeVar(_, Some(bound)) => self.check_subtype(actual, bound),
                        JuliaType::TypeVar(_, None) => true,
                        _ => self.check_subtype(actual, expected.name().as_ref()),
                    }
                };
                if let Some(vararg_elem) = expected_elems
                    .last()
                    .and_then(crate::types::unbounded_vararg_element)
                {
                    let lead_count = expected_elems.len() - 1;
                    return params.len() >= lead_count
                        && params
                            .iter()
                            .zip(expected_elems[..lead_count].iter())
                            .all(|(actual, expected)| elem_matches(actual, expected))
                        && params[lead_count..]
                            .iter()
                            .all(|actual| elem_matches(actual, &vararg_elem));
                }
                params.len() == expected_elems.len()
                    && params
                        .iter()
                        .zip(expected_elems.iter())
                        .all(|(actual, expected)| elem_matches(actual, expected))
            }
            JuliaType::VectorOf(expected_elem) => {
                if !matches!(runtime_base, "Array" | "Vector") {
                    return false;
                }
                let params = subset_julia_vm_bytecode::parse_parametric_params(runtime_type);
                if params.is_empty() {
                    return true;
                }
                matches!(expected_elem.as_ref(), JuliaType::TypeVar(_, _))
                    || type_utils::type_objects_equal(
                        &JuliaType::from_name_or_struct(params[0]),
                        expected_elem,
                    )
            }
            JuliaType::MatrixOf(expected_elem) => {
                if !matches!(runtime_base, "Array" | "Matrix") {
                    return false;
                }
                let params = subset_julia_vm_bytecode::parse_parametric_params(runtime_type);
                if params.is_empty() {
                    return true;
                }
                matches!(expected_elem.as_ref(), JuliaType::TypeVar(_, _))
                    || type_utils::type_objects_equal(
                        &JuliaType::from_name_or_struct(params[0]),
                        expected_elem,
                    )
            }
            // Pure nominal subtype questions are routed through the shared
            // subtype engine (`check_subtype` → CoreSubtypeEngine). The
            // engine matches upstream Julia where the old hand-rolled arms
            // did not: ranges/views ARE `AbstractArray`s, `String <:
            // AbstractString`, `Char <: AbstractChar`, `IOBuffer <: IO`
            // (Issues #5921, #5915).
            JuliaType::Int64
            | JuliaType::Float64
            | JuliaType::Real
            | JuliaType::Number
            | JuliaType::Integer
            | JuliaType::Signed
            | JuliaType::Unsigned
            | JuliaType::AbstractFloat
            | JuliaType::UnitRange
            | JuliaType::StepRange
            | JuliaType::AbstractRange
            | JuliaType::Array
            | JuliaType::AbstractArray
            | JuliaType::Tuple
            | JuliaType::AbstractString
            | JuliaType::AbstractChar
            | JuliaType::IO => self.check_subtype(runtime_type, param_type.name().as_ref()),
            // User-declared abstract types (incl. boot.jl ones such as
            // AbstractDict / AbstractSet) are a pure nominal subtype
            // question over the VM's declared hierarchy; the engine-backed
            // `check_subtype` resolves it. The old exact-name-equality
            // fallback could never match a subtype value here (Issue #5915).
            JuliaType::AbstractUser(name, _) => self.check_subtype(runtime_type, name),
            // Ref / RefValue dispatch (Issue #5130). The VM models Base.RefValue{T}
            // via Value::Ref, whose runtime type prints as "Base.RefValue{T}".
            //   - `::Ref`            matches any RefValue (Ref is its abstract supertype)
            //   - `::RefValue`       matches any RefValue
            //   - `::Ref{E}` / `::Base.RefValue{E}` match when the element type E agrees
            //     (a type-variable element matches any RefValue).
            JuliaType::Struct(name)
                if {
                    let pb = strip_module(name.split('{').next().unwrap_or(name));
                    (pb == "Ref" || pb == "RefValue") && runtime_base == "RefValue"
                } =>
            {
                if !name.contains('{') {
                    // Bare ::Ref / ::RefValue: matches any element type.
                    return true;
                }
                let param_params = subset_julia_vm_bytecode::parse_parametric_params(name);
                let runtime_params =
                    subset_julia_vm_bytecode::parse_parametric_params(runtime_type);
                match (param_params.first(), runtime_params.first()) {
                    (Some(param_elem), Some(runtime_elem)) => {
                        // Type-variable element parameter matches any concrete element.
                        crate::vm::util::has_type_variable_param(name)
                            || crate::types::JuliaType::from_name_or_struct(runtime_elem)
                                == crate::types::JuliaType::from_name_or_struct(param_elem)
                    }
                    // Parametric param but runtime element unknown: be permissive.
                    _ => true,
                }
            }
            // Type-level NamedTuple dispatch (Issue #5063). A runtime named
            // tuple `@NamedTuple{a::T1, b::T2}` matches a concrete
            // `@NamedTuple{...}` or names-only `NamedTuple{(:a, :b)}` parameter
            // via the structured subtype relation, which understands field
            // names and covariant field types.
            JuliaType::Struct(name)
                if (name.starts_with("@NamedTuple{") || name.starts_with("NamedTuple{"))
                    && (runtime_type.starts_with("@NamedTuple{")
                        || runtime_type.starts_with("NamedTuple{(")) =>
            {
                type_utils::type_values_subtype(
                    &crate::types::JuliaType::from_name_or_struct(runtime_type),
                    &crate::types::JuliaType::from_name_or_struct(name),
                )
            }
            JuliaType::Struct(name)
                if matches!(
                    strip_module(name.split('{').next().unwrap_or(name)),
                    "AbstractArray"
                        | "AbstractVector"
                        | "AbstractMatrix"
                        | "DenseArray"
                        | "DenseVector"
                        | "DenseMatrix"
                ) =>
            {
                self.check_subtype(runtime_type, name)
            }
            JuliaType::Struct(name) => {
                // Handle parametric types: "Complex{Float64}" matches "Complex"
                let param_has_type_params = name.contains('{');
                let param_base_raw = if let Some(idx) = name.find('{') {
                    &name[..idx]
                } else {
                    name.as_str()
                };
                let param_base = strip_module(param_base_raw);
                let runtime_has_type_params = runtime_type.contains('{');

                // Module-owner guard (Issue #11076, dispatch-matching sibling
                // of the type-identity fix in Issue #11021): if BOTH the
                // declared parameter name and the actual runtime type name
                // carry a module-qualification prefix and those owners
                // differ, this candidate can never match -- regardless of
                // what the module-stripped fast-path equality below (or the
                // `check_subtype` mutual-subtype fallback, which routes
                // through `CoreType` and has already lost owner information
                // by construction) would otherwise conclude. Without this,
                // `f(x::A1x.Box) = ...` and `f(x::A2x.Box) = ...` both
                // "matched" `A1x.Box(1)` because every downstream comparison
                // here strips module qualification unconditionally, making
                // the call wrongly ambiguous instead of resolving to the one
                // method whose owner the argument actually matches.
                if !crate::types::struct_owners_compatible(name, runtime_type) {
                    return false;
                }

                if param_base == "Array" && param_has_type_params && runtime_has_type_params {
                    let runtime_params =
                        subset_julia_vm_bytecode::parse_parametric_params(runtime_type);
                    let param_params = subset_julia_vm_bytecode::parse_parametric_params(name);
                    let runtime_rank = match runtime_base {
                        "Vector" => Some(1),
                        "Matrix" => Some(2),
                        "Array" => runtime_params.get(1).and_then(|p| p.parse::<usize>().ok()),
                        _ => None,
                    };
                    if runtime_rank.is_none() {
                        return false;
                    }
                    let param_rank = param_params.get(1).and_then(|p| p.parse::<usize>().ok());
                    if param_rank.is_some() && runtime_rank != param_rank {
                        return false;
                    }
                    if let (Some(runtime_elem), Some(param_elem)) =
                        (runtime_params.first(), param_params.first())
                    {
                        if let Some((_, bound)) = param_elem.split_once("<:") {
                            let bound_ty = crate::types::JuliaType::from_name_or_struct(bound);
                            return type_utils::type_values_subtype(
                                &crate::types::JuliaType::from_name_or_struct(runtime_elem),
                                &bound_ty,
                            );
                        }
                        return crate::vm::util::has_type_variable_param(name)
                            || crate::types::JuliaType::from_name_or_struct(runtime_elem)
                                == crate::types::JuliaType::from_name_or_struct(param_elem);
                    }
                    return matches!(runtime_base, "Array" | "Vector" | "Matrix");
                }

                // Type-variable params (Dict{K,V}, Rational{T}) match any
                // runtime params with the same base type (Issue #2748); the
                // bindings-driven dispatcher extracts the bindings, so the
                // wildcard match stays local.
                if param_has_type_params && crate::vm::util::has_type_variable_param(name) {
                    return runtime_base == param_base;
                }
                // Parametric param but runtime params unknown (bare runtime
                // name): keep the legacy permissive base-name match.
                if param_has_type_params && !runtime_has_type_params {
                    return runtime_base == param_base;
                }
                // Partial parametric application is normally a UnionAll subtype
                // question (`SVector{0,Float64} <: SVector{0}`). Keep the old
                // exact-tag behavior only for native view wrappers whose Base
                // methods still use shortened carrier spellings (Issue #9785).
                if param_has_type_params
                    && subset_julia_vm_bytecode::parse_parametric_params(name).len()
                        != subset_julia_vm_bytecode::parse_parametric_params(runtime_type).len()
                    && matches!(param_base, "SubArray" | "ReshapedArray" | "MatrixView")
                {
                    return strip_module(runtime_type) == strip_module(name);
                }
                // Pure nominal/parametric subtype question → engine-backed
                // `check_subtype` (Issue #5915). It preserves invariance
                // (`Rational{Int64}` does NOT match `Rational{BigInt}`) and
                // adds declared-parent relations the old string equality
                // missed (`MyVec{Int64} <: Wrapper{Int64}` for
                // `struct MyVec{T} <: Wrapper{T}`). A bare/qualified alias
                // stays on the fast path for renderer differences
                // (`Base.OneTo{Int64}` vs `OneTo{Int64}`), while two explicit
                // sibling owners remain distinct (Issue #11076).
                !crate::types::explicit_sibling_nominal_family_conflict(runtime_type, name)
                    && (crate::types::nominal_type_names_compatible(runtime_type, name)
                        || self.check_subtype(runtime_type, name))
            }
            JuliaType::TypeVar(_, _) => true, // Type variables match anything
            // Function singleton runtime names (`typeof(+)`, `typeof(f)`) are
            // subtypes of Function in upstream Julia. Route them through the
            // shared engine now that callable n-ary intrinsic folds preserve
            // narrow integers (Issue #6512). The #6512 exact-name carve-out was
            // re-evaluated (Issue #6597) and confirmed fully removable: the
            // f6adade84 (#6529) native-array wrapper fence — now the selection-core
            // policy `selection::signature_is_broad_wrapper_fence` (Issue #6595) —
            // keeps empty narrow-int / Bool reductions on the type-specialized
            // Base method. Pinned by
            // `runtime_type_matches_function_param_via_core_subtype_issue_6597`.
            JuliaType::Function => self.check_subtype(runtime_type, param_type.name().as_ref()),
            // Remaining variants are plain nominal names (concrete leaves
            // like Int8 / Float32 / Symbol, and type-level names like
            // DataType): a pure subtype question for the shared engine,
            // which keeps exact-name equality as its fast path and adds the
            // upstream relations the old string equality missed
            // (`DataType <: Type`, `Set{Int64} <: Set`) — Issue #5915.
            _ => self.check_subtype(runtime_type, param_type.name().as_ref()),
        }
    }

    /// Match a runtime value against a JuliaType parameter, including Type{T} patterns.
    pub(super) fn value_matches_param(
        &self,
        value: &Value,
        param_type: &crate::types::JuliaType,
    ) -> bool {
        use crate::types::JuliaType;

        match (value, param_type) {
            (Value::DataType(dt), JuliaType::TypeOf(inner)) => match inner.as_ref() {
                JuliaType::TypeVar(_, Some(bound)) => self.check_subtype(dt.name().as_ref(), bound),
                JuliaType::TypeVar(_, None) => true,
                other => **dt == *other,
            },
            // A type-object argument matches a `Union{Type{A}, Type{B}, ...}`
            // parameter when it matches any member. The string-based
            // `type_matches` Union branch cannot see the `Value::DataType` and so
            // never matches a `Type{...}` member, so route each member back
            // through `value_matches_param` (which has the `DataType`/`Type{T}`
            // arm above). The BigFloat / BigInt / Rational `promote_rule` methods
            // are written with `Union{Type{...}, ...}` second arguments
            // (Issue #5070); without this they fell through to the generic
            // `promote_rule(::Type{T}, ::Type{S}) = Union{}` and promotion
            // widened to `typejoin` (Issue #6781).
            (Value::DataType(_), JuliaType::Union(members)) => members
                .iter()
                .any(|member| self.value_matches_param(value, member)),
            _ => {
                let runtime_type = self.get_type_name(value);
                self.type_matches(&runtime_type, param_type)
            }
        }
    }

    pub(super) fn method_dispatch_key(&self, names: &[&str], args: &[Value]) -> MethodDispatchKey {
        MethodDispatchKey {
            names: names.iter().map(|name| hash_type_name(name)).collect(),
            arg_types: args
                .iter()
                .map(|arg| {
                    let ty = self.dispatch_julia_type_for_value(arg);
                    hash_type_name(ty.name().as_ref())
                })
                .collect(),
        }
    }

    /// Find the best matching method index for a function name and arguments.
    pub(super) fn find_best_method_index(
        &mut self,
        names: &[&str],
        args: &[Value],
    ) -> Option<usize> {
        let key = self.method_dispatch_key(names, args);
        if let Some(cached) = self.method_dispatch_cache.get(&key) {
            match cached {
                Some(idx) => {
                    crate::vm::profiler::record_event("MethodDispatchCacheHit");
                    return Some(*idx);
                }
                None => {
                    crate::vm::profiler::record_event("MethodDispatchNegativeCacheHit");
                    return None;
                }
            }
        }

        crate::vm::profiler::record_event("MethodDispatchCacheMiss");
        let result = self.find_best_method_index_uncached(names, args);
        self.method_dispatch_cache.insert(key, result);
        self.enforce_method_dispatch_cache_limit();
        result
    }

    /// Morespecific dominance pre-check for the runtime dispatch path (Issue
    /// #5926) — the runtime mirror of `MethodTable::dispatch_inner`'s pre-check.
    /// Among the candidates the concrete arguments match, if exactly one method's
    /// where-wrapped `Tuple` signature strictly dominates every other's (and the
    /// argument tuple is a subtype of it), it is unambiguously the most specific,
    /// so select it directly — resolving the container-vs-abstract-supertype,
    /// diagonal `Tuple{T,T}`, bounded-`where`, and invariant-parametric relations
    /// the integer specificity score mis-ranks. Returns `None` (defer to the
    /// score loop) when no single method dominates, or when any matching
    /// candidate is a vararg (whose `core_signature` is not subtype-faithful).
    pub(super) fn dominant_method_index_runtime(
        &self,
        names: &[&str],
        args: &[Value],
    ) -> Option<usize> {
        let candidate_indices: Vec<_> = names
            .iter()
            .flat_map(|name| self.get_function_indices_by_name(name).iter().copied())
            .collect();
        self.dominant_method_index_runtime_for_indices(&candidate_indices, args)
    }

    pub(super) fn dominant_method_index_runtime_for_indices(
        &self,
        candidate_indices: &[usize],
        args: &[Value],
    ) -> Option<usize> {
        use crate::inference_core::{CoreSubtypeEngine, CoreType, CoreTypeVar};

        let arg_types: Vec<_> = args
            .iter()
            .map(|arg| self.dispatch_julia_type_for_value(arg))
            .collect();
        let mut cands: Vec<(usize, CoreType)> = Vec::new();
        let mut runtime_matches: Vec<RuntimeCandidateMatch> = Vec::new();
        let world = self.current_dispatch_world();
        for &idx in candidate_indices {
            let Some(func) = self.functions.get(idx) else {
                continue;
            };
            if !self.function_visible_in_world(idx, world) {
                continue;
            }
            let Some(param_types) = expanded_param_types_for_call(func, args.len()) else {
                continue;
            };
            if self.is_base_program_function_index(idx)
                && !self.is_native_array_exempt_function(idx)
                && params_cross_native_array_wrapper_boundary(args, &param_types)
            {
                continue;
            }
            if self
                .function_candidate_binding_count(idx, args, &param_types, &func.type_params)
                .is_none()
            {
                continue;
            }
            // A matching vararg candidate has no subtype-faithful signature;
            // defer the whole decision to the score path.
            if func.vararg_param_index.is_some() {
                return None;
            }
            runtime_matches.push(RuntimeCandidateMatch {
                idx,
                param_types,
                score: 0,
                specificity: 0,
                is_vararg: false,
            });
            let mut sig = CoreType::Tuple(Self::scoped_runtime_param_cores(
                &func.param_julia_types,
                &func.type_params,
            ));
            for tp in func.type_params.iter().rev() {
                sig = CoreType::UnionAll {
                    var: CoreTypeVar::from(tp),
                    body: Box::new(sig),
                };
            }
            cands.push((idx, sig));
        }
        if cands.len() < 2 {
            return None;
        }

        if let Some(idx) =
            self.tuple_diagonal_dominant_candidate_index(&runtime_matches, &arg_types)
        {
            return Some(idx);
        }
        if let Some(idx) = self.union_actual_dominant_candidate_index(&runtime_matches, &arg_types)
        {
            return Some(idx);
        }
        if let Some(idx) =
            self.vector_diagonal_dominant_candidate_index(&runtime_matches, &arg_types)
        {
            return Some(idx);
        }
        if let Some(idx) =
            self.type_value_diagonal_dominant_candidate_index(&runtime_matches, &arg_types)
        {
            return Some(idx);
        }
        if let Some(idx) =
            self.type_vector_diagonal_dominant_candidate_index(&runtime_matches, &arg_types)
        {
            return Some(idx);
        }
        if let Some(idx) =
            self.type_matrix_diagonal_dominant_candidate_index(&runtime_matches, &arg_types)
        {
            return Some(idx);
        }
        if self.runtime_actual_aware_dominance_candidate_present(&runtime_matches) {
            return None;
        }

        let arg_tuple = CoreType::Tuple(
            arg_types
                .iter()
                .map(dispatch_binding::runtime_actual_core_type)
                .collect(),
        );
        let subtype = CoreSubtypeEngine::new();
        selection::unique_dominant_index(
            cands.len(),
            |i| {
                subtype.is_subtype(&arg_tuple, &cands[i].1)
                    && !self.base_runtime_dominance_crosses_user_candidate(&cands, i)
            },
            |i, j| cands[i].1.strict_subtype_dominates(&cands[j].1),
        )
        .map(|pos| cands[pos].0)
    }

    pub(super) fn runtime_actual_aware_dominance_candidate_present(
        &self,
        matches: &[RuntimeCandidateMatch],
    ) -> bool {
        matches
            .iter()
            .any(|candidate| self.runtime_function_has_actual_aware_dominance(candidate.idx))
    }

    pub(super) fn runtime_function_has_actual_aware_dominance(&self, idx: usize) -> bool {
        self.functions.get(idx).is_some_and(|func| {
            let param_cores =
                Self::scoped_runtime_param_cores(&func.param_julia_types, &func.type_params);
            let type_vars = Self::scoped_runtime_type_vars(&func.type_params);
            specificity::repeated_vector_typevar_pattern(&func.param_julia_types, &func.type_params)
                .is_some()
                || specificity::type_value_diagonal_pattern(
                    &func.param_julia_types,
                    &func.type_params,
                )
                .is_some()
                || specificity::core_type_vector_diagonal_pattern(&param_cores, &type_vars)
                    .is_some()
                || specificity::core_type_matrix_diagonal_pattern(&param_cores, &type_vars)
                    .is_some()
        })
    }

    fn dedup_runtime_candidate_indices(&self, indices: Vec<usize>, arg_len: usize) -> Vec<usize> {
        let mut deduped: Vec<(
            usize,
            Option<(
                Option<usize>,
                Option<usize>,
                bool,
                crate::inference_core::CoreType,
            )>,
        )> = Vec::with_capacity(indices.len());

        for idx in indices {
            let Some(func) = self.functions.get(idx) else {
                continue;
            };
            let key = Self::runtime_candidate_signature_for_dedup(func, arg_len).map(|sig| {
                (
                    func.vararg_param_index,
                    func.vararg_fixed_count,
                    Self::runtime_candidate_vararg_mentions_type_vars(func, arg_len),
                    sig,
                )
            });
            if let Some(key) = key {
                if let Some(pos) = deduped
                    .iter()
                    .position(|(_, existing)| existing.as_ref() == Some(&key))
                {
                    if self.runtime_candidate_is_newer(deduped[pos].0, idx) {
                        deduped[pos] = (idx, Some(key));
                    }
                } else {
                    deduped.push((idx, Some(key)));
                }
            } else {
                deduped.push((idx, None));
            }
        }

        deduped.into_iter().map(|(idx, _)| idx).collect()
    }

    /// Julia's replacement chronology is source order, not function-vector
    /// order. Full REPL rebuilds intentionally place current definitions before
    /// retained prior definitions, so a larger index can be the older body.
    /// Legacy/synthesized rows have no source ordinal and retain the historical
    /// index fallback (Issue #9784).
    fn runtime_candidate_is_newer(&self, existing_index: usize, candidate_index: usize) -> bool {
        let Some(existing) = self.functions.get(existing_index) else {
            return true;
        };
        let Some(candidate) = self.functions.get(candidate_index) else {
            return false;
        };
        if existing.definition_order != 0 && candidate.definition_order != 0 {
            (candidate.definition_order, candidate_index)
                > (existing.definition_order, existing_index)
        } else {
            candidate_index > existing_index
        }
    }

    fn runtime_candidate_signature_for_dedup(
        func: &FunctionInfo,
        arg_len: usize,
    ) -> Option<crate::inference_core::CoreType> {
        let param_types = expanded_param_types_for_call(func, arg_len)?;
        let candidate_sig = dispatch_binding::build_runtime_candidate_core_signature(
            &param_types,
            &func.type_params,
        );
        let core_sig = candidate_sig
            .signature
            .unwrap_or(crate::inference_core::CoreType::Tuple(
                candidate_sig.slots.clone(),
            ));
        let core_vars = Self::scoped_runtime_type_vars(&func.type_params);
        if let Some(vararg_idx) = func.vararg_param_index {
            if crate::inference_core::dispatch_resolver::core_match::vararg_param_mentions_type_vars_core(
                &candidate_sig.slots,
                &core_vars,
                Some(vararg_idx),
            ) {
                return Some(core_sig);
            }
        }
        Some(core_sig.canonicalize_signature_for_dedup())
    }

    fn runtime_candidate_vararg_mentions_type_vars(func: &FunctionInfo, arg_len: usize) -> bool {
        let Some(vararg_idx) = func.vararg_param_index else {
            return false;
        };
        if expanded_param_types_for_call(func, arg_len).is_none() {
            return false;
        }
        crate::inference_core::dispatch_resolver::typed_vararg_where_bonus_julia(
            &func.param_julia_types,
            &func.type_params,
            Some(vararg_idx),
        ) > 0
    }

    pub(crate) fn find_best_method_index_from_candidates(
        &self,
        candidate_indices: &[usize],
        args: &[Value],
    ) -> Result<Option<usize>, VmError> {
        // Note (Issue #6782): there is intentionally NO blanket "candidate set
        // crosses the Base/user origin boundary -> bail to the string resolver"
        // fence here. That coarse fence disabled the metadata scorer for the WHOLE
        // function whenever a user added any method to a Base function (e.g. a user
        // `promote_rule`), which silently broke dispatch of the Base `where`-bounded
        // `Type{T}` methods (`promote_rule(::Type{Bool}, ::Type{T}) where {T<:Number}`,
        // `Complex{T}`/`Rational{T}` rules) — the metadata scorer is the only channel
        // that can resolve them, so they fell through to the generic `Union{}` rule and
        // `promote_type` widened to the `typejoin` (Integer/Number). The two safety
        // invariants the fence was a coarse proxy for are enforced surgically inside
        // the function body instead: the per-candidate native-array wrapper boundary
        // exclusion (Issue #6202, `params_cross_native_array_wrapper_boundary` below)
        // keeps Base array-wrapper candidates on the legacy string resolver, and the
        // Issue #5926 origin dominance fence (`base_runtime_dominance_crosses_user_candidate`,
        // applied inside `dominant_method_index_runtime_for_indices`) prevents a
        // Base-origin method from overriding a user-origin candidate by dominance alone.
        let mut unique_indices = Vec::with_capacity(candidate_indices.len());
        let world = self.current_dispatch_world();
        for &idx in candidate_indices {
            if idx < self.functions.len()
                && self.function_visible_in_world(idx, world)
                && !unique_indices.contains(&idx)
            {
                unique_indices.push(idx);
            }
        }
        let unique_indices = self.dedup_runtime_candidate_indices(unique_indices, args.len());

        if let Some(idx) = self.dominant_method_index_runtime_for_indices(&unique_indices, args) {
            return Ok(Some(idx));
        }

        let arg_types: Vec<_> = args
            .iter()
            .map(|arg| self.dispatch_julia_type_for_value(arg))
            .collect();
        let mut matches = Vec::new();
        for idx in unique_indices {
            let func = &self.functions[idx];
            let Some(param_types) = expanded_param_types_for_call(func, args.len()) else {
                continue;
            };
            if self.is_base_program_function_index(idx)
                && !self.is_native_array_exempt_function(idx)
                && params_cross_native_array_wrapper_boundary(args, &param_types)
            {
                continue;
            }
            let Some(binding_count) =
                self.function_candidate_binding_count(idx, args, &param_types, &func.type_params)
            else {
                continue;
            };
            let (score, specificity) = Self::runtime_signature_score(
                &func.param_julia_types,
                &param_types,
                &arg_types,
                &func.type_params,
                binding_count,
                func.vararg_param_index,
                func.vararg_fixed_count.is_some(),
            );
            let is_vararg = func.vararg_param_index.is_some();
            matches.push(RuntimeCandidateMatch {
                idx,
                param_types,
                score,
                specificity,
                is_vararg,
            });
        }
        // Dominance pre-checks, conflict gate, and final scored pick run
        // through the shared selection pipeline driver
        // (`selection::select_method`, Issue #6502) — the same control flow
        // as the compile-time `MethodTable::dispatch_inner`, with the
        // runtime's own semantics injected as closures.
        let selected = selection::select_method(
            matches.len(),
            || self.runtime_dominance_precheck_index(&matches, &arg_types),
            || {
                self.tuple_vararg_conflicting_candidates(&matches, &arg_types)
                    .then(|| self.runtime_ambiguous_method_error(&matches, &arg_types))
            },
            || {
                // Final scored pick: first-best max score, preferring the
                // non-vararg candidate on a score tie. Candidates carrying an
                // actual-aware dominance pattern that failed its pre-check are
                // suppressed so the score fold cannot resurrect them.
                let suppress_actual_aware_candidates = matches.len() > 1
                    && self.runtime_actual_aware_dominance_candidate_present(&matches);
                let eligible: Vec<&RuntimeCandidateMatch> = matches
                    .iter()
                    .filter(|candidate| {
                        !(suppress_actual_aware_candidates
                            && self.runtime_function_has_actual_aware_dominance(candidate.idx))
                    })
                    .collect();
                let best = selection::pick_best(eligible.iter().copied(), |new, best| {
                    new.score > best.score
                        || (new.score == best.score
                            && ((best.is_vararg && !new.is_vararg)
                                || (new.is_vararg == best.is_vararg
                                    && new.specificity > best.specificity)))
                });
                match best {
                    Some(candidate) => {
                        let tied: Vec<RuntimeCandidateMatch> = eligible
                            .iter()
                            .copied()
                            .filter(|other| {
                                other.score == candidate.score
                                    && other.is_vararg == candidate.is_vararg
                                    && other.specificity == candidate.specificity
                            })
                            .cloned()
                            .collect();
                        if tied.len() > 1 {
                            selection::Selection::Ambiguous(
                                self.runtime_ambiguous_method_error(&tied, &arg_types),
                            )
                        } else {
                            selection::Selection::Selected(candidate.idx)
                        }
                    }
                    None => selection::Selection::NoMatch,
                }
            },
        );

        match selected {
            selection::Selection::NoMatch => Ok(None),
            selection::Selection::Selected(idx) => Ok(Some(idx)),
            selection::Selection::Ambiguous(err) => Err(err),
        }
    }

    /// Build the semantic request shared by direct dynamic calls and
    /// callable-value calls (Issue #10461). The caller supplies the callee
    /// identity produced by lexical resolution; argument types, scope, world,
    /// span, and candidates are projected exactly once here.
    pub(crate) fn runtime_call_request(
        &self,
        callee: crate::inference_core::dispatch_resolver::CalleeIdentity,
        candidate_indices: &[usize],
        args: &[Value],
    ) -> crate::inference_core::dispatch_resolver::CallRequest {
        use crate::inference_core::dispatch_resolver::{
            dispatch_core_type_from_julia, CandidateSet, LexicalScopeId, MethodId,
        };

        let method = self
            .frames
            .last()
            .and_then(|frame| frame.func_index)
            .map(MethodId);
        let module = method
            .and_then(|method| self.functions.get(method.0))
            .and_then(|function| {
                let base = function.name.split('#').next().unwrap_or(&function.name);
                base.rsplit_once('.').map(|(module_path, _)| module_path)
            })
            .map(|path| path.split('.').map(str::to_string).collect())
            .unwrap_or_default();

        crate::inference_core::dispatch_resolver::CallRequest {
            callee,
            positional: args
                .iter()
                .map(|arg| dispatch_core_type_from_julia(&self.dispatch_julia_type_for_value(arg)))
                .collect(),
            keywords: Vec::new(),
            lexical_scope: LexicalScopeId { module, method },
            world: self.current_dispatch_world(),
            call_span: self
                .ip
                .checked_sub(1)
                .and_then(|ip| self.source_map.get(ip).copied().flatten())
                .unwrap_or_else(|| crate::span::Span::new(0, 0, 0, 0, 0, 0)),
            candidates: CandidateSet(candidate_indices.iter().copied().map(MethodId).collect()),
        }
    }

    /// Resolve a semantic runtime call request through the single structured
    /// value-aware scorer. Adapters retain their execution/fallback policies,
    /// but no longer invoke the scorer without a `CallRequest` (Issue #10461).
    pub(crate) fn resolve_runtime_call_request(
        &self,
        request: &crate::inference_core::dispatch_resolver::CallRequest,
        args: &[Value],
    ) -> Result<Option<usize>, VmError> {
        debug_assert_eq!(request.positional.len(), args.len());
        debug_assert_eq!(request.world, self.current_dispatch_world());
        let candidate_indices: Vec<usize> =
            request.candidates.0.iter().map(|method| method.0).collect();
        self.find_best_method_index_from_candidates(&candidate_indices, args)
    }

    /// Run the runtime morespecific dominance pre-check rules in their
    /// historical order, returning the first rule that finds a unique
    /// dominant candidate (Issue #5926 family; control flow shared via
    /// [`selection::unique_dominant_index`], Issue #6502). The runtime
    /// mirror of `runtime_types::method_table::dominance_precheck_index`.
    pub(super) fn runtime_dominance_precheck_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        self.runtime_signature_dominant_candidate_index(matches, arg_types)
            .or_else(|| self.tuple_vararg_dominant_candidate_index(matches, arg_types))
            .or_else(|| self.tuple_diagonal_dominant_candidate_index(matches, arg_types))
            .or_else(|| self.vector_diagonal_dominant_candidate_index(matches, arg_types))
            .or_else(|| self.union_actual_dominant_candidate_index(matches, arg_types))
            .or_else(|| self.type_value_diagonal_dominant_candidate_index(matches, arg_types))
            .or_else(|| self.type_vector_diagonal_dominant_candidate_index(matches, arg_types))
            .or_else(|| self.type_matrix_diagonal_dominant_candidate_index(matches, arg_types))
    }

    pub(super) fn runtime_signature_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        if matches.iter().any(|candidate| candidate.is_vararg) {
            return None;
        }

        let sigs = matches
            .iter()
            .map(|candidate| {
                let func = self.functions.get(candidate.idx)?;
                let core_sig = dispatch_binding::build_runtime_candidate_core_signature(
                    &candidate.param_types,
                    &func.type_params,
                );
                Some(
                    core_sig
                        .signature
                        .unwrap_or(CoreType::Tuple(core_sig.slots)),
                )
            })
            .collect::<Option<Vec<_>>>()?;
        let arg_tuple = CoreType::Tuple(
            arg_types
                .iter()
                .map(dispatch_binding::runtime_actual_core_type)
                .collect(),
        );

        selection::unique_dominant_index(
            sigs.len(),
            |i| arg_tuple.is_subtype_of_with_hierarchy(&sigs[i], &self.struct_hierarchy),
            |i, j| {
                sigs[i].strict_subtype_dominates_with_hierarchy(&sigs[j], &self.struct_hierarchy)
            },
        )
        .map(|pos| matches[pos].idx)
    }

    fn runtime_signature_score(
        declared_param_types: &[crate::types::JuliaType],
        param_types: &[crate::types::JuliaType],
        arg_types: &[crate::types::JuliaType],
        type_params: &[crate::types::TypeParam],
        binding_count: usize,
        vararg_param_index: Option<usize>,
        fixed_varargs: bool,
    ) -> (u32, i32) {
        let mut score =
            crate::inference_core::dispatch_resolver::score_julia_signature_with_binding_count(
                param_types,
                arg_types,
                binding_count,
                vararg_param_index.is_some(),
                fixed_varargs,
            )
            .score;
        score = score.saturating_add(
            crate::inference_core::dispatch_resolver::typed_vararg_where_bonus_julia(
                declared_param_types,
                type_params,
                vararg_param_index,
            ),
        );

        let candidate_sig =
            dispatch_binding::build_runtime_candidate_core_signature(param_types, type_params);
        let specificity =
            crate::inference_core::dispatch_resolver::core_signature_pattern_specificity(
                &candidate_sig.slots,
            );
        let exact_leaf_struct_count = candidate_sig
            .slots
            .iter()
            .zip(
                arg_types
                    .iter()
                    .map(dispatch_binding::runtime_actual_core_type),
            )
            .filter(|(param, actual)| {
                *param == actual
                    && matches!(
                        param,
                        crate::inference_core::CoreType::Struct { params, .. }
                            if params.is_empty()
                    )
            })
            .count();
        score = score.saturating_add((exact_leaf_struct_count as u32) * 10);
        let range_family_specific_count = param_types
            .iter()
            .zip(arg_types.iter())
            .filter(|(param, actual)| {
                Self::julia_type_is_range_family(actual) && Self::julia_type_is_range_family(param)
            })
            .count();
        score = score.saturating_add((range_family_specific_count as u32) * 10);
        (score, specificity)
    }

    fn scoped_runtime_param_cores(
        param_types: &[crate::types::JuliaType],
        type_params: &[crate::types::TypeParam],
    ) -> Vec<crate::inference_core::CoreType> {
        param_types
            .iter()
            .map(|ty| {
                let core =
                    crate::inference_core::dispatch_resolver::dispatch_core_type_from_julia(ty);
                crate::inference_core::dispatch_resolver::embed_type_param_bounds(core, type_params)
            })
            .collect()
    }

    fn scoped_runtime_type_vars(
        type_params: &[crate::types::TypeParam],
    ) -> Vec<crate::inference_core::CoreTypeVar> {
        type_params
            .iter()
            .map(crate::inference_core::CoreTypeVar::from)
            .collect()
    }

    pub(super) fn tuple_vararg_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        let expanded = self.tuple_vararg_expansions_for_candidates(matches, arg_types)?;

        selection::unique_dominant_index(
            matches.len(),
            |_| true,
            |i, j| {
                specificity::tuple_vararg_pattern_dominates(
                    &expanded[i],
                    &expanded[j],
                    &self.struct_hierarchy,
                )
            },
        )
        .map(|pos| matches[pos].idx)
    }

    pub(super) fn tuple_diagonal_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        let [crate::types::JuliaType::TupleOf(actual_elems)] = arg_types else {
            return None;
        };

        // Per-candidate diagonal pattern, kept only when the actual tuple
        // satisfies it (the candidate's eligibility to *win*; pattern-less
        // candidates still participate as dominance opponents).
        let patterns: Vec<Option<_>> = matches
            .iter()
            .map(|candidate| {
                let func = self.functions.get(candidate.idx)?;
                let pattern = specificity::repeated_tuple_typevar_pattern(
                    &func.param_julia_types,
                    &func.type_params,
                )?;
                specificity::actual_tuple_satisfies_diagonal_pattern(
                    actual_elems,
                    &pattern,
                    &self.struct_hierarchy,
                )
                .then_some(pattern)
            })
            .collect();

        selection::unique_dominant_index(
            matches.len(),
            |i| patterns[i].is_some(),
            |i, j| {
                let Some(pattern) = &patterns[i] else {
                    return false;
                };
                let Some(other_func) = self.functions.get(matches[j].idx) else {
                    return false;
                };
                specificity::tuple_diagonal_candidate_dominates_other(
                    &other_func.param_julia_types,
                    &other_func.type_params,
                    pattern,
                    &self.struct_hierarchy,
                )
            },
        )
        .map(|pos| matches[pos].idx)
    }

    pub(super) fn vector_diagonal_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        let patterns: Vec<Option<_>> = matches
            .iter()
            .map(|candidate| {
                let func = self.functions.get(candidate.idx)?;
                let pattern = specificity::repeated_vector_typevar_pattern(
                    &func.param_julia_types,
                    &func.type_params,
                )?;
                specificity::actual_vector_slots_share_element_type(arg_types, &pattern.slots)
                    .then_some(pattern)
            })
            .collect();

        selection::unique_dominant_index(
            matches.len(),
            |i| patterns[i].is_some(),
            |i, j| {
                let Some(pattern) = &patterns[i] else {
                    return false;
                };
                specificity::independent_vector_bounds_are_no_tighter(
                    &matches[j].param_types,
                    pattern,
                    &self.struct_hierarchy,
                )
            },
        )
        .map(|pos| matches[pos].idx)
    }

    pub(super) fn union_actual_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        selection::unique_dominant_index(
            matches.len(),
            |i| {
                matches[i]
                    .param_types
                    .iter()
                    .any(|ty| matches!(ty, crate::types::JuliaType::Union(_)))
            },
            |i, j| {
                specificity::union_actual_candidate_dominates(
                    &matches[i].param_types,
                    &matches[j].param_types,
                    arg_types,
                    &self.struct_hierarchy,
                )
            },
        )
        .map(|pos| matches[pos].idx)
    }

    pub(super) fn type_value_diagonal_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        if arg_types.len() != 2 {
            return None;
        }

        let patterns: Vec<Option<_>> = matches
            .iter()
            .map(|candidate| {
                let func = self.functions.get(candidate.idx)?;
                let pattern = specificity::type_value_diagonal_pattern(
                    &func.param_julia_types,
                    &func.type_params,
                )?;
                let binding = specificity::actual_type_value_diagonal_binding(
                    arg_types,
                    &pattern,
                    &self.struct_hierarchy,
                )?;
                Some((pattern, binding))
            })
            .collect();

        selection::unique_dominant_index(
            matches.len(),
            |i| patterns[i].is_some(),
            |i, j| {
                let Some((pattern, binding)) = &patterns[i] else {
                    return false;
                };
                specificity::type_value_diagonal_candidate_dominates_other(
                    &matches[j].param_types,
                    pattern,
                    binding,
                    &self.struct_hierarchy,
                )
            },
        )
        .map(|pos| matches[pos].idx)
    }

    pub(super) fn type_vector_diagonal_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        if arg_types.len() != 2 {
            return None;
        }

        let patterns: Vec<Option<_>> = matches
            .iter()
            .map(|candidate| {
                let func = self.functions.get(candidate.idx)?;
                let param_cores =
                    Self::scoped_runtime_param_cores(&candidate.param_types, &func.type_params);
                let type_vars = Self::scoped_runtime_type_vars(&func.type_params);
                let pattern =
                    specificity::core_type_vector_diagonal_pattern(&param_cores, &type_vars)?;
                let binding = specificity::actual_type_vector_diagonal_binding(
                    arg_types,
                    &pattern,
                    &self.struct_hierarchy,
                )?;
                Some((pattern, crate::inference_core::CoreType::from(binding)))
            })
            .collect();

        selection::unique_dominant_index(
            matches.len(),
            |i| patterns[i].is_some(),
            |i, j| {
                let Some((pattern, binding)) = &patterns[i] else {
                    return false;
                };
                let Some(func) = self.functions.get(matches[j].idx) else {
                    return false;
                };
                let other_cores =
                    Self::scoped_runtime_param_cores(&matches[j].param_types, &func.type_params);
                specificity::core_type_vector_diagonal_candidate_dominates_other(
                    &other_cores,
                    pattern,
                    binding,
                    &self.struct_hierarchy,
                )
            },
        )
        .map(|pos| matches[pos].idx)
    }

    pub(super) fn type_matrix_diagonal_dominant_candidate_index(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<usize> {
        if arg_types.len() != 2 {
            return None;
        }

        let patterns: Vec<Option<_>> = matches
            .iter()
            .map(|candidate| {
                let func = self.functions.get(candidate.idx)?;
                let param_cores =
                    Self::scoped_runtime_param_cores(&candidate.param_types, &func.type_params);
                let type_vars = Self::scoped_runtime_type_vars(&func.type_params);
                let pattern =
                    specificity::core_type_matrix_diagonal_pattern(&param_cores, &type_vars)?;
                let binding = specificity::actual_type_matrix_diagonal_binding(
                    arg_types,
                    &pattern,
                    &self.struct_hierarchy,
                )?;
                Some((pattern, crate::inference_core::CoreType::from(binding)))
            })
            .collect();

        selection::unique_dominant_index(
            matches.len(),
            |i| patterns[i].is_some(),
            |i, j| {
                let Some((pattern, binding)) = &patterns[i] else {
                    return false;
                };
                let Some(func) = self.functions.get(matches[j].idx) else {
                    return false;
                };
                let other_cores =
                    Self::scoped_runtime_param_cores(&matches[j].param_types, &func.type_params);
                specificity::core_type_matrix_diagonal_candidate_dominates_other(
                    &other_cores,
                    pattern,
                    binding,
                    &self.struct_hierarchy,
                )
            },
        )
        .map(|pos| matches[pos].idx)
    }

    pub(super) fn tuple_vararg_conflicting_candidates(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> bool {
        let Some(expanded) = self.tuple_vararg_expansions_for_candidates(matches, arg_types) else {
            return false;
        };

        for i in 0..expanded.len() {
            for j in (i + 1)..expanded.len() {
                if specificity::tuple_vararg_patterns_conflict(
                    &expanded[i],
                    &expanded[j],
                    &self.struct_hierarchy,
                ) {
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn tuple_vararg_expansions_for_candidates(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> Option<Vec<specificity::TupleVarargExpansion>> {
        let [actual_arg] = arg_types else {
            return None;
        };
        let crate::types::JuliaType::TupleOf(actual_elems) = actual_arg else {
            return None;
        };

        let mut expanded = Vec::new();
        for candidate in matches {
            let func = self.functions.get(candidate.idx)?;
            if func.vararg_param_index.is_some() || !func.type_params.is_empty() {
                return None;
            }
            let [param_ty] = candidate.param_types.as_slice() else {
                return None;
            };
            let crate::types::JuliaType::TupleOf(pattern_elems) = param_ty else {
                return None;
            };
            expanded.push(specificity::expand_tuple_vararg_pattern_for_len(
                pattern_elems,
                actual_elems.len(),
            )?);
        }
        Some(expanded)
    }

    pub(super) fn runtime_ambiguous_method_error(
        &self,
        matches: &[RuntimeCandidateMatch],
        arg_types: &[crate::types::JuliaType],
    ) -> VmError {
        let name = matches
            .first()
            .and_then(|candidate| self.functions.get(candidate.idx))
            .map(|func| func.name.as_str())
            .unwrap_or("<runtime>");
        let types: Vec<_> = arg_types.iter().map(|ty| format!("::{}", ty)).collect();
        let mut msg = format!("{}({}) is ambiguous. Candidates:\n", name, types.join(", "));
        for candidate in matches {
            if let Some(func) = self.functions.get(candidate.idx) {
                let sig: Vec<_> = func
                    .param_julia_types
                    .iter()
                    .map(|ty| format!("::{}", ty))
                    .collect();
                msg.push_str(&format!("  {}({})\n", func.name, sig.join(", ")));
            }
        }
        VmError::MethodError(msg)
    }

    pub(super) fn is_base_program_function_index(&self, idx: usize) -> bool {
        idx < self.base_function_count
    }

    /// True for Base's three generic `convert` fallbacks whose constructor
    /// bodies cannot represent a Union target. Structural Union conversion may
    /// replace these methods, but never a user-defined or target-specific
    /// method selected by normal dispatch (Issues #11781/#10835).
    pub(super) fn is_base_generic_convert_fallback(&self, idx: usize) -> bool {
        if !self.is_base_program_function_index(idx) {
            return false;
        }
        let Some(func) = self.functions.get(idx) else {
            return false;
        };
        // Callers pass only a method selected from the convert family, so its
        // structural signature is sufficient here (Issue #10870 inventory).
        if func.type_params.len() != 1 {
            return false;
        }
        let [JuliaType::TypeOf(target), source] = func.param_julia_types.as_slice() else {
            return false;
        };
        let JuliaType::TypeVar(target_name, _) = target.as_ref() else {
            return false;
        };
        let type_param = &func.type_params[0];
        if type_param.name != *target_name {
            return false;
        }
        match source {
            JuliaType::TypeVar(source_name, _) => source_name == target_name,
            JuliaType::Number | JuliaType::Bool => {
                type_param.upper_bound.as_deref() == Some("Number")
            }
            _ => false,
        }
    }

    /// Precomputed native-array boundary exemption for a function index
    /// (Issue #6336). Out-of-range (e.g. test harnesses that build the
    /// function table by hand without refreshing the flags) means "not
    /// exempt", preserving the dispatch fence.
    pub(super) fn is_native_array_exempt_function(&self, idx: usize) -> bool {
        self.native_array_exempt_functions
            .get(idx)
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn base_runtime_dominance_crosses_user_candidate(
        &self,
        candidates: &[(usize, CoreType)],
        candidate_idx: usize,
    ) -> bool {
        self.base_function_count > 0
            && self.is_base_program_function_index(candidates[candidate_idx].0)
            && candidates
                .iter()
                .any(|(idx, _)| !self.is_base_program_function_index(*idx))
    }

    pub(super) fn find_best_method_index_uncached(
        &self,
        names: &[&str],
        args: &[Value],
    ) -> Option<usize> {
        if let Some(idx) = self.dominant_method_index_runtime(names, args) {
            return Some(idx);
        }

        // Use function_name_index for O(1) lookup per name (Issue #3361).
        // Lazy match + score enumeration; the first-best winnowing (max
        // score, preferring non-vararg on a tie) is owned by the shared
        // selection core (`selection::pick_best`, Issue #6502).
        let scored = names
            .iter()
            .flat_map(|name| self.get_function_indices_by_name(name).iter().copied())
            .filter(|&idx| self.function_visible_in_world(idx, self.current_dispatch_world()))
            .filter_map(|idx| {
                let func = &self.functions[idx];
                let param_types = expanded_param_types_for_call(func, args.len())?;
                if self.is_base_program_function_index(idx)
                    && !self.is_native_array_exempt_function(idx)
                    && params_cross_native_array_wrapper_boundary(args, &param_types)
                {
                    return None;
                }

                let binding_count = self.function_candidate_binding_count(
                    idx,
                    args,
                    &param_types,
                    &func.type_params,
                )?;

                let arg_types: Vec<_> = args
                    .iter()
                    .map(|arg| self.dispatch_julia_type_for_value(arg))
                    .collect();
                let (score, specificity) = Self::runtime_signature_score(
                    &func.param_julia_types,
                    &param_types,
                    &arg_types,
                    &func.type_params,
                    binding_count,
                    func.vararg_param_index,
                    func.vararg_fixed_count.is_some(),
                );
                let is_vararg = func.vararg_param_index.is_some();
                Some((idx, score, specificity, is_vararg))
            });

        selection::pick_best(scored, |new, best| {
            new.1 > best.1
                || (new.1 == best.1 && ((best.3 && !new.3) || (new.3 == best.3 && new.2 > best.2)))
        })
        .map(|(idx, _, _, _)| idx)
    }

    pub(super) fn dims_from_values(
        &self,
        values: &[Value],
        context: &str,
    ) -> Result<Vec<usize>, VmError> {
        if let [Value::Tuple(tuple)] = values {
            return self.dims_from_values(&tuple.elements, context);
        }

        let mut dims = Vec::with_capacity(values.len());
        for value in values {
            let dim = self.convert_to_i64(value)?;
            if dim < 0 {
                return Err(VmError::TypeError(format!(
                    "{}: dimension must be non-negative, got {}",
                    context, dim
                )));
            }
            dims.push(usize::try_from(dim).map_err(|_| {
                VmError::TypeError(format!(
                    "{}: dimension out of range for usize, got {}",
                    context, dim
                ))
            })?);
        }
        Ok(dims)
    }

    pub(super) fn values_match_params_binding_count(
        &self,
        args: &[Value],
        param_types: &[crate::types::JuliaType],
        type_params: &[crate::types::TypeParam],
    ) -> Option<usize> {
        let mut bindings: HashMap<String, crate::types::JuliaType> = HashMap::new();

        for (arg, param_ty) in args.iter().zip(param_types.iter()) {
            if !self.value_matches_param_with_bindings(arg, param_ty, type_params, &mut bindings) {
                return None;
            }
        }

        if !bindings.is_empty()
            && !crate::types::JuliaType::check_diagonal_rule_for_params(param_types, &bindings)
        {
            return None;
        }

        Some(bindings.len())
    }

    pub(super) fn function_candidate_has_nominal_origin_conflict(
        &self,
        function_index: usize,
        args: &[Value],
        param_types: &[crate::types::JuliaType],
        type_params: &[crate::types::TypeParam],
    ) -> bool {
        if self.functions.get(function_index).is_none()
            || !self.is_base_program_function_index(function_index)
        {
            return false;
        }
        args.iter().zip(param_types).any(|(arg, param)| {
            let actual = self.dispatch_julia_type_for_value(arg);
            let actual_outer_family = match &actual {
                crate::types::JuliaType::Struct(name)
                | crate::types::JuliaType::AbstractUser(name, _)
                | crate::types::JuliaType::RuntimeParametric { base: name, .. } => {
                    Some(crate::types::qualified_family_name(name))
                }
                _ => None,
            };
            let actual_value_type_id = match arg {
                Value::Struct(instance) => Some(instance.type_id),
                Value::StructRef(heap_index) => self
                    .struct_heap
                    .get(*heap_index)
                    .map(|instance| instance.type_id),
                _ => None,
            };
            let mut pattern_matches =
                |pattern: &crate::types::JuliaType, actual: &crate::types::JuliaType| {
                    crate::inference_core::dispatch_resolver::julia_signature_match_with_bindings(
                        std::slice::from_ref(pattern),
                        std::slice::from_ref(actual),
                        type_params,
                    )
                    .is_some()
                };
            let mut origin_conflicts = |base_family: &str, actual_family: &str| {
                let expected_type_id = match param {
                    _ if !base_family.contains('.') => self.struct_defs.iter().position(|def| {
                        crate::types::qualified_family_name(&def.name) == base_family
                            && !crate::types::qualified_family_name(&def.name).contains('.')
                    }),
                    _ => None,
                };
                let actual_type_id = if actual_outer_family == Some(actual_family) {
                    actual_value_type_id
                } else {
                    self.struct_defs.iter().position(|def| {
                        crate::types::qualified_family_name(&def.name) == actual_family
                    })
                };
                expected_type_id.is_some_and(|type_id| Some(type_id) != actual_type_id)
            };
            crate::types::base_bare_nominal_origin_conflict_with(
                param,
                &actual,
                &mut pattern_matches,
                &mut origin_conflicts,
            )
        })
    }

    pub(super) fn function_candidate_binding_count(
        &self,
        function_index: usize,
        args: &[Value],
        param_types: &[crate::types::JuliaType],
        type_params: &[crate::types::TypeParam],
    ) -> Option<usize> {
        self.functions.get(function_index)?;
        if self.function_candidate_has_nominal_origin_conflict(
            function_index,
            args,
            param_types,
            type_params,
        ) {
            return None;
        }
        self.values_match_params_binding_count(args, param_types, type_params)
    }

    pub(crate) fn dispatch_julia_type_for_value(&self, value: &Value) -> crate::types::JuliaType {
        if let Some(arr) = native_array_value_ref(value) {
            let arr_ref = arr.borrow();
            if let Some(container_type) = arr_ref.array_type_override() {
                return crate::types::JuliaType::Struct(container_type.to_string());
            }
            let elem_jtype = self.array_value_declared_element_julia_type(&arr_ref);
            return julia_array_type_for_ndims(elem_jtype, arr_ref.shape.len());
        }
        match value {
            Value::DataType(jt) if matches!(jt.as_ref(), crate::types::JuliaType::TypeOf(_)) => {
                *jt.clone()
            }
            Value::DataType(jt) => crate::types::JuliaType::TypeOf(Box::new(*jt.clone())),
            _ => self.get_value_julia_type(value),
        }
    }

    /// Thin adapter over the shared runtime matcher (Issue #5915): derives the
    /// argument's runtime type once and keeps only the value-representation
    /// fallback (`value_matches_param`) VM-owned. The binding-aware structural
    /// matching lives in `inference_core::dispatch_resolver`.
    pub(super) fn value_matches_param_with_bindings(
        &self,
        arg: &Value,
        param_ty: &crate::types::JuliaType,
        type_params: &[crate::types::TypeParam],
        bindings: &mut HashMap<String, crate::types::JuliaType>,
    ) -> bool {
        let arg_value_type = self.dispatch_julia_type_for_value(arg);
        let arg_type_object = match arg {
            Value::DataType(dt) => Some(dt),
            _ => None,
        };
        crate::inference_core::dispatch_resolver::runtime_value_type_matches_param_with_bindings(
            &self.struct_hierarchy,
            &arg_value_type,
            arg_type_object.map(|v| &**v),
            param_ty,
            type_params,
            bindings,
            || self.value_matches_param(arg, param_ty),
        )
    }

    /// Find binary operator method and cache result by operand types.
    ///
    /// Positive cache only: misses are not cached so newly added methods remain visible.
    pub(super) fn find_cached_binary_method_index(
        &mut self,
        op: BinaryDispatchOp,
        names: &[&str],
        left: &Value,
        right: &Value,
    ) -> Option<usize> {
        let key = BinaryDispatchKey {
            op,
            left: self.get_value_type(left),
            right: self.get_value_type(right),
        };

        if let Some(idx) = self.binary_method_cache.get(&key) {
            crate::vm::profiler::record_event("BinaryMethodCacheHit");
            return Some(*idx);
        }

        crate::vm::profiler::record_event("BinaryMethodCacheMiss");
        let args = [left.clone(), right.clone()];
        let found = self.find_best_method_index(names, &args);
        if let Some(idx) = found {
            crate::vm::profiler::record_event("BinaryMethodCacheFill");
            self.binary_method_cache.insert(key, idx);
            self.enforce_binary_method_cache_limit();
        }
        found
    }

    /// Extract and bind type parameter values from arguments to a frame.
    /// This enables `where T` type parameters to be used as values inside the function body.
    /// Must be called after frame creation and before pushing the frame for execution.
    pub(super) fn bind_type_params(&self, func: &FunctionInfo, args: &[Value], frame: &mut Frame) {
        if func.type_params.is_empty() {
            return;
        }
        for (idx, arg) in args.iter().enumerate() {
            if let Some(param_jtype) = func.param_julia_types.get(idx) {
                let arg_jtype = self.get_value_julia_type(arg);

                // Special handling for Val{N} - extract integer, boolean, or symbol value directly
                if let crate::types::JuliaType::Struct(param_type_name) = param_jtype {
                    if param_type_name.starts_with("Val{") && param_type_name.ends_with("}") {
                        let param_type_arg = &param_type_name[4..param_type_name.len() - 1];
                        if func.type_params.iter().any(|tp| tp.name == param_type_arg) {
                            if let crate::types::JuliaType::Struct(arg_type_name) = &arg_jtype {
                                if arg_type_name.starts_with("Val{") && arg_type_name.ends_with("}")
                                {
                                    let arg_value_str = &arg_type_name[4..arg_type_name.len() - 1];
                                    if let Some(value) =
                                        parse_val_constructor_parameter(arg_value_str)
                                    {
                                        bind_val_parameter_value(frame, param_type_arg, value);
                                        continue;
                                    }
                                    if let Ok(int_val) = arg_value_str.parse::<i64>() {
                                        frame.locals_any.insert(
                                            param_type_arg.to_string(),
                                            Value::I64(int_val),
                                        );
                                        frame.var_types.insert(
                                            param_type_arg.to_string(),
                                            frame::VarTypeTag::I64,
                                        );
                                        continue;
                                    }
                                    if let Ok(float_val) = arg_value_str.parse::<f64>() {
                                        frame.locals_any.insert(
                                            param_type_arg.to_string(),
                                            Value::F64(float_val),
                                        );
                                        frame.var_types.insert(
                                            param_type_arg.to_string(),
                                            frame::VarTypeTag::F64,
                                        );
                                        continue;
                                    }
                                    if arg_value_str == "true" {
                                        frame
                                            .locals_any
                                            .insert(param_type_arg.to_string(), Value::Bool(true));
                                        frame.var_types.insert(
                                            param_type_arg.to_string(),
                                            frame::VarTypeTag::Bool,
                                        );
                                        continue;
                                    } else if arg_value_str == "false" {
                                        frame
                                            .locals_any
                                            .insert(param_type_arg.to_string(), Value::Bool(false));
                                        frame.var_types.insert(
                                            param_type_arg.to_string(),
                                            frame::VarTypeTag::Bool,
                                        );
                                        continue;
                                    }
                                    if let Some(char_val) = parse_val_char_parameter(arg_value_str)
                                    {
                                        frame.locals_any.insert(
                                            param_type_arg.to_string(),
                                            Value::Char(char_val),
                                        );
                                        frame.var_types.insert(
                                            param_type_arg.to_string(),
                                            frame::VarTypeTag::Char,
                                        );
                                        continue;
                                    }
                                    if let Some(tuple_val) =
                                        parse_val_tuple_parameter(arg_value_str)
                                    {
                                        frame.locals_any.insert(
                                            param_type_arg.to_string(),
                                            Value::Tuple(tuple_val),
                                        );
                                        frame.var_types.insert(
                                            param_type_arg.to_string(),
                                            frame::VarTypeTag::Tuple,
                                        );
                                        continue;
                                    }
                                    if arg_value_str.starts_with(':') {
                                        let symbol_name = arg_value_str.trim_start_matches(':');
                                        frame.locals_any.insert(
                                            param_type_arg.to_string(),
                                            Value::Symbol(SymbolValue::new(symbol_name)),
                                        );
                                        frame.var_types.insert(
                                            param_type_arg.to_string(),
                                            frame::VarTypeTag::ValSymbol,
                                        );
                                        continue;
                                    }
                                }
                            }
                        }
                    }

                    if param_type_name.starts_with("NTuple{") && param_type_name.ends_with("}") {
                        if let Value::Tuple(tuple) = arg {
                            if self.bind_ntuple_params(param_type_name, tuple, func, frame) {
                                continue;
                            }
                        }
                    }
                }

                // `::Type{...}` binds type parameters from the concrete type object
                // passed as a value, including nested patterns like `Type{Array{T}}`.
                if let crate::types::JuliaType::TypeOf(inner) = param_jtype {
                    if let Value::DataType(jt) = arg {
                        bind_array_rank_type_param(frame, inner, jt, func);
                        if let Some(bindings) = jt
                            .extract_type_bindings_in(
                                inner,
                                &func.type_params,
                                &self.struct_hierarchy,
                            )
                            .or_else(|| {
                                self.extract_type_bindings_for_selected_method(
                                    jt,
                                    inner,
                                    &func.type_params,
                                )
                            })
                        {
                            let mut inserted = false;
                            for (name, bound_type) in &bindings {
                                if !self.static_type_binding_satisfies_declared_bounds(
                                    name,
                                    bound_type,
                                    &bindings,
                                    &func.type_params,
                                ) {
                                    continue;
                                }
                                insert_frame_type_binding(frame, name.clone(), bound_type.clone());
                                inserted = true;
                            }
                            if inserted {
                                continue;
                            }
                        }
                    }
                }

                bind_array_rank_type_param(frame, param_jtype, &arg_jtype, func);

                if let Some(bindings) = arg_jtype
                    .extract_type_bindings_in(
                        param_jtype,
                        &func.type_params,
                        &self.struct_hierarchy,
                    )
                    .or_else(|| {
                        self.extract_type_bindings_for_selected_method(
                            &arg_jtype,
                            param_jtype,
                            &func.type_params,
                        )
                    })
                {
                    for (name, bound_type) in &bindings {
                        if !self.static_type_binding_satisfies_declared_bounds(
                            name,
                            bound_type,
                            &bindings,
                            &func.type_params,
                        ) {
                            continue;
                        }
                        // Value type parameters (e.g. `N` in `Arr{T,N}` bound to
                        // `2` from `Arr{Int,2}`, `sym` in `Foo{:hello}`, `v` in
                        // `VP{1.5}` / `VP{(1, 2)}` / `VP{Int8(5)}`) must
                        // materialize as raw values in the method body, not
                        // `DataType` wrappers, for EVERY bindable kind — not just
                        // the integer (Issue #6625) and Symbol (Issue #8869)
                        // subsets: Float64/Bool/Char/Tuple and constructor-form
                        // narrow numerics previously leaked DataType wrappers
                        // into the body (Issue #10599). Type parameters
                        // (`T` -> `Int64`) keep their `DataType` binding: a
                        // genuine type name never parses as a value literal.
                        // Mirrors the `Val{N}` / `NTuple{N,…}` arms above.
                        if let Some(value) = parse_value_type_param_literal(&bound_type.name()) {
                            bind_val_parameter_value(frame, name, value);
                        }
                        insert_frame_type_binding(frame, name.clone(), bound_type.clone());
                    }
                }
            }
        }

        // Two-phase check-then-remove (Issue #8658): the bound check consults
        // OTHER bindings in the map it is handed (cross-variable `where`
        // bounds), so removing entries while iterating a `HashMap` clone made
        // the surviving binding set depend on the seed-dependent iteration
        // order. Judge every binding against the same immutable snapshot,
        // then remove the rejected ones.
        let bindings = frame.type_bindings.clone();
        let rejected: Vec<&String> = bindings
            .keys()
            .filter(|name| {
                !self.static_type_binding_satisfies_declared_bounds(
                    name,
                    &bindings[*name],
                    &bindings,
                    &func.type_params,
                )
            })
            .collect();
        for name in rejected {
            frame.type_bindings.remove(name);
            frame.locals_any.remove(name);
            frame.var_types.remove(name);
        }
    }

    pub(crate) fn static_type_binding_satisfies_declared_bounds(
        &self,
        name: &str,
        bound_type: &crate::types::JuliaType,
        bindings: &HashMap<String, crate::types::JuliaType>,
        type_params: &[crate::types::TypeParam],
    ) -> bool {
        let Some(type_param) = type_params.iter().find(|tp| tp.name == name) else {
            return true;
        };

        let engine =
            crate::inference_core::CoreSubtypeEngine::with_hierarchy(&self.struct_hierarchy);
        let binding_core = crate::inference_core::CoreType::from(bound_type);

        if let Some(lower) = type_param.lower_bound.as_deref() {
            let lower_core =
                self.resolve_static_type_param_bound_core(lower, bindings, type_params, false, 0);
            if !engine.is_subtype(&lower_core, &binding_core) {
                return false;
            }
        }

        if let Some(upper) = type_param.get_upper_bound() {
            let unresolved_upper = crate::inference_core::CoreType::from_julia_name(upper);
            if !static_core_mentions_type_param(&unresolved_upper, type_params) {
                let upper_core = self.resolve_static_type_param_bound_core(
                    upper,
                    bindings,
                    type_params,
                    true,
                    0,
                );
                if !engine.is_subtype(&binding_core, &upper_core) {
                    return false;
                }
            }
        }

        for other_param in type_params {
            if other_param.name == name {
                continue;
            }
            let Some(other_binding) = bindings.get(&other_param.name) else {
                continue;
            };
            let other_binding_core = crate::inference_core::CoreType::from(other_binding);

            if let Some(lower) = other_param.lower_bound.as_deref() {
                let unresolved_lower = crate::inference_core::CoreType::from_julia_name(lower);
                if static_core_mentions_named_type_param(&unresolved_lower, name) {
                    let lower_core = self.resolve_static_type_param_bound_core(
                        lower,
                        bindings,
                        type_params,
                        false,
                        0,
                    );
                    if !engine.is_subtype(&lower_core, &other_binding_core) {
                        return false;
                    }
                }
            }

            if let Some(upper) = other_param.get_upper_bound() {
                let unresolved_upper = crate::inference_core::CoreType::from_julia_name(upper);
                if static_core_mentions_named_type_param(&unresolved_upper, name) {
                    let upper_core = self.resolve_static_type_param_bound_core(
                        upper,
                        bindings,
                        type_params,
                        true,
                        0,
                    );
                    if !engine.is_subtype(&other_binding_core, &upper_core) {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn resolve_static_type_param_bound_core(
        &self,
        bound_name: &str,
        bindings: &HashMap<String, crate::types::JuliaType>,
        type_params: &[crate::types::TypeParam],
        use_upper_fallback: bool,
        depth: usize,
    ) -> crate::inference_core::CoreType {
        let bound = crate::inference_core::CoreType::from_julia_name(bound_name);
        self.resolve_static_core_type_params(
            bound,
            bindings,
            type_params,
            use_upper_fallback,
            depth,
        )
    }

    fn resolve_static_core_type_params(
        &self,
        ty: crate::inference_core::CoreType,
        bindings: &HashMap<String, crate::types::JuliaType>,
        type_params: &[crate::types::TypeParam],
        use_upper_fallback: bool,
        depth: usize,
    ) -> crate::inference_core::CoreType {
        if depth > 16 {
            return ty;
        }

        match ty {
            crate::inference_core::CoreType::Named(name) => self
                .resolve_static_named_type_param_core(
                    &name,
                    bindings,
                    type_params,
                    use_upper_fallback,
                    depth,
                )
                .unwrap_or(crate::inference_core::CoreType::Named(name)),
            crate::inference_core::CoreType::TypeVar(var) => self
                .resolve_static_named_type_param_core(
                    &var.name,
                    bindings,
                    type_params,
                    use_upper_fallback,
                    depth,
                )
                .unwrap_or(crate::inference_core::CoreType::TypeVar(var)),
            crate::inference_core::CoreType::AbstractUser { name, parent } => {
                crate::inference_core::CoreType::AbstractUser {
                    name,
                    parent: parent.map(|parent| {
                        Box::new(self.resolve_static_core_type_params(
                            *parent,
                            bindings,
                            type_params,
                            use_upper_fallback,
                            depth + 1,
                        ))
                    }),
                }
            }
            crate::inference_core::CoreType::Struct { name, params } => {
                crate::inference_core::CoreType::Struct {
                    name,
                    params: params
                        .into_iter()
                        .map(|param| {
                            self.resolve_static_core_type_params(
                                param,
                                bindings,
                                type_params,
                                use_upper_fallback,
                                depth + 1,
                            )
                        })
                        .collect(),
                }
            }
            crate::inference_core::CoreType::Tuple(elements) => {
                crate::inference_core::CoreType::Tuple(
                    elements
                        .into_iter()
                        .map(|element| {
                            self.resolve_static_core_type_params(
                                element,
                                bindings,
                                type_params,
                                use_upper_fallback,
                                depth + 1,
                            )
                        })
                        .collect(),
                )
            }
            crate::inference_core::CoreType::Vararg(element) => {
                crate::inference_core::CoreType::Vararg(Box::new(
                    self.resolve_static_core_type_params(
                        *element,
                        bindings,
                        type_params,
                        use_upper_fallback,
                        depth + 1,
                    ),
                ))
            }
            crate::inference_core::CoreType::VarargLen { element, len } => {
                crate::inference_core::CoreType::VarargLen {
                    element: Box::new(self.resolve_static_core_type_params(
                        *element,
                        bindings,
                        type_params,
                        use_upper_fallback,
                        depth + 1,
                    )),
                    len: Box::new(self.resolve_static_core_type_params(
                        *len,
                        bindings,
                        type_params,
                        use_upper_fallback,
                        depth + 1,
                    )),
                }
            }
            crate::inference_core::CoreType::NamedTuple(fields) => {
                crate::inference_core::CoreType::NamedTuple(
                    fields
                        .into_iter()
                        .map(|(name, field_ty)| {
                            (
                                name,
                                self.resolve_static_core_type_params(
                                    field_ty,
                                    bindings,
                                    type_params,
                                    use_upper_fallback,
                                    depth + 1,
                                ),
                            )
                        })
                        .collect(),
                )
            }
            crate::inference_core::CoreType::Union(members) => {
                crate::inference_core::CoreType::Union(
                    members
                        .into_iter()
                        .map(|member| {
                            self.resolve_static_core_type_params(
                                member,
                                bindings,
                                type_params,
                                use_upper_fallback,
                                depth + 1,
                            )
                        })
                        .collect(),
                )
            }
            crate::inference_core::CoreType::TypeOf(inner) => {
                crate::inference_core::CoreType::TypeOf(Box::new(
                    self.resolve_static_core_type_params(
                        *inner,
                        bindings,
                        type_params,
                        use_upper_fallback,
                        depth + 1,
                    ),
                ))
            }
            crate::inference_core::CoreType::UnionAll { var, body } => {
                crate::inference_core::CoreType::UnionAll {
                    var,
                    body: Box::new(self.resolve_static_core_type_params(
                        *body,
                        bindings,
                        type_params,
                        use_upper_fallback,
                        depth + 1,
                    )),
                }
            }
            other => other,
        }
    }

    fn resolve_static_named_type_param_core(
        &self,
        name: &str,
        bindings: &HashMap<String, crate::types::JuliaType>,
        type_params: &[crate::types::TypeParam],
        use_upper_fallback: bool,
        depth: usize,
    ) -> Option<crate::inference_core::CoreType> {
        if let Some(binding) = bindings.get(name) {
            return Some(crate::inference_core::CoreType::from(binding));
        }

        let declared = type_params.iter().find(|tp| tp.name == name)?;
        let fallback = if use_upper_fallback {
            declared.get_upper_bound().map(String::as_str)
        } else {
            declared.lower_bound.as_deref()
        };
        match fallback {
            Some(bound) => Some(self.resolve_static_type_param_bound_core(
                bound,
                bindings,
                type_params,
                use_upper_fallback,
                depth + 1,
            )),
            None if use_upper_fallback => Some(crate::inference_core::CoreType::Any),
            None => Some(crate::inference_core::CoreType::Bottom),
        }
    }

    fn extract_type_bindings_for_selected_method(
        &self,
        actual: &crate::types::JuliaType,
        pattern: &crate::types::JuliaType,
        type_params: &[crate::types::TypeParam],
    ) -> Option<HashMap<String, crate::types::JuliaType>> {
        let (
            crate::types::JuliaType::Struct(actual_name),
            crate::types::JuliaType::Struct(pattern_name),
        ) = (actual, pattern)
        else {
            return None;
        };
        let (actual_base, actual_args) = split_runtime_parametric_name(actual_name);
        let (pattern_base, pattern_args) = split_runtime_parametric_name(pattern_name);
        if strip_runtime_module_prefix(actual_base) != strip_runtime_module_prefix(pattern_base)
            || actual_args.len() < pattern_args.len()
        {
            return None;
        }

        let mut bindings = HashMap::new();
        for (actual_arg, pattern_arg) in actual_args.iter().zip(pattern_args.iter()) {
            let Some(type_param) = type_params.iter().find(|tp| tp.name == *pattern_arg) else {
                if actual_arg.trim() != pattern_arg.trim() {
                    return None;
                }
                continue;
            };
            let bound_type = crate::types::JuliaType::from_name(actual_arg)
                .unwrap_or_else(|| crate::types::JuliaType::Struct(actual_arg.to_string()));
            if let Some(bound) = type_param.get_upper_bound() {
                let actual_core = crate::inference_core::CoreType::from(&bound_type);
                let bound_core = crate::inference_core::CoreType::from_julia_name(bound);
                if !crate::inference_core::CoreSubtypeEngine::with_hierarchy(&self.struct_hierarchy)
                    .is_subtype(&actual_core, &bound_core)
                {
                    return None;
                }
            }
            // Reject a second, conflicting binding for the same `where`
            // parameter instead of silently overwriting it (Issue #11231):
            // `Pair{T,T}` must not match `Pair{Int,String}`.
            match bindings.entry(type_param.name.clone()) {
                std::collections::hash_map::Entry::Occupied(existing) => {
                    if existing.get() != &bound_type {
                        return None;
                    }
                }
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(bound_type);
                }
            }
        }
        Some(bindings)
    }

    /// Recursively bind the where-clause type parameters of an `NTuple{LEN}` or
    /// `NTuple{LEN, ELEM}` pattern against a concrete tuple value.
    ///
    /// Handles nested patterns such as `NTuple{N, NTuple{M, T}}` where the inner
    /// length value parameter `M` and element type parameter `T` must be bound
    /// from the elements of each outer tuple (Issue #4842). `LEN` (when it is a
    /// where-clause parameter) is bound to the tuple length as an `i64` local;
    /// `ELEM` is either recursed into (nested `NTuple`) or bound as a type
    /// parameter to the common element type. Returns `true` when the pattern was
    /// an `NTuple` form that was consumed.
    pub(super) fn bind_ntuple_params(
        &self,
        param_type_name: &str,
        tuple: &TupleValue,
        func: &FunctionInfo,
        frame: &mut Frame,
    ) -> bool {
        if !(param_type_name.starts_with("NTuple{") && param_type_name.ends_with("}")) {
            return false;
        }
        let param_type_args = &param_type_name[7..param_type_name.len() - 1];
        // Split on the top-level comma so a nested `NTuple{M,T}` element stays intact.
        let (len_arg, elem_arg) = if let Some((len, elem)) = split_top_level_comma(param_type_args)
        {
            (len.trim(), Some(elem.trim()))
        } else {
            (param_type_args.trim(), None)
        };

        if func.type_params.iter().any(|tp| tp.name == len_arg) {
            let tuple_len = i64::try_from(tuple.elements.len()).unwrap_or(i64::MAX);
            frame
                .locals_any
                .insert(len_arg.to_string(), Value::I64(tuple_len));
            frame
                .var_types
                .insert(len_arg.to_string(), frame::VarTypeTag::I64);
        }

        let Some(elem_arg) = elem_arg else {
            return true;
        };

        // Nested NTuple element: recurse into each tuple element so the inner
        // length/element parameters are bound too. All elements share the same
        // pattern, so binding from the first element is sufficient.
        if elem_arg.starts_with("NTuple{") && elem_arg.ends_with("}") {
            if let Some(Value::Tuple(inner)) = tuple.elements.first() {
                self.bind_ntuple_params(elem_arg, inner, func, frame);
            }
            return true;
        }

        if func.type_params.iter().any(|tp| tp.name == elem_arg) {
            let mut element_types = tuple.elements.iter().map(|v| self.get_value_julia_type(v));
            if let Some(first_type) = element_types.next() {
                let bound_type = if element_types.all(|ty| ty == first_type) {
                    first_type
                } else {
                    crate::types::JuliaType::Any
                };
                insert_frame_type_binding(frame, elem_arg.to_string(), bound_type);
            }
        }

        true
    }
}
