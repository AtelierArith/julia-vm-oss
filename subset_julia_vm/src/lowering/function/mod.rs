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
//! - `kw_defaults`: Per-call keyword-argument default re-evaluation (Issue #5121)
//! - `where_clause`: Where clause and type parameter handling

mod defaults;
mod full_form;
mod kw_defaults;
mod short_form;
mod signature;
pub(crate) mod where_clause;

#[cfg(test)]
mod tests;

// Re-export public functions
pub(crate) use defaults::generate_default_arg_stubs;
pub use full_form::{
    lower_anonymous_function_named, lower_anonymous_function_value, lower_function,
    lower_function_all, lower_function_all_with_ctx, lower_function_with_ctx,
};
pub(crate) use signature::{inject_parameter_destructuring_prologue, parse_parameter};
// Re-exported for value-position `where`-expression lowering (Issue #5047): the
// right-hand `where {T, N}` / `where T<:Number` constraint parser is shared with
// the declaration-position path so both interpret bounds identically.
pub use short_form::{
    is_lambda_assignment, is_short_function_definition, lower_arrow_function_with_name,
    lower_lambda_assignment, lower_operator_method, lower_operator_method_with_ctx,
    lower_short_function, lower_short_function_all, lower_short_function_all_with_ctx,
    lower_short_function_with_ctx,
};
pub(crate) use where_clause::parse_type_constraints;
