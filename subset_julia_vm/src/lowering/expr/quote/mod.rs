//! Quote expression handling for expression lowering.
//!
//! This module handles:
//! - Quote expressions (`:symbol`, `:(expr)`)
//! - CST to Expr constructor conversion
//! - Quote constructor to executable code conversion
//! - Macro hygiene (HygieneContext, variable renaming)
//!
//! # Module Organization
//!
//! - `hygiene.rs`: HygieneContext for macro hygiene tracking
//! - `cst_to_constructor.rs`: CST to Expr constructor conversion (lower_quote_expr, cst_to_expr_constructor)
//! - `code_generation.rs`: Quote constructor to executable code conversion
//! - `handlers.rs`: Expression head handlers for macro expansion + hygiene helpers
//!
//! The logic is organized into two main phases:
//! 1. CST → Quote Constructor: `cst_to_expr_constructor` converts CST to Expr builders
//! 2. Quote Constructor → Code: `quote_constructor_to_code` evaluates the constructors

mod code_generation;
mod cst_to_constructor;
pub(super) mod handlers;
mod hygiene;

pub(in crate::lowering) use hygiene::HygieneContext;

pub use code_generation::{
    quote_constructor_to_code, quote_constructor_to_code_with_locals,
    quote_constructor_to_code_with_varargs,
};
pub(crate) use cst_to_constructor::lower_quote_expr;
pub(super) use handlers::{collect_introduced_vars, extract_symbol_from_constructor};
