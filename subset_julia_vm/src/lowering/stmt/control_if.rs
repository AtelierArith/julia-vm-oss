//! If statement lowering
//!
//! Handles:
//! - If statements: `if cond ... end`
//! - If-else: `if cond ... else ... end`
//! - If-elseif-else chains: `if cond ... elseif cond ... else ... end`
//! - `if @generated ... else ... end` pattern:
//!   - Phase 1: Executes fallback (else branch) only
//!   - Phase 3: If generated branch is a simple quoted expression like `:(x^2)`,
//!     "unquotes" it and uses the inner expression directly
//!
//! Note: tree-sitter-julia has a parsing quirk where `if 1 + 2 > 3` is parsed as:
//!   - condition: `1`
//!   - block: `+ 2 > 3 ...`
//! This module detects and corrects this by merging the condition with the
//! block's leading binary expression when appropriate.

use std::collections::HashSet;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, Block, Expr, Literal, Stmt};
use crate::lowering::expr;
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

use super::lower_block;
use super::lower_block_with_ctx;
use super::{lower_stmt, lower_stmt_with_ctx};

/// Check if a node is a `@generated` or `@generated()` macro call.
/// This is used to detect the `if @generated ... else ... end` pattern.
fn is_generated_macro_call<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    let kind = walker.kind(&node);
    if kind != NodeKind::MacroCall {
        return false;
    }

    // Find the macro identifier
    if let Some(macro_ident) = walker.find_child(&node, NodeKind::MacroIdentifier) {
        let text = walker.text(&macro_ident);
        let name = text.trim_start_matches('@');
        return name == "generated";
    }

    false
}

// ==================== Phase 3 @generated Unquoting ====================

/// Try to extract the inner expression from a quote expression node.
/// Returns None if the node is not a valid `:(expr)` form (symbol quotes like `:foo` are skipped).
fn try_extract_quote_inner<'a>(walker: &CstWalker<'a>, quote_node: Node<'a>) -> Option<Node<'a>> {
    let text = walker.text(&quote_node);
    // Skip symbol quotes like `:foo` - only handle `:(expr)` form
    if !text.starts_with(":(") {
        return None;
    }

    let inner_children = walker.named_children(&quote_node);
    if inner_children.is_empty() {
        return None;
    }

    Some(inner_children[0])
}

fn dollar_unary_operand<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<Node<'a>> {
    if walker.kind(&node) != NodeKind::UnaryExpression {
        return None;
    }

    let mut saw_dollar = false;
    let mut operand = None;
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Operator && walker.text(&child) == "$" {
            saw_dollar = true;
        } else {
            operand = Some(child);
        }
    }

    if saw_dollar {
        operand
    } else {
        None
    }
}

fn collect_quote_identifier_refs<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    interpolated_names: &mut HashSet<String>,
    bare_names: &mut HashSet<String>,
) {
    if let Some(operand) = dollar_unary_operand(walker, node) {
        if walker.kind(&operand) == NodeKind::Identifier {
            interpolated_names.insert(walker.text(&operand).to_string());
        }
        // Identifiers inside `$(expr)` are generated-time expressions, not
        // runtime-frame references in the returned Expr.
        return;
    }

    if walker.kind(&node) == NodeKind::Identifier {
        bare_names.insert(walker.text(&node).to_string());
        return;
    }

    for child in walker.named_children(&node) {
        collect_quote_identifier_refs(walker, child, interpolated_names, bare_names);
    }
}

fn quote_mixes_interpolated_and_runtime_identifier<'a>(
    walker: &CstWalker<'a>,
    quote_node: Node<'a>,
) -> bool {
    let Some(inner_node) = try_extract_quote_inner(walker, quote_node) else {
        return false;
    };

    let mut interpolated_names = HashSet::new();
    let mut bare_names = HashSet::new();
    collect_quote_identifier_refs(walker, inner_node, &mut interpolated_names, &mut bare_names);

    interpolated_names
        .iter()
        .any(|name| bare_names.contains(name))
}

fn node_contains_dollar_unary<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    if dollar_unary_operand(walker, node).is_some() {
        return true;
    }

    walker
        .named_children(&node)
        .into_iter()
        .any(|child| node_contains_dollar_unary(walker, child))
}

fn quote_contains_generated_interpolation<'a>(
    walker: &CstWalker<'a>,
    quote_node: Node<'a>,
) -> bool {
    let Some(inner_node) = try_extract_quote_inner(walker, quote_node) else {
        return false;
    };
    node_contains_dollar_unary(walker, inner_node)
}

fn unquoted_statement_requires_generated_frame<'a>(
    walker: &CstWalker<'a>,
    stmt_node: Node<'a>,
) -> bool {
    match walker.kind(&stmt_node) {
        NodeKind::QuoteExpression => quote_contains_generated_interpolation(walker, stmt_node),
        NodeKind::Assignment => {
            let children = walker.named_children(&stmt_node);
            children
                .get(1)
                .is_some_and(|rhs| quote_contains_generated_interpolation(walker, *rhs))
        }
        NodeKind::ReturnStatement => walker
            .named_children(&stmt_node)
            .first()
            .is_some_and(|value| quote_contains_generated_interpolation(walker, *value)),
        _ => false,
    }
}

pub(super) fn unquoted_generated_block_requires_generated_frame<'a>(
    walker: &CstWalker<'a>,
    block_node: Node<'a>,
) -> bool {
    walker
        .named_children(&block_node)
        .into_iter()
        .any(|stmt| unquoted_statement_requires_generated_frame(walker, stmt))
}

pub(super) fn unquoted_generated_short_body_requires_generated_frame<'a>(
    walker: &CstWalker<'a>,
    body_node: Node<'a>,
) -> bool {
    quote_contains_generated_interpolation(walker, body_node)
}

/// Pattern 1: Single quote expression like `:(x^2)` or `:(sin(x))`
fn try_unquote_quote_expression<'a>(
    walker: &CstWalker<'a>,
    block_node: &Node<'a>,
    stmt_node: Node<'a>,
) -> Option<LowerResult<Block>> {
    if walker.kind(&stmt_node) != NodeKind::QuoteExpression {
        return None;
    }

    let span = walker.span(&stmt_node);
    if quote_mixes_interpolated_and_runtime_identifier(walker, stmt_node) {
        return None;
    }
    let inner_node = try_extract_quote_inner(walker, stmt_node)?;

    let result = expr::lower_expr(walker, inner_node);
    match result {
        Ok(unquoted_expr) => Some(Ok(Block {
            stmts: vec![Stmt::Expr {
                expr: unquoted_expr,
                span,
            }],
            span: walker.span(block_node),
        })),
        Err(e) => Some(Err(e)),
    }
}

/// Pattern 2: Assignment with quote RHS like `result = :(x^2)`
fn try_unquote_assignment<'a>(
    walker: &CstWalker<'a>,
    block_node: &Node<'a>,
    stmt_node: Node<'a>,
) -> Option<LowerResult<Block>> {
    if walker.kind(&stmt_node) != NodeKind::Assignment {
        return None;
    }

    let assign_children = walker.named_children(&stmt_node);
    if assign_children.len() != 2 {
        return None;
    }

    let lhs_node = assign_children[0];
    let rhs_node = assign_children[1];

    // LHS must be a simple identifier for Stmt::Assign
    if walker.kind(&lhs_node) != NodeKind::Identifier {
        return None;
    }
    let var_name = walker.text(&lhs_node).to_string();

    // Check if RHS is a quote expression
    if walker.kind(&rhs_node) != NodeKind::QuoteExpression {
        return None;
    }

    if quote_mixes_interpolated_and_runtime_identifier(walker, rhs_node) {
        return None;
    }
    let inner_node = try_extract_quote_inner(walker, rhs_node)?;
    let span = walker.span(&stmt_node);

    // Lower the unquoted RHS
    match expr::lower_expr(walker, inner_node) {
        Ok(rhs_expr) => {
            let assign_stmt = Stmt::Assign {
                var: var_name,
                value: rhs_expr,
                span,
            };
            Some(Ok(Block {
                stmts: vec![assign_stmt],
                span: walker.span(block_node),
            }))
        }
        Err(e) => Some(Err(e)),
    }
}

/// Pattern 3: Return statement with quote expression like `return :(x + y)`
fn try_unquote_return<'a>(
    walker: &CstWalker<'a>,
    block_node: &Node<'a>,
    stmt_node: Node<'a>,
) -> Option<LowerResult<Block>> {
    if walker.kind(&stmt_node) != NodeKind::ReturnStatement {
        return None;
    }

    let return_children = walker.named_children(&stmt_node);
    if return_children.len() != 1 {
        return None;
    }

    let return_value_node = return_children[0];

    // Check if return value is a quote expression
    if walker.kind(&return_value_node) != NodeKind::QuoteExpression {
        return None;
    }

    if quote_mixes_interpolated_and_runtime_identifier(walker, return_value_node) {
        return None;
    }
    let inner_node = try_extract_quote_inner(walker, return_value_node)?;
    let span = walker.span(&stmt_node);

    // Lower the unquoted return value
    match expr::lower_expr(walker, inner_node) {
        Ok(return_expr) => {
            let return_stmt = Stmt::Return {
                value: Some(return_expr),
                span,
            };
            Some(Ok(Block {
                stmts: vec![return_stmt],
                span: walker.span(block_node),
            }))
        }
        Err(e) => Some(Err(e)),
    }
}

/// Try to unquote a single statement in a generated block.
/// Returns the unquoted statement if successful, None if pattern doesn't match.
fn try_unquote_single_statement<'a>(
    walker: &CstWalker<'a>,
    stmt_node: Node<'a>,
) -> Option<LowerResult<Stmt>> {
    let span = walker.span(&stmt_node);
    let kind = walker.kind(&stmt_node);

    // Pattern: Quote expression like `:(x^2)`
    if kind == NodeKind::QuoteExpression {
        if quote_mixes_interpolated_and_runtime_identifier(walker, stmt_node) {
            return None;
        }
        let inner_node = try_extract_quote_inner(walker, stmt_node)?;
        return match expr::lower_expr(walker, inner_node) {
            Ok(unquoted_expr) => Some(Ok(Stmt::Expr {
                expr: unquoted_expr,
                span,
            })),
            Err(e) => Some(Err(e)),
        };
    }

    // Pattern: Assignment with quote RHS like `a = :(x + 1)`
    if kind == NodeKind::Assignment {
        let assign_children = walker.named_children(&stmt_node);
        if assign_children.len() == 2 {
            let lhs_node = assign_children[0];
            let rhs_node = assign_children[1];

            if walker.kind(&lhs_node) == NodeKind::Identifier
                && walker.kind(&rhs_node) == NodeKind::QuoteExpression
            {
                if quote_mixes_interpolated_and_runtime_identifier(walker, rhs_node) {
                    return None;
                }
                let var_name = walker.text(&lhs_node).to_string();
                if let Some(inner_node) = try_extract_quote_inner(walker, rhs_node) {
                    return match expr::lower_expr(walker, inner_node) {
                        Ok(rhs_expr) => Some(Ok(Stmt::Assign {
                            var: var_name,
                            value: rhs_expr,
                            span,
                        })),
                        Err(e) => Some(Err(e)),
                    };
                }
            }
        }
    }

    // Pattern: Return with quote expression like `return :(x + y)`
    if kind == NodeKind::ReturnStatement {
        let return_children = walker.named_children(&stmt_node);
        if return_children.len() == 1 {
            let return_value_node = return_children[0];
            if walker.kind(&return_value_node) == NodeKind::QuoteExpression {
                if quote_mixes_interpolated_and_runtime_identifier(walker, return_value_node) {
                    return None;
                }
                if let Some(inner_node) = try_extract_quote_inner(walker, return_value_node) {
                    return match expr::lower_expr(walker, inner_node) {
                        Ok(return_expr) => Some(Ok(Stmt::Return {
                            value: Some(return_expr),
                            span,
                        })),
                        Err(e) => Some(Err(e)),
                    };
                }
            }
        }
    }

    None
}

/// Try to unquote a one-line generated function body such as
/// `@generated f(x) = :(x + 1)`.
pub(super) fn try_unquote_generated_short_body<'a>(
    walker: &CstWalker<'a>,
    body_node: Node<'a>,
) -> Option<LowerResult<Block>> {
    if walker.kind(&body_node) != NodeKind::QuoteExpression {
        return None;
    }

    if quote_mixes_interpolated_and_runtime_identifier(walker, body_node) {
        return None;
    }

    // Mark the generated-unquote path active so a `$ident` interpolation in the
    // quoted body lowers to its bound parameter rather than being rejected as
    // `UnsupportedOperator("$")` (Issue #5934).
    let _unquote_scope = crate::lowering::generated_unquote::UnquoteScope::enter();

    let inner_node = try_extract_quote_inner(walker, body_node)?;
    let span = walker.span(&body_node);
    match expr::lower_expr(walker, inner_node) {
        Ok(return_expr) => Some(Ok(Block {
            stmts: vec![Stmt::Return {
                value: Some(return_expr),
                span,
            }],
            span,
        })),
        Err(e) => Some(Err(e)),
    }
}

/// Try to unquote a multi-statement generated block.
/// All statements must be unquotable for this to succeed.
fn try_unquote_multi_statement_block<'a>(
    walker: &CstWalker<'a>,
    block_node: Node<'a>,
    children: &[Node<'a>],
) -> Option<LowerResult<Block>> {
    let mut stmts = Vec::with_capacity(children.len());

    for child in children {
        match try_unquote_single_statement(walker, *child) {
            Some(Ok(stmt)) => stmts.push(stmt),
            Some(Err(e)) => return Some(Err(e)),
            None => return None, // Pattern doesn't match, fall back to Phase 1
        }
    }

    Some(Ok(Block {
        stmts,
        span: walker.span(&block_node),
    }))
}

/// Phase 3: Try to "unquote" simple generated expressions in a block.
///
/// Transforms blocks containing quote expressions:
/// - `:(x^2)` → lowered as `x^2`
/// - `result = :(x^2)` → lowered as `result = x^2`
/// - `return :(x + y)` → lowered as `return x + y`
///
/// Also supports multi-statement blocks where all statements are unquotable:
/// ```julia
/// a = :(x + 1)
/// b = :(a * 2)
/// b
/// ```
///
/// Supported patterns:
/// - Single quote expression: `:(expr)`
/// - Single assignment with quote RHS: `var = :(expr)`
/// - Single return with quote RHS: `return :(expr)`
/// - Multi-statement blocks with any combination of the above
///
/// Returns None if the block doesn't match a simple pattern.
pub(super) fn try_unquote_generated_block<'a>(
    walker: &CstWalker<'a>,
    block_node: Node<'a>,
) -> Option<LowerResult<Block>> {
    let children = walker.named_children(&block_node);

    if children.is_empty() {
        return None;
    }

    // Mark the generated-unquote path active so a `$ident` interpolation in any
    // quoted statement of the body lowers to its bound parameter rather than
    // being rejected as `UnsupportedOperator("$")` (Issue #5934).
    let _unquote_scope = crate::lowering::generated_unquote::UnquoteScope::enter();

    // Multi-statement block
    if children.len() > 1 {
        return try_unquote_multi_statement_block(walker, block_node, &children);
    }

    // Single statement - try each pattern
    let stmt_node = children[0];

    try_unquote_quote_expression(walker, &block_node, stmt_node)
        .or_else(|| try_unquote_assignment(walker, &block_node, stmt_node))
        .or_else(|| try_unquote_return(walker, &block_node, stmt_node))
}

pub fn lower_if_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    lower_if_stmt_impl(walker, node, None)
}

fn lower_if_stmt_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);

    // Get all children to understand structure
    let all_children: Vec<Node<'a>> = walker.children(&node);

    // Find condition (first non-keyword expression)
    // Find then block
    // Find elseif/else clauses

    let mut condition: Option<Node<'a>> = None;
    let mut then_block: Option<Node<'a>> = None;
    let mut else_branch: Option<Block> = None;

    let mut i = 0;
    while i < all_children.len() {
        let child = all_children[i];
        let kind_str = child.kind();

        match kind_str {
            "if" | "end" => {
                // Skip keywords
            }
            "elseif" => {
                // Parse elseif as nested if (old format: elseif keyword followed by condition and block)
                let elseif_stmt = lower_elseif_chain(walker, &all_children, i + 1, lambda_ctx)?;
                else_branch = Some(Block {
                    stmts: vec![elseif_stmt],
                    span: walker.span(&child),
                });
                break;
            }
            "elseif_clause" => {
                // Parse elseif_clause nodes (tree-sitter format)
                // Multiple elseif_clauses may appear as siblings - collect them all
                // and chain them into nested if-else statements
                let remaining_clauses: Vec<Node<'a>> = all_children[i..]
                    .iter()
                    .filter(|n| n.kind() == "elseif_clause" || n.kind() == "else_clause")
                    .copied()
                    .collect();
                let elseif_stmt =
                    lower_elseif_clause_chain(walker, &remaining_clauses, lambda_ctx)?;
                else_branch = Some(Block {
                    stmts: vec![elseif_stmt],
                    span: walker.span(&child),
                });
                break;
            }
            "else" => {
                // Parse else block (old tree-sitter format)
                i += 1;
                if i < all_children.len() {
                    let else_node = all_children[i];
                    if walker.kind(&else_node) == NodeKind::Block {
                        else_branch = Some(lower_block_maybe_ctx(walker, else_node, lambda_ctx)?);
                    }
                }
                break; // else is always last before end
            }
            "else_clause" => {
                // Parse else_clause node (tree-sitter format)
                // else_clause contains: else keyword + block
                let else_all: Vec<Node<'a>> = walker.children(&child);
                for else_child in else_all.iter() {
                    if else_child.kind() == "block" {
                        else_branch = Some(lower_block_maybe_ctx(walker, *else_child, lambda_ctx)?);
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
        UnsupportedFeature::new(UnsupportedFeatureKind::IfStatement, span)
            .with_hint("missing condition")
    })?;

    let then_block_node = then_block.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::IfStatement, span)
            .with_hint("missing then block")
    })?;

    // Check for `if @generated ... else ... end` pattern
    // Phase 3: Try to "unquote" simple generated expressions
    // Phase 1 fallback: Execute the else branch if unquoting fails
    if is_generated_macro_call(walker, condition_node) {
        if lambda_ctx.is_some() {
            if let Some(else_block) = else_branch {
                let then_branch = lower_block_maybe_ctx(walker, then_block_node, lambda_ctx)?;
                return Ok(Stmt::If {
                    condition: Expr::Literal(Literal::Bool(false), span),
                    then_branch,
                    else_branch: Some(else_block),
                    span,
                });
            }
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::IfStatement,
                span,
            )
            .with_hint(
                "@generated without fallback is not supported. Use `if @generated ... else fallback end` pattern.",
            ));
        }

        // Phase 3: Try to unquote the generated block
        // Supports patterns like `:(x^2)` and `result = :(x^2)`
        if let Some(unquoted_result) = try_unquote_generated_block(walker, then_block_node) {
            match unquoted_result {
                Ok(unquoted_block) => {
                    // Successfully unquoted - use the generated block directly
                    // Use condition=true to execute the unquoted code
                    return Ok(Stmt::If {
                        condition: Expr::Literal(Literal::Bool(true), span),
                        then_branch: unquoted_block,
                        else_branch,
                        span,
                    });
                }
                Err(_) => {
                    // Unquoting failed - fall through to Phase 1 fallback
                }
            }
        }

        // Phase 1 fallback: Execute the else branch
        // Return an if statement with condition=false to execute the else branch
        // This preserves the expression semantics (if returns a value)
        if let Some(else_block) = else_branch {
            let then_branch = lower_block_maybe_ctx(walker, then_block_node, lambda_ctx)?;
            return Ok(Stmt::If {
                condition: Expr::Literal(Literal::Bool(false), span),
                then_branch,
                else_branch: Some(else_block),
                span,
            });
        } else {
            // No else branch means this is `if @generated ... end` without fallback
            // Phase 3 couldn't extract a simple expression, so we can't run the generated code
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::IfStatement,
                span,
            )
            .with_hint(
                "@generated without fallback is not supported unless the generated block is a simple quoted expression like `:(x^2)`.",
            ));
        }
    }

    // Try to fix tree-sitter parsing quirk: `if 1 + 2 > 3` parsed as condition=1, block=+2>3...
    let (condition_expr, then_branch) =
        try_fix_if_condition_parsing(walker, condition_node, then_block_node, lambda_ctx)?;

    Ok(Stmt::If {
        condition: condition_expr,
        then_branch,
        else_branch,
        span,
    })
}

/// Attempt to fix tree-sitter's if-condition parsing quirk.
///
/// Tree-sitter-julia parses `if 1 + 2 > 3` incorrectly as:
///   - condition: integer_literal `1`
///   - block: binary_expression `+ 2 > 3` followed by other statements
///
/// This function detects this pattern and reconstructs the correct condition.
fn try_fix_if_condition_parsing<'a>(
    walker: &CstWalker<'a>,
    condition_node: Node<'a>,
    block_node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
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
                        if let Ok(stmt) = lower_stmt_maybe_ctx(walker, *child, lambda_ctx) {
                            Some(stmt)
                        } else if let Ok(expr) = lower_expr_maybe_ctx(walker, *child, lambda_ctx) {
                            Some(Stmt::Expr {
                                expr,
                                span: walker.span(child),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                let then_branch = Block {
                    stmts: remaining_stmts,
                    span: walker.span(&block_node),
                };

                return Ok((merged, then_branch));
            }
        }
    }

    // No fixup needed - use normal lowering
    let condition_expr = lower_expr_maybe_ctx(walker, condition_node, lambda_ctx)?;
    let then_branch = lower_block_maybe_ctx(walker, block_node, lambda_ctx)?;
    Ok((condition_expr, then_branch))
}

/// Try to merge a condition node with a binary expression from the block.
///
/// Example: condition=`1`, binary_expr=`+ 2 > 3`
/// Should produce: `1 + 2 > 3`
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
        } else if op_node.is_some() && right_node.is_none() && child.is_named() {
            // This should be the right operand
            if child.is_named() {
                right_node = Some(*child);
            }
        }
    }

    // If we don't have the expected structure, return None
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

    // Extract the unary operator and its operand
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

    // Only handle arithmetic operators that make sense to merge
    let binary_op = match unary_op {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        _ => return Ok(None),
    };

    // Get the comparison operator
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

    // Now build the merged expression: (condition binary_op unary_operand) comparison_op right
    let left_expr = expr::lower_expr(walker, condition_node)?;
    let mid_expr = expr::lower_expr(walker, unary_operand)?;
    let right_expr = expr::lower_expr(walker, right_node)?;

    let span = walker.span(&binary_expr_node);

    // Build: (left binary_op mid) comparison_op right
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

/// Lower elseif chain starting at index i
fn lower_elseif_chain<'a>(
    walker: &CstWalker<'a>,
    all_children: &[Node<'a>],
    start: usize,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let mut i = start;
    let mut condition: Option<Node<'a>> = None;
    let mut then_block: Option<Node<'a>> = None;
    let mut else_branch: Option<Block> = None;

    let span = if i < all_children.len() {
        walker.span(&all_children[i])
    } else {
        walker.span(&all_children[start - 1])
    };

    while i < all_children.len() {
        let child = all_children[i];
        let kind_str = child.kind();

        match kind_str {
            "end" => break,
            "elseif" => {
                if condition.is_some() && then_block.is_some() {
                    // This is a new elseif, parse recursively
                    let elseif_stmt = lower_elseif_chain(walker, all_children, i + 1, lambda_ctx)?;
                    else_branch = Some(Block {
                        stmts: vec![elseif_stmt],
                        span: walker.span(&child),
                    });
                    break;
                }
                // Skip the elseif keyword itself
            }
            "else" => {
                // Parse else block
                i += 1;
                if i < all_children.len() {
                    let else_node = all_children[i];
                    if walker.kind(&else_node) == NodeKind::Block {
                        else_branch = Some(lower_block_maybe_ctx(walker, else_node, lambda_ctx)?);
                    }
                }
                break;
            }
            "else_clause" => {
                // Parse else_clause node (tree-sitter format)
                let else_all: Vec<Node<'a>> = walker.children(&child);
                for else_child in else_all.iter() {
                    if else_child.kind() == "block" {
                        else_branch = Some(lower_block_maybe_ctx(walker, *else_child, lambda_ctx)?);
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

    let condition = condition.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::IfStatement, span)
            .with_hint("missing elseif condition")
    })?;

    let then_block = then_block.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::IfStatement, span)
            .with_hint("missing elseif block")
    })?;

    let condition_expr = lower_expr_maybe_ctx(walker, condition, lambda_ctx)?;
    let then_branch = lower_block_maybe_ctx(walker, then_block, lambda_ctx)?;

    Ok(Stmt::If {
        condition: condition_expr,
        then_branch,
        else_branch,
        span,
    })
}

/// Lower a chain of elseif_clause and else_clause nodes into nested if-else statements.
/// This handles the case where tree-sitter produces sibling elseif_clauses instead of nested ones.
/// Note: This function expects the first clause to be an elseif_clause, not else_clause.
fn lower_elseif_clause_chain<'a>(
    walker: &CstWalker<'a>,
    clauses: &[Node<'a>],
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    if clauses.is_empty() {
        // Should not happen - caller should check before calling
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::IfStatement,
            Span::new(0, 0, 0, 0, 0, 0),
        )
        .with_hint("empty elseif clause chain"));
    }

    let first_clause = clauses[0];
    let kind_str = first_clause.kind();

    // First clause MUST be elseif_clause. else_clause is handled specially below.
    if kind_str != "elseif_clause" {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::IfStatement,
            walker.span(&first_clause),
        )
        .with_hint("expected elseif_clause at start of chain"));
    }

    // This is an elseif_clause
    let span = walker.span(&first_clause);
    let all_children: Vec<Node<'a>> = walker.children(&first_clause);

    let mut condition: Option<Node<'a>> = None;
    let mut then_block: Option<Node<'a>> = None;

    for child in all_children.iter() {
        let child_kind = child.kind();
        match child_kind {
            "elseif" => {
                // Skip the elseif keyword
            }
            "block" => {
                if condition.is_some() && then_block.is_none() {
                    then_block = Some(*child);
                }
            }
            _ => {
                // Must be the condition expression
                if condition.is_none() {
                    condition = Some(*child);
                }
            }
        }
    }

    let condition = condition.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::IfStatement, span)
            .with_hint("missing elseif condition")
    })?;

    let then_block = then_block.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::IfStatement, span)
            .with_hint("missing elseif block")
    })?;

    let condition_expr = lower_expr_maybe_ctx(walker, condition, lambda_ctx)?;
    let then_branch = lower_block_maybe_ctx(walker, then_block, lambda_ctx)?;

    // Recursively process remaining clauses to build else_branch
    let else_branch = if clauses.len() > 1 {
        let remaining = &clauses[1..];
        let next = remaining[0];
        let next_kind = next.kind();

        if next_kind == "else_clause" {
            // Get the block directly from else_clause
            let else_children: Vec<Node<'a>> = walker.children(&next);
            let mut block: Option<Block> = None;
            for else_child in else_children.iter() {
                if else_child.kind() == "block" {
                    block = Some(lower_block_maybe_ctx(walker, *else_child, lambda_ctx)?);
                    break;
                }
            }
            block
        } else {
            // elseif_clause - recursive call
            let else_stmt = lower_elseif_clause_chain(walker, remaining, lambda_ctx)?;
            Some(Block {
                stmts: vec![else_stmt],
                span: walker.span(&next),
            })
        }
    } else {
        None
    };

    Ok(Stmt::If {
        condition: condition_expr,
        then_branch,
        else_branch,
        span,
    })
}

// ==================== Lambda Context Versions ====================

pub fn lower_if_stmt_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    lower_if_stmt_impl(walker, node, Some(lambda_ctx))
}

fn lower_expr_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    match lambda_ctx {
        Some(ctx) => expr::lower_expr_with_ctx(walker, node, ctx),
        None => expr::lower_expr(walker, node),
    }
}

fn lower_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_stmt_with_ctx(walker, node, ctx),
        None => lower_stmt(walker, node),
    }
}

fn lower_block_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Block> {
    match lambda_ctx {
        Some(ctx) => lower_block_with_ctx(walker, node, ctx),
        None => lower_block(walker, node),
    }
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
    fn test_if_basic() {
        let stmt = lower_first_stmt("if true\n  1\nend");
        assert!(
            matches!(
                stmt,
                Stmt::If {
                    else_branch: None,
                    ..
                }
            ),
            "Expected If without else branch, got {:?}",
            stmt
        );
    }

    #[test]
    fn test_if_else() {
        let stmt = lower_first_stmt("if true\n  1\nelse\n  2\nend");
        assert!(
            matches!(
                stmt,
                Stmt::If {
                    else_branch: Some(_),
                    ..
                }
            ),
            "Expected If with else branch, got {:?}",
            stmt
        );
    }

    #[test]
    fn test_if_elseif_else() {
        let stmt = lower_first_stmt("if x > 0\n  1\nelseif x < 0\n  2\nelse\n  3\nend");
        assert!(
            matches!(
                stmt,
                Stmt::If {
                    else_branch: Some(_),
                    ..
                }
            ),
            "Expected If with else branch, got {:?}",
            stmt
        );
        if let Stmt::If {
            else_branch: Some(ref else_block),
            ..
        } = stmt
        {
            assert_eq!(
                else_block.stmts.len(),
                1,
                "Else branch should contain one nested If"
            );
            assert!(
                matches!(else_block.stmts[0], Stmt::If { .. }),
                "Elseif should be lowered as nested If, got {:?}",
                else_block.stmts[0]
            );
        }
    }
}
