//! Macro expansion logic for statement-level macros.
//!
//! This module handles expansion of Base macros, stdlib macros,
//! and user-defined macros in statement context.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Expr, Literal, Stmt};
use crate::lowering::expr;
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::lowering::{get_node_macro_type, MacroParamType};
use crate::parser::cst::{CstWalker, Node};
use crate::span::Span;
use crate::stdlib_loader::get_stdlib_macro;

/// Expand a macro defined in Base (base/macros.jl)
pub(super) fn expand_base_macro<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    macro_name: &str,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Get arguments first to determine arity
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children(&args_node)
    } else if !direct_args.is_empty() {
        direct_args.to_vec()
    } else {
        vec![]
    };

    // Get the macro definition from Base registry with arity-based dispatch
    let macro_def = crate::base_loader::get_base_macro_with_arity(macro_name, args.len())
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "Base macro @{} not found (with {} args)",
                macro_name,
                args.len()
            ))
        })?;

    // Delegate to the common expansion logic
    expand_macro_with_def(
        walker,
        node,
        macro_name,
        args_node,
        direct_args,
        span,
        lambda_ctx,
        &macro_def,
    )
}

/// Expand a macro defined in a stdlib module (e.g., Test::test)
pub(super) fn expand_stdlib_macro<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    module_name: &str,
    macro_name: &str,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Get the macro definition from stdlib registry
    let macro_def = get_stdlib_macro(module_name, macro_name).ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
            "stdlib macro @{} not found in module {}",
            macro_name, module_name
        ))
    })?;

    // Delegate to the common expansion logic
    expand_macro_with_def(
        walker,
        node,
        macro_name,
        args_node,
        direct_args,
        span,
        lambda_ctx,
        &macro_def,
    )
}

/// Common macro expansion logic for both user-defined and Base macros
fn expand_macro_with_def<'a>(
    walker: &CstWalker<'a>,
    _node: Node<'a>,
    macro_name: &str,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
    macro_def: &crate::lowering::StoredMacroDef,
) -> LowerResult<Stmt> {
    // Get arguments from MacroArgumentList or direct children
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children(&args_node)
    } else if !direct_args.is_empty() {
        direct_args.to_vec()
    } else {
        vec![]
    };

    // Check argument count
    // If the macro has varargs, we need at least (params.len() - 1) arguments
    // (all fixed params plus at least 0 varargs)
    // If no varargs, we need exactly params.len() arguments
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

    // Handle macro body based on number of statements
    let stmts = &macro_def.body.stmts;

    if stmts.is_empty() {
        // Empty macro body: return a nothing expression statement
        return Ok(Stmt::Expr {
            expr: Expr::Literal(Literal::Nothing, span),
            span,
        });
    }

    if stmts.len() == 1 {
        // Single statement: check if it's an Expr containing a QuoteLiteral
        let stmt = &stmts[0];
        return match stmt {
            Stmt::Expr { expr, .. } => expand_macro_expr(
                walker,
                expr,
                &macro_def.params,
                &args,
                span,
                lambda_ctx,
                macro_def.has_varargs,
            ),
            Stmt::Return {
                value: Some(expr), ..
            } => expand_macro_expr(
                walker,
                expr,
                &macro_def.params,
                &args,
                span,
                lambda_ctx,
                macro_def.has_varargs,
            ),
            _ => Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    "Base macro expansion currently only supports macros that return expressions",
                ),
            ),
        };
    }

    // Multiple statements: track local bindings for macro-local variables
    // Local variable assignments are evaluated at macro expansion time and their values
    // are substituted into the final expanded code. They are NOT included in the runtime code.
    use std::collections::HashMap;
    let mut local_bindings: HashMap<String, Expr> = HashMap::new();
    let mut expanded_stmts = Vec::new();

    for stmt in stmts {
        match stmt {
            Stmt::Expr {
                expr,
                span: stmt_span,
            } => {
                let expanded = expand_macro_expr_with_locals(
                    walker,
                    expr,
                    &macro_def.params,
                    &args,
                    *stmt_span,
                    lambda_ctx,
                    macro_def.has_varargs,
                    &local_bindings,
                )?;
                expanded_stmts.push(expanded);
            }
            Stmt::Assign {
                var,
                value,
                span: _stmt_span,
            } => {
                // Evaluate the assignment value at macro expansion time
                let expanded_value =
                    substitute_params_in_expr(value, &macro_def.params, &args, walker, lambda_ctx)?;
                // Store the value for later substitution in quotes
                // Do NOT add to expanded_stmts - this is a compile-time binding
                local_bindings.insert(var.clone(), expanded_value);
            }
            Stmt::Return {
                value: Some(expr),
                span: stmt_span,
            } => {
                let expanded = expand_macro_expr_with_locals(
                    walker,
                    expr,
                    &macro_def.params,
                    &args,
                    *stmt_span,
                    lambda_ctx,
                    macro_def.has_varargs,
                    &local_bindings,
                )?;
                expanded_stmts.push(expanded);
            }
            _ => {
                return Err(UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    "Base macro expansion currently only supports expression and assignment statements",
                ));
            }
        }
    }

    // If only one statement remains after filtering assignments, return it directly
    if expanded_stmts.len() == 1 {
        return Ok(expanded_stmts.remove(0));
    }

    // Wrap in a Block statement
    Ok(Stmt::Block(Block {
        stmts: expanded_stmts,
        span,
    }))
}

// ==================== User-Defined Macro Expansion ====================

/// Expand a user-defined macro call.
///
/// This function handles macros defined by the user via `macro name(...) ... end`.
/// The macro body is evaluated at lowering time to produce an Expr, which is then
/// converted back to IR.
///
/// Currently supports simple macros that return a quote expression with interpolation.
pub(super) fn expand_user_defined_macro<'a>(
    walker: &CstWalker<'a>,
    _node: Node<'a>,
    macro_name: &str,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    #[cfg(debug_assertions)]
    super::macro_debug_log(format_args!(
        "[EXPAND] expand_user_defined_macro called for @{}",
        macro_name
    ));
    // Get arguments from MacroArgumentList or direct children FIRST to determine arity
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children(&args_node)
    } else if !direct_args.is_empty() {
        direct_args.to_vec()
    } else {
        vec![]
    };

    // Determine the types of each argument for type-based dispatch
    let arg_types: Vec<MacroParamType> = args
        .iter()
        .map(|arg| get_node_macro_type(walker, arg))
        .collect();

    #[cfg(debug_assertions)]
    super::macro_debug_log(format_args!(
        "[DISPATCH] macro @{}, arg_types={:?}",
        macro_name, arg_types
    ));

    // Get the macro definition with type-based dispatch
    // First try type-based matching, fall back to arity-only matching
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
    #[cfg(debug_assertions)]
    super::macro_debug_log(format_args!(
        "[DISPATCH] selected macro with param_types={:?}",
        macro_def.param_types
    ));

    // Check arity
    // If the macro has varargs, we need at least (params.len() - 1) arguments
    // (all fixed params plus at least 0 varargs)
    // If no varargs, we need exactly params.len() arguments
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

    // Handle macro body based on number of statements
    let stmts = &macro_def.body.stmts;

    if stmts.is_empty() {
        // Empty macro body: return a nothing expression statement
        return Ok(Stmt::Expr {
            expr: Expr::Literal(Literal::Nothing, span),
            span,
        });
    }

    if stmts.len() == 1 {
        // Single statement: check if it's an Expr containing a QuoteLiteral
        let stmt = &stmts[0];
        return match stmt {
            Stmt::Expr { expr, .. } => {
                // For simple identity macros or macros that return their arguments,
                // we can expand them directly
                expand_macro_expr(walker, expr, &macro_def.params, &args, span, lambda_ctx, macro_def.has_varargs)
            }
            Stmt::Return { value: Some(expr), .. } => {
                // Handle explicit return statements
                expand_macro_expr(walker, expr, &macro_def.params, &args, span, lambda_ctx, macro_def.has_varargs)
            }
            _ => Err(UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "user-defined macro expansion currently only supports macros that return expressions",
            )),
        };
    }

    // Multiple statements: expand each and wrap in a Block statement
    let mut expanded_stmts = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Expr {
                expr,
                span: stmt_span,
            } => {
                let expanded = expand_macro_expr(
                    walker,
                    expr,
                    &macro_def.params,
                    &args,
                    *stmt_span,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(expanded);
            }
            Stmt::Assign {
                var,
                value,
                span: stmt_span,
            } => {
                let expanded_value =
                    substitute_params_in_expr(value, &macro_def.params, &args, walker, lambda_ctx)?;
                expanded_stmts.push(Stmt::Assign {
                    var: var.clone(),
                    value: expanded_value,
                    span: *stmt_span,
                });
            }
            Stmt::Return {
                value: Some(expr),
                span: stmt_span,
            } => {
                let expanded = expand_macro_expr(
                    walker,
                    expr,
                    &macro_def.params,
                    &args,
                    *stmt_span,
                    lambda_ctx,
                    macro_def.has_varargs,
                )?;
                expanded_stmts.push(expanded);
            }
            _ => {
                return Err(UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    "user-defined macro expansion currently only supports expression and assignment statements",
                ));
            }
        }
    }

    // Wrap in a Block statement
    Ok(Stmt::Block(Block {
        stmts: expanded_stmts,
        span,
    }))
}

/// Expand a macro expression by substituting parameters with arguments.
fn expand_macro_expr<'a>(
    walker: &CstWalker<'a>,
    expr: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
    has_varargs: bool,
) -> LowerResult<Stmt> {
    use std::collections::HashMap;

    // Build parameter -> argument mapping
    let _param_map: HashMap<&str, &Node<'a>> = params
        .iter()
        .zip(args.iter())
        .map(|(p, a)| (p.as_str(), a))
        .collect();

    // Handle different expression types
    match expr {
        // For QuoteLiteral, convert the quote constructor to executable code
        Expr::QuoteLiteral {
            constructor,
            span: quote_span,
        } => {
            // Convert the quote constructor (e.g., ExprNew(:block, ...)) to executable IR code
            // Pass has_varargs to support varargs parameter expansion in quotes
            let executable = expr::quote_constructor_to_code_with_varargs(
                constructor,
                params,
                args,
                *quote_span,
                walker,
                lambda_ctx,
                has_varargs,
            )?;
            Ok(Stmt::Expr {
                expr: executable,
                span,
            })
        }
        // For variable references, check if it's a parameter and substitute
        Expr::Var(name, _) => {
            // Check if this is a macro parameter
            if let Some(idx) = params.iter().position(|p| p == name) {
                // Check if this is the varargs parameter (last parameter when has_varargs is true)
                if has_varargs && idx == params.len() - 1 {
                    // This is the varargs parameter - collect remaining arguments into a tuple
                    let fixed_param_count = params.len() - 1;
                    if args.len() > fixed_param_count {
                        // Lower remaining arguments and create a tuple
                        let varargs_exprs: Result<Vec<_>, _> = args[fixed_param_count..]
                            .iter()
                            .map(|arg| expr::lower_expr_with_ctx(walker, *arg, lambda_ctx))
                            .collect();
                        let varargs_exprs = varargs_exprs?;
                        let tuple_expr = Expr::TupleLiteral {
                            elements: varargs_exprs,
                            span,
                        };
                        Ok(Stmt::Expr {
                            expr: tuple_expr,
                            span,
                        })
                    } else {
                        // No varargs provided - return empty tuple
                        Ok(Stmt::Expr {
                            expr: Expr::TupleLiteral {
                                elements: vec![],
                                span,
                            },
                            span,
                        })
                    }
                } else {
                    // Regular parameter - lower the corresponding argument
                    let arg_expr = expr::lower_expr_with_ctx(walker, args[idx], lambda_ctx)?;
                    Ok(Stmt::Expr {
                        expr: arg_expr,
                        span,
                    })
                }
            } else {
                // Not a parameter, return as-is
                Ok(Stmt::Expr {
                    expr: expr.clone(),
                    span,
                })
            }
        }
        // Fallback: return other expressions as-is
        _ => Ok(Stmt::Expr {
            expr: expr.clone(),
            span,
        }),
    }
}

/// Expand a macro expression with local bindings support.
/// This version also substitutes macro-local variables that were assigned in the macro body.
fn expand_macro_expr_with_locals<'a>(
    walker: &CstWalker<'a>,
    expr: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
    has_varargs: bool,
    local_bindings: &std::collections::HashMap<String, Expr>,
) -> LowerResult<Stmt> {
    // Handle different expression types
    match expr {
        // For QuoteLiteral, convert the quote constructor to executable code
        // and substitute local bindings
        Expr::QuoteLiteral {
            constructor,
            span: quote_span,
        } => {
            // Convert the quote constructor to executable IR code
            let executable = expr::quote_constructor_to_code_with_locals(
                constructor,
                params,
                args,
                *quote_span,
                walker,
                lambda_ctx,
                has_varargs,
                local_bindings,
            )?;
            Ok(Stmt::Expr {
                expr: executable,
                span,
            })
        }
        // For variable references, check if it's a parameter or local binding
        Expr::Var(name, _) => {
            // First check if this is a macro parameter
            if let Some(idx) = params.iter().position(|p| p == name) {
                if has_varargs && idx == params.len() - 1 {
                    let fixed_param_count = params.len() - 1;
                    if args.len() > fixed_param_count {
                        let varargs_exprs: Result<Vec<_>, _> = args[fixed_param_count..]
                            .iter()
                            .map(|arg| expr::lower_expr_with_ctx(walker, *arg, lambda_ctx))
                            .collect();
                        let varargs_exprs = varargs_exprs?;
                        let tuple_expr = Expr::TupleLiteral {
                            elements: varargs_exprs,
                            span,
                        };
                        Ok(Stmt::Expr {
                            expr: tuple_expr,
                            span,
                        })
                    } else {
                        Ok(Stmt::Expr {
                            expr: Expr::TupleLiteral {
                                elements: vec![],
                                span,
                            },
                            span,
                        })
                    }
                } else {
                    let arg_expr = expr::lower_expr_with_ctx(walker, args[idx], lambda_ctx)?;
                    Ok(Stmt::Expr {
                        expr: arg_expr,
                        span,
                    })
                }
            } else if let Some(bound_value) = local_bindings.get(name) {
                // This is a macro-local variable - substitute its value
                Ok(Stmt::Expr {
                    expr: bound_value.clone(),
                    span,
                })
            } else {
                // Not a parameter or local binding, return as-is
                Ok(Stmt::Expr {
                    expr: expr.clone(),
                    span,
                })
            }
        }
        // Fallback: return other expressions as-is
        _ => Ok(Stmt::Expr {
            expr: expr.clone(),
            span,
        }),
    }
}

/// Substitute macro parameters in an expression with actual arguments.
fn substitute_params_in_expr<'a>(
    expr: &Expr,
    params: &[String],
    args: &[Node<'a>],
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    match expr {
        Expr::Var(name, span) => {
            // Check if this is a macro parameter
            if let Some(idx) = params.iter().position(|p| p == name) {
                // Lower the corresponding argument
                expr::lower_expr_with_ctx(walker, args[idx], lambda_ctx)
            } else {
                // Not a parameter, return as-is
                Ok(Expr::Var(name.clone(), *span))
            }
        }
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => {
            let new_left = substitute_params_in_expr(left, params, args, walker, lambda_ctx)?;
            let new_right = substitute_params_in_expr(right, params, args, walker, lambda_ctx)?;
            Ok(Expr::BinaryOp {
                op: *op,
                left: Box::new(new_left),
                right: Box::new(new_right),
                span: *span,
            })
        }
        Expr::UnaryOp { op, operand, span } => {
            let new_operand = substitute_params_in_expr(operand, params, args, walker, lambda_ctx)?;
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
            // Special case: string(param) where param is a macro parameter
            // This should return the source text of the expression at macro expansion time
            if function == "string" && call_args.len() == 1 {
                if let Expr::Var(arg_name, _) = &call_args[0] {
                    if let Some(idx) = params.iter().position(|p| p == arg_name) {
                        // Get the source text of the argument
                        let source_text = walker.text(&args[idx]).to_string();
                        return Ok(Expr::Literal(Literal::Str(source_text), *span));
                    }
                }
            }
            // For other calls, substitute in arguments and kwargs but keep function name
            let new_args: Result<Vec<_>, _> = call_args
                .iter()
                .map(|a| substitute_params_in_expr(a, params, args, walker, lambda_ctx))
                .collect();
            let new_kwargs: Result<Vec<_>, _> = kwargs
                .iter()
                .map(|(k, v)| {
                    substitute_params_in_expr(v, params, args, walker, lambda_ctx)
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
            let new_args: Result<Vec<_>, _> = builtin_args
                .iter()
                .map(|a| substitute_params_in_expr(a, params, args, walker, lambda_ctx))
                .collect();
            Ok(Expr::Builtin {
                name: *name,
                args: new_args?,
                span: *span,
            })
        }
        Expr::QuoteLiteral { constructor, span } => {
            let new_constructor =
                substitute_params_in_expr(constructor, params, args, walker, lambda_ctx)?;
            Ok(Expr::QuoteLiteral {
                constructor: Box::new(new_constructor),
                span: *span,
            })
        }
        // Literals and other expressions don't need substitution
        _ => Ok(expr.clone()),
    }
}
