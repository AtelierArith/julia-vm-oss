//! Macro expansion logic for expression lowering.
//!
//! This module handles:
//! - User-defined macro expansion
//! - Base macro expansion
//! - `@macroexpand` macro
//! - Macro parameter substitution
//! - Nested macro expansion
//! - Macro hygiene (gensym counter, HygieneContext)

mod expand;
mod nested;
mod static_eval;
mod views;

use std::sync::atomic::AtomicU64;
#[cfg(debug_assertions)]
use std::sync::OnceLock;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

// Re-exports for quote submodules (accessed via super::super::macros::)
pub(super) static GENSYM_COUNTER: AtomicU64 = AtomicU64::new(0);
pub(super) use expand::substitute_params_in_macro_expr;
pub(super) use nested::expand_nested_macro_from_expr_args;

// Re-export for parent mod.rs
pub(super) use views::lower_expr_with_views;

// Internal re-exports
use expand::{expand_base_macro_expr, expand_user_defined_macro_expr, lower_macroexpand_expr};
use static_eval::lower_static_macro_expr;
use views::dotify_expr;

#[cfg(debug_assertions)]
fn macro_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("SJULIA_MACRO_DEBUG").is_ok())
}

#[cfg(debug_assertions)]
pub(super) fn macro_debug_log(args: std::fmt::Arguments<'_>) {
    if macro_debug_enabled() {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{args}");
    }
}

/// Lower a macro call in expression context.
/// This handles the Pure Rust parser format where arguments are direct children.
pub(crate) fn lower_macro_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);

    // Find the macro identifier
    let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier);
    let macro_name = match macro_ident {
        Some(ident) => {
            let text = walker.text(&ident);
            text.trim_start_matches('@').to_string()
        }
        None => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::MacroCall,
                span,
            ))
        }
    };

    // Get arguments (all children except MacroIdentifier)
    let args: Vec<Node<'a>> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
        .collect();

    // Handle special compiler macros
    match macro_name.as_str() {
        // @isdefined(x) - check if variable x is defined
        "isdefined" => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@isdefined requires exactly one argument (a variable name)"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            let var_name = match arg_kind {
                NodeKind::Identifier => walker.text(&arg).to_string(),
                _ => {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint("@isdefined argument must be a variable name (identifier)"),
                    );
                }
            };

            return Ok(Expr::Builtin {
                name: crate::ir::core::BuiltinOp::IsDefined,
                args: vec![Expr::Literal(Literal::Str(var_name), span)],
                span,
            });
        }
        // @__dot__ / @. - broadcast all operations (Issue #2547)
        "__dot__" | "." => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@. / @__dot__ requires exactly one argument"),
                );
            }
            let inner = super::lower_expr(walker, args[0])?;
            return Ok(dotify_expr(inner, span));
        }
        _ => {}
    }

    // Check for Base macro (from base/macros.jl)
    if crate::base_loader::has_base_macro(&macro_name) {
        let temp_ctx = crate::lowering::LambdaContext::new();
        return expand_base_macro_expr(walker, &macro_name, &args, span, &temp_ctx);
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::MacroCall,
        span,
    ))
}

/// Lower a macro call in expression context with lambda context.
pub(crate) fn lower_macro_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);

    // Find the macro identifier
    let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier);
    let macro_name = match macro_ident {
        Some(ident) => {
            let text = walker.text(&ident);
            text.trim_start_matches('@').to_string()
        }
        None => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::MacroCall,
                span,
            ))
        }
    };

    // Get arguments (all children except MacroIdentifier)
    let args: Vec<Node<'a>> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
        .collect();

    match macro_name.as_str() {
        "macroexpand" => lower_macroexpand_expr(walker, &args, span, lambda_ctx),
        // @__FILE__ - returns the current file path as a string literal
        "__FILE__" => {
            let file_path = lambda_ctx.get_current_file();
            Ok(Expr::Literal(Literal::Str(file_path), span))
        }
        // @__DIR__ - returns the directory of the current file as a string literal
        "__DIR__" => {
            let dir_path = lambda_ctx.get_current_dir();
            Ok(Expr::Literal(Literal::Str(dir_path), span))
        }
        // @__LINE__ - returns the current line number as an integer literal
        "__LINE__" => {
            let line_number = span.start_line as i64;
            Ok(Expr::Literal(Literal::Int(line_number), span))
        }
        // @__MODULE__ - returns the current module as a module literal
        "__MODULE__" => Ok(Expr::Literal(Literal::Module("Main".to_string()), span)),
        // @view A[i:j] - create a view of an array slice
        "view" => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view requires exactly one argument: @view A[indices...]"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);

            if arg_kind == NodeKind::IndexExpression {
                let sub_children: Vec<Node<'a>> = walker.named_children(&arg);

                if sub_children.is_empty() {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint("@view argument must be an indexing expression like A[i:j]"),
                    );
                }

                let array_node = sub_children[0];
                let index_nodes = &sub_children[1..];

                let array_expr =
                    super::lower_expr_with_ctx(walker, array_node, lambda_ctx)?;

                let mut call_args = vec![array_expr];
                for index_node in index_nodes {
                    let index_expr =
                        super::lower_expr_with_ctx(walker, *index_node, lambda_ctx)?;
                    call_args.push(index_expr);
                }

                Ok(Expr::Call {
                    function: "view".to_string(),
                    args: call_args,
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                })
            } else {
                Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view argument must be an indexing expression like A[i:j]"),
                )
            }
        }
        // @views expression - convert all array slicing to views within expression
        "views" => {
            if args.is_empty() {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@views requires an expression argument"),
                );
            }

            let arg = args[0];
            lower_expr_with_views(walker, arg, lambda_ctx)
        }
        // @isdefined(x) - check if variable x is defined
        "isdefined" => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@isdefined requires exactly one argument (a variable name)"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            let var_name = match arg_kind {
                NodeKind::Identifier => walker.text(&arg).to_string(),
                _ => {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint("@isdefined argument must be a variable name (identifier)"),
                    );
                }
            };

            Ok(Expr::Builtin {
                name: crate::ir::core::BuiltinOp::IsDefined,
                args: vec![Expr::Literal(Literal::Str(var_name), span)],
                span,
            })
        }
        // @__dot__ / @. - broadcast all operations (Issue #2547)
        "__dot__" | "." => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@. / @__dot__ requires exactly one argument"),
                );
            }
            let inner = super::lower_expr_with_ctx(walker, args[0], lambda_ctx)?;
            Ok(dotify_expr(inner, span))
        }
        // @static - compile-time conditional evaluation
        "static" => lower_static_macro_expr(walker, &args, span, lambda_ctx),
        _ => {
            // Check for user-defined macro (local context first)
            if lambda_ctx.has_macro(&macro_name) {
                expand_user_defined_macro_expr(walker, &macro_name, &args, span, lambda_ctx)
            } else if crate::base_loader::has_base_macro(&macro_name) {
                expand_base_macro_expr(walker, &macro_name, &args, span, lambda_ctx)
            } else if macro_name == "test"
                || macro_name == "testset"
                || macro_name == "test_throws"
                || macro_name == "test_broken"
            {
                Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint(format!("@{} macro requires `using Test`", macro_name)),
                )
            } else {
                Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint(format!("unknown macro @{}", macro_name)),
                )
            }
        }
    }
}
