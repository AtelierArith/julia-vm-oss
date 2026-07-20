//! Type name normalization utilities for struct/type comparisons.
//!
//! These utilities are used across multiple builtin modules for
//! consistent type name handling during equality and isa checks.

use std::borrow::Cow;

use crate::types::JuliaType;

/// Normalize struct name for equality comparison.
/// Strips module prefix (e.g., "MyGeometry.Point{Int64}" -> "Point{Int64}")
/// to allow comparing structs from different contexts.
pub(crate) fn normalize_struct_name(name: &str) -> &str {
    // Find the last '.' before any '{' (type parameters)
    // This handles cases like "Module.Struct{T}" -> "Struct{T}"
    if let Some(brace_idx) = name.find('{') {
        // Only look for '.' in the base name part (before type params)
        let base = &name[..brace_idx];
        if let Some(dot_idx) = base.rfind('.') {
            return &name[dot_idx + 1..];
        }
    } else if let Some(dot_idx) = name.rfind('.') {
        // No type params, just strip module prefix
        return &name[dot_idx + 1..];
    }
    name
}

/// Check whether the type-parameter portion of a stripped name contains
/// any alias that requires normalization ("Int" or "UInt" as standalone
/// type arguments). Returns `true` when we can skip allocation entirely.
#[inline]
fn params_need_normalization(params: &str) -> bool {
    // Quick byte scan: if neither "Int" nor "UInt" appears, no work needed.
    // This avoids the 6× `.replace()` chain in the common case.
    params.contains("Int") || params.contains("UInt")
}

/// Normalize type name for isa comparison.
/// Strips module prefix and normalizes type aliases (Int/UInt -> native word type).
///
/// Issue #5067 / #5210: parametric type identity must ignore the cosmetic space
/// the pretty renderer inserts after commas. typeof renders structured value
/// parameters with a space (`Val{(1, 2)}`), while the same type written as a
/// source literal omits it (`Val{(1,2)}`); upstream Julia treats both as the
/// same DataType. Valid value/type parameters never contain a semantically
/// meaningful ASCII space, so dropping the spaces inside the brace portion is a
/// stable canonical form (matching `JuliaType::type_eq`'s `struct_name_eq`).
///
/// Returns `Cow::Borrowed` when no normalization is needed (the common case),
/// avoiding heap allocation entirely.
pub(crate) fn normalize_type_for_isa(name: &str) -> Cow<'_, str> {
    // A top-level trailing ` where ` clause means the name is the surface
    // form of a `UnionAll`, not a plain parametric application. The
    // space-stripping below would destroy the ` where ` keyword
    // (`MyWrap{T} where {S<:Real, T<:S}` -> `MyWrap{T}where{S<:Real,T<:S}`),
    // so the downstream `CoreType::from_julia_name` parse could no longer see
    // the `where` chain and `isa` rejected every member (Issue #10410). Leave
    // such names intact: the structured parse owns their canonicalization.
    if crate::inference_core::type_core::has_top_level_trailing_where(name) {
        return Cow::Borrowed(name);
    }

    // First strip module prefix
    let stripped = normalize_struct_name(name);

    // Normalize type aliases in type parameters
    // e.g., "Point{Int}" -> "Point{Int64}" on 64-bit, "Point{Int32}" on 32-bit.
    if let Some(brace_idx) = stripped.find('{') {
        let params = &stripped[brace_idx..];

        let has_space = params.as_bytes().contains(&b' ');

        // Fast path: if no alias keywords and no cosmetic spaces appear in the
        // parameter portion, borrow as-is.
        if !has_space && !params_need_normalization(params) {
            return Cow::Borrowed(stripped);
        }

        let base = &stripped[..brace_idx];

        // Drop cosmetic spaces (after commas / around braces) so spelling does
        // not affect identity, then replace remaining type aliases.
        let despaced: String = if has_space {
            params.chars().filter(|c| *c != ' ').collect()
        } else {
            params.to_string()
        };

        // After de-spacing, alias keywords sit flush against the delimiters, so
        // the comma-with-space variants collapse into the brace/comma forms.
        let native_int = crate::types::native_int_type_name();
        let native_uint = crate::types::native_uint_type_name();
        let int_open = format!("{{{native_int}}}");
        let uint_open = format!("{{{native_uint}}}");
        let int_close = format!(",{native_int}}}");
        let uint_close = format!(",{native_uint}}}");
        let int_middle = format!("{{{native_int},");
        let uint_middle = format!("{{{native_uint},");
        let normalized_params = despaced
            .replace("{Int}", &int_open)
            .replace("{UInt}", &uint_open)
            .replace(",Int}", &int_close)
            .replace(",UInt}", &uint_close)
            .replace("{Int,", &int_middle)
            .replace("{UInt,", &uint_middle);

        Cow::Owned(format!("{}{}", base, normalized_params))
    } else {
        // No type params, check if the name itself is an alias
        match stripped {
            "Int" => Cow::Borrowed(crate::types::native_int_type_name()),
            "UInt" => Cow::Borrowed(crate::types::native_uint_type_name()),
            _ => Cow::Borrowed(stripped),
        }
    }
}

/// Return whether two runtime type objects denote the same Julia type.
///
/// Julia defines type identity in terms of mutual subtyping
/// (`jl_types_equal(a, b) == subtype(a, b) && subtype(b, a)`). Keep runtime type
/// value equality routed through the same subtype helper used by `<:` so new
/// subtype cases do not need a parallel equality-only arm (Issue #5921).
pub(crate) fn type_objects_equal(left: &JuliaType, right: &JuliaType) -> bool {
    let left_has_runtime = left.contains_runtime_typevar();
    let right_has_runtime = right.contains_runtime_typevar();
    if left_has_runtime || right_has_runtime {
        let mixed_nominal_projection = left_has_runtime != right_has_runtime
            && !matches!(
                left,
                JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
            )
            && !matches!(
                right,
                JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
            );
        if mixed_nominal_projection {
            return crate::inference_core::CoreType::from(left)
                .is_semantically_equal(&crate::inference_core::CoreType::from(right));
        }
        if left.type_eq(right) {
            return true;
        }
    }
    if matches!(left, JuliaType::UnionAll { .. }) && matches!(right, JuliaType::UnionAll { .. }) {
        if unionall_alias_matches_bare_type(left, right, false)
            || unionall_alias_matches_bare_type(right, left, false)
        {
            return true;
        }
        return crate::inference_core::CoreType::from(left)
            .is_semantically_equal(&crate::inference_core::CoreType::from(right));
    }
    if bounded_legacy_unionall_shadows_nominal_binder(left)
        || bounded_legacy_unionall_shadows_nominal_binder(right)
    {
        return false;
    }
    if left.type_eq(right) {
        return true;
    }

    // Two named structs whose module-stripped, alias-normalized names are equal
    // denote the same `DataType` (Issue #8100). A module-private short type
    // referenced bare inside its module (`E` -> `JuliaType::Struct("E")`) and the
    // same type spelled qualified (`M2.E` -> `JuliaType::Struct("M2.E")`) must be
    // `===`. The mutual-subtype path below cannot decide this: a 1-char name like
    // `E` is parsed into a `CoreType::TypeVar` by the string-level type-variable
    // heuristic, so the qualified/bare pair never reconciles there. This is purely
    // additive — it only recognizes already-equal type names, mirroring the
    // module-prefix stripping the subtype engine already applies to longer names.
    // Module-prefix stripping is only safe in one direction (Issue #11021): a
    // BARE reference legitimately denotes the same type as a QUALIFIED
    // reference to it (the #8100 case above), but two DIFFERENT modules can
    // declare same-named structs that must stay distinct.
    if let (JuliaType::Struct(a), JuliaType::Struct(b)) = (left, right) {
        if struct_names_denote_same_type(a, b) {
            return true;
        }
        // Two structs whose owners are BOTH known and DIFFERENT can never be
        // the same nominal type, no matter what the mutual-subtype fallback
        // below concludes. This guard matters because that fallback routes
        // through `CoreType`, whose `Struct { name, .. }` construction
        // (`CoreType::from_julia_name`) already strips ALL module
        // qualification -- `CoreSubtypeEngine` has no way to see the two
        // owners are different on its own, so without this early return it
        // reports mutual subtyping and this function would wrongly return
        // `true`. Making `CoreType` itself module-aware is a much larger
        // change (`docs/vm/SEMANTIC_ID_MIGRATION.md`'s ~44 `inference_core`
        // struct/type-lattice sites); tracked as a follow-up, not required
        // here since this string-level guard already gets the right answer
        // for the identity question this function exists to answer.
        if !crate::types::struct_owners_compatible(a, b) {
            return false;
        }
    }
    if unionall_alias_matches_bare_type(left, right, false)
        || unionall_alias_matches_bare_type(right, left, false)
    {
        return true;
    }
    type_values_subtype(left, right) && type_values_subtype(right, left)
}

fn bounded_legacy_unionall_shadows_nominal_binder(ty: &JuliaType) -> bool {
    let JuliaType::UnionAll {
        var,
        lower_bound,
        bound,
        body,
    } = ty
    else {
        return false;
    };
    if lower_bound.is_none() && bound.is_none() {
        return false;
    }
    core_contains_nominal_leaf(&crate::inference_core::CoreType::from(body.as_ref()), var)
}

fn core_contains_nominal_leaf(ty: &crate::inference_core::CoreType, name: &str) -> bool {
    use crate::inference_core::CoreType;
    match ty {
        CoreType::Any | CoreType::Primitive(_) | CoreType::Abstract(_) | CoreType::Module(_) => {
            ty.to_julia_name() == name
        }
        CoreType::AbstractUser { name: value, .. } => value == name,
        CoreType::Struct {
            name: value,
            params,
        } => {
            (params.is_empty() && value == name)
                || params
                    .iter()
                    .any(|param| core_contains_nominal_leaf(param, name))
        }
        CoreType::Tuple(types) | CoreType::Union(types) => types
            .iter()
            .any(|member| core_contains_nominal_leaf(member, name)),
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => {
            core_contains_nominal_leaf(inner, name)
        }
        CoreType::VarargLen { element, len } => {
            core_contains_nominal_leaf(element, name) || core_contains_nominal_leaf(len, name)
        }
        CoreType::NamedTuple(fields) => fields
            .iter()
            .any(|(_, field)| core_contains_nominal_leaf(field, name)),
        CoreType::UnionAll { var, body } => {
            var.name != name && core_contains_nominal_leaf(body, name)
        }
        _ => false,
    }
}

fn unionall_alias_matches_bare_type(
    alias: &JuliaType,
    bare: &JuliaType,
    require_canonical_param_names: bool,
) -> bool {
    // Runtime-built UnionAlls bind by TypeVar identity. Project those identities
    // to alpha-renamed lexical binders before comparing the generic wrapper to
    // a bare alias; otherwise a hygiene-generated spelling such as `T##m#...`
    // falls through to the structured subtype lane and `Vector <: S` is
    // spuriously false (Issue #11013).
    let projected_alias = matches!(alias, JuliaType::RuntimeUnionAll { .. })
        .then(|| alias.semantic_alpha_projection())
        .flatten();
    let alias = projected_alias.as_ref().unwrap_or(alias);

    let Some((alias_family, params, vars)) = unionall_generic_alias_parts(alias) else {
        return false;
    };
    let Some(bare_family) = bare_type_family(bare) else {
        return false;
    };
    alias_family == bare_family && (!require_canonical_param_names || params == vars)
}

pub(crate) fn unbounded_unionall_alias_equivalent(left: &JuliaType, right: &JuliaType) -> bool {
    unionall_alias_matches_bare_type(left, right, false)
        || unionall_alias_matches_bare_type(right, left, false)
}

fn unionall_alias_matches_declared_builtin(alias: &JuliaType, bare: &JuliaType) -> bool {
    let Some((alias_family, params, vars)) = unionall_generic_alias_parts(alias) else {
        return false;
    };
    let Some(bare_family) = bare_type_family(bare) else {
        return false;
    };
    let Some(declared_params) = declared_builtin_alias_params(&alias_family) else {
        return false;
    };
    alias_family == bare_family
        && params.len() == declared_params.len()
        && vars.len() == declared_params.len()
        && params
            .iter()
            .zip(vars.iter())
            .zip(declared_params.iter())
            .all(|((param, var), declared)| param == declared && var == declared)
}

fn declared_builtin_alias_params(family: &str) -> Option<&'static [&'static str]> {
    match family {
        "Vector" | "Matrix" | "DenseVector" | "DenseMatrix" | "Set" | "Ref" | "RefValue" => {
            Some(&["T"])
        }
        "Array" | "DenseArray" => Some(&["T", "N"]),
        "Dict" => Some(&["K", "V"]),
        _ => None,
    }
}

fn unionall_generic_alias_parts(ty: &JuliaType) -> Option<(String, Vec<String>, Vec<String>)> {
    let mut vars = Vec::new();
    let mut current = ty;
    while let JuliaType::UnionAll {
        var,
        lower_bound: None,
        bound: None,
        body,
    } = current
    {
        vars.push(var.clone());
        current = body.as_ref();
    }
    if vars.is_empty() {
        return None;
    }

    let (family, params) = generic_alias_body_parts(current)?;
    if params.len() == vars.len() && params == vars {
        Some((family, params, vars))
    } else {
        None
    }
}

fn generic_alias_body_parts(ty: &JuliaType) -> Option<(String, Vec<String>)> {
    match ty {
        JuliaType::Struct(name) => {
            let normalized = normalize_type_for_isa(name);
            let type_name = normalized.as_ref();
            let brace_idx = type_name.find('{')?;
            if !type_name.ends_with('}') {
                return None;
            }
            let family = type_name[..brace_idx].to_string();
            let params: Vec<String> = subset_julia_vm_bytecode::parse_parametric_params(type_name)
                .into_iter()
                .map(|param| param.trim().to_string())
                .collect();
            if params.is_empty() {
                None
            } else {
                Some((family, params))
            }
        }
        JuliaType::VectorOf(elem) => Some((
            "Vector".to_string(),
            vec![unbounded_source_typevar_name(elem)?],
        )),
        JuliaType::MatrixOf(elem) => Some((
            "Matrix".to_string(),
            vec![unbounded_source_typevar_name(elem)?],
        )),
        _ => None,
    }
}

fn unbounded_source_typevar_name(ty: &JuliaType) -> Option<String> {
    match ty {
        JuliaType::TypeVar(name, None) => Some(name.clone()),
        _ => None,
    }
}

fn bare_type_family(ty: &JuliaType) -> Option<String> {
    match ty {
        JuliaType::Struct(name) => {
            let normalized = normalize_type_for_isa(name);
            let type_name = normalized.as_ref();
            if type_name.contains('{') {
                None
            } else {
                Some(type_name.to_string())
            }
        }
        JuliaType::Array => Some("Array".to_string()),
        _ => None,
    }
}

fn struct_type_objects_identical(left: &str, right: &str) -> bool {
    struct_names_denote_same_type(left, right)
        || JuliaType::Struct(left.to_string()).type_eq(&JuliaType::Struct(right.to_string()))
}

/// Module-prefix-aware struct name comparison (Issue #11021): two struct
/// display names denote the same `DataType` when their alias-normalized,
/// module-stripped tails match AND (if both sides carry a module-owner
/// prefix) those owners agree. A BARE name has no owner to disagree with, so
/// it always matches a qualified reference to the same declaration
/// (Issue #8100); two DIFFERENTLY qualified names never match even when
/// their bare tails coincide, unlike the old unconditional-strip behavior
/// this replaces.
fn struct_names_denote_same_type(a: &str, b: &str) -> bool {
    let a_norm = normalize_type_for_isa(a);
    let b_norm = normalize_type_for_isa(b);
    if a_norm != b_norm {
        return false;
    }
    crate::types::struct_owners_compatible(a, b)
}

fn type_object_is_vararg_any(ty: &JuliaType) -> bool {
    let JuliaType::Struct(name) = ty else {
        return false;
    };
    normalize_type_for_isa(name)
        .chars()
        .filter(|c| *c != ' ')
        .eq("Vararg{Any}".chars())
}

fn type_object_is_tuple_vararg_any(ty: &JuliaType) -> bool {
    if let JuliaType::TupleOf(elements) = ty {
        return elements.len() == 1 && type_object_is_vararg_any(&elements[0]);
    }
    let JuliaType::Struct(name) = ty else {
        return false;
    };
    normalize_type_for_isa(name)
        .chars()
        .filter(|c| *c != ' ')
        .eq("Tuple{Vararg{Any}}".chars())
}

fn tuple_vararg_any_identity(left: &JuliaType, right: &JuliaType) -> bool {
    (matches!(left, JuliaType::Tuple) && type_object_is_tuple_vararg_any(right))
        || (matches!(right, JuliaType::Tuple) && type_object_is_tuple_vararg_any(left))
}

fn runtime_unionall_binder_names_match(left: &JuliaType, right: &JuliaType) -> bool {
    match (left, right) {
        (
            JuliaType::RuntimeUnionAll {
                var: left_var,
                body: left_body,
            },
            JuliaType::RuntimeUnionAll {
                var: right_var,
                body: right_body,
            },
        ) => {
            let left_name = match left_var.as_ref() {
                JuliaType::RuntimeTypeVar { name, .. } => name,
                _ => return false,
            };
            let right_name = match right_var.as_ref() {
                JuliaType::RuntimeTypeVar { name, .. } => name,
                _ => return false,
            };
            left_name == right_name && runtime_unionall_binder_names_match(left_body, right_body)
        }
        (JuliaType::RuntimeUnionAll { .. }, _) | (_, JuliaType::RuntimeUnionAll { .. }) => false,
        _ => true,
    }
}

/// Return whether two runtime type objects are identical for `===`.
///
/// This is stricter than [`type_objects_equal`]: Julia's `==` equates a generic
/// `UnionAll` alias such as `Vector{Q} where Q` with `Vector`, while `===`
/// expects canonical aliases to have already been folded by the type
/// constructor (`UnionAll(T, Vector{T}) -> Vector`). Exact enum equality keeps
/// canonical aliases that are represented the same internally (for example
/// `Tuple{Vararg{Any}}` parses to `Tuple`) and the struct-name normalization
/// preserves module/alias spelling identity for concrete `DataType` values.
pub(crate) fn type_objects_identical(left: &JuliaType, right: &JuliaType) -> bool {
    if left == right {
        return true;
    }
    if let (
        JuliaType::RuntimeTypeVar { id: left_id, .. },
        JuliaType::RuntimeTypeVar { id: right_id, .. },
    ) = (left, right)
    {
        return left_id == right_id;
    }
    let left_has_runtime = left.contains_runtime_typevar();
    let right_has_runtime = right.contains_runtime_typevar();
    if left_has_runtime || right_has_runtime {
        let left_is_runtime_unionall = matches!(left, JuliaType::RuntimeUnionAll { .. });
        let right_is_runtime_unionall = matches!(right, JuliaType::RuntimeUnionAll { .. });
        let left_is_any_unionall = matches!(
            left,
            JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
        );
        let right_is_any_unionall = matches!(
            right,
            JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
        );
        if ((left_is_runtime_unionall && !right_is_any_unionall)
            || (right_is_runtime_unionall && !left_is_any_unionall))
            && unbounded_unionall_alias_equivalent(left, right)
        {
            // A source-shadowed UnionAll deliberately survives canonical alias
            // folding so it remains a fresh object (`Vector{Int64} where
            // Int64 !== Vector`). Semantic equality is handled by `==`, not
            // identity. Other partial runtime wrappers (for example
            // `SubArray{Int8}`) remain eligible for structured identity against
            // their canonical projected DataType (Issues #10460 / #10861).
            return false;
        }
        if left_is_runtime_unionall
            && right_is_runtime_unionall
            && !runtime_unionall_binder_names_match(left, right)
        {
            return false;
        }
        // A fresh runtime UnionAll can be semantically equal to a canonical
        // partial application without being the same object. Upstream only
        // returns `=== true` when UnionAll construction reused the canonical
        // partial type's own binder/body; that case has already canonicalized
        // to the same projection before reaching this comparison.
        if left_is_runtime_unionall != right_is_runtime_unionall {
            return false;
        }
        let mixed_nominal_projection = left_has_runtime != right_has_runtime
            && !matches!(
                left,
                JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
            )
            && !matches!(
                right,
                JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
            );
        if mixed_nominal_projection {
            return crate::inference_core::CoreType::from(left)
                .is_semantically_equal(&crate::inference_core::CoreType::from(right));
        }
        if left.type_eq(right) {
            return true;
        }
    }
    let left_is_unionall = matches!(
        left,
        JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
    );
    let right_is_unionall = matches!(
        right,
        JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
    );
    if left_is_unionall && right_is_unionall {
        if matches!(left, JuliaType::UnionAll { .. }) && matches!(right, JuliaType::UnionAll { .. })
        {
            return false;
        }
        if matches!(left, JuliaType::RuntimeUnionAll { .. })
            && matches!(right, JuliaType::RuntimeUnionAll { .. })
            && !runtime_unionall_binder_names_match(left, right)
        {
            return false;
        }
        return type_values_subtype(left, right) && type_values_subtype(right, left);
    }
    if let (JuliaType::Struct(a), JuliaType::Struct(b)) = (left, right) {
        return struct_type_objects_identical(a, b);
    }
    if tuple_vararg_any_identity(left, right) {
        return true;
    }
    if unionall_alias_matches_declared_builtin(left, right)
        || unionall_alias_matches_declared_builtin(right, left)
    {
        return true;
    }
    false
}

/// Return whether one runtime type value is a subtype of another.
pub(crate) fn type_values_subtype(left: &JuliaType, right: &JuliaType) -> bool {
    left.is_subtype_of(right)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_struct_name() {
        assert_eq!(normalize_struct_name("Point"), "Point");
        assert_eq!(normalize_struct_name("MyModule.Point"), "Point");
        assert_eq!(normalize_struct_name("Point{Int64}"), "Point{Int64}");
        assert_eq!(
            normalize_struct_name("MyModule.Point{Int64}"),
            "Point{Int64}"
        );
        assert_eq!(normalize_struct_name("A.B.Point{T}"), "Point{T}");
    }

    #[test]
    fn test_normalize_type_for_isa() {
        assert_eq!(
            normalize_type_for_isa("Int"),
            crate::types::native_int_type_name()
        );
        assert_eq!(
            normalize_type_for_isa("UInt"),
            crate::types::native_uint_type_name()
        );
        assert_eq!(normalize_type_for_isa("Float64"), "Float64");
        assert_eq!(
            normalize_type_for_isa("Point{Int}"),
            format!("Point{{{}}}", crate::types::native_int_type_name())
        );
        assert_eq!(
            normalize_type_for_isa("Rational{Int}"),
            format!("Rational{{{}}}", crate::types::native_int_type_name())
        );
        assert_eq!(
            normalize_type_for_isa("Module.Point{Int}"),
            format!("Point{{{}}}", crate::types::native_int_type_name())
        );
    }

    /// Issue #5067: structured value parameters (tuple / nested tuple) render
    /// with a cosmetic space after each comma in `typeof`, while a source
    /// literal omits it. Both spellings must canonicalize to the same form so
    /// `isa` treats them as the same DataType.
    #[test]
    fn test_normalize_type_for_isa_ignores_comma_whitespace() {
        // Tuple value parameter: spaced and tight forms collapse together.
        assert_eq!(
            normalize_type_for_isa("Val{(1, 2)}"),
            normalize_type_for_isa("Val{(1,2)}")
        );
        // Symbol-tuple value parameter.
        assert_eq!(
            normalize_type_for_isa("Val{(:a, :b)}"),
            normalize_type_for_isa("Val{(:a,:b)}")
        );
        // Nested tuple value parameter.
        assert_eq!(
            normalize_type_for_isa("Val{(1, (2, 3))}"),
            normalize_type_for_isa("Val{(1,(2,3))}")
        );
        // Distinct parameters stay distinct after normalization.
        assert_ne!(
            normalize_type_for_isa("Val{(1, 2)}"),
            normalize_type_for_isa("Val{(1, 3)}")
        );
    }

    /// Issue #10410: a rendered `UnionAll` surface form keeps its top-level
    /// ` where ` clause verbatim — the space-stripping normalization must not
    /// fuse it into `...{T}where{S<:Real,T<:S}`, which the structured
    /// `CoreType::from_julia_name` parse can no longer recognize as a
    /// `where` chain (inverting `isa` for every member of the family).
    #[test]
    fn test_normalize_type_for_isa_keeps_top_level_where_forms() {
        for name in [
            "Vector{T} where {S<:Real, T<:S}",
            "MyWrap10410{T} where {S<:Real, T<:S}",
            "W10410{T} where T<:Real",
            "Vector{T} where T",
        ] {
            assert_eq!(normalize_type_for_isa(name), name);
        }
        // A `where` nested inside braces is a plain application: cosmetic
        // spaces still collapse as before.
        assert_eq!(
            normalize_type_for_isa("Val{(1, 2)}"),
            normalize_type_for_isa("Val{(1,2)}")
        );
    }

    /// Alias normalization (Int -> native word type) must still fire for the spaced
    /// `Foo{X, Int}` rendering after the cosmetic spaces are dropped.
    #[test]
    fn test_normalize_type_for_isa_alias_with_spaces() {
        let native_int = crate::types::native_int_type_name();
        assert_eq!(
            normalize_type_for_isa("Pair{String, Int}"),
            normalize_type_for_isa(&format!("Pair{{String,{native_int}}}"))
        );
        assert_eq!(
            normalize_type_for_isa("Pair{Int, String}"),
            format!("Pair{{{native_int},String}}")
        );
    }

    #[test]
    fn test_type_objects_equal_uses_mutual_subtyping() {
        let spaced = JuliaType::Struct("Pair{String, Int64}".to_string());
        let canonical = JuliaType::Struct("Pair{String,Int64}".to_string());
        assert!(type_objects_equal(&spaced, &canonical));
        let qualified = JuliaType::Struct("Main.Pair{String,Int64}".to_string());
        assert!(type_objects_equal(&qualified, &canonical));
        let tuple_vararg_any = JuliaType::from_name("Tuple{Vararg{Any}}").unwrap();
        assert!(type_objects_equal(&JuliaType::Tuple, &tuple_vararg_any));
        assert!(!type_objects_equal(&JuliaType::Int64, &JuliaType::Float64));
    }

    #[test]
    fn unbounded_nominal_named_binder_fails_closed_before_structural_rebinding_10460() {
        let shadowed = JuliaType::UnionAll {
            var: "Module".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::Struct("Array{Module,1}".to_string())),
        };
        let vector = JuliaType::Struct("Vector".to_string());

        assert!(!unbounded_unionall_alias_equivalent(&shadowed, &vector));
        assert!(!type_objects_identical(&shadowed, &vector));
    }

    #[test]
    fn structured_array_alias_params_do_not_use_display_names_10460() {
        let binder = JuliaType::TypeVar("Q".to_string(), None);
        assert_eq!(
            generic_alias_body_parts(&JuliaType::VectorOf(Box::new(binder.clone()))),
            Some(("Vector".to_string(), vec!["Q".to_string()]))
        );
        assert_eq!(
            generic_alias_body_parts(&JuliaType::MatrixOf(Box::new(binder))),
            Some(("Matrix".to_string(), vec!["Q".to_string()]))
        );
        let unrelated = JuliaType::UnionAll {
            var: "Q".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::Int64))),
        };
        assert_eq!(unionall_generic_alias_parts(&unrelated), None);
    }

    #[test]
    fn alpha_renamed_source_unionalls_are_equal_but_not_identical_10613() {
        let wrapper = |var: &str| JuliaType::UnionAll {
            var: var.to_string(),
            lower_bound: Some(Box::new("Signed".to_string())),
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                var.to_string(),
                None,
            )))),
        };
        let left = wrapper("Int64");
        let right = wrapper("T");

        assert!(type_objects_equal(&left, &right));
        assert!(type_values_subtype(&left, &right));
        assert!(type_values_subtype(&right, &left));
        assert!(!type_objects_identical(&left, &right));

        let same_name_left = wrapper("T");
        let same_name_right = wrapper("T");
        assert!(type_objects_identical(&same_name_left, &same_name_right));
    }

    #[test]
    fn test_type_objects_identical_distinguishes_canonical_unionall_alias_issue_9563() {
        let vector_q_unionall = JuliaType::UnionAll {
            var: "Q".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                "Q".to_string(),
                None,
            )))),
        };
        let vector_t_unionall = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                "T".to_string(),
                None,
            )))),
        };
        let vector_alias = JuliaType::Struct("Vector".to_string());
        assert!(type_objects_equal(&vector_q_unionall, &vector_alias));
        assert!(!type_objects_identical(&vector_q_unionall, &vector_alias));
        assert!(type_objects_equal(&vector_t_unionall, &vector_alias));
        assert!(type_objects_identical(&vector_t_unionall, &vector_alias));
        assert!(type_objects_identical(
            &vector_q_unionall,
            &vector_q_unionall
        ));
        assert!(type_objects_identical(
            &JuliaType::Tuple,
            &JuliaType::Struct("Tuple{Vararg{Any}}".to_string())
        ));
        assert!(type_objects_identical(
            &JuliaType::Tuple,
            &JuliaType::TupleOf(vec![JuliaType::Struct("Vararg{Any}".to_string())])
        ));
        assert!(type_objects_identical(
            &JuliaType::Struct("DenseVector{Int64}".to_string()),
            &JuliaType::Struct("DenseArray{Int64,1}".to_string())
        ));
        assert!(type_objects_identical(
            &JuliaType::Struct("DenseMatrix{Float64}".to_string()),
            &JuliaType::Struct("DenseArray{Float64,2}".to_string())
        ));

        let box_q_unionall = JuliaType::UnionAll {
            var: "Q".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::Struct("Box3909{Q}".to_string())),
        };
        let box_t_unionall = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::Struct("Box3909{T}".to_string())),
        };
        let box_alias = JuliaType::Struct("Box3909".to_string());
        assert!(type_objects_equal(&box_q_unionall, &box_alias));
        assert!(!type_objects_identical(&box_q_unionall, &box_alias));
        assert!(type_objects_equal(&box_t_unionall, &box_alias));
        assert!(!type_objects_identical(&box_t_unionall, &box_alias));

        let tuple_t_unionall = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::TupleOf(vec![JuliaType::TypeVar(
                "T".to_string(),
                None,
            )])),
        };
        assert!(!type_objects_equal(&tuple_t_unionall, &JuliaType::Tuple));
    }

    #[test]
    fn test_type_values_subtype_uses_julia_subtype_relation() {
        assert!(type_values_subtype(&JuliaType::Int64, &JuliaType::Real));
        assert!(type_values_subtype(
            &JuliaType::Struct("Vector{Int64}".to_string()),
            &JuliaType::AbstractArray
        ));
        assert!(!type_values_subtype(&JuliaType::String, &JuliaType::Number));
    }

    #[test]
    fn runtime_unionall_identity_keeps_distinct_free_typevar_ids_10613() {
        let runtime_var = |id, name: &str| JuliaType::RuntimeTypeVar {
            id,
            name: name.to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let wrapper = |bound_id, free_id| {
            let bound = runtime_var(bound_id, "T");
            JuliaType::RuntimeUnionAll {
                var: Box::new(bound.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "Pair".to_string(),
                    params: vec![bound, runtime_var(free_id, "F")],
                }),
            }
        };
        let left = wrapper(1, 10);
        let right = wrapper(2, 11);

        assert!(!type_objects_equal(&left, &right));
        assert!(!type_objects_identical(&left, &right));
        assert!(!type_values_subtype(&left, &right));
        assert!(!type_values_subtype(&right, &left));

        let swapped = |bound_id, first_free_id, second_free_id| {
            let bound = runtime_var(bound_id, "T");
            JuliaType::RuntimeUnionAll {
                var: Box::new(bound.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "Triple".to_string(),
                    params: vec![
                        bound,
                        runtime_var(first_free_id, "F"),
                        runtime_var(second_free_id, "F"),
                    ],
                }),
            }
        };
        let ordered = swapped(3, 20, 21);
        let reversed = swapped(4, 21, 20);
        assert!(!type_objects_equal(&ordered, &reversed));
        assert!(!type_objects_identical(&ordered, &reversed));
        assert!(!type_values_subtype(&ordered, &reversed));
        assert!(!type_values_subtype(&reversed, &ordered));

        let bounded = |first_bound_id, second_bound_id, first_free_id, second_free_id| {
            let first = JuliaType::RuntimeTypeVar {
                id: first_bound_id,
                name: "A".to_string(),
                lower_bound: Box::new(JuliaType::Bottom),
                upper_bound: Box::new(runtime_var(first_free_id, "F")),
            };
            let second = JuliaType::RuntimeTypeVar {
                id: second_bound_id,
                name: "B".to_string(),
                lower_bound: Box::new(JuliaType::Bottom),
                upper_bound: Box::new(runtime_var(second_free_id, "F")),
            };
            JuliaType::RuntimeUnionAll {
                var: Box::new(first.clone()),
                body: Box::new(JuliaType::RuntimeUnionAll {
                    var: Box::new(second.clone()),
                    body: Box::new(JuliaType::RuntimeParametric {
                        base: "BoundPair".to_string(),
                        params: vec![first, second],
                    }),
                }),
            }
        };
        let bounded_ordered = bounded(5, 6, 30, 31);
        let bounded_alpha_equivalent = bounded(7, 8, 30, 31);
        assert!(type_objects_equal(
            &bounded_ordered,
            &bounded_alpha_equivalent
        ));
        assert!(type_objects_identical(
            &bounded_ordered,
            &bounded_alpha_equivalent
        ));
        assert!(type_values_subtype(
            &bounded_ordered,
            &bounded_alpha_equivalent
        ));
        assert!(type_values_subtype(
            &bounded_alpha_equivalent,
            &bounded_ordered
        ));
        let bounded_reversed = bounded(7, 8, 31, 30);
        assert!(!type_objects_equal(&bounded_ordered, &bounded_reversed));
        assert!(!type_objects_identical(&bounded_ordered, &bounded_reversed));
        assert!(!type_values_subtype(&bounded_ordered, &bounded_reversed));
        assert!(!type_values_subtype(&bounded_reversed, &bounded_ordered));
    }

    #[test]
    fn runtime_unionall_equality_preserves_wrapper_and_alpha_renames_binder_10613() {
        let runtime_var = |id, name: &str| JuliaType::RuntimeTypeVar {
            id,
            name: name.to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Real),
        };
        let wrapper = |id, name: &str| {
            let bound = runtime_var(id, name);
            JuliaType::RuntimeUnionAll {
                var: Box::new(bound.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "Vector".to_string(),
                    params: vec![bound],
                }),
            }
        };

        let module = wrapper(40, "Module");
        let canonical = wrapper(41, "T");
        assert!(type_objects_equal(&module, &canonical));
        assert!(!type_objects_equal(
            &module,
            &JuliaType::Struct("Vector{Module}".to_string())
        ));
    }
}

#[cfg(test)]
mod issue_10460_tests {
    use super::{type_objects_identical, JuliaType};

    #[test]
    fn fresh_partial_runtime_unionall_is_equal_but_not_identical_10460() {
        let binder = JuliaType::RuntimeTypeVar {
            id: 1,
            name: "N".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let runtime = JuliaType::RuntimeUnionAll {
            var: Box::new(binder.clone()),
            body: Box::new(JuliaType::RuntimeParametric {
                base: "Partial10460".to_string(),
                params: vec![JuliaType::Int8, binder],
            }),
        };
        let folded = JuliaType::Struct("Partial10460{Int8}".to_string());
        let mismatched = JuliaType::Struct("Partial10460{Int16}".to_string());
        let bare = JuliaType::Struct("Partial10460".to_string());
        let bounded_binder = JuliaType::RuntimeTypeVar {
            id: 2,
            name: "N".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Real),
        };
        let bounded_runtime = JuliaType::RuntimeUnionAll {
            var: Box::new(bounded_binder.clone()),
            body: Box::new(JuliaType::RuntimeParametric {
                base: "Partial10460".to_string(),
                params: vec![JuliaType::Int8, bounded_binder],
            }),
        };

        assert!(!type_objects_identical(&folded, &runtime));
        assert!(!type_objects_identical(&runtime, &folded));
        assert!(!type_objects_identical(&mismatched, &runtime));
        assert!(!type_objects_identical(&bare, &runtime));
        assert!(!type_objects_identical(&folded, &bounded_runtime));
        assert!(!type_objects_identical(&bounded_runtime, &folded));
    }
}
