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
/// storage-level `Memory{T}` wrapper. Wrapper/storage methods access fields or
/// allocate storage for a different value kind, so the transitional native
/// carrier must not satisfy these parameter slots during dispatch (Issues
/// #3908/#4189/#6337).
#[inline]
pub(crate) fn is_array_wrapper_param_type(param_type: &crate::types::JuliaType) -> bool {
    use crate::types::JuliaType;
    match param_type {
        JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) => true,
        JuliaType::Struct(name) => name.starts_with("Array{") || name.starts_with("Memory{"),
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

/// Whether a Base function (identified by name) is exempt from the native-array
/// wrapper dispatch fence (#3908/#4189).
///
/// Issue #6336: this name match runs once per function at program install.
/// Runtime dispatch consults the precomputed per-function flag instead of
/// matching names for every candidate.
pub(crate) fn base_function_accepts_native_array_value(name: &str) -> bool {
    matches!(name.strip_prefix("Base.").unwrap_or(name), "empty")
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
