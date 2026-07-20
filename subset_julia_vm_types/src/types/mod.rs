//! Julia type hierarchy for SubsetJuliaVM multiple dispatch.
//!
//! This module defines the type system used for method dispatch.
//!
//! # Module Organization
//!
//! - `julia_type/`: Variance, JuliaType enum, parsing, comparison, display
//! - `type_expr.rs`: TypeExpr for parametric type expressions
//! - `type_param.rs`: TypeParam for type parameter declarations
//! - `dispatch_error.rs`: DispatchError for method dispatch failures

mod builtin_type_registry;
mod dispatch_error;
mod julia_type;
mod native_word;
mod struct_hierarchy;
mod type_expr;
mod type_param;

#[cfg(test)]
mod tests;

pub use builtin_type_registry::{
    builtin_type_binding_authority, builtin_type_for_compiler, builtin_type_for_parser,
    builtin_type_for_reflection, BuiltinTypeBindingAuthority,
};
pub use dispatch_error::DispatchError;
// Changed from pub(crate) to pub so the main `subset_julia_vm` crate can access
// these via the `crate::types::` re-export path (Issue #8655).
pub use julia_type::unbounded_vararg_element;
pub use julia_type::{canonicalize_union, canonicalize_union_with_identity};
pub use julia_type::{
    struct_owners_compatible, JuliaType, Variance, SOURCE_ANONYMOUS_TYPEVAR_NAME,
};
pub use native_word::{
    native_int_julia_type, native_int_type_name, native_uint_julia_type, native_uint_type_name,
};
pub use struct_hierarchy::{
    base_bare_nominal_origin_conflict, base_bare_nominal_origin_conflict_with,
    explicit_sibling_nominal_family_conflict, has_qualified_nominal_family_collision,
    is_registered_type_name, nominal_family_name, nominal_family_names_compatible,
    nominal_type_names_compatible, qualified_family_name, register_type_name, StructHierarchy,
    StructHierarchyEntry,
};
pub use type_expr::{
    parse_parametric_call, parse_single_type_expr, parse_type_args_recursive, TypeExpr,
};
pub use type_param::TypeParam;
