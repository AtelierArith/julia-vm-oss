//! For statement lowering
//!
//! Handles:
//! - For loops with ranges: `for v in start:end ... end`
//! - Step ranges: `for v in start:step:end ... end`
//! - For-each loops over iterables: `for c in "string" ... end`, `for x in arr ... end`

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Expr, Literal, Stmt, UnaryOp};
use crate::lowering::expr;
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

use super::lower_block;
use super::lower_block_with_ctx;

/// Returns true if the expression is unambiguously non-integer at lowering time
/// (e.g., a float, char, or non-integer literal). Used by the for-range fast path
/// in `lower_for_binding` to decide whether to fall through to a generic
/// `ForEach` over `Expr::Range` instead of emitting `Stmt::For` (which assumes
/// integer-typed bounds and step). Issue #3551: `for x in 1.0:0.5:2.0` was
/// silently producing zero iterations because the integer fast path could not
/// represent a float step.
fn is_non_integer_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(lit, _) => matches!(
            lit,
            Literal::Float(_) | Literal::Float32(_) | Literal::Float16(_) | Literal::BigFloat(_)
        ),
        // Unary +/- applied to a float literal: `-0.5`, `+1.0`.
        Expr::UnaryOp { op, operand, .. } => {
            matches!(op, UnaryOp::Neg | UnaryOp::Pos) && is_non_integer_literal(operand)
        }
        _ => false,
    }
}

/// Returns true if any of `start`/`end`/`step` is a non-integer literal
/// (typically a float). The integer fast path in `Stmt::For` codegen pins
/// every component to `ValueType::I64`; encountering a float literal there
/// silently truncates and can yield zero iterations (Issue #3551). When this
/// is true we instead lower to `Stmt::ForEach` over `Expr::Range`, which goes
/// through Pure Julia `iterate(::StepRangeLen)` and handles floats correctly.
fn for_range_has_non_integer_bound(start: &Expr, end: &Expr, step: Option<&Expr>) -> bool {
    is_non_integer_literal(start)
        || is_non_integer_literal(end)
        || step.is_some_and(is_non_integer_literal)
}

/// Build an `Expr::Range` from already-lowered start/end/step components.
/// Used by the for-loop lowering to fall back from the integer fast path to
/// the generic `ForEach` over `Expr::Range` when a float literal is detected
/// in the for-head (Issue #3551).
fn build_range_expr(start: Expr, end: Expr, step: Option<Expr>, span: Span) -> Expr {
    Expr::Range {
        start: Box::new(start),
        step: step.map(Box::new),
        stop: Box::new(end),
        span,
    }
}

/// Result of lowering a for binding - either a range or a general iterable
enum ForBindingResult {
    /// Range-based iteration: (var, start, end, step)
    Range {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        /// Optional `::T` annotation on the loop variable (`for i::T in ...`),
        /// already lowered to a type expression. When present, lowering injects
        /// `i = convert(T, i)` at the top of the loop body so each iterate value
        /// is converted to `T` (Issue #8208).
        var_type: Option<Expr>,
    },
    /// General iterable (string, array, tuple, etc.): (var, iterable)
    Iterable {
        var: String,
        iterable: Expr,
        /// Optional `::T` annotation on the loop variable (see `Range::var_type`).
        var_type: Option<Expr>,
    },
    /// Tuple destructuring iteration: (vars, iterable)
    /// `for (a, b) in collection`
    TupleIterable { vars: Vec<String>, iterable: Expr },
}

pub fn lower_for_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    lower_for_stmt_impl(walker, node, None)
}

fn lower_for_stmt_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let body_node = walker
        .named_children(&node)
        .into_iter()
        .find(|child| walker.kind(child) == NodeKind::Block);

    let body = match body_node {
        Some(body_node) => lower_block_maybe_ctx(walker, body_node, lambda_ctx)?,
        None => Block {
            stmts: Vec::new(),
            span,
        },
    };

    lower_for_stmt_with_body(walker, node, lambda_ctx, body)
}

/// Lower a `ForStatement` while substituting an already-lowered `body` block for
/// the loop body. Used by statement-position `@sync for ... @async ... end`
/// lowering, which must rewrite each `@async` inside the loop body into an inline
/// try/catch that accumulates exceptions (Issue #7831); everything else (the
/// bindings, cartesian desugaring) is shared with the plain for-loop path.
pub(crate) fn lower_for_stmt_with_body<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
    body: Block,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let bindings: Vec<Node<'a>> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) == NodeKind::ForBinding)
        .collect();

    if bindings.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            span,
        ));
    }

    // Cartesian `for x in xs, y in ys ... end` desugars to nested loops
    // (`for x in xs; for y in ys ... end; end`), exactly as upstream Julia's
    // `expand-for` expands multiple comma-separated iterators (Issue #6865).
    // The first binding becomes the outermost loop and the last the innermost;
    // an inner iterator may reference the variables bound by outer ones
    // (e.g. `for i in 1:3, j in 1:i`), which falls out naturally because each
    // inner loop is lowered into the body of the loop that precedes it.
    let binding_results = bindings
        .iter()
        .map(|binding| lower_for_binding_impl(walker, *binding, lambda_ctx))
        .collect::<LowerResult<Vec<_>>>()?;

    let mut stmt: Option<Stmt> = None;
    for binding_result in binding_results.into_iter().rev() {
        let inner_body = match stmt.take() {
            Some(inner) => Block {
                stmts: vec![inner],
                span,
            },
            None => body.clone(),
        };
        stmt = Some(build_for_stmt(binding_result, inner_body, span));
    }

    // `stmt` is always `Some` here because `bindings` is non-empty.
    stmt.ok_or_else(|| UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedForBinding, span))
}

/// Build a single `for` `Stmt` from one lowered binding and an already-lowered
/// body block. Used by the cartesian-`for` desugaring to materialize each loop
/// level (innermost first) into the body of the level that encloses it.
fn build_for_stmt(binding_result: ForBindingResult, body: Block, span: Span) -> Stmt {
    match binding_result {
        ForBindingResult::Range {
            var,
            start,
            end,
            step,
            var_type,
        } => {
            let (loop_var, body) = apply_var_type_convert(var, var_type, body, span);
            Stmt::For {
                var: loop_var,
                start,
                end,
                step,
                body,
                span,
            }
        }
        ForBindingResult::Iterable {
            var,
            iterable,
            var_type,
        } => {
            let (loop_var, body) = apply_var_type_convert(var, var_type, body, span);
            Stmt::ForEach {
                var: loop_var,
                iterable,
                body,
                span,
            }
        }
        ForBindingResult::TupleIterable { vars, iterable } => Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            span,
        },
    }
}

/// Apply a `::T` annotation on a single loop variable (`for i::T in itr`),
/// mirroring upstream Julia, which iterates a hidden variable and binds the
/// typed local via `convert` on every iteration (Issue #8208):
///
/// ```text
/// for i::T in itr          for #i in itr
///     <body>          ==>      i = convert(T, #i)
/// end                          <body>
///                          end
/// ```
///
/// Returns `(loop_var, body)`: when typed, `loop_var` is a fresh hidden name so
/// the loop's own counter/iterate slot is never clobbered by the body — critical
/// for the integer-range fast path, which uses the loop variable itself as the
/// `I64` counter (writing a converted value back into it corrupts the slot). The
/// body keeps using `i`, now a separate typed local. `convert` errors
/// (InexactError/MethodError) surface exactly as upstream. When untyped this is a
/// no-op: the original `var` and `body` pass through unchanged.
fn apply_var_type_convert(
    var: String,
    var_type: Option<Expr>,
    mut body: Block,
    span: Span,
) -> (String, Block) {
    let Some(type_expr) = var_type else {
        return (var, body);
    };
    // Hidden iterate variable, distinct from the user-visible typed local `var`.
    // The span offset disambiguates nested typed loops that reuse the same name.
    let hidden = format!("{}#fortyped{}", var, span.start);
    let convert_call = Expr::Call {
        function: "convert".to_string(),
        args: vec![type_expr, Expr::Var(hidden.clone(), span)],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span,
    };
    body.stmts.insert(
        0,
        Stmt::Assign {
            var,
            value: convert_call,
            span,
        },
    );
    (hidden, body)
}

fn lower_for_binding_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<ForBindingResult> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // The parser marks `for outer i in itr` as `[outer, i, itr]` while still
    // allowing `for outer in itr` to bind the variable named `outer`.
    // Full Julia `outer` loop-variable semantics require local-scope tracking
    // that this IR does not model yet; reject the modifier instead of lowering
    // it as `for outer in i` and executing the wrong program (Issue #6465).
    if named.len() >= 3
        && walker.kind(&named[0]) == NodeKind::Identifier
        && walker.text(&named[0]) == "outer"
    {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedForBinding, span)
                .with_hint("no outer local variable declaration exists for \"for outer\""),
        );
    }

    // Find the variable (identifier or tuple pattern) and the expression after "in"
    let mut var: Option<String> = None;
    let mut tuple_vars: Option<Vec<String>> = None; // For tuple destructuring: (a, b)
    let mut binding_found = false; // Track if we found a binding (var or tuple)
    let mut range_node: Option<Node<'a>> = None;
    let mut iterable_node: Option<Node<'a>> = None;
    // Lowered `::T` annotation on a single loop variable (`for i::T in ...`),
    // when present. Threaded into the ForBindingResult so the loop body converts
    // each iterate value to `T` (Issue #8208).
    let mut var_type: Option<Expr> = None;

    for child in &named {
        match walker.kind(child) {
            // Check for TupleExpression as binding pattern (before we have any binding)
            NodeKind::TupleExpression if !binding_found => {
                let tuple_children = walker.named_children(child);
                let vars: Vec<String> = tuple_children
                    .iter()
                    .filter(|c| walker.kind(c) == NodeKind::Identifier)
                    .map(|c| walker.text(c).to_string())
                    .collect();
                if !vars.is_empty() {
                    tuple_vars = Some(vars);
                    binding_found = true;
                }
            }
            // Typed single loop variable: `for i::T in itr` (Issue #8208). The
            // binding parses as a TypedExpression `[Identifier, Type]`; bind `i`
            // and remember `T`. (Upstream rejects `for (a, b)::T in itr`, so only
            // the bare-identifier form reaches here.)
            NodeKind::TypedExpression if !binding_found => {
                let typed_children = walker.named_children(child);
                let name = typed_children
                    .iter()
                    .find(|c| walker.kind(c) == NodeKind::Identifier)
                    .map(|c| walker.text(c).to_string());
                // Convention (see extract_typed_type_node): children are
                // ordered `[var, type]`, so the type is the last named child.
                let type_node = if typed_children.len() >= 2 {
                    typed_children.last().copied()
                } else {
                    None
                };
                if let (Some(name), Some(type_node)) = (name, type_node) {
                    var = Some(name);
                    var_type = Some(lower_expr_maybe_ctx(walker, type_node, lambda_ctx)?);
                    binding_found = true;
                }
            }
            NodeKind::Identifier if !binding_found => {
                var = Some(walker.text(child).to_string());
                binding_found = true;
            }
            NodeKind::RangeExpression => {
                range_node = Some(*child);
            }
            NodeKind::Operator => {
                // Skip the "in" operator
            }
            // Handle non-range iterables (string, array, identifier, etc.)
            NodeKind::StringLiteral
            | NodeKind::VectorExpression
            | NodeKind::TupleExpression
            | NodeKind::CallExpression
            | NodeKind::ComprehensionExpression => {
                if binding_found {
                    // This is the iterable expression
                    iterable_node = Some(*child);
                }
            }
            // Identifier after "in" (for variable references like `arr`)
            NodeKind::Identifier if binding_found => {
                iterable_node = Some(*child);
            }
            _ => {
                // For any other expression type that could be an iterable
                if binding_found && range_node.is_none() {
                    iterable_node = Some(*child);
                }
            }
        }
    }

    // Must have either single var or tuple vars
    if !binding_found {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            span,
        ));
    }

    // If we have a range expression, handle it as before
    // Note: Range iteration doesn't support tuple destructuring
    if let Some(range_node) = range_node {
        let var = var.ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedForBinding, span)
                .with_hint("range iteration does not support tuple destructuring")
        })?;

        let range_children = walker.named_children(&range_node);
        if range_children.len() < 2 {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedRange,
                walker.span(&range_node),
            ));
        }

        // Tree-sitter parses `5:-1:1` as nested: (5:-1):1
        // Check if first child is a RangeExpression (indicates step range)
        if walker.kind(&range_children[0]) == NodeKind::RangeExpression {
            // Step range: (start:step):end
            let inner_range = range_children[0];
            let inner_children = walker.named_children(&inner_range);
            if inner_children.len() >= 2 {
                let start = lower_expr_maybe_ctx(walker, inner_children[0], lambda_ctx)?;
                let step = lower_expr_maybe_ctx(walker, inner_children[1], lambda_ctx)?;
                let end = lower_expr_maybe_ctx(walker, range_children[1], lambda_ctx)?;
                // Float-stepped range (e.g., `1.0:0.5:2.0`): fall back to the
                // generic ForEach over Expr::Range so Pure Julia
                // iterate(::StepRangeLen) handles non-integer iteration
                // correctly (Issue #3551).
                if for_range_has_non_integer_bound(&start, &end, Some(&step)) {
                    let range_span = walker.span(&range_node);
                    let iterable = build_range_expr(start, end, Some(step), range_span);
                    return Ok(ForBindingResult::Iterable {
                        var,
                        iterable,
                        var_type,
                    });
                }
                return Ok(ForBindingResult::Range {
                    var,
                    start,
                    end,
                    step: Some(step),
                    var_type,
                });
            }
        }

        // Unit range: start:end
        let start = lower_expr_maybe_ctx(walker, range_children[0], lambda_ctx)?;
        let end = lower_expr_maybe_ctx(walker, range_children[1], lambda_ctx)?;
        // Float unit range (e.g., `1.0:3.0`): fall back to ForEach over Range.
        // (Issue #3551 — though this case is less commonly hit since unit
        // ranges have implicit step=1; explicit step ranges are the primary
        // failure mode.)
        if for_range_has_non_integer_bound(&start, &end, None) {
            let range_span = walker.span(&range_node);
            let iterable = build_range_expr(start, end, None, range_span);
            return Ok(ForBindingResult::Iterable {
                var,
                iterable,
                var_type,
            });
        }
        return Ok(ForBindingResult::Range {
            var,
            start,
            end,
            step: None,
            var_type,
        });
    }

    // If we have an iterable expression (string, array, etc.)
    if let Some(iterable_node) = iterable_node {
        let iterable = lower_expr_maybe_ctx(walker, iterable_node, lambda_ctx)?;
        // Return TupleIterable if we have tuple_vars, otherwise Iterable
        if let Some(vars) = tuple_vars {
            return Ok(ForBindingResult::TupleIterable { vars, iterable });
        } else if let Some(var) = var {
            return Ok(ForBindingResult::Iterable {
                var,
                iterable,
                var_type,
            });
        }
    }

    // Neither range nor iterable found
    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedRange,
        span,
    ))
}

// ==================== Lambda Context Versions ====================

pub fn lower_for_stmt_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    lower_for_stmt_impl(walker, node, Some(lambda_ctx))
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
    use crate::ir::core::{Expr, Stmt};
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
    fn test_for_range() {
        let stmt = lower_first_stmt("for i in 1:10\n  i\nend");
        assert!(
            matches!(stmt, Stmt::For { .. }),
            "Expected For statement, got {:?}",
            stmt
        );
        if let Stmt::For { var, step, .. } = stmt {
            assert_eq!(var, "i");
            assert!(step.is_none(), "Simple range should have no step");
        }
    }

    #[test]
    fn test_for_step_range() {
        let stmt = lower_first_stmt("for i in 1:2:10\n  i\nend");
        assert!(
            matches!(stmt, Stmt::For { .. }),
            "Expected For statement with step, got {:?}",
            stmt
        );
        if let Stmt::For { var, step, .. } = stmt {
            assert_eq!(var, "i");
            assert!(step.is_some(), "Step range should have a step");
        }
    }

    #[test]
    fn test_for_each_string() {
        let stmt = lower_first_stmt("for c in \"hello\"\n  c\nend");
        assert!(
            matches!(stmt, Stmt::ForEach { .. }),
            "Expected ForEach statement, got {:?}",
            stmt
        );
        if let Stmt::ForEach { var, .. } = stmt {
            assert_eq!(var, "c");
        }
    }

    #[test]
    fn test_outer_identifier_for_loop_still_lowers_as_binding() {
        let stmt = lower_first_stmt("for outer in 1:10\n  outer\nend");
        assert!(
            matches!(stmt, Stmt::For { .. }),
            "Expected For statement, got {:?}",
            stmt
        );
        if let Stmt::For { var, .. } = stmt {
            assert_eq!(var, "outer");
        }
    }

    #[test]
    fn test_cartesian_for_desugars_to_nested_loops_issue_6865() {
        // `for x in 1:3, y in 1:3 ... end` becomes `for x in 1:3; for y in 1:3 ... end; end`.
        let stmt = lower_first_stmt("for x in 1:3, y in 1:3\n  x + y\nend");
        let Stmt::For { var, body, .. } = stmt else {
            panic!("Expected outer For statement, got {:?}", stmt);
        };
        assert_eq!(var, "x");
        assert_eq!(
            body.stmts.len(),
            1,
            "outer body should hold one nested loop"
        );
        let inner = &body.stmts[0];
        assert!(
            matches!(inner, Stmt::For { var, .. } if var == "y"),
            "Expected inner For over y, got {:?}",
            inner
        );
    }

    #[test]
    fn test_cartesian_for_mixed_iterables_issue_6865() {
        // Array iterable outside, range inside: ForEach wrapping a For.
        let stmt = lower_first_stmt("for c in [1, 2], k in 1:2\n  c + k\nend");
        let Stmt::ForEach { var, body, .. } = stmt else {
            panic!("Expected outer ForEach statement, got {:?}", stmt);
        };
        assert_eq!(var, "c");
        assert!(
            matches!(&body.stmts[0], Stmt::For { var, .. } if var == "k"),
            "Expected inner For over k, got {:?}",
            body.stmts.first()
        );
    }

    #[test]
    fn test_typed_loop_variable_injects_convert_with_hidden_counter_issue_8208() {
        // `for i::T in itr` desugars to a loop over a *hidden* variable plus a
        // leading `i = convert(T, #hidden)` in the body. The hidden counter keeps
        // the integer fast path's `I64` slot from being clobbered by the convert.
        let stmt = lower_first_stmt("for i::Float64 in 1:3\n  i\nend");
        let Stmt::For { var, body, .. } = stmt else {
            panic!("Expected For statement, got {:?}", stmt);
        };
        assert!(
            var.starts_with("i#fortyped"),
            "Expected a hidden loop counter distinct from `i`, got {:?}",
            var
        );
        let Stmt::Assign {
            var: assigned,
            value,
            ..
        } = &body.stmts[0]
        else {
            panic!(
                "Expected leading convert assignment, got {:?}",
                body.stmts.first()
            );
        };
        assert_eq!(assigned, "i", "convert must bind the user-visible variable");
        let Expr::Call { function, args, .. } = value else {
            panic!("Expected convert call, got {:?}", value);
        };
        assert_eq!(function, "convert");
        // convert(Float64, #hidden): second arg references the hidden counter.
        assert!(
            matches!(&args[1], Expr::Var(name, _) if name == &var),
            "convert must read the hidden counter, got {:?}",
            args.get(1)
        );
    }

    #[test]
    fn test_untyped_loop_variable_has_no_convert_issue_8208() {
        // Regression guard: a plain `for i in 1:3` must NOT inject a convert and
        // must keep `i` as the loop variable (no hidden counter).
        let stmt = lower_first_stmt("for i in 1:3\n  i\nend");
        let Stmt::For { var, body, .. } = stmt else {
            panic!("Expected For statement, got {:?}", stmt);
        };
        assert_eq!(var, "i");
        assert!(
            !matches!(body.stmts.first(), Some(Stmt::Assign { value: Expr::Call { function, .. }, .. }) if function == "convert"),
            "Untyped loop must not inject a convert, got {:?}",
            body.stmts.first()
        );
    }

    #[test]
    fn test_outer_modifier_for_loop_is_rejected_issue_6465() {
        let mut parser = Parser::new().expect("Failed to init parser");
        let source = "for outer i in 1:10\n  i\nend";
        let parse_outcome = parser.parse(source).expect("Failed to parse");
        let mut lowering = Lowering::new(source);
        let err = lowering
            .lower(parse_outcome)
            .expect_err("for outer modifier must be rejected until supported");
        assert_eq!(
            err.kind,
            crate::error::UnsupportedFeatureKind::UnsupportedForBinding
        );
        assert_eq!(
            err.hint.as_deref(),
            Some("no outer local variable declaration exists for \"for outer\"")
        );
    }
}
