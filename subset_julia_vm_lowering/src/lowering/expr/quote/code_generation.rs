//! Quote constructor to executable code conversion.
//!
//! Converts quote constructor expressions (built by `cst_to_expr_constructor`)
//! into actual executable IR code for macro expansion.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::expr_heads::{tracked_static_quote_gap_issue, ExprHead};
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::super::lower_expr_with_ctx;
use super::handlers::{
    collect_introduced_vars, extract_symbol_from_constructor, handle_arrow_expr, handle_block_expr,
    handle_call_expr, handle_comprehension_expr, handle_elseif_expr, handle_for_expr,
    handle_function_expr, handle_generator_expr, handle_if_expr, handle_let_expr,
    handle_macrocall_expr, handle_try_expr, handle_tuple_expr, handle_where_expr,
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
            let children = walker.named_children_vec(&node);
            let Some(inner) = children.last().copied() else {
                return lower_expr_with_ctx(walker, node, lambda_ctx);
            };
            match walker.kind(&inner) {
                NodeKind::ParenthesizedExpression => {
                    let paren_children = walker.named_children_vec(&inner);
                    if let Some(paren_inner) = paren_children.first().copied() {
                        return lower_quote_template_arg(walker, paren_inner, lambda_ctx);
                    }
                    lower_expr_with_ctx(walker, inner, lambda_ctx)
                }
                _ => lower_quote_template_arg(walker, inner, lambda_ctx),
            }
        }
        NodeKind::SplatExpression => {
            let children = walker.named_children_vec(&node);
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
                Ok(Expr::Var(*name, *span))
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
                    .map(|value| (*key, value))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::Call {
                function: *function,
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
                            if let Some(inner) = walker.named_children_vec(&arg).first() {
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
            if let Some(bound_value) = local_bindings.get(name.as_str()) {
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
            let new_kwargs: Vec<(crate::ir::core::InternedStr, Expr)> = kwargs
                .iter()
                .map(|(k, v)| {
                    (
                        *k,
                        substitute_local_bindings_in_constructor(v, local_bindings),
                    )
                })
                .collect();
            Expr::Call {
                function: *function,
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

/// Mirrors the outer dispatch match in
/// [`quote_constructor_to_code_with_hygiene`] below, checked against the
/// declared `static_quote_top_level` column of the shared per-head registry
/// (`expr_heads.rs`) via a `debug_assert_eq!` at the top of that match
/// (Issue #10627) -- the same anti-drift pattern the dynamic path already
/// uses for its own `macro_return_to_stmt`/`macro_return_to_expr` columns
/// (`macro_return_stmt_support`/`macro_return_expr_support` in
/// `macro_runtime.rs`).
fn static_quote_top_level_dispatch_support(head: Option<ExprHead>) -> bool {
    matches!(
        head,
        Some(
            ExprHead::Call
                | ExprHead::Block
                | ExprHead::MacroCall
                | ExprHead::Tuple
                | ExprHead::Try
                | ExprHead::If
                | ExprHead::ElseIf
                | ExprHead::For
                | ExprHead::While
                | ExprHead::Let
                // Issues #10916/#10617: nested function definitions, `where`
                // type values, comprehensions/generators, and arrow lambdas.
                | ExprHead::Function
                | ExprHead::Where
                | ExprHead::Comprehension
                | ExprHead::Generator
                | ExprHead::Arrow
        )
    )
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
            let head_enum = ExprHead::from_name(&head);
            // Issue #10627: keep this dispatcher's real coverage in sync with
            // the declared `static_quote_top_level` column of the shared
            // per-head registry (`expr_heads.rs`), mirroring the
            // `macro_return_to_stmt`/`macro_return_to_expr` consistency
            // checks the dynamic path already performs against the same
            // registry (`macro_runtime.rs`).
            debug_assert_eq!(
                head_enum
                    .map(|h| h.spec().static_quote_top_level)
                    .unwrap_or(false),
                static_quote_top_level_dispatch_support(head_enum),
                "registry/dispatch drift for Expr(:{head}, ...): update EXPR_HEAD_REGISTRY's \
                 static_quote_top_level column or this match's arm list so they agree"
            );

            match head_enum {
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
                Some(ExprHead::ElseIf) => handle_elseif_expr(
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
                Some(ExprHead::Let) => handle_let_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Function) => handle_function_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Where) => handle_where_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Comprehension) => handle_comprehension_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Generator) => handle_generator_expr(
                    builtin_args,
                    params,
                    args,
                    span,
                    walker,
                    lambda_ctx,
                    hygiene,
                    has_varargs,
                ),
                Some(ExprHead::Arrow) => handle_arrow_expr(
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
                    // Other expression heads not yet supported. When the head
                    // is a KNOWN gap tracked by a follow-up Issue (Issue
                    // #10627), name it in the error so a differential test
                    // (or a developer) can assert the documented error rather
                    // than a bare "not yet supported".
                    let hint = match head_enum.and_then(tracked_static_quote_gap_issue) {
                        Some(issue) => format!(
                            "quote expansion for Expr(:{head}, ...) not yet supported \
                             (tracked by Issue #{issue})"
                        ),
                        None => format!("quote expansion for Expr(:{head}, ...) not yet supported"),
                    };
                    Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(hint),
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
                    Ok(Expr::Var(resolved_name.into(), span))
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
                Ok(Expr::Var(resolved_name.into(), span))
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod static_pass2_codegen_tests {
    //! Issues #10916/#10617: static Pass-2 codegen for
    //! `Function`/`Where`/`Comprehension`/`Generator`/`Arrow` heads. None of
    //! these heads is reachable end-to-end through any current real
    //! stdlib macro's quote body (see "Static Pass-2 reachability" in
    //! docs/vm/LOWERING.md — the static engine's only entry is a
    //! statement-position stdlib macro), so these tests drive the constructor
    //! tree directly, exactly like the `collect_introduced_vars_tests`
    //! precedent in `handlers.rs` (Issues #10626/#10627/#10980).

    use super::*;
    use crate::lowering::LambdaContext;
    use crate::span::Span;

    fn span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn symbol(name: &str) -> Expr {
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args: vec![Expr::Literal(Literal::Str(name.to_string()), span())],
            span: span(),
        }
    }

    fn expr_new(args: Vec<Expr>) -> Expr {
        Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args,
            span: span(),
        }
    }

    fn int_literal(n: i64) -> Expr {
        Expr::Literal(Literal::Int(n), span())
    }

    /// `Expr(:call, :+, arg, arg)`-style constructor.
    fn call(op: &str, lhs: Expr, rhs: Expr) -> Expr {
        expr_new(vec![symbol("call"), symbol(op), lhs, rhs])
    }

    fn convert(constructor: &Expr, lambda_ctx: &LambdaContext) -> LowerResult<Expr> {
        let walker = CstWalker::new("");
        quote_constructor_to_code(constructor, &[], &[], span(), &walker, lambda_ctx)
    }

    fn collect_local_decl_and_assignment_names(
        block: &crate::ir::core::Block,
        declarations: &mut Vec<String>,
        assignments: &mut Vec<String>,
    ) {
        for stmt in &block.stmts {
            match stmt {
                crate::ir::core::Stmt::LocalDecl { var, kind, .. } => match kind {
                    crate::ir::core::LocalDeclKind::Explicit => declarations.push(var.clone()),
                    crate::ir::core::LocalDeclKind::CompilerEnclosing => {}
                },
                crate::ir::core::Stmt::Assign { var, .. } => assignments.push(var.clone()),
                crate::ir::core::Stmt::Block(nested) => {
                    collect_local_decl_and_assignment_names(nested, declarations, assignments);
                }
                _ => {}
            }
        }
    }

    #[test]
    fn local_constructor_preserves_explicit_declarations_11415() {
        // Expr(:block, Expr(:local, Expr(:(=), :assigned, 1), :bare, :second))
        // exercises initialized and bare declarations, including every
        // argument of a multi-name `local` form.
        let constructor = expr_new(vec![
            symbol("block"),
            expr_new(vec![
                symbol("local"),
                expr_new(vec![symbol("="), symbol("assigned"), int_literal(1)]),
                symbol("bare"),
                symbol("second"),
            ]),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::LetBlock { body, .. } = result else {
            panic!("expected LetBlock, got {result:?}");
        };

        let mut declarations = Vec::new();
        let mut assignments = Vec::new();
        collect_local_decl_and_assignment_names(&body, &mut declarations, &mut assignments);

        for prefix in ["#assigned#", "#bare#", "#second#"] {
            assert!(
                declarations.iter().any(|name| name.starts_with(prefix)),
                "missing explicit local with prefix {prefix:?}: {declarations:?}"
            );
        }
        let assigned = assignments
            .iter()
            .find(|name| name.starts_with("#assigned#"))
            .unwrap_or_else(|| panic!("missing initialized local assignment: {assignments:?}"));
        assert!(
            declarations.contains(assigned),
            "initialized local declaration and assignment must use the same hygienic name"
        );
    }

    // ── Arrow (Issue #10617) ────────────────────────────────────────────────

    #[test]
    fn arrow_lambda_lifts_a_function_and_yields_a_function_ref_10617() {
        // Expr(:->, :x, Expr(:call, :+, :x, 1))
        let constructor = expr_new(vec![
            symbol("->"),
            symbol("x"),
            call("+", symbol("x"), int_literal(1)),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::FunctionRef { name, .. } = &result else {
            panic!("expected FunctionRef, got {result:?}");
        };
        let lifted = lambda_ctx.take_lifted_functions();
        assert_eq!(lifted.len(), 1);
        assert_eq!(lifted[0].name, name.to_string());
        assert_eq!(lifted[0].params.len(), 1);
        assert_eq!(lifted[0].params[0].name, "x");
        assert!(matches!(
            lifted[0].body.stmts.as_slice(),
            [crate::ir::core::Stmt::Return { value: Some(_), .. }]
        ));
    }

    #[test]
    fn arrow_lambda_tuple_params_and_typed_param_are_read_10617() {
        // Expr(:->, Expr(:tuple, :a, Expr(:(::), :b, :Int)), Expr(:call, :*, :a, :b))
        let constructor = expr_new(vec![
            symbol("->"),
            expr_new(vec![
                symbol("tuple"),
                symbol("a"),
                expr_new(vec![symbol("::"), symbol("b"), symbol("Int")]),
            ]),
            call("*", symbol("a"), symbol("b")),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        assert!(matches!(result, Expr::FunctionRef { .. }));
        let lifted = lambda_ctx.take_lifted_functions();
        assert_eq!(lifted.len(), 1);
        assert_eq!(lifted[0].params.len(), 2);
        assert_eq!(lifted[0].params[0].name, "a");
        assert_eq!(lifted[0].params[1].name, "b");
        assert!(lifted[0].params[1].type_annotation.is_some());
    }

    #[test]
    fn arrow_param_sharing_a_quote_local_name_collapses_onto_its_gensym_10617() {
        // block( x = 10; f = Expr(:->, :x, x + 1) ): the whole-expansion local
        // `x` is Pass-1 registered; the lambda's own `x` param (flat map, like
        // upstream's own flat per-expansion rename) collapses onto the SAME
        // gensym in both the parameter list and the body.
        let constructor = expr_new(vec![
            symbol("block"),
            expr_new(vec![symbol("="), symbol("x"), int_literal(10)]),
            expr_new(vec![
                symbol("="),
                symbol("f"),
                expr_new(vec![
                    symbol("->"),
                    symbol("x"),
                    call("+", symbol("x"), int_literal(1)),
                ]),
            ]),
        ]);
        let lambda_ctx = LambdaContext::new();
        convert(&constructor, &lambda_ctx).unwrap();
        let lifted = lambda_ctx.take_lifted_functions();
        assert_eq!(lifted.len(), 1);
        let param_name = lifted[0].params[0].name.clone();
        assert_ne!(
            param_name, "x",
            "param should collapse onto the local's gensym"
        );
        assert!(param_name.starts_with("#x#"), "got {param_name:?}");
    }

    // ── Function (Issue #10916) ─────────────────────────────────────────────

    #[test]
    fn named_function_definition_emits_function_def_with_renamed_name_10916() {
        // Expr(:function, Expr(:call, :helper, :n), Expr(:call, :+, :n, 1))
        let constructor = expr_new(vec![
            symbol("function"),
            expr_new(vec![symbol("call"), symbol("helper"), symbol("n")]),
            call("+", symbol("n"), int_literal(1)),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::LetBlock { bindings, body, .. } = &result else {
            panic!("expected LetBlock, got {result:?}");
        };
        assert!(bindings.is_empty());
        let [crate::ir::core::Stmt::FunctionDef { func, .. }] = body.stmts.as_slice() else {
            panic!("expected a single FunctionDef, got {:?}", body.stmts);
        };
        // Pass 1 registers the introduced function NAME (like the dynamic
        // path's #8064 behavior), so the definition is gensym'd.
        assert_ne!(func.name, "helper");
        assert!(func.name.starts_with("#helper#"), "got {:?}", func.name);
        // The parameter itself stays unregistered (flat-map safety,
        // #10626/#10925) — call-frame scoping isolates it at runtime.
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "n");
    }

    #[test]
    fn named_function_call_site_in_same_quote_renames_consistently_10916() {
        // block( function helper() 1 end; helper() ) — the sibling call site
        // resolves to the SAME gensym as the definition.
        let constructor = expr_new(vec![
            symbol("block"),
            expr_new(vec![
                symbol("function"),
                expr_new(vec![symbol("call"), symbol("helper")]),
                int_literal(1),
            ]),
            expr_new(vec![symbol("call"), symbol("helper")]),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::LetBlock { body, .. } = &result else {
            panic!("expected LetBlock, got {result:?}");
        };
        let mut def_name = None;
        let mut call_name = None;
        for stmt in &body.stmts {
            match stmt {
                crate::ir::core::Stmt::Expr {
                    expr: Expr::LetBlock { body, .. },
                    ..
                } => {
                    if let [crate::ir::core::Stmt::FunctionDef { func, .. }] = body.stmts.as_slice()
                    {
                        def_name = Some(func.name.clone());
                    }
                }
                crate::ir::core::Stmt::Expr {
                    expr: Expr::Call { function, .. },
                    ..
                } => call_name = Some(function.to_string()),
                _ => {}
            }
        }
        let def_name = def_name.unwrap_or_else(|| panic!("no FunctionDef found"));
        let call_name = call_name.unwrap_or_else(|| panic!("no call found"));
        assert_eq!(def_name, call_name);
        assert!(def_name.starts_with("#helper#"), "got {def_name:?}");
    }

    #[test]
    fn where_wrapped_named_function_contributes_type_params_10916() {
        // Expr(:function, Expr(:where, Expr(:call, :f, Expr(:(::), :x, :T)),
        //      Expr(:<:, :T, :Number)), :x)
        let constructor = expr_new(vec![
            symbol("function"),
            expr_new(vec![
                symbol("where"),
                expr_new(vec![
                    symbol("call"),
                    symbol("f"),
                    expr_new(vec![symbol("::"), symbol("x"), symbol("T")]),
                ]),
                expr_new(vec![symbol("<:"), symbol("T"), symbol("Number")]),
            ]),
            symbol("x"),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::LetBlock { body, .. } = &result else {
            panic!("expected LetBlock, got {result:?}");
        };
        let [crate::ir::core::Stmt::FunctionDef { func, .. }] = body.stmts.as_slice() else {
            panic!("expected a single FunctionDef, got {:?}", body.stmts);
        };
        assert!(func.name.starts_with("#f#"), "got {:?}", func.name);
        assert_eq!(func.type_params.len(), 1);
        assert_eq!(func.type_params[0].name, "T");
        assert_eq!(func.type_params[0].upper_bound.as_deref(), Some("Number"));
    }

    #[test]
    fn anonymous_function_lifts_a_lambda_like_arrow_10916() {
        // Expr(:function, Expr(:tuple, :n), Expr(:call, :+, :n, 1))
        let constructor = expr_new(vec![
            symbol("function"),
            expr_new(vec![symbol("tuple"), symbol("n")]),
            call("+", symbol("n"), int_literal(1)),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        assert!(matches!(result, Expr::FunctionRef { .. }));
        let lifted = lambda_ctx.take_lifted_functions();
        assert_eq!(lifted.len(), 1);
        assert_eq!(lifted[0].params.len(), 1);
        assert_eq!(lifted[0].params[0].name, "n");
    }

    // ── Where (Issue #10916) ────────────────────────────────────────────────

    #[test]
    fn where_value_binds_typevars_and_wraps_in_unionall_10916() {
        // Expr(:where, :body_type, :T) — a `SomeType where T` bare value.
        let constructor = expr_new(vec![symbol("where"), symbol("body_type"), symbol("T")]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::LetBlock { bindings, body, .. } = &result else {
            panic!("expected LetBlock, got {result:?}");
        };
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0.to_string(), "T");
        assert!(matches!(&bindings[0].1, Expr::Call { function, .. } if *function == "TypeVar"));
        let [crate::ir::core::Stmt::Expr {
            expr: Expr::Call { function, .. },
            ..
        }] = body.stmts.as_slice()
        else {
            panic!("expected a single UnionAll call, got {:?}", body.stmts);
        };
        assert_eq!(function.to_string(), "UnionAll");
    }

    #[test]
    fn where_value_with_upper_bound_passes_bound_to_typevar_10916() {
        // Expr(:where, :body_type, Expr(:<:, :S, :Real))
        let constructor = expr_new(vec![
            symbol("where"),
            symbol("body_type"),
            expr_new(vec![symbol("<:"), symbol("S"), symbol("Real")]),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::LetBlock { bindings, .. } = &result else {
            panic!("expected LetBlock, got {result:?}");
        };
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0.to_string(), "S");
        let Expr::Call { args, .. } = &bindings[0].1 else {
            panic!("expected TypeVar call");
        };
        // TypeVar(:S, Bottom, Real): 3-arg bounded form.
        assert_eq!(args.len(), 3);
    }

    // ── Generator / Comprehension (Issue #10916) ────────────────────────────

    /// `Expr(:generator, body, Expr(:(=), :i, iter))`.
    fn generator_ctor(body: Expr, var: &str, iter: Expr) -> Expr {
        expr_new(vec![
            symbol("generator"),
            body,
            expr_new(vec![symbol("="), symbol(var), iter]),
        ])
    }

    #[test]
    fn generator_produces_generator_ir_with_hygienic_binding_10916() {
        let constructor =
            generator_ctor(call("*", symbol("i"), int_literal(2)), "i", symbol("items"));
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::Generator {
            var, iter, filter, ..
        } = &result
        else {
            panic!("expected Generator, got {result:?}");
        };
        // The binding is an `Expr(:(=), ...)` shape, registered by the
        // generic Pass-1 `Assign` recursion (like a `for` binding), so the
        // induction variable is gensym'd.
        assert!(var.to_string().starts_with("#i#"), "got {var:?}");
        assert!(matches!(**iter, Expr::Var(..)));
        assert!(filter.is_none());
    }

    #[test]
    fn comprehension_produces_comprehension_ir_10916() {
        let constructor = expr_new(vec![
            symbol("comprehension"),
            generator_ctor(call("+", symbol("j"), int_literal(1)), "j", symbol("items")),
        ]);
        let lambda_ctx = LambdaContext::new();
        let result = convert(&constructor, &lambda_ctx).unwrap();
        let Expr::Comprehension { var, filter, .. } = &result else {
            panic!("expected Comprehension, got {result:?}");
        };
        assert!(var.to_string().starts_with("#j#"), "got {var:?}");
        assert!(filter.is_none());
    }

    #[test]
    fn multi_binding_generator_is_rejected_with_10923_hint() {
        // Expr(:generator, body, binding, binding) — two bindings.
        let constructor = expr_new(vec![
            symbol("generator"),
            symbol("body"),
            expr_new(vec![symbol("="), symbol("i"), symbol("xs")]),
            expr_new(vec![symbol("="), symbol("j"), symbol("ys")]),
        ]);
        let lambda_ctx = LambdaContext::new();
        let err = convert(&constructor, &lambda_ctx)
            .err()
            .unwrap_or_else(|| panic!("multi-binding generator unexpectedly succeeded"));
        assert!(err.to_string().contains("10923"), "got {err:?}");
    }

    // ── Catch-all ───────────────────────────────────────────────────────────

    /// A genuinely unknown head (not in `ExprHead` at all) must still report
    /// "not yet supported" WITHOUT a tracked-issue reference — there are no
    /// tracked static Pass-2 gaps after Issues #10916/#10617.
    #[test]
    fn unknown_head_reports_plain_not_yet_supported_without_issue_reference() {
        let constructor = expr_new(vec![symbol("totally-unknown-head-10627"), symbol("arg")]);
        let walker = CstWalker::new("");
        let lambda_ctx = LambdaContext::new();
        let err = quote_constructor_to_code(&constructor, &[], &[], span(), &walker, &lambda_ctx)
            .err()
            .unwrap_or_else(|| panic!("unknown head unexpectedly succeeded"));
        let message = err.to_string();
        assert!(message.contains("not yet supported"), "got {message:?}");
        assert!(!message.contains("Issue #"), "got {message:?}");
    }
}
