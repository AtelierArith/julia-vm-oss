use super::{
    core_type_contains_typevar, core_type_is_concrete_diagonal, expand_concrete_vararg_len,
    normalize_union, CoreType, CoreTypeVar,
};

impl CoreType {
    /// If `self` is a `Tuple` whose elements include a concrete-length
    /// `Vararg{T, N}` (the lowered form of `NTuple{N, T}` as well), return the
    /// equivalent flat `Tuple{T, ..., T}`. Returns `None` when no rewriting is
    /// needed so callers can keep borrowing the original value (Issue #5062).
    fn normalize_concrete_vararg_tuple(&self) -> Option<Self> {
        match self {
            Self::Tuple(elements) => expand_concrete_vararg_len(elements).map(Self::Tuple),
            _ => None,
        }
    }

    /// Conservative type intersection. Returns `Bottom` when the represented
    /// subset can prove disjointness; otherwise returns the narrower side or a
    /// normalized union of distributed intersections.
    pub fn type_intersect(&self, other: &Self) -> Self {
        // Canonicalize `Tuple{Vararg{T, N}}` / `NTuple{N, T}` (concrete `N`) into
        // the flat `Tuple{T, ..., T}` shape so the result matches upstream
        // `typeintersect`, which always returns the expanded tuple (Issue #5062).
        let normalized_self = self.normalize_concrete_vararg_tuple();
        let normalized_other = other.normalize_concrete_vararg_tuple();
        if normalized_self.is_some() || normalized_other.is_some() {
            let lhs = normalized_self.as_ref().unwrap_or(self);
            let rhs = normalized_other.as_ref().unwrap_or(other);
            return lhs.type_intersect(rhs);
        }

        if let Some(intersection) = diagonal_unionall_tuple_intersect(self, other) {
            return intersection;
        }
        if let Some(intersection) = diagonal_unionall_tuple_intersect(other, self) {
            return intersection;
        }
        if let Some(intersection) = mixed_diagonal_unionall_tuple_intersect(self, other) {
            return intersection;
        }
        if let Some(intersection) = mixed_diagonal_unionall_tuple_intersect(other, self) {
            return intersection;
        }

        if self.is_subtype_of(other) {
            return self.clone();
        }
        if other.is_subtype_of(self) {
            return other.clone();
        }

        // Bare parametric container `UnionAll` met with a ground parametric
        // instantiation, e.g. `typeintersect(Vector{T} where T<:Real,
        // AbstractVector{Int})` -> `Vector{Int}` (Issue #5048). The concrete<->abstract
        // container relation already works above when neither side is a `UnionAll`;
        // this fills the `UnionAll`-on-one-side gap.
        if let Some(intersection) = unionall_parametric_container_intersect(self, other) {
            return intersection;
        }
        if let Some(intersection) = unionall_parametric_container_intersect(other, self) {
            return intersection;
        }

        match (self, other) {
            (Self::Union(types), _) => normalize_union(
                types
                    .iter()
                    .map(|t| t.type_intersect(other))
                    .filter(|t| !matches!(t, Self::Bottom))
                    .collect(),
            ),
            (_, Self::Union(types)) => normalize_union(
                types
                    .iter()
                    .map(|t| self.type_intersect(t))
                    .filter(|t| !matches!(t, Self::Bottom))
                    .collect(),
            ),
            (Self::Tuple(elements), Self::Tuple(other_elements)) => {
                tuple_type_intersect(elements, other_elements)
            }
            (Self::TypeOf(inner), Self::TypeOf(other_inner)) => {
                match inner.type_intersect(other_inner) {
                    Self::Bottom => Self::Bottom,
                    intersection => Self::TypeOf(Box::new(intersection)),
                }
            }
            _ => Self::Bottom,
        }
    }
}

fn diagonal_unionall_tuple_intersect(pattern: &CoreType, actual: &CoreType) -> Option<CoreType> {
    let CoreType::UnionAll { var, body } = pattern else {
        return None;
    };
    let CoreType::Tuple(pattern_elements) = body.as_ref() else {
        return None;
    };
    let CoreType::Tuple(actual_elements) = actual else {
        return None;
    };
    if pattern_elements.len() != actual_elements.len() || pattern_elements.is_empty() {
        return Some(CoreType::Bottom);
    }
    if !pattern_elements
        .iter()
        .all(|element| tuple_element_is_bound_var(element, &var.name))
    {
        return None;
    }

    let mut meet = var.upper_bound.as_deref().cloned().unwrap_or(CoreType::Any);
    for actual_element in actual_elements {
        meet = meet.type_intersect(actual_element);
        if matches!(meet, CoreType::Bottom) {
            return Some(CoreType::Bottom);
        }
    }

    if let Some(candidate) = actual_elements
        .iter()
        .find(|actual| core_type_is_concrete_diagonal(actual))
    {
        if candidate.is_subtype_of(&meet)
            && var
                .lower_bound
                .as_deref()
                .is_none_or(|lower| lower.is_subtype_of(candidate))
            && actual_elements
                .iter()
                .all(|actual| candidate.is_subtype_of(actual))
        {
            return Some(CoreType::Tuple(vec![
                candidate.clone();
                pattern_elements.len()
            ]));
        }
        return Some(CoreType::Bottom);
    }

    if core_type_is_concrete_diagonal(&meet) {
        return Some(CoreType::Tuple(vec![meet; pattern_elements.len()]));
    }

    let mut narrowed_var = var.clone();
    narrowed_var.upper_bound = Some(Box::new(meet));
    Some(CoreType::UnionAll {
        var: narrowed_var,
        body: body.clone(),
    })
}

fn tuple_element_is_bound_var(element: &CoreType, var_name: &str) -> bool {
    match element {
        CoreType::TypeVar(element_var) => element_var.name == var_name,
        CoreType::Named(name) => name == var_name,
        _ => false,
    }
}

fn mixed_diagonal_unionall_tuple_intersect(
    pattern: &CoreType,
    actual: &CoreType,
) -> Option<CoreType> {
    let CoreType::UnionAll { var, body } = pattern else {
        return None;
    };
    let CoreType::Tuple(pattern_elements) = body.as_ref() else {
        return None;
    };
    let CoreType::Tuple(actual_elements) = actual else {
        return None;
    };
    if pattern_elements.len() != actual_elements.len() || pattern_elements.is_empty() {
        return Some(CoreType::Bottom);
    }

    let bound = var.upper_bound.as_deref().cloned().unwrap_or(CoreType::Any);
    let mut candidate: Option<CoreType> = None;
    let mut direct_occurrences: Vec<(usize, CoreType)> = Vec::new();
    let mut output: Vec<Option<CoreType>> = vec![None; pattern_elements.len()];

    for (idx, (pattern_element, actual_element)) in pattern_elements
        .iter()
        .zip(actual_elements.iter())
        .enumerate()
    {
        if matches!(pattern_element, CoreType::TypeVar(element_var) if element_var.name == var.name)
        {
            direct_occurrences.push((idx, actual_element.clone()));
            continue;
        }

        let invariant_candidate =
            struct_typevar_invariant_candidate(pattern_element, actual_element, &var.name)?;
        if !invariant_candidate.is_subtype_of(&bound) {
            return Some(CoreType::Bottom);
        }
        if let Some(existing) = &candidate {
            if !(existing.is_subtype_of(&invariant_candidate)
                && invariant_candidate.is_subtype_of(existing))
            {
                return Some(CoreType::Bottom);
            }
        } else {
            candidate = Some(invariant_candidate);
        }
        output[idx] = Some(actual_element.clone());
    }

    let candidate = candidate?;
    if direct_occurrences.is_empty() {
        return None;
    }

    for (idx, actual_element) in direct_occurrences {
        let intersection = actual_element.type_intersect(&candidate);
        if matches!(intersection, CoreType::Bottom) {
            return Some(CoreType::Bottom);
        }
        output[idx] = Some(intersection);
    }

    output
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .map(CoreType::Tuple)
}

/// When a tuple element of a diagonal `UnionAll` pattern is an invariant
/// parametric container holding the diagonal `var` in one or more parameter
/// slots (e.g. `Vector{T}`, `Dict{Symbol,T}`, `Dict{T,Symbol}`, `Pair{Symbol,T}`,
/// `Dict{T,T}`), recover the value the diagonal variable takes from the matching
/// `actual` container. Every non-`var` slot must be invariantly equal between
/// `pattern` and `actual` (containers are invariant), so `Dict{Symbol,T}` vs
/// `Dict{Int,Real}` correctly yields `None` (-> `Union{}`). Multiple `var` slots
/// (the `Dict{T,T}` diagonal) must agree by invariant equality. Returns the
/// single agreed candidate, or `None` when this element is not such a
/// container occurrence of `var` (Issue #5048).
fn struct_typevar_invariant_candidate(
    pattern: &CoreType,
    actual: &CoreType,
    var_name: &str,
) -> Option<CoreType> {
    let CoreType::Struct { name, params } = pattern else {
        return None;
    };
    let CoreType::Struct {
        name: actual_name,
        params: actual_params,
    } = actual
    else {
        return None;
    };
    if name != actual_name || params.is_empty() || params.len() != actual_params.len() {
        return None;
    }

    let mut candidate: Option<CoreType> = None;
    let mut saw_var = false;
    for (pattern_param, actual_param) in params.iter().zip(actual_params.iter()) {
        match pattern_param {
            CoreType::TypeVar(var) if var.name == var_name => {
                saw_var = true;
                match &candidate {
                    Some(existing)
                        if !(existing.is_subtype_of(actual_param)
                            && actual_param.is_subtype_of(existing)) =>
                    {
                        return None;
                    }
                    None => candidate = Some(actual_param.clone()),
                    _ => {}
                }
            }
            _ => {
                // A non-diagonal slot is invariant: pattern and actual must be
                // equal (mutual subtype), else the containers do not match.
                if !(pattern_param.is_subtype_of(actual_param)
                    && actual_param.is_subtype_of(pattern_param))
                {
                    return None;
                }
            }
        }
    }

    if saw_var {
        candidate
    } else {
        None
    }
}

/// `typeintersect` for a bare parametric container `UnionAll` met with a ground
/// parametric instantiation, e.g.
/// `typeintersect(Vector{T} where T<:Real, AbstractVector{Int})` -> `Vector{Int}`
/// (Issue #5048).
///
/// The concrete-container <-> abstract-container relation
/// (`typeintersect(Vector{Int}, AbstractVector{Int}) == Vector{Int}`) already
/// works through the `is_subtype_of` branch of `type_intersect`; the only gap was
/// a `UnionAll` on one side. Containers are invariant, so each `where` variable is
/// forced to the matching positional parameter of `actual`. We read those values
/// off, bound-check each, then verify the resulting concrete body `<: actual` with
/// the proven subtype engine, so an unrelated family (`Vector{T} where T` vs
/// `AbstractSet{Int}`) or a wrong dimensionality (`Vector{T} where T` vs
/// `AbstractArray{Int,2}`) is rejected. A ground `actual` makes the binding unique,
/// so the answer is a single concrete type or `Union{}`, never a residual
/// `UnionAll`; the deeper `UnionAll`x`UnionAll` meet (a free type variable in
/// `actual`) is intentionally left to the environment machinery (Issue #5615) and
/// returns `None` here.
fn unionall_parametric_container_intersect(
    pattern: &CoreType,
    actual: &CoreType,
) -> Option<CoreType> {
    // Peel `where` layers (outer-to-inner == type-parameter order).
    let mut vars: Vec<&CoreTypeVar> = Vec::new();
    let mut body = pattern;
    while let CoreType::UnionAll { var, body: inner } = body {
        vars.push(var);
        body = inner;
    }
    if vars.is_empty() {
        return None;
    }
    let CoreType::Struct {
        name: pname,
        params: pparams,
    } = body
    else {
        return None;
    };
    // `actual` must be a ground parametric instantiation (concrete, or a parametric
    // abstract container such as `AbstractVector{Int}`). A free type variable means
    // a `UnionAll`x`UnionAll` meet, deferred to the environment machinery.
    let CoreType::Struct {
        params: aparams, ..
    } = actual
    else {
        return None;
    };
    if pparams.is_empty()
        || aparams.len() < pparams.len()
        || aparams.iter().any(core_type_contains_typevar)
    {
        return None;
    }
    // Each pattern parameter is either one of the `where` variables (a bare
    // parametric such as `Vector{T}` / `Dict{K,V}`) or a fixed concrete type
    // (a partial instantiation such as `Dict{Int,V} where V`). A type variable
    // takes the positionally matching parameter of `actual`; a fixed parameter is
    // kept as-is, with its invariance enforced by the final subtype check. A
    // non-variable parameter that itself contains a type variable is a nested shape
    // outside this focused path and is deferred.
    let mut substituted = Vec::with_capacity(pparams.len());
    let mut assigned: Vec<(&str, &CoreType)> = Vec::new();
    for (idx, pparam) in pparams.iter().enumerate() {
        let candidate = &aparams[idx];
        match pparam {
            CoreType::TypeVar(pvar) => {
                let bound_var = vars.iter().find(|v| v.name == pvar.name)?;
                if let Some(upper) = bound_var.upper_bound.as_deref() {
                    if !candidate.is_subtype_of(upper) {
                        return Some(CoreType::Bottom);
                    }
                }
                // A diagonal variable used in several invariant positions
                // (`Pair{T,T}`) forces those positions to be the *same* type, so
                // `Pair{Int,String}` intersects to `Union{}`, not `Pair{Int,String}`.
                if let Some((_, prior)) = assigned.iter().find(|(name, _)| *name == pvar.name) {
                    if !(prior.is_subtype_of(candidate) && candidate.is_subtype_of(prior)) {
                        return Some(CoreType::Bottom);
                    }
                } else {
                    assigned.push((&pvar.name, candidate));
                }
                substituted.push(candidate.clone());
            }
            fixed if !core_type_contains_typevar(fixed) => substituted.push(fixed.clone()),
            _ => return None,
        }
    }
    let result = CoreType::Struct {
        name: pname.clone(),
        params: substituted,
    };
    // Confirm the family relation + invariant parameters with the proven engine.
    result.is_subtype_of(actual).then_some(result)
}

fn tuple_type_intersect(elements: &[CoreType], other_elements: &[CoreType]) -> CoreType {
    // Flatten concrete-length `Vararg{T, N}` / `NTuple{N, T}` so the intersection
    // operates on plain fixed-arity tuples (Issue #5062). Mirrors the same
    // normalization applied during subtype checking.
    if let Some(expanded) = expand_concrete_vararg_len(elements) {
        return tuple_type_intersect(&expanded, other_elements);
    }
    if let Some(expanded) = expand_concrete_vararg_len(other_elements) {
        return tuple_type_intersect(elements, &expanded);
    }

    let (left_fixed, left_vararg) = split_trailing_vararg(elements);
    let (right_fixed, right_vararg) = split_trailing_vararg(other_elements);

    if left_vararg.is_none() && right_vararg.is_none() && left_fixed.len() != right_fixed.len() {
        return CoreType::Bottom;
    }
    if left_vararg.is_none() && left_fixed.len() < right_fixed.len() {
        return CoreType::Bottom;
    }
    if right_vararg.is_none() && right_fixed.len() < left_fixed.len() {
        return CoreType::Bottom;
    }

    let mut intersections = Vec::new();
    let shared_fixed = left_fixed.len().min(right_fixed.len());
    for (left, right) in left_fixed.iter().take(shared_fixed).zip(right_fixed.iter()) {
        let intersection = left.type_intersect(right);
        if matches!(intersection, CoreType::Bottom) {
            return CoreType::Bottom;
        }
        intersections.push(intersection);
    }

    if !push_extra_tuple_intersections(
        &mut intersections,
        &left_fixed[shared_fixed..],
        right_vararg,
    ) {
        return CoreType::Bottom;
    }
    if !push_extra_tuple_intersections(
        &mut intersections,
        &right_fixed[shared_fixed..],
        left_vararg,
    ) {
        return CoreType::Bottom;
    }

    if let (Some(left_vararg), Some(right_vararg)) = (left_vararg, right_vararg) {
        let vararg_intersection = left_vararg.type_intersect(right_vararg);
        if !matches!(vararg_intersection, CoreType::Bottom) {
            intersections.push(CoreType::Vararg(Box::new(vararg_intersection)));
        }
    }

    CoreType::Tuple(intersections)
}

fn split_trailing_vararg(elements: &[CoreType]) -> (&[CoreType], Option<&CoreType>) {
    if let Some((CoreType::Vararg(vararg_ty), fixed)) = elements.split_last() {
        (fixed, Some(vararg_ty.as_ref()))
    } else {
        (elements, None)
    }
}

fn push_extra_tuple_intersections(
    intersections: &mut Vec<CoreType>,
    extra_fixed: &[CoreType],
    other_vararg: Option<&CoreType>,
) -> bool {
    let Some(other_vararg) = other_vararg else {
        return extra_fixed.is_empty();
    };

    for element in extra_fixed {
        let intersection = element.type_intersect(other_vararg);
        if matches!(intersection, CoreType::Bottom) {
            return false;
        }
        intersections.push(intersection);
    }
    true
}
