//! Quote constructor to executable code conversion.
//!
//! Converts quote constructor expressions (built by `cst_to_expr_constructor`)
//! into actual executable IR code for macro expansion.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::expr_heads::ExprHead;
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::super::lower_expr_with_ctx;
use super::handlers::{
    collect_introduced_vars, extract_symbol_from_constructor, handle_block_expr, handle_call_expr,
    handle_for_expr, handle_if_expr, handle_macrocall_expr, handle_try_expr, handle_tuple_expr,
    handle_while_expr,
};
use super::HygieneContext;

/// Map an identifier that is one of Julia's value-keywords to its
/// literal value, when present.
///
/// In a quoted expression these names are stored as the Symbols
/// `:nothing` / `:missing` / `:true` / `:false` (Issue #4895). When the
/// quote is converted back into executable code (macro expansion), they
/// must resolve to the corresponding literal value rather than a `Var`
/// reference — otherwise a macro body whose `quote ... end` block ends
/// in a bare `nothing` raises `UndefVarError: nothing not defined`.
fn value_keyword_literal(name: &str) -> Option<Literal> {
    match name {
        "nothing" => Some(Literal::Nothing),
        "missing" => Some(Literal::Missing),
        "true" => Some(Literal::Bool(true)),
        "false" => Some(Literal::Bool(false)),
        _ => None,
    }
}

fn lower_quote_template_arg<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    match walker.kind(&node) {
        NodeKind::UnaryExpression if walker.text(&node).starts_with('$') => {
            let children = walker.named_children(&node);
            let Some(inner) = children.last().copied() else {
                return lower_expr_with_ctx(walker, node, lambda_ctx);
            };
            match walker.kind(&inner) {
                NodeKind::ParenthesizedExpression => {
                    let paren_children = walker.named_children(&inner);
                    if let Some(paren_inner) = paren_children.first().copied() {
                        return lower_quote_template_arg(walker, paren_inner, lambda_ctx);
                    }
                    lower_expr_with_ctx(walker, inner, lambda_ctx)
                }
                _ => lower_quote_template_arg(walker, inner, lambda_ctx),
            }
        }
        NodeKind::SplatExpression => {
            let children = walker.named_children(&node);
            if let Some(inner) = children.first().copied() {
                lower_expr_with_ctx(walker, inner, lambda_ctx)
            } else {
                lower_expr_with_ctx(walker, node, lambda_ctx)
            }
        }
        _ => lower_expr_with_ctx(walker, node, lambda_ctx),
    }
}

fn substitute_quote_template_params<'a>(
    expr: &Expr,
    params: &[String],
    args: &[Node<'a>],
    walker: &CstWalker<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    match expr {
        Expr::Var(name, span) => {
            if let Some(idx) = params.iter().position(|p| p == name) {
                if has_varargs && idx == params.len() - 1 {
                    let fixed_param_count = params.len() - 1;
                    let elements: Result<Vec<_>, _> = args[fixed_param_count..]
                        .iter()
                        .map(|arg| lower_quote_template_arg(walker, *arg, lambda_ctx))
                        .collect();
                    Ok(Expr::TupleLiteral {
                        elements: elements?,
                        span: *span,
                    })
                } else {
                    lower_quote_template_arg(walker, args[idx], lambda_ctx)
                }
            } else {
                Ok(Expr::Var(name.clone(), *span))
            }
        }
        Expr::QuoteLiteral { constructor, span } => quote_constructor_to_code_with_varargs(
            constructor,
            params,
            args,
            *span,
            walker,
            lambda_ctx,
            has_varargs,
        ),
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => {
            let left = substitute_quote_template_params(
                left,
                params,
                args,
                walker,
                lambda_ctx,
                has_varargs,
            )?;
            let right = substitute_quote_template_params(
                right,
                params,
                args,
                walker,
                lambda_ctx,
                has_varargs,
            )?;
            Ok(Expr::BinaryOp {
                op: *op,
                left: Box::new(left),
                right: Box::new(right),
                span: *span,
            })
        }
        Expr::UnaryOp { op, operand, span } => {
            let operand = substitute_quote_template_params(
                operand,
                params,
                args,
                walker,
                lambda_ctx,
                has_varargs,
            )?;
            Ok(Expr::UnaryOp {
                op: *op,
                operand: Box::new(operand),
                span: *span,
            })
        }
        Expr::Call {
            function,
            args: call_args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => {
            let new_call_args = call_args
                .iter()
                .map(|arg| {
                    substitute_quote_template_params(
                        arg,
                        params,
                        args,
                        walker,
                        lambda_ctx,
                        has_varargs,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let new_kwargs = kwargs
                .iter()
                .map(|(key, value)| {
                    substitute_quote_template_params(
                        value,
                        params,
                        args,
                        walker,
                        lambda_ctx,
                        has_varargs,
                    )
                    .map(|value| (key.clone(), value))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Call {
                function: function.clone(),
                args: new_call_args,
                kwargs: new_kwargs,
                splat_mask: splat_mask.clone(),
                kwargs_splat_mask: kwargs_splat_mask.clone(),
                span: *span,
            })
        }
        Expr::Builtin {
            name,
            args: builtin_args,
            span,
        } => {
            if *name == BuiltinOp::SplatInterpolation && builtin_args.len() == 1 {
                if let Expr::Var(param_name, _) = &builtin_args[0] {
                    if let Some(idx) = params.iter().position(|p| p == param_name) {
                        let arg = args[idx];
                        if walker.kind(&arg) == NodeKind::SplatExpression {
                            if let Some(inner) = walker.named_children(&arg).first() {
                                return Ok(Expr::Builtin {
                                    name: *name,
                                    args: vec![lower_quote_template_arg(
                                        walker, *inner, lambda_ctx,
                                    )?],
                                    span: *span,
                                });
                            }
                        }
                    }
                }
            }
            if *name == BuiltinOp::Esc && builtin_args.len() == 1 {
                return substitute_quote_template_params(
                    &builtin_args[0],
                    params,
                    args,
                    walker,
                    lambda_ctx,
                    has_varargs,
                );
            }
            let new_builtin_args = builtin_args
                .iter()
                .map(|arg| {
                    substitute_quote_template_params(
                        arg,
                        params,
                        args,
                        walker,
                        lambda_ctx,
                        has_varargs,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Builtin {
                name: *name,
                args: new_builtin_args,
                span: *span,
            })
        }
        _ => Ok(expr.clone()),
    }
}

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
            if *name == BuiltinOp::SymbolNew {
                if let Some(Expr::Literal(Literal::Str(symbol_name), _)) = args.first() {
                    if let Some(binding_name) = symbol_name.strip_prefix('$') {
                        if let Some(bound_value) = local_bindings.get(binding_name) {
                            return bound_value.clone();
                        }
                    }
                }
            }
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

            match ExprHead::from_name(&head) {
                Some(ExprHead::Call) => handle_call_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Block) => handle_block_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::MacroCall) => handle_macrocall_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Tuple) => handle_tuple_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Try) => handle_try_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::If) => handle_if_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::For) => handle_for_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::While) => handle_while_expr(
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
                } else if let Some(lit) = value_keyword_literal(name) {
                    // Issue #4895: `nothing` / `missing` (and `true` /
                    // `false`) quote to the `:nothing` / `:missing`
                    // Symbols, so a macro body that ends its
                    // `quote ... end` block with a bare `nothing`
                    // reaches here as `SymbolNew("nothing")`. When the
                    // quoted block is converted *back* into executable
                    // code (macro-expansion-scope), these identifiers
                    // must resolve to their literal value rather than a
                    // `Var` reference that would raise `UndefVarError`.
                    Ok(Expr::Literal(lit, span))
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
        _ => substitute_quote_template_params(
            constructor,
            params,
            args,
            walker,
            lambda_ctx,
            has_varargs,
        ),
    }
}
