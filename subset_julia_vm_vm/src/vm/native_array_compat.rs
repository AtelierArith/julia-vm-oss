//! Native-array carrier dispatch-boundary helpers.
//!
//! Since the carrier confinement (Issue #6888) the explicit native array carrier
//! is the ExprArgs variant, confined to the `expr.args` representation. Its layout
//! is known only to the `native_array_*` boundary helpers in
//! `vm/value/array_value` and `vm/value/array_wrapper`; no other module pattern-
//! matches the variant or reads its fields directly. This module owns the
//! *dispatch-fence* policy — which parameter slots the carrier may satisfy and
//! which Base functions are exempt — keeping it out of dispatch call sites
//! (Issues #6337/#6834).
//!
//! High-level carrier queries (`is_native_array_value`, the `native_array_*`
//! accessors) live next to the `Value` definition and are re-exported here so
//! `native_array_compat` is a single import surface for the boundary.

use super::value::{native_array_value_ref, Value};
use super::ArrayRef;

// Re-export the high-level carrier predicate (defined next to the `Value`
// carrier in `vm/value/array_value`) so callers can treat `native_array_compat`
// as the boundary surface (Issue #6834).
pub(crate) use super::value::is_native_array_value;

/// True when `param_type` names the Pure Julia `Array{T,N}` wrapper or one of
/// its alias projections (`Vector{T}`, `Matrix{T}`, `Array{T,...}`), or the
/// storage-level `Memory{T}` wrapper. View wrappers (`SubArray`, `ReshapedArray`,
/// `MatrixView`) are included for the same reason: their methods read wrapper
/// fields such as `len`, `parent`, or `dims` that native arrays do not carry.
/// Wrapper/storage methods access fields or allocate storage for a different
/// value kind, so the transitional native carrier must not satisfy these
/// parameter slots during dispatch (Issues #3908/#4189/#6337/#9778).
#[inline]
pub(crate) fn is_array_wrapper_param_type(param_type: &crate::types::JuliaType) -> bool {
    use crate::types::JuliaType;
    match param_type {
        JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => true,
        JuliaType::Struct(name) => {
            let base = name
                .split('{')
                .next()
                .unwrap_or(name)
                .rsplit('.')
                .next()
                .unwrap_or(name);
            matches!(
                base,
                "Array" | "Memory" | "SubArray" | "ReshapedArray" | "MatrixView"
            )
        }
        _ => false,
    }
}

pub(crate) fn params_cross_native_array_wrapper_boundary(
    args: &[Value],
    param_types: &[crate::types::JuliaType],
) -> bool {
    args.iter()
        .zip(param_types.iter())
        .any(|(arg, param_ty)| is_native_array_value(arg) && is_array_wrapper_param_type(param_ty))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeArrayFenceDecision {
    Exempt,
    Fenced,
}

// Issue #9820: keep the internal Array helper family near the native-array
// wrapper fence decision. Public wrappers stay fenced; only narrow helpers that
// know how to bridge the transitional native carrier are exempt.
const BASE_ARRAY_NATIVE_ARRAY_FENCE_DECISIONS: &[(&str, NativeArrayFenceDecision)] = &[
    ("empty", NativeArrayFenceDecision::Exempt),
    ("_array_dims", NativeArrayFenceDecision::Exempt),
    ("_array_memory", NativeArrayFenceDecision::Exempt),
    ("_array_offset", NativeArrayFenceDecision::Exempt),
    ("_array_reshape_tuple", NativeArrayFenceDecision::Exempt),
    ("_array_similar_tuple", NativeArrayFenceDecision::Fenced),
    (
        "_array_similar_typed_tuple",
        NativeArrayFenceDecision::Fenced,
    ),
];

/// Whether a Base function (identified by name) is exempt from the native-array
/// wrapper dispatch fence (#3908/#4189).
///
/// Issue #6336: this name match runs once per function at program install.
/// Runtime dispatch consults the precomputed per-function flag instead of
/// matching names for every candidate.
pub(crate) fn base_function_accepts_native_array_value(name: &str) -> bool {
    let bare = name.strip_prefix("Base.").unwrap_or(name);
    BASE_ARRAY_NATIVE_ARRAY_FENCE_DECISIONS
        .iter()
        .any(|(helper, decision)| *helper == bare && *decision == NativeArrayFenceDecision::Exempt)
}

#[inline]
pub(crate) fn native_array_value_ptr_eq(left: &Value, right: &Value) -> bool {
    match (native_array_value_ref(left), native_array_value_ref(right)) {
        (Some(a), Some(b)) => std::ptr::eq(a.as_ptr(), b.as_ptr()),
        _ => false,
    }
}

#[inline]
pub(crate) fn native_array_ref_from_borrowed_value(value: &Value) -> Option<&ArrayRef> {
    native_array_value_ref(value)
}

#[cfg(test)]
mod tests {
    use super::is_array_wrapper_param_type;
    use crate::types::JuliaType;

    #[test]
    fn native_array_boundary_includes_view_wrappers_issue_9778() {
        assert!(is_array_wrapper_param_type(&JuliaType::Struct(
            "SubArray{Float64}".to_string()
        )));
        assert!(is_array_wrapper_param_type(&JuliaType::Struct(
            "Base.ReshapedArray{Float64,1,Vector{Float64},Tuple{}}".to_string()
        )));
        assert!(is_array_wrapper_param_type(&JuliaType::Struct(
            "MatrixView{Int64}".to_string()
        )));
        assert!(!is_array_wrapper_param_type(&JuliaType::Struct(
            "NotAnArrayWrapper{Float64}".to_string()
        )));
    }

    #[test]
    fn native_array_exemptions_include_array_dims_issue_4419() {
        use super::base_function_accepts_native_array_value;

        let expectations = [
            ("empty", true),
            ("Base.empty", true),
            ("_array_dims", true),
            ("Base._array_dims", true),
            ("_array_memory", true),
            ("Base._array_memory", true),
            ("_array_offset", true),
            ("Base._array_offset", true),
            ("_array_reshape_tuple", true),
            ("Base._array_reshape_tuple", true),
            ("_array_similar_tuple", false),
            ("Base._array_similar_tuple", false),
            ("_array_similar_typed_tuple", false),
            ("Base._array_similar_typed_tuple", false),
            ("size", false),
            ("length", false),
            ("reshape", false),
            ("similar", false),
        ];

        for (name, expected) in expectations {
            assert_eq!(
                base_function_accepts_native_array_value(name),
                expected,
                "unexpected native-array fence decision for {name}"
            );
        }
    }

    #[test]
    fn native_array_helper_fence_decision_table_has_unique_rows_9820() {
        use super::BASE_ARRAY_NATIVE_ARRAY_FENCE_DECISIONS;

        for (idx, (left, _)) in BASE_ARRAY_NATIVE_ARRAY_FENCE_DECISIONS.iter().enumerate() {
            for (right, _) in BASE_ARRAY_NATIVE_ARRAY_FENCE_DECISIONS.iter().skip(idx + 1) {
                assert_ne!(left, right, "duplicate native-array helper decision row");
            }
        }
    }
}
