use super::{
    array_family_dim, array_family_name_subtype_allowed, array_family_pattern_params_match,
    array_family_pattern_params_match_in, base_type_name, core_type_contains_typevar,
    core_type_is_concrete_diagonal, struct_family_subtype, tuple_elements_match_with_bindings,
    tuple_elements_match_with_bindings_in, CoreAbstract, CoreType, CoreTypeVar,
};
use crate::types::StructHierarchy;
use std::collections::HashMap;

impl CoreType {
    pub(super) fn matches_unionall_pattern(&self, pattern: &Self) -> bool {
        self.matches_unionall_pattern_with_lookup(pattern, None)
    }

    pub(super) fn matches_unionall_pattern_with_hierarchy(
        &self,
        pattern: &Self,
        hierarchy: &StructHierarchy,
    ) -> bool {
        self.matches_unionall_pattern_with_lookup(pattern, Some(hierarchy))
    }

    fn matches_unionall_pattern_with_lookup(
        &self,
        pattern: &Self,
        hierarchy: Option<&StructHierarchy>,
    ) -> bool {
        let mut scope = HashMap::new();
        let mut bindings = TypeVarBindingState::default();
        core_type_matches_pattern_with_lookup(
            self,
            pattern,
            &mut scope,
            &mut bindings,
            TypeVarVariance::Covariant,
            hierarchy,
        ) && bindings.satisfies_diagonal_rule()
    }

    /// Specificity score used as a migration bridge for existing dispatch
    /// scoring. Larger values are more specific.
    pub fn specificity(&self) -> u8 {
        match self {
            Self::Any | Self::Bottom | Self::TypeVar(_) => 0,
            Self::Union(_) => 1,
            Self::Abstract(CoreAbstract::Number)
            | Self::Abstract(CoreAbstract::AbstractString)
            | Self::Abstract(CoreAbstract::AbstractChar)
            | Self::Abstract(CoreAbstract::AbstractArray)
            | Self::Abstract(CoreAbstract::AbstractVector)
            | Self::Abstract(CoreAbstract::AbstractMatrix)
            | Self::Abstract(CoreAbstract::DenseArray)
            | Self::Abstract(CoreAbstract::AbstractDict)
            | Self::Abstract(CoreAbstract::AbstractSet)
            | Self::Abstract(CoreAbstract::AbstractRange)
            | Self::Abstract(CoreAbstract::AbstractUnitRange)
            | Self::Abstract(CoreAbstract::Function)
            | Self::Abstract(CoreAbstract::IO)
            | Self::AbstractUser { .. } => 1,
            // Issue #5129: `Core.Builtin` sits one level below `Function`.
            Self::Abstract(CoreAbstract::Real) | Self::Abstract(CoreAbstract::Builtin) => 2,
            Self::Abstract(CoreAbstract::Integer) | Self::Abstract(CoreAbstract::AbstractFloat) => {
                3
            }
            Self::Abstract(CoreAbstract::Signed) | Self::Abstract(CoreAbstract::Unsigned) => 4,
            Self::Tuple(elements) => {
                if elements.is_empty() {
                    5
                } else {
                    elements.iter().map(Self::specificity).sum()
                }
            }
            Self::Struct { name, params } => {
                if name == "Vector" || name == "Matrix" {
                    return params.first().map_or(5, Self::specificity);
                }
                // A type-level `NamedTuple{(:a, :b)}` (names-only marker) or the
                // concrete `@NamedTuple{a::T1, b::T2}` parameter is strictly more
                // specific than the bare `NamedTuple` (specificity 5), so
                // dispatch picks the field-name-constrained method over the
                // catch-all (Issue #5063).
                if (name == "NamedTuple" || name == "@NamedTuple") && !params.is_empty() {
                    return 6;
                }
                if params.iter().any(|p| matches!(p, Self::TypeVar(_))) {
                    4
                } else {
                    5
                }
            }
            Self::TypeOf(inner) => 7 + inner.specificity().min(10),
            Self::UnionAll { body, .. } => body.specificity().saturating_sub(1).max(1),
            Self::Vararg(inner) => inner.specificity().saturating_sub(1).max(1),
            Self::VarargLen { element, .. } => element.specificity().saturating_sub(1).max(1),
            Self::Primitive(_)
            | Self::Abstract(CoreAbstract::Type)
            | Self::Abstract(CoreAbstract::DataType)
            | Self::NamedTuple(_)
            | Self::Value(_)
            | Self::Module(_)
            | Self::Named(_) => 5,
        }
    }

    /// Runtime dispatch pre-score for Julia method parameter patterns.
    ///
    /// This intentionally returns only the high-priority structural scores
    /// used before runtime registry subtype checks:
    /// - 4: exact pattern match
    /// - 3: same-family parametric pattern with a type variable, or tuple `Any`
    /// - 2: bare struct-family match such as `Rational` or `Array`
    /// - 0: no structural pre-match; callers may still do subtype scoring
    pub fn dispatch_pattern_score(&self, actual: &Self) -> u32 {
        self.dispatch_pattern_score_with_lookup(actual, None)
    }

    /// Hierarchy-aware variant of [`Self::dispatch_pattern_score`]
    /// (Issue #6502 / #6536): typevar bound checks and parametric subtype
    /// probes inside the structural tiers resolve user-declared ancestry
    /// through the supplied [`StructHierarchy`], so a pattern like
    /// `Box{T<:Animal}` keeps its tier-3 parametric score for `Box{Dog}`
    /// instead of dropping to the generic subtype-fallback tier.
    pub fn dispatch_pattern_score_in(&self, hierarchy: &StructHierarchy, actual: &Self) -> u32 {
        self.dispatch_pattern_score_with_lookup(actual, Some(hierarchy))
    }

    fn dispatch_pattern_score_with_lookup(
        &self,
        actual: &Self,
        hierarchy: Option<&StructHierarchy>,
    ) -> u32 {
        if self == actual {
            return 4;
        }

        match (self, actual) {
            (Self::Any, _) => 1,
            (Self::Union(members), _) => members
                .iter()
                .map(|member| member.dispatch_pattern_score_with_lookup(actual, hierarchy))
                .max()
                .unwrap_or(0),
            (Self::AbstractUser { name, .. } | Self::Module(name), Self::Named(actual_name))
                if name == actual_name =>
            {
                4
            }
            (
                Self::Struct { name, params },
                Self::Struct {
                    name: actual_name,
                    params: actual_params,
                },
            ) if name == actual_name => {
                struct_dispatch_score_for_name_with_lookup(name, params, actual_params, hierarchy)
            }
            (
                Self::Struct { name, params },
                Self::Struct {
                    name: actual_name, ..
                },
            ) if params.is_empty() && struct_family_subtype(actual_name, name) => 2,
            (Self::Struct { params, .. }, actual @ Self::Struct { .. })
                if !params.is_empty()
                    && core_type_is_subtype_with_lookup(actual, self, hierarchy) =>
            {
                3
            }
            (Self::Struct { name, params }, Self::Tuple(_))
                if name == "Tuple" && params.is_empty() =>
            {
                2
            }
            (Self::Tuple(expected), Self::Tuple(actual)) => tuple_dispatch_score(expected, actual),
            (Self::TypeOf(expected), Self::TypeOf(actual)) => {
                expected.dispatch_pattern_score_with_lookup(actual, hierarchy)
            }
            _ => 0,
        }
    }
}

fn container_or_ref_pattern_params_match_with_lookup(
    actual_params: &[CoreType],
    pattern_params: &[CoreType],
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    if pattern_params.is_empty() {
        return true;
    }
    actual_params.len() >= pattern_params.len()
        && actual_params
            .iter()
            .zip(pattern_params.iter())
            .all(|(actual_param, pattern_param)| {
                core_type_matches_pattern_with_lookup(
                    actual_param,
                    pattern_param,
                    scope,
                    bindings,
                    TypeVarVariance::Invariant,
                    hierarchy,
                )
            })
}

fn container_or_ref_pattern_pair(actual_name: &str, pattern_name: &str) -> bool {
    matches!(
        (base_type_name(actual_name), base_type_name(pattern_name)),
        ("Dict", "AbstractDict" | "Dict")
            | ("Set", "AbstractSet" | "Set")
            | ("RefValue", "Ref" | "RefValue")
            | ("Ref", "Ref")
    )
}

fn struct_dispatch_score_for_name_with_lookup(
    name: &str,
    expected_params: &[CoreType],
    actual_params: &[CoreType],
    hierarchy: Option<&StructHierarchy>,
) -> u32 {
    if name == "Array"
        && expected_params.len() == 1
        && actual_params.len() == 2
        && super::array_unionall_element_param_matches(&actual_params[0], &expected_params[0])
    {
        return 3;
    }

    if expected_params.is_empty() {
        2
    } else if expected_params.len() <= actual_params.len()
        && expected_params.iter().any(core_type_contains_typevar)
        && {
            let mut scope = HashMap::new();
            let mut bindings = TypeVarBindingState::default();
            actual_params
                .iter()
                .zip(expected_params.iter())
                .all(|(actual, expected)| {
                    core_type_matches_pattern_with_lookup(
                        actual,
                        expected,
                        &mut scope,
                        &mut bindings,
                        TypeVarVariance::Invariant,
                        hierarchy,
                    )
                })
                && bindings.satisfies_diagonal_rule()
        }
    {
        3
    } else {
        0
    }
}

fn tuple_dispatch_score(expected: &[CoreType], actual: &[CoreType]) -> u32 {
    if expected.len() != actual.len() {
        return 0;
    }
    if expected.iter().all(|p| matches!(p, CoreType::Any)) {
        return 3;
    }
    if expected.iter().any(core_type_contains_typevar) {
        return 3;
    }
    0
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TypeVarVariance {
    Covariant,
    Invariant,
}

#[derive(Debug, Clone, Default)]
pub(super) struct TypeVarBindingState {
    bindings: HashMap<String, CoreType>,
    covariant_occurrences: HashMap<String, usize>,
    invariant_occurrences: HashMap<String, usize>,
}

impl TypeVarBindingState {
    fn record(&mut self, name: &str, variance: TypeVarVariance) {
        let occurrences = match variance {
            TypeVarVariance::Covariant => &mut self.covariant_occurrences,
            TypeVarVariance::Invariant => &mut self.invariant_occurrences,
        };
        *occurrences.entry(name.to_string()).or_insert(0) += 1;
    }

    fn bind_or_check(
        &mut self,
        var: &CoreTypeVar,
        actual: &CoreType,
        variance: TypeVarVariance,
        scope: &HashMap<String, CoreTypeVar>,
        hierarchy: Option<&StructHierarchy>,
    ) -> bool {
        self.record(&var.name, variance);

        if let Some(lower_bound) = &var.lower_bound {
            let lower_bound = self.resolve_bound_for_check(lower_bound, scope, false);
            if !core_type_is_subtype_with_lookup(&lower_bound, actual, hierarchy) {
                return false;
            }
        }
        if let Some(upper_bound) = &var.upper_bound {
            let upper_bound = self.resolve_bound_for_check(upper_bound, scope, true);
            if !core_type_is_subtype_with_lookup(actual, &upper_bound, hierarchy) {
                return false;
            }
        }

        if let Some(existing) = self.bindings.get(&var.name) {
            existing == actual
        } else {
            self.bindings.insert(var.name.clone(), actual.clone());
            true
        }
    }

    pub(super) fn satisfies_diagonal_rule(&self) -> bool {
        self.bindings.iter().all(|(name, bound)| {
            let covariant_count = self.covariant_occurrences.get(name).copied().unwrap_or(0);
            let invariant_count = self.invariant_occurrences.get(name).copied().unwrap_or(0);
            invariant_count > 0 || covariant_count <= 1 || core_type_is_concrete_diagonal(bound)
        })
    }

    fn resolve_bound_for_check(
        &self,
        bound: &CoreType,
        scope: &HashMap<String, CoreTypeVar>,
        use_upper_fallback: bool,
    ) -> CoreType {
        match bound {
            CoreType::TypeVar(var) => self
                .resolve_typevar_reference_for_check(&var.name, scope, use_upper_fallback)
                .unwrap_or_else(|| bound.clone()),
            CoreType::Named(name) if scope.contains_key(name) => self
                .resolve_typevar_reference_for_check(name, scope, use_upper_fallback)
                .unwrap_or_else(|| bound.clone()),
            CoreType::Struct { name, params } => CoreType::Struct {
                name: name.clone(),
                params: params
                    .iter()
                    .map(|param| self.resolve_bound_for_check(param, scope, use_upper_fallback))
                    .collect(),
            },
            CoreType::Tuple(elements) => CoreType::Tuple(
                elements
                    .iter()
                    .map(|element| self.resolve_bound_for_check(element, scope, use_upper_fallback))
                    .collect(),
            ),
            CoreType::Union(types) => CoreType::Union(
                types
                    .iter()
                    .map(|ty| self.resolve_bound_for_check(ty, scope, use_upper_fallback))
                    .collect(),
            ),
            CoreType::TypeOf(inner) => CoreType::TypeOf(Box::new(self.resolve_bound_for_check(
                inner,
                scope,
                use_upper_fallback,
            ))),
            CoreType::Vararg(inner) => CoreType::Vararg(Box::new(self.resolve_bound_for_check(
                inner,
                scope,
                use_upper_fallback,
            ))),
            CoreType::VarargLen { element, len } => CoreType::VarargLen {
                element: Box::new(self.resolve_bound_for_check(element, scope, use_upper_fallback)),
                len: Box::new(self.resolve_bound_for_check(len, scope, use_upper_fallback)),
            },
            _ => bound.clone(),
        }
    }

    fn resolve_typevar_reference_for_check(
        &self,
        name: &str,
        scope: &HashMap<String, CoreTypeVar>,
        use_upper_fallback: bool,
    ) -> Option<CoreType> {
        if let Some(binding) = self.bindings.get(name) {
            return Some(binding.clone());
        }
        let scoped = scope.get(name)?;
        let fallback = if use_upper_fallback {
            scoped.upper_bound.as_deref()
        } else {
            scoped.lower_bound.as_deref()
        };
        match fallback {
            Some(bound) => Some(self.resolve_bound_for_check(bound, scope, use_upper_fallback)),
            None if use_upper_fallback => Some(CoreType::Any),
            None => Some(CoreType::Bottom),
        }
    }
}

fn core_type_is_subtype_with_lookup(
    actual: &CoreType,
    pattern: &CoreType,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    match hierarchy {
        Some(hierarchy) => actual.is_subtype_of_with_hierarchy(pattern, hierarchy),
        None => actual.is_subtype_of(pattern),
    }
}

pub(super) fn core_type_matches_pattern(
    actual: &CoreType,
    pattern: &CoreType,
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    variance: TypeVarVariance,
) -> bool {
    core_type_matches_pattern_with_lookup(actual, pattern, scope, bindings, variance, None)
}

pub(super) fn core_type_matches_pattern_in(
    hierarchy: &StructHierarchy,
    actual: &CoreType,
    pattern: &CoreType,
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    variance: TypeVarVariance,
) -> bool {
    core_type_matches_pattern_with_lookup(
        actual,
        pattern,
        scope,
        bindings,
        variance,
        Some(hierarchy),
    )
}

fn core_type_matches_pattern_with_lookup(
    actual: &CoreType,
    pattern: &CoreType,
    scope: &mut HashMap<String, CoreTypeVar>,
    bindings: &mut TypeVarBindingState,
    variance: TypeVarVariance,
    hierarchy: Option<&StructHierarchy>,
) -> bool {
    match pattern {
        CoreType::AbstractUser { name, .. }
            if matches!(CoreType::from_julia_name(name), CoreType::Abstract(_)) =>
        {
            let canonical = CoreType::from_julia_name(name);
            core_type_matches_pattern_with_lookup(
                actual, &canonical, scope, bindings, variance, hierarchy,
            )
        }
        CoreType::UnionAll { var, body } => {
            let previous = scope.insert(var.name.clone(), var.clone());
            let matched = core_type_matches_pattern_with_lookup(
                actual, body, scope, bindings, variance, hierarchy,
            );
            if let Some(previous) = previous {
                scope.insert(var.name.clone(), previous);
            } else {
                scope.remove(&var.name);
            }
            matched
        }
        CoreType::TypeVar(var) => {
            let scoped_var = scope.get(&var.name).unwrap_or(var);
            let effective_var = CoreTypeVar {
                name: scoped_var.name.clone(),
                lower_bound: var
                    .lower_bound
                    .clone()
                    .or_else(|| scoped_var.lower_bound.clone()),
                upper_bound: var
                    .upper_bound
                    .clone()
                    .or_else(|| scoped_var.upper_bound.clone()),
            };
            bindings.bind_or_check(&effective_var, actual, variance, scope, hierarchy)
        }
        CoreType::Union(types) => types.iter().any(|candidate| {
            let mut trial_bindings = bindings.clone();
            let mut trial_scope = scope.clone();
            if core_type_matches_pattern_with_lookup(
                actual,
                candidate,
                &mut trial_scope,
                &mut trial_bindings,
                variance,
                hierarchy,
            ) {
                *bindings = trial_bindings;
                *scope = trial_scope;
                true
            } else {
                false
            }
        }),
        CoreType::Struct { name, params } => match actual {
            CoreType::Struct {
                name: actual_name,
                params: actual_params,
            } if array_family_dim(name).is_some()
                && array_family_dim(actual_name).is_some()
                && array_family_name_subtype_allowed(actual_name, name) =>
            {
                match hierarchy {
                    Some(hierarchy) => array_family_pattern_params_match_in(
                        hierarchy,
                        actual_name,
                        actual_params,
                        name,
                        params,
                        scope,
                        bindings,
                    ),
                    None => array_family_pattern_params_match(
                        actual_name,
                        actual_params,
                        name,
                        params,
                        scope,
                        bindings,
                    ),
                }
            }
            CoreType::Struct {
                name: actual_name,
                params: actual_params,
            } if container_or_ref_pattern_pair(actual_name, name) => {
                container_or_ref_pattern_params_match_with_lookup(
                    actual_params,
                    params,
                    scope,
                    bindings,
                    hierarchy,
                )
            }
            CoreType::Struct {
                name: actual_name,
                params: actual_params,
            } if name == actual_name => {
                params.is_empty()
                    || (params.len() == actual_params.len()
                        && actual_params.iter().zip(params.iter()).all(
                            |(actual_param, pattern_param)| {
                                core_type_matches_pattern_with_lookup(
                                    actual_param,
                                    pattern_param,
                                    scope,
                                    bindings,
                                    TypeVarVariance::Invariant,
                                    hierarchy,
                                )
                            },
                        ))
            }
            // A user parametric struct matched against a DIFFERENT-named
            // parametric existential pattern (`MyVec{Int64}` vs the pattern
            // `Wrapper{S}` of `Wrapper{S} where S`) must walk the actual's
            // declared, parameter-substituted parent (`Wrapper{Int64}`) and
            // re-match it against the pattern with the binding state intact, so
            // `S` binds to `Int64`. A plain subtype check loses the binding
            // (`Int64 == TypeVar(S)` is false), which is why this previously fell
            // back to a separate runtime `type_ancestors` walk (Issue #5915
            // wave 3). The substituted parent is recomputed through the supplied
            // hierarchy; without a hierarchy nothing is known, so fall through to
            // the plain subtype check.
            CoreType::Struct {
                name: actual_name,
                params: actual_params,
            } if hierarchy.is_some_and(|h| {
                super::registered_instantiated_struct_parent_in(h, actual_name, actual_params)
                    .is_some()
            }) =>
            {
                let parent = hierarchy.and_then(|h| {
                    super::registered_instantiated_struct_parent_in(h, actual_name, actual_params)
                });
                match parent {
                    Some(parent) => core_type_matches_pattern_with_lookup(
                        &parent, pattern, scope, bindings, variance, hierarchy,
                    ),
                    None => core_type_is_subtype_with_lookup(actual, pattern, hierarchy),
                }
            }
            _ => core_type_is_subtype_with_lookup(actual, pattern, hierarchy),
        },
        CoreType::Tuple(pattern_elements) => match actual {
            CoreType::Tuple(actual_elements) => match hierarchy {
                Some(hierarchy) => tuple_elements_match_with_bindings_in(
                    hierarchy,
                    actual_elements,
                    pattern_elements,
                    scope,
                    bindings,
                ),
                None => tuple_elements_match_with_bindings(
                    actual_elements,
                    pattern_elements,
                    scope,
                    bindings,
                ),
            },
            _ => false,
        },
        CoreType::Vararg(pattern_inner) => match actual {
            CoreType::Vararg(actual_inner) => core_type_matches_pattern_with_lookup(
                actual_inner,
                pattern_inner,
                scope,
                bindings,
                variance,
                hierarchy,
            ),
            _ => core_type_matches_pattern_with_lookup(
                actual,
                pattern_inner,
                scope,
                bindings,
                variance,
                hierarchy,
            ),
        },
        CoreType::VarargLen {
            element: pattern_element,
            len: pattern_len,
        } => match actual {
            CoreType::VarargLen {
                element: actual_element,
                len: actual_len,
            } => {
                core_type_matches_pattern_with_lookup(
                    actual_element,
                    pattern_element,
                    scope,
                    bindings,
                    variance,
                    hierarchy,
                ) && core_type_matches_pattern_with_lookup(
                    actual_len,
                    pattern_len,
                    scope,
                    bindings,
                    variance,
                    hierarchy,
                )
            }
            _ => core_type_matches_pattern_with_lookup(
                actual,
                pattern_element,
                scope,
                bindings,
                variance,
                hierarchy,
            ),
        },
        CoreType::TypeOf(pattern_inner) => match actual {
            CoreType::TypeOf(actual_inner) => core_type_matches_pattern_with_lookup(
                actual_inner,
                pattern_inner,
                scope,
                bindings,
                TypeVarVariance::Invariant,
                hierarchy,
            ),
            _ => false,
        },
        // Concrete (non-typevar) leaf in invariant position: require equality,
        // not a one-directional covariant subtype. For example, the `Real`
        // inside `Vector{Real}` is invariant, so under covariant Tuple matching
        // `Tuple{Vector{Int}} <: Tuple{Vector{Real}}` must reduce to
        // `Int == Real` (false), not `Int <: Real` (true). Equality is checked
        // as mutual subtyping so aliases (`Int`/`Int64`) still agree. In
        // covariant position the catch-all stays a plain subtype check, keeping
        // `Tuple{Int} <: Tuple{Real}` true (Issue #5564).
        _ => match variance {
            TypeVarVariance::Invariant => {
                core_type_is_subtype_with_lookup(actual, pattern, hierarchy)
                    && core_type_is_subtype_with_lookup(pattern, actual, hierarchy)
            }
            TypeVarVariance::Covariant => {
                core_type_is_subtype_with_lookup(actual, pattern, hierarchy)
            }
        },
    }
}
