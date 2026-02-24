//! @views and @. (broadcast) macro support.

use crate::ir::core::{Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use super::super::lower_expr_with_ctx;

/// Recursively transform an expression to broadcast all operations (Issue #2547).
pub(super) fn dotify_expr(expr: Expr, span: crate::span::Span) -> Expr {
    use super::super::make_broadcasted_call;
    use crate::ir::core::BinaryOp;

    match expr {
        Expr::Call {
            function, args, kwargs, splat_mask, kwargs_splat_mask, span: call_span,
        } => {
            if function == "materialize" || function == "Broadcasted" {
                return Expr::Call { function, args, kwargs, splat_mask, kwargs_splat_mask, span: call_span };
            }
            let dotified_args: Vec<Expr> = args.into_iter().map(|a| dotify_expr(a, span)).collect();
            make_broadcasted_call(&function, dotified_args, call_span)
        }
        Expr::BinaryOp { op, left, right, span: op_span } => {
            let dotified_left = dotify_expr(*left, span);
            let dotified_right = dotify_expr(*right, span);
            let op_name = match op {
                BinaryOp::Add => "+", BinaryOp::Sub => "-", BinaryOp::Mul => "*",
                BinaryOp::Div => "/", BinaryOp::Mod => "%", BinaryOp::Pow => "^",
                BinaryOp::Lt => "<", BinaryOp::Gt => ">", BinaryOp::Le => "<=",
                BinaryOp::Ge => ">=", BinaryOp::Eq => "==", BinaryOp::Ne => "!=",
                BinaryOp::And => "andand", BinaryOp::Or => "oror",
                _ => {
                    return Expr::BinaryOp { op, left: Box::new(dotified_left), right: Box::new(dotified_right), span: op_span };
                }
            };
            make_broadcasted_call(op_name, vec![dotified_left, dotified_right], op_span)
        }
        Expr::AssignExpr { var, value, span: assign_span } => {
            let dotified_value = dotify_expr(*value, span);
            Expr::AssignExpr { var, value: Box::new(dotified_value), span: assign_span }
        }
        Expr::LetBlock { bindings, body, span: block_span } => {
            let dotified_stmts: Vec<crate::ir::core::Stmt> = body.stmts.into_iter().map(|stmt| match stmt {
                crate::ir::core::Stmt::Expr { expr, span: s } => crate::ir::core::Stmt::Expr { expr: dotify_expr(expr, span), span: s },
                other => other,
            }).collect();
            Expr::LetBlock { bindings, body: crate::ir::core::Block { stmts: dotified_stmts, span: body.span }, span: block_span }
        }
        Expr::Builtin { name, args: builtin_args, span: builtin_span } => {
            use crate::ir::core::BuiltinOp;
            let fn_name = match name { BuiltinOp::Sqrt => Some("sqrt"), _ => None };
            if let Some(fn_name) = fn_name {
                let dotified_args: Vec<Expr> = builtin_args.into_iter().map(|a| dotify_expr(a, span)).collect();
                make_broadcasted_call(fn_name, dotified_args, builtin_span)
            } else {
                Expr::Builtin { name, args: builtin_args, span: builtin_span }
            }
        }
        Expr::UnaryOp { op, operand, span: op_span } => {
            let dotified_operand = dotify_expr(*operand, span);
            let op_name = match op { crate::ir::core::UnaryOp::Neg => Some("-"), crate::ir::core::UnaryOp::Not => Some("!"), _ => None };
            if let Some(op_name) = op_name {
                make_broadcasted_call(op_name, vec![dotified_operand], op_span)
            } else {
                Expr::UnaryOp { op, operand: Box::new(dotified_operand), span: op_span }
            }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Literal, UnaryOp};
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), s())
    }

    // ── dotify_expr ───────────────────────────────────────────────────────────

    #[test]
    fn test_dotify_expr_literal_passes_through() {
        // Plain literals are left unchanged by @. transformation
        let expr = lit_int(42);
        let result = dotify_expr(expr, s());
        assert!(matches!(result, Expr::Literal(Literal::Int(42), _)));
    }

    #[test]
    fn test_dotify_expr_call_becomes_broadcasted() {
        // f(x) → materialize(Broadcasted(f, (x,)))
        let arg = lit_int(1);
        let call = Expr::Call {
            function: "f".to_string(),
            args: vec![arg],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: s(),
        };
        let result = dotify_expr(call, s());
        // Result should be materialize(...)
        assert!(
            matches!(&result, Expr::Call { function, .. } if function == "materialize"),
            "Expected materialize(...), got {:?}",
            result
        );
    }

    #[test]
    fn test_dotify_expr_materialize_call_not_rewrapped() {
        // materialize(...) is already broadcast — should NOT be re-wrapped
        let inner = Expr::Call {
            function: "Broadcasted".to_string(),
            args: vec![lit_int(0)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: s(),
        };
        let mat = Expr::Call {
            function: "materialize".to_string(),
            args: vec![inner],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: s(),
        };
        let result = dotify_expr(mat, s());
        // Should still be materialize (not double-wrapped)
        assert!(
            matches!(&result, Expr::Call { function, .. } if function == "materialize"),
            "Expected materialize to not be re-wrapped, got {:?}",
            result
        );
    }

    #[test]
    fn test_dotify_expr_binary_add_becomes_broadcasted() {
        // a + b → materialize(Broadcasted(+, (a, b)))
        let left = lit_int(1);
        let right = lit_int(2);
        let bin = Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(left),
            right: Box::new(right),
            span: s(),
        };
        let result = dotify_expr(bin, s());
        assert!(
            matches!(&result, Expr::Call { function, .. } if function == "materialize"),
            "Expected materialize(Broadcasted(+, ...)), got {:?}",
            result
        );
    }

    #[test]
    fn test_dotify_expr_assign_value_is_dotified() {
        // x = f(y) → x = materialize(Broadcasted(f, (y,)))
        let val = Expr::Call {
            function: "f".to_string(),
            args: vec![lit_int(1)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: s(),
        };
        let assign = Expr::AssignExpr {
            var: "x".to_string(),
            value: Box::new(val),
            span: s(),
        };
        let result = dotify_expr(assign, s());
        // AssignExpr is preserved, but value is dotified
        assert!(matches!(result, Expr::AssignExpr { .. }), "Expected AssignExpr, got {:?}", result);
        if let Expr::AssignExpr { var, value, .. } = result {
            assert_eq!(var, "x");
            assert!(
                matches!(*value, Expr::Call { ref function, .. } if function == "materialize"),
                "Expected dotified value, got {:?}",
                value
            );
        }
    }

    #[test]
    fn test_dotify_expr_unary_neg_becomes_broadcasted() {
        // -x → materialize(Broadcasted(-, (x,)))
        let operand = lit_int(5);
        let unary = Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand: Box::new(operand),
            span: s(),
        };
        let result = dotify_expr(unary, s());
        assert!(
            matches!(&result, Expr::Call { function, .. } if function == "materialize"),
            "Expected materialize(Broadcasted(-, ...)), got {:?}",
            result
        );
    }
}

/// Lower an expression with @views transformation enabled.
pub(crate) fn lower_expr_with_views<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let kind = walker.kind(&node);

    match kind {
        NodeKind::IndexExpression => {
            let children: Vec<Node<'a>> = walker.named_children(&node);
            if children.is_empty() { return lower_expr_with_ctx(walker, node, lambda_ctx); }

            let array_node = children[0];
            let index_nodes = &children[1..];

            let has_range_index = index_nodes.iter().any(|idx| matches!(walker.kind(idx), NodeKind::RangeExpression));

            if has_range_index {
                let array_expr = lower_expr_with_views(walker, array_node, lambda_ctx)?;
                let mut call_args = vec![array_expr];
                for index_node in index_nodes {
                    let index_expr = lower_expr_with_views(walker, *index_node, lambda_ctx)?;
                    call_args.push(index_expr);
                }
                Ok(Expr::Call { function: "view".to_string(), args: call_args, kwargs: vec![], splat_mask: vec![], kwargs_splat_mask: vec![], span })
            } else {
                lower_expr_with_ctx(walker, node, lambda_ctx)
            }
        }
        NodeKind::CompoundStatement => {
            let children: Vec<Node<'a>> = walker.named_children(&node);
            if children.is_empty() { return Ok(Expr::Literal(Literal::Nothing, span)); }
            if children.len() == 1 { return lower_expr_with_views(walker, children[0], lambda_ctx); }

            let mut stmts = Vec::new();
            for child in &children {
                let expr = lower_expr_with_views(walker, *child, lambda_ctx)?;
                stmts.push(crate::ir::core::Stmt::Expr { expr, span });
            }

            Ok(Expr::LetBlock { bindings: vec![], body: crate::ir::core::Block { stmts, span }, span })
        }
        _ => lower_expr_with_ctx(walker, node, lambda_ctx),
    }
}
