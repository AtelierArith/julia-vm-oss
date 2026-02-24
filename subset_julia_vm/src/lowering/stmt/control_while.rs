//! While statement lowering
//!
//! Handles:
//! - While loops: `while cond ... end`
//!
//! Note: Same parsing quirk as if statements - tree-sitter may parse
//! `while x + 1 <= y` as condition=x, block starts with `+ 1 <= y`.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, Block, Expr, Stmt};
use crate::lowering::expr;
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::lower_block;
use super::lower_block_with_ctx;
use super::lower_stmt;
use super::lower_stmt_with_ctx;

pub fn lower_while_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // First named child is condition, second is block
    if named.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::WhileStatement, span)
                .with_hint("while requires condition and body"),
        );
    }

    let condition_node = named[0];
    let body_node = named
        .iter()
        .find(|n| walker.kind(n) == NodeKind::Block)
        .copied();

    let body_node = body_node.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::WhileStatement, span)
            .with_hint("missing while body")
    })?;

    // Apply the same parsing quirk fix as for if statements:
    // Tree-sitter may parse `while x + 1 <= y` as condition=x, block starts with `+ 1 <= y`
    let (condition, body) = try_fix_while_condition_parsing(walker, condition_node, body_node)?;

    Ok(Stmt::While {
        condition,
        body,
        span,
    })
}

/// Attempt to fix tree-sitter's while-condition parsing quirk.
/// Same issue as if statements: `while x + 1 <= y` may be parsed as condition=x, block=`+ 1 <= y ...`
fn try_fix_while_condition_parsing<'a>(
    walker: &CstWalker<'a>,
    condition_node: Node<'a>,
    block_node: Node<'a>,
) -> LowerResult<(Expr, Block)> {
    let block_children: Vec<Node<'a>> = walker.named_children(&block_node);

    // Check if block starts with a binary expression that has a unary expression on the left
    if let Some(first_child) = block_children.first() {
        if first_child.kind() == "binary_expression" {
            if let Some(merged) =
                try_merge_condition_with_block_expr(walker, condition_node, *first_child)?
            {
                // Successfully merged - now lower the rest of the block
                let remaining_stmts: Vec<Stmt> = block_children[1..]
                    .iter()
                    .filter_map(|child| {
                        // Try to lower as statement or expression
                        if let Ok(stmt) = lower_stmt(walker, *child) {
                            Some(stmt)
                        } else if let Ok(expr) = expr::lower_expr(walker, *child) {
                            Some(Stmt::Expr {
                                expr,
                                span: walker.span(child),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                let body = Block {
                    stmts: remaining_stmts,
                    span: walker.span(&block_node),
                };

                return Ok((merged, body));
            }
        }
    }

    // No fixup needed - use normal lowering
    let condition_expr = expr::lower_expr(walker, condition_node)?;
    let body = lower_block(walker, block_node)?;
    Ok((condition_expr, body))
}

/// Try to merge a condition node with a binary expression from the block.
/// Reused from control_if but duplicated here to avoid circular dependencies.
fn try_merge_condition_with_block_expr<'a>(
    walker: &CstWalker<'a>,
    condition_node: Node<'a>,
    binary_expr_node: Node<'a>,
) -> LowerResult<Option<Expr>> {
    let bin_children: Vec<Node<'a>> = walker.children(&binary_expr_node);

    // Find the unary expression, operator, and right operand
    let mut unary_node: Option<Node<'a>> = None;
    let mut op_node: Option<Node<'a>> = None;
    let mut right_node: Option<Node<'a>> = None;

    for child in &bin_children {
        let kind = child.kind();
        if kind == "unary_expression" && unary_node.is_none() {
            unary_node = Some(*child);
        } else if kind == "operator" && unary_node.is_some() && op_node.is_none() {
            op_node = Some(*child);
        } else if op_node.is_some() && right_node.is_none() && child.is_named() && child.is_named()
        {
            right_node = Some(*child);
        }
    }

    let unary_node = match unary_node {
        Some(n) => n,
        None => return Ok(None),
    };
    let comparison_op_node = match op_node {
        Some(n) => n,
        None => return Ok(None),
    };
    let right_node = match right_node {
        Some(n) => n,
        None => return Ok(None),
    };

    let unary_children: Vec<Node<'a>> = walker.children(&unary_node);
    let mut unary_op: Option<&str> = None;
    let mut unary_operand: Option<Node<'a>> = None;

    for child in &unary_children {
        let kind = child.kind();
        if kind == "operator" {
            unary_op = Some(walker.text(child));
        } else if child.is_named() {
            unary_operand = Some(*child);
        }
    }

    let unary_op = match unary_op {
        Some(op) => op,
        None => return Ok(None),
    };
    let unary_operand = match unary_operand {
        Some(n) => n,
        None => return Ok(None),
    };

    let binary_op = match unary_op {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        _ => return Ok(None),
    };

    let comparison_op_text = walker.text(&comparison_op_node);
    let comparison_op = match comparison_op_text {
        ">" => BinaryOp::Gt,
        "<" => BinaryOp::Lt,
        ">=" => BinaryOp::Ge,
        "<=" => BinaryOp::Le,
        "==" => BinaryOp::Eq,
        "!=" => BinaryOp::Ne,
        "&&" => BinaryOp::And,
        "||" => BinaryOp::Or,
        _ => return Ok(None),
    };

    let left_expr = expr::lower_expr(walker, condition_node)?;
    let mid_expr = expr::lower_expr(walker, unary_operand)?;
    let right_expr = expr::lower_expr(walker, right_node)?;

    let span = walker.span(&binary_expr_node);

    let inner_expr = Expr::BinaryOp {
        op: binary_op,
        left: Box::new(left_expr),
        right: Box::new(mid_expr),
        span,
    };

    let merged_expr = Expr::BinaryOp {
        op: comparison_op,
        left: Box::new(inner_expr),
        right: Box::new(right_expr),
        span,
    };

    Ok(Some(merged_expr))
}

// ==================== Lambda Context Versions ====================

pub fn lower_while_stmt_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::WhileStatement, span)
                .with_hint("while requires condition and body"),
        );
    }

    let condition_node = named[0];
    let body_node = named
        .iter()
        .find(|n| walker.kind(n) == NodeKind::Block)
        .copied();

    let body_node = body_node.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::WhileStatement, span)
            .with_hint("missing while body")
    })?;

    // Apply the same parsing quirk fix as for if statements:
    // Tree-sitter may parse `while x + 1 <= y` as condition=x, block starts with `+ 1 <= y`
    let (condition, body) =
        try_fix_while_condition_parsing_with_ctx(walker, condition_node, body_node, lambda_ctx)?;

    Ok(Stmt::While {
        condition,
        body,
        span,
    })
}

/// Context-aware version of try_fix_while_condition_parsing for use with lambdas
fn try_fix_while_condition_parsing_with_ctx<'a>(
    walker: &CstWalker<'a>,
    condition_node: Node<'a>,
    block_node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(Expr, Block)> {
    let block_children: Vec<Node<'a>> = walker.named_children(&block_node);

    // Check if block starts with a binary expression that has a unary expression on the left
    if let Some(first_child) = block_children.first() {
        if first_child.kind() == "binary_expression" {
            if let Some(merged) =
                try_merge_condition_with_block_expr(walker, condition_node, *first_child)?
            {
                // Successfully merged - now lower the rest of the block with context
                let remaining_stmts: Vec<Stmt> = block_children[1..]
                    .iter()
                    .filter_map(|child| {
                        // Try to lower as statement or expression with context
                        if let Ok(stmt) = lower_stmt_with_ctx(walker, *child, lambda_ctx) {
                            Some(stmt)
                        } else if let Ok(expr) =
                            expr::lower_expr_with_ctx(walker, *child, lambda_ctx)
                        {
                            Some(Stmt::Expr {
                                expr,
                                span: walker.span(child),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                let body = Block {
                    stmts: remaining_stmts,
                    span: walker.span(&block_node),
                };

                return Ok((merged, body));
            }
        }
    }

    // No fixup needed - use normal lowering with context
    let condition_expr = expr::lower_expr_with_ctx(walker, condition_node, lambda_ctx)?;
    let body = lower_block_with_ctx(walker, block_node, lambda_ctx)?;
    Ok((condition_expr, body))
}

#[cfg(test)]
mod tests {
    use crate::ir::core::Stmt;
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    fn lower_first_stmt(source: &str) -> Stmt {
        let mut parser = Parser::new().expect("Failed to init parser");
        let parse_outcome = parser.parse(source).expect("Failed to parse");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parse_outcome).expect("Failed to lower");
        assert!(!program.main.stmts.is_empty(), "No statements found");
        program.main.stmts[0].clone()
    }

    #[test]
    fn test_while_basic() {
        let stmt = lower_first_stmt("while true\n  1\nend");
        assert!(
            matches!(stmt, Stmt::While { .. }),
            "Expected While statement, got {:?}",
            stmt
        );
    }

    #[test]
    fn test_while_with_comparison() {
        let stmt = lower_first_stmt("while x > 0\n  x\nend");
        assert!(
            matches!(stmt, Stmt::While { .. }),
            "Expected While statement, got {:?}",
            stmt
        );
    }

    #[test]
    fn test_while_body_has_statements() {
        let stmt = lower_first_stmt("while true\n  x = 1\n  y = 2\nend");
        assert!(matches!(stmt, Stmt::While { .. }), "Expected While statement, got {:?}", stmt);
        if let Stmt::While { body, .. } = stmt {
            assert_eq!(body.stmts.len(), 2, "While body should have 2 statements");
        }
    }
}
