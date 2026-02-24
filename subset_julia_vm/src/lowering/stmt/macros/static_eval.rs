//! @static compile-time conditional evaluation for statement context.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal, Stmt};
use crate::lowering::expr;
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

// ==================== @static Macro Implementation ====================

/// Lower @static macro - compile-time conditional evaluation.
///
/// Usage:
/// - `@static if condition then_branch else else_branch end`
/// - `@static condition ? then_expr : else_expr`
///
/// The condition is evaluated at compile time, and only the selected branch
/// is included in the output. This is primarily used for platform-specific code:
/// - `Sys.isapple()` - true for Apple platforms (macOS, iOS)
/// - `Sys.isunix()` - true for Unix-like systems
/// - `Sys.iswindows()` - true for Windows
/// - `Sys.islinux()` - true for Linux
pub(super) fn lower_static_macro_with_ctx<'a>(
    walker: &CstWalker<'a>,
    _node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Get the macro argument (should be an if statement or ternary expression)
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children(&args_node)
    } else {
        direct_args.to_vec()
    };

    if args.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static requires an if statement or ternary expression"),
        );
    }

    let arg = args[0];
    let arg_kind = walker.kind(&arg);

    match arg_kind {
        // @static if cond ... else ... end
        NodeKind::IfStatement => lower_static_if(walker, arg, span, lambda_ctx),
        // @static cond ? a : b
        NodeKind::TernaryExpression => lower_static_ternary(walker, arg, span, lambda_ctx),
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static requires an if statement or ternary expression"),
        ),
    }
}

/// Lower a static if statement.
fn lower_static_if<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Get condition from the if statement
    let children: Vec<Node<'a>> = walker.named_children(&node);

    // if_statement has: condition, then_body, (optional) else_body
    if children.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static if: missing condition"),
        );
    }

    let condition_node = children[0];
    let cond_result = try_eval_compile_time_bool(walker, condition_node);

    match cond_result {
        Some(true) => {
            // Condition is true at compile time - include then branch
            if children.len() >= 2 {
                crate::lowering::stmt::lower_stmt_with_ctx(walker, children[1], lambda_ctx)
            } else {
                Ok(Stmt::Expr {
                    expr: Expr::Literal(Literal::Nothing, span),
                    span,
                })
            }
        }
        Some(false) => {
            // Condition is false at compile time - include else branch (if any)
            if children.len() >= 3 {
                // Check for else clause
                let third_kind = walker.kind(&children[2]);
                if third_kind == NodeKind::ElseClause {
                    // Get the body from else clause
                    let else_children: Vec<Node<'a>> = walker.named_children(&children[2]);
                    if !else_children.is_empty() {
                        // Check if the else clause contains a nested if (elseif)
                        let else_child_kind = walker.kind(&else_children[0]);
                        if else_child_kind == NodeKind::IfStatement {
                            // Nested if statement - handle as elseif
                            return lower_static_if(walker, else_children[0], span, lambda_ctx);
                        }
                        return crate::lowering::stmt::lower_stmt_with_ctx(
                            walker,
                            else_children[0],
                            lambda_ctx,
                        );
                    }
                }
                crate::lowering::stmt::lower_stmt_with_ctx(walker, children[2], lambda_ctx)
            } else {
                // No else branch - return nothing
                Ok(Stmt::Expr {
                    expr: Expr::Literal(Literal::Nothing, span),
                    span,
                })
            }
        }
        None => {
            // Cannot evaluate at compile time - return error
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    "@static condition must be evaluable at compile time (e.g., true, false, Sys.isapple())",
                ),
            )
        }
    }
}

// Note: elseif handling is done via nested IfStatement nodes within ElseClause.
// No separate lower_static_elseif function is needed since lower_static_if handles this recursively.

/// Lower a static ternary expression.
fn lower_static_ternary<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Ternary: cond ? then_expr : else_expr
    let children: Vec<Node<'a>> = walker.named_children(&node);

    if children.len() < 3 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static ternary: malformed ternary expression"),
        );
    }

    let condition_node = children[0];
    let cond_result = try_eval_compile_time_bool(walker, condition_node);

    match cond_result {
        Some(true) => {
            // Return then expression
            let then_expr = expr::lower_expr_with_ctx(walker, children[1], lambda_ctx)?;
            Ok(Stmt::Expr {
                expr: then_expr,
                span,
            })
        }
        Some(false) => {
            // Return else expression
            let else_expr = expr::lower_expr_with_ctx(walker, children[2], lambda_ctx)?;
            Ok(Stmt::Expr {
                expr: else_expr,
                span,
            })
        }
        None => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@static condition must be evaluable at compile time (e.g., true, false, Sys.isapple())",
            ),
        ),
    }
}

/// Try to evaluate a condition at compile time.
///
/// Returns `Some(true)` or `Some(false)` if the condition can be evaluated,
/// or `None` if it cannot be determined at compile time.
///
/// Supported conditions:
/// - `true` / `false` literals
/// - `Sys.isapple()` - true (targeting iOS/macOS)
/// - `Sys.isunix()` - true (iOS is Unix-like)
/// - `Sys.iswindows()` - false
/// - `Sys.islinux()` - false
/// - `Sys.isbsd()` - true (iOS is BSD-based)
fn try_eval_compile_time_bool<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<bool> {
    let kind = walker.kind(&node);

    match kind {
        // true/false literals
        NodeKind::Identifier | NodeKind::BooleanLiteral => {
            let text = walker.text(&node);
            match text {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            }
        }
        // Call expressions like Sys.isapple()
        NodeKind::CallExpression => {
            // Get the function being called
            let children: Vec<Node<'a>> = walker.named_children(&node);
            if children.is_empty() {
                return None;
            }

            let func_node = children[0];
            let func_text = walker.text(&func_node);

            // Check for Sys.* functions (fully qualified)
            match func_text {
                "Sys.isapple" => Some(true), // iOS is Apple
                "Sys.isunix" => Some(true),  // iOS is Unix-like
                "Sys.iswindows" => Some(false),
                "Sys.islinux" => Some(false),
                "Sys.isbsd" => Some(true), // iOS is BSD-based
                "Sys.isfreebsd" => Some(false),
                "Sys.isnetbsd" => Some(false),
                "Sys.isopenbsd" => Some(false),
                "Sys.isdragonfly" => Some(false),
                _ => None,
            }
        }
        // Field expression like Sys.isapple() where Sys.isapple is the field access
        NodeKind::FieldExpression => {
            let text = walker.text(&node);
            match text {
                "Sys.isapple" => Some(true),
                "Sys.isunix" => Some(true),
                "Sys.iswindows" => Some(false),
                "Sys.islinux" => Some(false),
                "Sys.isbsd" => Some(true),
                _ => None,
            }
        }
        _ => None,
    }
}
