//! Macro statement lowering
//!
//! Note: Most macros (@show, @assert, @something, @time, @evalpoly, etc.) are now
//! implemented in Pure Julia (base/macros.jl). This module handles:
//! - User-defined macro expansion
//! - Base macro expansion (from base/macros.jl)
//! - Stdlib macro expansion (from stdlib modules like Test)
//!
//! The following macros have been migrated to Pure Julia:
//! - @show, @assert, @something → base/macros.jl
//! - @time, @elapsed, @timed → base/macros.jl
//! - @evalpoly, @sprintf, @printf → base/macros.jl
//! - @test, @testset, @test_throws → stdlib/Test/src/Test.jl

mod enum_impl;
mod expand;
mod static_eval;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal, Stmt};
use crate::lowering::expr;
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::stdlib_loader::has_stdlib_macro;
#[cfg(debug_assertions)]
use std::sync::OnceLock;

use enum_impl::lower_enum_macro_with_ctx;
use expand::{expand_base_macro, expand_stdlib_macro, expand_user_defined_macro};
use static_eval::lower_static_macro_with_ctx;

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

pub fn lower_macro<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    // Create an empty lambda context to support Base macros
    // This is used when lowering Base library itself where macros like @assert are used
    let lambda_ctx = LambdaContext::new();
    lower_macro_with_ctx(walker, node, &lambda_ctx)
}

// ==================== Lambda Context Versions ====================

pub fn lower_macro_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);

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

    // Find the macro argument list (tree-sitter) or collect direct children (Pure Rust parser)
    let args_node = walker.find_child(&node, NodeKind::MacroArgumentList);

    // For Pure Rust parser: if no MacroArgumentList, get arguments as direct children
    let direct_args: Vec<Node<'a>> = if args_node.is_none() {
        walker
            .named_children(&node)
            .into_iter()
            .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
            .collect()
    } else {
        vec![]
    };

    // Handle special compiler macros first
    match macro_name.as_str() {
        // @__FILE__ - returns the current file path as a string literal
        "__FILE__" => {
            let file_path = lambda_ctx.get_current_file();
            return Ok(Stmt::Expr {
                expr: Expr::Literal(Literal::Str(file_path), span),
                span,
            });
        }
        // @__DIR__ - returns the directory of the current file as a string literal
        "__DIR__" => {
            let dir_path = lambda_ctx.get_current_dir();
            return Ok(Stmt::Expr {
                expr: Expr::Literal(Literal::Str(dir_path), span),
                span,
            });
        }
        // @__LINE__ - returns the current line number as an integer literal
        "__LINE__" => {
            let line_number = span.start_line as i64;
            return Ok(Stmt::Expr {
                expr: Expr::Literal(Literal::Int(line_number), span),
                span,
            });
        }
        // @__MODULE__ - returns the current module as a module literal
        // In SubsetJuliaVM, we always return Main since module context isn't tracked at lowering
        "__MODULE__" => {
            return Ok(Stmt::Expr {
                expr: Expr::Literal(Literal::Module("Main".to_string()), span),
                span,
            });
        }
        // @label name - define a jump target for @goto
        // Usage: @label myloop
        "label" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children(&args_node)
            } else {
                direct_args.clone()
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@label requires exactly one identifier argument: @label name"),
                );
            }

            // The argument must be an identifier (symbol)
            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            if arg_kind != NodeKind::Identifier {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@label argument must be an identifier: @label name"),
                );
            }

            let label_name = walker.text(&arg).to_string();
            return Ok(Stmt::Label {
                name: label_name,
                span,
            });
        }
        // @goto name - unconditionally jump to @label name
        // Usage: @goto myloop
        "goto" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children(&args_node)
            } else {
                direct_args.clone()
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@goto requires exactly one identifier argument: @goto name"),
                );
            }

            // The argument must be an identifier (symbol)
            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            if arg_kind != NodeKind::Identifier {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@goto argument must be an identifier: @goto name"),
                );
            }

            let target_name = walker.text(&arg).to_string();
            return Ok(Stmt::Goto {
                name: target_name,
                span,
            });
        }
        // @simd - SIMD loop optimization hint (no-op in SubsetJuliaVM)
        // In SubsetJuliaVM, we don't have JIT or LLVM vectorization, so @simd simply
        // passes through the for loop body unchanged. This allows code that uses @simd
        // to run without modification.
        // Usage: @simd for i in 1:n ... end
        //        @simd ivdep for i in 1:n ... end
        "simd" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children(&args_node)
            } else {
                walker
                    .named_children(&node)
                    .into_iter()
                    .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
                    .collect()
            };

            if args.is_empty() {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@simd requires a for loop argument"),
                );
            }

            // Find the for loop - it's either the first or second argument
            // (second if first is "ivdep")
            let for_loop_node = if args.len() >= 2 {
                // Check if first arg is "ivdep" identifier
                let first_kind = walker.kind(&args[0]);
                if first_kind == NodeKind::Identifier && walker.text(&args[0]) == "ivdep" {
                    // @simd ivdep for ... - use second argument
                    args[1]
                } else {
                    // Use first argument
                    args[0]
                }
            } else {
                // Single argument - must be the for loop
                args[0]
            };

            // Check that it's a for loop
            let for_kind = walker.kind(&for_loop_node);
            if for_kind != NodeKind::ForStatement {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@simd requires a for loop as its argument"),
                );
            }

            // Lower the for loop normally - @simd is a no-op in SubsetJuliaVM
            return crate::lowering::stmt::lower_for_stmt_with_ctx(
                walker,
                for_loop_node,
                lambda_ctx,
            );
        }
        // @inbounds - bounds checking hint (no-op in SubsetJuliaVM)
        // In SubsetJuliaVM, bounds checking is handled at runtime, so @inbounds
        // simply passes through its argument unchanged.
        // Usage: @inbounds expr
        //        @inbounds for i in 1:n ... end
        "inbounds" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children(&args_node)
            } else {
                walker
                    .named_children(&node)
                    .into_iter()
                    .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
                    .collect()
            };

            if args.is_empty() {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@inbounds requires an argument"),
                );
            }

            // Lower the argument as-is
            let arg = args[0];
            let arg_kind = walker.kind(&arg);

            // Handle different statement types
            return match arg_kind {
                NodeKind::ForStatement => {
                    crate::lowering::stmt::lower_for_stmt_with_ctx(walker, arg, lambda_ctx)
                }
                NodeKind::WhileStatement => {
                    crate::lowering::stmt::lower_while_stmt_with_ctx(walker, arg, lambda_ctx)
                }
                NodeKind::IfStatement => {
                    crate::lowering::stmt::lower_if_stmt_with_ctx(walker, arg, lambda_ctx)
                }
                _ => {
                    // For expressions, lower them as an expression statement
                    let expr_result = expr::lower_expr_with_ctx(walker, arg, lambda_ctx)?;
                    Ok(Stmt::Expr {
                        expr: expr_result,
                        span,
                    })
                }
            };
        }
        // @view A[i:j] - create a view of an array slice
        // Transforms A[indices...] into view(A, indices...)
        "view" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children(&args_node)
            } else if !direct_args.is_empty() {
                direct_args.to_vec()
            } else {
                vec![]
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view requires exactly one argument: @view A[indices...]"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);

            // Check if the argument is an indexing expression (index_expression/ref)
            if arg_kind == NodeKind::IndexExpression {
                // Get the array and indices from the index expression
                // IndexExpression has children: [array, indices...]
                let sub_children: Vec<Node<'a>> = walker.named_children(&arg);
                if sub_children.is_empty() {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                            "@view requires an indexing expression: @view A[indices...]",
                        ),
                    );
                }

                // First child is the array
                let array_node = sub_children[0];
                let array_expr = expr::lower_expr_with_ctx(walker, array_node, lambda_ctx)?;

                // Remaining children are the indices
                let index_exprs: Vec<Expr> = sub_children[1..]
                    .iter()
                    .map(|idx| expr::lower_expr_with_ctx(walker, *idx, lambda_ctx))
                    .collect::<Result<Vec<_>, _>>()?;

                // Build the view function call: view(array, indices...)
                let mut call_args = vec![array_expr];
                call_args.extend(index_exprs);

                return Ok(Stmt::Expr {
                    expr: Expr::Call {
                        function: "view".to_string(),
                        args: call_args,
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span,
                    },
                    span,
                });
            } else {
                // Not an indexing expression - return error
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view requires an indexing expression: @view A[indices...]"),
                );
            }
        }
        // @static - compile-time conditional evaluation
        // Usage: @static if cond ... else ... end
        //        @static cond ? a : b
        // Evaluates the condition at compile time and only includes the selected branch.
        // Primarily used with Sys.isapple(), Sys.isunix(), Sys.iswindows(), Sys.islinux()
        "static" => {
            return lower_static_macro_with_ctx(
                walker,
                node,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            );
        }
        // @enum - define an enumerated type
        // Usage: @enum TypeName member1 member2 member3
        //        @enum TypeName member1=1 member2=2 member3=10
        //        @enum TypeName::BaseType member1 member2
        // Creates named constants backed by integers
        "enum" => {
            return lower_enum_macro_with_ctx(
                walker,
                node,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            );
        }
        // @isdefined(x) - check if variable x is defined
        // Returns a Builtin expression that will be compiled to IsDefined instruction
        "isdefined" => {
            // Get the argument (variable name)
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children(&args_node)
            } else if !direct_args.is_empty() {
                direct_args.to_vec()
            } else {
                vec![]
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@isdefined requires exactly one argument (a variable name)"),
                );
            }

            // Get the variable name from the argument
            // It should be an identifier (variable reference)
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

            // Return a Builtin expression that the compiler will convert to IsDefined instruction
            return Ok(Stmt::Expr {
                expr: Expr::Builtin {
                    name: crate::ir::core::BuiltinOp::IsDefined,
                    args: vec![Expr::Literal(Literal::Str(var_name), span)],
                    span,
                },
                span,
            });
        }
        _ => {}
    }

    // All macros are handled via the macro registry (user-defined, Base, or stdlib)
    // No more hardcoded macro handlers - everything goes through Pure Julia
    if lambda_ctx.has_macro(&macro_name) {
        // Check for user-defined macro (local context first)
        expand_user_defined_macro(
            walker,
            node,
            &macro_name,
            args_node,
            &direct_args,
            span,
            lambda_ctx,
        )
    } else if crate::base_loader::has_base_macro(&macro_name) {
        // Check for Base macro (from base/macros.jl)
        expand_base_macro(
            walker,
            node,
            &macro_name,
            args_node,
            &direct_args,
            span,
            lambda_ctx,
        )
    } else {
        // Check for stdlib macros from imported modules
        let mut found_macro = None;
        for module_name in lambda_ctx.get_usings() {
            if has_stdlib_macro(&module_name, &macro_name) {
                found_macro = Some(module_name);
                break;
            }
        }
        if let Some(module_name) = found_macro {
            expand_stdlib_macro(
                walker,
                node,
                &module_name,
                &macro_name,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            )
        } else if macro_name == "test"
            || macro_name == "testset"
            || macro_name == "test_throws"
            || macro_name == "test_broken"
        {
            // Special case: @test, @testset, @test_throws, @test_broken require using Test
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
