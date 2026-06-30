//! CoreType-native port of the legacy dispatch matcher (Issue #6495, stage 2).
//!
//! [`core_signature_match_with_bindings`] is the structured counterpart of
//! [`super::julia_signature_match_with_bindings`]: it consumes the
//! `core_signature`-projected parameter types
//! (`MethodSig::expanded_core_param_types_for_arity`), the call-site argument
//! core types, and the `where` clause as structured [`CoreTypeVar`]s
//! (`MethodSig::core_signature_type_vars`) — no `JuliaType` values and no
//! type-name string surgery (`find('{')` / `split_once("<:")` parsing).
//!
//! Every arm is a one-to-one port of the corresponding `JuliaType` arm,
//! translated through the canonical `JuliaType → CoreType` bridge
//! (`CoreType::from`); where the legacy behavior depended on which *spelling*
//! of a type lowering produced (`JuliaType::VectorOf` vs
//! `JuliaType::Struct("Vector{…}")` — both share one `CoreType` image), this
//! port follows the spelling the canonical inverse (`core_type_to_julia_type`)
//! reconstructs, which is pinned corpus-wide by
//! `base_method_signature_accessors_are_canonical_issue_6495`. Decision
//! equality with the legacy matcher was pinned during the migration by
//! `compile::cache::tests::base_method_core_dispatch_match_parity_issue_6495`;
//! production dispatch now calls this module directly (Issue #6495).

use std::collections::HashMap;

use crate::inference_core::{
    CoreAbstract, CorePrimitive, CoreSubtypeEngine, CoreType, CoreTypeVar, CoreValueParam,
};

/// Check if CoreType method parameters match argument core types while
/// tracking `where` type-variable bindings.
///
/// Structured counterpart of
/// [`super::julia_signature_match_with_bindings`]; `param_types` must already
/// be arity-normalized (`MethodSig::expanded_core_param_types_for_arity`).
pub fn core_signature_match_with_bindings(
    param_types: &[CoreType],
    arg_types: &[CoreType],
    type_vars: &[CoreTypeVar],
) -> Option<usize> {
    let mut bindings: HashMap<String, CoreType> = HashMap::new();

    for (param_ty, arg_ty) in param_types.iter().zip(arg_types.iter()) {
        if !core_dispatch_pattern_matches(param_ty, arg_ty, type_vars, &mut bindings) {
            return None;
        }
    }

    if !bindings.is_empty() && !check_diagonal_rule_for_params(param_types, &bindings) {
        return None;
    }
    Some(bindings.len())
}

/// CoreType-native port of `score_julia_signature_with_binding_count`
/// (Issue #6495, stage 4).
///
/// The legacy scorer already worked on `CoreType::from` projections of every
/// (param, arg) pair internally; this entry consumes the canonical
/// `core_signature` projections directly. The three `JuliaType`-keyed pieces
/// are ported on the canonical-image doctrine (see module docs):
/// [`core_value_param_base_specificity`], and the two `Type{…}` singleton
/// helpers. Score equality with the legacy scorer over the whole Base corpus
/// is pinned by
/// `compile::cache::tests::base_method_core_dispatch_score_parity_issue_6495`.
pub fn score_core_signature_with_binding_count(
    param_types: &[CoreType],
    arg_types: &[CoreType],
    binding_count: usize,
    has_varargs: bool,
    fixed_varargs: bool,
) -> super::JuliaSignatureScore {
    let fixed_param_count = param_types.len().min(arg_types.len());
    let base_score: u32 = param_types
        .iter()
        .take(fixed_param_count)
        .map(core_value_param_base_specificity)
        .sum();

    let match_quality_bonus: i32 = param_types
        .iter()
        .take(fixed_param_count)
        .zip(arg_types.iter().take(fixed_param_count))
        .map(|(param_core, arg_core)| {
            let pattern_score = param_core.dispatch_pattern_score(arg_core);
            let exact_bonus_eligible = (param_core.is_builtin_dispatch_primitive()
                && arg_core.is_builtin_dispatch_primitive())
                || (matches!(param_core, CoreType::TypeOf(_))
                    && matches!(arg_core, CoreType::TypeOf(_)))
                || (matches!(param_core, CoreType::Struct { .. })
                    && matches!(arg_core, CoreType::Struct { .. }));

            if core_is_type_any_non_exact_singleton_match(param_core, arg_core) {
                super::TYPE_ANY_NON_EXACT_SINGLETON_PENALTY
            } else if exact_bonus_eligible {
                if param_core == arg_core {
                    super::EXACT_PRIMITIVE_MATCH_BONUS
                } else if core_is_typevar_singleton_match(param_core, arg_core) {
                    super::PARAMETRIC_PATTERN_MATCH_BONUS
                } else if pattern_score == 3 {
                    super::PARAMETRIC_PATTERN_MATCH_BONUS
                        + i32::from(matches!(param_core, CoreType::TypeOf(_)))
                } else {
                    0
                }
            } else if matches!(arg_core, CoreType::Any) && !matches!(param_core, CoreType::Any) {
                super::ANY_ARG_SPECIFIC_PARAM_PENALTY
            } else if core_is_typevar_singleton_match(param_core, arg_core) {
                super::PARAMETRIC_PATTERN_MATCH_BONUS
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

    super::JuliaSignatureScore {
        binding_count,
        fixed_param_count,
        score,
    }
}

/// Port of `value_param_base_specificity` (Issues #5582/#5375): base
/// specificity of a value-position parameter.
///
/// The legacy `JuliaType::AbstractUser(_, Some(parent))` arm gated the
/// parent-specificity bonus on `JuliaType::from_name(parent)` returning
/// `Some`; the stored parent image already is `from_julia_name(parent)`, and
/// for every name in `from_name`'s domain the two bridges agree
/// (`CoreType::from(&from_name(p).unwrap()) == from_julia_name(p)`), so the
/// bonus reads the parent image directly, gated on
/// [`parent_in_from_name_domain`]. The `TypeVar`-bound arm already read
/// `CoreType::from(ty)` in the legacy scorer, so it ports verbatim.
fn core_value_param_base_specificity(core: &CoreType) -> u32 {
    if let CoreType::AbstractUser { name, .. } = core {
        // A value-parameterized abstract supertype (`AbsM{2,2,T}`) carries its
        // parameters in the name (Issue #7960) and is strictly more specific
        // than the bare family (`AbsM`); re-parse the carried parameters so the
        // specialization outranks the generic `::AbsM` method instead of tying
        // it (and then losing the fewest-`where`-params tie-break). Only the
        // value-parameter spelling carries `{...}` here — type-only parametric
        // abstracts keep their bare family name — so this never disturbs the
        // historical bare-family forms.
        if name.contains('{') {
            return u32::from(CoreType::from_julia_name(name).specificity());
        }
    }
    if let CoreType::AbstractUser {
        parent: Some(parent),
        ..
    } = core
    {
        if matches!(parent.as_ref(), CoreType::Any) {
            return 1;
        }
        if parent_in_from_name_domain(parent) {
            return u32::from(parent.specificity()).saturating_add(1);
        }
    }

    if let CoreType::TypeVar(var) = core {
        if let Some(bound) = &var.upper_bound {
            // `T<:Any` is equivalent to an unbounded `T` (≡ `Any`); it must
            // not outrank an untyped parameter, so keep it at 0.
            if matches!(bound.as_ref(), CoreType::Any) {
                return 0;
            }
            // Floor the bound at 1 so a structurally narrow bound whose
            // `specificity()` collapses to 0 still ranks strictly above an
            // untyped `Any` parameter, then add 1 to compensate the
            // single-binding `type_reuse_bonus`.
            return u32::from(bound.specificity().max(1)).saturating_add(1);
        }
    }
    u32::from(core.specificity())
}

/// Whether an `AbstractUser` parent image corresponds to a name
/// `JuliaType::from_name` resolves (the legacy bonus gate). Valid abstract
/// parents are builtin abstract names or user abstracts; of the builtin
/// abstract images, `from_name` does NOT know `AbstractDict` / `AbstractSet`
/// / `AbstractUnitRange` / `Builtin` (legacy fell through to the whole-type
/// specificity for them), and a user-abstract / parametric parent images as
/// `Named` / `TypeVar` / `Struct` (all outside `from_name`'s domain for
/// valid parent spellings).
fn parent_in_from_name_domain(parent: &CoreType) -> bool {
    match parent {
        CoreType::Abstract(a) => !matches!(
            a,
            CoreAbstract::AbstractDict
                | CoreAbstract::AbstractSet
                | CoreAbstract::AbstractUnitRange
                | CoreAbstract::Builtin
        ),
        CoreType::Primitive(_) => true,
        _ => false,
    }
}

/// Port of `is_type_any_non_exact_singleton_match`: a `Type{Any}` parameter
/// matched by a non-`Any` type-object argument.
fn core_is_type_any_non_exact_singleton_match(param: &CoreType, arg: &CoreType) -> bool {
    matches!(
        (param, arg),
        (CoreType::TypeOf(param_inner), CoreType::TypeOf(arg_inner))
            if matches!(param_inner.as_ref(), CoreType::Any)
                && !matches!(arg_inner.as_ref(), CoreType::Any)
    )
}

/// Port of `is_typevar_singleton_match`: a `Type{T}` parameter (type-variable
/// inner) matched by a type-object argument.
fn core_is_typevar_singleton_match(param: &CoreType, arg: &CoreType) -> bool {
    matches!(
        (param, arg),
        (CoreType::TypeOf(param_inner), CoreType::TypeOf(_))
            if matches!(param_inner.as_ref(), CoreType::TypeVar(_))
    )
}

/// Port of `julia_type_pattern_matches`, arm for arm.
fn core_dispatch_pattern_matches(
    param_ty: &CoreType,
    arg_ty: &CoreType,
    type_vars: &[CoreTypeVar],
    bindings: &mut HashMap<String, CoreType>,
) -> bool {
    // Tuple patterns, including a trailing unbounded `Vararg{T}` slot that
    // binds all remaining argument elements (Issue #4857). The legacy arm
    // fired on `TupleOf`/`TupleOf` pairs; the canonical bridge maps those to
    // `CoreType::Tuple` and the `Struct("Vararg{T}")` trailing marker to
    // `CoreType::Vararg`.
    if let (CoreType::Tuple(param_elems), CoreType::Tuple(arg_elems)) = (param_ty, arg_ty) {
        if matches!(param_elems.last(), Some(CoreType::VarargLen { .. })) {
            // `NTuple{N, T}` image (the `JT::Struct("NTuple{N, T}")` spelling
            // lowering produces): the legacy matcher had NO structural
            // `TupleOf` arm for this spelling — it decided the pair on the
            // engine subtype path (`is_subtype_of_parametric` →
            // `is_subtype_of` → CoreSubtypeEngine over the same images,
            // recording no bindings). Fall through to that path instead of
            // short-circuiting on a length mismatch here (Issue #6495
            // stage-3 regression: `h(xs::NTuple{N,T}) where {N,T}` stopped
            // matching `(1, 2, 3)`).
        } else if let Some(CoreType::Vararg(vararg_elem)) = param_elems.last() {
            let lead_count = param_elems.len() - 1;
            if arg_elems.len() < lead_count {
                return false;
            }
            let leads_ok = param_elems[..lead_count]
                .iter()
                .zip(arg_elems.iter())
                .all(|(param, arg)| core_dispatch_pattern_matches(param, arg, type_vars, bindings));
            if !leads_ok {
                return false;
            }
            return arg_elems[lead_count..]
                .iter()
                .all(|arg| core_dispatch_pattern_matches(vararg_elem, arg, type_vars, bindings));
        } else {
            return param_elems.len() == arg_elems.len()
                && param_elems
                    .iter()
                    .zip(arg_elems.iter())
                    .all(|(param, arg)| {
                        core_dispatch_pattern_matches(param, arg, type_vars, bindings)
                    });
        }
    }

    // Type-variable parameter. This merges the legacy `JuliaType::TypeVar` arm
    // and the `JuliaType::Struct`-as-typevar arm (#5314): both spellings share
    // the `CoreType::TypeVar` image (the context-free bridge classifies an
    // uppercase-letter[+digits] name as a type variable).
    if let CoreType::TypeVar(var) = param_ty {
        let base = type_var_base_name(&var.name);
        let declared = find_type_var(type_vars, base);
        if declared.is_none() && var.upper_bound.is_none() && base != "_" {
            // Issue #5314: the name is NOT a method `where` variable and has
            // no bound, so (per the only lowering spelling that produces this
            // shape: `JuliaType::Struct("Q")`) it names a concrete struct
            // type. A struct is a final leaf type: reject primitive
            // arguments here, and let non-primitive arguments keep the
            // ordinary fall-through matching below.
            if core_arg_is_dispatch_primitive(arg_ty) {
                return false;
            }
        } else {
            let upper = var
                .upper_bound
                .as_deref()
                .or_else(|| declared.and_then(|d| d.upper_bound.as_deref()));
            if base == "_" {
                let lower = var
                    .lower_bound
                    .as_deref()
                    .or_else(|| declared.and_then(|d| d.lower_bound.as_deref()));
                return upper.is_none_or(|bound| engine_is_subtype(arg_ty, bound))
                    && lower.is_none_or(|bound| engine_is_subtype(bound, arg_ty));
            }
            if let Some(bound_pattern) = parametric_type_var_bound_pattern(upper, type_vars) {
                // Issue #5383 sub-case 2: `T<:Vector{S}` — match the argument
                // structurally against the parametric bound (binding `S`),
                // then bind `T` itself without a redundant opaque check.
                return core_dispatch_pattern_matches(bound_pattern, arg_ty, type_vars, bindings)
                    && bind_or_check_type_var(base, None, None, arg_ty, bindings);
            }
            let upper = covariant_applicability_upper_bound(upper, type_vars);
            return bind_or_check_type_var(base, upper, None, arg_ty, bindings);
        }
    }

    // A `Named` parameter that names a method `where` variable (a var name the
    // context-free bridge does not classify as variable-like, e.g. `TX`).
    // Mirrors the legacy `Struct`-as-typevar lookup; a `Named` parameter that
    // is NOT a `where` variable keeps the #5314 struct-leaf rule.
    if let CoreType::Named(name) = param_ty {
        if let Some(declared) = find_type_var(type_vars, name) {
            let upper = declared.upper_bound.as_deref();
            if let Some(bound_pattern) = parametric_type_var_bound_pattern(upper, type_vars) {
                return core_dispatch_pattern_matches(bound_pattern, arg_ty, type_vars, bindings)
                    && bind_or_check_type_var(name, None, None, arg_ty, bindings);
            }
            let upper = covariant_applicability_upper_bound(upper, type_vars);
            return bind_or_check_type_var(name, upper, None, arg_ty, bindings);
        }
        if core_arg_is_dispatch_primitive(arg_ty) {
            return false;
        }
    }

    // `Type{…}` parameter against a type-object argument. `Type{T}` binds `T`
    // invariantly, so both the upper and lower bounds of `T` are enforced
    // here (Issue #5051).
    if let CoreType::TypeOf(inner_param) = param_ty {
        if let CoreType::TypeOf(inner_arg) = arg_ty {
            match inner_param.as_ref() {
                CoreType::TypeVar(var) => {
                    let base = type_var_base_name(&var.name);
                    let declared = find_type_var(type_vars, base);
                    return bind_or_check_type_var(
                        base,
                        var.upper_bound
                            .as_deref()
                            .or_else(|| declared.and_then(|d| d.upper_bound.as_deref())),
                        declared.and_then(|d| d.lower_bound.as_deref()),
                        inner_arg,
                        bindings,
                    );
                }
                CoreType::Named(name) => {
                    if let Some(declared) = find_type_var(type_vars, name) {
                        return bind_or_check_type_var(
                            name,
                            declared.upper_bound.as_deref(),
                            declared.lower_bound.as_deref(),
                            inner_arg,
                            bindings,
                        );
                    }
                }
                _ => {}
            }
            if core_mentions_type_vars(inner_param, type_vars) {
                if type_object_inner_family_mismatch(inner_param, inner_arg) {
                    return false;
                }
                if let Some(extracted) = extract_type_bindings(inner_arg, inner_param, type_vars) {
                    return extracted.into_iter().all(|(name, bound_ty)| {
                        bind_or_check_type_var(&name, None, None, &bound_ty, bindings)
                    });
                }
            }
            if matches!(inner_param.as_ref(), CoreType::Any) {
                return true;
            }
            return inner_arg.as_ref() == inner_param.as_ref();
        }
    }

    // A type-object argument only satisfies type-shaped parameters. A `Union`
    // is allowed through to the subtype fallthrough below: a type-object
    // argument matches `Union{Type{A}, Type{B}, ...}` when one member is its
    // `Type{...}` (the BigFloat/BigInt/Rational `promote_rule` methods are
    // written with `Union{Type{...}, ...}` arguments, Issue #5070), and the
    // `core_is_subtype_full` Union arm correctly rejects a type object against a
    // non-type-shaped `Union{Int64, Float64}` (Issue #6781). Non-type-object
    // arguments already reach that fallthrough, so only the type-object case
    // needed unblocking.
    if matches!(arg_ty, CoreType::TypeOf(_))
        && !matches!(
            param_ty,
            CoreType::Any
                | CoreType::Abstract(CoreAbstract::Type)
                | CoreType::Abstract(CoreAbstract::DataType)
                | CoreType::TypeOf(_)
                | CoreType::Union(_)
        )
    {
        return false;
    }

    // Nested diagonal binding (Issue #5050): a parametric parameter such as
    // `x::Vector{T}` mentions a `where` type variable below the top level.
    // Record the inner binding(s) so a repeated occurrence of the same
    // variable is rejected by `bind_or_check_type_var` (and the post-match
    // diagonal rule can see them); the match decision itself still rests on
    // the parametric subtype check.
    if core_mentions_type_vars(param_ty, type_vars)
        && core_is_subtype_of_parametric(arg_ty, param_ty, type_vars)
    {
        if let Some(extracted) = extract_type_bindings(arg_ty, param_ty, type_vars) {
            return extracted.into_iter().all(|(name, bound_ty)| {
                let upper = find_type_var(type_vars, &name).and_then(|d| d.upper_bound.as_deref());
                bind_or_check_type_var(&name, upper, None, &bound_ty, bindings)
            });
        }
        return true;
    }

    core_is_subtype_of_parametric(arg_ty, param_ty, type_vars)
}

/// Engine-backed `<:` query, mirroring the legacy matcher's direct
/// `CoreSubtypeEngine` use in bound checks.
fn engine_is_subtype(actual: &CoreType, expected: &CoreType) -> bool {
    CoreSubtypeEngine::new().is_subtype(actual, expected)
}

/// Port of `JuliaType::is_subtype_of` for canonical core images: equality /
/// `Bottom` / `Union` decomposition, then the shared engine, then the legacy
/// post-engine fallback arms translated to core shapes.
fn core_is_subtype_full(actual: &CoreType, expected: &CoreType) -> bool {
    if actual == expected {
        return true;
    }
    if matches!(actual, CoreType::Bottom) {
        return true;
    }
    if let CoreType::Union(arms) = actual {
        return arms.iter().all(|arm| core_is_subtype_full(arm, expected));
    }
    if let CoreType::Union(arms) = expected {
        return arms.iter().any(|arm| core_is_subtype_full(actual, arm));
    }
    if engine_is_subtype(actual, expected) {
        return true;
    }
    core_is_subtype_fallback(actual, expected)
}

/// The legacy `JuliaType::is_subtype_of` post-engine fallback arms, keyed on
/// the canonical core image of each legacy `other` shape.
fn core_is_subtype_fallback(actual: &CoreType, expected: &CoreType) -> bool {
    match expected {
        CoreType::Any => true,
        CoreType::Bottom => false,
        // `Type{T}` is invariant in its (concrete) parameter; only the
        // covariant `Type{<:B}` spelling reduces to `A <: B` (Issue #5068).
        CoreType::TypeOf(inner) => {
            let CoreType::TypeOf(actual_inner) = actual else {
                return false;
            };
            match inner.as_ref() {
                CoreType::TypeVar(var) => match var.upper_bound.as_deref() {
                    None => true,
                    Some(bound) => {
                        bound_unknown_to_from_name(bound)
                            || core_is_subtype_full(actual_inner, bound)
                    }
                },
                concrete => {
                    core_is_subtype_full(actual_inner, concrete)
                        && core_is_subtype_full(concrete, actual_inner)
                }
            }
        }
        CoreType::Abstract(CoreAbstract::AbstractArray) => {
            core_array_like(actual)
                || core_array_projection(actual).is_some()
                || core_range_projection(actual).is_some()
        }
        CoreType::Tuple(expected_elems) => {
            let CoreType::Tuple(actual_elems) = actual else {
                return false;
            };
            actual_elems.len() == expected_elems.len()
                && actual_elems
                    .iter()
                    .zip(expected_elems.iter())
                    .all(|(a, e)| core_is_subtype_full(a, e))
        }
        CoreType::AbstractUser { name, .. } => {
            if let CoreType::AbstractUser {
                name: actual_name,
                parent: actual_parent,
            } = actual
            {
                if actual_name == name {
                    return true;
                }
                if let Some(parent) = actual_parent {
                    if parent.as_ref() == &CoreType::from_julia_name(name) {
                        return true;
                    }
                }
            }
            let abstract_core = CoreType::from_julia_name(name);
            if matches!(abstract_core, CoreType::Abstract(_)) {
                return engine_is_subtype(actual, &abstract_core);
            }
            false
        }
        CoreType::TypeVar(var) => match var.upper_bound.as_deref() {
            None => true,
            Some(bound) => bound_unknown_to_from_name(bound) || core_is_subtype_full(actual, bound),
        },
        CoreType::UnionAll { var, body } => match var.upper_bound.as_deref() {
            None => core_is_subtype_full(actual, body),
            Some(bound) => {
                if bound_unknown_to_from_name(bound) {
                    core_is_subtype_full(actual, body)
                } else {
                    core_is_subtype_full(actual, bound) && core_is_subtype_full(actual, body)
                }
            }
        },
        CoreType::Struct {
            name: expected_name,
            params: expected_params,
        } => {
            let expected_base = strip_module_prefix(expected_name);
            // Legacy `Array`/`Tuple`/`NamedTuple` bare-family arms.
            if expected_params.is_empty() {
                match expected_base {
                    "Array" => {
                        return core_array_like(actual);
                    }
                    "Tuple" => {
                        return matches!(actual, CoreType::Tuple(_))
                            || matches!(actual, CoreType::Struct { name, params } if name == "Tuple" && params.is_empty());
                    }
                    "NamedTuple" => {
                        return matches!(actual, CoreType::NamedTuple(_))
                            || matches!(actual, CoreType::Struct { name, params } if name == "NamedTuple" && params.is_empty());
                    }
                    _ => {}
                }
            }
            // Parametric abstract array pattern (`AbstractArray{T,N}` family):
            // the legacy arm requires dims to match and elements to be EQUAL.
            if let Some((expected_elem, expected_dim)) =
                abstract_array_family_projection(expected_base, expected_params)
            {
                return core_abstract_array_projection(actual).is_some_and(
                    |(actual_elem, actual_dim)| {
                        array_dims_match(actual_dim, expected_dim) && actual_elem == expected_elem
                    },
                );
            }
            // Concrete `Array{T[,N]}` pattern: dims match + element equality.
            if expected_base == "Array" && !expected_params.is_empty() {
                if let (Some((actual_elem, actual_dim)), Some((expected_elem, expected_dim))) = (
                    core_array_projection(actual),
                    array_family_projection(expected_base, expected_params),
                ) {
                    return array_dims_match(actual_dim, expected_dim)
                        && actual_elem == expected_elem;
                }
            }
            // Vector/Matrix element equality (legacy `VectorOf`/`MatrixOf`
            // arms; the bridge maps both spellings to `Struct`).
            if matches!(expected_base, "Vector" | "Matrix") && expected_params.len() == 1 {
                if let CoreType::Struct {
                    name: actual_name,
                    params: actual_params,
                } = actual
                {
                    return strip_module_prefix(actual_name) == expected_base
                        && actual_params.len() == 1
                        && actual_params[0] == expected_params[0];
                }
                return false;
            }
            // Parametric base <-> bare base, both directions
            // (`Foo{Int64} <: Foo` and `Foo <: Foo{Int64}`). The legacy arms
            // required `self` (actual) to be a genuine `JT::Struct`, so a
            // dedicated-variant image (`Struct{Dict,[]}` = `JT::Dict`,
            // `Struct{Array,[]}` = `JT::Array`, …) must NOT satisfy them — the
            // legacy `is_subtype_of` has no `Dict`/`Set` arm and returns false
            // for `JT::Dict <: JT::Struct("Dict{K,V}")`.
            if core_maps_to_julia_struct(actual) {
                if let CoreType::Struct {
                    name: actual_name,
                    params: actual_params,
                } = actual
                {
                    let actual_base = strip_module_prefix(actual_name);
                    if actual_base == expected_base
                        && (actual_params.is_empty() != expected_params.is_empty())
                    {
                        return true;
                    }
                }
            }
            // Bare-name actual vs parametric expected of the same family
            // (legacy "Reverse: Foo <: Foo{Int64}"). A bare struct name the
            // bridge cannot structure (`JT::Struct("Val")`, e.g. the runtime
            // value type of a `Val{3}()` instance with the value parameter
            // erased) images as `Named`, not `Struct`, so the arm above never
            // fires for it (Issue #6495 stage-3 regression: `f(::Val{N})
            // where N` stopped matching a bare `Val` argument).
            if let CoreType::Named(actual_name) = actual {
                return strip_module_prefix(actual_name) == expected_base
                    && !expected_params.is_empty();
            }
            false
        }
        CoreType::Named(expected_name) => {
            // Parametric actual vs the bare family name (legacy "Parametric
            // struct: Foo{Int64} <: Foo" with a `JT::Struct("Foo")` expected
            // imaging as `Named`). Restricted to genuine `JT::Struct` images,
            // exactly like the parametric-base arm above.
            if core_maps_to_julia_struct(actual) {
                if let CoreType::Struct {
                    name: actual_name,
                    params: actual_params,
                } = actual
                {
                    return strip_module_prefix(actual_name) == strip_module_prefix(expected_name)
                        && !actual_params.is_empty();
                }
            }
            // Bare/qualified name vs bare/qualified name of the same family
            // (Issues #7263 / #7265). A user/package struct whose family is not a
            // built-in (`is_known_struct_family` is false) images as `Named`, not
            // `Struct`, so a within-module call (`ncategories(d)` with `d`'s
            // inferred type the bare `Named("Categorical")`) was never matched
            // against the method's module-qualified param
            // `Named("Distributions.Categorical")`. Module qualification is not
            // part of the type identity, so compare the stripped family names.
            if let CoreType::Named(actual_name) = actual {
                return strip_module_prefix(actual_name) == strip_module_prefix(expected_name);
            }
            false
        }
        _ => false,
    }
}

/// Legacy `JuliaType::from_name(bound)` returned `None` for user-defined type
/// names, making such bounds unconstraining in the `is_subtype_of` fallback
/// arms. The canonical core image of such a name is `Named`.
fn bound_unknown_to_from_name(bound: &CoreType) -> bool {
    matches!(bound, CoreType::Named(_))
}

/// Whether `actual` belongs to the concrete array family (legacy
/// `Array | VectorOf | MatrixOf | is_array_struct`).
fn core_array_like(actual: &CoreType) -> bool {
    match actual {
        CoreType::Abstract(CoreAbstract::AbstractArray) => true,
        CoreType::Struct { name, .. } => {
            matches!(strip_module_prefix(name), "Array" | "Vector" | "Matrix")
        }
        _ => false,
    }
}

/// Port of `is_subtype_of_parametric`: subtype check extended with method
/// `where`-parameter awareness for parametric matching.
fn core_is_subtype_of_parametric(
    actual: &CoreType,
    expected: &CoreType,
    type_vars: &[CoreTypeVar],
) -> bool {
    if core_is_subtype_full(actual, expected) {
        return true;
    }

    // `Any` arguments match primitive-ish parameters for compile-time
    // dispatch (runtime validates the actual type); parametric struct
    // parameters intentionally stay unmatched so the generic fallback wins.
    if matches!(actual, CoreType::Any)
        && (expected.is_builtin_dispatch_primitive_or_abstract_numeric()
            || matches!(expected, CoreType::Any))
    {
        return true;
    }

    // `expected` names a method `where` parameter (legacy `Struct(sn)` with
    // an exact `tp.name == sn` lookup; the bridge images such names as
    // `Named` when they are not variable-like).
    if let CoreType::Named(name) = expected {
        if let Some(declared) = type_vars.iter().find(|v| &v.name == name) {
            if let Some(upper) = declared.upper_bound.as_deref() {
                if !bound_unknown_to_from_name(upper) && !core_is_subtype_full(actual, upper) {
                    return false;
                }
            }
            if let Some(lower) = declared.lower_bound.as_deref() {
                if !bound_unknown_to_from_name(lower) && !core_is_subtype_full(lower, actual) {
                    return false;
                }
            }
            return true;
        }
    }

    // `Type{…}` parameter with a `DataType` argument.
    if matches!(expected, CoreType::TypeOf(_))
        && matches!(actual, CoreType::Abstract(CoreAbstract::DataType))
    {
        return true;
    }

    // Array{T} / Array{T,N} rank-aware matching (Vector/Matrix aliases
    // included).
    if let (Some((actual_elem, actual_dim)), Some((expected_elem, expected_dim))) = (
        core_array_projection(actual),
        core_array_projection(expected),
    ) {
        return array_dims_match(actual_dim, expected_dim)
            && array_elem_matches_parametric(&actual_elem, &expected_elem, type_vars);
    }

    // Array <-> Vector interop (legacy `Array` vs `VectorOf` arms). The bare
    // `Array` image (`Struct{Array,[]}` = `JT::Array`) and the `Vector{T}`
    // image (`JT::VectorOf`) are NOT `JT::Struct`, so the legacy parametric
    // struct arm below never fired for them — this interop must precede it.
    let bare_array = |ty: &CoreType| matches!(ty, CoreType::Struct { name, params } if strip_module_prefix(name) == "Array" && params.is_empty());
    let vector_of = |ty: &CoreType| matches!(ty, CoreType::Struct { name, params } if strip_module_prefix(name) == "Vector" && params.len() == 1);
    if (bare_array(actual) && vector_of(expected)) || (vector_of(actual) && bare_array(expected)) {
        return true;
    }

    // Parametric struct matching: `Complex{Float64}` vs `Complex{T}`. The
    // legacy arm fired only on `(JT::Struct, JT::Struct)`, so a side whose
    // canonical image is a dedicated variant (bare `Array`/`Dict`/`Set`/…,
    // `Vector{T}`/`Matrix{T}`) must NOT enter here.
    if let (
        CoreType::Struct {
            name: actual_name,
            params: actual_params,
        },
        CoreType::Struct {
            name: expected_name,
            params: expected_params,
        },
    ) = (actual, expected)
    {
        if strip_module_prefix(actual_name) == strip_module_prefix(expected_name)
            && actual_params.is_empty()
            && !expected_params.is_empty()
            && core_array_projection(actual).is_none()
            && !matches!(
                strip_module_prefix(actual_name),
                "Array"
                    | "Vector"
                    | "Matrix"
                    | "AbstractArray"
                    | "AbstractVector"
                    | "AbstractMatrix"
                    | "NamedTuple"
            )
            && expected_params
                .iter()
                .all(|param| exact_type_var_for_pattern(param, type_vars).is_some())
        {
            return true;
        }
        if core_maps_to_julia_struct(actual) && core_maps_to_julia_struct(expected) {
            if strip_module_prefix(actual_name) != strip_module_prefix(expected_name) {
                return false;
            }
            if expected_params.is_empty() && !actual_params.is_empty() {
                return true;
            }
            if actual_params.len() < expected_params.len() {
                return false;
            }
            for (actual_param, expected_param) in actual_params.iter().zip(expected_params.iter()) {
                if let Some(declared) = exact_type_var_for_pattern(expected_param, type_vars) {
                    if let Some(upper) = declared.upper_bound.as_deref() {
                        if !bound_unknown_to_from_name(upper)
                            && !core_is_subtype_full(actual_param, upper)
                        {
                            return false;
                        }
                    }
                    if let Some(lower) = declared.lower_bound.as_deref() {
                        if !bound_unknown_to_from_name(lower)
                            && !core_is_subtype_full(lower, actual_param)
                        {
                            return false;
                        }
                    }
                } else if !core_parametric_slot_matches(actual_param, expected_param, type_vars) {
                    return false;
                }
            }
            return true;
        }
    }

    // Tuple parametric matching, including a trailing unbounded vararg
    // pattern (Issue #4857).
    if let (CoreType::Tuple(actual_elems), CoreType::Tuple(expected_elems)) = (actual, expected) {
        if let Some(CoreType::Vararg(vararg_elem)) = expected_elems.last() {
            let lead_count = expected_elems.len() - 1;
            if actual_elems.len() < lead_count {
                return false;
            }
            let leads_ok = actual_elems
                .iter()
                .zip(expected_elems[..lead_count].iter())
                .all(|(a, e)| core_is_subtype_of_parametric(a, e, type_vars));
            if !leads_ok {
                return false;
            }
            return actual_elems[lead_count..]
                .iter()
                .all(|a| core_is_subtype_of_parametric(a, vararg_elem, type_vars));
        }
        if actual_elems.len() != expected_elems.len() {
            return false;
        }
        return actual_elems
            .iter()
            .zip(expected_elems.iter())
            .all(|(a, e)| core_is_subtype_of_parametric(a, e, type_vars));
    }

    false
}

/// Port of `array_elem_matches_parametric`: a method `where` variable in the
/// element pattern accepts any argument element; otherwise require equality,
/// an extractable binding, or a parametric subtype.
fn array_elem_matches_parametric(
    actual_elem: &CoreType,
    pattern_elem: &CoreType,
    type_vars: &[CoreTypeVar],
) -> bool {
    if exact_type_var_for_pattern(pattern_elem, type_vars).is_some() {
        return true;
    }
    actual_elem == pattern_elem
        || extract_type_bindings(actual_elem, pattern_elem, type_vars).is_some()
        || core_is_subtype_of_parametric(actual_elem, pattern_elem, type_vars)
}

fn core_parametric_slot_matches(
    actual: &CoreType,
    pattern: &CoreType,
    type_vars: &[CoreTypeVar],
) -> bool {
    if actual == pattern {
        return true;
    }
    if let CoreType::TypeVar(var) = pattern {
        if exact_type_var_for_pattern(pattern, type_vars).is_none() {
            if let Some(upper) = var.upper_bound.as_deref() {
                return bound_unknown_to_from_name(upper) || core_is_subtype_full(actual, upper);
            }
        }
    }
    core_is_subtype_of_parametric(actual, pattern, type_vars)
}

/// Port of `JuliaType::extract_type_bindings`: extract `where`-variable
/// bindings when `actual` matches the parametric `pattern`.
fn extract_type_bindings(
    actual: &CoreType,
    pattern: &CoreType,
    type_vars: &[CoreTypeVar],
) -> Option<HashMap<String, CoreType>> {
    let mut bindings = HashMap::new();

    let actual_projection = if pattern_uses_abstract_array_projection(pattern) {
        core_abstract_array_projection(actual)
    } else {
        core_array_projection(actual)
    };
    if let (Some((actual_elem, actual_dim)), Some((pattern_elem, pattern_dim))) =
        (actual_projection, core_pattern_array_projection(pattern))
    {
        if !array_dims_match(actual_dim, pattern_dim) {
            return None;
        }
        if let Some(declared) = exact_type_var_for_pattern(&pattern_elem, type_vars) {
            bindings.insert(declared.name.clone(), actual_elem);
            return Some(bindings);
        }
        if let Some(nested) = extract_type_bindings(&actual_elem, &pattern_elem, type_vars) {
            bindings.extend(nested);
            return Some(bindings);
        }
        if actual_elem == pattern_elem {
            return Some(bindings);
        }
        return None;
    }

    if let CoreType::Union(members) = pattern {
        for member in members {
            if let Some(extracted) = extract_type_bindings(actual, member, type_vars) {
                return Some(extracted);
            }
        }
        return None;
    }

    // Struct-to-struct matching.
    if let (
        CoreType::Struct {
            name: actual_name,
            params: actual_params,
        },
        CoreType::Struct {
            name: pattern_name,
            params: pattern_params,
        },
    ) = (actual, pattern)
    {
        if strip_module_prefix(actual_name) != strip_module_prefix(pattern_name)
            || actual_params.len() < pattern_params.len()
        {
            return None;
        }
        for (actual_param, pattern_param) in actual_params.iter().zip(pattern_params.iter()) {
            if let Some(declared) = exact_type_var_for_pattern(pattern_param, type_vars) {
                if let Some(upper) = declared.upper_bound.as_deref() {
                    if !bound_unknown_to_from_name(upper)
                        && !core_is_subtype_full(actual_param, upper)
                    {
                        return None;
                    }
                }
                bindings.insert(declared.name.clone(), actual_param.clone());
            } else if !core_parametric_slot_matches(actual_param, pattern_param, type_vars) {
                return None;
            }
        }
        return Some(bindings);
    }

    // Tuple patterns: trailing unbounded `Vararg{T}` binds `T` to the join of
    // the trailing element types (Issue #4857); fixed slots bind positionally.
    if let (CoreType::Tuple(actual_elems), CoreType::Tuple(pattern_elems)) = (actual, pattern) {
        if let Some(CoreType::Vararg(vararg_elem)) = pattern_elems.last() {
            let lead_count = pattern_elems.len() - 1;
            if actual_elems.len() < lead_count {
                return None;
            }
            for (actual_elem, pattern_elem) in
                actual_elems.iter().zip(pattern_elems[..lead_count].iter())
            {
                let extracted = extract_type_bindings(actual_elem, pattern_elem, type_vars)?;
                merge_tuple_bindings(&mut bindings, extracted)?;
            }
            if let Some(joined) = join_types(&actual_elems[lead_count..]) {
                let extracted = extract_type_bindings(&joined, vararg_elem, type_vars)?;
                merge_tuple_bindings(&mut bindings, extracted)?;
            } else if vararg_type_var_unbound(vararg_elem, type_vars) {
                // Zero trailing elements: an unbound `T` in the vararg element
                // cannot be determined.
                return None;
            }
            for (var_name, bound_ty) in &bindings {
                if !satisfies_diagonal_rule(var_name, bound_ty, pattern) {
                    return None;
                }
            }
            return Some(bindings);
        }
        if actual_elems.len() != pattern_elems.len() {
            return None;
        }
        for (actual_elem, pattern_elem) in actual_elems.iter().zip(pattern_elems.iter()) {
            let extracted = extract_type_bindings(actual_elem, pattern_elem, type_vars)?;
            merge_tuple_bindings(&mut bindings, extracted)?;
        }
        for (var_name, bound_ty) in &bindings {
            if !satisfies_diagonal_rule(var_name, bound_ty, pattern) {
                return None;
            }
        }
        return Some(bindings);
    }

    // Anonymous bounded variables (`<:Pairs{K,V,I,A}`): the anonymous slot
    // itself does not bind, but its bound may mention method `where`
    // parameters that must be recovered.
    if let CoreType::TypeVar(var) = pattern {
        if !type_vars.iter().any(|v| v.name == var.name) {
            if let Some(bound) = var.upper_bound.as_deref() {
                return extract_type_bindings(actual, bound, type_vars);
            }
        }
        if let Some(declared) = type_vars.iter().find(|v| v.name == var.name) {
            bindings.insert(declared.name.clone(), actual.clone());
            return Some(bindings);
        }
    }

    // A non-variable-like name in `where` position.
    if let CoreType::Named(name) = pattern {
        if let Some(declared) = type_vars.iter().find(|v| &v.name == name) {
            bindings.insert(declared.name.clone(), actual.clone());
            return Some(bindings);
        }
    }

    // `Type{T}` pattern matching.
    if let CoreType::TypeOf(inner) = pattern {
        if let Some(declared) = exact_type_var_for_pattern(inner, type_vars) {
            let CoreType::TypeOf(actual_inner) = actual else {
                return None;
            };
            bindings.insert(declared.name.clone(), actual_inner.as_ref().clone());
            return Some(bindings);
        }
        if let CoreType::TypeOf(actual_inner) = actual {
            if let Some(extracted) = extract_type_bindings(actual_inner, inner, type_vars) {
                return Some(extracted);
            }
        }
    }

    if core_is_subtype_full(actual, pattern) {
        Some(bindings)
    } else {
        None
    }
}

/// Port of `JuliaType::check_diagonal_rule_for_params` (Issue #2554).
fn check_diagonal_rule_for_params(
    param_types: &[CoreType],
    bindings: &HashMap<String, CoreType>,
) -> bool {
    let pattern = CoreType::Tuple(param_types.to_vec());
    bindings
        .iter()
        .all(|(var_name, bound_ty)| satisfies_diagonal_rule(var_name, bound_ty, &pattern))
}

/// A variable that appears more than once in covariant position and never in
/// invariant position must bind to a concrete type.
fn satisfies_diagonal_rule(var_name: &str, bound_ty: &CoreType, pattern: &CoreType) -> bool {
    let (covariant, invariant) = type_var_occurrences(pattern, var_name, false);
    if covariant <= 1 || invariant > 0 {
        return true;
    }
    bound_ty.is_concrete_type()
}

/// Port of `analyze_type_var_occurrences`: (covariant, invariant) occurrence
/// counts of `var_name` in `pattern`. Mirrors the legacy recursion shape:
/// full recursion through tuples and the Vector/Matrix element (the
/// `VectorOf`/`MatrixOf` spellings) and `Type{…}`, a one-level direct-param
/// check for other nominal structs (the legacy string-level brace scan).
fn type_var_occurrences(ty: &CoreType, var_name: &str, inside_invariant: bool) -> (u8, u8) {
    let mut covariant: u8 = 0;
    let mut invariant: u8 = 0;
    let add = |c: u8, i: u8, covariant: &mut u8, invariant: &mut u8| {
        *covariant = covariant.saturating_add(c).min(2);
        *invariant = invariant.saturating_add(i).min(2);
    };
    match ty {
        CoreType::TypeVar(var) if type_var_base_name(&var.name) == var_name => {
            if inside_invariant {
                invariant = 1;
            } else {
                covariant = 1;
            }
        }
        CoreType::Named(name) if name == var_name => {
            if inside_invariant {
                invariant = 1;
            } else {
                covariant = 1;
            }
        }
        CoreType::Tuple(elems) | CoreType::Union(elems) => {
            for elem in elems {
                let (c, i) = type_var_occurrences(elem, var_name, inside_invariant);
                add(c, i, &mut covariant, &mut invariant);
            }
        }
        CoreType::Struct { name, params }
            if matches!(strip_module_prefix(name), "Vector" | "Matrix") && params.len() == 1 =>
        {
            let (c, i) = type_var_occurrences(&params[0], var_name, true);
            add(c, i, &mut covariant, &mut invariant);
        }
        CoreType::TypeOf(inner) => {
            let (c, i) = type_var_occurrences(inner, var_name, true);
            add(c, i, &mut covariant, &mut invariant);
        }
        CoreType::Struct { params, .. } => {
            for param in params {
                if core_param_names_var(param, var_name) {
                    add(0, 1, &mut covariant, &mut invariant);
                }
            }
        }
        CoreType::Vararg(inner) if core_param_names_var(inner, var_name) => {
            add(0, 1, &mut covariant, &mut invariant);
        }
        _ => {}
    }
    (covariant, invariant)
}

fn core_param_names_var(param: &CoreType, var_name: &str) -> bool {
    match param {
        CoreType::TypeVar(var) => var.name == var_name,
        CoreType::Named(name) => name == var_name,
        _ => false,
    }
}

/// Merge bindings extracted from one tuple slot, rejecting conflicts.
fn merge_tuple_bindings(
    acc: &mut HashMap<String, CoreType>,
    extracted: HashMap<String, CoreType>,
) -> Option<()> {
    for (name, bound_ty) in extracted {
        match acc.entry(name) {
            std::collections::hash_map::Entry::Occupied(existing) => {
                if existing.get() != &bound_ty {
                    return None;
                }
            }
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(bound_ty);
            }
        }
    }
    Some(())
}

/// Join trailing vararg element types: the shared type when all equal, `Any`
/// when they differ, `None` for an empty slice.
fn join_types(elems: &[CoreType]) -> Option<CoreType> {
    let mut iter = elems.iter();
    let first = iter.next()?.clone();
    if iter.all(|ty| *ty == first) {
        Some(first)
    } else {
        Some(CoreType::Any)
    }
}

/// Whether the vararg element type contains a bare `where` variable that a
/// zero-length match would leave unbound.
fn vararg_type_var_unbound(vararg_elem: &CoreType, type_vars: &[CoreTypeVar]) -> bool {
    match vararg_elem {
        CoreType::TypeVar(var) => type_vars.iter().any(|v| v.name == var.name),
        CoreType::Named(name) => type_vars.iter().any(|v| &v.name == name),
        _ => false,
    }
}

/// Bind/check a `where` type variable against an argument core type.
///
/// Value-position applicability follows upstream Julia's covariant matching:
/// lower bounds and cross-variable bounds do not reject the method. Invariant
/// `Type{T}` callers pass both bounds explicitly and keep the stricter check.
fn bind_or_check_type_var(
    var_name: &str,
    upper: Option<&CoreType>,
    lower: Option<&CoreType>,
    arg_ty: &CoreType,
    bindings: &mut HashMap<String, CoreType>,
) -> bool {
    if let Some(upper) = upper {
        if !engine_is_subtype(arg_ty, upper) {
            return false;
        }
    }
    if let Some(lower) = lower {
        if !engine_is_subtype(lower, arg_ty) {
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

fn covariant_applicability_upper_bound<'a>(
    upper: Option<&'a CoreType>,
    type_vars: &[CoreTypeVar],
) -> Option<&'a CoreType> {
    upper.filter(|bound| !core_mentions_type_vars(bound, type_vars))
}

fn find_type_var<'a>(type_vars: &'a [CoreTypeVar], var_name: &str) -> Option<&'a CoreTypeVar> {
    type_vars
        .iter()
        .find(|var| type_var_base_name(&var.name) == var_name)
}

/// Base variable name before any embedded bound spelling (`T<:Number` /
/// `T>:Integer` legacy names survive verbatim in `CoreTypeVar::name`).
fn type_var_base_name(name: &str) -> &str {
    name.split_once("<:")
        .or_else(|| name.split_once(">:"))
        .map_or(name, |(base, _)| base)
        .trim()
}

/// A `where` variable's upper bound that is itself parametric and mentions
/// another `where` variable (`T<:Vector{S}`) is matched structurally (Issue
/// #5383 sub-case 2). Non-parametric bounds and parametric bounds without
/// inner variables keep the ordinary bound check.
fn parametric_type_var_bound_pattern<'a>(
    upper: Option<&'a CoreType>,
    type_vars: &[CoreTypeVar],
) -> Option<&'a CoreType> {
    let bound = upper?;
    let parametric = match bound {
        CoreType::Struct { params, .. } => !params.is_empty(),
        CoreType::Tuple(_)
        | CoreType::TypeOf(_)
        | CoreType::Union(_)
        | CoreType::Vararg(_)
        | CoreType::VarargLen { .. }
        | CoreType::NamedTuple(_) => true,
        _ => false,
    };
    (parametric && core_mentions_type_vars(bound, type_vars)).then_some(bound)
}

/// Whether a declared parameter type mentions any of the method's `where`
/// variables (port of `julia_type_mentions_type_params`).
fn core_mentions_type_vars(ty: &CoreType, type_vars: &[CoreTypeVar]) -> bool {
    type_vars
        .iter()
        .any(|var| core_mentions_type_var_name(ty, type_var_base_name(&var.name)))
}

/// Port of `julia_type_mentions_type_param_name`: full recursion through the
/// shapes whose legacy spellings recursed (`TypeOf`/`VectorOf`/`MatrixOf`/
/// `TupleOf`/`Union`/`UnionAll`), a one-level direct-argument check for other
/// nominal structs (the legacy string brace scan) and `Vararg` markers.
fn core_mentions_type_var_name(ty: &CoreType, var_name: &str) -> bool {
    match ty {
        CoreType::TypeVar(var) => type_var_base_name(&var.name) == var_name,
        CoreType::Named(name) => name == var_name,
        CoreType::TypeOf(inner) => core_mentions_type_var_name(inner, var_name),
        CoreType::Tuple(elems) | CoreType::Union(elems) => elems
            .iter()
            .any(|elem| core_mentions_type_var_name(elem, var_name)),
        CoreType::UnionAll { body, .. } => core_mentions_type_var_name(body, var_name),
        CoreType::Struct { name, params } => {
            if name == var_name {
                return true;
            }
            if matches!(strip_module_prefix(name), "Vector" | "Matrix") && params.len() == 1 {
                return core_mentions_type_var_name(&params[0], var_name);
            }
            params
                .iter()
                .any(|param| core_param_names_var(param, var_name))
        }
        CoreType::Vararg(inner) => core_param_names_var(inner, var_name),
        _ => false,
    }
}

/// Whether the argument is a primitive-ish dispatch leaf, mirroring the
/// legacy `JuliaType::is_primitive` used by the #5314 struct-leaf rule.
fn core_arg_is_dispatch_primitive(arg_ty: &CoreType) -> bool {
    arg_ty.is_builtin_dispatch_primitive_or_abstract_numeric()
}

/// Exact-name `where` lookup used by the extraction paths (the legacy
/// `tp.name == *p` comparisons).
fn exact_type_var_for_pattern<'a>(
    pattern: &CoreType,
    type_vars: &'a [CoreTypeVar],
) -> Option<&'a CoreTypeVar> {
    let name = match pattern {
        CoreType::TypeVar(var) => var.name.as_str(),
        CoreType::Named(name) => name.as_str(),
        _ => return None,
    };
    type_vars.iter().find(|var| var.name == name)
}

fn strip_module_prefix(name: &str) -> &str {
    name.rfind('.').map_or(name, |idx| &name[idx + 1..])
}

/// Whether the canonical inverse `core_type_to_julia_type` maps `ty` to a
/// genuine `JuliaType::Struct(_)` rather than a dedicated variant
/// (`JT::Array`/`JT::VectorOf`/`JT::Dict`/…). The legacy `is_subtype_of` /
/// `is_subtype_of_parametric` parametric-struct arms required `self`/`other`
/// to be a `JT::Struct`, so the bare-variant images must be excluded from
/// those arms (Issue #6495). Mirrors the `Struct` arm of
/// `inference_core::type_core::convert::core_type_to_julia_type`.
fn core_maps_to_julia_struct(ty: &CoreType) -> bool {
    let CoreType::Struct { name, params } = ty else {
        return false;
    };
    !matches!(
        (strip_module_prefix(name), params.len()),
        ("Vector", 1)
            | ("Matrix", 1)
            | ("Tuple", 0)
            | ("Array", 0)
            | ("Set", 0)
            | ("Dict", 0)
            | ("NamedTuple", 0)
            | ("UnitRange", 0)
            | ("StepRange", 0)
            | ("Generator", 0)
            | ("IOBuffer", 0)
            | ("Expr", 0)
            | ("QuoteNode", 0)
            | ("LineNumberNode", 0)
            | ("GlobalRef", 0)
    )
}

fn array_dims_match(actual_dim: Option<usize>, expected_dim: Option<usize>) -> bool {
    expected_dim.is_none_or(|expected| actual_dim.is_none_or(|actual| actual == expected))
}

fn core_value_param_rank(param: &CoreType) -> Option<usize> {
    match param {
        CoreType::Value(CoreValueParam::Int(value)) => usize::try_from(*value).ok(),
        CoreType::Value(CoreValueParam::SignedInt { value, .. }) => usize::try_from(*value).ok(),
        _ => None,
    }
}

/// Port of `array_projection`: `(element, rank)` for concrete array images.
fn core_array_projection(ty: &CoreType) -> Option<(CoreType, Option<usize>)> {
    let CoreType::Struct { name, params } = ty else {
        return None;
    };
    match strip_module_prefix(name) {
        "Vector" if params.len() == 1 => Some((params[0].clone(), Some(1))),
        "Matrix" if params.len() == 1 => Some((params[0].clone(), Some(2))),
        "BitVector" => Some((CoreType::Primitive(CorePrimitive::Bool), Some(1))),
        "BitMatrix" => Some((CoreType::Primitive(CorePrimitive::Bool), Some(2))),
        "BitArray" => Some((
            CoreType::Primitive(CorePrimitive::Bool),
            params.first().and_then(core_value_param_rank),
        )),
        "Array" if !params.is_empty() => Some((
            params[0].clone(),
            params.get(1).and_then(core_value_param_rank),
        )),
        _ => None,
    }
}

/// Port of `range_projection`.
fn core_range_projection(ty: &CoreType) -> Option<(CoreType, Option<usize>)> {
    match ty {
        CoreType::Abstract(CoreAbstract::AbstractRange | CoreAbstract::AbstractUnitRange) => {
            Some((CoreType::Any, Some(1)))
        }
        CoreType::Struct { name, params } => match strip_module_prefix(name) {
            "AbstractRange" | "AbstractUnitRange" | "UnitRange" | "StepRange" | "StepRangeLen"
            | "LinRange" | "OneTo" | "LogRange" => {
                Some((params.first().cloned().unwrap_or(CoreType::Any), Some(1)))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Port of `abstract_array_projection` (arrays plus ranges).
fn core_abstract_array_projection(ty: &CoreType) -> Option<(CoreType, Option<usize>)> {
    core_array_projection(ty).or_else(|| core_range_projection(ty))
}

/// `(element, rank)` for the parametric `AbstractArray{T,N}` /
/// `AbstractVector{T}` / `AbstractMatrix{T}` pattern family (port of
/// `abstract_array_struct_projection`). The bare `Abstract(AbstractVector)`
/// / `Abstract(AbstractMatrix)` images correspond to the legacy
/// `Struct("AbstractVector")` spellings, which projected with an `Any`
/// element; the bare `AbstractArray` had a dedicated legacy variant that did
/// NOT project.
fn abstract_array_family_projection(
    base: &str,
    params: &[CoreType],
) -> Option<(CoreType, Option<usize>)> {
    let rank = match base {
        "AbstractArray" => params.get(1).and_then(core_value_param_rank),
        "AbstractVector" => Some(1),
        "AbstractMatrix" => Some(2),
        _ => return None,
    };
    Some((params.first().cloned().unwrap_or(CoreType::Any), rank))
}

/// `(element, rank)` for concrete `Array{T[,N]}` patterns (port of
/// `array_struct_projection`, used by the `is_subtype_of` Struct arm).
fn array_family_projection(base: &str, params: &[CoreType]) -> Option<(CoreType, Option<usize>)> {
    if base != "Array" || params.is_empty() {
        return None;
    }
    Some((
        params[0].clone(),
        params.get(1).and_then(core_value_param_rank),
    ))
}

/// Whether the pattern uses the *abstract* array projection on the argument
/// side (port of `pattern_uses_abstract_array_projection`: only the
/// parametric `AbstractArray`/`AbstractVector`/`AbstractMatrix` struct
/// spellings, plus the bare abstract-vector/matrix images of the legacy
/// `Struct("AbstractVector")` spellings).
fn pattern_uses_abstract_array_projection(pattern: &CoreType) -> bool {
    match pattern {
        CoreType::Struct { name, params } => {
            abstract_array_family_projection(strip_module_prefix(name), params).is_some()
        }
        CoreType::Abstract(CoreAbstract::AbstractVector | CoreAbstract::AbstractMatrix) => true,
        _ => false,
    }
}

/// Pattern-side array projection used by `extract_type_bindings` (port of the
/// legacy `abstract_array_projection(pattern)` call, which accepted both the
/// abstract pattern family and concrete array/range patterns).
fn core_pattern_array_projection(pattern: &CoreType) -> Option<(CoreType, Option<usize>)> {
    if let CoreType::Struct { name, params } = pattern {
        if let Some(projection) =
            abstract_array_family_projection(strip_module_prefix(name), params)
        {
            return Some(projection);
        }
    }
    match pattern {
        CoreType::Abstract(CoreAbstract::AbstractVector) => Some((CoreType::Any, Some(1))),
        CoreType::Abstract(CoreAbstract::AbstractMatrix) => Some((CoreType::Any, Some(2))),
        _ => core_abstract_array_projection(pattern),
    }
}

/// Port of `type_object_inner_nominal_family_mismatch` for the `Type{…}`
/// invariant arm.
fn type_object_inner_family_mismatch(param_inner: &CoreType, arg_inner: &CoreType) -> bool {
    let Some(param_family) = type_object_inner_family(param_inner) else {
        return false;
    };
    let Some(arg_family) = type_object_inner_family(arg_inner) else {
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

fn type_object_inner_family(ty: &CoreType) -> Option<TypeObjectInnerFamily> {
    match ty {
        CoreType::Any | CoreType::TypeVar(_) => None,
        CoreType::Named(name) => Some(TypeObjectInnerFamily::Nominal(
            strip_module_prefix(name).to_string(),
        )),
        CoreType::Struct { name, params } => {
            let base = strip_module_prefix(name);
            match base {
                "Vector" if params.len() == 1 => {
                    Some(TypeObjectInnerFamily::Array { rank: Some(1) })
                }
                "Matrix" if params.len() == 1 => {
                    Some(TypeObjectInnerFamily::Array { rank: Some(2) })
                }
                "Array" => Some(TypeObjectInnerFamily::Array {
                    rank: params.get(1).and_then(core_value_param_rank),
                }),
                _ => Some(TypeObjectInnerFamily::Nominal(base.to_string())),
            }
        }
        other => Some(TypeObjectInnerFamily::Nominal(other.to_julia_name())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{JuliaType, TypeParam};

    fn cores(types: &[JuliaType]) -> Vec<CoreType> {
        types.iter().map(CoreType::from).collect()
    }

    fn vars(params: &[TypeParam]) -> Vec<CoreTypeVar> {
        params.iter().map(CoreTypeVar::from).collect()
    }

    fn type_var(name: &str) -> JuliaType {
        JuliaType::TypeVar(name.to_string(), None)
    }

    #[test]
    fn core_match_reuses_declared_typevar_bindings() {
        // A `where T` method (`f(x::T, y::T) where T`): both arguments must
        // bind a single concrete `T`. Unlike the legacy synthetic test, the
        // type variable is DECLARED — an undeclared `CoreType::TypeVar` image
        // is the #5314 struct-leaf case (covered separately), which is the
        // only shape real `core_signature` projections produce.
        let params = cores(&[type_var("T"), type_var("T")]);
        let type_vars = vars(&[TypeParam::new("T".to_string())]);
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::BigInt, JuliaType::BigInt]),
            &type_vars,
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::BigInt, JuliaType::Int64]),
            &type_vars,
        )
        .is_none());
    }

    #[test]
    fn core_match_partial_parametric_struct_signature_matches_prefix_issue_8348() {
        let params = cores(&[JuliaType::from_name_or_struct("TwoParamMatrixIssue{T}")]);
        let args = cores(&[JuliaType::from_name_or_struct(
            "TwoParamMatrixIssue{Float64, Vector{Float64}}",
        )]);
        let type_vars = vars(&[TypeParam::new("T".to_string())]);

        assert_eq!(
            core_signature_match_with_bindings(&params, &args, &type_vars),
            Some(1)
        );
    }

    #[test]
    fn core_match_covariant_partial_parametric_struct_signature_issue_8349() {
        let params = cores(&[JuliaType::from_name_or_struct(
            "CovariantParamIssue{<:Real}",
        )]);
        let args = cores(&[JuliaType::from_name_or_struct(
            "CovariantParamIssue{Float64, Vector{Float64}}",
        )]);
        let nonmatching_args = cores(&[JuliaType::from_name_or_struct(
            "CovariantParamIssue{String, Vector{String}}",
        )]);

        assert_eq!(
            core_signature_match_with_bindings(&params, &args, &[]),
            Some(0)
        );
        assert_eq!(
            core_signature_match_with_bindings(&params, &nonmatching_args, &[]),
            None
        );
    }

    #[test]
    fn core_match_keeps_anonymous_tuple_bounds_independent_issue_6251() {
        let broad_tuple = cores(&[JuliaType::TupleOf(vec![
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
            JuliaType::TypeVar("_".to_string(), Some("Real".to_string())),
        ])]);
        let diagonal_tuple = cores(&[JuliaType::TupleOf(vec![type_var("T"), type_var("T")])]);
        let diagonal_vars = vars(&[TypeParam::with_upper_bound(
            "T".to_string(),
            "Real".to_string(),
        )]);

        assert!(core_signature_match_with_bindings(
            &broad_tuple,
            &cores(&[JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::Float64
            ])]),
            &[],
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &broad_tuple,
            &cores(&[JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::String
            ])]),
            &[],
        )
        .is_none());
        assert!(core_signature_match_with_bindings(
            &diagonal_tuple,
            &cores(&[JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::Float64
            ])]),
            &diagonal_vars,
        )
        .is_none());
    }

    #[test]
    fn core_match_enforces_nested_diagonal_rule_issue_5050() {
        let nested_params = cores(&[JuliaType::VectorOf(Box::new(type_var("T"))), type_var("T")]);
        let type_vars = vars(&[TypeParam::new("T".to_string())]);

        assert!(core_signature_match_with_bindings(
            &nested_params,
            &cores(&[
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Int64,
            ]),
            &type_vars,
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &nested_params,
            &cores(&[
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Float64,
            ]),
            &type_vars,
        )
        .is_none());
        assert!(core_signature_match_with_bindings(
            &nested_params,
            &cores(&[
                JuliaType::VectorOf(Box::new(JuliaType::Float64)),
                JuliaType::Int64,
            ]),
            &type_vars,
        )
        .is_none());
    }

    #[test]
    fn core_match_trailing_vararg_tuple_pattern_issue_4857() {
        // Tuple{Int64, Vararg{Int64}} accepts (Int64,), (Int64, Int64, ...).
        let params = cores(&[JuliaType::TupleOf(vec![
            JuliaType::Int64,
            JuliaType::Struct("Vararg{Int64}".to_string()),
        ])]);
        let accepts = |elems: Vec<JuliaType>| {
            core_signature_match_with_bindings(&params, &cores(&[JuliaType::TupleOf(elems)]), &[])
                .is_some()
        };
        assert!(accepts(vec![JuliaType::Int64]));
        assert!(accepts(vec![
            JuliaType::Int64,
            JuliaType::Int64,
            JuliaType::Int64
        ]));
        assert!(!accepts(vec![JuliaType::Int64, JuliaType::Float64]));
        assert!(!accepts(vec![]));
    }

    #[test]
    fn core_match_typeof_array_pattern_binds_inner_typevars() {
        let type_vars = vars(&[TypeParam::new("T".to_string())]);
        let pattern = cores(&[JuliaType::TypeOf(Box::new(JuliaType::Struct(
            "Array{T}".to_string(),
        )))]);

        assert!(core_signature_match_with_bindings(
            &pattern,
            &cores(&[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "Array{Int64}".to_string(),
            )))]),
            &type_vars,
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &pattern,
            &cores(&[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
                JuliaType::Float64,
            ))))]),
            &type_vars,
        )
        .is_none());
    }

    #[test]
    fn core_match_typeof_double_bound_enforces_lower_and_upper_invariantly_issue_5051() {
        let type_vars = vars(&[TypeParam::with_both_bounds(
            "T".to_string(),
            "Integer".to_string(),
            "Real".to_string(),
        )]);
        let pattern = cores(&[JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )))]);
        let matches = |arg: JuliaType| {
            core_signature_match_with_bindings(
                &pattern,
                &cores(&[JuliaType::TypeOf(Box::new(arg))]),
                &type_vars,
            )
            .is_some()
        };

        assert!(matches(JuliaType::Struct("Integer".to_string())));
        assert!(matches(JuliaType::Struct("Real".to_string())));
        assert!(!matches(JuliaType::Int64));
        assert!(!matches(JuliaType::Float64));
        assert!(!matches(JuliaType::Struct("Number".to_string())));
    }

    #[test]
    fn core_match_covariant_typevar_ignores_lower_bound_issue_8427() {
        let type_vars = vars(&[TypeParam::with_both_bounds(
            "T".to_string(),
            "Int64".to_string(),
            "Real".to_string(),
        )]);
        let pattern = cores(&[JuliaType::TypeVar(
            "T".to_string(),
            Some("Real".to_string()),
        )]);

        assert!(core_signature_match_with_bindings(
            &pattern,
            &cores(&[JuliaType::Float64]),
            &type_vars
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &pattern,
            &cores(&[JuliaType::Int64]),
            &type_vars
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &pattern,
            &cores(&[JuliaType::String]),
            &type_vars
        )
        .is_none());
    }

    #[test]
    fn core_match_cross_typevar_bounds_do_not_reject_applicability_issue_8427() {
        let type_vars = vars(&[
            TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            TypeParam::with_upper_bound("S".to_string(), "T".to_string()),
        ]);
        let params = cores(&[type_var("T"), type_var("S")]);

        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Int64, JuliaType::Int64]),
            &type_vars,
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Int64, JuliaType::Float64]),
            &type_vars,
        )
        .is_some());
    }

    #[test]
    fn core_match_struct_leaf_typevar_image_rejects_primitives_issue_5314() {
        // `min(::Q, ::Q)` where Q is a user struct: the context-free bridge
        // images the annotation as an unbounded TypeVar, but it is NOT a
        // method `where` variable, so primitive arguments must not match.
        let params = cores(&[
            JuliaType::Struct("Q".to_string()),
            JuliaType::Struct("Q".to_string()),
        ]);
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Float64, JuliaType::Float64]),
            &[],
        )
        .is_none());
        // The same image WITH the `where` declaration binds normally.
        let type_vars = vars(&[TypeParam::new("Q".to_string())]);
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Float64, JuliaType::Float64]),
            &type_vars,
        )
        .is_some());
    }

    #[test]
    fn core_match_parametric_typevar_bound_pattern_issue_5383() {
        // g(x::T, y::S) where {S<:Number, T<:Vector{S}}: the parametric bound
        // `Vector{S}` is matched structurally, binding S consistently.
        let type_vars = vars(&[
            TypeParam::with_upper_bound("S".to_string(), "Number".to_string()),
            TypeParam::with_upper_bound("T".to_string(), "Vector{S}".to_string()),
        ]);
        let params = cores(&[type_var("T"), type_var("S")]);

        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[
                JuliaType::VectorOf(Box::new(JuliaType::Int64)),
                JuliaType::Int64,
            ]),
            &type_vars,
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[
                JuliaType::VectorOf(Box::new(JuliaType::String)),
                JuliaType::String,
            ]),
            &type_vars,
        )
        .is_none());
    }

    #[test]
    fn core_match_type_object_does_not_match_value_level_parametric_pattern_issue_6251() {
        let actual = cores(&[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
            JuliaType::Int64,
        ))))]);
        let type_vars = vars(&[TypeParam::new("T".to_string())]);

        assert!(core_signature_match_with_bindings(
            &cores(&[JuliaType::Struct("Array{T, 1}".to_string())]),
            &actual,
            &type_vars,
        )
        .is_none());
        assert!(core_signature_match_with_bindings(
            &cores(&[JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "LinRange{T}".to_string(),
            )))]),
            &actual,
            &type_vars,
        )
        .is_none());
        assert!(core_signature_match_with_bindings(
            &cores(&[JuliaType::TypeOf(Box::new(JuliaType::VectorOf(Box::new(
                type_var("T"),
            ))))]),
            &actual,
            &type_vars,
        )
        .is_some());
    }

    #[test]
    fn core_match_bare_family_arg_matches_parametric_param_issue_6495() {
        // `f(::Val{N}) where N` must accept a bare `Val` argument (the
        // runtime value type of `Val{3}()` erases the value parameter; legacy
        // `is_subtype_of` accepted bare `Foo <: Foo{...}`). The bare unknown
        // name images as `Named`, the parametric param as `Struct` (stage-3
        // regression, fixture `types_value_param_binding_4268`).
        let params = cores(&[JuliaType::Struct("Val{N}".to_string())]);
        let type_vars = vars(&[TypeParam::new("N".to_string())]);
        assert_eq!(
            core_signature_match_with_bindings(
                &params,
                &cores(&[JuliaType::Struct("Val".to_string())]),
                &type_vars,
            ),
            Some(0)
        );
        // A different family name must still be rejected.
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Struct("Wal".to_string())]),
            &type_vars,
        )
        .is_none());
        // And the parametric spelling keeps binding the value parameter.
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Struct("Val{3}".to_string())]),
            &type_vars,
        )
        .is_some());
    }

    #[test]
    fn core_match_ntuple_param_matches_homogeneous_tuple_issue_6495() {
        // `h(xs::NTuple{N,T}) where {N,T}`: legacy decided this on the engine
        // subtype path with no bindings (Some(0)); the structural Tuple arm
        // must not short-circuit on the VarargLen image (stage-3 regression,
        // fixture `types_value_param_binding_4268`).
        let params = cores(&[JuliaType::Struct("NTuple{N, T}".to_string())]);
        let type_vars = vars(&[
            TypeParam::new("N".to_string()),
            TypeParam::new("T".to_string()),
        ]);
        assert_eq!(
            core_signature_match_with_bindings(
                &params,
                &cores(&[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::Int64,
                    JuliaType::Int64,
                ])]),
                &type_vars,
            ),
            Some(0)
        );
        // Fixed-element NTuple{N, Int64} rejects non-matching element types.
        let int_params = cores(&[JuliaType::Struct("NTuple{N, Int64}".to_string())]);
        let n_var = vars(&[TypeParam::new("N".to_string())]);
        assert_eq!(
            core_signature_match_with_bindings(
                &int_params,
                &cores(&[JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])]),
                &n_var,
            ),
            Some(0)
        );
        assert!(core_signature_match_with_bindings(
            &int_params,
            &cores(&[JuliaType::TupleOf(vec![
                JuliaType::Float64,
                JuliaType::Float64,
            ])]),
            &n_var,
        )
        .is_none());

        // `NTuple{N}` is `Tuple{Vararg{Any, N}}`: it binds only the length and
        // accepts heterogeneous tuple elements.
        let any_params = cores(&[JuliaType::Struct("NTuple{N}".to_string())]);
        assert_eq!(
            core_signature_match_with_bindings(
                &any_params,
                &cores(&[JuliaType::TupleOf(vec![
                    JuliaType::Int64,
                    JuliaType::Float64,
                ])]),
                &n_var,
            ),
            Some(0)
        );
    }

    #[test]
    fn core_match_parametric_arg_matches_bare_family_param_issue_6495() {
        // The reverse direction (`Foo{Int64} <: Foo` with a bare user-struct
        // param imaging as `Named`).
        let params = cores(&[JuliaType::Struct("Foo".to_string())]);
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Struct("Foo{Int64}".to_string())]),
            &[],
        )
        .is_some());
        assert!(core_signature_match_with_bindings(
            &params,
            &cores(&[JuliaType::Struct("Bar{Int64}".to_string())]),
            &[],
        )
        .is_none());
    }
}
