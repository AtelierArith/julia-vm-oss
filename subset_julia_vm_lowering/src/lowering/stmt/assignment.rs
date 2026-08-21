//! Assignment statement lowering
//!
//! Handles:
//! - Simple assignments: `x = expr`
//! - Index assignments: `arr[i] = expr`
//! - Field assignments: `obj.field = expr`
//! - Multiple assignments: `a, b = expr` (tuple destructuring)
//! - Compound assignments: `x += expr`, `arr[i] *= expr`

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

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
const TUPLE_TAIL_FUNCTION: &str = "#__sjulia_tuple_tail__";

fn generate_temp_var() -> String {
    let id = TEMP_VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__tuple_tmp_{}", id)
}

#[derive(Debug, Clone)]
pub enum DestructureTarget {
    Identifier(String),
    Tuple(Vec<DestructureTarget>),
    Rest(String),
    Index { array: Expr, indices: Vec<Expr> },
    Field { object: Expr, field: String },
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

fn broadcast_assignment_call(dest: Expr, rhs_expr: Expr, span: Span) -> Expr {
    Expr::Call {
        function: "materialize!".to_string().into(),
        args: vec![dest, strip_outer_materialize_broadcast(rhs_expr)],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    }
}

fn broadcast_compound_assignment_call(
    op_name: &str,
    dest: Expr,
    rhs_expr: Expr,
    span: Span,
) -> Expr {
    let base_op = strip_broadcast_dot(op_name);
    let broadcasted = make_broadcasted_call(base_op, vec![dest.clone(), rhs_expr], span);
    broadcast_assignment_call(dest, broadcasted, span)
}

fn compound_assignment_value(op_text: &str, left: Expr, right: Expr, span: Span) -> Option<Expr> {
    let binary_op = match op_text {
        "+=" => Some(BinaryOp::Add),
        "-=" => Some(BinaryOp::Sub),
        "*=" => Some(BinaryOp::Mul),
        "/=" => Some(BinaryOp::Div),
        "^=" => Some(BinaryOp::Pow),
        "%=" => Some(BinaryOp::Mod),
        "÷=" => Some(BinaryOp::IntDiv),
        _ => None,
    };
    if let Some(op) = binary_op {
        return Some(Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
            span,
        });
    }

    let function = match op_text {
        "&=" => "&",
        "|=" => "|",
        "⊻=" => "⊻",
        "<<=" => "<<",
        ">>=" => ">>",
        ">>>=" => ">>>",
        _ => return None,
    };
    Some(Expr::Call {
        function: function.to_string().into(),
        args: vec![left, right],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span,
    })
}

fn is_compound_assignment_operator(op_text: &str) -> bool {
    matches!(
        op_text,
        "+=" | "-="
            | "*="
            | "/="
            | "^="
            | "%="
            | "÷="
            | "&="
            | "|="
            | "⊻="
            | "<<="
            | ">>="
            | ">>>="
    )
}

pub fn lower_assignment<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    lower_assignment_impl(walker, node, None)
}

fn lower_assignment_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);
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
                    return lower_assignment_parts(walker, *lhs, *rhs, span, &op_text, lambda_ctx);
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

    lower_assignment_parts(walker, lhs, rhs, span, &op_text, lambda_ctx)
}

fn lower_assignment_parts<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    op_text: &str,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match walker.kind(&lhs) {
        NodeKind::Identifier => {
            let var = walker.text(&lhs).to_string();
            if op_text == "=" {
                if let Some(block) = begin_rhs_block(walker, rhs) {
                    return lower_identifier_begin_rhs_assignment(
                        walker, var, block, span, lambda_ctx,
                    );
                }
            }
            let rhs_expr = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;
            let value = if op_text == ".=" {
                broadcast_assignment_call(Expr::Var(var.clone().into(), span), rhs_expr, span)
            } else {
                rhs_expr
            };
            Ok(Stmt::Assign { var, value, span })
        }
        NodeKind::TypedExpression | NodeKind::TypedParameter => {
            // Typed variable declaration: `x::T = value` (also `global x::T = value`).
            // Julia lowers this to `x = convert(T, value)::T`, i.e. the RHS is run
            // through `convert(T, value)` on assignment. Mirror that here by wrapping
            // the RHS in a `convert(T, rhs)` call so the value is coerced to the
            // declared type (and errors — InexactError, MethodError — surface exactly
            // as upstream when the conversion is invalid). Issue #5148.
            let var = extract_typed_var_name(walker, lhs).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(&lhs),
                )
                .with_hint("could not extract variable name from typed expression")
            })?;
            let type_node = extract_typed_type_node(walker, lhs).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(&lhs),
                )
                .with_hint("could not extract type annotation from typed expression")
            })?;
            let rhs_expr = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;
            let type_expr = lower_expr_maybe_ctx(walker, type_node, lambda_ctx)?;
            let value = Expr::Call {
                function: "convert".to_string().into(),
                args: vec![type_expr, rhs_expr],
                kwargs: Vec::new(),
                splat_mask: vec![false, false],
                kwargs_splat_mask: Vec::new(),
                span,
            };
            Ok(Stmt::Assign { var, value, span })
        }
        NodeKind::IndexExpression => {
            // Array index assignment: arr[i] = x or arr[i, j] = x
            lower_index_assignment_impl(walker, lhs, rhs, span, op_text, lambda_ctx)
        }
        NodeKind::FieldExpression => {
            // Field assignment: obj.field = value (for mutable structs)
            lower_field_assignment_impl(walker, lhs, rhs, span, lambda_ctx)
        }
        NodeKind::TupleExpression => {
            // Multiple assignment: a, b = expr (tuple destructuring)
            lower_tuple_destructuring_impl(walker, lhs, rhs, span, lambda_ctx)
        }
        NodeKind::ParametrizedTypeExpression => {
            // Plain-assignment parametric type alias whose RHS is not a flat
            // type template that the compile-time alias pre-scan
            // (`try_extract_type_alias_from_assignment`) can register — most
            // notably a `where`-clause RHS, e.g.
            // `MyVec{T} = Vector{T} where T<:Real` (Issue #10372). The
            // pre-scan deliberately declines a `where` RHS (Issue #5053: it
            // is a runtime `UnionAll` value, not a static string alias), so
            // any `Assignment` with a `ParametrizedTypeExpression` LHS that
            // still reaches ordinary statement lowering needs a real runtime
            // binding here instead of the `UnsupportedAssignmentTarget`
            // fallback below.
            lower_parametric_alias_value_assignment(walker, lhs, rhs, span, lambda_ctx)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )),
    }
}

/// Lower `Name{P1,...,Pn} = RHS` as a genuine runtime assignment, mirroring
/// upstream Julia's own desugaring of a parametric alias definition
/// (`Meta.lower` on `Name{T} = Body` shows `const Name = UnionAll(TypeVar(:T),
/// <lowered Body>)`): wrap the ordinarily-lowered RHS value expression in one
/// `UnionAll(TypeVar(:P), ...)` call per LHS parameter, outermost-first so the
/// leftmost LHS parameter becomes the outermost binder (Issue #10372).
///
/// This composes with the existing value-position `where`-expression lowering
/// (`lower_where_expression_value`) used for the RHS, so `RHS = Body where
/// T<:Real` builds its own nested `UnionAll` first; the LHS parameter then
/// wraps that as an additional (often unused, and therefore collapsed at
/// runtime by the `UnionAll` builtin — see `julia_type_references_typevar` in
/// `vm/builtins_types.rs`) outer binder, exactly matching upstream.
fn lower_parametric_alias_value_assignment<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let Some((name, params)) = crate::lowering::stmt::extract_parametric_alias_lhs(walker, lhs)
    else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
        .with_hint("could not extract a parametric alias name/params from the assignment target"));
    };

    let rhs_expr = if walker.kind(&rhs) == NodeKind::WhereExpression {
        lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?
    } else {
        // A flat alias body is a type template, not a runtime read of its type
        // parameters. Preserve `T` inside `Vector{T}` as type syntax; lowering
        // it as an ordinary curly expression would emit `LoadGlobal T` before
        // the surrounding UnionAll binder exists (Issue #10501).
        let target = crate::lowering::type_alias::expand_excluding(walker.text(&rhs), &params);
        Expr::Literal(
            crate::ir::core::Literal::DataType(target),
            walker.span(&rhs),
        )
    };
    let mut value = rhs_expr;
    for pname in params.iter().rev() {
        let typevar_expr = Expr::Call {
            function: "TypeVar".to_string().into(),
            args: vec![Expr::Literal(
                crate::ir::core::Literal::Symbol(pname.clone()),
                span,
            )],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
        value = Expr::Call {
            function: "UnionAll".to_string().into(),
            args: vec![typevar_expr, value],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
    }
    Ok(Stmt::Assign {
        var: name,
        value,
        span,
    })
}

fn begin_rhs_block<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<Node<'a>> {
    if walker.kind(&node) != NodeKind::Block {
        return None;
    }
    if node.kind() == "begin_block" {
        walker
            .named_children(&node)
            .find(|child| walker.kind(child) == NodeKind::Block)
    } else {
        Some(node)
    }
}

fn lower_identifier_begin_rhs_assignment<'a>(
    walker: &CstWalker<'a>,
    var: String,
    block: Node<'a>,
    span: Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    // `x = begin ... end` is still a transparent wrapper (Issue #10194): a
    // nested `struct`/`mutable struct` definition ahead of the tail value is
    // legal here whenever this assignment is itself reachable from top
    // level, matching upstream Julia.
    let mut stmts =
        super::lower_transparent_block_stmts(walker, walker.named_children(&block), lambda_ctx)?;
    append_begin_rhs_assignment(&mut stmts, var, span);
    Ok(Stmt::Block(Block { stmts, span }))
}

fn append_begin_rhs_assignment(stmts: &mut Vec<Stmt>, target: String, span: Span) {
    let Some(last) = stmts.pop() else {
        stmts.push(Stmt::Assign {
            var: target,
            value: Expr::Literal(crate::ir::core::Literal::Nothing, span),
            span,
        });
        return;
    };

    match last {
        Stmt::Expr { expr, .. } => {
            stmts.push(Stmt::Assign {
                var: target,
                value: expr,
                span,
            });
        }
        Stmt::Assign {
            var,
            value,
            span: assign_span,
        } => {
            stmts.push(Stmt::Assign {
                var: target,
                value: Expr::AssignExpr {
                    var: var.into(),
                    value: Box::new(value),
                    span: assign_span,
                },
                span,
            });
        }
        Stmt::Block(block) => {
            stmts.extend(block.stmts);
            append_begin_rhs_assignment(stmts, target, span);
        }
        // A trailing control-flow statement in value position yields the value of
        // whichever branch executed (upstream "the last statement of a block is
        // its value"). `x = begin …; if c; a; else; b; end; end` and the
        // `try`/`catch` form must assign the branch value, not `nothing`
        // (Issue #9358). Convert the tail to a self-contained value expression
        // (`if_stmt_into_value_expr` / `try_stmt_into_value_expr`) and assign it,
        // rather than running the statement for effect and defaulting to
        // `nothing`. Loops and other statements keep the `nothing` default below.
        last @ Stmt::If { .. } => {
            let stmt_span = last.span();
            match expr::if_stmt_into_value_expr(last, stmt_span) {
                Some(value) => stmts.push(Stmt::Assign {
                    var: target,
                    value,
                    span,
                }),
                None => unreachable!("Stmt::If did not convert to a value expression"),
            }
        }
        last @ Stmt::Try { .. } => {
            let stmt_span = last.span();
            match expr::try_stmt_into_value_expr(last, stmt_span) {
                Some(value) => stmts.push(Stmt::Assign {
                    var: target,
                    value,
                    span,
                }),
                None => unreachable!("Stmt::Try did not convert to a value expression"),
            }
        }
        other => {
            stmts.push(other);
            stmts.push(Stmt::Assign {
                var: target,
                value: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            });
        }
    }
}

/// Extract variable name from a typed expression like `x::Float64`
fn extract_typed_var_name<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    let named = walker.named_children_vec(&node);
    // The first Identifier child is the variable name
    for child in named {
        if walker.kind(&child) == NodeKind::Identifier {
            return Some(walker.text(&child).to_string());
        }
    }
    None
}

/// Extract the type-annotation node from a typed expression like `x::Float64`
/// or `x::Vector{Int}`. The CST orders the named children as `[var, type]`, so
/// the type node is the last named child (the variable name is the first). Used
/// to wrap the RHS of a typed assignment in `convert(T, rhs)` (Issue #5148).
fn extract_typed_type_node<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<Node<'a>> {
    let named = walker.named_children_vec(&node);
    if named.len() < 2 {
        return None;
    }
    named.last().copied()
}

/// Lower tuple destructuring assignment: `a, b = expr`
/// Expands to:
/// ```text
/// __tuple_tmp_N = expr
/// a = __tuple_tmp_N[1]
/// b = __tuple_tmp_N[2]
/// ```
fn lower_tuple_destructuring_impl<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let elements = walker.named_children_vec(&lhs);
    let patterns = parse_destructure_targets(walker, &elements, lambda_ctx)?;

    if patterns.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
        .with_hint("empty tuple in destructuring assignment"));
    }
    validate_rest_targets(&patterns, walker.span(&lhs))?;
    let flat_var_names = flat_destructure_identifiers(&patterns);

    let rhs_expr = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;
    if let (Some(var_names), Expr::TupleLiteral { elements, .. }) =
        (flat_var_names.as_ref(), &rhs_expr)
    {
        if elements.len() == var_names.len() {
            if elements
                .iter()
                .all(|expr| !expr_references_any_name(expr, var_names))
            {
                // Preserve the assignment's identity in IR. The compiler keeps
                // the ordinary non-tail path allocation-free while a tail use
                // can reconstruct and return the RHS tuple without guessing
                // from a generic Block's spans (Issues #6569, #10444).
                return Ok(Stmt::DestructuringAssign {
                    targets: var_names.clone(),
                    value: rhs_expr,
                    span,
                });
            }

            // Dependent swap with a tuple literal, e.g. `a, b = b, a % b`.
            // Evaluate every RHS element into its own temporary (reading the
            // *current* target values), then assign each target from its
            // temporary. This preserves Julia's simultaneous-assignment
            // semantics (all RHS evaluated left-to-right before any target is
            // written) while avoiding the per-iteration tuple allocation and
            // `IndexLoad`s that the temp-tuple form below would emit — matching
            // CPython's allocation-free swap and Julia's native handling
            // (Issue #6569).
            let temps: Vec<String> = elements.iter().map(|_| generate_temp_var()).collect();
            let mut stmts = Vec::with_capacity(elements.len() * 2);
            for (temp, value) in temps.iter().zip(elements.iter()) {
                stmts.push(Stmt::Assign {
                    var: temp.clone(),
                    value: value.clone(),
                    span,
                });
            }
            for (var_name, temp) in var_names.iter().zip(temps.iter()) {
                stmts.push(Stmt::Assign {
                    var: var_name.clone(),
                    value: Expr::Var(temp.clone().into(), span),
                    span,
                });
            }
            return Ok(Stmt::Block(Block { stmts, span }));
        }
    }

    if let Some(var_names) = flat_var_names {
        // A flat identifier pattern has a complete semantic representation in
        // this node: consumers evaluate `value` once, bind each requested
        // position, ignore extra RHS values, and let checked indexing raise for
        // a short RHS. The matching-arity dependent literal swap returned its
        // optimized Block above; nested/rest patterns remain expanded.
        return Ok(Stmt::DestructuringAssign {
            targets: var_names,
            value: rhs_expr,
            span,
        });
    }

    let temp_var = generate_temp_var();
    let mut stmts = Vec::new();

    stmts.push(Stmt::Assign {
        var: temp_var.clone(),
        value: rhs_expr,
        span,
    });

    for (i, pattern) in patterns.iter().enumerate() {
        match pattern {
            DestructureTarget::Rest(name) => {
                stmts.push(Stmt::Assign {
                    var: name.clone(),
                    value: tuple_tail_expr(Expr::Var(temp_var.clone().into(), span), i + 1, span),
                    span,
                });
            }
            _ => {
                let index_expr =
                    tuple_index_expr(Expr::Var(temp_var.clone().into(), span), i, span);
                emit_destructure_assignments(pattern, index_expr, &mut stmts, span);
            }
        }
    }

    Ok(Stmt::Block(Block { stmts, span }))
}

pub fn parse_destructure_targets<'a>(
    walker: &CstWalker<'a>,
    elements: &[Node<'a>],
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Vec<DestructureTarget>> {
    elements
        .iter()
        .map(|elem| parse_destructure_target(walker, elem, lambda_ctx))
        .collect()
}

fn parse_destructure_target<'a>(
    walker: &CstWalker<'a>,
    elem: &Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<DestructureTarget> {
    match walker.kind(elem) {
        NodeKind::Identifier => Ok(DestructureTarget::Identifier(walker.text(elem).to_string())),
        NodeKind::TupleExpression => {
            let children = walker.named_children_vec(elem);
            if children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(elem),
                )
                .with_hint("empty tuple in destructuring assignment"));
            }
            Ok(DestructureTarget::Tuple(parse_destructure_targets(
                walker, &children, lambda_ctx,
            )?))
        }
        NodeKind::SplatExpression => {
            let mut children = walker.named_children(elem);
            let Some(inner) = children.next() else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(elem),
                )
                .with_hint("empty rest/splat target in tuple destructuring"));
            };
            if walker.kind(&inner) != NodeKind::Identifier {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(&inner),
                )
                .with_hint("tuple destructuring rest/splat target must be an identifier"));
            }
            Ok(DestructureTarget::Rest(walker.text(&inner).to_string()))
        }
        NodeKind::IndexExpression => {
            let (array_node, index_nodes) = expr::extract_index_target_nodes(walker, *elem)
                .ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                        walker.span(elem),
                    )
                    .with_hint("indexed destructuring target requires array[index...] form")
                })?;
            let array = lower_expr_maybe_ctx(walker, array_node, lambda_ctx)?;
            let indices = index_nodes
                .into_iter()
                .map(|index| lower_expr_maybe_ctx(walker, index, lambda_ctx))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok(DestructureTarget::Index { array, indices })
        }
        NodeKind::FieldExpression => {
            let (object, field) = extract_nested_field_target_maybe_ctx(walker, *elem, lambda_ctx)
                .ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                        walker.span(elem),
                    )
                    .with_hint("field destructuring target requires object.field form")
                })?;
            Ok(DestructureTarget::Field { object, field })
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(elem),
        )
        .with_hint("tuple destructuring only supports identifiers or nested tuples")),
    }
}

fn flat_destructure_identifiers(patterns: &[DestructureTarget]) -> Option<Vec<String>> {
    patterns
        .iter()
        .map(|pattern| match pattern {
            DestructureTarget::Identifier(name) => Some(name.clone()),
            DestructureTarget::Tuple(_)
            | DestructureTarget::Rest(_)
            | DestructureTarget::Index { .. }
            | DestructureTarget::Field { .. } => None,
        })
        .collect()
}

fn validate_rest_targets(patterns: &[DestructureTarget], span: Span) -> LowerResult<()> {
    let mut rest_pos = None;
    for (i, pattern) in patterns.iter().enumerate() {
        match pattern {
            DestructureTarget::Rest(_) => {
                if rest_pos.is_some() {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                        span,
                    )
                    .with_hint("tuple destructuring supports only one rest/splat target"));
                }
                rest_pos = Some(i);
            }
            DestructureTarget::Tuple(children) if contains_rest_target(children) => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    span,
                )
                .with_hint("nested tuple destructuring rest/splat targets are not supported yet"));
            }
            _ => {}
        }
    }

    if let Some(pos) = rest_pos {
        if pos + 1 != patterns.len() {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                span,
            )
            .with_hint("tuple destructuring rest/splat target must be in final position"));
        }
    }

    Ok(())
}

fn contains_rest_target(patterns: &[DestructureTarget]) -> bool {
    patterns.iter().any(|pattern| match pattern {
        DestructureTarget::Rest(_) => true,
        DestructureTarget::Tuple(children) => contains_rest_target(children),
        DestructureTarget::Identifier(_)
        | DestructureTarget::Index { .. }
        | DestructureTarget::Field { .. } => false,
    })
}

fn tuple_index_expr(tuple: Expr, zero_based_index: usize, span: Span) -> Expr {
    Expr::Index {
        array: Box::new(tuple),
        indices: vec![Expr::Literal(
            crate::ir::core::Literal::Int((zero_based_index + 1) as i64),
            span,
        )],
        span,
    }
}

fn tuple_tail_expr(tuple: Expr, one_based_start_index: usize, span: Span) -> Expr {
    let start_index = i64::try_from(one_based_start_index).unwrap_or(i64::MAX);
    Expr::Call {
        function: TUPLE_TAIL_FUNCTION.to_string().into(),
        args: vec![
            tuple,
            Expr::Literal(crate::ir::core::Literal::Int(start_index), span),
        ],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span,
    }
}

/// Lower a tuple-destructuring assignment whose RHS is already a lowered `Expr`,
/// reusing the same `DestructureTarget` machinery as the CST path
/// (`lower_tuple_destructuring_impl`). Used by the macro-expansion lowering path
/// where assignments arrive as `Expr` values rather than CST nodes, so the
/// statement converter cannot call `lower_tuple_destructuring_impl` directly
/// (Issue #7900). Validates rest/splat placement, binds the RHS to a temporary,
/// then emits one assignment per (possibly nested) target — identical runtime
/// behavior to source-level `(a, b) = rhs`.
pub fn lower_destructuring_from_targets(
    patterns: Vec<DestructureTarget>,
    rhs: Expr,
    span: Span,
) -> LowerResult<Stmt> {
    if patterns.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            span,
        )
        .with_hint("empty tuple in destructuring assignment"));
    }
    validate_rest_targets(&patterns, span)?;

    let temp_var = generate_temp_var();
    let mut stmts = Vec::new();
    stmts.push(Stmt::Assign {
        var: temp_var.clone(),
        value: rhs,
        span,
    });

    for (i, pattern) in patterns.iter().enumerate() {
        match pattern {
            DestructureTarget::Rest(name) => {
                stmts.push(Stmt::Assign {
                    var: name.clone(),
                    value: tuple_tail_expr(Expr::Var(temp_var.clone().into(), span), i + 1, span),
                    span,
                });
            }
            _ => {
                let index_expr =
                    tuple_index_expr(Expr::Var(temp_var.clone().into(), span), i, span);
                emit_destructure_assignments(pattern, index_expr, &mut stmts, span);
            }
        }
    }

    Ok(Stmt::Block(Block { stmts, span }))
}

fn emit_destructure_assignments(
    pattern: &DestructureTarget,
    value: Expr,
    stmts: &mut Vec<Stmt>,
    span: Span,
) {
    match pattern {
        DestructureTarget::Identifier(name) => {
            stmts.push(Stmt::Assign {
                var: name.clone(),
                value,
                span,
            });
        }
        DestructureTarget::Tuple(children) => {
            for (i, child) in children.iter().enumerate() {
                emit_destructure_assignments(
                    child,
                    tuple_index_expr(value.clone(), i, span),
                    stmts,
                    span,
                );
            }
        }
        DestructureTarget::Rest(name) => {
            stmts.push(Stmt::Assign {
                var: name.clone(),
                value: tuple_tail_expr(value, 1, span),
                span,
            });
        }
        DestructureTarget::Index { array, indices } => {
            let mut args = Vec::with_capacity(2 + indices.len());
            args.push(array.clone());
            args.push(value);
            args.extend(indices.iter().cloned());
            let splat_mask = vec![false; args.len()];
            stmts.push(Stmt::Expr {
                expr: Expr::Call {
                    function: "setindex!".to_string().into(),
                    args,
                    kwargs: Vec::new(),
                    splat_mask,
                    kwargs_splat_mask: Vec::new(),
                    span,
                },
                span,
            });
        }
        DestructureTarget::Field { object, field } => {
            stmts.push(Stmt::Expr {
                expr: Expr::Call {
                    function: "setproperty!".to_string().into(),
                    args: vec![
                        object.clone(),
                        Expr::Literal(crate::ir::core::Literal::Symbol(field.clone()), span),
                        value,
                    ],
                    kwargs: Vec::new(),
                    splat_mask: vec![false, false, false],
                    kwargs_splat_mask: Vec::new(),
                    span,
                },
                span,
            });
        }
    }
}

fn expr_references_any_name(expr: &Expr, names: &[String]) -> bool {
    match expr {
        Expr::Var(name, _) => names.iter().any(|target| target == name),
        Expr::BinaryOp { left, right, .. } => {
            expr_references_any_name(left, names) || expr_references_any_name(right, names)
        }
        Expr::UnaryOp { operand, .. } => expr_references_any_name(operand, names),
        Expr::Call { args, kwargs, .. } => {
            args.iter().any(|arg| expr_references_any_name(arg, names))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_references_any_name(value, names))
        }
        Expr::Builtin { args, .. } | Expr::TupleLiteral { elements: args, .. } => {
            args.iter().any(|arg| expr_references_any_name(arg, names))
        }
        Expr::ArrayLiteral { elements, .. } => elements
            .iter()
            .any(|arg| expr_references_any_name(arg, names)),
        Expr::Index { array, indices, .. } => {
            expr_references_any_name(array, names)
                || indices
                    .iter()
                    .any(|index| expr_references_any_name(index, names))
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_references_any_name(start, names)
                || step
                    .as_deref()
                    .is_some_and(|step| expr_references_any_name(step, names))
                || expr_references_any_name(stop, names)
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            expr_references_any_name(body, names)
                || expr_references_any_name(iter, names)
                || filter
                    .as_deref()
                    .is_some_and(|filter| expr_references_any_name(filter, names))
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            expr_references_any_name(body, names)
                || iterations
                    .iter()
                    .any(|(_, iter)| expr_references_any_name(iter, names))
                || filter
                    .as_deref()
                    .is_some_and(|filter| expr_references_any_name(filter, names))
        }
        Expr::FieldAccess { object, .. } => expr_references_any_name(object, names),
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .any(|(_, value)| expr_references_any_name(value, names)),
        Expr::Pair { key, value, .. } => {
            expr_references_any_name(key, names) || expr_references_any_name(value, names)
        }
        Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
            expr_references_any_name(key, names) || expr_references_any_name(value, names)
        }),
        Expr::LetBlock { bindings, body, .. } => {
            bindings
                .iter()
                .any(|(_, value)| expr_references_any_name(value, names))
                || body
                    .stmts
                    .iter()
                    .any(|stmt| stmt_references_any_name(stmt, names))
        }
        Expr::StringConcat { parts, .. } => parts
            .iter()
            .any(|part| expr_references_any_name(part, names)),
        Expr::ModuleCall { args, kwargs, .. } => {
            args.iter().any(|arg| expr_references_any_name(arg, names))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_references_any_name(value, names))
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_references_any_name(condition, names)
                || expr_references_any_name(then_expr, names)
                || expr_references_any_name(else_expr, names)
        }
        Expr::New { args, .. } => args.iter().any(|arg| expr_references_any_name(arg, names)),
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr
                .as_deref()
                .is_some_and(|expr| expr_references_any_name(expr, names))
                || type_args
                    .iter()
                    .any(|arg| expr_references_any_name(arg, names))
        }
        Expr::QuoteLiteral { constructor, .. } => expr_references_any_name(constructor, names),
        Expr::AssignExpr { value, .. } => expr_references_any_name(value, names),
        Expr::ReturnExpr { value, .. } => value
            .as_deref()
            .is_some_and(|value| expr_references_any_name(value, names)),
        _ => false,
    }
}

fn stmt_references_any_name(stmt: &Stmt, names: &[String]) -> bool {
    match stmt {
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::Expr { expr: value, .. } => expr_references_any_name(value, names),
        Stmt::Block(block) => block
            .stmts
            .iter()
            .any(|stmt| stmt_references_any_name(stmt, names)),
        _ => true,
    }
}

/// Counter for generating unique temporary variable names for nested field assignment
static FIELD_TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn generate_field_temp_var() -> String {
    let id = FIELD_TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("__field_tmp_{}", id)
}

fn lower_field_assignment_impl<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    // Try simple case first: obj.field = value
    if let Some((object_name, field_name)) = expr::extract_field_target(walker, lhs) {
        let value = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;
        return Ok(Stmt::FieldAssign {
            object: object_name,
            field: field_name,
            value,
            span,
        });
    }

    // Handle nested field assignment: obj.inner.field = value
    // Decompose into: __field_tmp = obj.inner; __field_tmp.field = value
    let (object_expr, field_name) = extract_nested_field_target_maybe_ctx(walker, lhs, lambda_ctx)
        .ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            )
            .with_hint("field assignment requires variable.field form")
        })?;

    let temp_var = generate_field_temp_var();
    let value = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;

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

fn lower_index_assignment_impl<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    op_text: &str,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    if op_text == ".=" {
        let dest = lower_index_broadcast_assignment_dest(walker, lhs, span, lambda_ctx)?;
        let rhs_expr = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;
        let expr = broadcast_assignment_call(dest, rhs_expr, span);
        return Ok(Stmt::Expr { expr, span });
    }

    let (array_name, index_nodes) = match expr::extract_index_target(walker, lhs) {
        Some(t) => t,
        // The array target is not a bare variable (e.g. `obj.field[i] = v`,
        // `f(x)[i] = v`). Desugar to `setindex!(<array expr>, value, indices...)`,
        // the standard meaning of `a[i] = v`, instead of rejecting it (Issue #6640).
        None => return lower_complex_index_assignment(walker, lhs, rhs, span, lambda_ctx),
    };

    // Extract indices
    let mut indices = Vec::new();
    for idx_node in index_nodes {
        match walker.kind(&idx_node) {
            NodeKind::RangeExpression => {
                indices.push(lower_expr_maybe_ctx(walker, idx_node, lambda_ctx)?)
            }
            NodeKind::Operator => {
                let text = walker.text(&idx_node);
                if text == ":" {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::ArraySlicing,
                        walker.span(&idx_node),
                    ));
                }
                indices.push(lower_expr_maybe_ctx(walker, idx_node, lambda_ctx)?);
            }
            _ => indices.push(lower_expr_maybe_ctx(walker, idx_node, lambda_ctx)?),
        }
    }

    let value = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;

    Ok(Stmt::IndexAssign {
        array: array_name,
        indices,
        value,
        span,
    })
}

fn lower_index_broadcast_assignment_dest<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    span: Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let (array_node, index_nodes) =
        expr::extract_index_target_nodes(walker, lhs).ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            )
        })?;

    let mut args = Vec::with_capacity(1 + index_nodes.len());
    args.push(lower_expr_maybe_ctx(walker, array_node, lambda_ctx)?);
    for idx_node in index_nodes {
        args.push(lower_expr_maybe_ctx(walker, idx_node, lambda_ctx)?);
    }
    let splat_mask = vec![false; args.len()];

    Ok(Expr::Call {
        function: "view".to_string().into(),
        args,
        kwargs: Vec::new(),
        splat_mask,
        kwargs_splat_mask: Vec::new(),
        span,
    })
}

/// Lower an indexed assignment whose array is not a bare variable, e.g.
/// `obj.field[i] = v` or `f(x)[i] = v`, by desugaring to
/// `setindex!(<array expr>, value, indices...)` — the standard meaning of
/// `a[i] = v` in Julia (Issue #6640). The previous lowering rejected these with
/// `UnsupportedAssignmentTarget`, forcing a local-variable workaround.
fn lower_complex_index_assignment<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let (array_node, index_nodes) =
        expr::extract_index_target_nodes(walker, lhs).ok_or_else(|| {
            UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                walker.span(&lhs),
            )
        })?;

    let array_expr = lower_expr_maybe_ctx(walker, array_node, lambda_ctx)?;

    let mut indices = Vec::new();
    for idx_node in index_nodes {
        match walker.kind(&idx_node) {
            NodeKind::RangeExpression => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::ArraySlicing,
                    walker.span(&idx_node),
                ))
            }
            NodeKind::Operator if walker.text(&idx_node) == ":" => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::ArraySlicing,
                    walker.span(&idx_node),
                ))
            }
            _ => indices.push(lower_expr_maybe_ctx(walker, idx_node, lambda_ctx)?),
        }
    }

    let value = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;

    // setindex!(array, value, indices...)
    let mut args = Vec::with_capacity(2 + indices.len());
    args.push(array_expr);
    args.push(value);
    args.extend(indices);
    let splat_mask = vec![false; args.len()];

    Ok(Stmt::Expr {
        expr: Expr::Call {
            function: "setindex!".to_string().into(),
            args,
            kwargs: Vec::new(),
            splat_mask,
            kwargs_splat_mask: Vec::new(),
            span,
        },
        span,
    })
}

pub fn lower_compound_assignment<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    lower_compound_assignment_impl(walker, node, None)
}

fn lower_compound_assignment_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);
    if named.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("compound assignment".to_string()),
            span,
        ));
    }

    let lhs = named[0];
    let rhs = named[named.len() - 1];

    let op_text = extract_operator_text(walker, node).unwrap_or_else(|| "?".to_string());
    let rhs_expr = lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?;

    let broadcast_op = match op_text.as_str() {
        ".+=" => Some(".+"),
        ".-=" => Some(".-"),
        ".*=" => Some(".*"),
        "./=" => Some("./"),
        ".^=" => Some(".^"),
        ".&=" => Some(".&"),
        ".|=" => Some(".|"),
        ".⊻=" => Some(".⊻"),
        ".<<=" => Some(".<<"),
        ".>>=" => Some(".>>"),
        ".>>>=" => Some(".>>>"),
        _ => None,
    };

    // Handle IndexExpression on LHS: arr[i] += x
    if walker.kind(&lhs) == NodeKind::IndexExpression {
        if op_text == ".=" {
            let dest = lower_index_broadcast_assignment_dest(walker, lhs, span, lambda_ctx)?;
            let expr = broadcast_assignment_call(dest, rhs_expr, span);
            return Ok(Stmt::Expr { expr, span });
        }

        if let Some(broadcast_op) = broadcast_op {
            let dest = lower_index_broadcast_assignment_dest(walker, lhs, span, lambda_ctx)?;
            let expr = if op_text == ".=" {
                broadcast_assignment_call(dest, rhs_expr, span)
            } else {
                broadcast_compound_assignment_call(broadcast_op, dest, rhs_expr, span)
            };
            return Ok(Stmt::Expr { expr, span });
        }

        if is_compound_assignment_operator(&op_text) {
            // Complex array target (e.g. `obj.field[i] += x`): desugar to
            // `setindex!(<array>, getindex(<array>, i) op rhs, i)` (Issue #6640).
            // The array expression is re-evaluated; this is safe for the common
            // `obj.field` (side-effect-free getfield) case.
            if expr::extract_index_target(walker, lhs).is_none() {
                let (array_node, index_nodes) = expr::extract_index_target_nodes(walker, lhs)
                    .ok_or_else(|| {
                        UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                            walker.span(&lhs),
                        )
                    })?;
                let array_expr = lower_expr_maybe_ctx(walker, array_node, lambda_ctx)?;
                let mut indices = Vec::new();
                for idx_node in &index_nodes {
                    indices.push(lower_expr_maybe_ctx(walker, *idx_node, lambda_ctx)?);
                }
                let current_value = Expr::Index {
                    array: Box::new(array_expr.clone()),
                    indices: indices.clone(),
                    span,
                };
                let new_value = compound_assignment_value(&op_text, current_value, rhs_expr, span)
                    .ok_or_else(|| {
                        UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedOperator(op_text.clone()),
                            span,
                        )
                    })?;
                let mut args = Vec::with_capacity(2 + indices.len());
                args.push(array_expr);
                args.push(new_value);
                args.extend(indices);
                let splat_mask = vec![false; args.len()];
                return Ok(Stmt::Expr {
                    expr: Expr::Call {
                        function: "setindex!".to_string().into(),
                        args,
                        kwargs: Vec::new(),
                        splat_mask,
                        kwargs_splat_mask: Vec::new(),
                        span,
                    },
                    span,
                });
            }

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
                indices.push(lower_expr_maybe_ctx(walker, *idx_node, lambda_ctx)?);
            }

            // Create the index expression for the current value: arr[i]
            let current_value = Expr::Index {
                array: Box::new(Expr::Var(array_name.clone().into(), span)),
                indices: indices.clone(),
                span,
            };

            // Create the binary operation: arr[i] op rhs
            let new_value = compound_assignment_value(&op_text, current_value, rhs_expr, span)
                .ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedOperator(op_text.clone()),
                        span,
                    )
                })?;

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
        // Split the `.=` and `.op=` cases so the broadcast operator is bound
        // by the `if let Some(..)` pattern itself (matching the sibling
        // `IndexExpression` handling above) instead of re-testing
        // `broadcast_op.is_some()` in the condition and re-extracting it a
        // few lines later (Issue #10905, Phase 1b of #10869).
        if op_text == ".=" {
            let dest = lower_expr_maybe_ctx(walker, lhs, lambda_ctx)?;
            let expr = broadcast_assignment_call(dest, rhs_expr, span);
            return Ok(Stmt::Expr { expr, span });
        }
        if let Some(broadcast_op) = broadcast_op {
            let dest = lower_expr_maybe_ctx(walker, lhs, lambda_ctx)?;
            let expr = broadcast_compound_assignment_call(broadcast_op, dest, rhs_expr, span);
            return Ok(Stmt::Expr { expr, span });
        }

        if is_compound_assignment_operator(&op_text) {
            // Try simple case first: obj.field += x
            if let Some((object_name, field_name)) = expr::extract_field_target(walker, lhs) {
                let current_value = Expr::FieldAccess {
                    object: Box::new(Expr::Var(object_name.clone().into(), span)),
                    field: field_name.clone().into(),
                    span,
                };

                let new_value = compound_assignment_value(&op_text, current_value, rhs_expr, span)
                    .ok_or_else(|| {
                        UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedOperator(op_text.clone()),
                            span,
                        )
                    })?;

                return Ok(Stmt::FieldAssign {
                    object: object_name,
                    field: field_name,
                    value: new_value,
                    span,
                });
            }

            // Handle nested case: obj.inner.field += x (Issue #2309)
            if let Some((object_expr, field_name)) =
                extract_nested_field_target_maybe_ctx(walker, lhs, lambda_ctx)
            {
                let temp_var = generate_field_temp_var();

                let current_value = Expr::FieldAccess {
                    object: Box::new(Expr::Var(temp_var.clone().into(), span)),
                    field: field_name.clone().into(),
                    span,
                };

                let new_value = compound_assignment_value(&op_text, current_value, rhs_expr, span)
                    .ok_or_else(|| {
                        UnsupportedFeature::new(
                            UnsupportedFeatureKind::UnsupportedOperator(op_text.clone()),
                            span,
                        )
                    })?;

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

    if op_text == ".=" {
        let dest = lower_expr_maybe_ctx(walker, lhs, lambda_ctx)?;
        let expr = broadcast_assignment_call(dest, rhs_expr, span);
        return Ok(Stmt::Expr { expr, span });
    }
    if let Some(broadcast_op) = broadcast_op {
        let dest = lower_expr_maybe_ctx(walker, lhs, lambda_ctx)?;
        let expr = broadcast_compound_assignment_call(broadcast_op, dest, rhs_expr, span);
        return Ok(Stmt::Expr { expr, span });
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

    // Convert compound assignment to a binary operation or operator call.
    // x op= val → x = x op val
    if is_compound_assignment_operator(&op_text) {
        let var_expr = Expr::Var(var.clone().into(), span);
        let value =
            compound_assignment_value(&op_text, var_expr, rhs_expr, span).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedOperator(op_text.clone()),
                    span,
                )
            })?;
        return Ok(Stmt::Assign { var, value, span });
    }

    // Handle broadcast assignment (.=)
    // Z .= expr lowers to Z = materialize!(Z, expr) so alias-observable in-place semantics
    // are preserved.
    if op_text == ".=" {
        let value = broadcast_assignment_call(Expr::Var(var.clone().into(), span), rhs_expr, span);
        return Ok(Stmt::Assign { var, value, span });
    }

    // Handle broadcast compound assignments (.+=, .-=, .*=, .&=, etc.)
    if let Some(op_name) = broadcast_op {
        let var_expr = Expr::Var(var.clone().into(), span);
        let value = broadcast_compound_assignment_call(op_name, var_expr, rhs_expr, span);
        return Ok(Stmt::Assign { var, value, span });
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedOperator(op_text),
        span,
    ))
}

/// Lower a compound assignment (`x += y`, `p.z *= y`, `a[i] -= y`, `Z .+= y`, …)
/// that appears in **expression position** — as the value of a `return`, the RHS
/// of another assignment, or an argument (Issue #7269).
///
/// In Julia a compound assignment is an expression whose value is the *newly
/// assigned value*. Upstream lowers `p.z += 1.0` to
/// `tmp = getproperty(p,:z) + 1.0; setproperty!(p,:z,tmp); return tmp` — i.e. the
/// yielded value is the freshly computed value, NOT a re-read of the target. We
/// mirror that here by lowering the statement form first (which already produces
/// the correct desugaring for every supported LHS shape) and then converting the
/// resulting `Stmt` into a value-producing `Expr` that yields the assigned value.
pub fn lower_compound_assignment_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Expr> {
    lower_compound_assignment_expr_impl(walker, node, None)
}

pub fn lower_compound_assignment_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_compound_assignment_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_compound_assignment_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let stmt = lower_compound_assignment_impl(walker, node, lambda_ctx)?;
    compound_assign_stmt_to_value_expr(stmt, span)
}

/// Lower a plain `=` assignment whose target is NOT a bare identifier — e.g.
/// `a[i] = v`, `obj.field = v`, `obj.field[i] = v` — that appears in **expression
/// position**: the RHS of another assignment (`y = (a[1] = 5)`), a single-
/// expression arrow-lambda body (`x -> (x[2] = v)`), an argument, etc.
///
/// In Julia an assignment is an expression whose value is the assigned RHS value
/// (`(a[1] = 5)` yields `5`). We mirror the compound-assignment expression path
/// (Issue #7269): lower the **statement** form first — which already produces the
/// canonical `setindex!` / `setproperty!` (`IndexAssign` / `FieldAssign`)
/// desugaring for every supported LHS shape — then convert the resulting `Stmt`
/// into a value-producing `Expr` that yields the assigned value (Issue #8007).
pub fn lower_assignment_value_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Expr> {
    lower_assignment_value_expr_impl(walker, node, None)
}

pub fn lower_assignment_value_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_assignment_value_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_assignment_value_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);
    if named.len() >= 2
        && walker.kind(&named[0]) == NodeKind::TupleExpression
        && extract_operator_text(walker, node)
            .as_deref()
            .unwrap_or("=")
            == "="
    {
        return lower_tuple_destructuring_value_expr_impl(
            walker,
            named[0],
            named[named.len() - 1],
            span,
            lambda_ctx,
        );
    }

    let stmt = lower_assignment_impl(walker, node, lambda_ctx)?;
    compound_assign_stmt_to_value_expr(stmt, span)
}

fn lower_tuple_destructuring_value_expr_impl<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
    rhs: Node<'a>,
    span: Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let elements = walker.named_children_vec(&lhs);
    let patterns = parse_destructure_targets(walker, &elements, lambda_ctx)?;
    if patterns.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
        .with_hint("empty tuple in destructuring assignment"));
    }
    validate_rest_targets(&patterns, walker.span(&lhs))?;

    let temp_var = generate_temp_var();
    let mut stmts = Vec::new();
    stmts.push(Stmt::Assign {
        var: temp_var.clone(),
        value: lower_expr_maybe_ctx(walker, rhs, lambda_ctx)?,
        span,
    });

    for (i, pattern) in patterns.iter().enumerate() {
        match pattern {
            DestructureTarget::Rest(name) => {
                stmts.push(Stmt::Assign {
                    var: name.clone(),
                    value: tuple_tail_expr(Expr::Var(temp_var.clone().into(), span), i + 1, span),
                    span,
                });
            }
            _ => {
                let index_expr =
                    tuple_index_expr(Expr::Var(temp_var.clone().into(), span), i, span);
                emit_destructure_assignments(pattern, index_expr, &mut stmts, span);
            }
        }
    }

    stmts.push(Stmt::Expr {
        expr: Expr::Var(temp_var.into(), span),
        span,
    });

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block { stmts, span },
        span,
    })
}

/// Convert a lowered compound-assignment `Stmt` into an `Expr` whose value is the
/// freshly assigned value, preserving Julia's "compound assignment is an
/// expression" semantics (Issue #7269).
///
/// The statement form of `lhs op= rhs` always computes the new value exactly once
/// (the `lhs op rhs` binary op) and stores it into the target. To yield that value
/// without re-reading the target (and without recomputing the binop, which could
/// re-trigger side effects), we bind the computed value to a fresh temporary,
/// perform the store using that temporary, and let the temporary be the block's
/// value.
fn compound_assign_stmt_to_value_expr(stmt: Stmt, span: Span) -> LowerResult<Expr> {
    // Helper: build `let __tmp = <value>; <store(__tmp)>; __tmp end` as a value.
    fn let_block_yielding_temp(temp: String, value: Expr, store: Stmt, span: Span) -> Expr {
        let init = Stmt::Assign {
            var: temp.clone(),
            value,
            span,
        };
        let read = Stmt::Expr {
            expr: Expr::Var(temp.into(), span),
            span,
        };
        Expr::LetBlock {
            bindings: vec![],
            body: Block {
                stmts: vec![init, store, read],
                span,
            },
            span,
        }
    }

    match stmt {
        // Simple variable (`x += y`), broadcast (`Z .= …`), and broadcast-compound
        // (`Z .+= …`) all lower to `Stmt::Assign { var, value }`. `AssignExpr`
        // already assigns `value` to `var` and yields `value`, exactly matching
        // Julia's semantics — no temporary needed.
        Stmt::Assign { var, value, span } => Ok(Expr::AssignExpr {
            var: var.into(),
            value: Box::new(value),
            span,
        }),
        // Simple field target (`obj.field += y`).
        Stmt::FieldAssign {
            object,
            field,
            value,
            span: stmt_span,
        } => {
            let temp = generate_field_temp_var();
            let store = Stmt::FieldAssign {
                object,
                field,
                value: Expr::Var(temp.clone().into(), stmt_span),
                span: stmt_span,
            };
            Ok(let_block_yielding_temp(temp, value, store, span))
        }
        // Simple indexed target (`a[i] += y`).
        Stmt::IndexAssign {
            array,
            indices,
            value,
            span: stmt_span,
        } => {
            let temp = generate_field_temp_var();
            let store = Stmt::IndexAssign {
                array,
                indices,
                value: Expr::Var(temp.clone().into(), stmt_span),
                span: stmt_span,
            };
            Ok(let_block_yielding_temp(temp, value, store, span))
        }
        // Complex indexed target (`obj.field[i] += y`) desugars to a
        // `setindex!(<array>, <new_value>, <indices>…)` call statement. The new
        // value is the second argument; bind it to a temp and yield the temp.
        Stmt::Expr {
            expr:
                Expr::Call {
                    function,
                    mut args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span: call_span,
                },
            span: stmt_span,
        } if function == "setindex!" && args.len() >= 2 => {
            let temp = generate_field_temp_var();
            let value = std::mem::replace(&mut args[1], Expr::Var(temp.clone().into(), call_span));
            let store = Stmt::Expr {
                expr: Expr::Call {
                    function,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span: call_span,
                },
                span: stmt_span,
            };
            Ok(let_block_yielding_temp(temp, value, store, span))
        }
        Stmt::Expr {
            expr:
                Expr::Call {
                    function,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span: call_span,
                },
            span: _,
        } if function == "materialize!" => Ok(Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span: call_span,
        }),
        // Nested field target (`obj.inner.field += y`) desugars to a block:
        // `__field_tmp = obj.inner; __field_tmp.field = <new_value>`. Re-bind the
        // new value to a fresh temp inside the block and yield that temp.
        Stmt::Block(Block {
            mut stmts,
            span: block_span,
        }) => {
            if let Some(Stmt::FieldAssign {
                object,
                field,
                value,
                span: fa_span,
            }) = stmts.pop()
            {
                let temp = generate_field_temp_var();
                stmts.push(Stmt::Assign {
                    var: temp.clone(),
                    value,
                    span: fa_span,
                });
                stmts.push(Stmt::FieldAssign {
                    object,
                    field,
                    value: Expr::Var(temp.clone().into(), fa_span),
                    span: fa_span,
                });
                stmts.push(Stmt::Expr {
                    expr: Expr::Var(temp.into(), span),
                    span,
                });
                Ok(Expr::LetBlock {
                    bindings: vec![],
                    body: Block {
                        stmts,
                        span: block_span,
                    },
                    span,
                })
            } else {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "compound_assignment_expression".to_string(),
                    ),
                    span,
                ))
            }
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "compound_assignment_expression".to_string(),
            ),
            span,
        )),
    }
}

pub fn extract_operator_text<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
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
    lower_assignment_impl(walker, node, Some(lambda_ctx))
}

pub fn lower_compound_assignment_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    lower_compound_assignment_impl(walker, node, Some(lambda_ctx))
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

fn extract_nested_field_target_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> Option<(Expr, String)> {
    match lambda_ctx {
        Some(ctx) => expr::extract_nested_field_target_with_ctx(walker, node, ctx),
        None => expr::extract_nested_field_target(walker, node),
    }
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
            function: "Broadcasted".to_string().into(),
            args,
            kwargs: vec![],
            splat_mask: vec![false; n],
            kwargs_splat_mask: vec![],
            span: s(),
        }
    }

    fn make_materialize(inner: Expr) -> Expr {
        Expr::Call {
            function: "materialize".to_string().into(),
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
            function: "other".to_string().into(),
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
            function: "foo".to_string().into(),
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
