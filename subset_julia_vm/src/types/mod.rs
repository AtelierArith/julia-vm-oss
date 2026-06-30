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

mod dispatch_error;
mod julia_type;
mod native_word;
mod struct_hierarchy;
mod type_expr;
mod type_param;

#[cfg(test)]
mod tests;

pub use dispatch_error::DispatchError;
pub(crate) use julia_type::canonicalize_union;
pub(crate) use julia_type::unbounded_vararg_element;
pub use julia_type::{JuliaType, Variance};
pub(crate) use native_word::{
    native_int_julia_type, native_int_type_name, native_uint_julia_type, native_uint_type_name,
};
pub use struct_hierarchy::{nominal_family_name, StructHierarchy, StructHierarchyEntry};
pub use type_expr::TypeExpr;
pub use type_param::TypeParam;

// Re-export for tests only (is_type_variable_name is an internal helper)
#[cfg(test)]
pub(crate) use julia_type::is_type_variable_name;
