//! Quote constructor to executable code conversion.
//!
//! Converts quote constructor expressions (built by `cst_to_expr_constructor`)
//! into actual executable IR code for macro expansion.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node};

use super::super::lower_expr_with_ctx;
use super::handlers::{
    collect_introduced_vars, extract_symbol_from_constructor, handle_block_expr, handle_call_expr,
    handle_for_expr, handle_if_expr, handle_macrocall_expr, handle_try_expr, handle_tuple_expr,
    handle_while_expr,
};
use super::HygieneContext;

/// Convert a quote constructor expression to actual executable code.
/// This is used for macro expansion where we need to "eval" the quote.
///
/// For example:
/// - `Builtin(ExprNew, [SymbolNew("call"), SymbolNew("*"), 2, x])` -> `BinaryOp(Mul, 2, x)`
/// - `Builtin(SymbolNew, ["foo"])` -> `Var("foo")` (for non-macro context, but here it's an identifier)
///
/// This function now implements macro hygiene:
/// 1. First pass: collect all variables introduced in the macro (not inside esc())
/// 2. Second pass: rename those variables to gensym'd names
pub fn quote_constructor_to_code<'a>(
    constructor: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    // Default: no varargs
    quote_constructor_to_code_with_varargs(
        constructor,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        false,
    )
}

/// Convert a quote constructor to executable code with varargs support.
pub fn quote_constructor_to_code_with_varargs<'a>(
    constructor: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Pass 1: Collect variables that need hygiene renaming
    let mut hygiene = HygieneContext::new();
    collect_introduced_vars(constructor, &mut hygiene, false);

    // Pass 2: Convert with hygiene applied
    quote_constructor_to_code_with_hygiene(
        constructor,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        &hygiene,
        has_varargs,
    )
}

/// Convert a quote constructor to executable code with varargs and local bindings support.
/// Local bindings are variables assigned in the macro body that should be substituted at expansion time.
pub fn quote_constructor_to_code_with_locals<'a>(
    constructor: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
    has_varargs: bool,
    local_bindings: &std::collections::HashMap<String, Expr>,
) -> LowerResult<Expr> {
    // Pre-substitute local bindings in the constructor
    // This replaces Var("expr_str") with the actual value from local_bindings
    let substituted_constructor =
        substitute_local_bindings_in_constructor(constructor, local_bindings);

    // Pass 1: Collect variables that need hygiene renaming
    let mut hygiene = HygieneContext::new();
    collect_introduced_vars(&substituted_constructor, &mut hygiene, false);

    // Pass 2: Convert with hygiene applied
    quote_constructor_to_code_with_hygiene(
        &substituted_constructor,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        &hygiene,
        has_varargs,
    )
}

/// Recursively substitute local bindings in a quote constructor.
/// This handles variables that were assigned in the macro body (like `expr_str = string(ex)`).
fn substitute_local_bindings_in_constructor(
    constructor: &Expr,
    local_bindings: &std::collections::HashMap<String, Expr>,
) -> Expr {
    // Fast path: no bindings to substitute, skip tree traversal
    if local_bindings.is_empty() {
        return constructor.clone();
    }
    match constructor {
        Expr::Var(name, _span) => {
            // Check if this variable is a local binding
            if let Some(bound_value) = local_bindings.get(name) {
                bound_value.clone()
            } else {
                constructor.clone()
            }
        }
        Expr::Builtin { name, args, span } => {
            // Recursively substitute in builtin arguments
            let new_args: Vec<Expr> = args
                .iter()
                .map(|arg| substitute_local_bindings_in_constructor(arg, local_bindings))
                .collect();
            Expr::Builtin {
                name: *name,
                args: new_args,
                span: *span,
            }
        }
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => {
            let new_args: Vec<Expr> = args
                .iter()
                .map(|arg| substitute_local_bindings_in_constructor(arg, local_bindings))
                .collect();
            let new_kwargs: Vec<(String, Expr)> = kwargs
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        substitute_local_bindings_in_constructor(v, local_bindings),
                    )
                })
                .collect();
            Expr::Call {
                function: function.clone(),
                args: new_args,
                kwargs: new_kwargs,
                splat_mask: splat_mask.clone(),
                kwargs_splat_mask: kwargs_splat_mask.clone(),
                span: *span,
            }
        }
        Expr::TupleLiteral { elements, span } => {
            let new_elements: Vec<Expr> = elements
                .iter()
                .map(|e| substitute_local_bindings_in_constructor(e, local_bindings))
                .collect();
            Expr::TupleLiteral {
                elements: new_elements,
                span: *span,
            }
        }
        // For other expressions, return as-is (literals, etc.)
        _ => constructor.clone(),
    }
}

/// Convert an Expr to a Block by wrapping it in Stmt::Expr if needed.
pub(super) fn expr_to_block(expr: Expr, span: crate::span::Span) -> crate::ir::core::Block {
    crate::ir::core::Block {
        stmts: vec![crate::ir::core::Stmt::Expr { expr, span }],
        span,
    }
}

/// Internal function that does the actual conversion with hygiene context.
pub(super) fn quote_constructor_to_code_with_hygiene<'a>(
    constructor: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    match constructor {
        // ExprNew(:call, :op, arg1, arg2, ...) -> actual call/operation
        Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args: builtin_args,
            ..
        } => {
            if builtin_args.len() < 2 {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "ExprNew with too few arguments".to_string(),
                    ),
                    span,
                ));
            }

            // First arg should be SymbolNew("call") or similar
            let head = extract_symbol_from_constructor(&builtin_args[0])?;

            match head.as_str() {
                "call" => handle_call_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                "block" => handle_block_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                "macrocall" => handle_macrocall_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                "tuple" => handle_tuple_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                "try" => handle_try_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                "if" => handle_if_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                "for" => handle_for_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                "while" => handle_while_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                _ => {
                    // Other expression heads not yet supported
                    Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(format!(
                            "quote expansion for Expr(:{}, ...) not yet supported",
                            head
                        )),
                        span,
                    ))
                }
            }
        }

        // SymbolNew("name") in executable context becomes a variable reference
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args: builtin_args,
            ..
        } => {
            if let Some(Expr::Literal(Literal::Str(name), _)) = builtin_args.first() {
                // Check if this is a macro parameter
                if let Some(idx) = params.iter().position(|p| p == name) {
                    // Check if this is the varargs parameter (last param when has_varargs=true)
                    if has_varargs && idx == params.len() - 1 {
                        // Collect remaining arguments into a tuple
                        let fixed_param_count = params.len() - 1;
                        let vararg_exprs: Result<Vec<_>, _> = args[fixed_param_count..]
                            .iter()
                            .map(|arg| lower_expr_with_ctx(walker, *arg, lambda_ctx))
                            .collect();
                        Ok(Expr::TupleLiteral {
                            elements: vararg_exprs?,
                            span,
                        })
                    } else {
                        lower_expr_with_ctx(walker, args[idx], lambda_ctx)
                    }
                } else {
                    // Apply hygiene renaming if applicable
                    let resolved_name = hygiene.resolve(name);
                    Ok(Expr::Var(resolved_name, span))
                }
            } else {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "SymbolNew with non-string argument".to_string(),
                    ),
                    span,
                ))
            }
        }

        // Variable reference - might be a macro parameter
        Expr::Var(name, _) => {
            if let Some(idx) = params.iter().position(|p| p == name) {
                // Check if this is the varargs parameter (last param when has_varargs=true)
                if has_varargs && idx == params.len() - 1 {
                    // Collect remaining arguments into a tuple
                    let fixed_param_count = params.len() - 1;
                    let vararg_exprs: Result<Vec<_>, _> = args[fixed_param_count..]
                        .iter()
                        .map(|arg| lower_expr_with_ctx(walker, *arg, lambda_ctx))
                        .collect();
                    Ok(Expr::TupleLiteral {
                        elements: vararg_exprs?,
                        span,
                    })
                } else {
                    lower_expr_with_ctx(walker, args[idx], lambda_ctx)
                }
            } else {
                // Apply hygiene renaming if applicable
                let resolved_name = hygiene.resolve(name);
                Ok(Expr::Var(resolved_name, span))
            }
        }

        // Literals stay as literals
        Expr::Literal(lit, _) => Ok(Expr::Literal(lit.clone(), span)),

        // For other expressions, just substitute parameters
        _ => super::super::macros::substitute_params_in_macro_expr(
            constructor,
            params,
            args,
            walker,
            lambda_ctx,
            false,
        ),
    }
}
