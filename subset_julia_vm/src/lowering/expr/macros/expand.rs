//! User-defined and Base macro expansion in expression context.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::{get_node_macro_type, LowerResult, MacroParamType};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use super::super::lower_expr_with_ctx;
use super::super::quote::quote_constructor_to_code_with_varargs;
#[cfg(debug_assertions)]
use super::macro_debug_log;

pub(crate) fn lower_macroexpand_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@macroexpand requires a macro call as argument"),
        );
    }

    let inner = args[0];
    if walker.kind(&inner) != NodeKind::MacroCall {
        return lower_expr_with_ctx(walker, inner, lambda_ctx);
    }

    let macro_ident = walker.find_child(&inner, NodeKind::MacroIdentifier);
    let inner_macro_name = match macro_ident {
        Some(ident) => walker.text(&ident).trim_start_matches('@').to_string(),
        None => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("@macroexpand: could not find macro identifier"),
            );
        }
    };

    let _args_node = walker.find_child(&inner, NodeKind::MacroArgumentList);
    let direct_args: Vec<_> = walker
        .named_children(&inner)
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
        .collect();

    #[cfg(debug_assertions)]
    macro_debug_log(format_args!("@macroexpand @{}", inner_macro_name));

    let inner_span = walker.span(&inner);

    if lambda_ctx.has_macro(&inner_macro_name) {
        let expanded = expand_user_defined_macro_expr(
            walker,
            &inner_macro_name,
            &direct_args,
            inner_span,
            lambda_ctx,
        )?;
        #[cfg(debug_assertions)]
        macro_debug_log(format_args!("  Expanded to: {:?}", expanded));
        return Ok(expanded);
    }

    if crate::base_loader::has_base_macro(&inner_macro_name) {
        let expanded = expand_base_macro_expr(
            walker,
            &inner_macro_name,
            &direct_args,
            inner_span,
            lambda_ctx,
        )?;
        #[cfg(debug_assertions)]
        macro_debug_log(format_args!("  Expanded to: {:?}", expanded));
        return Ok(expanded);
    }

    let result = if crate::base_loader::has_base_macro(&inner_macro_name) {
        expand_base_macro_expr(
            walker,
            &inner_macro_name,
            &direct_args,
            inner_span,
            lambda_ctx,
        )
    } else {
        Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, inner_span)
                .with_hint(format!("unknown macro @{}", inner_macro_name)),
        )
    };

    if let Ok(ref expanded) = result {
        #[cfg(debug_assertions)]
        macro_debug_log(format_args!("  Expanded to: {:?}", expanded));
    }

    result
}

/// Expand a user-defined macro call in expression context.
pub(crate) fn expand_user_defined_macro_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let arg_types: Vec<MacroParamType> = args
        .iter()
        .map(|arg| get_node_macro_type(walker, arg))
        .collect();

    let macro_def = lambda_ctx
        .get_macro_with_types(macro_name, &arg_types)
        .or_else(|| lambda_ctx.get_macro_with_arity(macro_name, args.len()))
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} not found (with {} args)",
                macro_name,
                args.len()
            ))
        })?;

    let min_args = if macro_def.has_varargs {
        macro_def.params.len() - 1
    } else {
        macro_def.params.len()
    };
    if args.len() < min_args {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} expects at least {} arguments, got {}",
                macro_name,
                min_args,
                args.len()
            )),
        );
    }

    let stmts = &macro_def.body.stmts;

    if stmts.is_empty() {
        return Ok(Expr::Literal(Literal::Nothing, span));
    }

    if stmts.len() == 1 {
        return match &stmts[0] {
            crate::ir::core::Stmt::Expr { expr, .. } => {
                substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )
            }
            crate::ir::core::Stmt::Return {
                value: Some(expr), ..
            } => {
                substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )
            }
            _ => Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    "user-defined macro expansion currently only supports macros that return expressions",
                ),
            ),
        };
    }

    let mut expanded_stmts = Vec::new();
    for stmt in stmts {
        match stmt {
            crate::ir::core::Stmt::Expr {
                expr,
                span: stmt_span,
            } => {
                let expanded = substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(crate::ir::core::Stmt::Expr {
                    expr: expanded,
                    span: *stmt_span,
                });
            }
            crate::ir::core::Stmt::Assign {
                var,
                value,
                span: stmt_span,
            } => {
                let expanded_value = substitute_params_in_macro_expr(
                    value,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(crate::ir::core::Stmt::Assign {
                    var: var.clone(),
                    value: expanded_value,
                    span: *stmt_span,
                });
            }
            crate::ir::core::Stmt::Return {
                value: Some(expr),
                span: stmt_span,
            } => {
                let expanded = substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(crate::ir::core::Stmt::Expr {
                    expr: expanded,
                    span: *stmt_span,
                });
            }
            _ => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                        "user-defined macro expansion currently only supports expression and assignment statements",
                    ),
                );
            }
        }
    }

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: crate::ir::core::Block {
            stmts: expanded_stmts,
            span,
        },
        span,
    })
}

/// Expand a Base macro call in expression context.
pub(crate) fn expand_base_macro_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let macro_def = crate::base_loader::get_base_macro(macro_name).ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
            .with_hint(format!("Base macro @{} not found", macro_name))
    })?;

    let min_args = if macro_def.has_varargs {
        macro_def.params.len() - 1
    } else {
        macro_def.params.len()
    };
    if args.len() < min_args {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "macro @{} expects at least {} arguments, got {}",
                macro_name,
                min_args,
                args.len()
            )),
        );
    }

    let stmts = &macro_def.body.stmts;

    if stmts.is_empty() {
        return Ok(Expr::Literal(Literal::Nothing, span));
    }

    if stmts.len() == 1 {
        return match &stmts[0] {
            crate::ir::core::Stmt::Expr { expr, .. } => {
                substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )
            }
            crate::ir::core::Stmt::Return {
                value: Some(expr), ..
            } => {
                substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )
            }
            _ => Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    "Base macro expansion currently only supports macros that return expressions",
                ),
            ),
        };
    }

    let mut expanded_stmts = Vec::new();
    for stmt in stmts {
        match stmt {
            crate::ir::core::Stmt::Expr {
                expr,
                span: stmt_span,
            } => {
                let expanded = substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(crate::ir::core::Stmt::Expr {
                    expr: expanded,
                    span: *stmt_span,
                });
            }
            crate::ir::core::Stmt::Assign {
                var,
                value,
                span: stmt_span,
            } => {
                let expanded_value = substitute_params_in_macro_expr(
                    value,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(crate::ir::core::Stmt::Assign {
                    var: var.clone(),
                    value: expanded_value,
                    span: *stmt_span,
                });
            }
            crate::ir::core::Stmt::Return {
                value: Some(expr),
                span: stmt_span,
            } => {
                let expanded = substitute_params_in_macro_expr(
                    expr,
                    &macro_def.params,
                    args,
                    walker,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(crate::ir::core::Stmt::Expr {
                    expr: expanded,
                    span: *stmt_span,
                });
            }
            _ => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                        "Base macro expansion currently only supports expression and assignment statements",
                    ),
                );
            }
        }
    }

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: crate::ir::core::Block {
            stmts: expanded_stmts,
            span,
        },
        span,
    })
}

/// Substitute macro parameters in an expression with actual arguments.
pub(crate) fn substitute_params_in_macro_expr<'a>(
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
                    if args.len() > fixed_param_count {
                        let varargs_exprs: Result<Vec<_>, _> = args[fixed_param_count..]
                            .iter()
                            .map(|arg| lower_expr_with_ctx(walker, *arg, lambda_ctx))
                            .collect();
                        Ok(Expr::TupleLiteral {
                            elements: varargs_exprs?,
                            span: *span,
                        })
                    } else {
                        Ok(Expr::TupleLiteral {
                            elements: vec![],
                            span: *span,
                        })
                    }
                } else {
                    lower_expr_with_ctx(walker, args[idx], lambda_ctx)
                }
            } else {
                Ok(Expr::Var(name.clone(), *span))
            }
        }
        Expr::QuoteLiteral { constructor, span } => {
            quote_constructor_to_code_with_varargs(
                constructor,
                params,
                args,
                *span,
                walker,
                lambda_ctx,
                has_varargs,
            )
        }
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => {
            let new_left = substitute_params_in_macro_expr(left, params, args, walker, lambda_ctx, has_varargs)?;
            let new_right = substitute_params_in_macro_expr(right, params, args, walker, lambda_ctx, has_varargs)?;
            Ok(Expr::BinaryOp {
                op: *op,
                left: Box::new(new_left),
                right: Box::new(new_right),
                span: *span,
            })
        }
        Expr::UnaryOp { op, operand, span } => {
            let new_operand = substitute_params_in_macro_expr(operand, params, args, walker, lambda_ctx, has_varargs)?;
            Ok(Expr::UnaryOp {
                op: *op,
                operand: Box::new(new_operand),
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
            if function == "string" && call_args.len() == 1 && kwargs.is_empty() {
                if let Expr::Var(param_name, _) = &call_args[0] {
                    if let Some(idx) = params.iter().position(|p| p == param_name) {
                        let arg_text = walker.text(&args[idx]);
                        return Ok(Expr::Literal(Literal::Str(arg_text.to_string()), *span));
                    }
                }
            }

            let new_args: Result<Vec<_>, _> = call_args
                .iter()
                .map(|a| substitute_params_in_macro_expr(a, params, args, walker, lambda_ctx, has_varargs))
                .collect();
            let new_kwargs: Result<Vec<_>, _> = kwargs
                .iter()
                .map(|(k, v)| {
                    substitute_params_in_macro_expr(v, params, args, walker, lambda_ctx, has_varargs)
                        .map(|nv| (k.clone(), nv))
                })
                .collect();
            Ok(Expr::Call {
                function: function.clone(),
                args: new_args?,
                kwargs: new_kwargs?,
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
            if *name == crate::ir::core::BuiltinOp::Esc && builtin_args.len() == 1 {
                return substitute_params_in_macro_expr(
                    &builtin_args[0], params, args, walker, lambda_ctx, has_varargs,
                );
            }

            let new_args: Result<Vec<_>, _> = builtin_args
                .iter()
                .map(|a| substitute_params_in_macro_expr(a, params, args, walker, lambda_ctx, has_varargs))
                .collect();
            Ok(Expr::Builtin {
                name: *name,
                args: new_args?,
                span: *span,
            })
        }
        _ => Ok(expr.clone()),
    }
}
