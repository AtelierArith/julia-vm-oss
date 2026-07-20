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
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;
use std::sync::atomic::{AtomicU64, Ordering};

static MACRO_LOCAL_GENSYM_COUNTER: AtomicU64 = AtomicU64::new(0);

fn macro_local_gensym(base: &str) -> String {
    let counter = MACRO_LOCAL_GENSYM_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("##{}#{}", base, counter)
}

fn base_macro_preserves_statement_value(macro_name: &str) -> bool {
    // Runtime-expanded value macros must preserve their top-level statement
    // result; lowering them as bare blocks drops the tail value (Issue #7764).
    matches!(
        macro_name,
        "show"
            | "time"
            | "elapsed"
            | "timed"
            | "timev"
            | "showtime"
            | "allocated"
            | "allocations"
            | "something"
            | "coalesce"
            | "evalpoly"
            | "sprintf"
            | "printf"
    )
}

/// Expand a macro defined in Base (base/macros.jl)
pub(super) fn expand_base_macro<'a>(
    walker: &CstWalker<'a>,
    _node: Node<'a>,
    macro_name: &str,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Get arguments first to determine arity
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children_vec(&args_node)
    } else if !direct_args.is_empty() {
        direct_args.to_vec()
    } else {
        vec![]
    };

    // Multi-argument @show (Issue #4868): the Pure-Julia macro-substitution
    // engine cannot iterate over a varargs parameter to emit one statement per
    // argument, so `@show x y z` is expanded here in Rust. The single-argument
    // case stays on the Pure-Julia `macro show(ex)` path in base/macros.jl.
    if macro_name == "show" && args.len() != 1 {
        let block = build_multiarg_show_block(walker, &args, span, lambda_ctx)?;
        return Ok(Stmt::Block(block));
    }

    // Get the macro definition from Base registry with arity-based dispatch
    let macro_def =
        crate::lowering::macros_registry::get_base_macro_with_arity(macro_name, args.len())
            .ok_or_else(|| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                    "Base macro @{} not found (with {} args)",
                    macro_name,
                    args.len()
                ))
            })?;

    if base_macro_preserves_statement_value(macro_name) {
        let expr = crate::lowering::macro_expander::expand_macro_to_expr(
            walker, macro_name, &macro_def, &args, span, lambda_ctx,
        )?;
        return Ok(Stmt::Expr { expr, span });
    }

    crate::lowering::macro_expander::expand_macro_to_stmt(
        walker, macro_name, &macro_def, &args, span, lambda_ctx,
    )
}

/// Expand a macro defined in a stdlib module (e.g., Test::test)
///
/// `pub` (not `pub(super)`): the expression-position stdlib-macro
/// expander (`lowering::expr::macros::expand::expand_stdlib_macro_expr`)
/// reuses this exact function so a stdlib macro like `Test.@test` expands
/// identically whether it appears as its own statement or in value position
/// (assignment RHS, operand, argument) -- Issue #10293 / #10307.
pub fn expand_stdlib_macro<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    module_name: &str,
    macro_name: &str,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children_vec(&args_node)
    } else if !direct_args.is_empty() {
        direct_args.to_vec()
    } else {
        vec![]
    };

    if module_name == "Test" && macro_name == "testset" {
        if let Some(stmt) = lower_testset_for_macro(walker, &args, span, lambda_ctx)? {
            return Ok(stmt);
        }
    }

    // Get the macro definition from stdlib registry
    let macro_def = crate::lowering::macros_registry::get_stdlib_macro(module_name, macro_name)
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "stdlib macro @{} not found in module {}",
                macro_name, module_name
            ))
        })?;

    // Delegate to the common expansion logic
    let stmt = expand_macro_with_def(
        walker,
        node,
        macro_name,
        args_node,
        direct_args,
        span,
        lambda_ctx,
        &macro_def,
    )?;
    Ok(if module_name == "Test" {
        discard_macro_tail_value(stmt, span)
    } else {
        stmt
    })
}

/// Statement-position adapter for expanded stdlib `Test` macro quotes
/// (Issue #10630 contract; root cause of #10307/#10496, fixed in PR #10625).
///
/// A stdlib macro quote lowers through nested `Block` and `LetBlock` wrappers
/// whose FINAL statement is the expansion's result value (e.g. the recorded
/// `Test.Result`), while every EARLIER statement is a required effect (the
/// recorder control-flow subtree). In statement position the result value is
/// unused, so this adapter walks the wrapper spine to the innermost tail and
/// replaces ONLY that tail expression with `nothing`:
///
/// - Deleting the outer final statement instead would drop the whole recorder
///   subtree (the failure/error would never be recorded — tail-effect loss).
/// - Keeping the tail would construct a result object whose store can clobber
///   an unrelated caller slot in statement position (#10496).
///
/// Expression position must NOT use this adapter — it keeps the
/// value-preserving path so `r = @test ...` receives the `Test.Result`
/// (Issue #10293/#10307). Pinned by the unit tests below, the fixture
/// `macros/test_macro_stmt_expr_value_matrix_10630.jl`, and the
/// `*_matrix_10630` cases in `tests/testset_exit_code_8191_tests.rs`; the
/// contract is documented in docs/vm/LOWERING.md.
fn discard_macro_tail_value(stmt: Stmt, span: Span) -> Stmt {
    match stmt {
        Stmt::Block(mut block) => {
            if let Some(last) = block.stmts.last_mut() {
                let placeholder = Stmt::Expr {
                    expr: Expr::Literal(Literal::Nothing, span),
                    span,
                };
                let tail = std::mem::replace(last, placeholder);
                *last = discard_macro_tail_value(tail, span);
            }
            Stmt::Block(block)
        }
        Stmt::Expr {
            expr:
                Expr::LetBlock {
                    bindings,
                    mut body,
                    span: let_span,
                },
            span: stmt_span,
        } => {
            if let Some(last) = body.stmts.last_mut() {
                let placeholder = Stmt::Expr {
                    expr: Expr::Literal(Literal::Nothing, span),
                    span,
                };
                let tail = std::mem::replace(last, placeholder);
                *last = discard_macro_tail_value(tail, span);
            }
            Stmt::Expr {
                expr: Expr::LetBlock {
                    bindings,
                    body,
                    span: let_span,
                },
                span: stmt_span,
            }
        }
        Stmt::Expr { .. } => Stmt::Expr {
            expr: Expr::Literal(Literal::Nothing, span),
            span,
        },
        other => other,
    }
}

fn lower_testset_for_macro<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<Stmt>> {
    if args.len() != 2 || walker.kind(&args[1]) != NodeKind::ForStatement {
        return Ok(None);
    }

    let name = expr::lower_expr_with_ctx(walker, args[0], lambda_ctx)?;
    let for_span = walker.span(&args[1]);
    let for_stmt = crate::lowering::stmt::lower_for_stmt_with_ctx(walker, args[1], lambda_ctx)?;

    let begin = Stmt::Expr {
        expr: Expr::Call {
            function: "_testset_begin!".to_string().into(),
            args: vec![name],
            kwargs: Vec::new(),
            splat_mask: vec![false],
            kwargs_splat_mask: Vec::new(),
            span,
        },
        span,
    };
    let body = Stmt::Expr {
        expr: Expr::LetBlock {
            bindings: Vec::new(),
            body: Block {
                stmts: vec![for_stmt],
                span: for_span,
            },
            span: for_span,
        },
        span: for_span,
    };
    let end = Stmt::Expr {
        expr: Expr::Call {
            function: "_testset_end!".to_string().into(),
            args: Vec::new(),
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        },
        span,
    };

    Ok(Some(Stmt::Block(Block {
        stmts: vec![begin, body, end],
        span,
    })))
}

/// Expand a bundled-package macro (e.g. Plots' `@animate`/`@gif`) in statement
/// context.
///
/// Like the expression-context variant, bundled-package macros go through the full
/// `macro_runtime` expander (the same one user-defined macros use), not the
/// template path used for Base/stdlib macros — so they may construct AST nodes at
/// expansion time. Issue #6355.
///
/// We expand to an *expression* and wrap it in `Stmt::Expr`, rather than calling
/// `expand_macro_to_stmt`: the latter yields a `begin … end` block, whose value is
/// not propagated as the program result at top level (a bare trailing block does
/// not display). `@gif for … end` written as a standalone statement (the issue's
/// MVP) must still yield its `AnimatedGif` so the REPL/iOS/Web auto-display path
/// renders it — exactly like a trailing `plot(x, y)` call does.
pub(super) fn expand_bundled_package_macro<'a>(
    walker: &CstWalker<'a>,
    module_name: &str,
    macro_name: &str,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children_vec(&args_node)
    } else if !direct_args.is_empty() {
        direct_args.to_vec()
    } else {
        vec![]
    };

    let macro_def =
        crate::lowering::macros_registry::get_bundled_package_macro(module_name, macro_name)
            .ok_or_else(|| {
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                    "bundled-package macro @{} not found in {}",
                    macro_name, module_name
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

    crate::lowering::macros_registry::add_bundled_package_macro_context(module_name, lambda_ctx);
    if module_name == "MacroTools" && macro_name == "capture" {
        return crate::lowering::macro_expander::expand_macro_to_stmt(
            walker, macro_name, &macro_def, &args, span, lambda_ctx,
        );
    }
    let expr = crate::lowering::macro_expander::expand_macro_to_expr(
        walker, macro_name, &macro_def, &args, span, lambda_ctx,
    )?;
    Ok(Stmt::Expr { expr, span })
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
        walker.named_children_vec(&args_node)
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
        walker.named_children_vec(&args_node)
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

    crate::lowering::macro_expander::expand_macro_to_stmt(
        walker, macro_name, &macro_def, &args, span, lambda_ctx,
    )
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
            } else if let Some(bound_value) = local_bindings.get(name.as_str()) {
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
                Ok(Expr::Var(*name, *span))
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
            if function == "gensym" && call_args.len() <= 1 && kwargs.is_empty() {
                let base = match call_args.first() {
                    Some(Expr::Literal(Literal::Str(base), _)) => base.as_str(),
                    Some(Expr::Var(base, _)) => base.as_str(),
                    Some(_) => "gensym",
                    None => "gensym",
                };
                let symbol = macro_local_gensym(base);
                return Ok(Expr::Builtin {
                    name: crate::ir::core::BuiltinOp::SymbolNew,
                    args: vec![Expr::Literal(Literal::Str(symbol), *span)],
                    span: *span,
                });
            }
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
                        .map(|nv| (*k, nv))
                })
                .collect();
            Ok(Expr::Call {
                function: *function,
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

/// Build the expanded block for a multi-argument `@show a b c ...` (Issue #4868).
///
/// Mirrors upstream Julia's `macro show(exs...)` (julia/base/show.jl): emit one
/// `_do_show("<source text>", <arg>)` call per argument and let the block
/// evaluate to the value of the last call (so `(@show a b c) == c`).
///
/// `_do_show` is the same Pure-Julia helper used by the single-argument
/// `macro show(ex)` in base/macros.jl, so the printed form stays identical.
/// Each argument is lowered with `lower_expr_with_ctx`, which evaluates it in
/// the macro call site's scope — the Rust equivalent of `esc(ex)`.
pub fn build_multiarg_show_block<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Block> {
    let mut stmts = Vec::with_capacity(args.len().max(1));
    for arg in args {
        let expr_str = walker.text(arg).to_string();
        let value_expr = crate::lowering::expr::lower_expr_with_ctx(walker, *arg, lambda_ctx)?;
        let call = Expr::Call {
            function: "_do_show".to_string().into(),
            args: vec![Expr::Literal(Literal::Str(expr_str), span), value_expr],
            kwargs: vec![],
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span,
        };
        stmts.push(Stmt::Expr { expr: call, span });
    }
    // `@show` with zero arguments is valid in upstream Julia and evaluates to
    // `nothing`; keep the block non-empty so it has a well-defined value.
    if stmts.is_empty() {
        stmts.push(Stmt::Expr {
            expr: Expr::Literal(Literal::Nothing, span),
            span,
        });
    }
    Ok(Block { stmts, span })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    //! Unit pins for the statement-position adapter contract (Issue #10630):
    //! `discard_macro_tail_value` must retain every effect statement through
    //! nested `Block`/`LetBlock` wrappers and replace ONLY the innermost
    //! result tail with `nothing`.

    use super::*;

    fn sp() -> Span {
        Span::new(0, 0, 1, 1, 0, 0)
    }

    /// An effect statement (a recorder-style call) that must survive the
    /// adapter untouched.
    fn effect_call(name: &str) -> Stmt {
        Stmt::Expr {
            expr: Expr::Call {
                function: name.to_string().into(),
                args: vec![],
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span: sp(),
            },
            span: sp(),
        }
    }

    /// A result-tail statement (a bare variable read of the recorded value).
    fn value_tail(name: &str) -> Stmt {
        Stmt::Expr {
            expr: Expr::Var(name.to_string().into(), sp()),
            span: sp(),
        }
    }

    fn is_effect_call(stmt: &Stmt, name: &str) -> bool {
        matches!(
            stmt,
            Stmt::Expr {
                expr: Expr::Call { function, .. },
                ..
            } if function.as_ref() == name
        )
    }

    fn is_discarded_tail(stmt: &Stmt) -> bool {
        matches!(
            stmt,
            Stmt::Expr {
                expr: Expr::Literal(Literal::Nothing, _),
                ..
            }
        )
    }

    #[test]
    fn block_tail_removed_but_effects_retained_10630() {
        let stmt = Stmt::Block(Block {
            stmts: vec![effect_call("record!"), value_tail("__result")],
            span: sp(),
        });
        let adapted = discard_macro_tail_value(stmt, sp());
        let Stmt::Block(block) = adapted else {
            panic!("Block wrapper must be preserved, got {adapted:?}");
        };
        assert_eq!(block.stmts.len(), 2, "no statement may be deleted");
        assert!(
            is_effect_call(&block.stmts[0], "record!"),
            "the recorder effect statement must survive: {:?}",
            block.stmts[0]
        );
        assert!(
            is_discarded_tail(&block.stmts[1]),
            "only the result tail is replaced with nothing: {:?}",
            block.stmts[1]
        );
    }

    #[test]
    fn nested_letblock_tail_removed_but_effects_retained_10630() {
        // Block whose final statement is a LetBlock expression — the shape a
        // stdlib Test macro quote lowers to. The adapter must recurse through
        // the wrapper spine: the LetBlock (and its effect statements) stay,
        // only the LetBlock's own innermost tail is discarded.
        let inner = Stmt::Expr {
            expr: Expr::LetBlock {
                bindings: vec![],
                body: Block {
                    stmts: vec![effect_call("record_inner!"), value_tail("__inner_result")],
                    span: sp(),
                },
                span: sp(),
            },
            span: sp(),
        };
        let stmt = Stmt::Block(Block {
            stmts: vec![effect_call("record_outer!"), inner],
            span: sp(),
        });

        let adapted = discard_macro_tail_value(stmt, sp());
        let Stmt::Block(block) = adapted else {
            panic!("outer Block wrapper must be preserved, got {adapted:?}");
        };
        assert_eq!(block.stmts.len(), 2, "no outer statement may be deleted");
        assert!(
            is_effect_call(&block.stmts[0], "record_outer!"),
            "the outer effect must survive: {:?}",
            block.stmts[0]
        );
        let Stmt::Expr {
            expr: Expr::LetBlock { body, .. },
            ..
        } = &block.stmts[1]
        else {
            panic!(
                "the LetBlock wrapper (recorder subtree) must NOT be deleted \
                 as a disposable tail: {:?}",
                block.stmts[1]
            );
        };
        assert_eq!(body.stmts.len(), 2, "no LetBlock statement may be deleted");
        assert!(
            is_effect_call(&body.stmts[0], "record_inner!"),
            "the nested effect must survive: {:?}",
            body.stmts[0]
        );
        assert!(
            is_discarded_tail(&body.stmts[1]),
            "only the innermost result tail is replaced: {:?}",
            body.stmts[1]
        );
    }

    #[test]
    fn bare_expr_tail_becomes_nothing_10630() {
        let adapted = discard_macro_tail_value(value_tail("__result"), sp());
        assert!(
            is_discarded_tail(&adapted),
            "an unwrapped value expression in statement position is discarded: {adapted:?}"
        );
    }

    #[test]
    fn non_value_stmt_is_untouched_10630() {
        // A statement that is not a value tail (e.g. an assignment) must pass
        // through unchanged: replacing it would lose its effect.
        let stmt = Stmt::Assign {
            var: "x".to_string(),
            value: Expr::Literal(Literal::Int(1), sp()),
            span: sp(),
        };
        let adapted = discard_macro_tail_value(stmt, sp());
        assert!(
            matches!(&adapted, Stmt::Assign { var, .. } if var == "x"),
            "non-value statements pass through unchanged: {adapted:?}"
        );
    }
}
