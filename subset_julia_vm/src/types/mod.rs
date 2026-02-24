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
mod type_expr;
mod type_param;

#[cfg(test)]
mod tests;

pub use dispatch_error::DispatchError;
pub use julia_type::{JuliaType, Variance};
pub use type_expr::TypeExpr;
pub use type_param::TypeParam;

// Re-export for tests only (is_type_variable_name is an internal helper)
#[cfg(test)]
pub(crate) use julia_type::is_type_variable_name;
