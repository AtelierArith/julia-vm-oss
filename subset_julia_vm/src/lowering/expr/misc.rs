//! Miscellaneous expression lowering.
//!
//! This module handles lowering of field expressions, tuples, adjoint,
//! broadcast calls, let expressions, ternary expressions, and parenthesized expressions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Expr, Stmt};
use crate::lowering::stmt::lower_stmt;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::{lower_argument_list, lower_expr};

/// Lower parenthesized expression: (expr)
pub fn lower_parenthesized_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let mut named = walker.named_children(&node);
    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("empty parentheses".to_string()),
            walker.span(&node),
        ));
    }
    lower_expr(walker, named.remove(0))
}

/// Lower parenthesized expression with lambda context: (expr)
/// Propagates the lambda context so that `(==(x))` correctly creates a lifted lambda.
pub fn lower_parenthesized_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let mut named = walker.named_children(&node);
    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("empty parentheses".to_string()),
            walker.span(&node),
        ));
    }
    super::lower_expr_with_ctx(walker, named.remove(0), lambda_ctx)
}

/// Lower field expression: obj.field
pub fn lower_field_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("field expression".to_string()),
            span,
        ));
    }

    // First child is the object, second is the field name
    let object = lower_expr(walker, named[0])?;

    // The field name should be an identifier
    let field_node = named[1];
    let field = match walker.kind(&field_node) {
        NodeKind::Identifier => walker.text(&field_node).to_string(),
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression("invalid field name".to_string()),
                walker.span(&field_node),
            ));
        }
    };

    Ok(Expr::FieldAccess {
        object: Box::new(object),
        field,
        span,
    })
}

/// Lower tuple expression: (a, b, c) or (a=1, b=2)
pub fn lower_tuple_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Check if this is a named tuple (all elements are assignments)
    let mut is_named = true;
    let mut elements = Vec::new();
    let mut named_fields = Vec::new();

    for child in &named {
        match walker.kind(child) {
            NodeKind::Assignment | NodeKind::KeywordArgument => {
                // Named tuple field: name = value (Assignment or KeywordArgument)
                // For Assignment, children may be [name, =, value] where = is an Operator
                // For KeywordArgument, children are [name, value]
                let children = walker.named_children(child);
                // Filter out Operator nodes to get [name, value]
                let non_op_children: Vec<_> = children
                    .into_iter()
                    .filter(|c| walker.kind(c) != NodeKind::Operator)
                    .collect();
                if non_op_children.len() >= 2 {
                    let name_node = non_op_children[0];
                    let value_node = non_op_children[1];
                    if walker.kind(&name_node) == NodeKind::Identifier {
                        let name = walker.text(&name_node).to_string();
                        let value = lower_expr(walker, value_node)?;
                        named_fields.push((name, value));
                    } else {
                        is_named = false;
                    }
                } else {
                    is_named = false;
                }
            }
            _ => {
                is_named = false;
                let elem = lower_expr(walker, *child)?;
                elements.push(elem);
            }
        }
    }

    if is_named && !named_fields.is_empty() {
        Ok(Expr::NamedTupleLiteral {
            fields: named_fields,
            span,
        })
    } else {
        // Regular tuple - collect all elements
        if !elements.is_empty() {
            Ok(Expr::TupleLiteral { elements, span })
        } else {
            // Re-process as regular tuple
            let mut elems = Vec::new();
            for child in &named {
                let elem = lower_expr(walker, *child)?;
                elems.push(elem);
            }
            Ok(Expr::TupleLiteral {
                elements: elems,
                span,
            })
        }
    }
}

/// Extract field assignment target from a field expression (for FieldAssign)
/// Returns (object_var_name, field_name) if the object is a simple variable
pub fn extract_field_target<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(String, String)> {
    let named = walker.named_children(&node);
    if named.len() < 2 {
        return None;
    }

    // First child should be an identifier (the object variable)
    let object_node = named[0];
    if walker.kind(&object_node) != NodeKind::Identifier {
        return None;
    }
    let object_name = walker.text(&object_node).to_string();

    // Second child should be an identifier (the field name)
    let field_node = named[1];
    if walker.kind(&field_node) != NodeKind::Identifier {
        return None;
    }
    let field_name = walker.text(&field_node).to_string();

    Some((object_name, field_name))
}

/// Extract nested field assignment target from a (possibly nested) field expression.
/// For `o.inner.value`, returns the object expression (lowered) and the final field name.
/// This supports arbitrary nesting depth like `a.b.c.d = x`.
pub fn extract_nested_field_target<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(Expr, String)> {
    let named = walker.named_children(&node);
    if named.len() < 2 {
        return None;
    }

    let object_node = named[0];
    let field_node = named[1];

    // Field name must be an identifier
    if walker.kind(&field_node) != NodeKind::Identifier {
        return None;
    }
    let field_name = walker.text(&field_node).to_string();

    // Object can be an identifier (simple case) or a nested field expression
    let object_expr = lower_expr(walker, object_node).ok()?;

    Some((object_expr, field_name))
}

/// Extract nested field assignment target with lambda context.
/// Same as `extract_nested_field_target` but uses `lower_expr_with_ctx` for closures.
pub fn extract_nested_field_target_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> Option<(Expr, String)> {
    let named = walker.named_children(&node);
    if named.len() < 2 {
        return None;
    }

    let object_node = named[0];
    let field_node = named[1];

    // Field name must be an identifier
    if walker.kind(&field_node) != NodeKind::Identifier {
        return None;
    }
    let field_name = walker.text(&field_node).to_string();

    // Object can be an identifier (simple case) or a nested field expression
    let object_expr = super::lower_expr_with_ctx(walker, object_node, lambda_ctx).ok()?;

    Some((object_expr, field_name))
}

/// Lower adjoint expression: x' (transpose/conjugate transpose)
/// Now generates a function call to the Pure Julia `adjoint` function
pub fn lower_adjoint_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("empty adjoint".to_string()),
            span,
        ));
    }

    // First named child is the operand
    let operand = lower_expr(walker, named[0])?;

    // Generate a function call to Pure Julia adjoint
    Ok(Expr::Call {
        function: "adjoint".to_string(),
        args: vec![operand],
        kwargs: vec![],
        splat_mask: vec![false],
        kwargs_splat_mask: vec![],
        span,
    })
}

/// Lower broadcast call expression: f.(x) - apply function element-wise
/// Also handles dotted operator calls: .+([1,2,3]), .-(x, y), etc.
pub fn lower_broadcast_call_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedCallTarget,
            span,
        ));
    }

    // First child is the function being called (identifier or operator)
    let callee = named[0];
    let fn_name = match walker.kind(&callee) {
        NodeKind::Identifier => walker.text(&callee).to_string(),
        NodeKind::Operator => {
            // Handle dotted operator as function: .+, .-, .*, etc.
            // The operator text includes the dot prefix (e.g., ".+")
            // We need to extract the base operator for the broadcast call
            let op_text = walker.text(&callee);
            if let Some(stripped) = op_text.strip_prefix('.') {
                // Extract base operator (e.g., ".+" -> "+")
                stripped.to_string()
            } else {
                op_text.to_string()
            }
        }
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedCallTarget,
                walker.span(&callee),
            ))
        }
    };

    // Remaining children form the argument list
    // Tree-sitter uses ArgumentList or TupleExpression, Pure Rust parser uses direct children
    let args_node = named.iter().skip(1).find(|n| {
        matches!(
            walker.kind(n),
            NodeKind::ArgumentList | NodeKind::TupleExpression
        )
    });

    let args = match args_node {
        Some(node) => lower_argument_list(walker, *node)?,
        None => {
            // Pure Rust parser: arguments are direct children after the function name
            let mut args = Vec::new();
            for child in named.iter().skip(1) {
                args.push(super::lower_expr(walker, *child)?);
            }
            args
        }
    };

    // Generate materialize(Broadcasted(fn, (args...))) for Pure Julia broadcast (Issue #2546)
    // e.g., sqrt.(x) -> materialize(Broadcasted(sqrt, (x,)))
    // e.g., .+([1,2,3]) -> materialize(Broadcasted(+, ([1,2,3],)))
    // Nested broadcasts are fused: sin.(cos.(x)) -> materialize(Broadcasted(sin, (Broadcasted(cos, (x,)),)))
    Ok(super::make_broadcasted_call(&fn_name, args, span))
}

/// Lower let statement/expression: let a = 1, b = 2; body end
/// Tree structure:
///   let_statement
///     let
///     assignment 'a = 1'
///     [assignment 'b = 2']
///     block 'body'
///     end
pub fn lower_let_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    let mut bindings = Vec::new();
    let mut body_block = None;

    for child in named {
        match walker.kind(&child) {
            NodeKind::LetBindings => {
                // Pure Rust parser wraps assignments in LetBindings node
                for binding_child in walker.named_children(&child) {
                    if walker.kind(&binding_child) == NodeKind::Assignment {
                        if let Some((var_name, value)) = parse_let_binding(walker, binding_child)? {
                            bindings.push((var_name, value));
                        }
                    }
                }
            }
            NodeKind::Assignment => {
                // Parse assignment: var = value
                if let Some((var_name, value)) = parse_let_binding(walker, child)? {
                    bindings.push((var_name, value));
                }
            }
            NodeKind::Block => {
                // Lower the entire body block
                let block_children = walker.named_children(&child);
                let block_span = walker.span(&child);
                let mut stmts = Vec::new();
                for stmt_node in block_children {
                    stmts.push(lower_stmt(walker, stmt_node)?);
                }
                body_block = Some(Block {
                    stmts,
                    span: block_span,
                });
            }
            _ => {
                // Try to handle as a single expression (for single-expression let blocks)
                if body_block.is_none() {
                    if let Ok(expr) = lower_expr(walker, child) {
                        // Wrap single expression in a block
                        let child_span = walker.span(&child);
                        body_block = Some(Block {
                            stmts: vec![Stmt::Expr {
                                expr,
                                span: child_span,
                            }],
                            span: child_span,
                        });
                    }
                }
            }
        }
    }

    // If no body block found, default to empty block
    let body = body_block.unwrap_or_else(|| Block {
        stmts: vec![],
        span,
    });

    Ok(Expr::LetBlock {
        bindings,
        body,
        span,
    })
}

/// Parse a single let binding assignment.
/// Returns (variable_name, value_expr) if successful.
fn parse_let_binding<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<(String, Expr)>> {
    let assign_children = walker.named_children(&node);

    // Find identifier and value, skipping the operator
    let var_node = assign_children
        .iter()
        .find(|n| matches!(walker.kind(n), NodeKind::Identifier));
    let value_node = assign_children
        .iter()
        .find(|n| !matches!(walker.kind(n), NodeKind::Identifier | NodeKind::Operator));

    if let (Some(var_node), Some(value_node)) = (var_node, value_node) {
        let var_name = walker.text(var_node).to_string();
        let value = lower_expr(walker, *value_node)?;
        Ok(Some((var_name, value)))
    } else {
        Ok(None)
    }
}

/// Lower ternary expression: cond ? then_expr : else_expr
pub fn lower_ternary_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);

    // Get all named children (condition, then_expr, else_expr)
    // Filter out operator nodes (? and :)
    let operands: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if operands.len() != 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "ternary expression (expected 3 operands, got {})",
                operands.len()
            )),
            span,
        ));
    }

    let condition = lower_expr(walker, operands[0])?;
    let then_expr = lower_expr(walker, operands[1])?;
    let else_expr = lower_expr(walker, operands[2])?;

    Ok(Expr::Ternary {
        condition: Box::new(condition),
        then_expr: Box::new(then_expr),
        else_expr: Box::new(else_expr),
        span,
    })
}

/// Lower if expression: if cond expr1 else expr2 end
/// Converts to Ternary when each branch has a single expression
pub fn lower_if_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let all_children: Vec<Node<'a>> = walker.children(&node);

    let mut condition: Option<Node<'a>> = None;
    let mut then_block: Option<Node<'a>> = None;
    let mut else_expr: Option<Expr> = None;

    let mut i = 0;
    while i < all_children.len() {
        let child = all_children[i];
        let kind_str = child.kind();

        match kind_str {
            "if" | "end" => {
                // Skip keywords
            }
            "elseif" | "elseif_clause" => {
                // elseif chains: recursively process as nested ternary
                let elseif_expr = lower_elseif_as_expr(walker, &all_children, i)?;
                else_expr = Some(elseif_expr);
                break;
            }
            "else" => {
                // Next child should be the else block
                i += 1;
                if i < all_children.len() {
                    let else_node = all_children[i];
                    if walker.kind(&else_node) == NodeKind::Block {
                        else_expr = Some(lower_block_as_expr(walker, else_node)?);
                    }
                }
                break;
            }
            "else_clause" => {
                // else_clause contains: else keyword + block
                let else_all: Vec<Node<'a>> = walker.children(&child);
                for else_child in else_all.iter() {
                    if else_child.kind() == "block" {
                        else_expr = Some(lower_block_as_expr(walker, *else_child)?);
                        break;
                    }
                }
                break;
            }
            _ => {
                // Must be condition or block
                if condition.is_none() {
                    condition = Some(child);
                } else if then_block.is_none() && walker.kind(&child) == NodeKind::Block {
                    then_block = Some(child);
                }
            }
        }
        i += 1;
    }

    let condition_node = condition.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "if expression: missing condition".to_string(),
            ),
            span,
        )
    })?;

    let then_block_node = then_block.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "if expression: missing then block".to_string(),
            ),
            span,
        )
    })?;

    let condition_expr = lower_expr(walker, condition_node)?;
    let then_expr = lower_block_as_expr(walker, then_block_node)?;

    // If no else branch, the else value is `nothing`
    let else_expr = else_expr.unwrap_or(Expr::Literal(crate::ir::core::Literal::Nothing, span));

    Ok(Expr::Ternary {
        condition: Box::new(condition_expr),
        then_expr: Box::new(then_expr),
        else_expr: Box::new(else_expr),
        span,
    })
}

/// Lower an elseif chain as a nested ternary expression
fn lower_elseif_as_expr<'a>(
    walker: &CstWalker<'a>,
    all_children: &[Node<'a>],
    start: usize,
) -> LowerResult<Expr> {
    let first = all_children[start];
    let span = walker.span(&first);
    let kind_str = first.kind();

    if kind_str == "elseif_clause" {
        // Parse elseif_clause node
        let clause_children: Vec<Node<'a>> = walker.children(&first);
        let mut condition: Option<Node<'a>> = None;
        let mut then_block: Option<Node<'a>> = None;

        for child in clause_children.iter() {
            let child_kind = child.kind();
            match child_kind {
                "elseif" => {}
                "block" => {
                    if condition.is_some() && then_block.is_none() {
                        then_block = Some(*child);
                    }
                }
                _ => {
                    if condition.is_none() {
                        condition = Some(*child);
                    }
                }
            }
        }

        let condition_node = condition.ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(
                    "elseif: missing condition".to_string(),
                ),
                span,
            )
        })?;

        let then_block_node = then_block.ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression("elseif: missing block".to_string()),
                span,
            )
        })?;

        let condition_expr = lower_expr(walker, condition_node)?;
        let then_expr = lower_block_as_expr(walker, then_block_node)?;

        // Look for remaining elseif/else clauses
        let remaining: Vec<Node<'a>> = all_children[start + 1..]
            .iter()
            .filter(|n| n.kind() == "elseif_clause" || n.kind() == "else_clause")
            .copied()
            .collect();

        let else_expr = if !remaining.is_empty() {
            let next = remaining[0];
            if next.kind() == "else_clause" {
                // Get block from else_clause
                let else_children: Vec<Node<'a>> = walker.children(&next);
                let mut block_expr = None;
                for else_child in else_children.iter() {
                    if else_child.kind() == "block" {
                        block_expr = Some(lower_block_as_expr(walker, *else_child)?);
                        break;
                    }
                }
                block_expr.unwrap_or(Expr::Literal(crate::ir::core::Literal::Nothing, span))
            } else {
                // Another elseif_clause - recurse with remaining
                lower_elseif_clause_chain_as_expr(walker, &remaining)?
            }
        } else {
            Expr::Literal(crate::ir::core::Literal::Nothing, span)
        };

        return Ok(Expr::Ternary {
            condition: Box::new(condition_expr),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
            span,
        });
    }

    // Fallback for old "elseif" keyword format
    let mut i = start;
    let mut condition: Option<Node<'a>> = None;
    let mut then_block: Option<Node<'a>> = None;
    let mut else_expr: Option<Expr> = None;

    while i < all_children.len() {
        let child = all_children[i];
        let child_kind = child.kind();

        match child_kind {
            "end" => break,
            "elseif" => {
                if condition.is_some() && then_block.is_some() {
                    // New elseif - recurse
                    else_expr = Some(lower_elseif_as_expr(walker, all_children, i + 1)?);
                    break;
                }
            }
            "else" => {
                i += 1;
                if i < all_children.len() {
                    let else_node = all_children[i];
                    if walker.kind(&else_node) == NodeKind::Block {
                        else_expr = Some(lower_block_as_expr(walker, else_node)?);
                    }
                }
                break;
            }
            "else_clause" => {
                let else_all: Vec<Node<'a>> = walker.children(&child);
                for else_child in else_all.iter() {
                    if else_child.kind() == "block" {
                        else_expr = Some(lower_block_as_expr(walker, *else_child)?);
                        break;
                    }
                }
                break;
            }
            _ => {
                if condition.is_none() {
                    condition = Some(child);
                } else if then_block.is_none() && walker.kind(&child) == NodeKind::Block {
                    then_block = Some(child);
                }
            }
        }
        i += 1;
    }

    let condition_node = condition.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("elseif: missing condition".to_string()),
            span,
        )
    })?;

    let then_block_node = then_block.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("elseif: missing block".to_string()),
            span,
        )
    })?;

    let condition_expr = lower_expr(walker, condition_node)?;
    let then_expr = lower_block_as_expr(walker, then_block_node)?;
    let else_expr = else_expr.unwrap_or(Expr::Literal(crate::ir::core::Literal::Nothing, span));

    Ok(Expr::Ternary {
        condition: Box::new(condition_expr),
        then_expr: Box::new(then_expr),
        else_expr: Box::new(else_expr),
        span,
    })
}

/// Lower a chain of elseif_clause nodes as nested ternary
fn lower_elseif_clause_chain_as_expr<'a>(
    walker: &CstWalker<'a>,
    clauses: &[Node<'a>],
) -> LowerResult<Expr> {
    if clauses.is_empty() {
        return Ok(Expr::Literal(
            crate::ir::core::Literal::Nothing,
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        ));
    }

    let first = clauses[0];
    let span = walker.span(&first);
    let kind_str = first.kind();

    if kind_str == "else_clause" {
        // Get block from else_clause
        let else_children: Vec<Node<'a>> = walker.children(&first);
        for else_child in else_children.iter() {
            if else_child.kind() == "block" {
                return lower_block_as_expr(walker, *else_child);
            }
        }
        return Ok(Expr::Literal(crate::ir::core::Literal::Nothing, span));
    }

    // Must be elseif_clause
    let clause_children: Vec<Node<'a>> = walker.children(&first);
    let mut condition: Option<Node<'a>> = None;
    let mut then_block: Option<Node<'a>> = None;

    for child in clause_children.iter() {
        let child_kind = child.kind();
        match child_kind {
            "elseif" => {}
            "block" => {
                if condition.is_some() && then_block.is_none() {
                    then_block = Some(*child);
                }
            }
            _ => {
                if condition.is_none() {
                    condition = Some(*child);
                }
            }
        }
    }

    let condition_node = condition.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("elseif: missing condition".to_string()),
            span,
        )
    })?;

    let then_block_node = then_block.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("elseif: missing block".to_string()),
            span,
        )
    })?;

    let condition_expr = lower_expr(walker, condition_node)?;
    let then_expr = lower_block_as_expr(walker, then_block_node)?;

    let else_expr = if clauses.len() > 1 {
        lower_elseif_clause_chain_as_expr(walker, &clauses[1..])?
    } else {
        Expr::Literal(crate::ir::core::Literal::Nothing, span)
    };

    Ok(Expr::Ternary {
        condition: Box::new(condition_expr),
        then_expr: Box::new(then_expr),
        else_expr: Box::new(else_expr),
        span,
    })
}

/// Lower a block to an expression (returns the last expression's value)
fn lower_block_as_expr<'a>(walker: &CstWalker<'a>, block_node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&block_node);
    let children = walker.named_children(&block_node);

    if children.is_empty() {
        // Empty block returns nothing
        return Ok(Expr::Literal(crate::ir::core::Literal::Nothing, span));
    }

    if children.len() == 1 {
        // Single expression - just lower it directly
        return lower_expr(walker, children[0]);
    }

    // Multiple statements - wrap in a LetBlock
    let mut stmts = Vec::new();
    for child in children {
        let stmt = lower_stmt(walker, child)?;
        stmts.push(stmt);
    }

    let body = Block { stmts, span };
    Ok(Expr::LetBlock {
        bindings: vec![],
        body,
        span,
    })
}

/// Lower pair expression: key => value
/// Returns Expr::Pair { key, value, span }
pub fn lower_pair_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);

    // Get all named children (key, operator, value)
    // Filter out operator node (=>)
    let operands: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if operands.len() != 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "pair expression (expected 2 operands, got {})",
                operands.len()
            )),
            span,
        ));
    }

    let key = lower_expr(walker, operands[0])?;
    let value = lower_expr(walker, operands[1])?;

    Ok(Expr::Pair {
        key: Box::new(key),
        value: Box::new(value),
        span,
    })
}
