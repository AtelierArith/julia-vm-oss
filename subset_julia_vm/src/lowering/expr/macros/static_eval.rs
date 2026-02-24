//! @static compile-time conditional evaluation.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use super::super::lower_expr_with_ctx;

/// Lower @static macro in expression context.
pub(super) fn lower_static_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static requires an if statement or ternary expression"),
        );
    }

    let arg = args[0];
    let arg_kind = walker.kind(&arg);

    match arg_kind {
        NodeKind::TernaryExpression => lower_static_ternary_expr(walker, arg, span, lambda_ctx),
        NodeKind::IfStatement => lower_static_if_expr(walker, arg, span, lambda_ctx),
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static requires an if statement or ternary expression"),
        ),
    }
}

fn lower_static_ternary_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children: Vec<Node<'a>> = walker.named_children(&node);
    if children.len() < 3 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static ternary: malformed ternary expression"),
        );
    }

    match try_eval_compile_time_bool_expr(walker, children[0]) {
        Some(true) => lower_expr_with_ctx(walker, children[1], lambda_ctx),
        Some(false) => lower_expr_with_ctx(walker, children[2], lambda_ctx),
        None => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@static condition must be evaluable at compile time (e.g., true, false, Sys.isapple())",
            ),
        ),
    }
}

fn lower_static_if_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children: Vec<Node<'a>> = walker.named_children(&node);
    if children.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@static if: missing condition"),
        );
    }

    match try_eval_compile_time_bool_expr(walker, children[0]) {
        Some(true) => {
            if children.len() >= 2 { lower_expr_with_ctx(walker, children[1], lambda_ctx) }
            else { Ok(Expr::Literal(Literal::Nothing, span)) }
        }
        Some(false) => {
            if children.len() >= 3 {
                let third_kind = walker.kind(&children[2]);
                if third_kind == NodeKind::ElseClause {
                    let else_children: Vec<Node<'a>> = walker.named_children(&children[2]);
                    if !else_children.is_empty() {
                        if walker.kind(&else_children[0]) == NodeKind::IfStatement {
                            return lower_static_if_expr(walker, else_children[0], span, lambda_ctx);
                        }
                        return lower_expr_with_ctx(walker, else_children[0], lambda_ctx);
                    }
                }
                lower_expr_with_ctx(walker, children[2], lambda_ctx)
            } else {
                Ok(Expr::Literal(Literal::Nothing, span))
            }
        }
        None => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@static condition must be evaluable at compile time (e.g., true, false, Sys.isapple())",
            ),
        ),
    }
}

fn try_eval_compile_time_bool_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<bool> {
    let kind = walker.kind(&node);
    match kind {
        NodeKind::Identifier | NodeKind::BooleanLiteral => {
            match walker.text(&node) { "true" => Some(true), "false" => Some(false), _ => None }
        }
        NodeKind::CallExpression => {
            let children: Vec<Node<'a>> = walker.named_children(&node);
            if children.is_empty() { return None; }
            match walker.text(&children[0]) {
                "Sys.isapple" => Some(true), "Sys.isunix" => Some(true),
                "Sys.iswindows" => Some(false), "Sys.islinux" => Some(false),
                "Sys.isbsd" => Some(true), "Sys.isfreebsd" => Some(false),
                "Sys.isnetbsd" => Some(false), "Sys.isopenbsd" => Some(false),
                "Sys.isdragonfly" => Some(false), _ => None,
            }
        }
        NodeKind::FieldExpression => {
            match walker.text(&node) {
                "Sys.isapple" => Some(true), "Sys.isunix" => Some(true),
                "Sys.iswindows" => Some(false), "Sys.islinux" => Some(false),
                "Sys.isbsd" => Some(true), _ => None,
            }
        }
        _ => None,
    }
}
