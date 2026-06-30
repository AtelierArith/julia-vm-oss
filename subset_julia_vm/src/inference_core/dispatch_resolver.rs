//! Shared method-dispatch matching helpers.
//!
//! This module is the migration point for dispatch semantics that used to live
//! separately in compiler method tables and VM runtime call instructions.

pub mod core_match;

use std::collections::{HashMap, HashSet};

use crate::types::{JuliaType, StructHierarchy, TypeParam};

use super::{specificity, CoreSubtypeEngine, CoreType, CoreTypeVar};

/// Bonus for exact match between two concrete primitive-ish dispatch leaves.
///
/// This mirrors the compile-time MethodTable policy while giving the shared
/// resolver ownership of the scoring constants used during CoreType migration.
pub const EXACT_PRIMITIVE_MATCH_BONUS: i32 = 10;

/// Penalty when an argument is statically `Any` but the candidate parameter is
/// more specific. This keeps `f(::Any)` preferred for unknown values.
pub const ANY_ARG_SPECIFIC_PARAM_PENALTY: i32 = -EXACT_PRIMITIVE_MATCH_BONUS;

/// Penalty for `Type{Any}` matching a non-`Any` type object through sjulia's
/// transitional broad singleton rule.
///
/// Upstream Julia keeps `f(::Type{Any})` exact for `f(Any)`, but `f(::Type)`
/// wins for `f(Int64)`. Until the Type/UnionAll lattice is exact enough to
/// express that purely in matching, scoring demotes only the non-exact case.
pub const TYPE_ANY_NON_EXACT_SINGLETON_PENALTY: i32 = -100;

/// Bonus for a structured parametric pattern match, such as
/// `Matrix{<:Integer}` accepting `Matrix{Int64}`.
pub const PARAMETRIC_PATTERN_MATCH_BONUS: i32 = 3;

/// Bonus for typed varargs that bind a method `where` variable.
///
/// Without this, a keyword-forwarding fallback such as `f(xs...; kws...)` ties a
/// diagonal typed vararg like `f(xs::T...; kw=nothing, kws...) where T` and wins
/// by insertion order, which recursively forwards QuadGK `segbuf` calls
/// (Issue #8407).
const VARARG_TYPE_PARAM_BINDING_BONUS: u32 = 2;

/// Result of matching a Julia method signature projection against call-site
/// argument types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JuliaSignatureScore {
    pub binding_count: usize,
    pub fixed_param_count: usize,
    pub score: u32,
}

/// Runtime callable-value candidate metadata.
///
/// This mirrors the callable function-variable dispatch input while allowing
/// the score policy to live in the shared resolver during the #3910 migration.
#[derive(Debug, Clone, Copy)]
pub struct CallableValueCandidate<'a> {
    pub idx: usize,
    pub param_types: &'a [JuliaType],
    pub param_count: usize,
    pub vararg_param_index: Option<usize>,
    pub vararg_fixed_count: Option<usize>,
    /// `where` type parameters of the candidate method. Used to enforce the
    /// diagonal rule when a type variable appears in more than one covariant
    /// parameter position (Issue #5050).
    pub type_params: &'a [TypeParam],
}

/// Build the structured dispatch cache key for a call-site argument tuple.
pub fn core_tuple_signature_from_julia_types(arg_types: &[JuliaType]) -> CoreType {
    CoreType::Tuple(arg_types.iter().map(CoreType::from).collect())
}

/// Resolve callable-value candidates with the existing VM score policy.
///
/// The VM still owns runtime representation matching and exactness checks. The
/// shared resolver owns arity, fixed-prefix bonuses, exact-match bonuses, and
/// first-best tie behavior so callable-value dispatch no longer duplicates that
/// policy locally.
pub fn resolve_callable_value_candidates<'a, I, M, E>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_type_names: &[String],
    mut type_matches: M,
    mut exact_matches: E,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = CallableValueCandidate<'a>>,
    M: FnMut(&str, &JuliaType) -> bool,
    E: FnMut(&str, &JuliaType) -> bool,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32)> = None;
    for candidate in candidates {
        let required_arity = candidate
            .vararg_param_index
            .unwrap_or(candidate.param_count);
        let arity_match = if candidate.vararg_param_index.is_some() {
            if let Some(fixed_count) = candidate.vararg_fixed_count {
                actual_type_names.len() == required_arity + fixed_count
            } else {
                actual_type_names.len() >= required_arity
            }
        } else {
            actual_type_names.len() == required_arity
        };
        if !arity_match {
            continue;
        }

        let Some(specificity) = callable_value_candidate_score(
            &candidate,
            actual_type_names,
            &mut type_matches,
            &mut exact_matches,
        ) else {
            continue;
        };
        // Diagonal rule (Issue #5050): a `where` variable that appears in more
        // than one covariant parameter position must bind to a single concrete
        // type across the matched arguments. The per-argument `type_matches`
        // checks above are independent, so `f(x::T, y::T) where T` and the
        // nested `g(x::Vector{T}, y::T) where T` would otherwise accept
        // mismatched argument types.
        if !candidate.type_params.is_empty()
            && !callable_value_candidate_diagonal_ok(&candidate, actual_type_names)
        {
            continue;
        }
        // `where`-clause bound enforcement (Issue #6539): the per-argument
        // `type_matches` checks bind typevars loosely, so a bounded method
        // such as `abs(h::Holder{T}) where {T<:Real}` matched
        // `Holder{String}`. Mirror the CallDynamic* channels (Issue #6536 /
        // PR #6543): for candidates whose `where` parameters carry explicit
        // bounds, gate selection on `Tuple{actuals...} <: core_signature`
        // through the shared subtype engine. Unbounded `where T` candidates
        // skip the gate (the diagonal rule above already owns their
        // cross-slot consistency), keeping the legacy loose matching for the
        // common case.
        if callable_value_candidate_has_bounded_type_params(&candidate)
            && !callable_value_candidate_signature_ok(
                hierarchy,
                &candidate,
                actual_type_names,
                &mut actual_tuple,
            )
        {
            continue;
        }
        if best_match.is_none_or(|(_, best_score)| specificity > best_score) {
            best_match = Some((candidate.idx, specificity));
        }
    }
    best_match
}

/// Whether any of the candidate's `where` parameters carries an explicit
/// (non-`Any`) bound that loose per-argument matching cannot enforce.
fn callable_value_candidate_has_bounded_type_params(
    candidate: &CallableValueCandidate<'_>,
) -> bool {
    candidate.type_params.iter().any(|tp| {
        tp.upper_bound.as_deref().is_some_and(|b| b != "Any")
            || tp.lower_bound.as_deref().is_some_and(|b| b != "Union{}")
    })
}

/// Full `core_signature` subtype gate for a bounded `where`-parametric
/// callable-value candidate (Issue #6539, mirroring Issue #6536 for the
/// CallDynamic* channels).
///
/// Builds the runtime mirror of the method's `core_signature` from the
/// declared parameter types (with `where` bounds re-attached via
/// [`embed_type_param_bounds`]) and checks `Tuple{actuals...} <: signature`
/// through the shared [`CoreSubtypeEngine`]. The actual-side tuple is built
/// lazily once per dispatch and shared across candidates via `actual_tuple`.
fn callable_value_candidate_signature_ok(
    hierarchy: &StructHierarchy,
    candidate: &CallableValueCandidate<'_>,
    actual_type_names: &[String],
    actual_tuple: &mut Option<CoreType>,
) -> bool {
    let mut slot_cores: Vec<CoreType> = Vec::with_capacity(actual_type_names.len());
    for arg_idx in 0..actual_type_names.len() {
        let param_jt = if arg_idx < candidate.param_types.len() {
            Some(&candidate.param_types[arg_idx])
        } else if let Some(vararg_idx) = candidate.vararg_param_index {
            candidate.param_types.get(vararg_idx)
        } else {
            None
        };
        let Some(param_jt) = param_jt else {
            // Arity shapes the scorer accepted but the declared parameter
            // list cannot describe: leave dispatch unchanged.
            return true;
        };
        let rendered = param_jt.to_string();
        slot_cores.push(embed_type_param_bounds(
            runtime_candidate_core_type(param_jt, &rendered),
            candidate.type_params,
        ));
    }
    let signature = runtime_core_signature(&slot_cores, candidate.type_params);
    let tuple = actual_tuple.get_or_insert_with(|| {
        CoreType::Tuple(
            actual_type_names
                .iter()
                .map(|name| CoreType::from_julia_name(name))
                .collect(),
        )
    });
    CoreSubtypeEngine::with_hierarchy(hierarchy).is_subtype(tuple, &signature)
}

/// Resolve string-encoded runtime candidates against string-encoded actual
/// argument type names.
///
/// The input shape matches `Instr::CallTypedDispatch` candidates while the
/// matching logic is expressed through [`CoreType`] instead of local parsers in
/// the VM instruction handler.
#[cfg(test)]
pub fn resolve_type_name_candidates<'a, I>(
    candidates: I,
    actual_type_names: &[String],
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = (usize, &'a [String])>,
{
    let mut best_match: Option<(usize, u32, i32)> = None;
    for (idx, expected_types) in candidates {
        if type_name_pattern_matches(expected_types, actual_type_names) {
            let quality = type_name_pattern_match_quality(expected_types, actual_type_names);
            let specificity = type_name_pattern_specificity(expected_types);
            if best_match.is_none_or(|(_, best_quality, best_specificity)| {
                quality > best_quality
                    || (quality == best_quality && specificity > best_specificity)
            }) {
                best_match = Some((idx, quality, specificity));
            }
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

fn callable_value_candidate_score<M, E>(
    candidate: &CallableValueCandidate<'_>,
    actual_type_names: &[String],
    type_matches: &mut M,
    exact_matches: &mut E,
) -> Option<u32>
where
    M: FnMut(&str, &JuliaType) -> bool,
    E: FnMut(&str, &JuliaType) -> bool,
{
    let mut specificity = 0;
    for (arg_idx, arg_type_name) in actual_type_names.iter().enumerate() {
        let param_jt = if arg_idx < candidate.param_types.len() {
            Some(&candidate.param_types[arg_idx])
        } else if let Some(vararg_idx) = candidate.vararg_param_index {
            candidate.param_types.get(vararg_idx)
        } else {
            None
        };
        let Some(param_jt) = param_jt else {
            break;
        };

        if !type_matches(arg_type_name, param_jt) {
            return None;
        }

        let mut param_score = u32::from(param_jt.specificity());
        if param_score == 0 {
            param_score = 1;
        }
        if candidate
            .vararg_param_index
            .is_none_or(|vararg_idx| arg_idx < vararg_idx)
        {
            param_score += 5;
        } else if julia_type_mentions_type_params(param_jt, candidate.type_params) {
            param_score += VARARG_TYPE_PARAM_BINDING_BONUS;
        }
        specificity += param_score;
        if exact_matches(arg_type_name, param_jt) {
            specificity += 10;
        } else {
            if matches!(param_jt, JuliaType::TypeOf(inner) if matches!(inner.as_ref(), JuliaType::Any))
                && arg_type_name != "Type{Any}"
            {
                specificity = specificity.saturating_sub(
                    u32::try_from(-TYPE_ANY_NON_EXACT_SINGLETON_PENALTY).unwrap_or(0),
                );
            }
            let param_core = CoreType::from(param_jt);
            let arg_core = CoreType::from_julia_name(arg_type_name);
            if param_core.dispatch_pattern_score(&arg_core) == 3 {
                specificity += u32::try_from(PARAMETRIC_PATTERN_MATCH_BONUS).unwrap_or(0);
            }
        }
    }
    Some(specificity)
}

/// Enforce the diagonal rule for a callable-value candidate (Issue #5050).
///
/// `callable_value_candidate_score` checks each argument against its parameter
/// independently, so it cannot tell that `f(x::T, y::T) where T` requires both
/// arguments to share one concrete type, nor that `g(x::Vector{T}, y::T)`
/// requires `y` to match the element type of `x`. This rebuilds the shared
/// `where`-variable binding map from the matched arguments and rejects the
/// candidate when a variable binds inconsistently or when a diagonal variable
/// (appearing 2+ times covariantly) binds to a non-concrete type.
///
/// Returns `true` (accept) when the candidate has no relevant bindings, so
/// non-parametric and single-occurrence signatures keep their existing
/// behavior.
fn callable_value_candidate_diagonal_ok(
    candidate: &CallableValueCandidate<'_>,
    actual_type_names: &[String],
) -> bool {
    let mut bindings: HashMap<String, JuliaType> = HashMap::new();
    let mut param_pattern: Vec<JuliaType> = Vec::with_capacity(actual_type_names.len());

    for (arg_idx, arg_type_name) in actual_type_names.iter().enumerate() {
        let param_jt = if arg_idx < candidate.param_types.len() {
            Some(&candidate.param_types[arg_idx])
        } else if let Some(vararg_idx) = candidate.vararg_param_index {
            candidate.param_types.get(vararg_idx)
        } else {
            None
        };
        let Some(param_jt) = param_jt else {
            break;
        };
        param_pattern.push(param_jt.clone());

        // Only parameters that mention a `where` variable can contribute a
        // binding; the rest are concrete and irrelevant to the diagonal rule.
        if !julia_type_mentions_type_params(param_jt, candidate.type_params) {
            continue;
        }
        let Some(arg_jt) = JuliaType::from_name(arg_type_name) else {
            // Cannot reconstruct the argument type; leave dispatch unchanged.
            continue;
        };
        let Some(extracted) = arg_jt.extract_type_bindings(param_jt, candidate.type_params) else {
            continue;
        };
        for (name, bound_ty) in extracted {
            match bindings.entry(name) {
                std::collections::hash_map::Entry::Occupied(existing) => {
                    if existing.get() != &bound_ty {
                        return false;
                    }
                }
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(bound_ty);
                }
            }
        }
    }

    bindings.is_empty() || JuliaType::check_diagonal_rule_for_params(&param_pattern, &bindings)
}

/// Whether a declared parameter type mentions any of the method's `where`
/// type variables, recursing into parametric containers (Issue #5050).
fn julia_type_mentions_type_params(ty: &JuliaType, type_params: &[TypeParam]) -> bool {
    type_params
        .iter()
        .any(|tp| julia_type_mentions_type_param_name(ty, type_param_base_name(&tp.name)))
}

fn julia_type_mentions_type_param_name(ty: &JuliaType, name: &str) -> bool {
    match ty {
        JuliaType::TypeVar(type_name, _) => type_name == name,
        JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_mentions_type_param_name(inner, name)
        }
        JuliaType::TupleOf(types) | JuliaType::Union(types) => types
            .iter()
            .any(|t| julia_type_mentions_type_param_name(t, name)),
        JuliaType::UnionAll { body, .. } => julia_type_mentions_type_param_name(body, name),
        JuliaType::Struct(type_name) => {
            if type_name == name {
                return true;
            }
            match type_name.find('{') {
                Some(brace) if type_name.ends_with('}') => type_name
                    [brace + 1..type_name.len() - 1]
                    .split(',')
                    .any(|arg| arg.trim() == name),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Resolve string-encoded runtime candidates, allowing VM-owned subtype facts
/// to satisfy covariant bound patterns that CoreType cannot represent yet.
///
/// This keeps the typed-dispatch path on the existing i32 specificity policy
/// while moving the handler-local covariant fallback loops into the resolver.
#[cfg(test)]
pub fn resolve_type_name_candidates_with_subtype_fallback<'a, I, F>(
    candidates: I,
    actual_type_names: &[String],
    mut subtype_matches: F,
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = (usize, &'a [String])>,
    F: FnMut(&str, &str) -> bool,
{
    let candidates: Vec<(usize, &'a [String])> = candidates.into_iter().collect();
    if let Some(primary_match) = resolve_type_name_candidates(
        candidates
            .iter()
            .filter(|(_, sig)| {
                !sig.iter()
                    .any(|expected| expected.contains("_<:") || expected.contains("<:"))
            })
            .map(|(idx, sig)| (*idx, *sig)),
        actual_type_names,
    ) {
        return Some(primary_match);
    }

    let mut best_match: Option<(usize, u32, i32)> = None;
    for (idx, expected_types) in candidates {
        if type_name_pattern_matches_with_subtype_fallback(
            expected_types,
            actual_type_names,
            &mut subtype_matches,
        ) {
            let quality = type_name_pattern_match_quality(expected_types, actual_type_names);
            let specificity = type_name_pattern_specificity(expected_types);
            if best_match.is_none_or(|(_, best_quality, best_specificity)| {
                quality > best_quality
                    || (quality == best_quality && specificity > best_specificity)
            }) {
                best_match = Some((idx, quality, specificity));
            }
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

/// Score one runtime method-signature pattern against runtime type names.
///
/// Structural scoring is owned by [`CoreType::dispatch_pattern_score`].  VM
/// callers can inject a subtype fallback for user-defined ancestry that is not
/// fully represented in `CoreType` yet.  Higher scores are more specific.
#[cfg(test)]
pub fn runtime_type_pattern_score<F>(
    expected_types: &[&str],
    actual_type_names: &[&str],
    subtype_matches: &mut F,
) -> Option<u32>
where
    F: FnMut(&str, &str) -> bool,
{
    if expected_types.len() != actual_type_names.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_type_names.iter()) {
        let mut score = CoreType::from_julia_name(expected)
            .dispatch_pattern_score(&CoreType::from_julia_name(actual));
        if score == 0 && subtype_matches(actual, expected) {
            score = 1;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

/// Resolve runtime type-pattern candidates with shared score ordering.
///
/// Ties keep the first candidate, matching the existing VM no-fallback binary
/// dispatch behavior while moving the scoring policy into the shared resolver.
#[cfg(test)]
pub fn resolve_runtime_type_pattern_candidates<'a, I, F>(
    candidates: I,
    actual_type_names: &[&str],
    mut subtype_matches: F,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = (usize, Vec<&'a str>)>,
    F: FnMut(&str, &str) -> bool,
{
    let mut best_match: Option<(usize, u32)> = None;
    for (idx, expected_types) in candidates {
        let Some(score) =
            runtime_type_pattern_score(&expected_types, actual_type_names, &mut subtype_matches)
        else {
            continue;
        };
        if best_match.is_none_or(|(_, best_score)| score > best_score) {
            best_match = Some((idx, score));
        }
    }
    best_match
}

/// Resolve runtime candidates with an extra same-family fallback.
///
/// Some VM paths still carry string-encoded wrapper families that CoreType does
/// not fully know. The fallback keeps the same score tier as a bare-family
/// match (`2`) while centralizing the ordering in this module.
#[cfg(test)]
pub fn resolve_runtime_type_pattern_candidates_with_family_fallback<'a, I, M, F>(
    candidates: I,
    actual_type_names: &[&str],
    mut family_matches: M,
    mut subtype_matches: F,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = (usize, Vec<&'a str>)>,
    M: FnMut(&str, &str) -> bool,
    F: FnMut(&str, &str) -> bool,
{
    let mut best_match: Option<(usize, u32)> = None;
    for (idx, expected_types) in candidates {
        let Some(score) = runtime_type_pattern_score_with_family_fallback(
            &expected_types,
            actual_type_names,
            &mut family_matches,
            &mut subtype_matches,
        ) else {
            continue;
        };
        if best_match.is_none_or(|(_, best_score)| score > best_score) {
            best_match = Some((idx, score));
        }
    }
    best_match
}

#[cfg(test)]
fn runtime_type_pattern_score_with_family_fallback<M, F>(
    expected_types: &[&str],
    actual_type_names: &[&str],
    family_matches: &mut M,
    subtype_matches: &mut F,
) -> Option<u32>
where
    M: FnMut(&str, &str) -> bool,
    F: FnMut(&str, &str) -> bool,
{
    if expected_types.len() != actual_type_names.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_type_names.iter()) {
        let expected_core = CoreType::from_julia_name(expected);
        let actual_core = CoreType::from_julia_name(actual);
        let mut score = expected_core.dispatch_pattern_score(&actual_core);
        if score == 0
            && core_type_allows_family_fallback(&expected_core)
            && family_matches(actual, expected)
        {
            score = 2;
        }
        if score == 0 && subtype_matches(actual, expected) {
            score = 1;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

fn core_type_allows_family_fallback(expected: &CoreType) -> bool {
    match expected {
        CoreType::Struct { params, .. } => params.is_empty(),
        CoreType::Named(_) => true,
        _ => false,
    }
}

/// A structured runtime dispatch candidate projected from the method's
/// canonical `core_signature` (Issue #6502 slice 2).
///
/// `slots` carries one expected [`CoreType`] per call argument position with
/// `where`-clause bounds embedded into the typevars (see
/// [`embed_type_param_bounds`]); `signature` carries the full
/// `core_signature`-shaped form (`Tuple{slots...}` wrapped by one `UnionAll`
/// per `where` parameter) when the method has `where` parameters, so the
/// resolver can enforce bounds AND cross-slot typevar binding consistency
/// through the shared subtype engine (Issue #6536). Typevar-free methods set
/// `signature: None` and skip the gate entirely.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeCoreCandidate<'a, const N: usize> {
    pub idx: usize,
    pub slots: [&'a CoreType; N],
    pub signature: Option<&'a CoreType>,
}

/// Runtime dispatch candidate for dynamically sized call-site arity.
///
/// This is the slice-backed counterpart of [`RuntimeCoreCandidate`], used by
/// fallback paths such as `IterateDynamic` where arity is known only from the
/// instruction operand. It keeps the same `core_signature` gate semantics.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeCoreSliceCandidate<'a> {
    pub idx: usize,
    pub slots: &'a [CoreType],
    pub signature: Option<&'a CoreType>,
}

/// Runtime typed-dispatch candidate with both the structured signature and the
/// rendered names used by the legacy typed-dispatch ordering policy.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeTypedCoreCandidate<'a> {
    pub idx: usize,
    pub rendered: &'a [String],
    pub slots: &'a [CoreType],
    pub signature: Option<&'a CoreType>,
}

/// Score one structured runtime signature against structured actual argument
/// types — the `core_signature`-based replacement for the string-encoded
/// [`runtime_type_pattern_score`] (Issue #6502 slice 2).
///
/// Per-slot structural scoring is owned by
/// [`CoreType::dispatch_pattern_score_in`] (hierarchy-aware, so user-declared
/// ancestry inside typevar bounds keeps its structural tier); the injected
/// `subtype_matches` fallback admits user-hierarchy matches the structural
/// tiers do not cover, at the same score (`1`) as the string path.
pub fn runtime_core_pattern_score<F>(
    hierarchy: &StructHierarchy,
    expected_types: &[&CoreType],
    actual_types: &[CoreType],
    subtype_matches: &mut F,
) -> Option<u32>
where
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_types.iter()) {
        let mut score = expected.dispatch_pattern_score_in(hierarchy, actual);
        if score == 0 && subtype_matches(actual, expected) {
            score = 1;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

/// Structured scorer with an explicit same-family fallback tier.
///
/// This is the `core_signature` replacement for
/// [`runtime_type_pattern_score_with_family_fallback`]: structural scoring owns
/// exact/parametric/container tiers, `family_matches` admits legacy wrapper
/// families at tier 2 only for bare `Struct`/`Named` candidates, and
/// `subtype_matches` remains the final tier-1 fallback.
pub fn runtime_core_pattern_score_with_family_fallback<M, F>(
    hierarchy: &StructHierarchy,
    expected_types: &[&CoreType],
    actual_types: &[CoreType],
    family_matches: &mut M,
    subtype_matches: &mut F,
) -> Option<u32>
where
    M: FnMut(&CoreType, &CoreType) -> bool,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_types.iter()) {
        let mut score = expected.dispatch_pattern_score_in(hierarchy, actual);
        if score == 0
            && core_type_allows_family_fallback(expected)
            && family_matches(actual, expected)
        {
            score = 2;
        }
        if score == 0 && subtype_matches(actual, expected) {
            score = 1;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

fn runtime_core_pattern_score_slice_with_family_fallback<M, F>(
    hierarchy: &StructHierarchy,
    expected_types: &[CoreType],
    actual_types: &[CoreType],
    family_matches: &mut M,
    subtype_matches: &mut F,
) -> Option<u32>
where
    M: FnMut(&CoreType, &CoreType) -> bool,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return None;
    }

    let mut total_score = 0;
    for (expected, actual) in expected_types.iter().zip(actual_types.iter()) {
        let mut score = expected.dispatch_pattern_score_in(hierarchy, actual);
        if score == 0
            && core_type_allows_family_fallback(expected)
            && family_matches(actual, expected)
        {
            score = 2;
        }
        if score == 0 && subtype_matches(actual, expected) {
            score = 1;
        }
        if score == 0 {
            return None;
        }
        total_score += score;
    }
    Some(total_score)
}

/// Resolve structured runtime candidates with the shared score ordering —
/// the `core_signature`-based primary path replacing the string-encoded
/// [`resolve_runtime_type_pattern_candidates`] at the VM's dynamic dispatch
/// call sites (Issue #6502 slice 2).
///
/// Ties keep the first candidate, matching the string path. Candidates whose
/// method has `where` parameters additionally pass through the
/// `core_signature` subtype gate (`Tuple{actuals...} <: signature` via the
/// shared engine), which enforces `where` bounds and cross-slot typevar
/// binding consistency that the per-slot string scoring missed (Issue #6536).
pub fn resolve_runtime_core_signature_candidates<'a, const N: usize, I, F>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType; N],
    mut subtype_matches: F,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = RuntimeCoreCandidate<'a, N>>,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32)> = None;
    for candidate in candidates {
        let Some(score) = runtime_core_pattern_score(
            hierarchy,
            &candidate.slots,
            actual_types,
            &mut subtype_matches,
        ) else {
            continue;
        };
        if let Some(signature) = candidate.signature {
            let tuple = actual_tuple.get_or_insert_with(|| CoreType::Tuple(actual_types.to_vec()));
            if !CoreSubtypeEngine::with_hierarchy(hierarchy).is_subtype(tuple, signature) {
                continue;
            }
        }
        if best_match.is_none_or(|(_, best_score)| score > best_score) {
            best_match = Some((candidate.idx, score));
        }
    }
    best_match
}

/// Resolve slice-backed structured runtime candidates with an explicit
/// same-family fallback tier (Issue #6502 residual string fallback removal).
pub fn resolve_runtime_core_signature_slice_candidates_with_family_fallback<'a, I, M, F>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType],
    mut family_matches: M,
    mut subtype_matches: F,
) -> Option<(usize, u32)>
where
    I: IntoIterator<Item = RuntimeCoreSliceCandidate<'a>>,
    M: FnMut(&CoreType, &CoreType) -> bool,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32)> = None;
    for candidate in candidates {
        let Some(score) = runtime_core_pattern_score_slice_with_family_fallback(
            hierarchy,
            candidate.slots,
            actual_types,
            &mut family_matches,
            &mut subtype_matches,
        ) else {
            continue;
        };
        if let Some(signature) = candidate.signature {
            let tuple = actual_tuple.get_or_insert_with(|| CoreType::Tuple(actual_types.to_vec()));
            if !CoreSubtypeEngine::with_hierarchy(hierarchy).is_subtype(tuple, signature) {
                continue;
            }
        }
        if best_match.is_none_or(|(_, best_score)| score > best_score) {
            best_match = Some((candidate.idx, score));
        }
    }
    best_match
}

/// Resolve typed-dispatch candidates from structured per-slot [`CoreType`]s.
///
/// This is the `core_signature`-backed counterpart of
/// [`resolve_type_name_candidates_with_subtype_fallback`] for
/// `CallTypedDispatch[OrBuiltin*]`. Candidate matching uses structured slots
/// and the optional full-signature gate, while the final tie-break uses the
/// same typed-dispatch quality/specificity policy over the structured slots so
/// this slice can replace the VM's production string resolver without changing
/// method ordering.
pub fn resolve_typed_runtime_core_candidates_with_subtype_fallback<'a, I, F>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType],
    mut subtype_matches: F,
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = RuntimeTypedCoreCandidate<'a>>,
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    let candidates: Vec<_> = candidates.into_iter().collect();
    if let Some(primary_match) = resolve_typed_runtime_core_candidates(
        hierarchy,
        candidates.iter().copied().filter(|candidate| {
            !candidate
                .slots
                .iter()
                .any(core_type_pattern_has_explicit_bound)
        }),
        actual_types,
    ) {
        return Some(primary_match);
    }

    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32, i32)> = None;
    for candidate in candidates {
        if !typed_core_candidate_matches_with_subtype_fallback(
            hierarchy,
            &candidate,
            actual_types,
            &mut subtype_matches,
            &mut actual_tuple,
        ) {
            continue;
        }
        let quality = typed_core_pattern_match_quality(candidate.slots, actual_types);
        let specificity = core_type_pattern_specificity(candidate.slots);
        if best_match.is_none_or(|(_, best_quality, best_specificity)| {
            quality > best_quality || (quality == best_quality && specificity > best_specificity)
        }) {
            best_match = Some((candidate.idx, quality, specificity));
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

fn resolve_typed_runtime_core_candidates<'a, I>(
    hierarchy: &StructHierarchy,
    candidates: I,
    actual_types: &[CoreType],
) -> Option<(usize, i32)>
where
    I: IntoIterator<Item = RuntimeTypedCoreCandidate<'a>>,
{
    let mut actual_tuple: Option<CoreType> = None;
    let mut best_match: Option<(usize, u32, i32)> = None;
    for candidate in candidates {
        if !typed_core_candidate_matches(hierarchy, &candidate, actual_types, &mut actual_tuple) {
            continue;
        }
        let quality = typed_core_pattern_match_quality(candidate.slots, actual_types);
        let specificity = core_type_pattern_specificity(candidate.slots);
        if best_match.is_none_or(|(_, best_quality, best_specificity)| {
            quality > best_quality || (quality == best_quality && specificity > best_specificity)
        }) {
            best_match = Some((candidate.idx, quality, specificity));
        }
    }
    best_match.map(|(idx, _, specificity)| (idx, specificity))
}

fn typed_core_candidate_matches(
    hierarchy: &StructHierarchy,
    candidate: &RuntimeTypedCoreCandidate<'_>,
    actual_types: &[CoreType],
    actual_tuple: &mut Option<CoreType>,
) -> bool {
    if candidate.slots.len() != actual_types.len() {
        return false;
    }

    let mut bindings = HashMap::new();
    if !candidate
        .slots
        .iter()
        .zip(actual_types.iter())
        .all(|(expected, actual)| core_pattern_matches(expected, actual, &mut bindings))
    {
        return false;
    }
    typed_core_signature_gate_passes(hierarchy, candidate, actual_types, actual_tuple)
}

fn typed_core_candidate_matches_with_subtype_fallback<F>(
    hierarchy: &StructHierarchy,
    candidate: &RuntimeTypedCoreCandidate<'_>,
    actual_types: &[CoreType],
    subtype_matches: &mut F,
    actual_tuple: &mut Option<CoreType>,
) -> bool
where
    F: FnMut(&CoreType, &CoreType) -> bool,
{
    if candidate.slots.len() != actual_types.len() {
        return false;
    }

    let mut bindings = HashMap::new();
    for (expected, actual) in candidate.slots.iter().zip(actual_types.iter()) {
        if same_invariant_container_family_concrete_miss_core(expected, actual) {
            return false;
        }
        if core_pattern_matches(expected, actual, &mut bindings) {
            continue;
        }
        if core_type_has_previously_bound_typevars(expected, &bindings) {
            return false;
        }
        if !subtype_matches(actual, expected) {
            return false;
        }
    }
    typed_core_signature_gate_passes(hierarchy, candidate, actual_types, actual_tuple)
}

fn typed_core_signature_gate_passes(
    hierarchy: &StructHierarchy,
    candidate: &RuntimeTypedCoreCandidate<'_>,
    actual_types: &[CoreType],
    actual_tuple: &mut Option<CoreType>,
) -> bool {
    let Some(signature) = candidate.signature else {
        return true;
    };
    let tuple = actual_tuple.get_or_insert_with(|| CoreType::Tuple(actual_types.to_vec()));
    CoreSubtypeEngine::with_hierarchy(hierarchy).is_subtype(tuple, signature)
}

fn typed_core_pattern_match_quality(expected_types: &[CoreType], actual_types: &[CoreType]) -> u32 {
    expected_types
        .iter()
        .zip(actual_types.iter())
        .map(|(expected, actual)| {
            if expected == actual {
                2
            } else if expected.dispatch_pattern_score(actual) == 3 {
                1
            } else {
                0
            }
        })
        .sum()
}

fn core_type_pattern_specificity(expected_types: &[CoreType]) -> i32 {
    let mut specificity = 0;
    let mut type_var_count = 0;
    let mut same_type_var_bonus = 0;
    let mut seen_type_vars = HashSet::new();

    for expected in expected_types {
        let type_vars = core_typevar_names(expected);
        for name in type_vars {
            type_var_count += 1;
            if !seen_type_vars.insert(name) {
                same_type_var_bonus += 100;
            }
        }

        if !matches!(expected, CoreType::TypeVar(_)) {
            let param_bonus = i32::from(core_type_pattern_has_parametric_surface(expected));
            specificity += expected.specificity() as i32 + param_bonus;
        }
    }

    specificity - type_var_count + same_type_var_bonus
}

fn core_type_pattern_has_parametric_surface(core: &CoreType) -> bool {
    match core {
        CoreType::Bottom => true,
        CoreType::Struct { params, .. } => !params.is_empty(),
        CoreType::Tuple(_) | CoreType::Union(_) | CoreType::TypeOf(_) => true,
        CoreType::Vararg(_) | CoreType::VarargLen { .. } => true,
        CoreType::NamedTuple(_) => true,
        CoreType::UnionAll { body, .. } => core_type_pattern_has_parametric_surface(body),
        CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::TypeVar(_)
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => false,
    }
}

fn core_type_pattern_has_explicit_bound(core: &CoreType) -> bool {
    match core {
        CoreType::TypeVar(var) => var.upper_bound.is_some() || var.lower_bound.is_some(),
        CoreType::Named(name) => name.contains("_<:") || name.contains("<:"),
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            params.iter().any(core_type_pattern_has_explicit_bound)
        }
        CoreType::TypeOf(inner) | CoreType::Vararg(inner) => {
            core_type_pattern_has_explicit_bound(inner)
        }
        CoreType::VarargLen { element, len } => {
            core_type_pattern_has_explicit_bound(element)
                || core_type_pattern_has_explicit_bound(len)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, ty)| core_type_pattern_has_explicit_bound(ty)),
        CoreType::UnionAll { var, body } => {
            var.upper_bound.is_some()
                || var.lower_bound.is_some()
                || core_type_pattern_has_explicit_bound(body)
        }
        CoreType::Any
        | CoreType::Bottom
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_) => false,
    }
}

fn same_invariant_container_family_concrete_miss_core(
    expected: &CoreType,
    actual: &CoreType,
) -> bool {
    let (
        CoreType::Struct {
            name: expected_name,
            params: expected_params,
        },
        CoreType::Struct {
            name: actual_name,
            params: actual_params,
        },
    ) = (expected, actual)
    else {
        return false;
    };

    expected_name == actual_name
        && matches!(
            expected_name.as_str(),
            "Array" | "Vector" | "Matrix" | "Dict" | "Set"
        )
        && !expected_params.is_empty()
        && expected_params.len() == actual_params.len()
        && core_typevar_names(expected).is_empty()
}

/// Embed `where`-clause bounds into the typevars of a structurally converted
/// parameter core type.
///
/// Lowering keeps bounds for typevars inside parametric struct annotations
/// only on the method's `type_params` (`convert_type_with_type_vars` does not
/// descend into `JuliaType::Struct("Wrap{T}")`, so the rendered string carries
/// no bound — Issue #6536). The structured candidate path re-attaches them so
/// per-slot matching enforces the same bounds the compile-time matcher checks
/// via `core_signature`. `UnionAll` binders inside the type shadow outer
/// `where` parameters of the same name, mirroring scope rules.
pub fn embed_type_param_bounds(core: CoreType, type_params: &[TypeParam]) -> CoreType {
    if type_params.is_empty() {
        return core;
    }
    embed_type_param_bounds_scoped(core, type_params, &mut Vec::new())
}

fn embed_type_param_bounds_scoped(
    core: CoreType,
    type_params: &[TypeParam],
    shadowed: &mut Vec<String>,
) -> CoreType {
    match core {
        CoreType::TypeVar(var)
            if var.upper_bound.is_none()
                && var.lower_bound.is_none()
                && !shadowed.contains(&var.name) =>
        {
            match type_params.iter().find(|tp| tp.name == var.name) {
                Some(tp) => CoreType::TypeVar(CoreTypeVar::from(tp)),
                None => CoreType::TypeVar(var),
            }
        }
        CoreType::Named(name) if !shadowed.contains(&name) => {
            match type_params.iter().find(|tp| tp.name == name) {
                Some(tp) => CoreType::TypeVar(CoreTypeVar::from(tp)),
                None => CoreType::Named(name),
            }
        }
        CoreType::Struct { name, params } => CoreType::Struct {
            name,
            params: params
                .into_iter()
                .map(|p| embed_type_param_bounds_scoped(p, type_params, shadowed))
                .collect(),
        },
        CoreType::Tuple(elems) => CoreType::Tuple(
            elems
                .into_iter()
                .map(|e| embed_type_param_bounds_scoped(e, type_params, shadowed))
                .collect(),
        ),
        CoreType::Union(arms) => CoreType::Union(
            arms.into_iter()
                .map(|a| embed_type_param_bounds_scoped(a, type_params, shadowed))
                .collect(),
        ),
        CoreType::TypeOf(inner) => CoreType::TypeOf(Box::new(embed_type_param_bounds_scoped(
            *inner,
            type_params,
            shadowed,
        ))),
        CoreType::Vararg(inner) => CoreType::Vararg(Box::new(embed_type_param_bounds_scoped(
            *inner,
            type_params,
            shadowed,
        ))),
        CoreType::VarargLen { element, len } => CoreType::VarargLen {
            element: Box::new(embed_type_param_bounds_scoped(
                *element,
                type_params,
                shadowed,
            )),
            len: Box::new(embed_type_param_bounds_scoped(*len, type_params, shadowed)),
        },
        CoreType::UnionAll { var, body } => {
            shadowed.push(var.name.clone());
            let body = embed_type_param_bounds_scoped(*body, type_params, shadowed);
            shadowed.pop();
            CoreType::UnionAll {
                var,
                body: Box::new(body),
            }
        }
        other => other,
    }
}

/// Build the runtime mirror of `MethodSig::core_signature` from per-call slot
/// core types and the method's `where` parameters: `Tuple{slots...}` wrapped
/// by one `UnionAll` per `where` parameter (outermost wrapper = first
/// parameter, same construction as `MethodSig::compute_core_signature`).
pub fn runtime_core_signature(slot_cores: &[CoreType], type_params: &[TypeParam]) -> CoreType {
    let mut sig = CoreType::Tuple(slot_cores.to_vec());
    for type_param in type_params.iter().rev() {
        sig = CoreType::UnionAll {
            var: CoreTypeVar::from(type_param),
            body: Box::new(sig),
        };
    }
    sig
}

/// Project a declared parameter `JuliaType` onto the [`CoreType`] used for
/// structured runtime candidate matching (Issue #6502 slice 2).
///
/// The structural `CoreType::from(&JuliaType)` conversion is the default source
/// for runtime candidate slots (the same shape `MethodSig::core_signature`
/// serializes). Some method payloads still carry erased `JuliaType::Array`
/// declarations while their rendered signature preserves `Vector{T}` /
/// `Matrix{T}` / `Array{T}` parameters; keep that parametric array shape
/// structured here so runtime typed dispatch can enforce diagonal bindings
/// without returning to string matching at the call site. Divergent rendered
/// forms such as `AbstractUser` and `Module` keep their structured image and
/// rely on `CoreType`'s nominal bridge rules.
pub fn runtime_candidate_core_type(declared: &JuliaType, rendered: &str) -> CoreType {
    if matches!(declared, JuliaType::Array) && rendered_parametric_array_core(rendered).is_some() {
        return CoreType::from_julia_name(rendered);
    }
    CoreType::from(declared)
}

fn rendered_parametric_array_core(rendered: &str) -> Option<&str> {
    let base = rendered
        .split_once('{')?
        .0
        .rsplit('.')
        .next()
        .unwrap_or(rendered);
    matches!(
        base,
        "Array" | "Vector" | "Matrix" | "AbstractArray" | "AbstractVector" | "AbstractMatrix"
    )
    .then_some(base)
}

/// Check if a string-encoded method signature pattern matches actual runtime
/// type names.
#[cfg(test)]
pub fn type_name_pattern_matches(expected_types: &[String], actual_types: &[String]) -> bool {
    if expected_types.len() != actual_types.len() {
        return false;
    }

    let mut bindings = HashMap::new();
    expected_types
        .iter()
        .zip(actual_types.iter())
        .all(|(expected, actual)| {
            let expected_core = CoreType::from_julia_name(expected);
            let actual_core = CoreType::from_julia_name(actual);
            core_pattern_matches(&expected_core, &actual_core, &mut bindings)
        })
}

#[cfg(test)]
fn type_name_pattern_matches_with_subtype_fallback<F>(
    expected_types: &[String],
    actual_types: &[String],
    subtype_matches: &mut F,
) -> bool
where
    F: FnMut(&str, &str) -> bool,
{
    if expected_types.len() != actual_types.len() {
        return false;
    }

    let type_params = inferred_type_params_from_expected_names(expected_types);
    let mut bindings = HashMap::new();
    expected_types
        .iter()
        .zip(actual_types.iter())
        .all(|(expected, actual)| {
            if same_invariant_container_family_concrete_miss(expected, actual) {
                return false;
            }
            let param_ty = JuliaType::from_name_or_struct(expected);
            let arg_ty = JuliaType::from_name_or_struct(actual);
            if julia_type_pattern_matches(&param_ty, &arg_ty, &type_params, &mut bindings) {
                return true;
            }
            if julia_type_mentions_type_params(&param_ty, &type_params) {
                return false;
            }
            if expected.contains("_<:") || expected.contains("<:") {
                covariant_bound_matches(expected, actual, subtype_matches)
            } else if same_invariant_container_family_concrete_miss(expected, actual) {
                false
            } else {
                type_name_pattern_matches(
                    std::slice::from_ref(expected),
                    std::slice::from_ref(actual),
                ) || subtype_matches(actual, expected)
            }
        })
}

#[cfg(test)]
fn same_invariant_container_family_concrete_miss(expected: &str, actual: &str) -> bool {
    let expected_core = CoreType::from_julia_name(expected);
    let actual_core = CoreType::from_julia_name(actual);
    let (
        CoreType::Struct {
            name: expected_name,
            params: expected_params,
        },
        CoreType::Struct {
            name: actual_name,
            params: actual_params,
        },
    ) = (&expected_core, &actual_core)
    else {
        return false;
    };

    expected_name == actual_name
        && matches!(
            expected_name.as_str(),
            "Array" | "Vector" | "Matrix" | "Dict" | "Set"
        )
        && !expected_params.is_empty()
        && expected_params.len() == actual_params.len()
        && core_typevar_names(&expected_core).is_empty()
}

#[cfg(test)]
fn inferred_type_params_from_expected_names(expected_types: &[String]) -> Vec<TypeParam> {
    let mut seen = HashSet::new();
    let mut params = Vec::new();
    for expected in expected_types {
        for name in core_typevar_names(&CoreType::from_julia_name(expected)) {
            if name == "_" || !seen.insert(name.clone()) {
                continue;
            }
            params.push(TypeParam::new(name));
        }
    }
    params
}

#[cfg(test)]
fn covariant_bound_matches<F>(expected: &str, actual: &str, subtype_matches: &mut F) -> bool
where
    F: FnMut(&str, &str) -> bool,
{
    if subtype_matches(actual, expected) {
        return true;
    }

    if let Some(expected_inner) = type_singleton_inner(expected) {
        let Some(bound) = strip_covariant_bound(expected_inner) else {
            return false;
        };
        let Some(actual_inner) = type_singleton_inner(actual) else {
            return false;
        };
        return subtype_matches(actual_inner, bound);
    }

    if expected.contains('{') {
        return false;
    }

    strip_covariant_bound(expected).is_some_and(|bound| subtype_matches(actual, bound))
}

#[cfg(test)]
fn strip_covariant_bound(type_name: &str) -> Option<&str> {
    type_name
        .strip_prefix("_<:")
        .or_else(|| type_name.strip_prefix("<:"))
        .or_else(|| type_name.split_once("<:").map(|(_, bound)| bound.trim()))
}

#[cfg(test)]
fn type_singleton_inner(type_name: &str) -> Option<&str> {
    type_name
        .strip_prefix("Type{")
        .and_then(|inner| inner.strip_suffix('}'))
}

/// Calculate relative pattern specificity for string-encoded dispatch
/// candidates. Higher is more specific.
pub fn type_name_pattern_specificity(expected_types: &[String]) -> i32 {
    let mut specificity = 0;
    let mut type_var_count = 0;
    let mut same_type_var_bonus = 0;
    let mut seen_type_vars = HashSet::new();

    for expected in expected_types {
        let core = CoreType::from_julia_name(expected);
        let type_vars = core_typevar_names(&core);
        for name in type_vars {
            type_var_count += 1;
            if !seen_type_vars.insert(name) {
                same_type_var_bonus += 100;
            }
        }

        if !matches!(core, CoreType::TypeVar(_)) {
            let param_bonus = i32::from(expected.contains('{'));
            specificity += core.specificity() as i32 + param_bonus;
        }
    }

    specificity - type_var_count + same_type_var_bonus
}

#[cfg(test)]
fn type_name_pattern_match_quality(expected_types: &[String], actual_types: &[String]) -> u32 {
    expected_types
        .iter()
        .zip(actual_types.iter())
        .map(|(expected, actual)| {
            let expected_core = CoreType::from_julia_name(expected);
            let actual_core = CoreType::from_julia_name(actual);
            if expected_core == actual_core {
                2
            } else if expected_core.dispatch_pattern_score(&actual_core) == 3 {
                1
            } else {
                0
            }
        })
        .sum()
}

/// Check if JuliaType method parameters match argument types while tracking
/// `where` type-variable bindings.
pub fn julia_signature_match_with_bindings(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    type_params: &[TypeParam],
) -> Option<usize> {
    let mut bindings: HashMap<String, JuliaType> = HashMap::new();

    for (param_ty, arg_ty) in param_types.iter().zip(arg_types.iter()) {
        if !julia_type_pattern_matches(param_ty, arg_ty, type_params, &mut bindings) {
            return None;
        }
    }

    if !bindings.is_empty() && !JuliaType::check_diagonal_rule_for_params(param_types, &bindings) {
        return None;
    }

    Some(bindings.len())
}

/// Match and score a Julia method signature using the shared CoreType scoring
/// policy. `param_types` and `arg_types` must already be arity-normalized for
/// fixed/trailing varargs.
pub fn score_julia_signature(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    type_params: &[TypeParam],
    has_varargs: bool,
    fixed_varargs: bool,
) -> Option<JuliaSignatureScore> {
    let binding_count = julia_signature_match_with_bindings(param_types, arg_types, type_params)?;
    Some(score_julia_signature_with_binding_count(
        param_types,
        arg_types,
        binding_count,
        has_varargs,
        fixed_varargs,
    ))
}

/// Score a signature that was matched by a caller-owned fallback.
///
/// This is used by MethodTable's user-defined struct-parent fallback so the
/// fallback keeps its existing matching policy while sharing CoreType scoring.
/// Base specificity of a value-position parameter for method scoring.
///
/// A bounded type variable `x::T where {T<:B}` is as specific as a concrete `B`
/// parameter (in Julia `Tuple{T} where T<:B == Tuple{B}`), so it must outrank an
/// untyped `Any` parameter. `CoreType::specificity()` scores every type variable
/// as 0 (ignoring the bound), and the `type_reuse_bonus` below additionally
/// rewards a parameter that binds no type variable; together those made an
/// untyped fallback out-score a bounded type variable. The `+1` here compensates
/// the single-binding `type_reuse_bonus`, keeping `T<:B` tied with a concrete `B`
/// and strictly above `Any`. An unbounded `T` (≡ `Any`) stays at 0.
///
/// This is intentionally local to value-position scoring: it does not perturb
/// `CoreType::specificity()` itself, so type-position dispatch (`Type{<:B}`
/// patterns, e.g. `eltype(::Type{<:Pairs{K,V,I,A}})`) keeps its existing
/// ordering (Issue #5375).
///
/// The bound's specificity is read from `CoreType::from(ty)`, which derives it
/// from the bound's type name; it is exact for built-in abstract bounds
/// (`Number`, `Real`, `Integer`, ...) used by the reported cases and remains a
/// heuristic for more exotic bounds.
fn value_param_base_specificity(ty: &JuliaType) -> u32 {
    let core = CoreType::from(ty);

    if let CoreType::AbstractUser {
        parent: Some(parent),
        ..
    } = &core
    {
        // Bug #5582 / parent #5072: a user abstract that sits below a built-in
        // abstract, e.g. `AbstractIrrational <: Real`, must outrank that parent.
        // The declared parent is carried structurally on `CoreType::AbstractUser`
        // (Issue #6594: replaces the legacy `JuliaType::from_name(parent)` string
        // re-parse), so the parent boost reads the structured `CoreType` directly
        // and only fires for a parent that resolves to a recognized built-in
        // abstract/concrete type. An `Any` parent, or a parent that names another
        // (unresolved) user abstract — which the legacy `from_name` parse rejected
        // — keeps the flat `AbstractUser` floor.
        if user_abstract_parent_is_boostable(parent) {
            return u32::from(parent.specificity()).saturating_add(1);
        }
        return u32::from(core.specificity());
    }

    if let CoreType::TypeVar(var) = &core {
        if let Some(bound) = &var.upper_bound {
            // `T<:Any` is equivalent to an unbounded `T` (≡ `Any`); it must not
            // outrank an untyped parameter, so keep it at 0.
            if matches!(bound.as_ref(), CoreType::Any) {
                return 0;
            }
            // Floor the bound at 1 so a structurally narrow bound whose
            // `specificity()` collapses to 0 (e.g. `Vector{S}` with a
            // type-variable element) still ranks strictly above an untyped `Any`
            // parameter, then add 1 to compensate the single-binding
            // `type_reuse_bonus`.
            return u32::from(bound.specificity().max(1)).saturating_add(1);
        }
    }
    u32::from(core.specificity())
}

/// Whether a structured `CoreType::AbstractUser` parent resolves to a recognized
/// built-in type whose specificity should boost the user abstract above its
/// parent (Issue #6594). This mirrors the legacy `JuliaType::from_name(parent)`
/// gate structurally: that parse returned `None` for `Any`, for names that map to
/// another (unresolved) user abstract, and for bare type-variable spellings, all
/// of which kept the flat `AbstractUser` floor. Built-in abstracts/concretes
/// (`Number`, `Real`, `Integer`, `AbstractVector`, ...) resolve and contribute
/// their specificity.
fn user_abstract_parent_is_boostable(parent: &CoreType) -> bool {
    !matches!(
        parent,
        CoreType::Any
            | CoreType::Bottom
            | CoreType::Named(_)
            | CoreType::AbstractUser { .. }
            | CoreType::TypeVar(_)
    )
}

pub fn score_julia_signature_with_binding_count(
    param_types: &[JuliaType],
    arg_types: &[JuliaType],
    binding_count: usize,
    has_varargs: bool,
    fixed_varargs: bool,
) -> JuliaSignatureScore {
    let fixed_param_count = param_types.len().min(arg_types.len());
    let base_score: u32 = param_types
        .iter()
        .take(fixed_param_count)
        .map(value_param_base_specificity)
        .sum();

    let match_quality_bonus: i32 = param_types
        .iter()
        .take(fixed_param_count)
        .zip(arg_types.iter().take(fixed_param_count))
        .map(|(param_ty, arg_ty)| {
            let exact_struct_match = matches!(
                (param_ty, arg_ty),
                (JuliaType::Struct(param_name), JuliaType::Struct(arg_name))
                    if param_name == arg_name
            );
            let param_core = CoreType::from(param_ty);
            let arg_core = CoreType::from(arg_ty);
            let pattern_score = param_core.dispatch_pattern_score(&arg_core);
            let exact_bonus_eligible = (param_core.is_builtin_dispatch_primitive()
                && arg_core.is_builtin_dispatch_primitive())
                || (matches!(param_core, CoreType::TypeOf(_))
                    && matches!(arg_core, CoreType::TypeOf(_)))
                || (matches!(param_core, CoreType::Struct { .. })
                    && matches!(arg_core, CoreType::Struct { .. }));

            if is_type_any_non_exact_singleton_match(param_ty, arg_ty) {
                TYPE_ANY_NON_EXACT_SINGLETON_PENALTY
            } else if exact_struct_match {
                EXACT_PRIMITIVE_MATCH_BONUS
            } else if exact_bonus_eligible {
                if param_core == arg_core {
                    EXACT_PRIMITIVE_MATCH_BONUS
                } else if is_typevar_singleton_match(param_ty, arg_ty) {
                    PARAMETRIC_PATTERN_MATCH_BONUS
                } else if pattern_score == 3 {
                    PARAMETRIC_PATTERN_MATCH_BONUS
                        + i32::from(matches!(param_core, CoreType::TypeOf(_)))
                } else {
                    0
                }
            } else if matches!(arg_core, CoreType::Any) && !matches!(param_core, CoreType::Any) {
                ANY_ARG_SPECIFIC_PARAM_PENALTY
            } else if is_typevar_singleton_match(param_ty, arg_ty) {
                PARAMETRIC_PATTERN_MATCH_BONUS
            } else {
                0
            }
        })
        .sum();

    let score_i32 = (base_score as i32 + match_quality_bonus).max(0);
    let score = u32::try_from(score_i32).unwrap_or(0);
    let type_reuse_bonus = if binding_count < fixed_param_count {
        (fixed_param_count - binding_count) as u32
    } else {
        0
    };
    let score = if has_varargs {
        if fixed_varargs || base_score > 0 {
            score + type_reuse_bonus
        } else {
            score.saturating_sub(1) + type_reuse_bonus
        }
    } else {
        score + type_reuse_bonus
    };

    JuliaSignatureScore {
        binding_count,
        fixed_param_count,
        score,
    }
}

fn is_type_any_non_exact_singleton_match(param_ty: &JuliaType, arg_ty: &JuliaType) -> bool {
    matches!(
        (param_ty, arg_ty),
        (
            JuliaType::TypeOf(param_inner),
            JuliaType::TypeOf(arg_inner),
        ) if matches!(param_inner.as_ref(), JuliaType::Any)
            && !matches!(arg_inner.as_ref(), JuliaType::Any)
    )
}

fn is_typevar_singleton_match(param_ty: &JuliaType, arg_ty: &JuliaType) -> bool {
    matches!(
        (param_ty, arg_ty),
        (
            JuliaType::TypeOf(param_inner),
            JuliaType::TypeOf(_),
        ) if matches!(param_inner.as_ref(), JuliaType::TypeVar(_, _))
    )
}

fn type_object_inner_nominal_family_mismatch(param_ty: &JuliaType, arg_ty: &JuliaType) -> bool {
    let Some(param_family) = type_object_inner_nominal_family(param_ty) else {
        return false;
    };
    let Some(arg_family) = type_object_inner_nominal_family(arg_ty) else {
        return false;
    };

    match (&param_family, &arg_family) {
        (
            TypeObjectInnerFamily::Array { rank: param_rank },
            TypeObjectInnerFamily::Array { rank: arg_rank },
        ) => param_rank != arg_rank,
        _ => param_family != arg_family,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeObjectInnerFamily {
    Array { rank: Option<usize> },
    Nominal(String),
}

fn type_object_inner_nominal_family(ty: &JuliaType) -> Option<TypeObjectInnerFamily> {
    match ty {
        JuliaType::VectorOf(_) => Some(TypeObjectInnerFamily::Array { rank: Some(1) }),
        JuliaType::MatrixOf(_) => Some(TypeObjectInnerFamily::Array { rank: Some(2) }),
        JuliaType::Array => Some(TypeObjectInnerFamily::Array { rank: None }),
        JuliaType::Struct(name) => {
            let (base, params) = split_nominal_type_name(name);
            let base = base.rsplit('.').next().unwrap_or(base);
            if base == "Array" {
                let rank = params
                    .get(1)
                    .and_then(|rank| rank.trim().parse::<usize>().ok());
                return Some(TypeObjectInnerFamily::Array { rank });
            }
            Some(TypeObjectInnerFamily::Nominal(base.to_string()))
        }
        JuliaType::TypeVar(_, _) | JuliaType::Any => None,
        _ => Some(TypeObjectInnerFamily::Nominal(ty.name().to_string())),
    }
}

fn split_nominal_type_name(name: &str) -> (&str, Vec<&str>) {
    let Some(brace_idx) = name.find('{') else {
        return (name, Vec::new());
    };
    if !name.ends_with('}') {
        return (name, Vec::new());
    }
    let inner = &name[brace_idx + 1..name.len() - 1];
    (&name[..brace_idx], split_top_level_type_args(inner))
}

fn split_top_level_type_args(s: &str) -> Vec<&str> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut start = 0;
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                result.push(s[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    result.push(s[start..].trim());
    result
}

fn julia_type_pattern_matches(
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    if let (JuliaType::TupleOf(param_elems), JuliaType::TupleOf(arg_elems)) = (param_ty, arg_ty) {
        // Trailing unbounded `Vararg{T}` pattern: match leading slots
        // positionally, then match every remaining argument element against the
        // vararg element type (Issue #4857).
        if let Some(last) = param_elems.last() {
            if let Some(vararg_elem) = crate::types::unbounded_vararg_element(last) {
                let lead_count = param_elems.len() - 1;
                if arg_elems.len() < lead_count {
                    return false;
                }
                let leads_ok =
                    param_elems[..lead_count]
                        .iter()
                        .zip(arg_elems.iter())
                        .all(|(param, arg)| {
                            julia_type_pattern_matches(param, arg, type_params, bindings)
                        });
                if !leads_ok {
                    return false;
                }
                return arg_elems[lead_count..].iter().all(|arg| {
                    julia_type_pattern_matches(&vararg_elem, arg, type_params, bindings)
                });
            }
        }
        return param_elems.len() == arg_elems.len()
            && param_elems
                .iter()
                .zip(arg_elems.iter())
                .all(|(param, arg)| julia_type_pattern_matches(param, arg, type_params, bindings));
    }

    if let JuliaType::TypeVar(var_name, bound) = param_ty {
        let type_param = find_type_param(type_params, var_name);
        let upper = usable_upper_bound(bound.as_deref())
            .or_else(|| type_param.and_then(type_param_upper_bound));
        if var_name == "_" {
            return upper.is_none_or(|bound_name| {
                core_is_subtype(
                    &CoreType::from(arg_ty),
                    &CoreType::from_julia_name(bound_name),
                )
            });
        }
        if let Some(bound_pattern) = parametric_typevar_bound_pattern(upper, type_params) {
            return julia_type_pattern_matches(&bound_pattern, arg_ty, type_params, bindings)
                && bind_or_check_julia_type_var(var_name, None, arg_ty, bindings);
        }
        return bind_or_check_julia_type_var(var_name, upper, arg_ty, bindings);
    }
    if let JuliaType::Struct(var_name) = param_ty {
        if let Some(type_param) = find_type_param(type_params, var_name) {
            let upper = type_param_upper_bound(type_param);
            if let Some(bound_pattern) = parametric_typevar_bound_pattern(upper, type_params) {
                return julia_type_pattern_matches(&bound_pattern, arg_ty, type_params, bindings)
                    && bind_or_check_julia_type_var(var_name, None, arg_ty, bindings);
            }
            return bind_or_check_julia_type_var(var_name, upper, arg_ty, bindings);
        }
        // Issue #5314: `var_name` is NOT a method type parameter, so `::var_name`
        // names a concrete (possibly parametric) struct type — even when the name
        // is a single uppercase letter (e.g. `Q`) or an uppercase letter followed
        // by digits (`Q5314`) that the context-free CoreType layer otherwise
        // misclassifies as an unbounded type variable, making `is_subtype_of`
        // accept *any* argument. A struct is a final leaf type, so a primitive
        // argument (`Float64`, `Int64`, ...) can never be a subtype of it. Reject
        // it here instead of falling through to the misclassifying subtype check;
        // without this, adding a `min(::Q, ::Q)` method broke `min(1.0, 2.0)`
        // (AmbiguousMethod) and made `oneunit(3.0)` mis-dispatch a `Float64` into
        // the struct method. Non-primitive arguments (tuples, value-parameter
        // bindings, ...) keep their existing matching so parametric value
        // parameters are unaffected.
        if arg_ty.is_primitive() {
            return false;
        }
    }

    if let JuliaType::TypeOf(inner_param) = param_ty {
        if let JuliaType::TypeOf(inner_arg) = arg_ty {
            // `Type{T}` binds `T` invariantly to the argument, so both the
            // upper and lower bounds of `T` are enforced here (Issue #5051).
            if let JuliaType::TypeVar(var_name, bound) = inner_param.as_ref() {
                let type_param = find_type_param(type_params, var_name);
                return bind_or_check_julia_type_var_bounded(
                    var_name,
                    usable_upper_bound(bound.as_deref())
                        .or_else(|| type_param.and_then(type_param_upper_bound)),
                    type_param.and_then(type_param_lower_bound),
                    inner_arg.as_ref(),
                    bindings,
                );
            }
            if let JuliaType::Struct(var_name) = inner_param.as_ref() {
                if let Some(type_param) = find_type_param(type_params, var_name) {
                    return bind_or_check_julia_type_var_bounded(
                        var_name,
                        type_param_upper_bound(type_param),
                        type_param_lower_bound(type_param),
                        inner_arg.as_ref(),
                        bindings,
                    );
                }
            }
            if julia_type_mentions_type_params(inner_param, type_params) {
                if type_object_inner_nominal_family_mismatch(inner_param, inner_arg.as_ref()) {
                    return false;
                }
                if let Some(extracted) = inner_arg.extract_type_bindings(inner_param, type_params) {
                    return extracted.into_iter().all(|(name, bound_ty)| {
                        bind_or_check_julia_type_var(&name, None, &bound_ty, bindings)
                    });
                }
            }
            if matches!(inner_param.as_ref(), JuliaType::Any) {
                return true;
            }
            return inner_arg.as_ref() == inner_param.as_ref();
        }
    }

    if matches!(arg_ty, JuliaType::TypeOf(_))
        && !matches!(
            param_ty,
            JuliaType::Any | JuliaType::Type | JuliaType::DataType | JuliaType::TypeOf(_)
        )
    {
        return false;
    }

    // Nested diagonal binding (Issue #5050): a parametric parameter such as
    // `x::Vector{T}` mentions a `where` type variable below the top level. The
    // structural cases above only bind a variable that sits at the very top of
    // the parameter type, so without this branch the inner `T` from `Vector{T}`
    // is never recorded in the shared binding map and a later `y::T` could not
    // enforce the diagonal rule.
    //
    // The match decision still rests on the existing subtype check below — we
    // only *record* the inner binding(s) so that a repeated occurrence of the
    // same variable is later rejected by `bind_or_check_julia_type_var` (and so
    // the post-match `check_diagonal_rule_for_params` can see them). The binding
    // is only recorded when the argument actually subtypes the parameter, so we
    // never change which arguments a parameter accepts in isolation.
    if julia_type_mentions_type_params(param_ty, type_params)
        && arg_ty.is_subtype_of_parametric(param_ty, type_params)
    {
        if let Some(extracted) = arg_ty.extract_type_bindings(param_ty, type_params) {
            return extracted.into_iter().all(|(name, bound_ty)| {
                let upper = find_type_param(type_params, &name).and_then(type_param_upper_bound);
                bind_or_check_julia_type_var(&name, upper, &bound_ty, bindings)
            });
        }
        return true;
    }

    arg_ty.is_subtype_of_parametric(param_ty, type_params)
}

fn core_pattern_matches(
    expected: &CoreType,
    actual: &CoreType,
    bindings: &mut HashMap<String, CoreType>,
) -> bool {
    match expected {
        CoreType::TypeVar(var) => bind_or_check_core_type_var(var, actual, bindings),
        CoreType::Named(name) if name.starts_with("_<:") => {
            let bound = CoreType::from_julia_name(name.trim_start_matches("_<:"));
            core_is_subtype(actual, &bound)
        }
        CoreType::Struct { name, params } => {
            let CoreType::Struct {
                name: actual_name,
                params: actual_params,
            } = actual
            else {
                return core_is_subtype(actual, expected);
            };
            if !struct_family_matches(name, actual_name) {
                return false;
            }
            params.is_empty()
                || (params.len() == actual_params.len()
                    && params
                        .iter()
                        .zip(actual_params.iter())
                        .all(|(param, actual)| {
                            if core_typevar_names(param).is_empty() {
                                param == actual
                            } else {
                                core_pattern_matches(param, actual, bindings)
                            }
                        }))
        }
        CoreType::Tuple(expected_elements) => {
            let CoreType::Tuple(actual_elements) = actual else {
                return false;
            };
            expected_elements.len() == actual_elements.len()
                && expected_elements
                    .iter()
                    .zip(actual_elements.iter())
                    .all(|(expected, actual)| core_pattern_matches(expected, actual, bindings))
        }
        CoreType::TypeOf(expected_inner) => {
            let CoreType::TypeOf(actual_inner) = actual else {
                return false;
            };
            match expected_inner.as_ref() {
                CoreType::TypeVar(_) => {
                    core_pattern_matches(expected_inner, actual_inner, bindings)
                }
                CoreType::Named(name) if name.starts_with("_<:") => {
                    core_pattern_matches(expected_inner, actual_inner, bindings)
                }
                _ if !core_typevar_names(expected_inner).is_empty() => {
                    core_pattern_matches(expected_inner, actual_inner, bindings)
                }
                _ => expected_inner == actual_inner,
            }
        }
        _ => expected == actual || core_is_subtype(actual, expected),
    }
}

fn core_is_subtype(actual: &CoreType, expected: &CoreType) -> bool {
    CoreSubtypeEngine::new().is_subtype(actual, expected)
}

fn bind_or_check_core_type_var(
    var: &CoreTypeVar,
    actual: &CoreType,
    bindings: &mut HashMap<String, CoreType>,
) -> bool {
    if let Some(lower_bound) = &var.lower_bound {
        if !core_is_subtype(lower_bound, actual) {
            return false;
        }
    }
    if let Some(upper_bound) = &var.upper_bound {
        if !core_is_subtype(actual, upper_bound) {
            return false;
        }
    }

    let var_name = &var.name;
    if var_name == "_" {
        return true;
    }
    if let Some(existing) = bindings.get(var_name) {
        actual == existing
    } else {
        bindings.insert(var_name.clone(), actual.clone());
        true
    }
}

fn bind_or_check_julia_type_var(
    var_name: &str,
    bound: Option<&str>,
    arg_ty: &JuliaType,
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    bind_or_check_julia_type_var_bounded(var_name, bound, None, arg_ty, bindings)
}

/// Bind/check a `where` type variable against an argument type, enforcing both
/// the upper and (optional) lower bound.
///
/// The lower bound (`lower <: arg`) is only meaningful in invariant positions
/// such as `Type{T}` where `T` is bound to the argument exactly. In covariant
/// positions (`x::T`) Julia widens `T` to a union that absorbs the declared
/// lower bound, so callers there must pass `lower = None` (Issue #5051).
fn bind_or_check_julia_type_var_bounded(
    var_name: &str,
    upper: Option<&str>,
    lower: Option<&str>,
    arg_ty: &JuliaType,
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    let arg_core = CoreType::from(arg_ty);
    if let Some(bound_name) = usable_upper_bound(upper) {
        let bound_core = CoreType::from_julia_name(bound_name);
        if !core_is_subtype(&arg_core, &bound_core) {
            return false;
        }
    }
    if let Some(lower_name) = lower {
        let lower_core = CoreType::from_julia_name(lower_name);
        if !core_is_subtype(&lower_core, &arg_core) {
            return false;
        }
    }

    if let Some(existing) = bindings.get(var_name) {
        arg_ty == existing
    } else {
        bindings.insert(var_name.to_string(), arg_ty.clone());
        true
    }
}

fn find_type_param<'a>(type_params: &'a [TypeParam], var_name: &str) -> Option<&'a TypeParam> {
    type_params
        .iter()
        .find(|tp| type_param_base_name(&tp.name) == var_name)
}

fn type_param_upper_bound(type_param: &TypeParam) -> Option<&str> {
    type_param
        .get_upper_bound()
        .map(String::as_str)
        .and_then(|bound| usable_upper_bound(Some(bound)))
        .or_else(|| {
            type_param
                .name
                .contains("<:")
                .then(|| usable_upper_bound(Some(&type_param.name)))
                .flatten()
        })
}

/// The declared lower bound of a `where` type parameter (`Lower<:T` or
/// `Lower<:T<:Upper`), if any. Used to enforce `Lower <: arg` in invariant
/// positions such as `Type{T}` (Issue #5051).
fn type_param_lower_bound(type_param: &TypeParam) -> Option<&str> {
    type_param
        .lower_bound
        .as_deref()
        .map(str::trim)
        .filter(|lower| !lower.is_empty())
}

fn type_param_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

fn upper_bound_type_name(bound: &str) -> &str {
    bound
        .rsplit_once("<:")
        .map_or(bound, |(_, upper)| upper)
        .trim()
}

fn usable_upper_bound(bound: Option<&str>) -> Option<&str> {
    let normalized = upper_bound_type_name(bound?);
    (!normalized.is_empty() && normalized != "<:").then_some(normalized)
}

/// When a `where` type variable's upper bound is itself a *parametric* type that
/// mentions another `where` variable — `T<:Vector{S}` with `S<:Number` — the
/// covariant parameter `x::T` is equivalent to `x::Vector{S}`. Parse such a
/// bound into a pattern so the argument can be matched structurally (binding the
/// inner variable and enforcing its own bound) via the existing parametric path,
/// instead of an opaque `from_julia_name` subtype check that drops `S<:Number`
/// and rejects the concrete element (Issue #5383, sub-case 2).
///
/// Returns `None` for non-parametric bounds (`T<:Number`) and for parametric
/// bounds with no inner type variable (`T<:Vector{Int64}`), both of which the
/// ordinary bound check already handles correctly.
fn parametric_typevar_bound_pattern(
    upper: Option<&str>,
    type_params: &[TypeParam],
) -> Option<JuliaType> {
    let bound = upper?;
    if !bound.contains('{') {
        return None;
    }
    let parsed = JuliaType::from_name_or_struct(bound);
    julia_type_mentions_type_params(&parsed, type_params).then_some(parsed)
}

/// Whether the actual struct base name belongs to the expected pattern's
/// nominal family. The membership decision is delegated to the shared
/// subtype engine instead of a hand-rolled alias list (Issue #5915):
/// `is_subtype_by_name("Vector", "Array")` is true, `"BitVector"` /
/// `"Dict"` are not. The rank-erasing direction (`expected == "Vector"`,
/// `actual == "Array"`) must stay false as in upstream Julia
/// (`Array <: Vector` is false because the rank is not fixed), but the
/// engine's bare-name query is existentially loose there, so only the
/// fixed-rank-erased `Array` family question is delegated.
fn struct_family_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        || (expected == "Array" && CoreSubtypeEngine::new().is_subtype_by_name(actual, expected))
}

fn core_typevar_names(core: &CoreType) -> Vec<String> {
    match core {
        CoreType::TypeVar(var) => vec![var.name.clone()],
        CoreType::Struct { params, .. } | CoreType::Tuple(params) | CoreType::Union(params) => {
            params.iter().flat_map(core_typevar_names).collect()
        }
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => core_typevar_names(inner),
        CoreType::VarargLen { element, len } => {
            let mut names = core_typevar_names(element);
            names.extend(core_typevar_names(len));
            names
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .flat_map(|(_, ty)| core_typevar_names(ty))
            .collect(),
        CoreType::UnionAll { var, body } => {
            let mut names = vec![var.name.clone()];
            names.extend(core_typevar_names(body));
            names
        }
        CoreType::Bottom
        | CoreType::Any
        | CoreType::Primitive(_)
        | CoreType::Abstract(_)
        | CoreType::AbstractUser { .. }
        | CoreType::Value(_)
        | CoreType::Module(_)
        | CoreType::Named(_) => vec![],
    }
}

fn core_type_has_previously_bound_typevars(
    core: &CoreType,
    bindings: &HashMap<String, CoreType>,
) -> bool {
    core_typevar_names(core)
        .iter()
        .any(|name| name.as_str() != "_" && bindings.contains_key(name))
}

/// Runtime (value-side) twin of [`julia_type_pattern_matches`]: match one call
/// argument's runtime type against a declared parameter type while tracking
/// `where` type-variable bindings (Issue #5915).
///
/// The VM derives `arg_value_type` from the argument value
/// (`Vm::get_value_julia_type`) and passes the type-object payload when the
/// argument is a first-class type (`Value::DataType`). Judgments that need the
/// runtime *value* representation (native array wrappers, dict payloads, ...)
/// stay VM-owned behind `value_fallback`; everything binding-aware lives here,
/// with the `<:` legs engine-backed through `JuliaType::is_subtype_of` (which
/// delegates to the shared `CoreSubtypeEngine`).
///
/// This intentionally preserves the historical runtime matcher semantics
/// (previously the private `Vm::value_matches_param_with_bindings`); merging it
/// with the compile-side [`julia_type_pattern_matches`] is the remaining #6502
/// unification step.
pub fn runtime_value_type_matches_param_with_bindings(
    hierarchy: &StructHierarchy,
    arg_value_type: &JuliaType,
    arg_type_object: Option<&JuliaType>,
    param_ty: &JuliaType,
    type_params: &[TypeParam],
    bindings: &mut HashMap<String, JuliaType>,
    value_fallback: impl FnOnce() -> bool,
) -> bool {
    if let JuliaType::TypeVar(var_name, bound) = param_ty {
        let type_param = specificity::find_type_param(type_params, var_name);
        return bind_or_check_runtime_type_var(
            hierarchy,
            var_name,
            specificity::usable_upper_bound(bound.as_deref())
                .or_else(|| type_param.and_then(specificity::type_param_upper_bound)),
            arg_value_type,
            bindings,
        );
    }
    if let JuliaType::Struct(var_name) = param_ty {
        if let Some(type_param) = specificity::find_type_param(type_params, var_name) {
            return bind_or_check_runtime_type_var(
                hierarchy,
                var_name,
                specificity::type_param_upper_bound(type_param),
                arg_value_type,
                bindings,
            );
        }
        if runtime_concrete_leaf_struct_param(hierarchy, var_name) {
            return matches!(
                arg_value_type,
                JuliaType::Struct(arg_name)
                    if runtime_same_concrete_struct_name(arg_name, var_name)
            );
        }
    }

    if let (Some(dt), JuliaType::TypeOf(inner)) = (arg_type_object, param_ty) {
        if let JuliaType::TypeVar(var_name, bound) = inner.as_ref() {
            let type_param = specificity::find_type_param(type_params, var_name);
            return bind_or_check_runtime_type_var(
                hierarchy,
                var_name,
                specificity::usable_upper_bound(bound.as_deref())
                    .or_else(|| type_param.and_then(specificity::type_param_upper_bound)),
                dt,
                bindings,
            );
        }
        if let JuliaType::Struct(var_name) = inner.as_ref() {
            if let Some(type_param) = specificity::find_type_param(type_params, var_name) {
                return bind_or_check_runtime_type_var(
                    hierarchy,
                    var_name,
                    specificity::type_param_upper_bound(type_param),
                    dt,
                    bindings,
                );
            }
        }
        if let Some(extracted) = dt.extract_type_bindings(inner, type_params) {
            for (var_name, bound_type) in extracted {
                let Some(type_param) = specificity::find_type_param(type_params, &var_name) else {
                    continue;
                };
                if !bind_or_check_runtime_type_var(
                    hierarchy,
                    &var_name,
                    specificity::type_param_upper_bound(type_param),
                    &bound_type,
                    bindings,
                ) {
                    return false;
                }
            }
            return true;
        }
    }

    if runtime_julia_type_contains_type_var(param_ty)
        || runtime_julia_type_mentions_type_params(param_ty, type_params)
        || runtime_julia_type_needs_array_projection_match(param_ty)
    {
        // A first-class type argument dispatches as `Type{T}` (the runtime
        // analogue of `Vm::dispatch_julia_type_for_value`); plain values use
        // their derived runtime type.
        let type_object_dispatch_type;
        let arg_jtype = if let Some(dt) = arg_type_object {
            type_object_dispatch_type = JuliaType::TypeOf(Box::new(dt.clone()));
            &type_object_dispatch_type
        } else {
            arg_value_type
        };
        let Some(extracted) = arg_jtype.extract_type_bindings(param_ty, type_params) else {
            return false;
        };
        for (var_name, bound_type) in extracted {
            let Some(type_param) = specificity::find_type_param(type_params, &var_name) else {
                continue;
            };
            if !bind_or_check_runtime_type_var(
                hierarchy,
                &var_name,
                specificity::type_param_upper_bound(type_param),
                &bound_type,
                bindings,
            ) {
                return false;
            }
        }
        return true;
    }

    value_fallback()
}

/// Bind/check a `where` type variable against a runtime-derived argument type.
///
/// Runtime flavor of [`bind_or_check_julia_type_var`]: the upper bound is
/// enforced by the shared [`CoreSubtypeEngine`] with the VM's
/// [`StructHierarchy`], so user-defined abstract bounds no longer fall back to
/// the older `JuliaType::from_name`-only gate.
fn bind_or_check_runtime_type_var(
    hierarchy: &StructHierarchy,
    var_name: &str,
    bound: Option<&str>,
    arg_ty: &JuliaType,
    bindings: &mut HashMap<String, JuliaType>,
) -> bool {
    if let Some(bound_name) = specificity::usable_upper_bound(bound) {
        let arg_core = CoreType::from(arg_ty);
        let bound_core = CoreType::from_julia_name(bound_name);
        if !CoreSubtypeEngine::with_hierarchy(hierarchy).is_subtype(&arg_core, &bound_core) {
            return false;
        }
    }

    if var_name == "_" {
        return true;
    }

    if let Some(existing) = bindings.get(var_name) {
        arg_ty == existing
    } else {
        bindings.insert(var_name.to_string(), arg_ty.clone());
        true
    }
}

/// Structural scan for an unbound `TypeVar` anywhere inside a declared
/// parameter type (runtime matcher gate; also used by the VM's tuple
/// `type_matches` to decide between pure-subtype and wildcard matching).
pub fn runtime_julia_type_contains_type_var(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::TypeVar(_, _) => true,
        JuliaType::TupleOf(types) | JuliaType::Union(types) => {
            types.iter().any(runtime_julia_type_contains_type_var)
        }
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) | JuliaType::TypeOf(inner) => {
            runtime_julia_type_contains_type_var(inner)
        }
        JuliaType::UnionAll { body, .. } => runtime_julia_type_contains_type_var(body),
        JuliaType::Struct(_) => core_type_pattern_has_explicit_bound(&CoreType::from(ty)),
        _ => false,
    }
}

/// Whether `ty` mentions any of the method's `where` parameters as a free
/// variable. Unlike the compile-side [`julia_type_mentions_type_params`] this
/// uses `JuliaType::mentions_free_var` (whole-token struct-parameter scan with
/// `UnionAll` binder shadowing) — the historical runtime matcher gate.
fn runtime_julia_type_mentions_type_params(ty: &JuliaType, type_params: &[TypeParam]) -> bool {
    type_params
        .iter()
        .any(|tp| ty.mentions_free_var(specificity::type_param_base_name(&tp.name)))
}

fn runtime_concrete_leaf_struct_param(hierarchy: &StructHierarchy, name: &str) -> bool {
    !name.contains('{')
        && hierarchy
            .entry(name)
            .is_some_and(|entry| entry.type_params().is_empty())
}

fn runtime_same_concrete_struct_name(actual: &str, expected: &str) -> bool {
    let actual_base = actual.split('{').next().unwrap_or(actual);
    let expected_base = expected.split('{').next().unwrap_or(expected);
    if actual_base.contains('.') && expected_base.contains('.') {
        return actual_base == expected_base;
    }
    actual_base.rsplit('.').next().unwrap_or(actual_base)
        == expected_base.rsplit('.').next().unwrap_or(expected_base)
}

/// Whether the declared parameter is an array-shaped pattern that must be
/// matched through `extract_type_bindings` (projecting the runtime array type
/// onto `Vector{T}` / `Matrix{T}` / `AbstractVector{T}` / `AbstractMatrix{T}`)
/// even when it binds no type variable.
fn runtime_julia_type_needs_array_projection_match(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::VectorOf(_) | JuliaType::MatrixOf(_))
        || (matches!(ty, JuliaType::Struct(_))
            && (specificity::abstract_vector_param_type(ty).is_some()
                || specificity::abstract_matrix_param_type(ty).is_some()))
}

/// Runtime single-argument matcher between a rendered runtime type name and a
/// declared parameter `JuliaType` (Issue #5915): the matching policy of the
/// VM's typed dynamic dispatch (`Vm::check_type_match`), centralized next to
/// the other shared matchers.
///
/// The VM supplies `is_known_struct_base` (declared-struct lookup for the
/// Issue #5314 leaf-struct guard) and `subtype_by_name` (the engine-backed
/// runtime `<:` authority, `Vm::check_subtype`).
pub fn runtime_type_name_matches_param(
    arg_type_name: &str,
    param_jt: &JuliaType,
    is_known_struct_base: impl FnOnce(&str) -> bool,
    subtype_by_name: impl FnOnce(&str, &str) -> bool,
) -> bool {
    // Any parameter type matches any argument.
    if matches!(param_jt, JuliaType::Any) {
        return true;
    }

    let param_type_name = param_jt.name();

    // Exact match.
    if arg_type_name == param_type_name.as_ref() {
        return true;
    }

    // Issue #5314: when the parameter names a known, concrete (non-parametric)
    // struct, only an argument of that same struct may match. A struct is a
    // final leaf type with no subtypes, so a primitive argument (`Int64`,
    // `Float64`, ...) must be rejected here. Otherwise the context-free
    // `CoreType` layer below misclassifies a struct name spelled as an
    // uppercase letter followed by digits (`Q`, `Q5314`) as an unbounded type
    // variable and `dispatch_pattern_score` matches *any* argument — making a
    // dynamic `min(a.I, b.I)` (untyped fields) mis-dispatch `Int64` values into
    // the `min(::Q, ::Q)` method.
    if let JuliaType::Struct(name) = param_jt {
        if !name.contains('{') {
            let param_base = name.rsplit('.').next().unwrap_or(name);
            if is_known_struct_base(param_base) {
                let arg_base = arg_type_name
                    .split('{')
                    .next()
                    .unwrap_or(arg_type_name)
                    .rsplit('.')
                    .next()
                    .unwrap_or(arg_type_name);
                return arg_base == param_base;
            }
        }
    }

    let arg_core = CoreType::from_julia_name(arg_type_name);
    let param_core = CoreType::from(param_jt);
    if param_core.dispatch_pattern_score(&arg_core) > 0 {
        return true;
    }

    subtype_by_name(arg_type_name, &param_type_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn strings(types: &[&str]) -> Vec<String> {
        types.iter().map(|ty| (*ty).to_string()).collect()
    }

    #[test]
    fn runtime_struct_leaf_guard_survives_structured_matcher_issue_5314() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Q5314", Some("Any".to_string()), Vec::new());

        let mut bindings = HashMap::new();
        assert!(runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::Struct("Q5314".to_string()),
            None,
            &JuliaType::Struct("Q5314".to_string()),
            &[],
            &mut bindings,
            || false,
        ));

        let mut bindings = HashMap::new();
        assert!(!runtime_value_type_matches_param_with_bindings(
            &hierarchy,
            &JuliaType::Float64,
            None,
            &JuliaType::Struct("Q5314".to_string()),
            &[],
            &mut bindings,
            || true,
        ));
    }

    /// `struct_family_matches` is decided by the shared subtype engine
    /// (Issue #5915): the legacy Array alias family keeps matching, and
    /// engine-known abstract container bases admit their concrete
    /// carriers (julia: `Vector{Int64} <: AbstractVector{Int64}`).
    #[test]
    fn struct_family_matching_uses_subtype_engine_issue_5915() {
        // Legacy Array alias family preserved.
        assert!(type_name_pattern_matches(
            &strings(&["Array{Int64}"]),
            &strings(&["Vector{Int64}"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["Array{Int64}"]),
            &strings(&["Vector{Float64}"])
        ));
        // Abstract container bases stay outside the strict pattern tier;
        // they are admitted (at a lower score) by the subtype-fallback
        // channel in `resolve_type_name_candidates_with_subtype_fallback`,
        // whose `subtype_matches` closure is the VM's engine-backed
        // `check_subtype`.
        assert!(!type_name_pattern_matches(
            &strings(&["AbstractVector{Int64}"]),
            &strings(&["Vector{Float64}"])
        ));
        // Unrelated families stay rejected.
        assert!(!type_name_pattern_matches(
            &strings(&["Vector{Int64}"]),
            &strings(&["Dict{String, Int64}"])
        ));
        // Bare nominal family question (julia: Matrix <: Array).
        assert!(struct_family_matches("Array", "Matrix"));
        assert!(!struct_family_matches("Array", "BitVector"));
        assert!(!struct_family_matches("Vector", "Array"));
    }

    #[test]
    fn runtime_resolver_matches_exact_abstract_parametric_and_typevars() {
        let candidates = [
            (10usize, strings(&["Any"])),
            (11usize, strings(&["Real"])),
            (12usize, strings(&["Vector{T}"])),
            (13usize, strings(&["Vector{Int64}"])),
        ];

        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Vector{Int64}"])
            ),
            Some((13, 6))
        );
        assert_eq!(
            resolve_type_name_candidates(
                candidates[..2]
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Int64"])
            ),
            Some((11, 2))
        );
    }

    #[test]
    fn runtime_resolver_matches_matrix_times_diagonal() {
        let candidates = [(99usize, strings(&["Any", "Diagonal"]))];
        let actual = strings(&["Matrix{Float64}", "LinearAlgebra.Diagonal{Float64}"]);
        let actual_refs: Vec<&str> = actual.iter().map(String::as_str).collect();

        let mut subtype = |actual: &str, expected: &str| -> bool {
            if expected == "Any" {
                return true;
            }
            let actual_base = actual.find('{').map_or(actual, |idx| &actual[..idx]);
            let expected_base = expected.find('{').map_or(expected, |idx| &expected[..idx]);
            actual_base.rsplit('.').next().unwrap_or(actual_base)
                == expected_base.rsplit('.').next().unwrap_or(expected_base)
        };

        assert_eq!(
            resolve_runtime_type_pattern_candidates(
                candidates
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.iter().map(String::as_str).collect())),
                &actual_refs,
                &mut subtype,
            ),
            Some((99, 3))
        );
    }

    #[test]
    fn runtime_resolver_prefers_bare_memory_family_over_any_issue_4052() {
        let candidates = [
            (10usize, strings(&["Any", "Any"])),
            (11usize, strings(&["Any", "Memory"])),
        ];

        assert!(type_name_pattern_matches(
            &strings(&["Memory"]),
            &strings(&["Memory{Int64}"])
        ));
        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Function", "Memory{Int64}"])
            ),
            Some((11, 5))
        );
    }

    #[test]
    fn runtime_resolver_reuses_typevar_bindings() {
        assert!(type_name_pattern_matches(
            &strings(&["T", "T"]),
            &strings(&["Int64", "Int64"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["T", "T"]),
            &strings(&["Int64", "Float64"])
        ));
        assert!(
            type_name_pattern_specificity(&strings(&["T", "T"]))
                > type_name_pattern_specificity(&strings(&["T", "S"]))
        );
    }

    #[test]
    fn typed_core_specificity_matches_rendered_policy_issue_6502() {
        let signatures = [
            vec!["Any"],
            vec!["T", "T"],
            vec!["T", "S"],
            vec!["Type"],
            vec!["Type{T}"],
            vec!["Type{<:Number}"],
            vec!["Type{Int64}"],
            vec!["Vector{T}", "Vector{T}"],
            vec!["Vector{<:Real}", "Vector{<:Real}"],
            vec!["Tuple{}"],
            vec!["Tuple{Int64, Float64}"],
            vec!["Union{}"],
            vec!["Union{Int64, String}"],
            vec!["Vector{T} where T<:Real"],
        ];

        for signature in signatures {
            let rendered = strings(&signature);
            let slots: Vec<_> = rendered
                .iter()
                .map(|name| CoreType::from_julia_name(name))
                .collect();
            assert_eq!(
                core_type_pattern_specificity(&slots),
                type_name_pattern_specificity(&rendered),
                "signature {signature:?}"
            );
        }
    }

    #[test]
    fn runtime_resolver_handles_covariant_bound_patterns() {
        assert!(type_name_pattern_matches(
            &strings(&["Vector{_<:Real}"]),
            &strings(&["Vector{Int64}"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["Vector{_<:Integer}"]),
            &strings(&["Vector{Float64}"])
        ));
    }

    #[test]
    fn runtime_resolver_keeps_invariant_vector_params_issue_4276() {
        let sig = strings(&["Vector{Int64}", "Int64"]);
        let actual = strings(&["Vector{Any}", "Int64"]);
        assert!(!type_name_pattern_matches(&sig, &actual));
        assert!(same_invariant_container_family_concrete_miss(
            &sig[0], &actual[0]
        ));
        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                std::iter::once((1usize, sig.as_slice())),
                &actual,
                |actual, expected| CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(expected)),
            ),
            None
        );
    }

    #[test]
    fn runtime_resolver_uses_type_singleton_specificity_issue_4131() {
        let exact_any_candidates = [(1, strings(&["Type"])), (2, strings(&["Type{Any}"]))];
        assert_eq!(
            resolve_type_name_candidates(
                exact_any_candidates
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Any}"])
            ),
            Some((2, 8))
        );
        assert_eq!(
            resolve_type_name_candidates(
                exact_any_candidates
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Int64}"])
            ),
            Some((1, 5))
        );

        let typevar_candidates = [(1, strings(&["Type"])), (2, strings(&["Type{T}"]))];
        assert_eq!(
            resolve_type_name_candidates(
                typevar_candidates
                    .iter()
                    .map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Int64}"])
            ),
            Some((2, 7))
        );
    }

    #[test]
    fn runtime_resolver_binds_typeof_parametric_inner_typevars_issue_4569() {
        assert!(type_name_pattern_matches(
            &strings(&["Type{Array{T}}"]),
            &strings(&["Type{Array{Int64}}"])
        ));
        assert!(!type_name_pattern_matches(
            &strings(&["Type{Array{Real}}"]),
            &strings(&["Type{Array{Int64}}"])
        ));

        let candidates = [
            (1, strings(&["Array{T}", "Tuple"])),
            (2, strings(&["Type{Array{T}}", "Tuple"])),
        ];
        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Array{Int64}}", "Tuple"])
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn runtime_resolver_prefers_parametric_type_pattern_over_bare_type_issue_4636() {
        let candidates = [
            (1, strings(&["Type{Pair}", "Tuple"])),
            (2, strings(&["Type{Pair{K,V}}", "Tuple"])),
            (3, strings(&["Type{T}", "Tuple"])),
        ];

        assert!(type_name_pattern_matches(
            &strings(&["Type{Pair{K,V}}", "Tuple"]),
            &strings(&["Type{Pair{Int64,Int8}}", "Tuple{Int64}"])
        ));
        assert_eq!(
            resolve_type_name_candidates(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Pair{Int64,Int8}}", "Tuple{Int64}"])
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_resolver_uses_covariant_subtype_fallback_issue_3910() {
        let candidates = [
            (1, strings(&["Type{_<:Animal}"])),
            (2, strings(&["Type{Dog}"])),
        ];

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Dog}"]),
                |actual, bound| matches!((actual, bound), ("Dog", "Animal") | ("Cat", "Animal"))
            ),
            Some((2, 13))
        );

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Cat}"]),
                |actual, bound| matches!((actual, bound), ("Dog", "Animal") | ("Cat", "Animal"))
            ),
            Some((1, 7))
        );

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Rock}"]),
                |actual, bound| matches!((actual, bound), ("Dog", "Animal") | ("Cat", "Animal"))
            ),
            None
        );
    }

    /// Issue #6502: the typed-dispatch structured resolver preserves the
    /// legacy quality/specificity ordering while matching on cached CoreType
    /// slots instead of reparsing rendered names at each call site.
    #[test]
    fn typed_core_resolver_matches_legacy_string_order_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let rows: Vec<(usize, Vec<String>, Vec<CoreType>)> = vec![
            (1, strings(&["Any"]), vec![CoreType::Any]),
            (
                2,
                strings(&["Type{T}"]),
                vec![CoreType::from_julia_name("Type{T}")],
            ),
            (
                3,
                strings(&["Type{<:Number}"]),
                vec![CoreType::from_julia_name("Type{<:Number}")],
            ),
            (
                4,
                strings(&["Type{Int64}"]),
                vec![CoreType::from_julia_name("Type{Int64}")],
            ),
        ];
        let actual = strings(&["Type{Int64}"]);
        let actual_cores: Vec<_> = actual
            .iter()
            .map(|name| CoreType::from_julia_name(name))
            .collect();

        let legacy = resolve_type_name_candidates_with_subtype_fallback(
            rows.iter()
                .map(|(idx, rendered, _)| (*idx, rendered.as_slice())),
            &actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(
                    &CoreType::from_julia_name(actual),
                    &CoreType::from_julia_name(expected),
                )
            },
        );
        let structured = resolve_typed_runtime_core_candidates_with_subtype_fallback(
            &hierarchy,
            rows.iter()
                .map(|(idx, rendered, slots)| RuntimeTypedCoreCandidate {
                    idx: *idx,
                    rendered: rendered.as_slice(),
                    slots: slots.as_slice(),
                    signature: None,
                }),
            &actual_cores,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );

        assert_eq!(structured, legacy);
        assert_eq!(structured.map(|(idx, _)| idx), Some(4));
    }

    #[test]
    fn typed_core_resolver_uses_covariant_slots_without_rendered_bridge_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let rendered = strings(&["Vector{Any}"]);
        let slots = [CoreType::from_julia_name("Vector{<:Real}")];
        let rows = [RuntimeTypedCoreCandidate {
            idx: 1,
            rendered: rendered.as_slice(),
            slots: slots.as_slice(),
            signature: None,
        }];
        let run = |actual: &str| {
            let actual = [CoreType::from_julia_name(actual)];
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Vector{Int64}"), Some(1));
        assert_eq!(run("Vector{String}"), None);
    }

    #[test]
    fn typed_core_resolver_tier_split_uses_bounded_slots_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let broad_rendered = strings(&["Any"]);
        let broad_slots = [CoreType::Any];
        let bounded_rendered_without_marker = strings(&["Vector{Any}"]);
        let bounded_slots = [CoreType::from_julia_name("Vector{<:Real}")];
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: broad_rendered.as_slice(),
                slots: broad_slots.as_slice(),
                signature: None,
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: bounded_rendered_without_marker.as_slice(),
                slots: bounded_slots.as_slice(),
                signature: None,
            },
        ];
        let actual = [CoreType::from_julia_name("Vector{Int64}")];

        let selected = resolve_typed_runtime_core_candidates_with_subtype_fallback(
            &hierarchy,
            rows,
            &actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        )
        .map(|(idx, _)| idx);

        assert_eq!(selected, Some(1));
    }

    /// Issue #6502 / #6229: typed-dispatch candidates may still declare
    /// `JuliaType::Array` while their rendered runtime signature preserves the
    /// parametric `Vector{T}` shape. Keep that shape in structured slots so
    /// repeated typevars reject mixed element types.
    #[test]
    fn typed_core_resolver_keeps_rendered_array_diagonal_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let diagonal_rendered = strings(&["Vector{T}", "Vector{T}"]);
        let diagonal_slots: Vec<CoreType> = diagonal_rendered
            .iter()
            .map(|name| {
                embed_type_param_bounds(
                    runtime_candidate_core_type(&JuliaType::Array, name),
                    &type_params,
                )
            })
            .collect();
        assert_eq!(
            diagonal_slots[0],
            embed_type_param_bounds(CoreType::from_julia_name("Vector{T}"), &type_params)
        );

        let diagonal_signature = runtime_core_signature(&diagonal_slots, &type_params);
        let independent_rendered = strings(&["Vector{<:Real}", "Vector{<:Real}"]);
        let independent_slots: Vec<CoreType> = independent_rendered
            .iter()
            .map(|name| CoreType::from_julia_name(name))
            .collect();
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: diagonal_rendered.as_slice(),
                slots: diagonal_slots.as_slice(),
                signature: Some(&diagonal_signature),
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: independent_rendered.as_slice(),
                slots: independent_slots.as_slice(),
                signature: None,
            },
        ];

        let run = |left: &str, right: &str| {
            let actual = [
                CoreType::from_julia_name(left),
                CoreType::from_julia_name(right),
            ];
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Vector{Int64}", "Vector{Int64}"), Some(1));
        assert_eq!(run("Vector{Int64}", "Vector{Float64}"), Some(2));
    }

    #[test]
    fn typed_resolver_rejects_mismatched_type_vector_diagonal_issue_6239() {
        let candidates = [
            (1, strings(&["Type{T}", "AbstractVector{T}"])),
            (2, strings(&["Type{Integer}", "AbstractVector{<:Real}"])),
        ];

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Integer}", "Vector{Int64}"]),
                |actual, bound| matches!(
                    (actual, bound),
                    ("Vector{Int64}", "AbstractVector{<:Real}")
                        | ("Vector{Int64}", "AbstractVector")
                        | ("Int64", "Real")
                )
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_core_resolver_rejects_mismatched_type_vector_diagonal_issue_6573() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let diagonal_rendered = strings(&["Type{T}", "AbstractVector{T}"]);
        let diagonal_slots: Vec<CoreType> = diagonal_rendered
            .iter()
            .map(|name| embed_type_param_bounds(CoreType::from_julia_name(name), &type_params))
            .collect();
        let diagonal_signature = runtime_core_signature(&diagonal_slots, &type_params);
        let fixed_rendered = strings(&["Type{Integer}", "AbstractVector{<:Real}"]);
        let fixed_slots: Vec<CoreType> = fixed_rendered
            .iter()
            .map(|name| CoreType::from_julia_name(name))
            .collect();
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: diagonal_rendered.as_slice(),
                slots: diagonal_slots.as_slice(),
                signature: Some(&diagonal_signature),
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: fixed_rendered.as_slice(),
                slots: fixed_slots.as_slice(),
                signature: None,
            },
        ];
        let actual = [
            CoreType::from_julia_name("Type{Integer}"),
            CoreType::from_julia_name("Vector{Int64}"),
        ];

        assert_eq!(
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_core_resolver_matches_rank_typevar_abstract_array_issue_6577() {
        let hierarchy = StructHierarchy::new();
        let diagonal_type_params = [
            TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            TypeParam::new("N".to_string()),
        ];
        let diagonal_rendered = strings(&["Type{T}", "AbstractArray{T,N}"]);
        let diagonal_slots: Vec<CoreType> = diagonal_rendered
            .iter()
            .map(|name| {
                embed_type_param_bounds(CoreType::from_julia_name(name), &diagonal_type_params)
            })
            .collect();
        let diagonal_signature = runtime_core_signature(&diagonal_slots, &diagonal_type_params);

        let fixed_type_params = [TypeParam::new("N".to_string())];
        let fixed_rendered = strings(&["Type{Integer}", "AbstractArray{<:Real,N}"]);
        let fixed_slots: Vec<CoreType> = fixed_rendered
            .iter()
            .map(|name| {
                embed_type_param_bounds(CoreType::from_julia_name(name), &fixed_type_params)
            })
            .collect();
        let fixed_signature = runtime_core_signature(&fixed_slots, &fixed_type_params);
        let rows = [
            RuntimeTypedCoreCandidate {
                idx: 1,
                rendered: diagonal_rendered.as_slice(),
                slots: diagonal_slots.as_slice(),
                signature: Some(&diagonal_signature),
            },
            RuntimeTypedCoreCandidate {
                idx: 2,
                rendered: fixed_rendered.as_slice(),
                slots: fixed_slots.as_slice(),
                signature: Some(&fixed_signature),
            },
        ];
        let actual = [
            CoreType::from_julia_name("Type{Integer}"),
            CoreType::from_julia_name("Vector{Int64}"),
        ];

        assert_eq!(
            resolve_typed_runtime_core_candidates_with_subtype_fallback(
                &hierarchy,
                rows,
                &actual,
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn typed_resolver_prefers_type_value_diagonal_issue_6233() {
        let candidates = [
            (1, strings(&["Type{T}", "T<:Real"])),
            (2, strings(&["Type{Integer}", "Integer"])),
        ];

        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Int64}", "Int64"]),
                |actual, bound| CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(bound)),
            )
            .map(|(idx, _)| idx),
            Some(1)
        );
        assert_eq!(
            resolve_type_name_candidates_with_subtype_fallback(
                candidates.iter().map(|(idx, sig)| (*idx, sig.as_slice())),
                &strings(&["Type{Integer}", "Int64"]),
                |actual, bound| CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(bound)),
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn runtime_type_pattern_candidates_use_shared_scores_issue_3910() {
        let actual = ["Rational{Int64}", "Int64"];
        let candidates = [
            (1, vec!["Rational", "Real"]),
            (2, vec!["Rational{T}", "Integer"]),
            (3, vec!["Rational{Int64}", "Int64"]),
        ];

        assert_eq!(
            resolve_runtime_type_pattern_candidates(candidates, &actual, |actual, expected| {
                CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(expected))
            }),
            Some((3, 8))
        );
    }

    #[test]
    fn runtime_type_pattern_score_uses_subtype_fallback_issue_3910() {
        let mut subtype_matches = |actual: &str, expected: &str| {
            CoreType::from_julia_name(actual).is_subtype_of(&CoreType::from_julia_name(expected))
        };

        assert_eq!(
            runtime_type_pattern_score(
                &["Real", "Number"],
                &["Int64", "Float64"],
                &mut subtype_matches
            ),
            Some(2)
        );

        let mut no_subtype_fallback = |_: &str, _: &str| false;
        assert_eq!(
            runtime_type_pattern_score(
                &["Real", "Number"],
                &["Int64", "Float64"],
                &mut no_subtype_fallback
            ),
            None
        );
    }

    #[test]
    fn runtime_type_pattern_family_fallback_respects_parametric_miss_issue_4020() {
        assert_eq!(
            resolve_runtime_type_pattern_candidates_with_family_fallback(
                std::iter::once((1, vec!["Matrix{_<:Integer}"])),
                &["Matrix{Float64}"],
                |actual, expected| {
                    actual.split('{').next().unwrap_or(actual)
                        == expected.split('{').next().unwrap_or(expected)
                },
                |actual, expected| {
                    CoreType::from_julia_name(actual)
                        .is_subtype_of(&CoreType::from_julia_name(expected))
                },
            ),
            None
        );
    }

    #[test]
    fn unary_runtime_type_pattern_candidates_keep_call_dynamic_order_issue_3910() {
        let actual = ["Rational{Int64}"];
        let candidates = [
            (1, vec!["Real"]),
            (2, vec!["Rational"]),
            (3, vec!["Rational{T}"]),
            (4, vec!["Rational{Int64}"]),
        ];

        assert_eq!(
            resolve_runtime_type_pattern_candidates(candidates, &actual, |actual, expected| {
                CoreType::from_julia_name(actual)
                    .is_subtype_of(&CoreType::from_julia_name(expected))
            }),
            Some((4, 4))
        );

        let tied_candidates = [(10, vec!["Rational{T}"]), (11, vec!["Rational{S}"])];
        assert_eq!(
            resolve_runtime_type_pattern_candidates(tied_candidates, &actual, |_, _| false),
            Some((10, 3))
        );
    }

    #[test]
    fn runtime_type_pattern_candidates_use_family_fallback_issue_3910() {
        let actual = ["Drop{Vector{Int64}}"];
        let candidates = [(1, vec!["Any"]), (2, vec!["Drop"]), (3, vec!["Drop{T}"])];

        assert_eq!(
            resolve_runtime_type_pattern_candidates_with_family_fallback(
                candidates,
                &actual,
                |actual, expected| {
                    actual.split('{').next().unwrap_or(actual)
                        == expected.split('{').next().unwrap_or(expected)
                },
                |_, _| false
            ),
            Some((3, 3))
        );

        let bare_candidates = [(1, vec!["Any"]), (2, vec!["Drop"])];
        assert_eq!(
            resolve_runtime_type_pattern_candidates_with_family_fallback(
                bare_candidates,
                &actual,
                |actual, expected| {
                    actual.split('{').next().unwrap_or(actual)
                        == expected.split('{').next().unwrap_or(expected)
                },
                |_, _| false
            ),
            Some((2, 2))
        );
    }

    #[test]
    fn runtime_type_pattern_family_fallback_strips_modules_issue_3910() {
        let actual = ["Zip{Tuple{Vector{Int64}, Vector{Int64}}}"];
        let candidates = [
            (1, vec!["Any"]),
            (2, vec!["Base.Iterators.Zip"]),
            (3, vec!["Base.Iterators.Enumerate"]),
        ];

        assert_eq!(
            resolve_runtime_type_pattern_candidates_with_family_fallback(
                candidates,
                &actual,
                |actual, expected| {
                    let actual_base = actual.split('{').next().unwrap_or(actual);
                    let expected_base = expected.split('{').next().unwrap_or(expected);
                    actual_base.rsplit('.').next().unwrap_or(actual_base)
                        == expected_base.rsplit('.').next().unwrap_or(expected_base)
                },
                |_, _| false
            ),
            Some((2, 2))
        );
    }

    #[test]
    fn runtime_type_pattern_scores_module_qualified_parametric_exact_issue_3910() {
        let actual = ["Enumerate{Vector{Int64}}"];
        let candidates = [
            (1, vec!["Any"]),
            (2, vec!["Enumerate{I}"]),
            (3, vec!["Base.Iterators.Enumerate{Vector{Int64}}"]),
        ];

        assert_eq!(
            resolve_runtime_type_pattern_candidates_with_family_fallback(
                candidates,
                &actual,
                |actual, expected| {
                    let actual_base = actual.split('{').next().unwrap_or(actual);
                    let expected_base = expected.split('{').next().unwrap_or(expected);
                    actual_base.rsplit('.').next().unwrap_or(actual_base)
                        == expected_base.rsplit('.').next().unwrap_or(expected_base)
                },
                |_, _| false
            ),
            Some((3, 4))
        );
    }

    /// Issue #6539: the callable-value channel must enforce explicit `where`
    /// bounds through the `core_signature` subtype gate. A bounded
    /// `f(::Holder{T}) where {T<:Real}` must be rejected for
    /// `Holder{String}` (selecting the bare `f(::Holder)` sibling) while
    /// still winning for `Holder{Int64}`.
    #[test]
    fn callable_value_candidates_enforce_where_bounds_issue_6539() {
        let bounded_params = vec![JuliaType::Struct("Holder{T}".to_string())];
        let bare_params = vec![JuliaType::Struct("Holder".to_string())];
        let bounded_type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let candidates = || {
            [
                CallableValueCandidate {
                    idx: 1,
                    param_types: &bounded_params,
                    param_count: 1,
                    vararg_param_index: None,
                    vararg_fixed_count: None,
                    type_params: &bounded_type_params,
                },
                CallableValueCandidate {
                    idx: 2,
                    param_types: &bare_params,
                    param_count: 1,
                    vararg_param_index: None,
                    vararg_fixed_count: None,
                    type_params: &[],
                },
            ]
        };
        // The VM's loose matcher accepts both candidates for any Holder
        // instantiation; the bound gate is what must discriminate.
        let loose = |actual: &str, param: &JuliaType| {
            actual.starts_with("Holder") && param.name().starts_with("Holder")
        };

        // Out-of-bound element type: the bounded method is rejected by the
        // gate, the bare sibling wins.
        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates(),
                &strings(&["Holder{String}"]),
                loose,
                |_, _| false
            )
            .map(|(idx, _)| idx),
            Some(2)
        );

        // In-bound element type: the bounded parametric method passes the
        // gate and outscores the bare sibling.
        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates(),
                &strings(&["Holder{Int64}"]),
                loose,
                |_, _| false
            )
            .map(|(idx, _)| idx),
            Some(1)
        );
    }

    /// Issue #6539: candidates with *unbounded* `where` parameters skip the
    /// signature gate entirely — legacy loose matching is preserved (the
    /// diagonal rule owns their cross-slot consistency).
    #[test]
    fn callable_value_candidates_unbounded_where_skips_gate_issue_6539() {
        let unbounded_params = vec![JuliaType::Struct("Holder{T}".to_string())];
        let unbounded_type_params = vec![TypeParam::new("T".to_string())];
        let candidates = [CallableValueCandidate {
            idx: 1,
            param_types: &unbounded_params,
            param_count: 1,
            vararg_param_index: None,
            vararg_fixed_count: None,
            type_params: &unbounded_type_params,
        }];

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &strings(&["Holder{String}"]),
                |actual, param| {
                    actual.starts_with("Holder") && param.name().starts_with("Holder")
                },
                |_, _| false
            )
            .map(|(idx, _)| idx),
            Some(1)
        );
    }

    #[test]
    fn callable_value_candidates_use_shared_vm_score_policy_issue_3910() {
        let any_params = vec![JuliaType::Any];
        let int_params = vec![JuliaType::Int64];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &any_params,
                param_count: 1,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &int_params,
                param_count: 1,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["Int64"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| param == &JuliaType::Any || param.name() == actual,
                |actual, param| param.name() == actual
            ),
            Some((2, 20))
        );
    }

    #[test]
    fn callable_value_candidates_demote_non_exact_type_any_issue_4657() {
        let type_any_params = vec![
            JuliaType::TypeOf(Box::new(JuliaType::Any)),
            JuliaType::Tuple,
        ];
        let typevar_params = vec![
            JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::Tuple,
        ];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &type_any_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &typevar_params,
                param_count: 2,
                vararg_param_index: None,
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["Type{Union{Nothing, Int64}}", "Tuple"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| match param {
                    JuliaType::TypeOf(inner) => {
                        actual.starts_with("Type{")
                            && (matches!(inner.as_ref(), JuliaType::Any | JuliaType::TypeVar(_, _))
                                || param.name() == actual)
                    }
                    _ => param.name() == actual,
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_candidates_preserve_fixed_prefix_vararg_bonus_issue_3910() {
        let pure_vararg_params = vec![JuliaType::Any];
        let fixed_prefix_params = vec![JuliaType::Int64, JuliaType::Any];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &pure_vararg_params,
                param_count: 1,
                vararg_param_index: Some(0),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &fixed_prefix_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &[],
            },
        ];
        let actual = strings(&["Int64", "Int64"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| param == &JuliaType::Any || param.name() == actual,
                |actual, param| param.name() == actual
            ),
            Some((2, 21))
        );
    }

    #[test]
    fn callable_value_candidates_prefer_partial_parametric_fixed_vararg_issue_8407() {
        let generic_params = vec![JuliaType::Any, JuliaType::Any];
        let batch_params = vec![
            JuliaType::Struct("BatchIntegrand{Y, Nothing}".to_string()),
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ];
        let type_params = vec![
            TypeParam::new("Y".to_string()),
            TypeParam::new("T".to_string()),
        ];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &generic_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &batch_params,
                param_count: 4,
                vararg_param_index: Some(3),
                vararg_fixed_count: None,
                type_params: &type_params,
            },
        ];
        let actual = strings(&[
            "QuadGK.BatchIntegrand{Float64, Nothing, Vector{Float64}, Vector{Nothing}, typeof(f!)}",
            "Float64",
            "Float64",
        ]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn callable_value_candidates_prefer_diagonal_typevar_vararg_issue_8407() {
        let forwarding_params = vec![JuliaType::Any, JuliaType::Any];
        let diagonal_params = vec![JuliaType::Any, JuliaType::TypeVar("T".to_string(), None)];
        let type_params = vec![TypeParam::new("T".to_string())];
        let candidates = [
            CallableValueCandidate {
                idx: 1,
                param_types: &forwarding_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &[],
            },
            CallableValueCandidate {
                idx: 2,
                param_types: &diagonal_params,
                param_count: 2,
                vararg_param_index: Some(1),
                vararg_fixed_count: None,
                type_params: &type_params,
            },
        ];
        let actual = strings(&["Function", "Float64", "Float64"]);

        assert_eq!(
            resolve_callable_value_candidates(
                &StructHierarchy::new(),
                candidates,
                &actual,
                |actual, param| {
                    runtime_type_name_matches_param(
                        actual,
                        param,
                        |_| true,
                        |actual, expected| {
                            CoreType::from_julia_name(actual)
                                .is_subtype_of(&CoreType::from_julia_name(expected))
                        },
                    )
                },
                |actual, param| param.name() == actual
            )
            .map(|(idx, _)| idx),
            Some(2)
        );
    }

    #[test]
    fn julia_signature_reuses_implicit_typevar_bindings() {
        let same_type_params = vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ];

        assert!(julia_signature_match_with_bindings(
            &same_type_params,
            &[JuliaType::BigInt, JuliaType::BigInt],
            &[]
        )
        .is_some());
        assert!(julia_signature_match_with_bindings(
            &same_type_params,
            &[JuliaType::BigInt, JuliaType::Int64],
            &[]
        )
        .is_none());
    }

    #[test]
    fn julia_signature_keeps_anonymous_tuple_bounds_independent_issue_6251() {
        let broad_tuple = vec![JuliaType::TupleOf(vec![
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ])];
        let diagonal_tuple = vec![JuliaType::TupleOf(vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ])];
        let diagonal_type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];

        assert!(
            julia_signature_match_with_bindings(
                &broad_tuple,
                &[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::Float64
                ])],
                &[],
            )
            .is_some(),
            "Tuple{{<:Real,<:Real}} should match mixed real tuple elements independently"
        );
        assert!(
            julia_signature_match_with_bindings(
                &broad_tuple,
                &[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::String
                ])],
                &[],
            )
            .is_none(),
            "Tuple{{<:Real,<:Real}} must still reject non-Real elements"
        );
        assert!(
            julia_signature_match_with_bindings(
                &diagonal_tuple,
                &[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::Float64
                ])],
                &diagonal_type_params,
            )
            .is_none(),
            "Tuple{{T,T}} where T<:Real must still reject mixed element types"
        );
    }

    #[test]
    fn julia_signature_enforces_nested_diagonal_rule_issue_5050() {
        // nest(x::Vector{T}, y::T) where T: the element type of the vector and
        // the bare argument must share a single concrete `T`.
        let nested_params = vec![
            JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::TypeVar("T".to_string(), None),
        ];
        let type_params = vec![TypeParam::new("T".to_string())];

        // Vector{Int64} + Int64 -> T = Int64 consistently: accepted.
        assert!(julia_signature_match_with_bindings(
            &nested_params,
            &[
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Int64,
            ],
            &type_params,
        )
        .is_some());

        // Vector{Int64} + Float64 -> T would be both Int64 and Float64: rejected.
        assert!(julia_signature_match_with_bindings(
            &nested_params,
            &[
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Float64,
            ],
            &type_params,
        )
        .is_none());

        // Vector{Float64} + Int64: also rejected (order of conflict is symmetric).
        assert!(julia_signature_match_with_bindings(
            &nested_params,
            &[
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::Int64,
            ],
            &type_params,
        )
        .is_none());
    }

    #[test]
    fn core_tuple_cache_key_preserves_parametric_value_and_vararg_shape() {
        assert_eq!(
            core_tuple_signature_from_julia_types(&[
                JuliaType::Struct("Val{1}".to_string()),
                JuliaType::Struct("Array{Int64, 2}".to_string()),
                JuliaType::Struct("Tuple{Vararg{Int64, 3}}".to_string()),
            ]),
            CoreType::Tuple(vec![
                CoreType::from_julia_name("Val{1}"),
                CoreType::from_julia_name("Array{Int64, 2}"),
                CoreType::from_julia_name("Tuple{Vararg{Int64, 3}}"),
            ])
        );
    }

    #[test]
    fn typeof_array_pattern_binds_inner_typevars() {
        let type_params = vec![TypeParam::new("T".to_string())];
        let pattern = vec![JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "Array{T}".to_string(),
        )))];

        assert!(julia_signature_match_with_bindings(
            &pattern,
            &[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "Array{Int64}".to_string(),
            )))],
            &type_params,
        )
        .is_some());

        assert!(julia_signature_match_with_bindings(
            &pattern,
            &[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
                JuliaType::Float64,
            ))))],
            &type_params,
        )
        .is_none());
    }

    #[test]
    fn typeof_double_bound_enforces_lower_and_upper_invariantly() {
        // Type{T} where Integer<:T<:Real binds T invariantly, so both bounds
        // are enforced (Issue #5051).
        let type_params = vec![TypeParam::with_both_bounds(
            "T".to_string(),
            "Integer".to_string(),
            "Real".to_string(),
        )];
        let pattern = vec![JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )))];
        let matches = |arg: JuliaType| {
            julia_signature_match_with_bindings(
                &pattern,
                &[JuliaType::TypeOf(Box::new(arg))],
                &type_params,
            )
            .is_some()
        };

        // Within [Integer, Real]: matches.
        assert!(matches(JuliaType::Struct("Integer".to_string())));
        assert!(matches(JuliaType::Struct("Real".to_string())));
        // Below the lower bound (Integer <: Int64 is false): rejected.
        assert!(!matches(JuliaType::Int64));
        assert!(!matches(JuliaType::Float64));
        // Above the upper bound (Number <: Real is false): rejected.
        assert!(!matches(JuliaType::Struct("Number".to_string())));
    }

    #[test]
    fn typeof_lower_bound_only_enforced_invariantly() {
        // Type{T} where T>:Integer: T must be a supertype of Integer
        // (Integer <: T) (Issue #5051).
        let type_params = vec![TypeParam::with_lower_bound(
            "T".to_string(),
            "Integer".to_string(),
        )];
        let pattern = vec![JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "T".to_string(),
        )))];
        let matches = |arg: JuliaType| {
            julia_signature_match_with_bindings(
                &pattern,
                &[JuliaType::TypeOf(Box::new(arg))],
                &type_params,
            )
            .is_some()
        };

        assert!(matches(JuliaType::Struct("Integer".to_string())));
        assert!(matches(JuliaType::Struct("Real".to_string())));
        assert!(matches(JuliaType::Struct("Number".to_string())));
        // Int64 is not a supertype of Integer.
        assert!(!matches(JuliaType::Int64));
    }

    #[test]
    fn covariant_typevar_ignores_lower_bound() {
        // x::T where Integer<:T<:Real binds T covariantly; the lower bound does
        // not restrict matching, so Float64 (not a supertype of Integer) still
        // matches as long as it is <: Real (Issue #5051).
        let type_params = vec![TypeParam::with_both_bounds(
            "T".to_string(),
            "Integer".to_string(),
            "Real".to_string(),
        )];
        let pattern = vec![JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )];

        assert!(
            julia_signature_match_with_bindings(&pattern, &[JuliaType::Float64], &type_params,)
                .is_some()
        );
        assert!(
            julia_signature_match_with_bindings(&pattern, &[JuliaType::Int64], &type_params,)
                .is_some()
        );
        // Above the upper bound still rejected even covariantly.
        assert!(
            julia_signature_match_with_bindings(&pattern, &[JuliaType::String], &type_params,)
                .is_none()
        );
    }

    #[test]
    fn score_julia_signature_uses_coretype_exact_and_any_policies() {
        let int_score =
            score_julia_signature(&[JuliaType::Int64], &[JuliaType::Int64], &[], false, false)
                .expect("Int64 should match Int64");
        let any_score =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Int64], &[], false, false)
                .expect("Any should match Int64");

        assert!(int_score.score > any_score.score);

        let unknown_specific_score =
            score_julia_signature(&[JuliaType::Int64], &[JuliaType::Any], &[], false, false)
                .expect("specific param should still match Any for compile-time fallback");
        let unknown_any_score =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Any], &[], false, false)
                .expect("Any should match Any");

        assert!(unknown_any_score.score >= unknown_specific_score.score);
    }

    #[test]
    fn score_julia_signature_exact_uppercase_struct_beats_any_issue_5314() {
        let concrete = JuliaType::Struct("Q5314".to_string());
        let concrete_arg = concrete.clone();
        let concrete_score = score_julia_signature(
            std::slice::from_ref(&concrete),
            std::slice::from_ref(&concrete_arg),
            &[],
            false,
            false,
        )
        .expect("concrete struct should match itself");
        let any_score = score_julia_signature(
            &[JuliaType::Any],
            &[JuliaType::Struct("Q5314".to_string())],
            &[],
            false,
            false,
        )
        .expect("Any should match concrete struct");

        assert!(concrete_score.score > any_score.score);
    }

    #[test]
    fn score_julia_signature_demotes_non_exact_type_any_singleton_issue_4165() {
        let bare_type_score = score_julia_signature(
            &[JuliaType::Type],
            &[JuliaType::TypeOf(Box::new(JuliaType::Int64))],
            &[],
            false,
            false,
        )
        .expect("bare Type should match concrete type objects");
        let broad_any_score = score_julia_signature(
            &[JuliaType::TypeOf(Box::new(JuliaType::Any))],
            &[JuliaType::TypeOf(Box::new(JuliaType::Int64))],
            &[],
            false,
            false,
        )
        .expect("transitional Type{Any} broad match should still match");
        assert!(bare_type_score.score > broad_any_score.score);

        let exact_any_score = score_julia_signature(
            &[JuliaType::TypeOf(Box::new(JuliaType::Any))],
            &[JuliaType::TypeOf(Box::new(JuliaType::Any))],
            &[],
            false,
            false,
        )
        .expect("Type{Any} should exactly match Any");
        assert!(exact_any_score.score > bare_type_score.score);
    }

    #[test]
    fn score_julia_signature_prefers_exact_type_any_over_typevar_issue_4574() {
        let exact_any_score = score_julia_signature(
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::Tuple,
            ],
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ],
            &[],
            false,
            false,
        )
        .expect("Type{Any}, Tuple should match Any and tuple dims");
        let generic_typevar_score = score_julia_signature(
            &[
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                JuliaType::Tuple,
            ],
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ],
            &[TypeParam::new("T".to_string())],
            false,
            false,
        )
        .expect("Type{T}, Tuple should also match Any and tuple dims");

        assert!(exact_any_score.score > generic_typevar_score.score);
    }

    #[test]
    fn score_julia_signature_prefers_typevar_over_non_exact_type_any_issue_4577() {
        let broad_any_score = score_julia_signature(
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Any)),
                JuliaType::Tuple,
            ],
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Symbol)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ],
            &[],
            false,
            false,
        )
        .expect("Type{Any}, Tuple should still transitionally match Symbol and tuple dims");
        let generic_typevar_score = score_julia_signature(
            &[
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                JuliaType::Tuple,
            ],
            &[
                JuliaType::TypeOf(Box::new(JuliaType::Symbol)),
                JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            ],
            &[TypeParam::new("T".to_string())],
            false,
            false,
        )
        .expect("Type{T}, Tuple should match Symbol and tuple dims");

        assert!(generic_typevar_score.score > broad_any_score.score);
    }

    #[test]
    fn type_object_actual_does_not_match_value_level_parametric_pattern_issue_6251() {
        let actual = [JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
            JuliaType::Int64,
        ))))];
        let type_params = [TypeParam::new("T".to_string())];

        assert!(
            julia_signature_match_with_bindings(
                &[JuliaType::Struct("Array{T, 1}".to_string())],
                &actual,
                &type_params,
            )
            .is_none(),
            "a type object argument must not satisfy a value-level Array{{T,1}} parameter"
        );
        assert!(
            julia_signature_match_with_bindings(
                &[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                    "LinRange{T}".to_string(),
                )))],
                &actual,
                &type_params,
            )
            .is_none(),
            "Type{{LinRange{{T}}}} must not satisfy Type{{Vector{{Int64}}}} via range projection"
        );
        assert!(
            julia_signature_match_with_bindings(
                &[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
                    JuliaType::TypeVar("T".to_string(), None),
                ))))],
                &actual,
                &type_params,
            )
            .is_some(),
            "the same type object should still satisfy Type{{Vector{{T}}}}"
        );
    }

    #[test]
    fn bounded_typevar_param_outranks_untyped_any_issue_5375() {
        // `h(x::T) where {T<:Number}` must be ranked strictly more specific than
        // the untyped fallback `h(x)` when called as `h(5)`. Previously the
        // bounded type variable scored 0 (the bound was ignored) while the
        // untyped `Any` param earned the `type_reuse_bonus`, so the fallback won.
        let bounded = score_julia_signature(
            &[JuliaType::TypeVar(
                "T".to_string(),
                Some("Number".to_string()),
            )],
            &[JuliaType::Int64],
            &[TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            false,
            false,
        )
        .expect("bounded T<:Number must match an Int64 argument");
        let untyped =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Int64], &[], false, false)
                .expect("untyped Any must match an Int64 argument");
        assert!(
            bounded.score > untyped.score,
            "bounded T<:Number ({}) must outrank untyped Any ({})",
            bounded.score,
            untyped.score
        );
    }

    #[test]
    fn bounded_typevar_param_loses_to_tighter_concrete_param_issue_5375() {
        // The bound must not be over-weighted: a concrete `Int64` parameter is a
        // subtype of `Number`, so it stays at least as specific as `T<:Number`.
        let bounded = score_julia_signature(
            &[JuliaType::TypeVar(
                "T".to_string(),
                Some("Number".to_string()),
            )],
            &[JuliaType::Int64],
            &[TypeParam::with_upper_bound(
                "T".to_string(),
                "Number".to_string(),
            )],
            false,
            false,
        )
        .expect("bounded T<:Number must match an Int64 argument");
        let concrete =
            score_julia_signature(&[JuliaType::Int64], &[JuliaType::Int64], &[], false, false)
                .expect("Int64 must match an Int64 argument");
        assert!(
            concrete.score >= bounded.score,
            "concrete Int64 ({}) must stay at least as specific as T<:Number ({})",
            concrete.score,
            bounded.score
        );
    }

    #[test]
    fn bounded_typevar_any_bound_does_not_outrank_untyped_issue_5375() {
        // `T<:Any` is equivalent to an unbounded `T` (≡ `Any`), so it must not be
        // scored above an untyped fallback parameter (review hardening for #5375).
        let any_bound = score_julia_signature(
            &[JuliaType::TypeVar("T".to_string(), Some("Any".to_string()))],
            &[JuliaType::Int64],
            &[TypeParam::with_upper_bound(
                "T".to_string(),
                "Any".to_string(),
            )],
            false,
            false,
        )
        .expect("T<:Any must match an Int64 argument");
        let untyped =
            score_julia_signature(&[JuliaType::Any], &[JuliaType::Int64], &[], false, false)
                .expect("untyped Any must match an Int64 argument");
        assert!(
            any_bound.score <= untyped.score,
            "T<:Any ({}) must not outrank untyped Any ({})",
            any_bound.score,
            untyped.score
        );
    }

    #[test]
    fn user_abstract_with_builtin_parent_outranks_parent_issue_5582() {
        let abstract_irrational =
            JuliaType::AbstractUser("AbstractIrrational".to_string(), Some("Real".to_string()));
        assert!(
            value_param_base_specificity(&abstract_irrational)
                > value_param_base_specificity(&JuliaType::Real),
            "AbstractIrrational <: Real must score above the Real fallback"
        );

        let any_rooted = JuliaType::AbstractUser("MyAbstract".to_string(), Some("Any".to_string()));
        assert_eq!(
            value_param_base_specificity(&any_rooted),
            1,
            "Any-rooted user abstracts keep the previous flat abstract score"
        );
    }

    /// Issue #6594: pin the *exact* `value_param_base_specificity` scores for the
    /// full `AbstractUser` parent matrix BEFORE migrating the legacy
    /// `JuliaType::from_name(parent)` string parse to structured `CoreType`
    /// matching. The structured replacement must reproduce every value here.
    #[test]
    fn user_abstract_base_specificity_parent_matrix_issue_6594() {
        let cases: &[(Option<&str>, u32)] = &[
            // No declared parent: the structural `CoreType::AbstractUser` floor.
            (None, 1),
            // `Any` parent collapses to the flat abstract floor (≡ unbounded).
            (Some("Any"), 1),
            // Built-in abstract parents add 1 to the parent's CoreType specificity.
            (Some("Number"), 2),  // Number spec 1 -> 2
            (Some("Real"), 3),    // Real spec 2 -> 3
            (Some("Integer"), 4), // Integer spec 3 -> 4
            // Built-in abstract container parents (resolve via `from_name`).
            (Some("AbstractVector"), 2), // AbstractVector spec 1 -> 2
            (Some("AbstractArray"), 2),  // AbstractArray spec 1 -> 2
            // Unknown / user-abstract parent names are NOT resolvable by
            // `from_name`, so the legacy path falls through to the flat floor.
            (Some("Animal"), 1),
            (Some("MyOtherAbstract"), 1),
        ];
        for (parent, expected) in cases {
            let ty =
                JuliaType::AbstractUser("MyAbstract".to_string(), parent.map(|p| p.to_string()));
            assert_eq!(
                value_param_base_specificity(&ty),
                *expected,
                "AbstractUser parent {parent:?} must score {expected}"
            );
        }
    }

    /// Issue #6594: pin the exact-name tier-4 bridge that the legacy rendered
    /// parse provided for `AbstractUser`/`Module` candidate slots. A structured
    /// `AbstractUser`/`Module` slot must score 4 (exact) against the rendered
    /// runtime name's parsed `Named(_)` image, while a child user struct still
    /// passes the structured signature gate through the shared subtype engine.
    #[test]
    fn user_abstract_and_module_keep_exact_name_tier4_issue_6594() {
        // AbstractUser: structured slot scores tier-4 against the rendered name.
        let user_abstract = JuliaType::AbstractUser("Animal".to_string(), Some("Any".to_string()));
        let structural = runtime_candidate_core_type(&user_abstract, &user_abstract.to_string());
        let rendered = CoreType::from_julia_name(&user_abstract.to_string());
        assert!(matches!(structural, CoreType::AbstractUser { .. }));
        assert_eq!(
            structural.dispatch_pattern_score(&rendered),
            4,
            "AbstractUser slot must keep the exact-name tier-4 bridge"
        );

        // Child user structs still pass the structured signature gate.
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Dog", Some("Animal".to_string()), vec![]);
        hierarchy.insert("Animal", Some("Any".to_string()), vec![]);
        let dog = CoreType::from_julia_name("Dog");
        assert!(
            CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(&dog, &structural),
            "Dog <: Animal must hold through the shared subtype engine"
        );
        assert!(
            !CoreSubtypeEngine::with_hierarchy(&hierarchy)
                .is_subtype(&CoreType::from_julia_name("Int64"), &structural),
            "Int64 must not be admitted by the AbstractUser slot"
        );

        // Module: structured slot scores tier-4 against the rendered name.
        let module = JuliaType::Module;
        let module_core = runtime_candidate_core_type(&module, &module.to_string());
        let module_rendered = CoreType::from_julia_name(&module.to_string());
        assert!(matches!(module_core, CoreType::Module(_)));
        assert_eq!(
            module_core.dispatch_pattern_score(&module_rendered),
            4,
            "Module slot must keep the exact-name tier-4 bridge"
        );
    }

    /// Issue #6502 slice 2: for string-faithful signatures the structured
    /// resolver returns the same `(index, score)` as the string resolver.
    #[test]
    fn structured_resolver_matches_string_resolver_scores_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let string_candidates = [
            (1usize, vec!["Real"]),
            (2usize, vec!["Rational"]),
            (3usize, vec!["Rational{T}"]),
            (4usize, vec!["Rational{Int64}"]),
        ];
        let core_slots: Vec<(usize, CoreType)> = string_candidates
            .iter()
            .map(|(idx, sig)| (*idx, CoreType::from_julia_name(sig[0])))
            .collect();

        let mut subtype = |actual: &str, expected: &str| {
            CoreType::from_julia_name(actual).is_subtype_of(&CoreType::from_julia_name(expected))
        };
        let string_result = resolve_runtime_type_pattern_candidates(
            string_candidates
                .iter()
                .map(|(idx, sig)| (*idx, sig.clone())),
            &["Rational{Int64}"],
            &mut subtype,
        );

        let actual_cores = [CoreType::from_julia_name("Rational{Int64}")];
        let core_result = resolve_runtime_core_signature_candidates(
            &hierarchy,
            core_slots.iter().map(|(idx, slot)| RuntimeCoreCandidate {
                idx: *idx,
                slots: [slot],
                signature: None,
            }),
            &actual_cores,
            |actual, expected| actual.is_subtype_of(expected),
        );

        assert_eq!(string_result, Some((4, 4)));
        assert_eq!(core_result, string_result);
    }

    /// Issue #6502 residual slice: the structured fallback resolver keeps the
    /// legacy same-family tier for native/legacy sentinel names without
    /// returning to string-encoded candidate matching.
    #[test]
    fn structured_slice_resolver_uses_family_fallback_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let sentinel = CoreType::Named("Base.Generator".to_string());
        let catch_all = CoreType::Any;
        let actual = CoreType::Struct {
            name: "Base.Generator".to_string(),
            params: vec![CoreType::Any],
        };
        let actual_cores = [actual];

        let result = resolve_runtime_core_signature_slice_candidates_with_family_fallback(
            &hierarchy,
            [
                RuntimeCoreSliceCandidate {
                    idx: usize::MAX,
                    slots: std::slice::from_ref(&sentinel),
                    signature: None,
                },
                RuntimeCoreSliceCandidate {
                    idx: 2,
                    slots: std::slice::from_ref(&catch_all),
                    signature: None,
                },
            ],
            &actual_cores,
            // Issue #6593: structured family match via the `core_signature`
            // accessor, not a `to_julia_name()` string round-trip.
            |actual, expected| match (actual.nominal_family_name(), expected.nominal_family_name())
            {
                (Some(actual_family), Some(expected_family)) => actual_family == expected_family,
                _ => false,
            },
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );

        assert_eq!(result, Some((usize::MAX, 2)));
    }

    /// Issue #6502: family fallback must not admit parametric candidates that
    /// the old string fallback intentionally rejected via
    /// `core_type_allows_family_fallback`.
    #[test]
    fn structured_slice_family_fallback_rejects_parametric_expected_issue_6502() {
        let hierarchy = StructHierarchy::new();
        let expected = CoreType::Struct {
            name: "Box".to_string(),
            params: vec![CoreType::Any],
        };
        let actual = CoreType::Struct {
            name: "Box".to_string(),
            params: vec![CoreType::from_julia_name("String")],
        };
        let actual_cores = [actual];

        let result = resolve_runtime_core_signature_slice_candidates_with_family_fallback(
            &hierarchy,
            [RuntimeCoreSliceCandidate {
                idx: 1,
                slots: std::slice::from_ref(&expected),
                signature: None,
            }],
            &actual_cores,
            |_, _| true,
            |_, _| false,
        );

        assert_eq!(result, None);
    }

    /// Issue #6536: `where`-clause bounds embedded into parametric slots are
    /// enforced — `Wrap{T} where T<:Real` must reject `Wrap{String}` and keep
    /// the tier-3 parametric score for `Wrap{Int64}`.
    #[test]
    fn structured_resolver_enforces_embedded_bounds_issue_6536() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )];
        let raw = CoreType::from_julia_name("Wrap{T}");
        let bounded = embed_type_param_bounds(raw.clone(), &type_params);
        assert_ne!(bounded, raw, "bound must be embedded into the typevar");

        let signature = runtime_core_signature(std::slice::from_ref(&bounded), &type_params);
        let generic_slot = CoreType::from_julia_name("Wrap{S}");

        let run = |actual_name: &str| {
            let actual_cores = [CoreType::from_julia_name(actual_name)];
            resolve_runtime_core_signature_candidates(
                &hierarchy,
                [
                    RuntimeCoreCandidate {
                        idx: 1,
                        slots: [&bounded],
                        signature: Some(&signature),
                    },
                    RuntimeCoreCandidate {
                        idx: 2,
                        slots: [&generic_slot],
                        signature: None,
                    },
                ],
                &actual_cores,
                |_, _| false,
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Wrap{Int64}"), Some(1));
        assert_eq!(run("Wrap{String}"), Some(2));
    }

    /// Issue #5137/#6536: multi-letter `where` names such as `MI` are not
    /// parsed as type variables without method context, but runtime candidate
    /// signature building has that context through `type_params` (Issue #5915
    /// cross-credit).
    #[test]
    fn embed_type_param_bounds_recovers_named_multi_letter_typevars_issue_5137() {
        let hierarchy = StructHierarchy::new();
        let type_params = [
            TypeParam::new("T".to_string()),
            TypeParam::new("P".to_string()),
            TypeParam::new("MI".to_string()),
        ];
        let slot = embed_type_param_bounds(
            CoreType::from_julia_name("ReshapedArray{T, 1, P, MI}"),
            &type_params,
        );
        let CoreType::Struct { params, .. } = &slot else {
            panic!("ReshapedArray slot should stay a struct");
        };
        assert!(matches!(params.get(3), Some(CoreType::TypeVar(var)) if var.name == "MI"));

        let signature = runtime_core_signature(std::slice::from_ref(&slot), &type_params);
        let actual = [CoreType::from_julia_name(
            "ReshapedArray{Int64, 1, SubArray{Int64, 2, Matrix{Int64}, Tuple{UnitRange{Int64}, UnitRange{Int64}}, false}, Tuple{}}",
        )];

        let result = resolve_runtime_core_signature_candidates(
            &hierarchy,
            [RuntimeCoreCandidate {
                idx: 1,
                slots: [&slot],
                signature: Some(&signature),
            }],
            &actual,
            |actual, expected| {
                CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
            },
        );

        assert_eq!(result.map(|(idx, _)| idx), Some(1));
    }

    /// Issue #6536: the `core_signature` gate enforces cross-slot typevar
    /// binding consistency — `(Holder{T}, Holder{T}) where T` must reject
    /// `(Holder{Int64}, Holder{String})`.
    #[test]
    fn structured_resolver_enforces_cross_slot_bindings_issue_6536() {
        let hierarchy = StructHierarchy::new();
        let type_params = [TypeParam::new("T".to_string())];
        let slot = embed_type_param_bounds(CoreType::from_julia_name("Holder{T}"), &type_params);
        let signature = runtime_core_signature(&[slot.clone(), slot.clone()], &type_params);
        let bare = CoreType::from_julia_name("Holder");

        let run = |left: &str, right: &str| {
            let actual_cores = [
                CoreType::from_julia_name(left),
                CoreType::from_julia_name(right),
            ];
            resolve_runtime_core_signature_candidates(
                &hierarchy,
                [
                    RuntimeCoreCandidate {
                        idx: 1,
                        slots: [&slot, &slot],
                        signature: Some(&signature),
                    },
                    RuntimeCoreCandidate {
                        idx: 2,
                        slots: [&bare, &bare],
                        signature: None,
                    },
                ],
                &actual_cores,
                // Mirror the VM's `check_subtype_core` fallback (the engine
                // admits bare `Named` family patterns at tier 1).
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Holder{Int64}", "Holder{Int64}"), Some(1));
        assert_eq!(run("Holder{Int64}", "Holder{String}"), Some(2));
    }

    /// Issue #6502/#6536: user-abstract bounds keep the structural tier when
    /// the hierarchy resolves them — `Box{T} where T<:Animal` scores tier 3
    /// for `Box{Dog}` (beating the bare `Box` tier-2 catch-all) and rejects
    /// `Box{Int64}`.
    #[test]
    fn structured_resolver_resolves_user_bounds_through_hierarchy_issue_6536() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Animal", Some("Any".to_string()), Vec::new());
        hierarchy.insert("Dog", Some("Animal".to_string()), Vec::new());
        hierarchy.insert("Box", Some("Any".to_string()), vec!["T".to_string()]);

        let type_params = [TypeParam::with_upper_bound(
            "T".to_string(),
            "Animal".to_string(),
        )];
        let slot = embed_type_param_bounds(CoreType::from_julia_name("Box{T}"), &type_params);
        let signature = runtime_core_signature(&[slot.clone(), slot.clone()], &type_params);
        let bare = CoreType::from_julia_name("Box");

        let run = |name: &str| {
            let actual = CoreType::from_julia_name(name);
            let actual_cores = [actual.clone(), actual];
            resolve_runtime_core_signature_candidates(
                &hierarchy,
                [
                    RuntimeCoreCandidate {
                        idx: 1,
                        slots: [&slot, &slot],
                        signature: Some(&signature),
                    },
                    RuntimeCoreCandidate {
                        idx: 2,
                        slots: [&bare, &bare],
                        signature: None,
                    },
                ],
                &actual_cores,
                // Mirror the VM's `check_subtype_core` fallback.
                |actual, expected| {
                    CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(actual, expected)
                },
            )
            .map(|(idx, _)| idx)
        };

        assert_eq!(run("Box{Dog}"), Some(1), "tier-3 bounded beats bare tier-2");
        assert_eq!(run("Box{Int64}"), Some(2), "bound rejects non-Animal");
    }

    /// Issue #6502 residual slice: runtime candidate slots are projected from
    /// `JuliaType` structurally, while `CoreType` keeps the exact-name and
    /// subtype behavior that the old rendered-name parse provided for
    /// `AbstractUser` and `Module`.
    #[test]
    fn runtime_candidate_core_type_replaces_legacy_parse_issue_6502() {
        // Faithful shapes: structural == parsed.
        for jt in [
            JuliaType::Int64,
            JuliaType::Real,
            JuliaType::String,
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
            JuliaType::Struct("Wrap{T}".to_string()),
            JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::Nothing]),
        ] {
            let rendered = jt.to_string();
            assert_eq!(
                CoreType::from(&jt),
                CoreType::from_julia_name(&rendered),
                "expected structural == parsed for {rendered}"
            );
        }

        // User abstract annotations diverge from rendered parsing, but runtime
        // candidates now keep the structured `AbstractUser` image and preserve
        // the old exact-name tier against rendered runtime names via CoreType.
        let user_abstract = JuliaType::AbstractUser("Animal".to_string(), Some("Any".to_string()));
        let rendered = user_abstract.to_string();
        let structural = CoreType::from(&user_abstract);
        let parsed = CoreType::from_julia_name(&rendered);
        assert_ne!(structural, parsed);
        assert_eq!(
            runtime_candidate_core_type(&user_abstract, &rendered),
            structural
        );
        assert_eq!(structural.dispatch_pattern_score(&parsed), 4);

        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Dog", Some("Animal".to_string()), vec![]);
        hierarchy.insert("Animal", Some("Any".to_string()), vec![]);
        let dog = CoreType::from_julia_name("Dog");
        assert!(
            CoreSubtypeEngine::with_hierarchy(&hierarchy).is_subtype(&dog, &structural),
            "child user structs still pass the structured AbstractUser signature gate"
        );

        // Module has the same divergent shape: declared `JuliaType::Module`
        // becomes `CoreType::Module("Module")`, while rendered runtime type
        // names parse as `Named("Module")`. Keep it structural and bridge the
        // exact runtime annotation match in CoreType.
        let module = JuliaType::Module;
        let rendered = module.to_string();
        let structural = CoreType::from(&module);
        let parsed = CoreType::from_julia_name(&rendered);
        assert_ne!(structural, parsed);
        assert_eq!(runtime_candidate_core_type(&module, &rendered), structural);
        assert_eq!(structural.dispatch_pattern_score(&parsed), 4);
        assert!(CoreSubtypeEngine::new().is_subtype(&parsed, &structural));
    }
}
