//! Shared method-specificity (diagonal dispatch / vararg expansion) logic.
//!
//! Issue #6331 — these predicates were previously duplicated nearly verbatim
//! between the compile-time method table (`compile/method_table.rs`) and the
//! runtime dispatcher (`vm/mod.rs`, as `runtime_`-prefixed free functions).
//! Upstream Julia keeps specificity in a single implementation
//! (`jl_type_morespecific` in `julia/src/subtype.c`, called from `gf.c` for
//! both method insertion and runtime ambiguity checks); this module mirrors
//! that single-source structure for our subset.
//!
//! The functions take the common denominator of both callers — plain
//! `&[JuliaType]` parameter/argument lists plus `&[TypeParam]` where-clause
//! parameters — so each side keeps only a thin adapter (`&MethodSig` on the
//! compile side, `&FunctionInfo` / expanded candidate params on the runtime
//! side).

use std::collections::HashMap;

use crate::types::{JuliaType, StructHierarchy, TypeParam};

use super::{CoreAbstract, CoreType, CoreTypeVar, CoreValueParam};

// ---------------------------------------------------------------------------
// Subtype helpers shared by every specificity family
// ---------------------------------------------------------------------------

/// `left <: right` through the shared subtype engine.
pub(crate) fn julia_type_subtypes(
    left: &JuliaType,
    right: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    CoreType::from(left).is_subtype_of_with_hierarchy(&CoreType::from(right), hierarchy)
}

/// `left <: right` but not `right <: left` — the strict half-order used by the
/// diagonal dominance predicates (Issue #6336: bounds are compared as
/// structured `CoreType`s, never as raw type-name strings).
fn core_strictly_subtypes(left: &CoreType, right: &CoreType, hierarchy: &StructHierarchy) -> bool {
    left.is_subtype_of_with_hierarchy(right, hierarchy)
        && !right.is_subtype_of_with_hierarchy(left, hierarchy)
}

// ---------------------------------------------------------------------------
// `where` type-parameter helpers
// ---------------------------------------------------------------------------

/// Strip a `<:`/`>:` bound spelled inside the parameter name (`"T<:Real"` → `"T"`).
pub(crate) fn type_param_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

/// Normalize a bound string to its outermost upper-bound type name
/// (`"S<:Number"` → `"Number"`).
fn upper_bound_type_name(bound: &str) -> &str {
    bound
        .rsplit_once("<:")
        .map_or(bound, |(_, upper)| upper)
        .trim()
}

/// Normalized, non-degenerate upper bound (`None` for empty or bare `"<:"`).
pub(crate) fn usable_upper_bound(bound: Option<&str>) -> Option<&str> {
    let normalized = upper_bound_type_name(bound?);
    (!normalized.is_empty() && normalized != "<:").then_some(normalized)
}

/// Resolve a `where` parameter's upper-bound type name, falling back to a
/// bound spelled inside the parameter name itself (`"T<:Real"`).
///
/// Unified form (Issue #6331): the runtime copy normalized nested `<:` and
/// rejected degenerate bounds while the compile copy returned the raw string;
/// both agree on well-formed inputs and the defensive variant is kept. The
/// name fallback is gated on an actual `<:` so an unbounded parameter (`"T"`)
/// yields `None` (the compile behavior) instead of its own name (the runtime
/// quirk, observable only as a no-op `JuliaType::from_name` lookup).
pub(crate) fn type_param_upper_bound(type_param: &TypeParam) -> Option<&str> {
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

/// Structured upper bound of a `where` parameter (`Any` when unbounded).
///
/// Issue #6336: the bound is resolved once, at the pattern-construction
/// boundary, through the central `CoreType` name bridge — the diagonal
/// dominance predicates below then compare structured `CoreType`s instead of
/// threading raw `&str` bound names through dispatch.
pub(crate) fn type_param_upper_bound_core(type_param: &TypeParam) -> CoreType {
    type_param_upper_bound(type_param).map_or(CoreType::Any, CoreType::from_julia_name)
}

/// Find a `where` parameter by base name.
pub(crate) fn find_type_param<'a>(
    type_params: &'a [TypeParam],
    var_name: &str,
) -> Option<&'a TypeParam> {
    type_params
        .iter()
        .find(|tp| type_param_base_name(&tp.name) == var_name)
}

/// Structured upper bound of a `where` core type variable (`Any` when
/// unbounded), mirroring [`type_param_upper_bound_core`] for the
/// `core_signature` projection (Issue #6495 stage 5): the converted bound when
/// present, else a bound spelled inside the variable name itself (legacy
/// `"T<:Real"` names survive verbatim in `CoreTypeVar::name`), else `Any`.
/// `CoreTypeVar::upper_bound` is produced by the same normalization as
/// [`usable_upper_bound`] (`core_upper_bound_from_name`), so the two helpers
/// resolve identical bounds for round-tripping signatures.
pub(crate) fn core_type_var_upper_bound(var: &CoreTypeVar) -> CoreType {
    if let Some(bound) = &var.upper_bound {
        return (**bound).clone();
    }
    var.name
        .contains("<:")
        .then(|| usable_upper_bound(Some(&var.name)).map(CoreType::from_julia_name))
        .flatten()
        .unwrap_or(CoreType::Any)
}

/// Find a `where` core type variable by base name (mirrors
/// [`find_type_param`]).
pub(crate) fn find_core_type_var<'a>(
    type_vars: &'a [CoreTypeVar],
    var_name: &str,
) -> Option<&'a CoreTypeVar> {
    type_vars
        .iter()
        .find(|var| type_param_base_name(&var.name) == var_name)
}

// ---------------------------------------------------------------------------
// Tuple vararg expansion (`f(t::Tuple{Int, Vararg{Int}})` style patterns)
// ---------------------------------------------------------------------------

/// A tuple-vararg pattern expanded to a concrete call-site length: the
/// expanded element list plus the declared vararg element type (kept separate
/// because expansion erases it from per-slot comparison — Issue #6218).
///
/// Issue #6495 (stage 5): the expansion is `CoreType`-native — the comparison
/// predicates below previously bridged every slot through `CoreType::from`
/// per comparison, so storing the bridged forms once is decision-identical.
pub(crate) struct TupleVarargExpansion {
    pub expanded: Vec<CoreType>,
    pub vararg_element: CoreType,
}

/// Expand a `Tuple{lead..., Vararg{T}}` pattern across `actual_len` elements.
/// Returns `None` when the pattern has no trailing unbounded vararg or the
/// actual tuple is shorter than the fixed lead.
///
/// Legacy-spelling adapter: bridges the elements through the canonical
/// `CoreType::from` and delegates to [`core_expand_tuple_vararg_pattern_for_len`].
/// The trailing-vararg detection is image-equivalent: the legacy
/// `unbounded_vararg_element` accepted exactly the single-argument
/// `Struct("Vararg{T}")` spellings, which are exactly the `JuliaType`s whose
/// core image is `CoreType::Vararg` (`Vararg{T, N}` images as `VarargLen` and
/// stays rejected).
pub(crate) fn expand_tuple_vararg_pattern_for_len(
    pattern_elems: &[JuliaType],
    actual_len: usize,
) -> Option<TupleVarargExpansion> {
    let cores: Vec<CoreType> = pattern_elems.iter().map(CoreType::from).collect();
    core_expand_tuple_vararg_pattern_for_len(&cores, actual_len)
}

/// `CoreType`-native form of [`expand_tuple_vararg_pattern_for_len`]
/// (Issue #6495 stage 5): consumed by the compile-time dominance pre-checks
/// directly from the `core_signature` projections.
pub(crate) fn core_expand_tuple_vararg_pattern_for_len(
    pattern_elems: &[CoreType],
    actual_len: usize,
) -> Option<TupleVarargExpansion> {
    let CoreType::Vararg(vararg_element) = pattern_elems.last()? else {
        return None;
    };
    let vararg_element = (**vararg_element).clone();
    let lead_count = pattern_elems.len() - 1;
    if actual_len < lead_count {
        return None;
    }

    let mut expanded = pattern_elems[..lead_count].to_vec();
    for _ in lead_count..actual_len {
        expanded.push(vararg_element.clone());
    }
    Some(TupleVarargExpansion {
        expanded,
        vararg_element,
    })
}

/// Whether `candidate` strictly dominates `other` after both were expanded to
/// the same call-site length: every expanded slot is a subtype, and either the
/// expanded tuple or the declared vararg element is a *strict* subtype.
pub(crate) fn tuple_vararg_pattern_dominates(
    candidate: &TupleVarargExpansion,
    other: &TupleVarargExpansion,
    hierarchy: &StructHierarchy,
) -> bool {
    let candidate_tuple = CoreType::Tuple(candidate.expanded.clone());
    let other_tuple = CoreType::Tuple(other.expanded.clone());
    let candidate_vararg = &candidate.vararg_element;
    let other_vararg = &other.vararg_element;

    candidate_tuple.is_subtype_of_with_hierarchy(&other_tuple, hierarchy)
        && candidate_vararg.is_subtype_of_with_hierarchy(other_vararg, hierarchy)
        && (candidate_tuple.strict_subtype_dominates_with_hierarchy(&other_tuple, hierarchy)
            || candidate_vararg.strict_subtype_dominates_with_hierarchy(other_vararg, hierarchy))
}

/// Whether two expanded tuple-vararg patterns are mutually incomparable in a
/// conflicting way (each side wins a different axis), i.e. the call is
/// ambiguous (Issue #6220).
pub(crate) fn tuple_vararg_patterns_conflict(
    left: &TupleVarargExpansion,
    right: &TupleVarargExpansion,
    hierarchy: &StructHierarchy,
) -> bool {
    if tuple_vararg_pattern_dominates(left, right, hierarchy)
        || tuple_vararg_pattern_dominates(right, left, hierarchy)
    {
        return false;
    }

    let left_vararg_more_specific = left
        .vararg_element
        .strict_subtype_dominates_with_hierarchy(&right.vararg_element, hierarchy);
    let right_vararg_more_specific = right
        .vararg_element
        .strict_subtype_dominates_with_hierarchy(&left.vararg_element, hierarchy);

    (left_vararg_more_specific
        && tuple_has_strict_slot_advantage(&right.expanded, &left.expanded, hierarchy))
        || (right_vararg_more_specific
            && tuple_has_strict_slot_advantage(&left.expanded, &right.expanded, hierarchy))
}

fn tuple_has_strict_slot_advantage(
    candidate: &[CoreType],
    other: &[CoreType],
    hierarchy: &StructHierarchy,
) -> bool {
    candidate.len() == other.len()
        && candidate
            .iter()
            .zip(other.iter())
            .any(|(candidate, other)| {
                candidate.strict_subtype_dominates_with_hierarchy(other, hierarchy)
            })
}

// ---------------------------------------------------------------------------
// Tuple diagonal (`f(t::Tuple{T, T}) where T` style patterns)
// ---------------------------------------------------------------------------

/// A single-tuple-argument diagonal pattern `Tuple{T, T, ...} where T<:Bound`.
pub(crate) struct TupleDiagonalPattern {
    pub upper_bound: CoreType,
    pub arity: usize,
}

/// Detect a diagonal repeated-typevar tuple pattern in a single-parameter
/// method signature.
pub(crate) fn repeated_tuple_typevar_pattern(
    param_types: &[JuliaType],
    type_params: &[TypeParam],
) -> Option<TupleDiagonalPattern> {
    let [JuliaType::TupleOf(pattern_elems)] = param_types else {
        return None;
    };
    if pattern_elems.len() < 2 {
        return None;
    }

    for type_param in type_params {
        let var_name = type_param_base_name(&type_param.name);
        if pattern_elems
            .iter()
            .all(|elem| matches!(elem, JuliaType::TypeVar(name, _) if name == var_name))
        {
            return Some(TupleDiagonalPattern {
                upper_bound: type_param_upper_bound_core(type_param),
                arity: pattern_elems.len(),
            });
        }
    }
    None
}

/// Whether the actual tuple element types satisfy the diagonal constraint
/// (all equal, all under the bound).
pub(crate) fn actual_tuple_satisfies_diagonal_pattern(
    actual_elems: &[JuliaType],
    pattern: &TupleDiagonalPattern,
    hierarchy: &StructHierarchy,
) -> bool {
    if actual_elems.len() != pattern.arity {
        return false;
    }
    let Some(first) = actual_elems.first() else {
        return false;
    };
    actual_elems.iter().all(|elem| {
        elem.type_eq(first)
            && CoreType::from(elem).is_subtype_of_with_hierarchy(&pattern.upper_bound, hierarchy)
    })
}

/// Whether the diagonal candidate strictly dominates another single-tuple
/// method (`other_param_types` / `other_type_params`).
pub(crate) fn tuple_diagonal_candidate_dominates_other(
    other_param_types: &[JuliaType],
    other_type_params: &[TypeParam],
    pattern: &TupleDiagonalPattern,
    hierarchy: &StructHierarchy,
) -> bool {
    let [JuliaType::TupleOf(other_elems)] = other_param_types else {
        return false;
    };
    if other_elems.len() != pattern.arity {
        return false;
    }
    if let Some(other_pattern) =
        repeated_tuple_typevar_pattern(other_param_types, other_type_params)
    {
        return core_strictly_subtypes(&pattern.upper_bound, &other_pattern.upper_bound, hierarchy);
    }

    let candidate_bound = &pattern.upper_bound;
    let mut strict = false;
    for elem in other_elems {
        let Some(other_bound) = tuple_slot_upper_bound_core(other_type_params, elem) else {
            return false;
        };
        if !candidate_bound.is_subtype_of_with_hierarchy(&other_bound, hierarchy) {
            return false;
        }
        if !other_bound.is_subtype_of_with_hierarchy(candidate_bound, hierarchy)
            || matches!(elem, JuliaType::Any | JuliaType::TypeVar(_, _))
        {
            strict = true;
        }
    }
    strict
}

fn tuple_slot_upper_bound_core(type_params: &[TypeParam], elem: &JuliaType) -> Option<CoreType> {
    match elem {
        JuliaType::TypeVar(_, Some(bound)) => Some(CoreType::from_julia_name(bound)),
        JuliaType::TypeVar(name, None) => Some(
            find_type_param(type_params, name)
                .map(type_param_upper_bound_core)
                .unwrap_or(CoreType::Any),
        ),
        _ => Some(CoreType::from(elem)),
    }
}

/// `CoreType`-native form of [`repeated_tuple_typevar_pattern`] (Issue #6495
/// stage 5), consuming the `core_signature` projections. Arm-for-arm port:
/// the element check keeps the legacy raw-name comparison (`TypeVar` element
/// name against the *base* `where` variable name).
pub(crate) fn core_repeated_tuple_typevar_pattern(
    param_types: &[CoreType],
    type_vars: &[CoreTypeVar],
) -> Option<TupleDiagonalPattern> {
    let [CoreType::Tuple(pattern_elems)] = param_types else {
        return None;
    };
    if pattern_elems.len() < 2 {
        return None;
    }

    for type_var in type_vars {
        let var_name = type_param_base_name(&type_var.name);
        if pattern_elems
            .iter()
            .all(|elem| matches!(elem, CoreType::TypeVar(v) if v.name == var_name))
        {
            return Some(TupleDiagonalPattern {
                upper_bound: core_type_var_upper_bound(type_var),
                arity: pattern_elems.len(),
            });
        }
    }
    None
}

/// `CoreType`-native form of [`tuple_diagonal_candidate_dominates_other`]
/// (Issue #6495 stage 5).
pub(crate) fn core_tuple_diagonal_candidate_dominates_other(
    other_param_types: &[CoreType],
    other_type_vars: &[CoreTypeVar],
    pattern: &TupleDiagonalPattern,
    hierarchy: &StructHierarchy,
) -> bool {
    let [CoreType::Tuple(other_elems)] = other_param_types else {
        return false;
    };
    if other_elems.len() != pattern.arity {
        return false;
    }
    if let Some(other_pattern) =
        core_repeated_tuple_typevar_pattern(other_param_types, other_type_vars)
    {
        return core_strictly_subtypes(&pattern.upper_bound, &other_pattern.upper_bound, hierarchy);
    }

    let candidate_bound = &pattern.upper_bound;
    let mut strict = false;
    for elem in other_elems {
        let Some(other_bound) = core_tuple_slot_upper_bound(other_type_vars, elem) else {
            return false;
        };
        if !candidate_bound.is_subtype_of_with_hierarchy(&other_bound, hierarchy) {
            return false;
        }
        if !other_bound.is_subtype_of_with_hierarchy(candidate_bound, hierarchy)
            || matches!(elem, CoreType::Any | CoreType::TypeVar(_))
        {
            strict = true;
        }
    }
    strict
}

/// `CoreType`-native form of [`tuple_slot_upper_bound_core`]: a typevar slot
/// resolves to its own bound, then the `where` clause bound, then `Any`; any
/// other slot compares as itself.
fn core_tuple_slot_upper_bound(type_vars: &[CoreTypeVar], elem: &CoreType) -> Option<CoreType> {
    match elem {
        CoreType::TypeVar(var) => Some(match &var.upper_bound {
            Some(bound) => (**bound).clone(),
            None => find_core_type_var(type_vars, &var.name)
                .map(core_type_var_upper_bound)
                .unwrap_or(CoreType::Any),
        }),
        _ => Some(elem.clone()),
    }
}

// ---------------------------------------------------------------------------
// Union-vs-actual dominance (`f(x::Union{A,B})` against the actual arg types)
// ---------------------------------------------------------------------------

/// Whether a candidate parameter list with `Union` arms strictly dominates
/// another for the given actual argument types (Issue #6231): each slot must
/// be a subtype of the other's, or a `Union` arm covering the actual argument
/// must strictly subtype the other slot.
pub(crate) fn union_actual_candidate_dominates(
    candidate: &[JuliaType],
    other: &[JuliaType],
    arg_types: &[JuliaType],
    hierarchy: &StructHierarchy,
) -> bool {
    if candidate.len() != other.len() || candidate.len() != arg_types.len() {
        return false;
    }

    let mut strict = false;
    for ((candidate_ty, other_ty), actual_ty) in
        candidate.iter().zip(other.iter()).zip(arg_types.iter())
    {
        if julia_type_subtypes(candidate_ty, other_ty, hierarchy) {
            if !julia_type_subtypes(other_ty, candidate_ty, hierarchy) {
                strict = true;
            }
            continue;
        }
        if union_actual_member_strictly_subtypes_other(candidate_ty, other_ty, actual_ty, hierarchy)
        {
            strict = true;
            continue;
        }
        return false;
    }
    strict
}

fn union_actual_member_strictly_subtypes_other(
    candidate: &JuliaType,
    other: &JuliaType,
    actual: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let JuliaType::Union(members) = candidate else {
        return false;
    };
    members.iter().any(|member| {
        julia_type_subtypes(actual, member, hierarchy)
            && julia_type_subtypes(member, other, hierarchy)
            && !julia_type_subtypes(other, member, hierarchy)
    })
}

/// `CoreType`-native form of [`union_actual_candidate_dominates`] (Issue
/// #6495 stage 5): the legacy form bridged every pair through
/// `julia_type_subtypes` (= `CoreType::from` both sides), so consuming the
/// already-bridged projections is decision-identical.
pub(crate) fn core_union_actual_candidate_dominates(
    candidate: &[CoreType],
    other: &[CoreType],
    arg_cores: &[CoreType],
    hierarchy: &StructHierarchy,
) -> bool {
    if candidate.len() != other.len() || candidate.len() != arg_cores.len() {
        return false;
    }

    let mut strict = false;
    for ((candidate_ty, other_ty), actual_ty) in
        candidate.iter().zip(other.iter()).zip(arg_cores.iter())
    {
        if candidate_ty.is_subtype_of_with_hierarchy(other_ty, hierarchy) {
            if !other_ty.is_subtype_of_with_hierarchy(candidate_ty, hierarchy) {
                strict = true;
            }
            continue;
        }
        if core_union_actual_member_strictly_subtypes_other(
            candidate_ty,
            other_ty,
            actual_ty,
            hierarchy,
        ) {
            strict = true;
            continue;
        }
        return false;
    }
    strict
}

fn core_union_actual_member_strictly_subtypes_other(
    candidate: &CoreType,
    other: &CoreType,
    actual: &CoreType,
    hierarchy: &StructHierarchy,
) -> bool {
    let CoreType::Union(members) = candidate else {
        return false;
    };
    members.iter().any(|member| {
        actual.is_subtype_of_with_hierarchy(member, hierarchy)
            && member.is_subtype_of_with_hierarchy(other, hierarchy)
            && !other.is_subtype_of_with_hierarchy(member, hierarchy)
    })
}

// ---------------------------------------------------------------------------
// Type/value diagonal (`f(::Type{T}, ::T) where T` style patterns)
// ---------------------------------------------------------------------------

/// A two-parameter diagonal pattern binding a type slot (`::Type{T}`) to a
/// value slot (`::T`).
pub(crate) struct TypeValueDiagonalPattern {
    pub upper_bound: CoreType,
    pub type_slot: usize,
    pub value_slot: usize,
}

/// Detect a `(::Type{T}, ::T) where T` diagonal pattern in a two-parameter
/// signature (Issue #6233).
pub(crate) fn type_value_diagonal_pattern(
    param_types: &[JuliaType],
    type_params: &[TypeParam],
) -> Option<TypeValueDiagonalPattern> {
    if param_types.len() != 2 {
        return None;
    }

    for type_param in type_params {
        let var_name = type_param.name.as_str();
        let mut type_slot = None;
        let mut value_slot = None;
        for (slot, ty) in param_types.iter().enumerate() {
            match ty {
                JuliaType::TypeOf(inner) => {
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, _) if name == var_name) {
                        type_slot = Some(slot);
                    }
                }
                JuliaType::TypeVar(name, _) if name == var_name => {
                    value_slot = Some(slot);
                }
                _ => {}
            }
        }
        if let (Some(type_slot), Some(value_slot)) = (type_slot, value_slot) {
            if type_slot == value_slot {
                continue;
            }
            return Some(TypeValueDiagonalPattern {
                upper_bound: type_param_upper_bound_core(type_param),
                type_slot,
                value_slot,
            });
        }
    }
    None
}

/// The concrete type the actual arguments bind the diagonal `T` to, if the
/// type argument and value argument agree and respect the bound.
///
/// Unified form (Issue #6331): the runtime copy additionally excluded a list
/// of abstract `JuliaType` variants, but every one of them already maps to a
/// non-concrete `CoreType`, so plain `is_concrete()` (the compile form) is
/// equivalent.
pub(crate) fn actual_type_value_diagonal_binding<'a>(
    arg_types: &'a [JuliaType],
    pattern: &TypeValueDiagonalPattern,
    hierarchy: &StructHierarchy,
) -> Option<&'a JuliaType> {
    let JuliaType::TypeOf(type_arg) = arg_types.get(pattern.type_slot)? else {
        return None;
    };
    let value_arg = arg_types.get(pattern.value_slot)?;
    if type_arg.as_ref() != value_arg || !type_arg.is_concrete() {
        return None;
    }
    if !CoreType::from(type_arg.as_ref())
        .is_subtype_of_with_hierarchy(&pattern.upper_bound, hierarchy)
    {
        return None;
    }
    Some(type_arg.as_ref())
}

/// Whether the bound diagonal candidate strictly beats another method's
/// corresponding slots.
pub(crate) fn type_value_diagonal_candidate_dominates_other(
    other_param_types: &[JuliaType],
    pattern: &TypeValueDiagonalPattern,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(type_param) = other_param_types.get(pattern.type_slot) else {
        return false;
    };
    let Some(value_param) = other_param_types.get(pattern.value_slot) else {
        return false;
    };
    type_value_diagonal_binding_beats_type_slot(type_param, binding, hierarchy)
        && type_value_diagonal_binding_beats_value_slot(value_param, binding, hierarchy)
}

fn type_value_diagonal_binding_beats_type_slot(
    other: &JuliaType,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let JuliaType::TypeOf(other_inner) = other else {
        return false;
    };
    julia_type_subtypes(binding, other_inner, hierarchy)
        && !julia_type_subtypes(other_inner, binding, hierarchy)
}

fn type_value_diagonal_binding_beats_value_slot(
    other: &JuliaType,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    julia_type_subtypes(binding, other, hierarchy)
        && !julia_type_subtypes(other, binding, hierarchy)
}

/// `CoreType`-native form of [`type_value_diagonal_pattern`] (Issue #6495
/// stage 5). The slot detection keeps the legacy *exact* `where`-name
/// comparison (no base-name stripping).
pub(crate) fn core_type_value_diagonal_pattern(
    param_types: &[CoreType],
    type_vars: &[CoreTypeVar],
) -> Option<TypeValueDiagonalPattern> {
    if param_types.len() != 2 {
        return None;
    }

    for type_var in type_vars {
        let var_name = type_var.name.as_str();
        let mut type_slot = None;
        let mut value_slot = None;
        for (slot, ty) in param_types.iter().enumerate() {
            match ty {
                CoreType::TypeOf(inner) => {
                    if matches!(inner.as_ref(), CoreType::TypeVar(v) if v.name == var_name) {
                        type_slot = Some(slot);
                    }
                }
                CoreType::TypeVar(v) if v.name == var_name => {
                    value_slot = Some(slot);
                }
                _ => {}
            }
        }
        if let (Some(type_slot), Some(value_slot)) = (type_slot, value_slot) {
            if type_slot == value_slot {
                continue;
            }
            return Some(TypeValueDiagonalPattern {
                upper_bound: core_type_var_upper_bound(type_var),
                type_slot,
                value_slot,
            });
        }
    }
    None
}

/// `CoreType`-native form of [`type_value_diagonal_candidate_dominates_other`]
/// (Issue #6495 stage 5); `binding` is the bridged actual-binding type.
pub(crate) fn core_type_value_diagonal_candidate_dominates_other(
    other_param_types: &[CoreType],
    pattern: &TypeValueDiagonalPattern,
    binding: &CoreType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(type_param) = other_param_types.get(pattern.type_slot) else {
        return false;
    };
    let Some(value_param) = other_param_types.get(pattern.value_slot) else {
        return false;
    };
    core_diagonal_binding_beats_type_slot(type_param, binding, hierarchy)
        && core_strictly_subtypes(binding, value_param, hierarchy)
}

/// `CoreType`-native form of [`type_value_diagonal_binding_beats_type_slot`].
fn core_diagonal_binding_beats_type_slot(
    other: &CoreType,
    binding: &CoreType,
    hierarchy: &StructHierarchy,
) -> bool {
    let CoreType::TypeOf(other_inner) = other else {
        return false;
    };
    core_strictly_subtypes(binding, other_inner, hierarchy)
}

// ---------------------------------------------------------------------------
// Type/vector diagonal (`f(::Type{T}, ::Vector{T}) where T` style patterns)
// ---------------------------------------------------------------------------

/// A two-parameter diagonal pattern binding a type slot (`::Type{T}`) to a
/// vector slot (`::Vector{T}` / `::AbstractVector{T}` / `::AbstractArray{T,1}`).
pub(crate) struct TypeVectorDiagonalPattern {
    pub upper_bound: CoreType,
    pub type_slot: usize,
    pub vector_slot: usize,
}

/// Detect a `(::Type{T}, ::Vector{T}) where T` diagonal pattern (Issue #6235).
pub(crate) fn type_vector_diagonal_pattern(
    param_types: &[JuliaType],
    type_params: &[TypeParam],
) -> Option<TypeVectorDiagonalPattern> {
    if param_types.len() != 2 {
        return None;
    }

    for type_param in type_params {
        let var_name = type_param.name.as_str();
        let mut type_slot = None;
        let mut vector_slot = None;
        for (slot, ty) in param_types.iter().enumerate() {
            match ty {
                JuliaType::TypeOf(inner) => {
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, _) if name == var_name) {
                        type_slot = Some(slot);
                    }
                }
                JuliaType::VectorOf(inner) => {
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, _) if name == var_name) {
                        vector_slot = Some(slot);
                    }
                }
                JuliaType::Struct(_)
                    if abstract_vector_param_type(ty)
                        .is_some_and(|inner| diagonal_param_matches_var(&inner, var_name)) =>
                {
                    vector_slot = Some(slot);
                }
                _ => {}
            }
        }
        if let (Some(type_slot), Some(vector_slot)) = (type_slot, vector_slot) {
            if type_slot == vector_slot {
                continue;
            }
            return Some(TypeVectorDiagonalPattern {
                upper_bound: type_param_upper_bound_core(type_param),
                type_slot,
                vector_slot,
            });
        }
    }
    None
}

/// The concrete type the actual `(::Type{T}, ::Vector{T})` arguments bind `T`
/// to, if both agree and respect the bound.
pub(crate) fn actual_type_vector_diagonal_binding<'a>(
    arg_types: &'a [JuliaType],
    pattern: &TypeVectorDiagonalPattern,
    hierarchy: &StructHierarchy,
) -> Option<&'a JuliaType> {
    let JuliaType::TypeOf(type_arg) = arg_types.get(pattern.type_slot)? else {
        return None;
    };
    let JuliaType::VectorOf(vector_arg) = arg_types.get(pattern.vector_slot)? else {
        return None;
    };
    if type_arg.as_ref() != vector_arg.as_ref() || !type_arg.is_concrete() {
        return None;
    }
    if !CoreType::from(type_arg.as_ref())
        .is_subtype_of_with_hierarchy(&pattern.upper_bound, hierarchy)
    {
        return None;
    }
    Some(type_arg.as_ref())
}

/// Whether the bound diagonal candidate strictly beats another method's type
/// and vector slots.
pub(crate) fn type_vector_diagonal_candidate_dominates_other(
    other_param_types: &[JuliaType],
    pattern: &TypeVectorDiagonalPattern,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(type_param) = other_param_types.get(pattern.type_slot) else {
        return false;
    };
    let Some(vector_param) = other_param_types.get(pattern.vector_slot) else {
        return false;
    };
    type_value_diagonal_binding_beats_type_slot(type_param, binding, hierarchy)
        && type_vector_diagonal_binding_beats_vector_slot(vector_param, binding, hierarchy)
}

/// Unified form (Issue #6331): when the other slot's element is a bounded
/// typevar (`Vector{S} where S<:Bound`), compare against the bound — this arm
/// previously existed only in the runtime copy (added for Issue #6251); the
/// compile copy compared against the raw `TypeVar` and could never dominate.
fn type_vector_diagonal_binding_beats_vector_slot(
    other: &JuliaType,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(other_inner) = vector_like_param_type(other) else {
        return false;
    };
    diagonal_binding_beats_container_element(&other_inner, binding, hierarchy)
}

/// Shared element-slot comparison for the type/vector and type/matrix
/// diagonals: a bounded typevar element compares against its bound, anything
/// else compares structurally (Issue #6336 — both sides are `CoreType`s).
fn diagonal_binding_beats_container_element(
    other_inner: &CoreType,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let binding_core = CoreType::from(binding);
    if let CoreType::TypeVar(var) = other_inner {
        if let Some(bound) = &var.upper_bound {
            return core_strictly_subtypes(&binding_core, bound, hierarchy);
        }
    }
    core_strictly_subtypes(&binding_core, other_inner, hierarchy)
}

fn vector_like_param_type(ty: &JuliaType) -> Option<CoreType> {
    match ty {
        JuliaType::VectorOf(inner) => Some(CoreType::from(inner.as_ref())),
        JuliaType::Struct(_) => abstract_vector_param_type(ty),
        _ => None,
    }
}

/// Element type of an abstract vector-like parameter spelled as a struct name
/// (`AbstractVector{T}`, `AbstractArray{T,1}`, `AbstractArray{T,N}`, ...).
pub(crate) fn abstract_vector_param_type(ty: &JuliaType) -> Option<CoreType> {
    abstract_container_element_type(ty, &CoreAbstract::AbstractVector, "AbstractVector", 1)
}

/// `CoreType`-native form of [`type_vector_diagonal_pattern`] (Issue #6495
/// stage 5). The concrete-vector slot arm consumes the `Struct("Vector", [_])`
/// image (the canonical projection of `JuliaType::VectorOf`); abstract
/// spellings go through [`core_abstract_vector_param_type`].
pub(crate) fn core_type_vector_diagonal_pattern(
    param_types: &[CoreType],
    type_vars: &[CoreTypeVar],
) -> Option<TypeVectorDiagonalPattern> {
    if param_types.len() != 2 {
        return None;
    }

    for type_var in type_vars {
        let var_name = type_var.name.as_str();
        let mut type_slot = None;
        let mut vector_slot = None;
        for (slot, ty) in param_types.iter().enumerate() {
            match ty {
                CoreType::TypeOf(inner) => {
                    if matches!(inner.as_ref(), CoreType::TypeVar(v) if v.name == var_name) {
                        type_slot = Some(slot);
                    }
                }
                CoreType::Struct { name, params }
                    if name == "Vector"
                        && matches!(params.as_slice(),
                            [CoreType::TypeVar(v)] if v.name == var_name) =>
                {
                    vector_slot = Some(slot);
                }
                _ if core_abstract_vector_param_type(ty)
                    .is_some_and(|inner| diagonal_param_matches_var(&inner, var_name)) =>
                {
                    vector_slot = Some(slot);
                }
                _ => {}
            }
        }
        if let (Some(type_slot), Some(vector_slot)) = (type_slot, vector_slot) {
            if type_slot == vector_slot {
                continue;
            }
            return Some(TypeVectorDiagonalPattern {
                upper_bound: core_type_var_upper_bound(type_var),
                type_slot,
                vector_slot,
            });
        }
    }
    None
}

/// `CoreType`-native form of
/// [`type_vector_diagonal_candidate_dominates_other`] (Issue #6495 stage 5).
pub(crate) fn core_type_vector_diagonal_candidate_dominates_other(
    other_param_types: &[CoreType],
    pattern: &TypeVectorDiagonalPattern,
    binding: &CoreType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(type_param) = other_param_types.get(pattern.type_slot) else {
        return false;
    };
    let Some(vector_param) = other_param_types.get(pattern.vector_slot) else {
        return false;
    };
    core_diagonal_binding_beats_type_slot(type_param, binding, hierarchy)
        && core_vector_like_param_type(vector_param).is_some_and(|other_inner| {
            core_diagonal_binding_beats_container_element(&other_inner, binding, hierarchy)
        })
}

/// `CoreType`-native form of [`diagonal_binding_beats_container_element`].
fn core_diagonal_binding_beats_container_element(
    other_inner: &CoreType,
    binding: &CoreType,
    hierarchy: &StructHierarchy,
) -> bool {
    if let CoreType::TypeVar(var) = other_inner {
        if let Some(bound) = &var.upper_bound {
            return core_strictly_subtypes(binding, bound, hierarchy);
        }
    }
    core_strictly_subtypes(binding, other_inner, hierarchy)
}

/// `CoreType`-native form of [`vector_like_param_type`].
fn core_vector_like_param_type(ty: &CoreType) -> Option<CoreType> {
    match ty {
        CoreType::Struct { name, params } if name == "Vector" && params.len() == 1 => {
            Some(params[0].clone())
        }
        _ => core_abstract_vector_param_type(ty),
    }
}

/// `CoreType`-native form of [`abstract_vector_param_type`] (Issue #6495
/// stage 5).
pub(crate) fn core_abstract_vector_param_type(ty: &CoreType) -> Option<CoreType> {
    core_abstract_container_element_type(ty, &CoreAbstract::AbstractVector, "AbstractVector", 1)
}

// ---------------------------------------------------------------------------
// Type/matrix diagonal (`f(::Type{T}, ::Matrix{T}) where T` style patterns)
// ---------------------------------------------------------------------------

/// A two-parameter diagonal pattern binding a type slot (`::Type{T}`) to a
/// matrix slot (`::Matrix{T}` / `::AbstractMatrix{T}` / `::AbstractArray{T,2}`).
pub(crate) struct TypeMatrixDiagonalPattern {
    pub upper_bound: CoreType,
    pub type_slot: usize,
    pub matrix_slot: usize,
}

/// Detect a `(::Type{T}, ::Matrix{T}) where T` diagonal pattern (Issue #6237).
pub(crate) fn type_matrix_diagonal_pattern(
    param_types: &[JuliaType],
    type_params: &[TypeParam],
) -> Option<TypeMatrixDiagonalPattern> {
    if param_types.len() != 2 {
        return None;
    }

    for type_param in type_params {
        let var_name = type_param.name.as_str();
        let mut type_slot = None;
        let mut matrix_slot = None;
        for (slot, ty) in param_types.iter().enumerate() {
            match ty {
                JuliaType::TypeOf(inner) => {
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, _) if name == var_name) {
                        type_slot = Some(slot);
                    }
                }
                JuliaType::MatrixOf(inner) => {
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, _) if name == var_name) {
                        matrix_slot = Some(slot);
                    }
                }
                JuliaType::Struct(_)
                    if abstract_matrix_param_type(ty)
                        .is_some_and(|inner| diagonal_param_matches_var(&inner, var_name)) =>
                {
                    matrix_slot = Some(slot);
                }
                _ => {}
            }
        }
        if let (Some(type_slot), Some(matrix_slot)) = (type_slot, matrix_slot) {
            if type_slot == matrix_slot {
                continue;
            }
            return Some(TypeMatrixDiagonalPattern {
                upper_bound: type_param_upper_bound_core(type_param),
                type_slot,
                matrix_slot,
            });
        }
    }
    None
}

/// The concrete type the actual `(::Type{T}, ::Matrix{T})` arguments bind `T`
/// to, if both agree and respect the bound.
pub(crate) fn actual_type_matrix_diagonal_binding<'a>(
    arg_types: &'a [JuliaType],
    pattern: &TypeMatrixDiagonalPattern,
    hierarchy: &StructHierarchy,
) -> Option<&'a JuliaType> {
    let JuliaType::TypeOf(type_arg) = arg_types.get(pattern.type_slot)? else {
        return None;
    };
    let JuliaType::MatrixOf(matrix_arg) = arg_types.get(pattern.matrix_slot)? else {
        return None;
    };
    if type_arg.as_ref() != matrix_arg.as_ref() || !type_arg.is_concrete() {
        return None;
    }
    if !CoreType::from(type_arg.as_ref())
        .is_subtype_of_with_hierarchy(&pattern.upper_bound, hierarchy)
    {
        return None;
    }
    Some(type_arg.as_ref())
}

/// Whether the bound diagonal candidate strictly beats another method's type
/// and matrix slots.
pub(crate) fn type_matrix_diagonal_candidate_dominates_other(
    other_param_types: &[JuliaType],
    pattern: &TypeMatrixDiagonalPattern,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(type_param) = other_param_types.get(pattern.type_slot) else {
        return false;
    };
    let Some(matrix_param) = other_param_types.get(pattern.matrix_slot) else {
        return false;
    };
    type_value_diagonal_binding_beats_type_slot(type_param, binding, hierarchy)
        && type_matrix_diagonal_binding_beats_matrix_slot(matrix_param, binding, hierarchy)
}

/// See [`type_vector_diagonal_binding_beats_vector_slot`] for the unified
/// bounded-typevar arm (Issue #6331).
fn type_matrix_diagonal_binding_beats_matrix_slot(
    other: &JuliaType,
    binding: &JuliaType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(other_inner) = matrix_like_param_type(other) else {
        return false;
    };
    diagonal_binding_beats_container_element(&other_inner, binding, hierarchy)
}

fn matrix_like_param_type(ty: &JuliaType) -> Option<CoreType> {
    match ty {
        JuliaType::MatrixOf(inner) => Some(CoreType::from(inner.as_ref())),
        JuliaType::Struct(_) => abstract_matrix_param_type(ty),
        _ => None,
    }
}

/// Element type of an abstract matrix-like parameter spelled as a struct name
/// (`AbstractMatrix{T}`, `AbstractArray{T,2}`, `AbstractArray{T,N}`, ...).
pub(crate) fn abstract_matrix_param_type(ty: &JuliaType) -> Option<CoreType> {
    abstract_container_element_type(ty, &CoreAbstract::AbstractMatrix, "AbstractMatrix", 2)
}

/// `CoreType`-native form of [`type_matrix_diagonal_pattern`] (Issue #6495
/// stage 5).
pub(crate) fn core_type_matrix_diagonal_pattern(
    param_types: &[CoreType],
    type_vars: &[CoreTypeVar],
) -> Option<TypeMatrixDiagonalPattern> {
    if param_types.len() != 2 {
        return None;
    }

    for type_var in type_vars {
        let var_name = type_var.name.as_str();
        let mut type_slot = None;
        let mut matrix_slot = None;
        for (slot, ty) in param_types.iter().enumerate() {
            match ty {
                CoreType::TypeOf(inner) => {
                    if matches!(inner.as_ref(), CoreType::TypeVar(v) if v.name == var_name) {
                        type_slot = Some(slot);
                    }
                }
                CoreType::Struct { name, params }
                    if name == "Matrix"
                        && matches!(params.as_slice(),
                            [CoreType::TypeVar(v)] if v.name == var_name) =>
                {
                    matrix_slot = Some(slot);
                }
                _ if core_abstract_matrix_param_type(ty)
                    .is_some_and(|inner| diagonal_param_matches_var(&inner, var_name)) =>
                {
                    matrix_slot = Some(slot);
                }
                _ => {}
            }
        }
        if let (Some(type_slot), Some(matrix_slot)) = (type_slot, matrix_slot) {
            if type_slot == matrix_slot {
                continue;
            }
            return Some(TypeMatrixDiagonalPattern {
                upper_bound: core_type_var_upper_bound(type_var),
                type_slot,
                matrix_slot,
            });
        }
    }
    None
}

/// `CoreType`-native form of
/// [`type_matrix_diagonal_candidate_dominates_other`] (Issue #6495 stage 5).
pub(crate) fn core_type_matrix_diagonal_candidate_dominates_other(
    other_param_types: &[CoreType],
    pattern: &TypeMatrixDiagonalPattern,
    binding: &CoreType,
    hierarchy: &StructHierarchy,
) -> bool {
    let Some(type_param) = other_param_types.get(pattern.type_slot) else {
        return false;
    };
    let Some(matrix_param) = other_param_types.get(pattern.matrix_slot) else {
        return false;
    };
    core_diagonal_binding_beats_type_slot(type_param, binding, hierarchy)
        && core_matrix_like_param_type(matrix_param).is_some_and(|other_inner| {
            core_diagonal_binding_beats_container_element(&other_inner, binding, hierarchy)
        })
}

/// `CoreType`-native form of [`matrix_like_param_type`].
fn core_matrix_like_param_type(ty: &CoreType) -> Option<CoreType> {
    match ty {
        CoreType::Struct { name, params } if name == "Matrix" && params.len() == 1 => {
            Some(params[0].clone())
        }
        _ => core_abstract_matrix_param_type(ty),
    }
}

/// `CoreType`-native form of [`abstract_matrix_param_type`] (Issue #6495
/// stage 5).
pub(crate) fn core_abstract_matrix_param_type(ty: &CoreType) -> Option<CoreType> {
    core_abstract_container_element_type(ty, &CoreAbstract::AbstractMatrix, "AbstractMatrix", 2)
}

// ---------------------------------------------------------------------------
// Abstract container parameter extraction (structured — Issue #6336)
// ---------------------------------------------------------------------------

/// Element type of an abstract container parameter, extracted from the
/// structured `CoreType` view of the parameter instead of re-parsing the
/// string-encoded `JuliaType::Struct` name in the dispatch path (Issue #6336:
/// the only name→structure step is the central `CoreType::from` bridge).
///
/// Accepted shapes (for `expected_name = "AbstractVector"`, `expected_rank = 1`):
/// bare `AbstractVector` (element unconstrained → `Any`), `AbstractVector{T}`,
/// `AbstractArray{T}` (rank unconstrained), `AbstractArray{T, 1}` and
/// `AbstractArray{T, N}` (typevar rank).
fn abstract_container_element_type(
    ty: &JuliaType,
    bare: &CoreAbstract,
    expected_name: &str,
    expected_rank: i64,
) -> Option<CoreType> {
    let JuliaType::Struct(_) = ty else {
        return None;
    };
    core_abstract_container_element_type(&CoreType::from(ty), bare, expected_name, expected_rank)
}

/// `CoreType`-native body of [`abstract_container_element_type`] (Issue #6495
/// stage 5): consumed directly by the compile-time dominance pre-checks. The
/// legacy `JuliaType::Struct(_)` gate above is a no-op on the image set — the
/// accepted shapes below only arise as images of `Struct` name spellings.
fn core_abstract_container_element_type(
    ty: &CoreType,
    bare: &CoreAbstract,
    expected_name: &str,
    expected_rank: i64,
) -> Option<CoreType> {
    match ty {
        CoreType::Abstract(kind) if kind == bare => Some(CoreType::Any),
        CoreType::Struct { name, params } if name == expected_name => {
            Some(params.first().cloned().unwrap_or(CoreType::Any))
        }
        CoreType::Struct { name, params } if name == "AbstractArray" => {
            abstract_array_element_for_rank(params.clone(), expected_rank)
        }
        // Defensive: a namespaced bare spelling (`Base.AbstractVector`) that the
        // bridge could not resolve still names the same container family.
        CoreType::Named(name) if name.rsplit('.').next() == Some(expected_name) => {
            Some(CoreType::Any)
        }
        _ => None,
    }
}

/// Element of a structured `AbstractArray{...}` application whose rank slot is
/// compatible with `expected_rank` (absent, the exact rank value, or a free
/// typevar rank `N`).
fn abstract_array_element_for_rank(params: Vec<CoreType>, expected_rank: i64) -> Option<CoreType> {
    let mut params = params.into_iter();
    let elem = params.next()?;
    if params.len() > 1 {
        return None;
    }
    match params.next() {
        // `AbstractArray{T}` — rank unconstrained.
        None => Some(elem),
        // `AbstractArray{T, 1}` / `AbstractArray{T, 2}` — rank must agree.
        Some(CoreType::Value(CoreValueParam::Int(rank))) if rank == expected_rank => Some(elem),
        // `AbstractArray{T, N}` — a free typevar rank matches any dimension.
        Some(CoreType::TypeVar(_)) => Some(elem),
        Some(_) => None,
    }
}

fn diagonal_param_matches_var(param: &CoreType, var_name: &str) -> bool {
    match param {
        CoreType::TypeVar(var) => var.name == var_name,
        CoreType::Named(name) => name == var_name,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Vector diagonal (`f(a::Vector{T}, b::Vector{T}) where T` style patterns)
// ---------------------------------------------------------------------------

/// A diagonal pattern repeating `Vector{T}` over two or more slots.
pub(crate) struct VectorDiagonalPattern<'a> {
    pub var_name: &'a str,
    pub upper_bound: CoreType,
    pub slots: Vec<usize>,
}

/// Detect a repeated `Vector{T}` diagonal pattern across the parameter slots
/// (Issue #6229).
pub(crate) fn repeated_vector_typevar_pattern<'a>(
    param_types: &[JuliaType],
    type_params: &'a [TypeParam],
) -> Option<VectorDiagonalPattern<'a>> {
    let mut counts: HashMap<&str, Vec<usize>> = HashMap::new();
    for (slot, ty) in param_types.iter().enumerate() {
        let JuliaType::VectorOf(elem) = ty else {
            continue;
        };
        let JuliaType::TypeVar(name, _) = elem.as_ref() else {
            continue;
        };
        if type_params.iter().any(|tp| tp.name == *name) {
            counts.entry(name.as_str()).or_default().push(slot);
        }
    }

    for type_param in type_params {
        let Some(slots) = counts.remove(type_param.name.as_str()) else {
            continue;
        };
        if slots.len() < 2 {
            continue;
        }
        return Some(VectorDiagonalPattern {
            var_name: type_param.name.as_str(),
            upper_bound: type_param_upper_bound_core(type_param),
            slots,
        });
    }
    None
}

/// Whether the actual arguments at the diagonal slots are vectors sharing one
/// element type.
pub(crate) fn actual_vector_slots_share_element_type(
    arg_types: &[JuliaType],
    slots: &[usize],
) -> bool {
    let Some(first_slot) = slots.first() else {
        return false;
    };
    let Some(first_arg) = arg_types.get(*first_slot) else {
        return false;
    };
    let Some(first_elem) = vector_element_type(first_arg) else {
        return false;
    };
    slots.iter().skip(1).all(|slot| {
        arg_types
            .get(*slot)
            .and_then(vector_element_type)
            .is_some_and(|elem| elem == first_elem)
    })
}

/// Whether another method's parameters at the diagonal slots carry independent
/// typevar bounds no tighter than the diagonal candidate's bound (so the
/// diagonal candidate may dominate).
pub(crate) fn independent_vector_bounds_are_no_tighter(
    param_types: &[JuliaType],
    pattern: &VectorDiagonalPattern<'_>,
    hierarchy: &StructHierarchy,
) -> bool {
    pattern.slots.iter().all(|slot| {
        param_types
            .get(*slot)
            .and_then(|ty| independent_vector_bound(ty, pattern.var_name))
            .is_some_and(|bound| {
                pattern
                    .upper_bound
                    .is_subtype_of_with_hierarchy(&bound, hierarchy)
            })
    })
}

fn vector_element_type(ty: &JuliaType) -> Option<&JuliaType> {
    match ty {
        JuliaType::VectorOf(elem) => Some(elem.as_ref()),
        _ => None,
    }
}

fn independent_vector_bound(ty: &JuliaType, diagonal_var: &str) -> Option<CoreType> {
    match ty {
        JuliaType::VectorOf(elem) => match elem.as_ref() {
            JuliaType::TypeVar(name, Some(bound)) if name != diagonal_var => {
                Some(CoreType::from_julia_name(bound))
            }
            JuliaType::Any => Some(CoreType::Any),
            _ => None,
        },
        _ => None,
    }
}

/// `CoreType`-native form of [`repeated_vector_typevar_pattern`] (Issue #6495
/// stage 5). Mirrors the legacy *exact* `where`-name comparisons.
pub(crate) fn core_repeated_vector_typevar_pattern<'a>(
    param_types: &[CoreType],
    type_vars: &'a [CoreTypeVar],
) -> Option<VectorDiagonalPattern<'a>> {
    let mut counts: HashMap<&str, Vec<usize>> = HashMap::new();
    for (slot, ty) in param_types.iter().enumerate() {
        let CoreType::Struct { name, params } = ty else {
            continue;
        };
        if name != "Vector" {
            continue;
        }
        let [CoreType::TypeVar(var)] = params.as_slice() else {
            continue;
        };
        if type_vars.iter().any(|tv| tv.name == var.name) {
            counts.entry(var.name.as_str()).or_default().push(slot);
        }
    }

    for type_var in type_vars {
        let Some(slots) = counts.remove(type_var.name.as_str()) else {
            continue;
        };
        if slots.len() < 2 {
            continue;
        }
        return Some(VectorDiagonalPattern {
            var_name: type_var.name.as_str(),
            upper_bound: core_type_var_upper_bound(type_var),
            slots,
        });
    }
    None
}

/// `CoreType`-native form of [`independent_vector_bounds_are_no_tighter`]
/// (Issue #6495 stage 5).
pub(crate) fn core_independent_vector_bounds_are_no_tighter(
    param_types: &[CoreType],
    pattern: &VectorDiagonalPattern<'_>,
    hierarchy: &StructHierarchy,
) -> bool {
    pattern.slots.iter().all(|slot| {
        param_types
            .get(*slot)
            .and_then(|ty| core_independent_vector_bound(ty, pattern.var_name))
            .is_some_and(|bound| {
                pattern
                    .upper_bound
                    .is_subtype_of_with_hierarchy(&bound, hierarchy)
            })
    })
}

/// `CoreType`-native form of [`independent_vector_bound`]: a non-diagonal
/// bounded typevar element yields its bound, a bare `Any` element yields
/// `Any`, anything else (including an unbounded typevar) yields `None`.
fn core_independent_vector_bound(ty: &CoreType, diagonal_var: &str) -> Option<CoreType> {
    let CoreType::Struct { name, params } = ty else {
        return None;
    };
    if name != "Vector" {
        return None;
    }
    match params.as_slice() {
        [CoreType::TypeVar(var)] if var.name != diagonal_var => var.upper_bound.as_deref().cloned(),
        [CoreType::Any] => Some(CoreType::Any),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuple_vararg_expansion_expands_trailing_vararg() {
        let pattern = vec![
            JuliaType::Int64,
            JuliaType::Struct("Vararg{Int64}".to_string()),
        ];
        let expansion = expand_tuple_vararg_pattern_for_len(&pattern, 3)
            .expect("trailing vararg pattern must expand");
        let int64 = CoreType::Primitive(super::super::CorePrimitive::Int64);
        assert_eq!(
            expansion.expanded,
            vec![int64.clone(), int64.clone(), int64.clone()]
        );
        assert_eq!(expansion.vararg_element, int64);

        // CoreType-native entry: identical expansion from the bridged pattern
        // (Issue #6495 stage 5).
        let core_pattern: Vec<CoreType> = pattern.iter().map(CoreType::from).collect();
        let core_expansion = core_expand_tuple_vararg_pattern_for_len(&core_pattern, 3)
            .expect("core trailing vararg pattern must expand");
        assert_eq!(core_expansion.expanded, expansion.expanded);
        assert_eq!(core_expansion.vararg_element, expansion.vararg_element);
    }

    #[test]
    fn tuple_vararg_expansion_rejects_short_actual_and_non_vararg() {
        let pattern = vec![
            JuliaType::Int64,
            JuliaType::Float64,
            JuliaType::Struct("Vararg{Int64}".to_string()),
        ];
        assert!(expand_tuple_vararg_pattern_for_len(&pattern, 1).is_none());
        let fixed = vec![JuliaType::Int64, JuliaType::Float64];
        assert!(expand_tuple_vararg_pattern_for_len(&fixed, 2).is_none());
    }

    #[test]
    fn type_value_diagonal_binding_dominates_fixed_supertype_slots() {
        let hierarchy = StructHierarchy::new();
        let type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Integer".to_string(),
        )];
        // f(::Type{T}, ::T) where T<:Integer
        let diagonal_params = vec![
            JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::TypeVar("T".to_string(), None),
        ];
        let pattern = type_value_diagonal_pattern(&diagonal_params, &type_params)
            .expect("diagonal pattern must be detected");
        assert_eq!(pattern.type_slot, 0);
        assert_eq!(pattern.value_slot, 1);
        assert_eq!(
            pattern.upper_bound,
            CoreType::Abstract(CoreAbstract::Integer)
        );

        // Call site: f(Int64, 1) — binds T = Int64.
        let arg_types = vec![
            JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            JuliaType::Int64,
        ];
        let binding = actual_type_value_diagonal_binding(&arg_types, &pattern, &hierarchy)
            .expect("binding must resolve to the concrete type argument");
        assert_eq!(binding, &JuliaType::Int64);

        // The bound diagonal strictly beats f(::Type{Integer}, ::Integer) ...
        let fixed_supertype = vec![
            JuliaType::TypeOf(Box::new(JuliaType::Integer)),
            JuliaType::Integer,
        ];
        assert!(type_value_diagonal_candidate_dominates_other(
            &fixed_supertype,
            &pattern,
            binding,
            &hierarchy
        ));
        // ... but not an equally specific f(::Type{Int64}, ::Int64).
        let same_specificity = vec![
            JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            JuliaType::Int64,
        ];
        assert!(!type_value_diagonal_candidate_dominates_other(
            &same_specificity,
            &pattern,
            binding,
            &hierarchy
        ));
    }

    #[test]
    fn tuple_diagonal_pattern_dominates_independent_any_slots() {
        let hierarchy = StructHierarchy::new();
        let type_params = vec![TypeParam::new("T".to_string())];
        // f(t::Tuple{T, T}) where T
        let diagonal_params = vec![JuliaType::TupleOf(vec![
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeVar("T".to_string(), None),
        ])];
        let pattern = repeated_tuple_typevar_pattern(&diagonal_params, &type_params)
            .expect("tuple diagonal pattern must be detected");
        assert_eq!(pattern.arity, 2);

        let actual = vec![JuliaType::Int64, JuliaType::Int64];
        assert!(actual_tuple_satisfies_diagonal_pattern(
            &actual, &pattern, &hierarchy
        ));
        let mixed = vec![JuliaType::Int64, JuliaType::Float64];
        assert!(!actual_tuple_satisfies_diagonal_pattern(
            &mixed, &pattern, &hierarchy
        ));

        // Diagonal Tuple{T,T} beats the independent Tuple{Any,Any} fallback.
        let any_params = vec![JuliaType::TupleOf(vec![JuliaType::Any, JuliaType::Any])];
        assert!(tuple_diagonal_candidate_dominates_other(
            &any_params,
            &[],
            &pattern,
            &hierarchy
        ));
    }

    #[test]
    fn vector_diagonal_pattern_detects_repeated_typevar_slots() {
        let type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Number".to_string(),
        )];
        // f(a::Vector{T}, b::Vector{T}) where T<:Number
        let params = vec![
            JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::VectorOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
        ];
        let pattern = repeated_vector_typevar_pattern(&params, &type_params)
            .expect("vector diagonal pattern must be detected");
        assert_eq!(pattern.slots, vec![0, 1]);
        assert_eq!(
            pattern.upper_bound,
            CoreType::Abstract(CoreAbstract::Number)
        );

        let same = vec![
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
        ];
        assert!(actual_vector_slots_share_element_type(
            &same,
            &pattern.slots
        ));
        let mixed = vec![
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::VectorOf(Box::new(JuliaType::Float64)),
        ];
        assert!(!actual_vector_slots_share_element_type(
            &mixed,
            &pattern.slots
        ));
    }

    /// Issue #6495 (stage 5): the CoreType-native counterparts must decide
    /// identically to the legacy projections for representative diagonal /
    /// union / vararg shapes (corpus-wide parity is pinned by
    /// `base_method_core_dominance_parity_issue_6495` in `compile/cache.rs`).
    #[test]
    fn core_specificity_counterparts_match_legacy_issue_6495() {
        let hierarchy = StructHierarchy::new();
        let t = || JuliaType::TypeVar("T".to_string(), None);
        let bridge =
            |params: &[JuliaType]| -> Vec<CoreType> { params.iter().map(CoreType::from).collect() };

        // Tuple diagonal: f(t::Tuple{T, T}) where T<:Number vs Tuple{Any, Any}.
        let type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Number".to_string(),
        )];
        let core_vars: Vec<CoreTypeVar> = type_params.iter().map(CoreTypeVar::from).collect();
        let diagonal = vec![JuliaType::TupleOf(vec![t(), t()])];
        let legacy_pattern = repeated_tuple_typevar_pattern(&diagonal, &type_params).unwrap();
        let core_pattern =
            core_repeated_tuple_typevar_pattern(&bridge(&diagonal), &core_vars).unwrap();
        assert_eq!(core_pattern.upper_bound, legacy_pattern.upper_bound);
        assert_eq!(core_pattern.arity, legacy_pattern.arity);
        let any_params = vec![JuliaType::TupleOf(vec![JuliaType::Any, JuliaType::Any])];
        assert_eq!(
            core_tuple_diagonal_candidate_dominates_other(
                &bridge(&any_params),
                &[],
                &core_pattern,
                &hierarchy
            ),
            tuple_diagonal_candidate_dominates_other(&any_params, &[], &legacy_pattern, &hierarchy)
        );

        // Union-vs-actual: f(::Union{Int64, String}) vs f(::Any) for Int64.
        let union_params = vec![JuliaType::Union(vec![JuliaType::Int64, JuliaType::String])];
        let any_param = vec![JuliaType::Any];
        let args = vec![JuliaType::Int64];
        let arg_cores = bridge(&args);
        assert!(core_union_actual_candidate_dominates(
            &bridge(&union_params),
            &bridge(&any_param),
            &arg_cores,
            &hierarchy
        ));
        assert_eq!(
            core_union_actual_candidate_dominates(
                &bridge(&union_params),
                &bridge(&any_param),
                &arg_cores,
                &hierarchy
            ),
            union_actual_candidate_dominates(&union_params, &any_param, &args, &hierarchy)
        );

        // Type/value diagonal: f(::Type{T}, ::T) where T<:Integer.
        let tv_params = vec![JuliaType::TypeOf(Box::new(t())), t()];
        let tv_type_params = vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Integer".to_string(),
        )];
        let tv_core_vars: Vec<CoreTypeVar> = tv_type_params.iter().map(CoreTypeVar::from).collect();
        let legacy_tv = type_value_diagonal_pattern(&tv_params, &tv_type_params).unwrap();
        let core_tv = core_type_value_diagonal_pattern(&bridge(&tv_params), &tv_core_vars).unwrap();
        assert_eq!(core_tv.upper_bound, legacy_tv.upper_bound);
        assert_eq!(
            (core_tv.type_slot, core_tv.value_slot),
            (legacy_tv.type_slot, legacy_tv.value_slot)
        );
        let fixed_supertype = vec![
            JuliaType::TypeOf(Box::new(JuliaType::Integer)),
            JuliaType::Integer,
        ];
        let binding = JuliaType::Int64;
        assert_eq!(
            core_type_value_diagonal_candidate_dominates_other(
                &bridge(&fixed_supertype),
                &core_tv,
                &CoreType::from(&binding),
                &hierarchy
            ),
            type_value_diagonal_candidate_dominates_other(
                &fixed_supertype,
                &legacy_tv,
                &binding,
                &hierarchy
            )
        );

        // Type/vector diagonal: f(::Type{T}, ::Vector{T}) where T.
        let tvec_params = vec![
            JuliaType::TypeOf(Box::new(t())),
            JuliaType::VectorOf(Box::new(t())),
        ];
        let tvec_type_params = vec![TypeParam::new("T".to_string())];
        let tvec_core_vars: Vec<CoreTypeVar> =
            tvec_type_params.iter().map(CoreTypeVar::from).collect();
        let legacy_tvec = type_vector_diagonal_pattern(&tvec_params, &tvec_type_params).unwrap();
        let core_tvec =
            core_type_vector_diagonal_pattern(&bridge(&tvec_params), &tvec_core_vars).unwrap();
        assert_eq!(
            (core_tvec.type_slot, core_tvec.vector_slot),
            (legacy_tvec.type_slot, legacy_tvec.vector_slot)
        );
        let other_vec = vec![
            JuliaType::TypeOf(Box::new(JuliaType::Integer)),
            JuliaType::VectorOf(Box::new(JuliaType::Integer)),
        ];
        assert_eq!(
            core_type_vector_diagonal_candidate_dominates_other(
                &bridge(&other_vec),
                &core_tvec,
                &CoreType::from(&binding),
                &hierarchy
            ),
            type_vector_diagonal_candidate_dominates_other(
                &other_vec,
                &legacy_tvec,
                &binding,
                &hierarchy
            )
        );

        // Type/matrix diagonal: f(::Type{T}, ::Matrix{T}) where T.
        let tmat_params = vec![
            JuliaType::TypeOf(Box::new(t())),
            JuliaType::MatrixOf(Box::new(t())),
        ];
        let legacy_tmat = type_matrix_diagonal_pattern(&tmat_params, &tvec_type_params).unwrap();
        let core_tmat =
            core_type_matrix_diagonal_pattern(&bridge(&tmat_params), &tvec_core_vars).unwrap();
        assert_eq!(
            (core_tmat.type_slot, core_tmat.matrix_slot),
            (legacy_tmat.type_slot, legacy_tmat.matrix_slot)
        );

        // Vector diagonal: f(a::Vector{T}, b::Vector{T}) where T<:Number vs
        // independently bounded slots.
        let vec_params = vec![
            JuliaType::VectorOf(Box::new(t())),
            JuliaType::VectorOf(Box::new(t())),
        ];
        let legacy_vd = repeated_vector_typevar_pattern(&vec_params, &type_params).unwrap();
        let core_vd =
            core_repeated_vector_typevar_pattern(&bridge(&vec_params), &core_vars).unwrap();
        assert_eq!(core_vd.slots, legacy_vd.slots);
        assert_eq!(core_vd.upper_bound, legacy_vd.upper_bound);
        let independent = vec![
            JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                "A".to_string(),
                Some("Number".to_string()),
            ))),
            JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                "B".to_string(),
                Some("Number".to_string()),
            ))),
        ];
        assert_eq!(
            core_independent_vector_bounds_are_no_tighter(
                &bridge(&independent),
                &core_vd,
                &hierarchy
            ),
            independent_vector_bounds_are_no_tighter(&independent, &legacy_vd, &hierarchy)
        );
    }

    #[test]
    fn abstract_container_element_extraction_is_structured() {
        // Bare spellings: element unconstrained.
        let bare = JuliaType::Struct("AbstractVector".to_string());
        assert_eq!(abstract_vector_param_type(&bare), Some(CoreType::Any));
        assert_eq!(abstract_matrix_param_type(&bare), None);

        // Parametric element, including a NESTED parametric element that the
        // retired ad-hoc string splitter was prone to mis-parse (Issue #6336).
        let nested = JuliaType::Struct("AbstractVector{Vector{Int64}}".to_string());
        assert_eq!(
            abstract_vector_param_type(&nested),
            Some(CoreType::Struct {
                name: "Vector".to_string(),
                params: vec![CoreType::Primitive(super::super::CorePrimitive::Int64)],
            })
        );

        // Rank-checked `AbstractArray` spellings.
        let rank1 = JuliaType::Struct("AbstractArray{T, 1}".to_string());
        let rank2 = JuliaType::Struct("AbstractArray{T, 2}".to_string());
        let rank_var = JuliaType::Struct("AbstractArray{T, N}".to_string());
        let typevar = CoreType::TypeVar(crate::inference_core::CoreTypeVar {
            name: "T".to_string(),
            lower_bound: None,
            upper_bound: None,
        });
        assert_eq!(abstract_vector_param_type(&rank1), Some(typevar.clone()));
        assert_eq!(abstract_vector_param_type(&rank2), None);
        assert_eq!(abstract_matrix_param_type(&rank2), Some(typevar.clone()));
        assert_eq!(abstract_vector_param_type(&rank_var), Some(typevar.clone()));
        assert_eq!(abstract_matrix_param_type(&rank_var), Some(typevar));

        // Non-container structs never produce an element.
        let other = JuliaType::Struct("Pair{Int64, String}".to_string());
        assert_eq!(abstract_vector_param_type(&other), None);
        assert_eq!(abstract_matrix_param_type(&other), None);
    }

    #[test]
    fn type_param_upper_bound_normalizes_nested_bounds() {
        let plain = TypeParam::with_upper_bound("T".to_string(), "Number".to_string());
        assert_eq!(type_param_upper_bound(&plain), Some("Number"));

        let inline = TypeParam::new("T<:Integer".to_string());
        assert_eq!(type_param_upper_bound(&inline), Some("Integer"));
        assert_eq!(type_param_base_name("T<:Integer"), "T");

        let unbounded = TypeParam::new("T".to_string());
        assert_eq!(type_param_upper_bound(&unbounded), None);
    }

    #[test]
    fn tuple_vararg_int_pattern_dominates_integer_pattern() {
        let hierarchy = StructHierarchy::new();
        let int_pattern = vec![JuliaType::Struct("Vararg{Int64}".to_string())];
        let integer_pattern = vec![JuliaType::Struct("Vararg{Integer}".to_string())];
        let int_exp = expand_tuple_vararg_pattern_for_len(&int_pattern, 2).unwrap();
        let integer_exp = expand_tuple_vararg_pattern_for_len(&integer_pattern, 2).unwrap();
        assert!(tuple_vararg_pattern_dominates(
            &int_exp,
            &integer_exp,
            &hierarchy
        ));
        assert!(!tuple_vararg_pattern_dominates(
            &integer_exp,
            &int_exp,
            &hierarchy
        ));
        assert!(!tuple_vararg_patterns_conflict(
            &int_exp,
            &integer_exp,
            &hierarchy
        ));
    }
}
