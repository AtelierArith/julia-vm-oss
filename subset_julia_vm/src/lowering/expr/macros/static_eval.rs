//! @static compile-time conditional evaluation.

use super::super::lower_expr_with_ctx;
use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

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
        NodeKind::Identifier | NodeKind::BooleanLiteral => match walker.text(&node) {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        NodeKind::CallExpression => {
            let children: Vec<Node<'a>> = walker.named_children(&node);
            if children.is_empty() {
                return None;
            }
            match walker.text(&children[0]) {
                "Sys.isapple" => Some(true),
                "Sys.isunix" => Some(true),
                "Sys.iswindows" => Some(false),
                "Sys.islinux" => Some(false),
                "Sys.isbsd" => Some(true),
                "Sys.isfreebsd" => Some(false),
                "Sys.isnetbsd" => Some(false),
                "Sys.isopenbsd" => Some(false),
                "Sys.isdragonfly" => Some(false),
                _ => None,
            }
        }
        NodeKind::FieldExpression => match walker.text(&node) {
            "Sys.isapple" => Some(true),
            "Sys.isunix" => Some(true),
            "Sys.iswindows" => Some(false),
            "Sys.islinux" => Some(false),
            "Sys.isbsd" => Some(true),
            _ => None,
        },
        NodeKind::BinaryExpression => try_eval_version_comparison_expr(walker, node),
        _ => None,
    }
}

fn try_eval_version_comparison_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<bool> {
    let children = walker.named_children(&node);
    let operands: Vec<Node<'a>> = children
        .iter()
        .copied()
        .filter(|child| walker.kind(child) != NodeKind::Operator)
        .collect();
    if operands.len() != 2 {
        return None;
    }
    let op = children
        .iter()
        .find(|child| walker.kind(child) == NodeKind::Operator)
        .map(|child| walker.text(child))?;

    let lhs = compile_time_version_value_expr(walker, operands[0])?;
    let rhs = compile_time_version_value_expr(walker, operands[1])?;
    Some(match op {
        "==" => lhs == rhs,
        "!=" => lhs != rhs,
        "<" => lhs < rhs,
        "<=" => lhs <= rhs,
        ">" => lhs > rhs,
        ">=" => lhs >= rhs,
        _ => return None,
    })
}

fn compile_time_version_value_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(u64, u64, u64)> {
    match walker.kind(&node) {
        NodeKind::Identifier if walker.text(&node) == "VERSION" => {
            parse_version_tuple_expr(env!("CARGO_PKG_VERSION"))
        }
        _ if node.kind() == "prefixed_string_literal" => {
            let children = walker.named_children(&node);
            if children.len() < 2 || walker.text(&children[0]) != "v" {
                return None;
            }
            parse_version_tuple_expr(walker.text(&children[1]).trim_matches('"'))
        }
        _ => None,
    }
}

fn parse_version_tuple_expr(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next().unwrap_or(text);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}
