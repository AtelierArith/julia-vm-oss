//! Binary and unary expression lowering.
//!
//! This module handles lowering of binary operators, unary operators,
//! and juxtaposition expressions (implicit multiplication).

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, Expr};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::{
    is_broadcast_op, is_comparison_operator, is_flattenable_operator, is_operator_token,
    lower_expr, lower_expr_with_ctx, make_broadcasted_call, map_binary_op, map_unary_op,
    strip_broadcast_dot,
};
use crate::lowering::LambdaContext;

/// Lower binary expression: a + b, a == b, etc.
pub fn lower_binary_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    // Filter out operator nodes - tree-sitter includes them as named children
    let operands: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();
    if operands.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("binary expression".to_string()),
            span,
        ));
    }

    let op_text = extract_operator_text(walker, node).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedOperator("unknown".to_string()),
            span,
        )
    })?;

    // Julia parses chained operators like `a + b + c` as a single multi-argument call `+(a, b, c)`.
    // This is important for method dispatch: if user defines `+(a::Int, b::Int)` (2-argument),
    // then `1 + 2 + 3` should fail because there's no 3-argument method.
    //
    // tree-sitter parses `a + b + c` as nested binary expressions: `((a + b) + c)`
    // We need to flatten these into a single multi-argument call for operators that Julia flattens.
    if is_flattenable_operator(&op_text) {
        let mut all_operands = Vec::new();
        collect_chained_operands(walker, node, &op_text, &mut all_operands);

        if all_operands.len() > 2 {
            // Generate a multi-argument call: +(a, b, c)
            let all_args: Vec<Expr> = all_operands
                .iter()
                .map(|n| lower_expr(walker, *n))
                .collect::<LowerResult<Vec<_>>>()?;
            return Ok(Expr::Call {
                function: op_text,
                args: all_args,
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            });
        }
    }

    // Handle chained comparison operators: a < b < c => (a < b) && (b < c)
    // Julia expands chained comparisons into conjunctions with short-circuit evaluation.
    // The parser gives us nested binary expressions like ((a < b) < c).
    // We need to detect this pattern and expand it properly.
    if is_comparison_operator(&op_text) {
        // Check if this is a chained comparison by looking at the structure
        let mut comparisons = Vec::new();
        let mut operand_nodes = Vec::new();
        collect_chained_comparisons(walker, node, &mut comparisons, &mut operand_nodes);

        if comparisons.len() > 1 {
            // We have a chained comparison like a < b < c
            // Expand to (a < b) && (b < c)
            let lowered_operands: Vec<Expr> = operand_nodes
                .iter()
                .map(|n| lower_expr(walker, *n))
                .collect::<LowerResult<Vec<_>>>()?;

            let mut result: Option<Expr> = None;
            for (i, comp_op) in comparisons.iter().enumerate() {
                let comp_binary_op = map_binary_op(comp_op).ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedOperator(comp_op.clone()),
                        span,
                    )
                })?;

                let comparison = Expr::BinaryOp {
                    op: comp_binary_op,
                    left: Box::new(lowered_operands[i].clone()),
                    right: Box::new(lowered_operands[i + 1].clone()),
                    span,
                };

                result = match result {
                    None => Some(comparison),
                    Some(prev) => Some(Expr::BinaryOp {
                        op: BinaryOp::And,
                        left: Box::new(prev),
                        right: Box::new(comparison),
                        span,
                    }),
                };
            }

            return Ok(result.unwrap());
        }
    }

    let left = lower_expr(walker, operands[0])?;
    let right = lower_expr(walker, operands[1])?;

    // Pair operator: key => value
    // Sometimes parsed as BinaryExpression instead of PairExpression
    if op_text == "=>" {
        return Ok(Expr::Pair {
            key: Box::new(left),
            value: Box::new(right),
            span,
        });
    }

    // Broadcast operators: generate materialize(Broadcasted(op, (left, right))) (Issue #2546)
    if is_broadcast_op(&op_text) {
        let base_op = strip_broadcast_dot(&op_text);
        // Short-circuit operators .&& and .|| must use andand/oror wrapper functions
        // because && and || are not callable as functions (Issue #2545)
        let fn_name = match base_op {
            "&&" => "andand",
            "||" => "oror",
            other => other,
        };
        return Ok(make_broadcasted_call(fn_name, vec![left, right], span));
    }

    // Rational division operator: 1 // 2 => //(1, 2)
    // Julia-compliant: // is a regular function, defined in rational.jl as //(n,d) = Rational(n,d)
    if op_text == "//" {
        return Ok(Expr::Call {
            function: "//".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Integer division operator: a ÷ b => div(a, b)
    // This matches Julia's design where `const ÷ = div`
    if op_text == "÷" {
        return Ok(Expr::Call {
            function: "div".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Pipe operator: x |> f => f(x), x |> f |> g => g(f(x))
    if op_text == "|>" {
        // right should be a callable (identifier or FunctionRef from lambda)
        match right {
            Expr::Var(name, _) => {
                return Ok(Expr::Call {
                    function: name,
                    args: vec![left],
                    kwargs: Vec::new(),
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                });
            }
            Expr::FunctionRef { name, .. } => {
                // x |> (y -> expr) where lambda was lifted to a named function
                return Ok(Expr::Call {
                    function: name,
                    args: vec![left],
                    kwargs: Vec::new(),
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                });
            }
            _ => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "pipe operator requires a function name or lambda on the right side"
                            .to_string(),
                    ),
                    span,
                ));
            }
        }
    }

    // Bit-shift operators: a << b, a >> b, a >>> b
    // Lowered as function calls to Pure Julia wrappers (base/int.jl)
    if op_text == "<<" || op_text == ">>" || op_text == ">>>" {
        return Ok(Expr::Call {
            function: op_text,
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Bitwise operators: a & b, a | b, a ⊻ b (xor)
    // Lowered as function calls to Pure Julia wrappers (base/int.jl)
    if op_text == "&" || op_text == "|" || op_text == "⊻" {
        return Ok(Expr::Call {
            function: op_text,
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Compose operator: f ∘ g => compose(f, g)
    if op_text == "∘" {
        return Ok(Expr::Call {
            function: "compose".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // isa operator: a isa T => isa(a, T)
    if op_text == "isa" {
        return Ok(Expr::Call {
            function: "isa".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Approximate equality: a ≈ b => isapprox(a, b)
    if op_text == "≈" {
        return Ok(Expr::Call {
            function: "isapprox".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Not approximately equal: a ≉ b => !isapprox(a, b)
    if op_text == "≉" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "isapprox".to_string(),
                args: vec![left, right],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // NAND operator: a ⊼ b => nand(a, b)
    if op_text == "⊼" {
        return Ok(Expr::Call {
            function: "nand".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // NOR operator: a ⊽ b => nor(a, b)
    if op_text == "⊽" {
        return Ok(Expr::Call {
            function: "nor".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Identity operator: a ≡ b => a === b (object identity)
    // This is the Unicode equivalent of ===
    if op_text == "≡" {
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(left),
            right: Box::new(right),
            span,
        });
    }

    // Non-identity operator: a ≢ b => !(a === b)
    // Also written as a !== b
    if op_text == "≢" || op_text == "!==" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::BinaryOp {
                op: BinaryOp::Egal,
                left: Box::new(left),
                right: Box::new(right),
                span,
            }),
            span,
        });
    }

    // Supertype operator: A >: B => B <: A (swapped subtype check)
    if op_text == ">:" {
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Subtype,
            left: Box::new(right), // Swap left and right
            right: Box::new(left),
            span,
        });
    }

    // Set operators
    // ⊆ (subset): a ⊆ b => issubset(a, b)
    if op_text == "⊆" {
        return Ok(Expr::Call {
            function: "issubset".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ⊈ (not subset): a ⊈ b => !issubset(a, b)
    if op_text == "⊈" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "issubset".to_string(),
                args: vec![left, right],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // ⊊ (proper subset): a ⊊ b => proper subset check
    if op_text == "⊊" {
        return Ok(Expr::Call {
            function: "issubset_proper".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ⊇ (superset): a ⊇ b => issubset(b, a)
    if op_text == "⊇" {
        return Ok(Expr::Call {
            function: "issubset".to_string(),
            args: vec![right, left], // Swap arguments
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ⊉ (not superset): a ⊉ b => !issubset(b, a)
    if op_text == "⊉" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "issubset".to_string(),
                args: vec![right, left], // Swap arguments
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // ⊋ (proper superset): a ⊋ b => proper superset check
    if op_text == "⊋" {
        return Ok(Expr::Call {
            function: "issuperset_proper".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∈ (in): x ∈ a => in(x, a)
    if op_text == "∈" {
        return Ok(Expr::Call {
            function: "in".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∉ (not in): x ∉ a => !in(x, a)
    if op_text == "∉" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "in".to_string(),
                args: vec![left, right],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // ∋ (contains): a ∋ x => in(x, a)  (reversed arguments)
    if op_text == "∋" {
        return Ok(Expr::Call {
            function: "in".to_string(),
            args: vec![right, left], // Swap arguments
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∌ (not contains): a ∌ x => !in(x, a)
    if op_text == "∌" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "in".to_string(),
                args: vec![right, left], // Swap arguments
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // Left division (backslash): A \ b => \(A, b) which solves Ax = b for x
    // Julia treats \ as a function call with name "\"
    if op_text == "\\" {
        return Ok(Expr::Call {
            function: "\\".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // in operator: x in a => in(x, a)
    if op_text == "in" {
        return Ok(Expr::Call {
            function: "in".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    let op = map_binary_op(&op_text).ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedOperator(op_text), span)
    })?;

    Ok(Expr::BinaryOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
        span,
    })
}

/// Lower binary expression with lambda context: a + b, a == b, etc.
/// This version supports arrow functions and do syntax in arguments.
pub fn lower_binary_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    // Filter out operator nodes - tree-sitter includes them as named children
    let operands: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();
    if operands.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("binary expression".to_string()),
            span,
        ));
    }

    let op_text = extract_operator_text(walker, node).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedOperator("unknown".to_string()),
            span,
        )
    })?;

    // Julia parses chained operators like `a + b + c` as a single multi-argument call `+(a, b, c)`.
    // This is important for method dispatch: if user defines `+(a::Int, b::Int)` (2-argument),
    // then `1 + 2 + 3` should fail because there's no 3-argument method.
    //
    // tree-sitter parses `a + b + c` as nested binary expressions: `((a + b) + c)`
    // We need to flatten these into a single multi-argument call for operators that Julia flattens.
    if is_flattenable_operator(&op_text) {
        let mut all_operands = Vec::new();
        collect_chained_operands(walker, node, &op_text, &mut all_operands);

        if all_operands.len() > 2 {
            // Generate a multi-argument call: +(a, b, c)
            let all_args: Vec<Expr> = all_operands
                .iter()
                .map(|n| lower_expr_with_ctx(walker, *n, lambda_ctx))
                .collect::<LowerResult<Vec<_>>>()?;
            return Ok(Expr::Call {
                function: op_text,
                args: all_args,
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            });
        }
    }

    // Handle chained comparison operators: a < b < c => (a < b) && (b < c)
    // Julia expands chained comparisons into conjunctions with short-circuit evaluation.
    // The parser gives us nested binary expressions like ((a < b) < c).
    // We need to detect this pattern and expand it properly.
    if is_comparison_operator(&op_text) {
        // Check if this is a chained comparison by looking at the structure
        let mut comparisons = Vec::new();
        let mut operand_nodes = Vec::new();
        collect_chained_comparisons(walker, node, &mut comparisons, &mut operand_nodes);

        if comparisons.len() > 1 {
            // We have a chained comparison like a < b < c
            // Expand to (a < b) && (b < c)
            let lowered_operands: Vec<Expr> = operand_nodes
                .iter()
                .map(|n| lower_expr_with_ctx(walker, *n, lambda_ctx))
                .collect::<LowerResult<Vec<_>>>()?;

            let mut result: Option<Expr> = None;
            for (i, comp_op) in comparisons.iter().enumerate() {
                let comp_binary_op = map_binary_op(comp_op).ok_or_else(|| {
                    UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedOperator(comp_op.clone()),
                        span,
                    )
                })?;

                let comparison = Expr::BinaryOp {
                    op: comp_binary_op,
                    left: Box::new(lowered_operands[i].clone()),
                    right: Box::new(lowered_operands[i + 1].clone()),
                    span,
                };

                result = match result {
                    None => Some(comparison),
                    Some(prev) => Some(Expr::BinaryOp {
                        op: BinaryOp::And,
                        left: Box::new(prev),
                        right: Box::new(comparison),
                        span,
                    }),
                };
            }

            return Ok(result.unwrap());
        }
    }

    let left = lower_expr_with_ctx(walker, operands[0], lambda_ctx)?;
    let right = lower_expr_with_ctx(walker, operands[1], lambda_ctx)?;

    // Pair operator: key => value
    // Sometimes parsed as BinaryExpression instead of PairExpression
    if op_text == "=>" {
        return Ok(Expr::Pair {
            key: Box::new(left),
            value: Box::new(right),
            span,
        });
    }

    // Broadcast operators: generate materialize(Broadcasted(op, (left, right))) (Issue #2546)
    if is_broadcast_op(&op_text) {
        let base_op = strip_broadcast_dot(&op_text);
        // Short-circuit operators .&& and .|| must use andand/oror wrapper functions
        // because && and || are not callable as functions (Issue #2545)
        let fn_name = match base_op {
            "&&" => "andand",
            "||" => "oror",
            other => other,
        };
        return Ok(make_broadcasted_call(fn_name, vec![left, right], span));
    }

    // Rational division operator: 1 // 2 => //(1, 2)
    // Julia-compliant: // is a regular function, defined in rational.jl as //(n,d) = Rational(n,d)
    if op_text == "//" {
        return Ok(Expr::Call {
            function: "//".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Integer division operator: a ÷ b => div(a, b)
    // This matches Julia's design where `const ÷ = div`
    if op_text == "÷" {
        return Ok(Expr::Call {
            function: "div".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Pipe operator: x |> f => f(x), x |> f |> g => g(f(x))
    if op_text == "|>" {
        // right should be a callable (identifier or FunctionRef from lambda)
        match right {
            Expr::Var(name, _) => {
                return Ok(Expr::Call {
                    function: name,
                    args: vec![left],
                    kwargs: Vec::new(),
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                });
            }
            Expr::FunctionRef { name, .. } => {
                // x |> (y -> expr) where lambda was lifted to a named function
                return Ok(Expr::Call {
                    function: name,
                    args: vec![left],
                    kwargs: Vec::new(),
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                });
            }
            _ => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "pipe operator requires a function name or lambda on the right side"
                            .to_string(),
                    ),
                    span,
                ));
            }
        }
    }

    // Bit-shift operators: a << b, a >> b, a >>> b
    // Lowered as function calls to Pure Julia wrappers (base/int.jl)
    if op_text == "<<" || op_text == ">>" || op_text == ">>>" {
        return Ok(Expr::Call {
            function: op_text,
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Bitwise operators: a & b, a | b, a ⊻ b (xor)
    // Lowered as function calls to Pure Julia wrappers (base/int.jl)
    if op_text == "&" || op_text == "|" || op_text == "⊻" {
        return Ok(Expr::Call {
            function: op_text,
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Compose operator: f ∘ g => compose(f, g)
    if op_text == "∘" {
        return Ok(Expr::Call {
            function: "compose".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // isa operator: a isa T => isa(a, T)
    if op_text == "isa" {
        return Ok(Expr::Call {
            function: "isa".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Approximate equality: a ≈ b => isapprox(a, b)
    if op_text == "≈" {
        return Ok(Expr::Call {
            function: "isapprox".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Not approximately equal: a ≉ b => !isapprox(a, b)
    if op_text == "≉" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "isapprox".to_string(),
                args: vec![left, right],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // NAND operator: a ⊼ b => nand(a, b)
    if op_text == "⊼" {
        return Ok(Expr::Call {
            function: "nand".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // NOR operator: a ⊽ b => nor(a, b)
    if op_text == "⊽" {
        return Ok(Expr::Call {
            function: "nor".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Identity operator: a ≡ b => a === b (object identity)
    // This is the Unicode equivalent of ===
    if op_text == "≡" {
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(left),
            right: Box::new(right),
            span,
        });
    }

    // Non-identity operator: a ≢ b => !(a === b)
    // Also written as a !== b
    if op_text == "≢" || op_text == "!==" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::BinaryOp {
                op: BinaryOp::Egal,
                left: Box::new(left),
                right: Box::new(right),
                span,
            }),
            span,
        });
    }

    // Supertype operator: A >: B => B <: A (swapped subtype check)
    if op_text == ">:" {
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Subtype,
            left: Box::new(right), // Swap left and right
            right: Box::new(left),
            span,
        });
    }

    // Set operators
    // ⊆ (subset): a ⊆ b => issubset(a, b)
    if op_text == "⊆" {
        return Ok(Expr::Call {
            function: "issubset".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ⊈ (not subset): a ⊈ b => !issubset(a, b)
    if op_text == "⊈" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "issubset".to_string(),
                args: vec![left, right],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // ⊊ (proper subset): a ⊊ b => proper subset check
    if op_text == "⊊" {
        return Ok(Expr::Call {
            function: "issubset_proper".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ⊇ (superset): a ⊇ b => issubset(b, a)
    if op_text == "⊇" {
        return Ok(Expr::Call {
            function: "issubset".to_string(),
            args: vec![right, left], // Swap arguments
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ⊉ (not superset): a ⊉ b => !issubset(b, a)
    if op_text == "⊉" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "issubset".to_string(),
                args: vec![right, left], // Swap arguments
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // ⊋ (proper superset): a ⊋ b => proper superset check
    if op_text == "⊋" {
        return Ok(Expr::Call {
            function: "issuperset_proper".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∈ (in): x ∈ a => in(x, a)
    if op_text == "∈" {
        return Ok(Expr::Call {
            function: "in".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∉ (not in): x ∉ a => !in(x, a)
    if op_text == "∉" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "in".to_string(),
                args: vec![left, right],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // ∋ (contains): a ∋ x => in(x, a)  (reversed arguments)
    if op_text == "∋" {
        return Ok(Expr::Call {
            function: "in".to_string(),
            args: vec![right, left], // Swap arguments
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∌ (not contains): a ∌ x => !in(x, a)
    if op_text == "∌" {
        return Ok(Expr::UnaryOp {
            op: crate::ir::core::UnaryOp::Not,
            operand: Box::new(Expr::Call {
                function: "in".to_string(),
                args: vec![right, left], // Swap arguments
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }),
            span,
        });
    }

    // Left division (backslash): A \ b => \(A, b) which solves Ax = b for x
    // Julia treats \ as a function call with name "\"
    if op_text == "\\" {
        return Ok(Expr::Call {
            function: "\\".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // in operator: x in a => in(x, a)
    if op_text == "in" {
        return Ok(Expr::Call {
            function: "in".to_string(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    let op = map_binary_op(&op_text).ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedOperator(op_text), span)
    })?;

    Ok(Expr::BinaryOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
        span,
    })
}

/// Lower juxtaposition expression (2x => 2 * x) as implicit multiplication
pub fn lower_juxtaposition_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    if children.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("juxtaposition expression".to_string()),
            span,
        ));
    }

    // Juxtaposition is implicit multiplication: 2x => 2 * x
    let left = lower_expr(walker, children[0])?;
    let right = lower_expr(walker, children[1])?;

    Ok(Expr::BinaryOp {
        op: BinaryOp::Mul,
        left: Box::new(left),
        right: Box::new(right),
        span,
    })
}

/// Lower unary expression: -x, !x, +x
pub fn lower_unary_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    // Filter out operator nodes - tree-sitter includes them as named children
    let mut operands: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();
    if operands.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("unary expression".to_string()),
            span,
        ));
    }

    let op_text = extract_operator_text(walker, node).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedOperator("unknown".to_string()),
            span,
        )
    })?;

    let operand = lower_expr(walker, operands.remove(operands.len() - 1))?;

    // Broadcast NOT (.!) is represented as a Call expression
    if op_text == ".!" {
        return Ok(Expr::Call {
            function: ".!".to_string(),
            args: vec![operand],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Unicode root operators: √x => sqrt(x), ∛x => cbrt(x), ∜x => fourthroot(x)
    if op_text == "√" {
        return Ok(Expr::Call {
            function: "sqrt".to_string(),
            args: vec![operand],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }
    if op_text == "∛" {
        return Ok(Expr::Call {
            function: "cbrt".to_string(),
            args: vec![operand],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }
    if op_text == "∜" {
        return Ok(Expr::Call {
            function: "fourthroot".to_string(),
            args: vec![operand],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Bitwise NOT: ~x => function call to ~
    if op_text == "~" {
        return Ok(Expr::Call {
            function: "~".to_string(),
            args: vec![operand],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    let op = map_unary_op(&op_text).ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedOperator(op_text), span)
    })?;

    Ok(Expr::UnaryOp {
        op,
        operand: Box::new(operand),
        span,
    })
}

/// Recursively collect all operands from chained same-operator binary expressions.
/// For example, `((a + b) + c) + d` with operator `+` collects [a, b, c, d].
fn collect_chained_operands<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    target_op: &str,
    operands: &mut Vec<Node<'a>>,
) {
    // Check if this node is a binary expression with the same operator
    if walker.kind(&node) != NodeKind::BinaryExpression {
        operands.push(node);
        return;
    }

    let node_op = extract_operator_text(walker, node);
    if node_op.as_deref() != Some(target_op) {
        // Different operator, treat as a single operand
        operands.push(node);
        return;
    }

    // Same operator, recursively collect from children
    let children: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if children.len() >= 2 {
        // Recursively process left operand (may be another chained expression)
        collect_chained_operands(walker, children[0], target_op, operands);
        // Right operand: also check if it's a chain of the same operator
        collect_chained_operands(walker, children[1], target_op, operands);
    }
}

/// Recursively collect all comparison operators and operands from chained comparisons.
/// For example, `a < b <= c` (parsed as `((a < b) <= c)`) collects:
/// - comparisons: ["<", "<="]
/// - operands: [a, b, c]
/// This allows us to expand to `(a < b) && (b <= c)`.
fn collect_chained_comparisons<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    comparisons: &mut Vec<String>,
    operands: &mut Vec<Node<'a>>,
) {
    // Check if this node is a binary expression
    if walker.kind(&node) != NodeKind::BinaryExpression {
        operands.push(node);
        return;
    }

    let node_op = extract_operator_text(walker, node);
    let op_str = match node_op {
        Some(ref op) if is_comparison_operator(op) => op.clone(),
        _ => {
            // Not a comparison operator, treat as a single operand
            operands.push(node);
            return;
        }
    };

    // Get left and right children
    let children: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if children.len() >= 2 {
        // Recursively process left operand (may be another chained comparison)
        collect_chained_comparisons(walker, children[0], comparisons, operands);
        // Add this operator
        comparisons.push(op_str);
        // Right operand is a leaf (not recursed because chained comparisons are left-associative)
        operands.push(children[1]);
    }
}

/// Extract operator text from a binary or unary expression node.
fn extract_operator_text<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    for child in walker.children(&node) {
        let kind = child.kind();
        if kind == "operator" || is_operator_token(kind) {
            return Some(walker.text(&child).to_string());
        }
    }
    None
}
