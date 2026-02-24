//! Type name normalization utilities for struct/type comparisons.
//!
//! These utilities are used across multiple builtin modules for
//! consistent type name handling during equality and isa checks.

use std::borrow::Cow;

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
/// Strips module prefix and normalizes type aliases (Int -> Int64, UInt -> UInt64).
///
/// Returns `Cow::Borrowed` when no normalization is needed (the common case),
/// avoiding heap allocation entirely.
pub(crate) fn normalize_type_for_isa(name: &str) -> Cow<'_, str> {
    // First strip module prefix
    let stripped = normalize_struct_name(name);

    // Normalize type aliases in type parameters
    // e.g., "Point{Int}" -> "Point{Int64}", "Rational{Int}" -> "Rational{Int64}"
    if let Some(brace_idx) = stripped.find('{') {
        let params = &stripped[brace_idx..];

        // Fast path: if no alias keywords appear in params, borrow as-is.
        if !params_need_normalization(params) {
            return Cow::Borrowed(stripped);
        }

        let base = &stripped[..brace_idx];

        // Replace type aliases in parameters
        let normalized_params = params
            .replace("{Int}", "{Int64}")
            .replace("{UInt}", "{UInt64}")
            .replace(", Int}", ", Int64}")
            .replace(", UInt}", ", UInt64}")
            .replace("{Int,", "{Int64,")
            .replace("{UInt,", "{UInt64,");

        Cow::Owned(format!("{}{}", base, normalized_params))
    } else {
        // No type params, check if the name itself is an alias
        match stripped {
            "Int" => Cow::Borrowed("Int64"),
            "UInt" => Cow::Borrowed("UInt64"),
            _ => Cow::Borrowed(stripped),
        }
    }
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
        assert_eq!(normalize_type_for_isa("Int"), "Int64");
        assert_eq!(normalize_type_for_isa("UInt"), "UInt64");
        assert_eq!(normalize_type_for_isa("Float64"), "Float64");
        assert_eq!(normalize_type_for_isa("Point{Int}"), "Point{Int64}");
        assert_eq!(normalize_type_for_isa("Rational{Int}"), "Rational{Int64}");
        assert_eq!(normalize_type_for_isa("Module.Point{Int}"), "Point{Int64}");
    }
}
