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
// `signature::parse_kwparam_node` is the single authority for lowering a post-`;`
// keyword parameter; the arrow-lambda collector in `lowering/expr/call.rs` calls
// it so the arrow and named-function forms cannot drift apart (Issue #10354).
pub mod signature;
pub mod where_clause;

#[cfg(test)]
mod tests;

// Re-export public functions
pub use defaults::{
    extract_defaults_from_function_def, extract_defaults_from_short_function,
    generate_default_arg_stubs,
};
pub use full_form::{
    lower_anonymous_function_named, lower_anonymous_function_value, lower_function,
    lower_function_all, lower_function_all_with_ctx, lower_function_with_ctx,
};
pub use kw_defaults::inject_kwarg_default_prologues;
pub use signature::{
    inject_parameter_destructuring_prologue, parameter_binding_names, parse_parameter,
};
// Re-exported for value-position `where`-expression lowering (Issue #5047): the
// right-hand `where {T, N}` / `where T<:Number` constraint parser is shared with
// the declaration-position path so both interpret bounds identically.
pub use short_form::{
    is_lambda_assignment, is_short_function_definition, lower_arrow_function_with_name,
    lower_lambda_assignment, lower_lambda_assignment_with_ctx, lower_operator_method,
    lower_operator_method_with_ctx, lower_short_function, lower_short_function_all,
    lower_short_function_all_with_ctx, lower_short_function_with_ctx,
};
pub use where_clause::parse_type_constraints;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

/// Reject a `global` method definition lexically inside a function body,
/// matching upstream's lowering error
/// `syntax: Global method definition around <loc> needs to be placed at the
/// top level, or use "eval".` (verified against julia 1.12.6; Issue #10937).
///
/// A top-level `let` body is NOT a function body — the Base bootstrap pattern
/// `let ... global function include(...) ... end ... end` must keep lowering
/// to a module-scope method — so this check runs as a pre-scan over the body
/// CST of each function being lowered rather than keying off `lambda_ctx`
/// (which does not distinguish the two).
///
/// Quoted code and unexpanded macro arguments are skipped: upstream's
/// documented escape hatch is exactly `@eval`/`quote`.
pub fn reject_global_method_definitions_in_body<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<()> {
    match walker.kind(&node) {
        NodeKind::QuoteStatement | NodeKind::QuoteExpression | NodeKind::MacroCall => {
            return Ok(());
        }
        NodeKind::GlobalStatement => {
            for child in walker.named_children(&node) {
                let is_method_definition = walker.kind(&child) == NodeKind::FunctionDefinition
                    || short_form::is_short_function_definition(walker, child);
                if is_method_definition {
                    let span = walker.span(&child);
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::Other(format!(
                            "syntax: Global method definition around line {} needs to be placed at the top level, or use \"eval\".",
                            span.start_line
                        )),
                        span,
                    ));
                }
            }
        }
        _ => {}
    }
    for child in walker.named_children(&node) {
        reject_global_method_definitions_in_body(walker, child)?;
    }
    Ok(())
}
