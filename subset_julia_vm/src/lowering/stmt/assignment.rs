//! Assignment statement lowering
//!
//! Handles:
//! - Simple assignments: `x = expr`
//! - Index assignments: `arr[i] = expr`
//! - Field assignments: `obj.field = expr`
//! - Multiple assignments: `a, b = expr` (tuple destructuring)
//! - Compound assignments: `x += expr`, `arr[i] *= expr`

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, Block, Expr, Stmt};
use crate::lowering::expr;
use crate::lowering::expr::{make_broadcasted_call, strip_broadcast_dot};
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

/// Counter for generating unique temporary variable names
static TEMP_VAR_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn generate_temp_var() -> String {
    let id = TEMP_VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__tuple_tmp_{}", id)
}

/// For `.=` lowering, avoid `materialize!(dest, materialize(Broadcasted(...)))`.
/// If RHS is `materialize(Broadcasted(...))`, pass inner `Broadcasted(...)` directly
/// so `materialize!` can run in-place without an intermediate materialized array.
fn strip_outer_materialize_broadcast(expr: Expr) -> Expr {
    match expr {
        Expr::Call {
            function,
            mut args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        } if function == "materialize" && args.len() == 1 => {
            if let Expr::Call {
                function: ref inner_function,
                ..
            } = args[0]
            {
                if inner_function == "Broadcasted" {
                    return args.remove(0);
                }
            }
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            }
        }
        other => other,
    }
}

pub fn lower_assignment<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);
    let op_text = extract_operator_text(walker, node).unwrap_or_else(|| "=".to_string());

    // Assignment node structure: lhs, = (operator, not named), rhs
    // We need at least lhs and rhs (the = operator is not named)
    if named.len() < 2 {
        // Try using all children (including unnamed) as fallback
        let all_children = walker.children(&node);
        if all_children.len() >= 3 {
            // Structure: lhs, =, rhs (or lhs, =, rhs, ... for chained assignments)
            // Find the = operator and split into lhs and rhs
            let mut lhs_idx = None;
            let mut rhs_idx = None;

            for (i, child) in all_children.iter().enumerate() {
                let kind = child.kind();
                if kind == "operator" && walker.text(child) == "=" && lhs_idx.is_none() {
                    lhs_idx = Some(i);
                    rhs_idx = Some(i + 1);
                    break;
                }
            }

            if let (Some(lhs_i), Some(rhs_i)) = (lhs_idx, rhs_idx) {
                if lhs_i > 0 && rhs_i < all_children.len() {
                    let lhs = &all_children[lhs_i - 1];
                    let rhs = &all_children[rhs_i];
                    return lower_assignment_parts(walker, *lhs, *rhs, span, &op_text);
                }
            }
        }

        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("assignment".to_string()),
            span,
        ));
    }

    let lhs = named[0];
    let rhs = named[named.len() - 1];

    lower_assignment_parts(walker, lhs, rhs, span, &op_text)
}

fn lower_assignment_parts<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    op_text: &str,
) -> LowerResult<Stmt> {
    match walker.kind(&lhs) {
        NodeKind::Identifier => {
            let var = walker.text(&lhs).to_string();
            let rhs_expr = expr::lower_expr(walker, rhs)?;
            let value = if op_text == ".=" {
                let rhs_expr = strip_outer_materialize_broadcast(rhs_expr);
                Expr::Call {
                    function: "materialize!".to_string(),
                    args: vec![Expr::Var(var.clone(), span), rhs_expr],
                    kwargs: Vec::new(),
                    splat_mask: vec![false, false],
                    kwargs_splat_mask: vec![],
                    span,
                }
            } else {
                rhs_expr
            };
            Ok(Stmt::Assign { var, value, span })
        }
        NodeKind::TypedExpression | NodeKind::TypedParameter => {
            // Typed local variable declaration: x::Type = value
            // Extract the variable name from the typed expression and ignore the type annotation
            // (Julia uses type annotations as hints, but the actual type is inferred at runtime)
            let var = extract_typed_var_name(walker, lhs).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(&lhs),
                )
                .with_hint("could not extract variable name from typed expression")
            })?;
            let value = expr::lower_expr(walker, rhs)?;
            Ok(Stmt::Assign { var, value, span })
        }
        NodeKind::IndexExpression => {
            // Array index assignment: arr[i] = x or arr[i, j] = x
            lower_index_assignment(walker, lhs, rhs, span)
        }
        NodeKind::FieldExpression => {
            // Field assignment: obj.field = value (for mutable structs)
            lower_field_assignment(walker, lhs, rhs, span)
        }
        NodeKind::TupleExpression => {
            // Multiple assignment: a, b = expr (tuple destructuring)
            lower_tuple_destructuring(walker, lhs, rhs, span)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )),
    }
}

/// Extract variable name from a typed expression like `x::Float64`
fn extract_typed_var_name<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    let named = walker.named_children(&node);
    // The first Identifier child is the variable name
    for child in named {
        if walker.kind(&child) == NodeKind::Identifier {
            return Some(walker.text(&child).to_string());
        }
    }
    None
}

/// Lower tuple destructuring assignment: `a, b = expr`
/// Expands to:
/// ```text
/// __tuple_tmp_N = expr
/// a = __tuple_tmp_N[1]
/// b = __tuple_tmp_N[2]
/// ```
fn lower_tuple_destructuring<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
) -> LowerResult<Stmt> {
    // Extract variable names from the tuple
    let elements = walker.named_children(&lhs);
    let mut var_names = Vec::new();

    for elem in &elements {
        match walker.kind(elem) {
            NodeKind::Identifier => {
                var_names.push(walker.text(elem).to_string());
            }
            _ => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(elem),
                )
                .with_hint("tuple destructuring only supports simple identifiers"));
            }
        }
    }

    if var_names.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
        .with_hint("empty tuple in destructuring assignment"));
    }

    // Lower the RHS expression
    let rhs_expr = expr::lower_expr(walker, rhs)?;

    // Generate a unique temporary variable name
    let temp_var = generate_temp_var();

    // Create statements
    let mut stmts = Vec::new();

    // First statement: __tuple_tmp_N = rhs
    stmts.push(Stmt::Assign {
        var: temp_var.clone(),
        value: rhs_expr,
        span,
    });

    // For each variable, create: var = __tuple_tmp_N[i]
    for (i, var_name) in var_names.iter().enumerate() {
        let index_expr = Expr::Index {
            array: Box::new(Expr::Var(temp_var.clone(), span)),
            indices: vec![Expr::Literal(
                crate::ir::core::Literal::Int((i + 1) as i64),
                span,
            )],
            span,
        };
        stmts.push(Stmt::Assign {
            var: var_name.clone(),
            value: index_expr,
            span,
        });
    }

    // Return a block containing all statements
    Ok(Stmt::Block(Block { stmts, span }))
}

fn lower_tuple_destructuring_with_ctx<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let elements = walker.named_children(&lhs);
    let mut var_names = Vec::new();

    for elem in &elements {
        match walker.kind(elem) {
            NodeKind::Identifier => {
                var_names.push(walker.text(elem).to_string());
            }
            _ => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(elem),
                )
                .with_hint("tuple destructuring only supports simple identifiers"));
            }
        }
    }

    if var_names.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
        .with_hint("empty tuple in destructuring assignment"));
    }

    let rhs_expr = expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;
    let temp_var = generate_temp_var();
    let mut stmts = Vec::new();

    stmts.push(Stmt::Assign {
        var: temp_var.clone(),
        value: rhs_expr,
        span,
    });

    for (i, var_name) in var_names.iter().enumerate() {
        let index_expr = Expr::Index {
            array: Box::new(Expr::Var(temp_var.clone(), span)),
            indices: vec![Expr::Literal(
                crate::ir::core::Literal::Int((i + 1) as i64),
                span,
            )],
            span,
        };
        stmts.push(Stmt::Assign {
            var: var_name.clone(),
            value: index_expr,
            span,
        });
    }

    Ok(Stmt::Block(Block { stmts, span }))
}

/// Counter for generating unique temporary variable names for nested field assignment
static FIELD_TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn generate_field_temp_var() -> String {
    let id = FIELD_TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__field_tmp_{}", id)
}

fn lower_field_assignment<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
) -> LowerResult<Stmt> {
    // Try simple case first: obj.field = value
    if let Some((object_name, field_name)) = expr::extract_field_target(walker, lhs) {
        let value = expr::lower_expr(walker, rhs)?;
        return Ok(Stmt::FieldAssign {
            object: object_name,
            field: field_name,
            value,
            span,
        });
    }

    // Handle nested field assignment: obj.inner.field = value
    // Decompose into: __field_tmp = obj.inner; __field_tmp.field = value
    let (object_expr, field_name) =
        expr::extract_nested_field_target(walker, lhs).ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            )
            .with_hint("field assignment requires variable.field form")
        })?;

    let temp_var = generate_field_temp_var();
    let value = expr::lower_expr(walker, rhs)?;

    let stmts = vec![
        Stmt::Assign {
            var: temp_var.clone(),
            value: object_expr,
            span,
        },
        Stmt::FieldAssign {
            object: temp_var,
            field: field_name,
            value,
            span,
        },
    ];

    Ok(Stmt::Block(Block { stmts, span }))
}

fn lower_index_assignment<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
) -> LowerResult<Stmt> {
    let (array_name, index_nodes) = expr::extract_index_target(walker, lhs).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
    })?;

    // Extract indices
    let mut indices = Vec::new();
    for idx_node in index_nodes {
        match walker.kind(&idx_node) {
            NodeKind::RangeExpression => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::ArraySlicing,
                    walker.span(&idx_node),
                ))
            }
            NodeKind::Operator => {
                let text = walker.text(&idx_node);
                if text == ":" {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::ArraySlicing,
                        walker.span(&idx_node),
                    ));
                }
                indices.push(expr::lower_expr(walker, idx_node)?);
            }
            _ => indices.push(expr::lower_expr(walker, idx_node)?),
        }
    }

    let value = expr::lower_expr(walker, rhs)?;

    Ok(Stmt::IndexAssign {
        array: array_name,
        indices,
        value,
        span,
    })
}

pub fn lower_compound_assignment<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);
    if named.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("compound assignment".to_string()),
            span,
        ));
    }

    let lhs = named[0];
    let rhs = named[named.len() - 1];

    let op_text = extract_operator_text(walker, node).unwrap_or_else(|| "?".to_string());
    let rhs_expr = expr::lower_expr(walker, rhs)?;

    // Determine the binary operation from the compound assignment operator
    let binary_op = match op_text.as_str() {
        "+=" => Some(BinaryOp::Add),
        "-=" => Some(BinaryOp::Sub),
        "*=" => Some(BinaryOp::Mul),
        "/=" => Some(BinaryOp::Div),
        "^=" => Some(BinaryOp::Pow),
        "%=" => Some(BinaryOp::Mod),
        "÷=" => Some(BinaryOp::IntDiv),
        _ => None,
    };

    // Handle IndexExpression on LHS: arr[i] += x
    if walker.kind(&lhs) == NodeKind::IndexExpression {
        if let Some(op) = binary_op {
            // Extract array name and indices
            let (array_name, index_nodes) =
                expr::extract_index_target(walker, lhs).ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                        walker.span(&lhs),
                    )
                })?;

            // Lower all index expressions
            let mut indices = Vec::new();
            for idx_node in &index_nodes {
                indices.push(expr::lower_expr(walker, *idx_node)?);
            }

            // Create the index expression for the current value: arr[i]
            let current_value = Expr::Index {
                array: Box::new(Expr::Var(array_name.clone(), span)),
                indices: indices.clone(),
                span,
            };

            // Create the binary operation: arr[i] op rhs
            let new_value = Expr::BinaryOp {
                op,
                left: Box::new(current_value),
                right: Box::new(rhs_expr),
                span,
            };

            // Return IndexAssign: arr[i] = arr[i] op rhs
            return Ok(Stmt::IndexAssign {
                array: array_name,
                indices,
                value: new_value,
                span,
            });
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedOperator(op_text),
                span,
            ));
        }
    }

    // Handle FieldExpression on LHS: obj.field += x (Issue #2140)
    // Also handles nested field expressions: obj.inner.field += x (Issue #2309)
    if walker.kind(&lhs) == NodeKind::FieldExpression {
        if let Some(op) = binary_op {
            // Try simple case first: obj.field += x
            if let Some((object_name, field_name)) = expr::extract_field_target(walker, lhs) {
                let current_value = Expr::FieldAccess {
                    object: Box::new(Expr::Var(object_name.clone(), span)),
                    field: field_name.clone(),
                    span,
                };

                let new_value = Expr::BinaryOp {
                    op,
                    left: Box::new(current_value),
                    right: Box::new(rhs_expr),
                    span,
                };

                return Ok(Stmt::FieldAssign {
                    object: object_name,
                    field: field_name,
                    value: new_value,
                    span,
                });
            }

            // Handle nested case: obj.inner.field += x (Issue #2309)
            if let Some((object_expr, field_name)) = expr::extract_nested_field_target(walker, lhs)
            {
                let temp_var = generate_field_temp_var();

                let current_value = Expr::FieldAccess {
                    object: Box::new(Expr::Var(temp_var.clone(), span)),
                    field: field_name.clone(),
                    span,
                };

                let new_value = Expr::BinaryOp {
                    op,
                    left: Box::new(current_value),
                    right: Box::new(rhs_expr),
                    span,
                };

                let stmts = vec![
                    Stmt::Assign {
                        var: temp_var.clone(),
                        value: object_expr,
                        span,
                    },
                    Stmt::FieldAssign {
                        object: temp_var,
                        field: field_name,
                        value: new_value,
                        span,
                    },
                ];

                return Ok(Stmt::Block(Block { stmts, span }));
            }

            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            ));
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedOperator(op_text),
                span,
            ));
        }
    }

    // Handle simple variable LHS: x += val
    let var = match walker.kind(&lhs) {
        NodeKind::Identifier => walker.text(&lhs).to_string(),
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            ))
        }
    };

    // Convert compound assignment to binary operation
    // x op= val → x = x op val
    if let Some(op) = binary_op {
        let var_expr = Expr::Var(var.clone(), span);
        let value = Expr::BinaryOp {
            op,
            left: Box::new(var_expr),
            right: Box::new(rhs_expr),
            span,
        };
        return Ok(Stmt::Assign { var, value, span });
    }

    // Handle broadcast assignment (.=)
    // Z .= expr lowers to Z = materialize!(Z, expr) so alias-observable in-place semantics
    // are preserved.
    if op_text == ".=" {
        let var_expr = Expr::Var(var.clone(), span);
        let value = Expr::Call {
            function: "materialize!".to_string(),
            args: vec![var_expr, strip_outer_materialize_broadcast(rhs_expr)],
            kwargs: Vec::new(),
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span,
        };
        return Ok(Stmt::Assign {
            var,
            value,
            span,
        });
    }

    // Handle broadcast compound assignments (.+=, .-=, .*=, .&=, etc.)
    let broadcast_op = match op_text.as_str() {
        ".+=" => Some(".+"),
        ".-=" => Some(".-"),
        ".*=" => Some(".*"),
        "./=" => Some("./"),
        ".^=" => Some(".^"),
        ".&=" => Some(".&"),
        ".|=" => Some(".|"),
        _ => None,
    };

    if let Some(op_name) = broadcast_op {
        let var_expr = Expr::Var(var.clone(), span);
        // Use make_broadcasted_call to create materialize(Broadcasted(op, (var, rhs)))
        // instead of calling ".+" directly, which is not a registered function (Issue #2685)
        let base_op = strip_broadcast_dot(op_name);
        let value = make_broadcasted_call(base_op, vec![var_expr, rhs_expr], span);
        return Ok(Stmt::Assign { var, value, span });
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedOperator(op_text),
        span,
    ))
}

pub(crate) fn extract_operator_text<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    for child in walker.children(&node) {
        let kind = child.kind();
        if kind == "operator" || kind.ends_with('=') {
            return Some(walker.text(&child).to_string());
        }
    }
    None
}

// ==================== Lambda Context Versions ====================

pub fn lower_assignment_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);
    let op_text = extract_operator_text(walker, node).unwrap_or_else(|| "=".to_string());
    if named.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("assignment".to_string()),
            span,
        ));
    }

    let lhs = named[0];
    let rhs = named[named.len() - 1];

    match walker.kind(&lhs) {
        NodeKind::Identifier => {
            let var = walker.text(&lhs).to_string();
            let rhs_expr = expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;
            let value = if op_text == ".=" {
                let rhs_expr = strip_outer_materialize_broadcast(rhs_expr);
                Expr::Call {
                    function: "materialize!".to_string(),
                    args: vec![Expr::Var(var.clone(), span), rhs_expr],
                    kwargs: Vec::new(),
                    splat_mask: vec![false, false],
                    kwargs_splat_mask: vec![],
                    span,
                }
            } else {
                rhs_expr
            };
            Ok(Stmt::Assign { var, value, span })
        }
        NodeKind::TypedExpression | NodeKind::TypedParameter => {
            // Typed local variable declaration: x::Type = value
            let var = extract_typed_var_name(walker, lhs).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(&lhs),
                )
                .with_hint("could not extract variable name from typed expression")
            })?;
            let value = expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;
            Ok(Stmt::Assign { var, value, span })
        }
        NodeKind::IndexExpression => {
            lower_index_assignment_with_ctx(walker, lhs, rhs, span, lambda_ctx)
        }
        NodeKind::FieldExpression => {
            lower_field_assignment_with_ctx(walker, lhs, rhs, span, lambda_ctx)
        }
        NodeKind::TupleExpression => {
            lower_tuple_destructuring_with_ctx(walker, lhs, rhs, span, lambda_ctx)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )),
    }
}

fn lower_field_assignment_with_ctx<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Try simple case first: obj.field = value
    if let Some((object_name, field_name)) = expr::extract_field_target(walker, lhs) {
        let value = expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;
        return Ok(Stmt::FieldAssign {
            object: object_name,
            field: field_name,
            value,
            span,
        });
    }

    // Handle nested field assignment: obj.inner.field = value (Issue #2309)
    let (object_expr, field_name) =
        expr::extract_nested_field_target_with_ctx(walker, lhs, lambda_ctx).ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            )
            .with_hint("field assignment requires variable.field form")
        })?;

    let temp_var = generate_field_temp_var();
    let value = expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;

    let stmts = vec![
        Stmt::Assign {
            var: temp_var.clone(),
            value: object_expr,
            span,
        },
        Stmt::FieldAssign {
            object: temp_var,
            field: field_name,
            value,
            span,
        },
    ];

    Ok(Stmt::Block(Block { stmts, span }))
}

fn lower_index_assignment_with_ctx<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let (array_name, index_nodes) = expr::extract_index_target(walker, lhs).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
    })?;

    let mut indices = Vec::new();
    for idx_node in index_nodes {
        match walker.kind(&idx_node) {
            NodeKind::RangeExpression => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::ArraySlicing,
                    walker.span(&idx_node),
                ))
            }
            NodeKind::Operator => {
                let text = walker.text(&idx_node);
                if text == ":" {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::ArraySlicing,
                        walker.span(&idx_node),
                    ));
                }
                indices.push(expr::lower_expr_with_ctx(walker, idx_node, lambda_ctx)?);
            }
            _ => indices.push(expr::lower_expr_with_ctx(walker, idx_node, lambda_ctx)?),
        }
    }

    let value = expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;

    Ok(Stmt::IndexAssign {
        array: array_name,
        indices,
        value,
        span,
    })
}

pub fn lower_compound_assignment_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);
    if named.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("compound assignment".to_string()),
            span,
        ));
    }

    let lhs = named[0];
    let rhs = named[named.len() - 1];

    let op_text = extract_operator_text(walker, node).unwrap_or_else(|| "?".to_string());
    let rhs_expr = expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;

    // Determine the binary operation from the compound assignment operator
    let binary_op = match op_text.as_str() {
        "+=" => Some(BinaryOp::Add),
        "-=" => Some(BinaryOp::Sub),
        "*=" => Some(BinaryOp::Mul),
        "/=" => Some(BinaryOp::Div),
        "^=" => Some(BinaryOp::Pow),
        "%=" => Some(BinaryOp::Mod),
        "÷=" => Some(BinaryOp::IntDiv),
        _ => None,
    };

    // Handle IndexExpression on LHS: arr[i] += x
    if walker.kind(&lhs) == NodeKind::IndexExpression {
        if let Some(op) = binary_op {
            // Extract array name and indices
            let (array_name, index_nodes) =
                expr::extract_index_target(walker, lhs).ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                        walker.span(&lhs),
                    )
                })?;

            // Lower all index expressions
            let mut indices = Vec::new();
            for idx_node in &index_nodes {
                indices.push(expr::lower_expr_with_ctx(walker, *idx_node, lambda_ctx)?);
            }

            // Create the index expression for the current value: arr[i]
            let current_value = Expr::Index {
                array: Box::new(Expr::Var(array_name.clone(), span)),
                indices: indices.clone(),
                span,
            };

            // Create the binary operation: arr[i] op rhs
            let new_value = Expr::BinaryOp {
                op,
                left: Box::new(current_value),
                right: Box::new(rhs_expr),
                span,
            };

            // Return IndexAssign: arr[i] = arr[i] op rhs
            return Ok(Stmt::IndexAssign {
                array: array_name,
                indices,
                value: new_value,
                span,
            });
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedOperator(op_text),
                span,
            ));
        }
    }

    // Handle FieldExpression on LHS: obj.field += x (Issue #2140)
    // Also handles nested field expressions: obj.inner.field += x (Issue #2309)
    if walker.kind(&lhs) == NodeKind::FieldExpression {
        if let Some(op) = binary_op {
            // Try simple case first: obj.field += x
            if let Some((object_name, field_name)) = expr::extract_field_target(walker, lhs) {
                let current_value = Expr::FieldAccess {
                    object: Box::new(Expr::Var(object_name.clone(), span)),
                    field: field_name.clone(),
                    span,
                };

                let new_value = Expr::BinaryOp {
                    op,
                    left: Box::new(current_value),
                    right: Box::new(rhs_expr),
                    span,
                };

                return Ok(Stmt::FieldAssign {
                    object: object_name,
                    field: field_name,
                    value: new_value,
                    span,
                });
            }

            // Handle nested case: obj.inner.field += x (Issue #2309)
            if let Some((object_expr, field_name)) = expr::extract_nested_field_target(walker, lhs)
            {
                let temp_var = generate_field_temp_var();

                let current_value = Expr::FieldAccess {
                    object: Box::new(Expr::Var(temp_var.clone(), span)),
                    field: field_name.clone(),
                    span,
                };

                let new_value = Expr::BinaryOp {
                    op,
                    left: Box::new(current_value),
                    right: Box::new(rhs_expr),
                    span,
                };

                let stmts = vec![
                    Stmt::Assign {
                        var: temp_var.clone(),
                        value: object_expr,
                        span,
                    },
                    Stmt::FieldAssign {
                        object: temp_var,
                        field: field_name,
                        value: new_value,
                        span,
                    },
                ];

                return Ok(Stmt::Block(Block { stmts, span }));
            }

            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            ));
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedOperator(op_text),
                span,
            ));
        }
    }

    // Handle simple variable LHS: x += val
    let var = match walker.kind(&lhs) {
        NodeKind::Identifier => walker.text(&lhs).to_string(),
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            ))
        }
    };

    // Convert compound assignment to binary operation
    // x op= val → x = x op val
    if let Some(op) = binary_op {
        let var_expr = Expr::Var(var.clone(), span);
        let value = Expr::BinaryOp {
            op,
            left: Box::new(var_expr),
            right: Box::new(rhs_expr),
            span,
        };
        return Ok(Stmt::Assign { var, value, span });
    }

    // Handle broadcast assignment (.=)
    // Z .= expr lowers to Z = materialize!(Z, expr) so alias-observable in-place semantics
    // are preserved.
    if op_text == ".=" {
        let var_expr = Expr::Var(var.clone(), span);
        let value = Expr::Call {
            function: "materialize!".to_string(),
            args: vec![var_expr, strip_outer_materialize_broadcast(rhs_expr)],
            kwargs: Vec::new(),
            splat_mask: vec![false, false],
            kwargs_splat_mask: vec![],
            span,
        };
        return Ok(Stmt::Assign {
            var,
            value,
            span,
        });
    }

    // Handle broadcast compound assignments (.+=, .-=, .*=, .&=, etc.)
    let broadcast_op = match op_text.as_str() {
        ".+=" => Some(".+"),
        ".-=" => Some(".-"),
        ".*=" => Some(".*"),
        "./=" => Some("./"),
        ".^=" => Some(".^"),
        ".&=" => Some(".&"),
        ".|=" => Some(".|"),
        _ => None,
    };

    if let Some(op_name) = broadcast_op {
        let var_expr = Expr::Var(var.clone(), span);
        // Use make_broadcasted_call to create materialize(Broadcasted(op, (var, rhs)))
        // instead of calling ".+" directly, which is not a registered function (Issue #2685)
        let base_op = strip_broadcast_dot(op_name);
        let value = make_broadcasted_call(base_op, vec![var_expr, rhs_expr], span);
        return Ok(Stmt::Assign { var, value, span });
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedOperator(op_text),
        span,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::Literal;
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn make_broadcasted(args: Vec<Expr>) -> Expr {
        let n = args.len();
        Expr::Call {
            function: "Broadcasted".to_string(),
            args,
            kwargs: vec![],
            splat_mask: vec![false; n],
            kwargs_splat_mask: vec![],
            span: s(),
        }
    }

    fn make_materialize(inner: Expr) -> Expr {
        Expr::Call {
            function: "materialize".to_string(),
            args: vec![inner],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: s(),
        }
    }

    fn lit_int(v: i64) -> Expr {
        Expr::Literal(Literal::Int(v), s())
    }

    // ── strip_outer_materialize_broadcast ─────────────────────────────────────

    #[test]
    fn test_strip_materialize_containing_broadcasted_strips_outer() {
        // materialize(Broadcasted(...)) → Broadcasted(...)
        let broadcasted = make_broadcasted(vec![lit_int(1)]);
        let mat = make_materialize(broadcasted);
        let result = strip_outer_materialize_broadcast(mat);
        assert!(
            matches!(&result, Expr::Call { function, .. } if function == "Broadcasted"),
            "Expected inner Broadcasted, got {:?}",
            result
        );
    }

    #[test]
    fn test_strip_materialize_not_containing_broadcasted_unchanged() {
        // materialize(other_call) → unchanged
        let inner = Expr::Call {
            function: "other".to_string(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: s(),
        };
        let mat = make_materialize(inner);
        let result = strip_outer_materialize_broadcast(mat);
        assert!(
            matches!(&result, Expr::Call { function, .. } if function == "materialize"),
            "Expected materialize unchanged, got {:?}",
            result
        );
    }

    #[test]
    fn test_strip_non_materialize_call_passes_through() {
        // foo(42) → unchanged
        let call = Expr::Call {
            function: "foo".to_string(),
            args: vec![lit_int(42)],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: s(),
        };
        let result = strip_outer_materialize_broadcast(call);
        assert!(
            matches!(&result, Expr::Call { function, .. } if function == "foo"),
            "Expected foo unchanged, got {:?}",
            result
        );
    }

    #[test]
    fn test_strip_literal_passes_through() {
        // Literals are passed through unchanged
        let result = strip_outer_materialize_broadcast(lit_int(99));
        assert!(
            matches!(result, Expr::Literal(Literal::Int(99), _)),
            "Expected Literal::Int(99) unchanged"
        );
    }
}
