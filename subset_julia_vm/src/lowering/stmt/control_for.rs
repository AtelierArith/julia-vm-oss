//! For statement lowering
//!
//! Handles:
//! - For loops with ranges: `for v in start:end ... end`
//! - Step ranges: `for v in start:step:end ... end`
//! - For-each loops over iterables: `for c in "string" ... end`, `for x in arr ... end`

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Expr, Stmt};
use crate::lowering::expr;
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::lower_block;
use super::lower_block_with_ctx;

/// Result of lowering a for binding - either a range or a general iterable
enum ForBindingResult {
    /// Range-based iteration: (var, start, end, step)
    Range {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
    },
    /// General iterable (string, array, tuple, etc.): (var, iterable)
    Iterable { var: String, iterable: Expr },
    /// Tuple destructuring iteration: (vars, iterable)
    /// `for (a, b) in collection`
    TupleIterable { vars: Vec<String>, iterable: Expr },
}

pub fn lower_for_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let bindings: Vec<Node<'a>> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) == NodeKind::ForBinding)
        .collect();

    if bindings.len() != 1 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            span,
        ));
    }

    let binding_result = lower_for_binding(walker, bindings[0])?;

    let body_node = walker
        .named_children(&node)
        .into_iter()
        .find(|child| walker.kind(child) == NodeKind::Block);

    let body = match body_node {
        Some(body_node) => lower_block(walker, body_node)?,
        None => Block {
            stmts: Vec::new(),
            span,
        },
    };

    match binding_result {
        ForBindingResult::Range {
            var,
            start,
            end,
            step,
        } => Ok(Stmt::For {
            var,
            start,
            end,
            step,
            body,
            span,
        }),
        ForBindingResult::Iterable { var, iterable } => Ok(Stmt::ForEach {
            var,
            iterable,
            body,
            span,
        }),
        ForBindingResult::TupleIterable { vars, iterable } => Ok(Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            span,
        }),
    }
}

fn lower_for_binding<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<ForBindingResult> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // Find the variable (identifier or tuple pattern) and the expression after "in"
    let mut var: Option<String> = None;
    let mut tuple_vars: Option<Vec<String>> = None; // For tuple destructuring: (a, b)
    let mut binding_found = false; // Track if we found a binding (var or tuple)
    let mut range_node: Option<Node<'a>> = None;
    let mut iterable_node: Option<Node<'a>> = None;

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
                let start = expr::lower_expr(walker, inner_children[0])?;
                let step = expr::lower_expr(walker, inner_children[1])?;
                let end = expr::lower_expr(walker, range_children[1])?;
                return Ok(ForBindingResult::Range {
                    var,
                    start,
                    end,
                    step: Some(step),
                });
            }
        }

        // Unit range: start:end
        let start = expr::lower_expr(walker, range_children[0])?;
        let end = expr::lower_expr(walker, range_children[1])?;
        return Ok(ForBindingResult::Range {
            var,
            start,
            end,
            step: None,
        });
    }

    // If we have an iterable expression (string, array, etc.)
    if let Some(iterable_node) = iterable_node {
        let iterable = expr::lower_expr(walker, iterable_node)?;
        // Return TupleIterable if we have tuple_vars, otherwise Iterable
        if let Some(vars) = tuple_vars {
            return Ok(ForBindingResult::TupleIterable { vars, iterable });
        } else if let Some(var) = var {
            return Ok(ForBindingResult::Iterable { var, iterable });
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
    let span = walker.span(&node);
    let bindings: Vec<Node<'a>> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) == NodeKind::ForBinding)
        .collect();

    if bindings.len() != 1 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            span,
        ));
    }

    let binding_result = lower_for_binding_with_ctx(walker, bindings[0], lambda_ctx)?;

    let body_node = walker
        .named_children(&node)
        .into_iter()
        .find(|child| walker.kind(child) == NodeKind::Block);

    let body = match body_node {
        Some(body_node) => lower_block_with_ctx(walker, body_node, lambda_ctx)?,
        None => Block {
            stmts: Vec::new(),
            span,
        },
    };

    match binding_result {
        ForBindingResult::Range {
            var,
            start,
            end,
            step,
        } => Ok(Stmt::For {
            var,
            start,
            end,
            step,
            body,
            span,
        }),
        ForBindingResult::Iterable { var, iterable } => Ok(Stmt::ForEach {
            var,
            iterable,
            body,
            span,
        }),
        ForBindingResult::TupleIterable { vars, iterable } => Ok(Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            span,
        }),
    }
}

fn lower_for_binding_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<ForBindingResult> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    let mut var: Option<String> = None;
    let mut tuple_vars: Option<Vec<String>> = None;
    let mut binding_found = false;
    let mut range_node: Option<Node<'a>> = None;
    let mut iterable_node: Option<Node<'a>> = None;

    for child in &named {
        match walker.kind(child) {
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
            NodeKind::Identifier if !binding_found => {
                var = Some(walker.text(child).to_string());
                binding_found = true;
            }
            NodeKind::RangeExpression => {
                range_node = Some(*child);
            }
            NodeKind::Operator => {}
            NodeKind::StringLiteral
            | NodeKind::VectorExpression
            | NodeKind::TupleExpression
            | NodeKind::CallExpression
            | NodeKind::ComprehensionExpression => {
                if binding_found {
                    iterable_node = Some(*child);
                }
            }
            NodeKind::Identifier if binding_found => {
                iterable_node = Some(*child);
            }
            _ => {
                if binding_found && range_node.is_none() {
                    iterable_node = Some(*child);
                }
            }
        }
    }

    if !binding_found {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedForBinding,
            span,
        ));
    }

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

        if walker.kind(&range_children[0]) == NodeKind::RangeExpression {
            let inner_range = range_children[0];
            let inner_children = walker.named_children(&inner_range);
            if inner_children.len() >= 2 {
                let start = expr::lower_expr_with_ctx(walker, inner_children[0], lambda_ctx)?;
                let step = expr::lower_expr_with_ctx(walker, inner_children[1], lambda_ctx)?;
                let end = expr::lower_expr_with_ctx(walker, range_children[1], lambda_ctx)?;
                return Ok(ForBindingResult::Range {
                    var,
                    start,
                    end,
                    step: Some(step),
                });
            }
        }

        let start = expr::lower_expr_with_ctx(walker, range_children[0], lambda_ctx)?;
        let end = expr::lower_expr_with_ctx(walker, range_children[1], lambda_ctx)?;
        return Ok(ForBindingResult::Range {
            var,
            start,
            end,
            step: None,
        });
    }

    if let Some(iterable_node) = iterable_node {
        let iterable = expr::lower_expr_with_ctx(walker, iterable_node, lambda_ctx)?;
        if let Some(vars) = tuple_vars {
            return Ok(ForBindingResult::TupleIterable { vars, iterable });
        } else if let Some(var) = var {
            return Ok(ForBindingResult::Iterable { var, iterable });
        }
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedRange,
        span,
    ))
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
    fn test_for_range() {
        let stmt = lower_first_stmt("for i in 1:10\n  i\nend");
        assert!(matches!(stmt, Stmt::For { .. }), "Expected For statement, got {:?}", stmt);
        if let Stmt::For { var, step, .. } = stmt {
            assert_eq!(var, "i");
            assert!(step.is_none(), "Simple range should have no step");
        }
    }

    #[test]
    fn test_for_step_range() {
        let stmt = lower_first_stmt("for i in 1:2:10\n  i\nend");
        assert!(matches!(stmt, Stmt::For { .. }), "Expected For statement with step, got {:?}", stmt);
        if let Stmt::For { var, step, .. } = stmt {
            assert_eq!(var, "i");
            assert!(step.is_some(), "Step range should have a step");
        }
    }

    #[test]
    fn test_for_each_string() {
        let stmt = lower_first_stmt("for c in \"hello\"\n  c\nend");
        assert!(matches!(stmt, Stmt::ForEach { .. }), "Expected ForEach statement, got {:?}", stmt);
        if let Stmt::ForEach { var, .. } = stmt {
            assert_eq!(var, "c");
        }
    }
}
