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
    // Two named structs whose module-stripped, alias-normalized names are equal
    // denote the same `DataType` (Issue #8100). A module-private short type
    // referenced bare inside its module (`E` -> `JuliaType::Struct("E")`) and the
    // same type spelled qualified (`M2.E` -> `JuliaType::Struct("M2.E")`) must be
    // `===`. The mutual-subtype path below cannot decide this: a 1-char name like
    // `E` is parsed into a `CoreType::TypeVar` by the string-level type-variable
    // heuristic, so the qualified/bare pair never reconciles there. This is purely
    // additive — it only recognizes already-equal type names, mirroring the
    // module-prefix stripping the subtype engine already applies to longer names.
    if let (JuliaType::Struct(a), JuliaType::Struct(b)) = (left, right) {
        if normalize_type_for_isa(a) == normalize_type_for_isa(b) {
            return true;
        }
    }
    type_values_subtype(left, right) && type_values_subtype(right, left)
}

/// Return whether one runtime type value is a subtype of another.
pub(crate) fn type_values_subtype(left: &JuliaType, right: &JuliaType) -> bool {
    left.is_subtype_of(right)
}

#[cfg(test)]
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
    fn test_type_values_subtype_uses_julia_subtype_relation() {
        assert!(type_values_subtype(&JuliaType::Int64, &JuliaType::Real));
        assert!(type_values_subtype(
            &JuliaType::Struct("Vector{Int64}".to_string()),
            &JuliaType::AbstractArray
        ));
        assert!(!type_values_subtype(&JuliaType::String, &JuliaType::Number));
    }
}
