//! Binary and unary expression lowering.
//!
//! This module handles lowering of binary operators, unary operators,
//! and juxtaposition expressions (implicit multiplication).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, Block, Expr, Literal, Stmt};
use crate::lowering::{internal_lowering_error, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};

use super::{
    is_broadcast_op, is_chainable_comparison_operator, is_flattenable_operator, is_operator_token,
    lower_expr, lower_expr_with_ctx, make_broadcasted_call, map_binary_op, map_unary_op,
    strip_broadcast_dot,
};
use crate::lowering::LambdaContext;

fn lower_pipe_operator(left: Expr, right: Expr, span: crate::span::Span) -> Expr {
    match right {
        Expr::Var(name, _) => Expr::Call {
            function: name.to_string().into(),
            args: vec![left],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        },
        callable_expr => {
            let arg_temp = format!("__pipe_arg_{}_{}", span.start, span.end);
            let func_temp = format!("__pipe_func_{}_{}", span.start, span.end);
            let call_expr = Expr::Call {
                function: func_temp.clone().into(),
                args: vec![Expr::Var(arg_temp.clone().into(), span)],
                kwargs: Vec::new(),
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            };

            Expr::LetBlock {
                bindings: vec![(arg_temp.into(), left), (func_temp.into(), callable_expr)],
                body: Block {
                    stmts: vec![Stmt::Expr {
                        expr: call_expr,
                        span,
                    }],
                    span,
                },
                span,
            }
        }
    }
}

/// Lower a binary expression whose operator the builtin table does not know.
///
/// Issue #11023: in upstream Julia every NON-syntactic operator is an ordinary
/// function name, so a user-defined (custom Unicode) operator may be used infix
/// exactly as it was defined — `⊗(a, b) = a * b + 1; 1 ⊗ 2` evaluates to `3`.
/// Lower it to the same `Expr::Call` the prefix spelling `⊗(1, 2)` produces, so
/// both spellings share one function/method identity and dispatch path (the
/// call-target half is Issue #10933's `is_syntactic_operator`, reused here as
/// the single authority). A genuinely SYNTACTIC operator (`&&`, `::`, `=`,
/// `<:`, …) is not a function and keeps erroring as before.
fn lower_custom_operator_call(
    op_text: String,
    left: Expr,
    right: Expr,
    span: crate::parser::span::Span,
) -> LowerResult<Expr> {
    if crate::lowering::expr::call::is_syntactic_operator(&op_text) {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedOperator(op_text),
            span,
        ));
    }
    Ok(Expr::Call {
        function: op_text.into(),
        args: vec![left, right],
        kwargs: Vec::new(),
        splat_mask: vec![false, false],
        kwargs_splat_mask: Vec::new(),
        span,
    })
}

/// Lower binary expression: a + b, a == b, etc.
pub fn lower_binary_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let op_text = extract_operator_text(walker, node).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedOperator("unknown".to_string()),
            span,
        )
    })?;

    // Pair operator: key => value. Do this before the generic operator filter so
    // a bare operator value (`:f => +`) is preserved as the RHS (Issue #6461).
    if op_text == "=>" {
        let (left_node, right_node) = pair_operand_nodes(walker, node, span)?;
        let left = lower_expr(walker, left_node)?;
        let right = lower_expr(walker, right_node)?;
        return Ok(Expr::Pair {
            key: Box::new(left),
            value: Box::new(right),
            span,
        });
    }

    // Filter out operator nodes - tree-sitter includes them as named children
    let operands: Vec<_> = walker
        .named_children(&node)
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();
    if operands.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("binary expression".to_string()),
            span,
        ));
    }

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
                function: op_text.into(),
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
    // We need to detect this pattern and expand it properly. Dotted (broadcast)
    // comparisons chain too (Issue #9300): `a .< b .< c` fuses into a
    // broadcasted `&`, not a scalar `&&`.
    if is_chainable_comparison_operator(&op_text) {
        // Check if this is a chained comparison by looking at the structure
        let mut comparisons = Vec::new();
        let mut operand_nodes = Vec::new();
        collect_chained_comparisons(walker, node, &mut comparisons, &mut operand_nodes);

        if comparisons.len() > 1 {
            // We have a chained comparison like a < b < c
            let lowered_operands: Vec<Expr> = operand_nodes
                .iter()
                .map(|n| lower_expr(walker, *n))
                .collect::<LowerResult<Vec<_>>>()?;

            return build_comparison_chain(&comparisons, lowered_operands, span);
        }
    }

    let left = lower_expr(walker, operands[0])?;
    let right = lower_expr(walker, operands[1])?;

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
            function: "//".to_string().into(),
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
            function: "div".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Pipe operator: x |> f => f(x), x |> f |> g => g(f(x))
    if op_text == "|>" {
        return Ok(lower_pipe_operator(left, right, span));
    }

    // Bit-shift operators: a << b, a >> b, a >>> b
    // Lowered as function calls to Pure Julia wrappers (base/int.jl)
    if op_text == "<<" || op_text == ">>" || op_text == ">>>" {
        return Ok(Expr::Call {
            function: op_text.into(),
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
            function: op_text.into(),
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
            function: "compose".to_string().into(),
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
            function: "isa".to_string().into(),
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
            function: "isapprox".to_string().into(),
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
                function: "isapprox".to_string().into(),
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
            function: "nand".to_string().into(),
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
            function: "nor".to_string().into(),
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
            function: "issubset".to_string().into(),
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
                function: "issubset".to_string().into(),
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
            function: "issubset_proper".to_string().into(),
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
            function: "issubset".to_string().into(),
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
                function: "issubset".to_string().into(),
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
            function: "issuperset_proper".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∈ (in): x ∈ a => ∈(x, a)
    if op_text == "∈" {
        return Ok(Expr::Call {
            function: "∈".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∉ (not in): x ∉ a => ∉(x, a)
    if op_text == "∉" {
        return Ok(Expr::Call {
            function: "∉".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∋ (contains): a ∋ x => ∋(a, x)
    if op_text == "∋" {
        return Ok(Expr::Call {
            function: "∋".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∌ (not contains): a ∌ x => ∌(a, x)
    if op_text == "∌" {
        return Ok(Expr::Call {
            function: "∌".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Left division (backslash): A \ b => \(A, b) which solves Ax = b for x
    // Julia treats \ as a function call with name "\"
    if op_text == "\\" {
        return Ok(Expr::Call {
            function: "\\".to_string().into(),
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
            function: "in".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // `x ^ <literal negative integer>` routes through `Base.literal_pow`
    // (Issue #7233) so e.g. `2^-3 == 0.125` rather than throwing a DomainError.
    if let Some(call) = try_lower_literal_negative_pow(walker, &op_text, &left, operands[1], span) {
        return Ok(call);
    }

    // Issue #11023: an unknown operator is a user-defined function used infix.
    let Some(op) = map_binary_op(&op_text) else {
        return lower_custom_operator_call(op_text, left, right, span);
    };

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
    let op_text = extract_operator_text(walker, node).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedOperator("unknown".to_string()),
            span,
        )
    })?;

    // Pair operator: key => value. Do this before the generic operator filter so
    // a bare operator value (`:f => +`) is preserved as the RHS (Issue #6461).
    if op_text == "=>" {
        let (left_node, right_node) = pair_operand_nodes(walker, node, span)?;
        let left = lower_expr_with_ctx(walker, left_node, lambda_ctx)?;
        let right = lower_expr_with_ctx(walker, right_node, lambda_ctx)?;
        return Ok(Expr::Pair {
            key: Box::new(left),
            value: Box::new(right),
            span,
        });
    }

    // Filter out operator nodes - tree-sitter includes them as named children
    let operands: Vec<_> = walker
        .named_children(&node)
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();
    if operands.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("binary expression".to_string()),
            span,
        ));
    }

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
                function: op_text.into(),
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
    // We need to detect this pattern and expand it properly. Dotted (broadcast)
    // comparisons chain too (Issue #9300): `a .< b .< c` fuses into a
    // broadcasted `&`, not a scalar `&&`.
    if is_chainable_comparison_operator(&op_text) {
        // Check if this is a chained comparison by looking at the structure
        let mut comparisons = Vec::new();
        let mut operand_nodes = Vec::new();
        collect_chained_comparisons(walker, node, &mut comparisons, &mut operand_nodes);

        if comparisons.len() > 1 {
            // We have a chained comparison like a < b < c
            let lowered_operands: Vec<Expr> = operand_nodes
                .iter()
                .map(|n| lower_expr_with_ctx(walker, *n, lambda_ctx))
                .collect::<LowerResult<Vec<_>>>()?;

            return build_comparison_chain(&comparisons, lowered_operands, span);
        }
    }

    let left = lower_expr_with_ctx(walker, operands[0], lambda_ctx)?;
    let right = lower_expr_with_ctx(walker, operands[1], lambda_ctx)?;

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
            function: "//".to_string().into(),
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
            function: "div".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Pipe operator: x |> f => f(x), x |> f |> g => g(f(x))
    if op_text == "|>" {
        return Ok(lower_pipe_operator(left, right, span));
    }

    // Bit-shift operators: a << b, a >> b, a >>> b
    // Lowered as function calls to Pure Julia wrappers (base/int.jl)
    if op_text == "<<" || op_text == ">>" || op_text == ">>>" {
        return Ok(Expr::Call {
            function: op_text.into(),
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
            function: op_text.into(),
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
            function: "compose".to_string().into(),
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
            function: "isa".to_string().into(),
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
            function: "isapprox".to_string().into(),
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
                function: "isapprox".to_string().into(),
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
            function: "nand".to_string().into(),
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
            function: "nor".to_string().into(),
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
            function: "issubset".to_string().into(),
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
                function: "issubset".to_string().into(),
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
            function: "issubset_proper".to_string().into(),
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
            function: "issubset".to_string().into(),
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
                function: "issubset".to_string().into(),
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
            function: "issuperset_proper".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∈ (in): x ∈ a => ∈(x, a)
    if op_text == "∈" {
        return Ok(Expr::Call {
            function: "∈".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∉ (not in): x ∉ a => ∉(x, a)
    if op_text == "∉" {
        return Ok(Expr::Call {
            function: "∉".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∋ (contains): a ∋ x => ∋(a, x)
    if op_text == "∋" {
        return Ok(Expr::Call {
            function: "∋".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // ∌ (not contains): a ∌ x => ∌(a, x)
    if op_text == "∌" {
        return Ok(Expr::Call {
            function: "∌".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // Left division (backslash): A \ b => \(A, b) which solves Ax = b for x
    // Julia treats \ as a function call with name "\"
    if op_text == "\\" {
        return Ok(Expr::Call {
            function: "\\".to_string().into(),
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
            function: "in".to_string().into(),
            args: vec![left, right],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }

    // `x ^ <literal negative integer>` routes through `Base.literal_pow`
    // (Issue #7233) so e.g. `2^-3 == 0.125` rather than throwing a DomainError.
    if let Some(call) = try_lower_literal_negative_pow(walker, &op_text, &left, operands[1], span) {
        return Ok(call);
    }

    // Issue #11023: an unknown operator is a user-defined function used infix.
    let Some(op) = map_binary_op(&op_text) else {
        return lower_custom_operator_call(op_text, left, right, span);
    };

    Ok(Expr::BinaryOp {
        op,
        left: Box::new(left),
        right: Box::new(right),
        span,
    })
}

/// Route `x ^ <literal negative integer>` through `Base.literal_pow`, matching
/// upstream Julia where a literal integer exponent `p` lowers to
/// `Base.literal_pow(^, x, Val(p))` so e.g. `2^-3 == 0.125` instead of throwing
/// a `DomainError` (Issue #7233).
///
/// Only a *literal* negative integer exponent is intercepted: the right operand
/// must be a unary `-` applied directly to a decimal `IntegerLiteral`. A
/// non-literal negative exponent (`n = -3; 2^n`) keeps the ordinary `Pow` path,
/// which throws `DomainError` for integer bases just like upstream. Positive
/// literal exponents (`x^2`) are likewise left on the fast `Pow` path; only the
/// previously-erroring negative case is redirected.
///
/// `right_node` is the (not-yet-lowered) right operand CST node; `left` is the
/// already-lowered base expression. Returns `Ok(None)` when the exponent is not
/// a literal negative integer so the caller falls back to `BinaryOp::Pow`.
fn try_lower_literal_negative_pow(
    walker: &CstWalker<'_>,
    op_text: &str,
    left: &Expr,
    right_node: Node<'_>,
    span: crate::span::Span,
) -> Option<Expr> {
    if op_text != "^" {
        return None;
    }
    let exponent = literal_negative_integer_exponent(walker, right_node)?;

    // Build `Base.literal_pow(^, left, exponent)`, mirroring upstream's lowering
    // of literal-integer powers. The exponent is passed as a plain integer
    // literal (rather than upstream's `Val(p)`) because sjulia does not reliably
    // recover a `Val{p}` type-parameter as a *value* for use in arithmetic
    // inside the method body; a plain `Integer` argument dispatches and computes
    // identically here.
    let pow_fn = Expr::Var("^".to_string().into(), span);
    Some(Expr::Call {
        function: "literal_pow".to_string().into(),
        args: vec![
            pow_fn,
            left.clone(),
            Expr::Literal(Literal::Int(exponent), span),
        ],
        kwargs: Vec::new(),
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    })
}

/// If `node` is a unary `-` applied directly to a decimal `IntegerLiteral`,
/// return the (negative) `i64` value of that exponent. Otherwise `None`.
fn literal_negative_integer_exponent(walker: &CstWalker<'_>, node: Node<'_>) -> Option<i64> {
    if walker.kind(&node) != NodeKind::UnaryExpression {
        return None;
    }
    let children = walker.named_children_vec(&node);
    // Operand is the single non-operator child; the operator must be `-`.
    let mut op_text = None;
    let mut operand = None;
    for child in &children {
        if walker.kind(child) == NodeKind::Operator {
            op_text = Some(walker.text(child).to_string());
        } else if operand.is_none() {
            operand = Some(*child);
        } else {
            // More than one operand: not a simple unary minus on a literal.
            return None;
        }
    }
    if op_text.as_deref() != Some("-") {
        return None;
    }
    let operand = operand?;
    if walker.kind(&operand) != NodeKind::IntegerLiteral {
        return None;
    }
    let magnitude: i64 = walker.text(&operand).replace('_', "").parse().ok()?;
    magnitude.checked_neg()
}

/// Lower juxtaposition expression (2x => 2 * x) as implicit multiplication
pub fn lower_juxtaposition_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

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
    lower_unary_expr_impl(walker, node, None)
}

pub fn lower_unary_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_unary_expr_impl(walker, node, Some(lambda_ctx))
}

fn lower_unary_expr_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    // Filter out operator nodes - tree-sitter includes them as named children
    let mut operands: Vec<_> = walker
        .named_children(&node)
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

    if op_text == "$" {
        // MacroTools @q templates can route quoted interpolation nodes through
        // unary lowering after nested macro substitution. Preserve the intended
        // inner expression for the forms needed by shortdef patterns (Issue #7541).
        if let Some(inner) = operands.last() {
            if matches!(
                walker.kind(inner),
                NodeKind::Identifier | NodeKind::FieldExpression
            ) {
                return match lambda_ctx {
                    Some(ctx) => lower_expr_with_ctx(walker, *inner, ctx),
                    None => lower_expr(walker, *inner),
                };
            }
            if walker.kind(inner) == NodeKind::ParenthesizedExpression {
                let mut paren_children = walker.named_children(inner);
                if let Some(paren_inner) = paren_children.next() {
                    if walker.kind(&paren_inner) == NodeKind::SplatExpression {
                        let mut splat_children = walker.named_children(&paren_inner);
                        if let Some(splat_inner) = splat_children.next() {
                            return match lambda_ctx {
                                Some(ctx) => lower_expr_with_ctx(walker, splat_inner, ctx),
                                None => lower_expr(walker, splat_inner),
                            };
                        }
                    }
                }
            }
        }
    }

    // `@generated` syntactic-unquote interpolation (Issues #5934 / #5936).
    //
    // While lowering a `@generated` quote's inner expression as plain code, a
    // `$ident` or `$(expr)` must resolve to the bound type/value expression
    // that upstream Julia would splice during staging. Gated on the
    // generated-unquote thread-local flag so `$` written outside a quote keeps
    // erroring as `UnsupportedOperator("$")`, matching Julia's
    // "`$` expression outside quote" syntax error.
    //
    // SCOPE: expression splicing only. `$(esc(...))` hygiene and `$(p...)`
    // splat forms still require the real staging engine.
    if op_text == "$" && crate::lowering::generated_unquote::is_active() {
        if let Some(inner) = operands.last() {
            match walker.kind(inner) {
                NodeKind::Identifier => {
                    let name = walker.text(inner);
                    return Ok(Expr::Var(name.to_string().into(), span));
                }
                NodeKind::ParenthesizedExpression => {
                    return match lambda_ctx {
                        Some(ctx) => lower_expr_with_ctx(walker, *inner, ctx),
                        None => lower_expr(walker, *inner),
                    };
                }
                _ => {}
            }
        }
    }

    // `+(1, 2, 3)`, `+(xs...)`, `-(0, xs...)`: the unary-capable operators `+`
    // and `-` followed by parentheses are parsed by tree-sitter as a unary
    // expression whose operand is a tuple (multi-arg) or a parenthesized splat,
    // never as a `CallExpression`. Detect those shapes and lower them as a real
    // operator-function call so dispatch and splat expansion match Julia
    // (Issue #5144). A bare single-argument paren (`+(1)` == `+1`) is left as a
    // unary op below — both lower to the same value.
    let operand_node = operands[operands.len() - 1];
    if let Some(call) =
        try_lower_operator_function_paren_call(walker, &op_text, operand_node, span)?
    {
        return Ok(call);
    }

    let operand_node = operands.remove(operands.len() - 1);
    let operand = match lambda_ctx {
        Some(ctx) => lower_expr_with_ctx(walker, operand_node, ctx)?,
        None => lower_expr(walker, operand_node)?,
    };

    // Broadcast NOT (.!) is represented as a Call expression
    if op_text == ".!" {
        return Ok(Expr::Call {
            function: ".!".to_string().into(),
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
            function: "sqrt".to_string().into(),
            args: vec![operand],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }
    if op_text == "∛" {
        return Ok(Expr::Call {
            function: "cbrt".to_string().into(),
            args: vec![operand],
            kwargs: Vec::new(),
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        });
    }
    if op_text == "∜" {
        return Ok(Expr::Call {
            function: "fourthroot".to_string().into(),
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
            function: "~".to_string().into(),
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

/// Detect and lower an operator-function call written with the unary-capable
/// operators `+`/`-` directly followed by parentheses (Issue #5144).
///
/// tree-sitter parses these as a `UnaryExpression` whose operand is either:
/// - a `TupleExpression` — `+(1, 2, 3)`, `-(0, xs...)` (two or more comma
///   separated items, any of which may be a splat); or
/// - a `ParenthesizedExpression` wrapping a single `SplatExpression` —
///   `+(xs...)`.
///
/// Both are real function calls in Julia, so they are lowered to
/// `Expr::Call { function: op, .. }` with the proper positional splat mask.
/// Any other operand shape (a plain value, e.g. `+(1)` == `+1`, or a non
/// `+`/`-` operator) returns `Ok(None)` so the caller falls back to the normal
/// unary-op lowering.
fn try_lower_operator_function_paren_call<'a>(
    walker: &CstWalker<'a>,
    op_text: &str,
    operand_node: Node<'a>,
    span: crate::span::Span,
) -> LowerResult<Option<Expr>> {
    if !super::is_operator_function_call_target(op_text) {
        return Ok(None);
    }

    let (mut args, mut splat_mask): (Vec<Expr>, Vec<bool>) = (Vec::new(), Vec::new());

    match walker.kind(&operand_node) {
        NodeKind::TupleExpression => {
            for child in walker.named_children(&operand_node) {
                lower_operator_call_argument(walker, child, &mut args, &mut splat_mask)?;
            }
        }
        NodeKind::ParenthesizedExpression => {
            let children = walker.named_children_vec(&operand_node);
            // Only a single splatted child is unambiguously a call: `+(xs...)`.
            // A single plain child is just `+(value)` (== `+value`); leave it
            // to the unary-op path.
            if children.len() == 1 && walker.kind(&children[0]) == NodeKind::SplatExpression {
                lower_operator_call_argument(walker, children[0], &mut args, &mut splat_mask)?;
            } else {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }

    if args.is_empty() {
        return Ok(None);
    }

    Ok(Some(Expr::Call {
        function: op_text.to_string().into(),
        args,
        kwargs: Vec::new(),
        splat_mask,
        kwargs_splat_mask: Vec::new(),
        span,
    }))
}

/// Lower one positional argument of an operator-function call, recording whether
/// it is splatted (Issue #5144). A `SplatExpression` (`xs...`) contributes its
/// inner expression with `splat_mask = true`; any other node is a plain
/// positional argument.
fn lower_operator_call_argument<'a>(
    walker: &CstWalker<'a>,
    child: Node<'a>,
    args: &mut Vec<Expr>,
    splat_mask: &mut Vec<bool>,
) -> LowerResult<()> {
    if walker.kind(&child) == NodeKind::SplatExpression {
        if let Some(inner) = walker.named_children(&child).next() {
            args.push(lower_expr(walker, inner)?);
            splat_mask.push(true);
        }
    } else {
        args.push(lower_expr(walker, child)?);
        splat_mask.push(false);
    }
    Ok(())
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
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if children.len() >= 2 {
        // Recursively process left operand (may be another chained expression)
        collect_chained_operands(walker, children[0], target_op, operands);
        // Right operand: also check if it's a chain of the same operator
        collect_chained_operands(walker, children[1], target_op, operands);
    }
}

/// Expand a collected comparison chain into the correct combined expression.
///
/// `comparisons[i]` is the operator between `lowered_operands[i]` and
/// `lowered_operands[i + 1]`.
///
/// - A chain with only *scalar* comparison operators expands to a short-circuit
///   `&&` conjunction. Non-atomic interior operands are bound to temporaries at
///   the point where their first adjacent link is evaluated, matching upstream's
///   `compare-one` ssavalue expansion (Issue #9375).
/// - A chain containing at least one *dotted* (broadcast) comparison expands to
///   the broadcast-fused `&` form, matching upstream's `expand-vector-compare`
///   (`julia/src/julia-syntax.scm`): each comparison becomes its own link (scalar
///   links stay scalar `Bool`s, dotted links become `broadcasted` calls),
///   consecutive scalar links are grouped with `&&`, and the resulting groups are
///   joined with a broadcasted `&`. This is what makes `0 .<= v .< 1` evaluate to
///   `(0 .<= v) .& (v .< 1)` instead of the wrong left-associative
///   `(0 .<= v) .< 1`, which collapses a `Bool` array to all-`false` (Issue #9300).
///
/// Interior operands appear in two adjacent links. When such an operand is not
/// atomic (e.g. `rand(3)`) it must be evaluated exactly once, so it is bound to a
/// temporary via a `let` block — mirroring upstream's use of an ssavalue in
/// `compare-one`. Without this, `0 .<= rand(3) .< 1` would draw two independent
/// arrays and compare the wrong values.
fn build_comparison_chain(
    comparisons: &[String],
    lowered_operands: Vec<Expr>,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let has_dot = comparisons.iter().any(|op| is_broadcast_op(op));

    if !has_dot {
        return build_scalar_comparison_chain(comparisons, &lowered_operands, span);
    }

    // Vector chain: bind non-atomic interior operands to temporaries so each is
    // evaluated exactly once, then reference the temporary in both adjacent links.
    let mut operand_exprs = lowered_operands;
    let mut bindings: Vec<(crate::ir::core::InternedStr, Expr)> = Vec::new();
    // Interior operands are indices `1..len-1`; each appears in two adjacent links.
    let last_operand = operand_exprs.len().saturating_sub(1);
    for (i, operand) in operand_exprs
        .iter_mut()
        .enumerate()
        .take(last_operand)
        .skip(1)
    {
        if !is_atomic_operand(operand) {
            let tmp = format!("__cmpchain_{}_{}_{}", span.start, span.end, i);
            let orig = std::mem::replace(operand, Expr::Var(tmp.clone().into(), span));
            bindings.push((tmp.into(), orig));
        }
    }

    // Build groups matching upstream `expand-vector-compare`: a maximal run of
    // consecutive scalar comparisons becomes one `&&`-joined group; each dotted
    // comparison is its own broadcasted group.
    let mut groups: Vec<Expr> = Vec::new();
    let mut i = 0usize;
    while i < comparisons.len() {
        if is_broadcast_op(&comparisons[i]) {
            let base_op = strip_broadcast_dot(&comparisons[i]);
            groups.push(make_broadcasted_call(
                base_op,
                vec![operand_exprs[i].clone(), operand_exprs[i + 1].clone()],
                span,
            ));
            i += 1;
        } else {
            let mut run: Option<Expr> = None;
            while i < comparisons.len() && !is_broadcast_op(&comparisons[i]) {
                let link = build_chain_link(
                    &comparisons[i],
                    operand_exprs[i].clone(),
                    operand_exprs[i + 1].clone(),
                    span,
                )?;
                run = Some(match run {
                    None => link,
                    Some(prev) => Expr::BinaryOp {
                        op: BinaryOp::And,
                        left: Box::new(prev),
                        right: Box::new(link),
                        span,
                    },
                });
                i += 1;
            }
            groups.push(run.ok_or_else(|| {
                internal_lowering_error(
                    span,
                    "comparison chain run: inner while loop always executes at least once here",
                )
            })?);
        }
    }

    // Join groups with a broadcasted `&`. `make_broadcasted_call` fuses adjacent
    // broadcasts (stripping the inner `materialize`), so the whole chain becomes a
    // single fused `materialize(Broadcasted(&, ...))`.
    let mut groups_iter = groups.into_iter();
    let mut combined = groups_iter.next().ok_or_else(|| {
        internal_lowering_error(span, "a dotted chain has at least two comparison groups")
    })?;
    for g in groups_iter {
        combined = make_broadcasted_call("&", vec![combined, g], span);
    }

    if bindings.is_empty() {
        Ok(combined)
    } else {
        Ok(Expr::LetBlock {
            bindings,
            body: Block {
                stmts: vec![Stmt::Expr {
                    expr: combined,
                    span,
                }],
                span,
            },
            span,
        })
    }
}

fn build_scalar_comparison_chain(
    comparisons: &[String],
    operands: &[Expr],
    span: crate::span::Span,
) -> LowerResult<Expr> {
    debug_assert!(!comparisons.is_empty());
    debug_assert_eq!(operands.len(), comparisons.len() + 1);
    build_scalar_comparison_chain_from(0, operands[0].clone(), comparisons, operands, span)
}

fn build_scalar_comparison_chain_from(
    index: usize,
    left: Expr,
    comparisons: &[String],
    operands: &[Expr],
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let right_index = index + 1;
    let is_last_link = index + 1 == comparisons.len();
    let right_is_interior = right_index + 1 < operands.len();

    if right_is_interior && !is_atomic_operand(&operands[right_index]) {
        let tmp = format!("__cmpchain_{}_{}_{}", span.start, span.end, right_index);
        let tmp_expr = Expr::Var(tmp.clone().into(), span);
        let link = build_chain_link(&comparisons[index], left, tmp_expr.clone(), span)?;
        let expr = if is_last_link {
            link
        } else {
            Expr::BinaryOp {
                op: BinaryOp::And,
                left: Box::new(link),
                right: Box::new(build_scalar_comparison_chain_from(
                    index + 1,
                    tmp_expr,
                    comparisons,
                    operands,
                    span,
                )?),
                span,
            }
        };
        return Ok(Expr::LetBlock {
            bindings: vec![(tmp.into(), operands[right_index].clone())],
            body: Block {
                stmts: vec![Stmt::Expr { expr, span }],
                span,
            },
            span,
        });
    }

    let right = operands[right_index].clone();
    let link = build_chain_link(&comparisons[index], left, right.clone(), span)?;
    if is_last_link {
        Ok(link)
    } else {
        Ok(Expr::BinaryOp {
            op: BinaryOp::And,
            left: Box::new(link),
            right: Box::new(build_scalar_comparison_chain_from(
                index + 1,
                right,
                comparisons,
                operands,
                span,
            )?),
            span,
        })
    }
}

/// Whether a comparison operand can be safely duplicated across two adjacent
/// chain links without changing evaluation semantics (mirrors upstream's `pair?`
/// test that decides when a chained-comparison operand needs an ssavalue).
fn is_atomic_operand(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Var(..) | Expr::Literal(..) | Expr::FunctionRef { .. }
    )
}

/// Build a single comparison link for a chained comparison expansion.
///
/// Most comparison operators map directly onto a `BinaryOp` via
/// [`map_binary_op`]. The supertype operator `>:` has no `BinaryOp` of its own:
/// just like the single-operator path (`A >: B` => `B <: A`), it lowers to a
/// `Subtype` check with the operands swapped (Issue #5492).
fn build_chain_link(
    op: &str,
    left: Expr,
    right: Expr,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    if op == ">:" {
        // A >: B  ==>  B <: A
        return Ok(Expr::BinaryOp {
            op: BinaryOp::Subtype,
            left: Box::new(right),
            right: Box::new(left),
            span,
        });
    }

    let binary_op = map_binary_op(op).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedOperator(op.to_string()),
            span,
        )
    })?;
    Ok(Expr::BinaryOp {
        op: binary_op,
        left: Box::new(left),
        right: Box::new(right),
        span,
    })
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
        Some(ref op) if is_chainable_comparison_operator(op) => op.clone(),
        _ => {
            // Not a comparison operator, treat as a single operand
            operands.push(node);
            return;
        }
    };

    // Get left and right children
    let children: Vec<_> = walker
        .named_children(&node)
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

fn pair_operand_nodes<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
) -> LowerResult<(Node<'a>, Node<'a>)> {
    let children = walker.named_children_vec(&node);
    for (idx, child) in children.iter().enumerate() {
        if walker.kind(child) == NodeKind::Operator && walker.text(child) == "=>" {
            let Some(left_idx) = idx.checked_sub(1) else {
                break;
            };
            let Some(left) = children.get(left_idx) else {
                break;
            };
            let Some(right) = children.get(idx + 1) else {
                break;
            };
            return Ok((*left, *right));
        }
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedExpression(
            "pair expression (expected operands around =>)".to_string(),
        ),
        span,
    ))
}
