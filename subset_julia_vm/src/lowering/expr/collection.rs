//! Collection expression lowering.
//!
//! This module handles lowering of vectors, matrices, ranges,
//! index expressions, and comprehensions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::Expr;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::lower_expr;

/// Lower vector expression: [1, 2, 3] or []
pub fn lower_vector_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Empty array [] is now supported - creates empty Any array
    if named.is_empty() {
        return Ok(Expr::ArrayLiteral {
            elements: vec![],
            shape: vec![0],
            span,
        });
    }

    let mut elements = Vec::new();
    for child in named {
        elements.push(lower_expr(walker, child)?);
    }

    let shape = vec![elements.len()];
    Ok(Expr::ArrayLiteral {
        elements,
        shape,
        span,
    })
}

/// Lower matrix expression: [1 2; 3 4]
/// Julia uses column-major order, so elements are stored column by column.
/// For [1 2 3; 4 5 6], the storage order is [1, 4, 2, 5, 3, 6].
pub fn lower_matrix_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::EmptyArray,
            span,
        ));
    }

    // Check if we have matrix rows
    let rows: Vec<_> = named
        .iter()
        .filter(|n| walker.kind(n) == NodeKind::MatrixRow)
        .collect();

    if rows.is_empty() {
        // Single row matrix: [1 2 3] - treat as row vector (1×n matrix)
        // For 1×n matrix, column-major order is the same as row order
        let mut elements = Vec::new();
        for child in named {
            elements.push(lower_expr(walker, child)?);
        }
        let cols = elements.len();
        return Ok(Expr::ArrayLiteral {
            elements,
            shape: vec![1, cols],
            span,
        });
    }

    // Multi-row matrix: collect elements row by row first
    let mut row_elements: Vec<Vec<Expr>> = Vec::new();
    let mut col_count = None;

    for row_node in &rows {
        let this_row_elements = walker.named_children(row_node);
        let this_col_count = this_row_elements.len();

        // Validate consistent column count
        match col_count {
            None => col_count = Some(this_col_count),
            Some(expected) if expected != this_col_count => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MalformedMatrix, span)
                        .with_hint(format!(
                            "inconsistent column count: expected {}, got {}",
                            expected, this_col_count
                        )),
                );
            }
            _ => {}
        }

        let mut row_vec = Vec::new();
        for elem in this_row_elements {
            row_vec.push(lower_expr(walker, elem)?);
        }
        row_elements.push(row_vec);
    }

    let row_count = rows.len();
    let cols = col_count.unwrap_or(0);

    // Convert from row-major to column-major order
    // For [1 2 3; 4 5 6], row_elements = [[1,2,3], [4,5,6]]
    // column-major order: [1, 4, 2, 5, 3, 6] (column 0, column 1, column 2)
    let mut all_elements = Vec::with_capacity(row_count * cols);
    for col in 0..cols {
        for row in &row_elements {
            all_elements.push(row[col].clone());
        }
    }

    Ok(Expr::ArrayLiteral {
        elements: all_elements,
        shape: vec![row_count, cols],
        span,
    })
}

/// Lower index expression: arr[i] or arr[i, j]
/// Supports `end` keyword: arr[end] -> arr[lastindex(arr)] for 1D, arr[lastindex(arr, dim)] for nD
/// Supports `begin` keyword: arr[begin] -> arr[firstindex(arr)] for 1D, arr[firstindex(arr, dim)] for nD
/// (Issue #2310, Issue #2349)
pub fn lower_index_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("empty index".to_string()),
            span,
        ));
    }

    // First child is the array being indexed
    let array = lower_expr(walker, named[0])?;

    // Collect all index nodes first to determine total number of dimensions
    let mut all_index_nodes = Vec::new();
    for child in named.iter().skip(1) {
        for idx_node in collect_index_nodes(walker, *child) {
            all_index_nodes.push(idx_node);
        }
    }

    let total_indices = all_index_nodes.len();

    // Remaining children are indices (could be wrapped in vector/tuple/argument list)
    let mut indices = Vec::new();

    for (dim_index, idx_node) in all_index_nodes.into_iter().enumerate() {
        let idx_expr = lower_index_component(walker, idx_node)?;
        // Replace `end` with `lastindex(array)` or `lastindex(array, dim)` (Issue #2349)
        // Replace `begin` with `firstindex(array)` or `firstindex(array, dim)` (Issue #2349)
        // Use dimension-aware version when there are multiple indices
        let dim = if total_indices > 1 {
            Some(dim_index + 1) // Julia uses 1-based dimension indexing
        } else {
            None
        };
        let idx_expr = replace_end_with_lastindex(idx_expr, &array, dim);
        let idx_expr = replace_begin_with_firstindex(idx_expr, &array, dim);
        indices.push(idx_expr);
    }

    // Check for T[] syntax: type name with empty indices creates empty typed array
    if indices.is_empty() {
        // Check if the "array" is actually a type name
        let type_name = match &array {
            Expr::Var(name, _) => Some(name.clone()),
            // Handle parametric types like Complex{Float64}
            Expr::Call { function, args, .. } => {
                // Reconstruct the full type name from function(arg)
                if args.len() == 1 {
                    if let Expr::Var(param, _) = &args[0] {
                        Some(format!("{}{{{}}}", function, param))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        // Check if this is a known type (basic types or structs)
        if let Some(name) = type_name {
            let is_type = matches!(
                name.as_str(),
                "Int"
                    | "Int64"
                    | "Int32"
                    | "Float64"
                    | "Float32"
                    | "Bool"
                    | "String"
                    | "Char"
                    | "Any"
            ) || name.starts_with("Complex")
                || name.starts_with("Point")
                || name
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false); // Heuristic: capitalized names are types

            if is_type {
                return Ok(Expr::TypedEmptyArray {
                    element_type: name,
                    span,
                });
            }
        }
    }

    Ok(Expr::Index {
        array: Box::new(array),
        indices,
        span,
    })
}

/// Lower range expression: 1:10 or 1:2:10
/// Tree-sitter parses `1:2:10` as nested: (1:2):10
/// We need to flatten this to start=1, step=2, stop=10
pub fn lower_range_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Filter out operator nodes (the `:` operators)
    let operands: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if operands.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedRange,
            span,
        ));
    }

    if operands.len() == 2 {
        // Check if first operand is also a RangeExpression (nested case: (1:2):10)
        if walker.kind(&operands[0]) == NodeKind::RangeExpression {
            // Flatten: (start:step):stop
            let inner = walker.named_children(&operands[0]);
            let inner_operands: Vec<_> = inner
                .into_iter()
                .filter(|n| walker.kind(n) != NodeKind::Operator)
                .collect();

            if inner_operands.len() == 2 {
                let start = lower_expr(walker, inner_operands[0])?;
                let step = lower_expr(walker, inner_operands[1])?;
                let stop = lower_expr(walker, operands[1])?;
                return Ok(Expr::Range {
                    start: Box::new(start),
                    step: Some(Box::new(step)),
                    stop: Box::new(stop),
                    span,
                });
            }
        }

        // Simple range: start:stop
        let start = lower_expr(walker, operands[0])?;
        let stop = lower_expr(walker, operands[1])?;
        Ok(Expr::Range {
            start: Box::new(start),
            step: None,
            stop: Box::new(stop),
            span,
        })
    } else if operands.len() == 3 {
        // start:step:stop (direct case, if tree-sitter ever produces this)
        let start = lower_expr(walker, operands[0])?;
        let step = lower_expr(walker, operands[1])?;
        let stop = lower_expr(walker, operands[2])?;
        Ok(Expr::Range {
            start: Box::new(start),
            step: Some(Box::new(step)),
            stop: Box::new(stop),
            span,
        })
    } else {
        Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedRange,
            span,
        ))
    }
}

/// Extract array name from an index expression (for IndexAssign)
pub fn extract_index_target<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(String, Vec<Node<'a>>)> {
    let named = walker.named_children(&node);
    if named.is_empty() {
        return None;
    }

    // First child should be the array (identifier)
    let array_node = named[0];
    if walker.kind(&array_node) != NodeKind::Identifier {
        return None;
    }

    let array_name = walker.text(&array_node).to_string();

    // Get index nodes
    let mut index_nodes = Vec::new();
    for child in named.into_iter().skip(1) {
        index_nodes.extend(collect_index_nodes(walker, child));
    }

    Some((array_name, index_nodes))
}

fn collect_index_nodes<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Vec<Node<'a>> {
    match walker.kind(&node) {
        NodeKind::ArgumentList | NodeKind::TupleExpression | NodeKind::VectorExpression => {
            walker.named_children(&node)
        }
        _ => vec![node],
    }
}

fn lower_index_component<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    match walker.kind(&node) {
        NodeKind::Operator => {
            let text = walker.text(&node);
            if text == ":" {
                Ok(Expr::SliceAll {
                    span: walker.span(&node),
                })
            } else {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(format!("operator {}", text)),
                    walker.span(&node),
                ))
            }
        }
        _ => lower_expr(walker, node),
    }
}

/// Replace all occurrences of `end` identifier with `lastindex(array)` or `lastindex(array, dim)`.
/// This enables Julia's `arr[end]`, `arr[end-1]`, `arr[1:end]` syntax.
/// When `dim` is Some(d), uses dimension-aware `lastindex(array, d)` for multi-dimensional indexing (Issue #2349).
fn replace_end_with_lastindex(expr: Expr, array: &Expr, dim: Option<usize>) -> Expr {
    match expr {
        Expr::Var(ref name, span) if name == "end" => {
            // Replace `end` with `lastindex(array)` or `lastindex(array, dim)`
            let args = if let Some(d) = dim {
                vec![
                    array.clone(),
                    Expr::Literal(crate::ir::core::Literal::Int(d as i64), span),
                ]
            } else {
                vec![array.clone()]
            };
            Expr::Call {
                function: "lastindex".to_string(),
                args,
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }
        }
        // Recursively process binary operations (e.g., end-1, 1:end)
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Expr::BinaryOp {
            op,
            left: Box::new(replace_end_with_lastindex(*left, array, dim)),
            right: Box::new(replace_end_with_lastindex(*right, array, dim)),
            span,
        },
        // Recursively process unary operations (e.g., -end, though rare)
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op,
            operand: Box::new(replace_end_with_lastindex(*operand, array, dim)),
            span,
        },
        // Recursively process range expressions (e.g., 1:end, 1:2:end)
        Expr::Range {
            start,
            step,
            stop,
            span,
        } => Expr::Range {
            start: Box::new(replace_end_with_lastindex(*start, array, dim)),
            step: step.map(|e| Box::new(replace_end_with_lastindex(*e, array, dim))),
            stop: Box::new(replace_end_with_lastindex(*stop, array, dim)),
            span,
        },
        // Recursively process function calls (e.g., min(end, 5))
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Expr::Call {
            function,
            args: args
                .into_iter()
                .map(|a| replace_end_with_lastindex(a, array, dim))
                .collect(),
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        },
        // All other expressions pass through unchanged
        _ => expr,
    }
}

/// Replace all occurrences of `begin` identifier with `firstindex(array)` or `firstindex(array, dim)`.
/// This enables Julia's `arr[begin]`, `arr[begin+1]`, `arr[begin:end]` syntax (Issue #2310).
/// When `dim` is Some(d), uses dimension-aware `firstindex(array, d)` for multi-dimensional indexing (Issue #2349).
fn replace_begin_with_firstindex(expr: Expr, array: &Expr, dim: Option<usize>) -> Expr {
    match expr {
        Expr::Var(ref name, span) if name == "begin" => {
            // Replace `begin` with `firstindex(array)` or `firstindex(array, dim)`
            let args = if let Some(d) = dim {
                vec![
                    array.clone(),
                    Expr::Literal(crate::ir::core::Literal::Int(d as i64), span),
                ]
            } else {
                vec![array.clone()]
            };
            Expr::Call {
                function: "firstindex".to_string(),
                args,
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }
        }
        Expr::BinaryOp {
            op,
            left,
            right,
            span,
        } => Expr::BinaryOp {
            op,
            left: Box::new(replace_begin_with_firstindex(*left, array, dim)),
            right: Box::new(replace_begin_with_firstindex(*right, array, dim)),
            span,
        },
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op,
            operand: Box::new(replace_begin_with_firstindex(*operand, array, dim)),
            span,
        },
        Expr::Range {
            start,
            step,
            stop,
            span,
        } => Expr::Range {
            start: Box::new(replace_begin_with_firstindex(*start, array, dim)),
            step: step.map(|e| Box::new(replace_begin_with_firstindex(*e, array, dim))),
            stop: Box::new(replace_begin_with_firstindex(*stop, array, dim)),
            span,
        },
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } => Expr::Call {
            function,
            args: args
                .into_iter()
                .map(|a| replace_begin_with_firstindex(a, array, dim))
                .collect(),
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        },
        _ => expr,
    }
}

/// Lower comprehension expression: [x^2 for x in 1:10] or [x for x in arr if x > 0]
pub fn lower_comprehension_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty comprehension"),
        );
    }

    // Find the body expression, all for-clauses, and optional if-clause
    let mut body_expr = None;
    let mut for_clauses = Vec::new();
    let mut if_clause = None;

    for child in &named {
        match walker.kind(child) {
            NodeKind::ForClause => {
                for_clauses.push(*child);
            }
            NodeKind::IfClause => {
                if_clause = Some(*child);
            }
            _ => {
                if body_expr.is_none() {
                    body_expr = Some(*child);
                }
            }
        }
    }

    let body_node = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
            .with_hint("missing body expression")
    })?;

    if for_clauses.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("missing for clause"),
        );
    }

    // Parse body expression
    let body = lower_expr(walker, body_node)?;

    // Parse optional filter
    let filter = if let Some(if_node) = if_clause {
        Some(Box::new(parse_if_clause(walker, if_node)?))
    } else {
        None
    };

    // Collect ALL bindings across all ForClauses.
    // The parser may pack multiple comma-separated bindings (e.g. `for i in R, j in R`)
    // into a single ForClause with multiple ForBinding children.
    let mut all_bindings = Vec::new();
    for fc in &for_clauses {
        let bindings = parse_for_clause_bindings(walker, *fc)?;
        all_bindings.extend(bindings);
    }

    if all_bindings.len() == 1 {
        // Single-variable comprehension: use existing Comprehension IR
        let Some((var_name, iter_expr)) = all_bindings.pop() else {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                    .with_hint("missing for clause binding"),
            );
        };
        return Ok(Expr::Comprehension {
            body: Box::new(body),
            var: var_name,
            iter: Box::new(iter_expr),
            filter,
            span,
        });
    }

    // Multi-variable comprehension: use MultiComprehension IR (Issue #2143)
    Ok(Expr::MultiComprehension {
        body: Box::new(body),
        iterations: all_bindings,
        filter,
        span,
    })
}

/// Lower generator expression: (x^2 for x in 1:10) or (x for x in arr if x > 0)
/// Produces a lazy Generator that doesn't evaluate until iterated.
pub fn lower_generator_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty generator"),
        );
    }

    let mut body_expr = None;
    let mut for_clause = None;
    let mut if_clause = None;

    for child in &named {
        match walker.kind(child) {
            NodeKind::ForClause => {
                if for_clause.is_some() {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::Comprehension,
                        span,
                    )
                    .with_hint("nested generators not supported"));
                }
                for_clause = Some(*child);
            }
            NodeKind::IfClause => {
                if_clause = Some(*child);
            }
            _ => {
                if body_expr.is_none() {
                    body_expr = Some(*child);
                }
            }
        }
    }

    let body_node = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
            .with_hint("missing body expression")
    })?;

    let for_node = for_clause.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
            .with_hint("missing for clause")
    })?;

    let (var_name, iter_expr) = parse_for_clause(walker, for_node)?;
    let body = lower_expr(walker, body_node)?;
    let filter = if let Some(if_node) = if_clause {
        Some(Box::new(parse_if_clause(walker, if_node)?))
    } else {
        None
    };

    // Return Generator (lazy evaluation)
    Ok(Expr::Generator {
        body: Box::new(body),
        var: var_name,
        iter: Box::new(iter_expr),
        filter,
        span,
    })
}

/// Parse ALL bindings from a for clause.
/// A single ForClause may contain multiple ForBindings when comma-separated:
///   `for i in 1:3, j in 1:3` produces one ForClause with two ForBinding children.
fn parse_for_clause_bindings<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<(String, Expr)>> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Collect all ForBinding children
    let mut bindings = Vec::new();
    for child in &named {
        if walker.kind(child) == NodeKind::ForBinding {
            bindings.push(parse_for_binding(walker, *child)?);
        }
    }

    if !bindings.is_empty() {
        return Ok(bindings);
    }

    // Fallback: try to parse as a single binding from direct children
    let non_op: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if non_op.len() >= 2 {
        let var_node = non_op[0];
        let iter_node = non_op[1];

        if walker.kind(&var_node) != NodeKind::Identifier {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedForBinding,
                walker.span(&var_node),
            ));
        }

        let var_name = walker.text(&var_node).to_string();
        let iter_expr = lower_expr(walker, iter_node)?;
        return Ok(vec![(var_name, iter_expr)]);
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedForBinding,
        span,
    ))
}

/// Parse a for clause: "for x in range" or "for x = range"
fn parse_for_clause<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<(String, Expr)> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Look for for_binding or direct children
    for child in &named {
        if walker.kind(child) == NodeKind::ForBinding {
            return parse_for_binding(walker, *child);
        }
    }

    // Filter out operator nodes (like "in" or "=")
    let non_op: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    // Try to parse directly: should have identifier and range
    if non_op.len() >= 2 {
        let var_node = non_op[0];
        let iter_node = non_op[1];

        if walker.kind(&var_node) != NodeKind::Identifier {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedForBinding,
                walker.span(&var_node),
            ));
        }

        let var_name = walker.text(&var_node).to_string();
        let iter_expr = lower_expr(walker, iter_node)?;

        return Ok((var_name, iter_expr));
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedForBinding,
        span,
    ))
}

/// Parse a for binding: "x in range" or "x = range"
fn parse_for_binding<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<(String, Expr)> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Filter out operator nodes (like "in" or "=")
    let non_op: Vec<_> = named
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if non_op.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            span,
        ));
    }

    let var_node = non_op[0];
    let iter_node = non_op[1];

    if walker.kind(&var_node) != NodeKind::Identifier {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            walker.span(&var_node),
        ));
    }

    let var_name = walker.text(&var_node).to_string();
    let iter_expr = lower_expr(walker, iter_node)?;

    Ok((var_name, iter_expr))
}

/// Parse an if clause: "if condition"
fn parse_if_clause<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::Comprehension, span)
                .with_hint("empty if clause"),
        );
    }

    // The if clause contains just the condition expression
    lower_expr(walker, named[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Literal};
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string(), s())
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), s())
    }

    fn array_ref() -> Expr {
        Expr::Var("arr".to_string(), s())
    }

    // ── replace_end_with_lastindex ────────────────────────────────────────────

    #[test]
    fn test_replace_end_becomes_lastindex_no_dim() {
        let result = replace_end_with_lastindex(var("end"), &array_ref(), None);
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "lastindex" && args.len() == 1),
            "Expected lastindex(arr), got {:?}", result
        );
    }

    #[test]
    fn test_replace_end_with_dim_becomes_lastindex_with_dim() {
        let result = replace_end_with_lastindex(var("end"), &array_ref(), Some(2));
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "lastindex" && args.len() == 2),
            "Expected lastindex(arr, 2), got {:?}", result
        );
    }

    #[test]
    fn test_replace_end_in_binary_op() {
        // end - 1 → lastindex(arr) - 1
        let expr = Expr::BinaryOp {
            op: BinaryOp::Sub,
            left: Box::new(var("end")),
            right: Box::new(lit_int(1)),
            span: s(),
        };
        let result = replace_end_with_lastindex(expr, &array_ref(), None);
        assert!(matches!(result, Expr::BinaryOp { .. }), "Expected BinaryOp, got {:?}", result);
        if let Expr::BinaryOp { left, right, .. } = result {
            assert!(matches!(*left, Expr::Call { ref function, .. } if function == "lastindex"));
            assert!(matches!(*right, Expr::Literal(Literal::Int(1), _)));
        }
    }

    #[test]
    fn test_replace_end_non_end_var_passes_through() {
        // Var("x") is not "end" → unchanged
        let x = var("x");
        let result = replace_end_with_lastindex(x, &array_ref(), None);
        assert!(matches!(&result, Expr::Var(name, _) if name == "x"),
            "Expected Var(x), got {:?}", result);
    }

    #[test]
    fn test_replace_end_literal_passes_through() {
        let lit = lit_int(42);
        let result = replace_end_with_lastindex(lit, &array_ref(), None);
        assert!(matches!(result, Expr::Literal(Literal::Int(42), _)));
    }

    // ── replace_begin_with_firstindex ─────────────────────────────────────────

    #[test]
    fn test_replace_begin_becomes_firstindex_no_dim() {
        let result = replace_begin_with_firstindex(var("begin"), &array_ref(), None);
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "firstindex" && args.len() == 1),
            "Expected firstindex(arr), got {:?}", result
        );
    }

    #[test]
    fn test_replace_begin_with_dim_becomes_firstindex_with_dim() {
        let result = replace_begin_with_firstindex(var("begin"), &array_ref(), Some(1));
        assert!(
            matches!(&result, Expr::Call { function, args, .. } if function == "firstindex" && args.len() == 2),
            "Expected firstindex(arr, 1), got {:?}", result
        );
    }

    #[test]
    fn test_replace_begin_in_binary_op() {
        // begin + 1 → firstindex(arr) + 1
        let expr = Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(var("begin")),
            right: Box::new(lit_int(1)),
            span: s(),
        };
        let result = replace_begin_with_firstindex(expr, &array_ref(), None);
        assert!(matches!(result, Expr::BinaryOp { .. }), "Expected BinaryOp, got {:?}", result);
        if let Expr::BinaryOp { left, right, .. } = result {
            assert!(matches!(*left, Expr::Call { ref function, .. } if function == "firstindex"));
            assert!(matches!(*right, Expr::Literal(Literal::Int(1), _)));
        }
    }

    #[test]
    fn test_replace_begin_non_begin_var_passes_through() {
        let x = var("y");
        let result = replace_begin_with_firstindex(x, &array_ref(), None);
        assert!(matches!(&result, Expr::Var(name, _) if name == "y"),
            "Expected Var(y), got {:?}", result);
    }
}
