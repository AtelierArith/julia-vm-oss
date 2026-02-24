//! Function definition lowering.
//!
//! This module handles lowering of all function definition forms:
//! - Full-form: `function f(...) ... end`
//! - Short-form: `f(x) = expr`
//! - Lambda: `f = x -> expr`
//! - Operator methods: `+(a, b) = ...`
//!
//! Submodules handle specific aspects:
//! - `full_form`: Full function definitions
//! - `short_form`: Short-form, lambda, and operator definitions
//! - `signature`: Signature and parameter parsing
//! - `defaults`: Default argument extraction and stub generation
//! - `where_clause`: Where clause and type parameter handling

mod defaults;
mod full_form;
mod short_form;
mod signature;
mod where_clause;

#[cfg(test)]
mod tests;

// Re-export public functions
pub use full_form::{lower_function, lower_function_all};
pub use short_form::{
    is_lambda_assignment, is_short_function_definition, lower_arrow_function_with_name,
    lower_lambda_assignment, lower_operator_method, lower_short_function, lower_short_function_all,
};
