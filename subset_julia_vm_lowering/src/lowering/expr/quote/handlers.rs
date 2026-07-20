//! Expression head handlers for quote expansion.
//!
//! Each handler processes a specific Expr head type (call, block, if, for, while, etc.)
//! during macro expansion. Also includes hygiene helpers for variable collection.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::expr_heads::{quote_binding_role, ExprHead, QuoteBindingRole};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal};
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node};

use super::super::lower_expr_with_ctx;
use super::super::macros::expand_nested_macro_from_expr_args;
use super::code_generation::{expr_to_block, quote_constructor_to_code_with_hygiene};
use super::HygieneContext;

// Helper functions for handling different expression heads

fn lower_quote_splice_arg<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    // MacroTools @q/@match templates pass interpolation nodes back through quote
    // expansion. Strip the interpolation shell before lowering the value that
    // should be spliced (Issue #7541).
    match walker.kind(&node) {
        crate::parser::cst::NodeKind::UnaryExpression if walker.text(&node).starts_with('$') => {
            let children = walker.named_children_vec(&node);
            if let Some(inner) = children.last().copied() {
                return lower_quote_splice_arg(walker, inner, lambda_ctx);
            }
            lower_expr_with_ctx(walker, node, lambda_ctx)
        }
        crate::parser::cst::NodeKind::ParenthesizedExpression => {
            let children = walker.named_children_vec(&node);
            if let Some(inner) = children.first().copied() {
                return lower_quote_splice_arg(walker, inner, lambda_ctx);
            }
            lower_expr_with_ctx(walker, node, lambda_ctx)
        }
        crate::parser::cst::NodeKind::SplatExpression => {
            let children = walker.named_children_vec(&node);
            if let Some(inner) = children.first().copied() {
                return lower_expr_with_ctx(walker, inner, lambda_ctx);
            }
            lower_expr_with_ctx(walker, node, lambda_ctx)
        }
        _ => lower_expr_with_ctx(walker, node, lambda_ctx),
    }
}

pub(super) fn handle_call_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Second arg is the function/operator symbol. Resolve it through the
    // Pass-1 hygiene map: a callee naming a quote-introduced nested function
    // definition (registered by the `FunctionName` arm of
    // `collect_introduced_vars`, Issue #10916) must rename consistently with
    // that definition's gensym; any other callee (operators, Base/stdlib
    // helpers, macro parameters) is not registered and stays unchanged.
    let func_symbol = hygiene.resolve(&extract_symbol_from_constructor(&builtin_args[1])?);

    // Check if this is an esc() call - enter escaped context
    if func_symbol == "esc" && builtin_args.len() >= 3 {
        // esc(expr) - process the inner expression with escaped hygiene
        let escaped_hygiene = hygiene.enter_escaped();
        return quote_constructor_to_code_with_hygiene(
            &builtin_args[2],
            params,
            args,
            span,
            walker,
            lambda_ctx,
            &escaped_hygiene,
            has_varargs,
        );
    }

    // Remaining args are the call arguments
    // Handle splat interpolation $(p...) by expanding varargs
    let mut call_args = Vec::new();
    for arg_constructor in &builtin_args[2..] {
        // Check for SplatInterpolation marker
        if let Expr::Builtin {
            name: BuiltinOp::SplatInterpolation,
            args: splat_args,
            ..
        } = arg_constructor
        {
            // Extract the parameter name from the splat
            if let Some(Expr::Var(param_name, _)) = splat_args.first() {
                // Find the parameter index
                if let Some(idx) = params.iter().position(|p| p == param_name) {
                    // Expand all arguments from this index onwards (varargs)
                    for arg_node in &args[idx..] {
                        let expanded = lower_quote_splice_arg(walker, *arg_node, lambda_ctx)?;
                        call_args.push(expanded);
                    }
                    continue;
                }
            }
        }
        // Regular argument processing
        let arg = quote_constructor_to_code_with_hygiene(
            arg_constructor,
            params,
            args,
            span,
            walker,
            lambda_ctx,
            hygiene,
            has_varargs,
        )?;
        call_args.push(arg);
    }

    // Check if it's a binary operator
    if call_args.len() == 2 {
        if let Some(op) = match func_symbol.as_str() {
            "+" => Some(BinaryOp::Add),
            "-" => Some(BinaryOp::Sub),
            "*" => Some(BinaryOp::Mul),
            "/" => Some(BinaryOp::Div),
            "^" => Some(BinaryOp::Pow),
            "%" => Some(BinaryOp::Mod),
            "==" => Some(BinaryOp::Eq),
            "!=" => Some(BinaryOp::Ne),
            "<" => Some(BinaryOp::Lt),
            "<=" => Some(BinaryOp::Le),
            ">" => Some(BinaryOp::Gt),
            ">=" => Some(BinaryOp::Ge),
            "&&" => Some(BinaryOp::And),
            "||" => Some(BinaryOp::Or),
            _ => None,
        } {
            return Ok(Expr::BinaryOp {
                op,
                left: Box::new(call_args[0].clone()),
                right: Box::new(call_args[1].clone()),
                span,
            });
        }
    }

    // Check if it's a unary operator (!, -, +)
    if call_args.len() == 1 {
        if let Some(op) = match func_symbol.as_str() {
            "!" => Some(crate::ir::core::UnaryOp::Not),
            "-" => Some(crate::ir::core::UnaryOp::Neg),
            "+" => Some(crate::ir::core::UnaryOp::Pos),
            _ => None,
        } {
            return Ok(Expr::UnaryOp {
                op,
                operand: Box::new(call_args[0].clone()),
                span,
            });
        }
    }

    // Check if it's a range expression (:)
    if func_symbol == ":" {
        if call_args.len() == 2 {
            // The parser nests a step range `a:b:c` as `(a:b):c`, so a quoted /
            // `esc`-ed step range arrives here as a 2-arg colon whose first operand
            // is itself a 2-arg `Expr::Range`. Flatten it to `start:step:stop`,
            // mirroring `lower_range_expr` (collection.rs); otherwise re-lowering
            // builds `Range : stop` and the VM fails with "expected numeric value,
            // got Range" (Issue #7020, surfaced by `@animate`/`@gif`, Issue #6355).
            if let Expr::Range {
                start: inner_start,
                stop: inner_stop,
                step: None,
                ..
            } = &call_args[0]
            {
                return Ok(Expr::Range {
                    start: inner_start.clone(),
                    stop: Box::new(call_args[1].clone()),
                    step: Some(inner_stop.clone()),
                    span,
                });
            }
            // Simple range: start:end
            return Ok(Expr::Range {
                start: Box::new(call_args[0].clone()),
                stop: Box::new(call_args[1].clone()),
                step: None,
                span,
            });
        } else if call_args.len() == 3 {
            // Range with step: start:step:end
            return Ok(Expr::Range {
                start: Box::new(call_args[0].clone()),
                stop: Box::new(call_args[2].clone()),
                step: Some(Box::new(call_args[1].clone())),
                span,
            });
        }
    }

    // Otherwise, it's a function call
    Ok(Expr::Call {
        function: func_symbol.into(),
        args: call_args,
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    })
}

pub(super) fn handle_block_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Expr(:block, stmt1, stmt2, ...) -> LetBlock with statements
    let mut stmts = Vec::new();
    for stmt_constructor in &builtin_args[1..] {
        // Skip LineNumberNodes - they're just metadata
        if let Expr::Builtin {
            name: BuiltinOp::LineNumberNodeNew,
            ..
        } = stmt_constructor
        {
            continue;
        }
        if let Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args: inner_args,
            ..
        } = stmt_constructor
        {
            if inner_args.len() >= 2 {
                if let Ok(head) = extract_symbol_from_constructor(&inner_args[0]) {
                    match ExprHead::from_name(&head) {
                        Some(ExprHead::Assign) if inner_args.len() >= 3 => {
                            let var_name = assignment_target_name(
                                &inner_args[1],
                                params,
                                args,
                                walker,
                                hygiene,
                            )?;
                            let value = quote_constructor_to_code_with_hygiene(
                                &inner_args[2],
                                params,
                                args,
                                span,
                                walker,
                                lambda_ctx,
                                hygiene,
                                has_varargs,
                            )?;
                            stmts.push(crate::ir::core::Stmt::Assign {
                                var: var_name,
                                value,
                                span,
                            });
                            continue;
                        }
                        Some(ExprHead::Const) if inner_args.len() >= 2 => {
                            if let Some((var_name, value)) = const_assignment_parts(
                                &inner_args[1],
                                params,
                                args,
                                span,
                                walker,
                                lambda_ctx,
                                hygiene,
                                has_varargs,
                            )? {
                                stmts.push(crate::ir::core::Stmt::Expr {
                                    expr: Expr::Call {
                                        function: "#__sjulia_declare_const__".to_string().into(),
                                        args: vec![Expr::Literal(
                                            Literal::Str(var_name.clone()),
                                            span,
                                        )],
                                        kwargs: Vec::new(),
                                        splat_mask: vec![false],
                                        kwargs_splat_mask: Vec::new(),
                                        span,
                                    },
                                    span,
                                });
                                stmts.push(crate::ir::core::Stmt::Assign {
                                    var: var_name,
                                    value,
                                    span,
                                });
                                continue;
                            }
                        }
                        Some(ExprHead::Local) => {
                            for local_inner in &inner_args[1..] {
                                if matches!(
                                    local_inner,
                                    Expr::Builtin {
                                        name: BuiltinOp::SymbolNew,
                                        ..
                                    }
                                ) {
                                    let var = assignment_target_name(
                                        local_inner,
                                        params,
                                        args,
                                        walker,
                                        hygiene,
                                    )?;
                                    stmts.push(crate::ir::core::Stmt::LocalDecl {
                                        var,
                                        kind: crate::ir::core::LocalDeclKind::Explicit,
                                        span,
                                    });
                                    continue;
                                }

                                let Expr::Builtin {
                                    name: BuiltinOp::ExprNew,
                                    args: local_assign_args,
                                    ..
                                } = local_inner
                                else {
                                    continue;
                                };
                                let Some(local_head) = local_assign_args
                                    .first()
                                    .and_then(|head| extract_symbol_from_constructor(head).ok())
                                else {
                                    continue;
                                };
                                let assignment = if ExprHead::from_name(&local_head)
                                    == Some(ExprHead::Assign)
                                    && local_assign_args.len() >= 3
                                {
                                    Some((&local_assign_args[1], &local_assign_args[2]))
                                } else if local_head == "call"
                                    && local_assign_args.len() >= 4
                                    && extract_symbol_from_constructor(&local_assign_args[1])
                                        .is_ok_and(|op| op == "=")
                                {
                                    Some((&local_assign_args[2], &local_assign_args[3]))
                                } else {
                                    None
                                };
                                let Some((target, value_constructor)) = assignment else {
                                    continue;
                                };
                                let var =
                                    assignment_target_name(target, params, args, walker, hygiene)?;
                                let value = quote_constructor_to_code_with_hygiene(
                                    value_constructor,
                                    params,
                                    args,
                                    span,
                                    walker,
                                    lambda_ctx,
                                    hygiene,
                                    has_varargs,
                                )?;
                                let assignment = crate::ir::core::Stmt::Assign { var, value, span };
                                stmts.push(crate::lowering::stmt::with_local_declarations(
                                    assignment, span,
                                ));
                            }
                            continue;
                        }
                        _ => {}
                    }
                }
            }
        }
        let expr = quote_constructor_to_code_with_hygiene(
            stmt_constructor,
            params,
            args,
            span,
            walker,
            lambda_ctx,
            hygiene,
            has_varargs,
        )?;
        stmts.push(crate::ir::core::Stmt::Expr { expr, span });
    }

    // If the block's tail statement is a `try` (lowered by `handle_try_expr` to a
    // `LetBlock` wrapping a single bare `Stmt::Try`), rewrite it into a
    // value-producing try so a `try/catch[/else/finally]` at the tail of a
    // macro-expansion block yields the executed branch's value rather than
    // `nothing`. This is what `@lock`'s `try … finally unlock(…) end` tail needs
    // in value position (Issue #7806). Non-tail trys keep the bare-statement form
    // so statement-position expansions (e.g. `@test_throws`) are unaffected.
    if let Some(crate::ir::core::Stmt::Expr {
        expr: tail_expr,
        span: tail_span,
    }) = stmts.last()
    {
        if let Some(value_try) = try_letblock_into_value_expr(tail_expr, *tail_span) {
            let last_idx = stmts.len() - 1;
            stmts[last_idx] = crate::ir::core::Stmt::Expr {
                expr: value_try,
                span: *tail_span,
            };
        }
    }

    if stmts.is_empty() {
        // Empty block evaluates to nothing
        Ok(Expr::Literal(Literal::Nothing, span))
    } else if stmts.len() == 1 {
        // Single statement: check if it's a pure expression (not assignment)
        let is_pure_expr = matches!(&stmts[0], crate::ir::core::Stmt::Expr { .. });
        if is_pure_expr {
            let only_stmt = stmts.remove(0);
            if let crate::ir::core::Stmt::Expr { expr, .. } = only_stmt {
                Ok(expr)
            } else {
                Ok(Expr::Literal(Literal::Nothing, span))
            }
        } else {
            // Single assignment: wrap in LetBlock for expression context
            let body = crate::ir::core::Block { stmts, span };
            Ok(Expr::LetBlock {
                bindings: vec![],
                body,
                span,
            })
        }
    } else {
        // Multiple statements: wrap in a LetBlock
        let body = crate::ir::core::Block { stmts, span };
        Ok(Expr::LetBlock {
            bindings: vec![],
            body,
            span,
        })
    }
}

struct QuoteAssignment<'a> {
    target: &'a Expr,
    value: &'a Expr,
}

fn assignment_constructor_parts(expr: &Expr) -> LowerResult<Option<QuoteAssignment<'_>>> {
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = expr
    else {
        return Ok(None);
    };
    if args.len() < 3 {
        return Ok(None);
    }
    let head = extract_symbol_from_constructor(&args[0])?;
    if head == "=" {
        return Ok(Some(QuoteAssignment {
            target: &args[1],
            value: &args[2],
        }));
    }
    if head == "call" && args.len() >= 4 {
        let op = extract_symbol_from_constructor(&args[1])?;
        if op == "=" {
            return Ok(Some(QuoteAssignment {
                target: &args[2],
                value: &args[3],
            }));
        }
    }
    Ok(None)
}

fn const_assignment_parts<'a>(
    expr: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Option<(String, Expr)>> {
    let Some(assignment) = assignment_constructor_parts(expr)? else {
        return Ok(None);
    };
    let var_name = assignment_target_name(assignment.target, params, args, walker, hygiene)?;
    let value = quote_constructor_to_code_with_hygiene(
        assignment.value,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    Ok(Some((var_name, value)))
}

fn assignment_target_name<'a>(
    expr: &Expr,
    params: &[String],
    args: &[Node<'a>],
    walker: &CstWalker<'a>,
    hygiene: &HygieneContext,
) -> LowerResult<String> {
    let mut name = match expr {
        Expr::Var(name, _) => {
            if let Some(idx) = params.iter().position(|param| param == name) {
                walker.text(&args[idx]).trim_start_matches(':').to_string()
            } else {
                name.to_string()
            }
        }
        _ => extract_symbol_from_constructor(expr)?,
    };
    if let Some(param_name) = name.strip_prefix('$') {
        if let Some(idx) = params.iter().position(|param| param == param_name) {
            name = walker.text(&args[idx]).trim_start_matches(':').to_string();
        }
    }
    Ok(hygiene.resolve(&name))
}

pub(super) fn handle_macrocall_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Nested macro call: Expr(:macrocall, Symbol("@name"), nothing, args...)
    // builtin_args[1] is the macro name symbol
    // builtin_args[2] is the line number node (nothing)
    // builtin_args[3...] are the arguments

    if builtin_args.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "macrocall with too few arguments".to_string(),
            ),
            span,
        ));
    }

    // Extract macro name (with @ prefix)
    let macro_name_with_at = extract_symbol_from_constructor(&builtin_args[1])?;
    let macro_name = macro_name_with_at.trim_start_matches('@').to_string();

    // Get arguments (skip head, name, and line number node)
    let macro_args = if builtin_args.len() > 3 {
        &builtin_args[3..]
    } else {
        &[]
    };

    // Convert arguments to executable code
    let converted_args: Result<Vec<_>, _> = macro_args
        .iter()
        .map(|a| {
            quote_constructor_to_code_with_hygiene(
                a,
                params,
                args,
                span,
                walker,
                lambda_ctx,
                hygiene,
                has_varargs,
            )
        })
        .collect();
    let converted_args = converted_args?;

    // Look up the macro with the correct arity
    let macro_def = if lambda_ctx.has_macro(&macro_name) {
        lambda_ctx.get_macro_with_arity(&macro_name, converted_args.len())
    } else if crate::lowering::macros_registry::has_base_macro(&macro_name) {
        crate::lowering::macros_registry::get_base_macro(&macro_name)
    } else {
        None
    };

    let macro_def = macro_def.ok_or_else(|| {
        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
            "nested macro @{} not found (with {} args)",
            macro_name,
            converted_args.len()
        ))
    })?;

    // Check arity
    if converted_args.len() != macro_def.params.len() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "nested macro @{} expects {} arguments, got {}",
                macro_name,
                macro_def.params.len(),
                converted_args.len()
            )),
        );
    }

    // Expand the nested macro
    expand_nested_macro_from_expr_args(&macro_def, &converted_args, span, lambda_ctx)
}

pub(super) fn handle_tuple_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Check if this is a named tuple: (a=1, b=2) -> Expr(:tuple, Expr(:(=), :a, 1), ...)
    let mut is_named_tuple = true;
    let mut named_fields: Vec<(crate::ir::core::InternedStr, Expr)> = Vec::new();

    for elem_constructor in &builtin_args[1..] {
        if let Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args: elem_args,
            ..
        } = elem_constructor
        {
            if elem_args.len() >= 3 {
                if let Ok(elem_head) = extract_symbol_from_constructor(&elem_args[0]) {
                    if elem_head == "=" {
                        // This is a named field: Expr(:(=), :name, value)
                        if let Ok(field_name) = extract_symbol_from_constructor(&elem_args[1]) {
                            let field_value = quote_constructor_to_code_with_hygiene(
                                &elem_args[2],
                                params,
                                args,
                                span,
                                walker,
                                lambda_ctx,
                                hygiene,
                                has_varargs,
                            )?;
                            named_fields.push((field_name.into(), field_value));
                            continue;
                        }
                    }
                }
            }
        }
        // Not a named field pattern
        is_named_tuple = false;
        break;
    }

    if is_named_tuple && !named_fields.is_empty() {
        // Create NamedTupleLiteral
        Ok(Expr::NamedTupleLiteral {
            fields: named_fields,
            span,
        })
    } else {
        // Regular tuple: Expr(:tuple, elem1, elem2, ...) -> TupleLiteral
        // Handle splat interpolation $(p...) by expanding varargs
        let mut elements = Vec::new();
        for elem_constructor in &builtin_args[1..] {
            // Check for SplatInterpolation marker
            if let Expr::Builtin {
                name: BuiltinOp::SplatInterpolation,
                args: splat_args,
                ..
            } = elem_constructor
            {
                // Extract the parameter name from the splat
                if let Some(Expr::Var(param_name, _)) = splat_args.first() {
                    // Find the parameter index
                    if let Some(idx) = params.iter().position(|p| p == param_name) {
                        // Expand all arguments from this index onwards (varargs)
                        for arg_node in &args[idx..] {
                            let expanded = lower_quote_splice_arg(walker, *arg_node, lambda_ctx)?;
                            elements.push(expanded);
                        }
                        continue;
                    }
                }
            }
            // Regular element processing
            let elem = quote_constructor_to_code_with_hygiene(
                elem_constructor,
                params,
                args,
                span,
                walker,
                lambda_ctx,
                hygiene,
                has_varargs,
            )?;
            elements.push(elem);
        }
        Ok(Expr::TupleLiteral { elements, span })
    }
}

/// If `expr` is the `LetBlock { [Stmt::Try] }` shape that `handle_try_expr`
/// produces, convert it into the equivalent *value-producing* try expression so
/// a `try` at the tail of a macro-expansion block yields the executed branch's
/// value (Issue #7806). Returns `None` for any other expression shape (leaving
/// non-try tails and statement-position trys untouched).
fn try_letblock_into_value_expr(expr: &Expr, span: crate::span::Span) -> Option<Expr> {
    if let Expr::LetBlock { bindings, body, .. } = expr {
        if bindings.is_empty() && body.stmts.len() == 1 {
            if let crate::ir::core::Stmt::Try { .. } = &body.stmts[0] {
                return crate::lowering::expr::try_stmt_into_value_expr(
                    body.stmts[0].clone(),
                    span,
                );
            }
        }
    }
    None
}

pub(super) fn handle_try_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Expr(:try, try_block, catch_var_or_false, catch_block_or_false,
    //      [finally_block_or_false[, else_block]])
    // Convert to a value-producing Stmt::Try (LetBlock).
    //
    // Upstream Julia stores the `:try` head with the `finally`/`else` slots
    // optional, so the catch-only form (no `finally`) is the valid 3-arg shape
    // `[try_block, catch_var_or_false, catch_block_or_false]` — with the head
    // symbol in `builtin_args[0]` this is `builtin_args.len() == 4`. The previous
    // `< 4` guard already accepted that, but the catch-only shape can also arrive
    // with the `catch_block` omitted; accept `>= 3` (head + try_block + catch_var)
    // since the `builtin_args[3..]` reads below are length-guarded (Issue #7832).
    if builtin_args.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "try expression requires at least 3 arguments (head, try_block, catch_var)"
                    .to_string(),
            ),
            span,
        ));
    }

    // Parse try block (builtin_args[1])
    let try_block_expr = quote_constructor_to_code_with_hygiene(
        &builtin_args[1],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    let try_block = expr_to_block(try_block_expr, span);

    // Parse catch variable (builtin_args[2]) - can be false or a symbol
    let catch_var = match builtin_args.get(2) {
        Some(Expr::Literal(Literal::Bool(false), _)) | None => None,
        Some(other) => extract_symbol_from_constructor(other)
            .ok()
            .map(|var_name| hygiene.resolve(&var_name)),
    };

    // Parse catch block (builtin_args[3]) - can be false or a block, and is
    // absent entirely for the catch-only 3-arg shape (Issue #7832).
    let catch_block = match builtin_args.get(3) {
        Some(Expr::Literal(Literal::Bool(false), _)) | None => None,
        Some(catch_constructor) => {
            let catch_block_expr = quote_constructor_to_code_with_hygiene(
                catch_constructor,
                params,
                args,
                span,
                walker,
                lambda_ctx,
                hygiene,
                has_varargs,
            )?;
            Some(expr_to_block(catch_block_expr, span))
        }
    };

    // Parse finally block (builtin_args[4]) if present and not false.
    let finally_block = if builtin_args.len() > 4
        && !matches!(&builtin_args[4], Expr::Literal(Literal::Bool(false), _))
    {
        let finally_block_expr = quote_constructor_to_code_with_hygiene(
            &builtin_args[4],
            params,
            args,
            span,
            walker,
            lambda_ctx,
            hygiene,
            has_varargs,
        )?;
        Some(expr_to_block(finally_block_expr, span))
    } else {
        None
    };

    // Parse else block (builtin_args[5]) if present.
    let else_block = if builtin_args.len() > 5 {
        let else_block_expr = quote_constructor_to_code_with_hygiene(
            &builtin_args[5],
            params,
            args,
            span,
            walker,
            lambda_ctx,
            hygiene,
            has_varargs,
        )?;
        Some(expr_to_block(else_block_expr, span))
    } else {
        None
    };

    // Create Stmt::Try and wrap in a LetBlock. The bare statement form is used
    // for statement-position trys (e.g. the `@test_throws` expansion's
    // `try …; catch …; end` whose value is discarded); the surrounding block's
    // tail-value handling (`handle_block_expr`) routes a try that is the block's
    // last expression through `try_stmt_into_value_expr` so value-position trys
    // (e.g. `@lock`, Issue #7806) still yield the executed branch's value.
    let try_stmt = crate::ir::core::Stmt::Try {
        try_block,
        catch_var,
        catch_block,
        else_block,
        finally_block,
        span,
    };

    let body = crate::ir::core::Block {
        stmts: vec![try_stmt],
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![],
        body,
        span,
    })
}

pub(super) fn handle_if_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Expr(:if, condition, then_block[, else_block])
    // Convert to Stmt::If wrapped in LetBlock

    if builtin_args.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "if expression requires at least 3 arguments (head, condition, then_block)"
                    .to_string(),
            ),
            span,
        ));
    }

    // Parse condition (builtin_args[1])
    let condition = quote_constructor_to_code_with_hygiene(
        &builtin_args[1],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;

    // Parse then block (builtin_args[2])
    let then_block_expr = quote_constructor_to_code_with_hygiene(
        &builtin_args[2],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    let then_block = expr_to_block(then_block_expr, span);

    // Parse else block (builtin_args[3]) if present
    let else_block = if builtin_args.len() > 3 {
        let else_block_expr = quote_constructor_to_code_with_hygiene(
            &builtin_args[3],
            params,
            args,
            span,
            walker,
            lambda_ctx,
            hygiene,
            has_varargs,
        )?;
        Some(expr_to_block(else_block_expr, span))
    } else {
        None
    };

    // Create Stmt::If and wrap in LetBlock
    let if_stmt = crate::ir::core::Stmt::If {
        condition,
        then_branch: then_block,
        else_branch: else_block,
        span,
    };

    let body = crate::ir::core::Block {
        stmts: vec![if_stmt],
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![],
        body,
        span,
    })
}

/// `Expr(:elseif, Expr(:block, condition), then_block[, else_or_elseif])`.
///
/// Upstream Julia desugars `if a; ...; elseif b; ...; else; ...; end` as
/// `Expr(:if, a, then_a, Expr(:elseif, Expr(:block, b), then_b, else_block))` —
/// the `elseif` clause is nested inside the parent `if`'s else-branch position
/// and is structurally just another `if` whose condition happens to be wrapped
/// in a one-element `Expr(:block, ...)` (see the CST desugaring comment in
/// `cst_to_constructor.rs` around the `IfStatement` case). `handle_if_expr`'s
/// condition arg is already lowered through the generic `Expr(:block, ...)`
/// handler (`handle_block_expr`), which unwraps a single-statement block to its
/// bare expression — so the wrapped condition round-trips to the same value a
/// plain (unwrapped) `if` condition would. The two heads are therefore handled
/// identically once dispatched here (Issue #10208).
pub(super) fn handle_elseif_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    handle_if_expr(
        builtin_args,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )
}

pub(super) fn handle_for_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Expr(:for, Expr(:(=), :var, iterable), body)
    // Convert to Stmt::ForEach wrapped in LetBlock

    if builtin_args.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "for expression requires at least 3 arguments (head, binding, body)".to_string(),
            ),
            span,
        ));
    }

    // Parse the binding: Expr(:(=), :var, iterable)
    let binding = &builtin_args[1];
    let (var_name, iterable) = if let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: binding_args,
        ..
    } = binding
    {
        if binding_args.len() >= 3 {
            if let Ok(binding_head) = extract_symbol_from_constructor(&binding_args[0]) {
                if binding_head == "=" {
                    let orig_var_name = extract_symbol_from_constructor(&binding_args[1])?;
                    let var_name = hygiene.resolve(&orig_var_name);
                    let iterable = quote_constructor_to_code_with_hygiene(
                        &binding_args[2],
                        params,
                        args,
                        span,
                        walker,
                        lambda_ctx,
                        hygiene,
                        has_varargs,
                    )?;
                    (var_name, iterable)
                } else {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::UnsupportedExpression(format!(
                            "for binding must be an assignment, got :{}",
                            binding_head
                        )),
                        span,
                    ));
                }
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "for binding must be an assignment expression".to_string(),
                    ),
                    span,
                ));
            }
        } else {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(
                    "for binding expression has too few arguments".to_string(),
                ),
                span,
            ));
        }
    } else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "for binding must be an Expr".to_string(),
            ),
            span,
        ));
    };

    // Parse body (builtin_args[2])
    let body_expr = quote_constructor_to_code_with_hygiene(
        &builtin_args[2],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    let body = expr_to_block(body_expr, span);

    // Create Stmt::ForEach and wrap in LetBlock
    let for_stmt = crate::ir::core::Stmt::ForEach {
        var: var_name,
        iterable,
        body,
        span,
    };

    let block = crate::ir::core::Block {
        stmts: vec![for_stmt],
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: block,
        span,
    })
}

/// Expand `Expr(:let, [bindings...], body)` produced inside a `quote ... end`
/// macro body into an `Expr::LetBlock`.
///
/// This mirrors source-level `let` lowering (`lowering::expr::misc::lower_let_expr`)
/// and the runtime macro-expansion path (`macro_runtime::let_expr_from_args`): a
/// bare `let ... end` with no bindings is given a synthetic `__sjulia_let_scope_*`
/// binding so the resulting `LetBlock` is a **hard** (local) scope rather than a
/// scope-transparent empty-bindings block. This is what makes upstream
/// `Test.@testset`'s `let`-wrapped body a hard scope (Issue #9312): testset-body
/// assignments become testset-local and a `for` loop that accumulates into one
/// mutates the enclosing local instead of hitting file-mode soft-scope
/// localization.
///
/// The CST parses a bare `let` with only a body child (`parse_let_expression`),
/// so the body is always the LAST constructor argument; any binding children sit
/// between the head and the body (`let x = 1, y = 2; body end`).
pub(super) fn handle_let_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // builtin_args[0] is the `:let` head symbol; there must be at least a body.
    if builtin_args.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "let expression requires a body".to_string(),
            ),
            span,
        ));
    }

    let body_constructor = &builtin_args[builtin_args.len() - 1];
    let binding_constructors = &builtin_args[1..builtin_args.len() - 1];

    let mut bindings: Vec<(crate::ir::core::InternedStr, Expr)> = Vec::new();
    for binding_constructor in binding_constructors {
        collect_let_bindings_from_constructor(
            binding_constructor,
            params,
            args,
            span,
            walker,
            lambda_ctx,
            hygiene,
            has_varargs,
            &mut bindings,
        )?;
    }

    // Bare `let` (no bindings): inject a synthetic binding so the `LetBlock` is a
    // hard scope, not a scope-transparent empty-bindings block. Matches
    // `lower_let_expr` / `let_bindings_from_value`.
    if bindings.is_empty() {
        bindings.push((
            format!("__sjulia_let_scope_{}", span.start).into(),
            Expr::Literal(Literal::Nothing, span),
        ));
    }

    // Lower the body. A `:block` body lowers to a scope-transparent
    // empty-bindings `LetBlock`; keep it *nested* inside this hard-scope `let`
    // (do NOT flatten it into the outer body). Both downstream passes descend
    // through a scope-transparent empty-bindings `LetBlock`:
    //   * the soft-scope loop-localization pass (`soft_scope.rs`) sees the loop
    //     and the enclosing-scope locals through it, so the Issue #9312 loop-
    //     accumulator fix still works;
    //   * the closure-boxing pass (`closure_box.rs`) treats the empty-bindings
    //     `LetBlock` as a defining scope and unifies a captured local with its
    //     later reassignments (Issue #6281). Flattening the body into this
    //     non-empty `let` breaks that box unification, so preserve the nesting.
    let body_expr = quote_constructor_to_code_with_hygiene(
        body_constructor,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    let body = expr_to_block(body_expr, span);

    Ok(Expr::LetBlock {
        bindings,
        body,
        span,
    })
}

/// Collect `let` bindings from a single quote-constructor argument into `out`.
///
/// Handles the two shapes the CST-to-constructor stage produces for let
/// bindings: a `:block` grouping several `var = value` assignments (multiple
/// bindings), or a single `:(=)` assignment (one binding). Exotic binding forms
/// (e.g. a bare `let x` with no value) are reported as unsupported rather than
/// silently mishandled.
#[allow(clippy::too_many_arguments)]
fn collect_let_bindings_from_constructor<'a>(
    constructor: &Expr,
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
    out: &mut Vec<(crate::ir::core::InternedStr, Expr)>,
) -> LowerResult<()> {
    if let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: inner_args,
        ..
    } = constructor
    {
        if let Some(head) = inner_args
            .first()
            .and_then(|h| extract_symbol_from_constructor(h).ok())
        {
            match ExprHead::from_name(&head) {
                // `let x = 1, y = 2` -> bindings collapsed into a :block.
                Some(ExprHead::Block) => {
                    for child in &inner_args[1..] {
                        if matches!(
                            child,
                            Expr::Builtin {
                                name: BuiltinOp::LineNumberNodeNew,
                                ..
                            }
                        ) {
                            continue;
                        }
                        collect_let_bindings_from_constructor(
                            child,
                            params,
                            args,
                            span,
                            walker,
                            lambda_ctx,
                            hygiene,
                            has_varargs,
                            out,
                        )?;
                    }
                    return Ok(());
                }
                // `let x = value` -> single binding assignment.
                Some(ExprHead::Assign) if inner_args.len() >= 3 => {
                    let name =
                        assignment_target_name(&inner_args[1], params, args, walker, hygiene)?;
                    let value = quote_constructor_to_code_with_hygiene(
                        &inner_args[2],
                        params,
                        args,
                        span,
                        walker,
                        lambda_ctx,
                        hygiene,
                        has_varargs,
                    )?;
                    out.push((name.into(), value));
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedExpression(
            "let binding in quote expansion must be a `var = value` assignment".to_string(),
        ),
        span,
    ))
}

pub(super) fn handle_while_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    // Expr(:while, condition, body)
    // Convert to Stmt::While wrapped in LetBlock

    if builtin_args.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "while expression requires at least 3 arguments (head, condition, body)"
                    .to_string(),
            ),
            span,
        ));
    }

    // Parse condition (builtin_args[1])
    let condition = quote_constructor_to_code_with_hygiene(
        &builtin_args[1],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;

    // Parse body (builtin_args[2])
    let body_expr = quote_constructor_to_code_with_hygiene(
        &builtin_args[2],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    let body = expr_to_block(body_expr, span);

    // Create Stmt::While and wrap in LetBlock
    let while_stmt = crate::ir::core::Stmt::While {
        condition,
        body,
        span,
    };

    let block = crate::ir::core::Block {
        stmts: vec![while_stmt],
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: block,
        span,
    })
}

/// Parse one function/lambda parameter constructor into a [`TypedParam`]:
/// a bare `SymbolNew` (`x`), a typed `Expr(:(::), :x, :T)`, or a varargs
/// `Expr(:..., param)` — the constructor-tree analog of the dynamic path's
/// `function_param_from_value` (`macro_runtime.rs`). Parameter names resolve
/// through the flat [`HygieneContext`], so a parameter sharing a bare name
/// with a whole-expansion quote-local collapses onto that local's gensym
/// (matching upstream's own flat per-expansion rename); a name registered
/// nowhere stays unchanged — parameters are deliberately NOT Pass-1
/// registered on this path (Issues #10626/#10925: the static rename is a
/// flat whole-tree substitution, so registering a parameter would clobber an
/// unrelated sibling reference sharing its bare name; sjulia's call-frame
/// scoping already isolates parameters at runtime without renaming).
fn typed_param_from_constructor(
    param: &Expr,
    span: crate::span::Span,
    hygiene: &HygieneContext,
) -> LowerResult<crate::ir::core::TypedParam> {
    use crate::ir::core::TypedParam;
    if let Ok(name) = extract_symbol_from_constructor(param) {
        return Ok(TypedParam::untyped(hygiene.resolve(&name), span));
    }
    if let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = param
    {
        if args.len() >= 2 {
            if let Ok(head) = extract_symbol_from_constructor(&args[0]) {
                match ExprHead::from_name(&head) {
                    Some(ExprHead::TypeAssert) if args.len() == 3 => {
                        let param_name = extract_symbol_from_constructor(&args[1])?;
                        let type_name = extract_symbol_from_constructor(&args[2])?;
                        return Ok(TypedParam::new(
                            hygiene.resolve(&param_name),
                            Some(crate::types::JuliaType::from_name_or_struct(&type_name)),
                            span,
                        ));
                    }
                    Some(ExprHead::Splat) if args.len() == 2 => {
                        let inner = typed_param_from_constructor(&args[1], span, hygiene)?;
                        return Ok(TypedParam::varargs(inner.name, inner.type_annotation, span));
                    }
                    _ => {}
                }
            }
        }
    }
    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedExpression(
            "quote expansion: unsupported function parameter shape".to_string(),
        ),
        span,
    ))
}

/// Parse an arrow lambda's parameter constructor — a bare `SymbolNew`
/// (`x -> ...`), a single typed parameter (`(x::Int) -> ...`), or an
/// `Expr(:tuple, params...)` (`(a, b) -> ...`, including the empty
/// `() -> ...`).
fn arrow_params_from_constructor(
    ctor: &Expr,
    span: crate::span::Span,
    hygiene: &HygieneContext,
) -> LowerResult<Vec<crate::ir::core::TypedParam>> {
    if let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = ctor
    {
        if let Some(head_ctor) = args.first() {
            if let Ok(head) = extract_symbol_from_constructor(head_ctor) {
                if ExprHead::from_name(&head) == Some(ExprHead::Tuple) {
                    return args[1..]
                        .iter()
                        .map(|arg| typed_param_from_constructor(arg, span, hygiene))
                        .collect();
                }
            }
        }
    }
    Ok(vec![typed_param_from_constructor(ctor, span, hygiene)?])
}

/// Lift a lambda body as a fresh named function on `lambda_ctx` and return
/// the `FunctionRef` value referencing it — shared by the arrow
/// (`Expr(:->, ...)`, Issue #10617) and anonymous-function
/// (`Expr(:function, Expr(:tuple, ...), ...)`, Issues #10916/#10926)
/// handlers, mirroring the dynamic path's `arrow_expr_from_values`.
#[allow(clippy::too_many_arguments)]
fn lift_quote_lambda(
    lambda_params: Vec<crate::ir::core::TypedParam>,
    type_params: Vec<crate::types::TypeParam>,
    body_expr: Expr,
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> Expr {
    let lambda_name = lambda_ctx.next_lambda_name();
    lambda_ctx.add_lifted_function(crate::ir::core::Function {
        name: lambda_name.clone(),
        params: lambda_params,
        kwparams: Vec::new(),
        type_params,
        return_type: None,
        body: crate::ir::core::Block {
            stmts: vec![crate::ir::core::Stmt::Return {
                value: Some(body_expr),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    });
    Expr::FunctionRef {
        name: lambda_name.into(),
        span,
    }
}

/// `Expr(:->, params, body)` — an arrow lambda inside a stdlib macro's
/// `quote` body (Issue #10617). Lifts the body as a fresh lambda and yields
/// a `FunctionRef` value, mirroring the dynamic path's
/// `arrow_expr_from_values` (`macro_runtime.rs`).
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_arrow_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    if builtin_args.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "arrow lambda requires at least 3 arguments (head, params, body)".to_string(),
            ),
            span,
        ));
    }
    let lambda_params = arrow_params_from_constructor(&builtin_args[1], span, hygiene)?;
    let body_expr = quote_constructor_to_code_with_hygiene(
        &builtin_args[2],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    Ok(lift_quote_lambda(
        lambda_params,
        Vec::new(),
        body_expr,
        span,
        lambda_ctx,
    ))
}

/// Extract the `where`-bound type parameters from a signature constructor,
/// returning the unwrapped inner signature plus the declared [`TypeParam`]s
/// (bare `SymbolNew` or upper-bounded `Expr(:<:, :T, :Bound)` forms) — the
/// constructor-tree analog of the dynamic path's
/// `constructor_signature_from_value` `Where` arm.
fn unwrap_where_signature(
    signature: &Expr,
    span: crate::span::Span,
) -> LowerResult<(&Expr, Vec<crate::types::TypeParam>)> {
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = signature
    else {
        return Ok((signature, Vec::new()));
    };
    if args.len() < 2 {
        return Ok((signature, Vec::new()));
    }
    let Ok(head) = extract_symbol_from_constructor(&args[0]) else {
        return Ok((signature, Vec::new()));
    };
    if ExprHead::from_name(&head) != Some(ExprHead::Where) {
        return Ok((signature, Vec::new()));
    }
    let mut type_params = Vec::new();
    for var in &args[2..] {
        type_params.push(where_type_param_from_constructor(var, span)?);
    }
    Ok((&args[1], type_params))
}

/// One `where` type-variable constructor → [`TypeParam`]: bare `SymbolNew`
/// (`where T`) or upper-bounded `Expr(:<:, :T, :Bound)` (`where T<:Bound`).
fn where_type_param_from_constructor(
    var: &Expr,
    span: crate::span::Span,
) -> LowerResult<crate::types::TypeParam> {
    use crate::types::TypeParam;
    if let Ok(name) = extract_symbol_from_constructor(var) {
        return Ok(TypeParam::new(name));
    }
    if let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = var
    {
        if args.len() == 3 {
            if let Ok(head) = extract_symbol_from_constructor(&args[0]) {
                if ExprHead::from_name(&head) == Some(ExprHead::Subtype) {
                    let name = extract_symbol_from_constructor(&args[1])?;
                    let bound = extract_symbol_from_constructor(&args[2])?;
                    return Ok(TypeParam::with_upper_bound(name, bound));
                }
            }
        }
    }
    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::UnsupportedExpression(
            "quote expansion: unsupported where type-variable shape".to_string(),
        ),
        span,
    ))
}

/// `Expr(:function, signature, body)` — a nested function definition inside
/// a stdlib macro's `quote` body (Issue #10916).
///
/// * Named form (`Expr(:call, :name, params...)` signature, optionally
///   `where`-wrapped): emits a `Stmt::FunctionDef` wrapped in an
///   empty-bindings `LetBlock`, mirroring the dynamic path's
///   `function_stmt_from_values`. The function NAME resolves through the
///   Pass-1 hygiene map (registered by `collect_introduced_vars`'
///   `FunctionName` arm), so same-expansion call sites rename consistently.
/// * Anonymous form (`Expr(:tuple, params...)` signature): lifts a lambda
///   and yields a `FunctionRef` value, exactly like `handle_arrow_expr`
///   (mirrors the dynamic path's Issue #10926 arm).
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_function_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    if builtin_args.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "function definition requires at least 3 arguments (head, signature, body)"
                    .to_string(),
            ),
            span,
        ));
    }
    let (signature, type_params) = unwrap_where_signature(&builtin_args[1], span)?;
    let body_expr = quote_constructor_to_code_with_hygiene(
        &builtin_args[2],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;

    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: sig_args,
        ..
    } = signature
    else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "quote expansion: unsupported function signature shape".to_string(),
            ),
            span,
        ));
    };
    let sig_head = sig_args
        .first()
        .map(extract_symbol_from_constructor)
        .transpose()?
        .and_then(|head| ExprHead::from_name(&head));

    match sig_head {
        // Anonymous function `function (params...) ... end`: value form.
        Some(ExprHead::Tuple) => {
            let lambda_params = sig_args[1..]
                .iter()
                .map(|arg| typed_param_from_constructor(arg, span, hygiene))
                .collect::<LowerResult<Vec<_>>>()?;
            Ok(lift_quote_lambda(
                lambda_params,
                type_params,
                body_expr,
                span,
                lambda_ctx,
            ))
        }
        // Named function `function name(params...) ... end`: statement form.
        Some(ExprHead::Call) if sig_args.len() >= 2 => {
            let fn_name = extract_symbol_from_constructor(&sig_args[1])?;
            let fn_params = sig_args[2..]
                .iter()
                .map(|arg| typed_param_from_constructor(arg, span, hygiene))
                .collect::<LowerResult<Vec<_>>>()?;
            let func = crate::ir::core::Function {
                name: hygiene.resolve(&fn_name),
                params: fn_params,
                kwparams: Vec::new(),
                type_params,
                return_type: None,
                body: expr_to_block(body_expr, span),
                is_base_extension: false,
                is_runtime_eval: false,
                span,
                new_struct_name: None,
            };
            let block = crate::ir::core::Block {
                stmts: vec![crate::ir::core::Stmt::FunctionDef {
                    func: Box::new(func),
                    span,
                }],
                span,
            };
            Ok(Expr::LetBlock {
                bindings: vec![],
                body: block,
                span,
            })
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "quote expansion: unsupported function signature shape".to_string(),
            ),
            span,
        )),
    }
}

/// `Expr(:where, body, vars...)` in value position — a `where`-bound type
/// used as a bare value inside a stdlib macro's `quote` body (Issue #10916).
/// Mirrors the dynamic path's `where_expr_from_values` (`macro_runtime.rs`,
/// Issue #7844): each introduced type variable becomes a `let`-bound runtime
/// `TypeVar(:name[, lower, upper])`, and the body is wrapped
/// innermost-last in `UnionAll` calls (first-listed variable outermost).
/// Per Issues #10626/#10925 the bound-variable names are deliberately NOT
/// Pass-1 hygiene-registered (flat rename would clobber unrelated sibling
/// references); the `let` binding shadows locally at runtime instead.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_where_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    if builtin_args.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "where expression requires at least 3 arguments (head, body, typevar)".to_string(),
            ),
            span,
        ));
    }

    let mut bindings: Vec<(crate::ir::core::InternedStr, Expr)> = Vec::new();
    for var in &builtin_args[2..] {
        let type_param = where_type_param_from_constructor(var, span)?;
        let mut call_args = vec![Expr::Literal(
            Literal::Symbol(type_param.name.clone()),
            span,
        )];
        if let Some(upper) = &type_param.upper_bound {
            call_args.push(Expr::Literal(
                Literal::DataType(crate::types::JuliaType::Bottom.name().into_owned()),
                span,
            ));
            call_args.push(Expr::Var(upper.clone().into(), span));
        }
        bindings.push((
            type_param.name.clone().into(),
            Expr::Call {
                function: "TypeVar".to_string().into(),
                args: call_args,
                kwargs: Vec::new(),
                splat_mask: Vec::new(),
                kwargs_splat_mask: Vec::new(),
                span,
            },
        ));
    }

    let mut body_expr = quote_constructor_to_code_with_hygiene(
        &builtin_args[1],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    for (name, _) in bindings.iter().rev() {
        body_expr = Expr::Call {
            function: "UnionAll".to_string().into(),
            args: vec![Expr::Var(*name, span), body_expr],
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span,
        };
    }

    Ok(Expr::LetBlock {
        bindings,
        body: crate::ir::core::Block {
            stmts: vec![crate::ir::core::Stmt::Expr {
                expr: body_expr,
                span,
            }],
            span,
        },
        span,
    })
}

/// Parse a generator constructor's pieces:
/// `Expr(:generator, body, Expr(:(=), :var, iter))` (single-binding,
/// unfiltered — the only form `generator_constructor` in
/// `cst_to_constructor.rs` emits; filtered/multi-binding forms are already
/// rejected there with the Issue #10923 hint, mirroring the dynamic path's
/// `generator_binding_from_generator_value` constraint).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn generator_parts_from_constructor<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<(Expr, String, Expr)> {
    if builtin_args.len() != 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "quote expansion supports only single-binding, unfiltered generators \
                 (Issue #10923)"
                    .to_string(),
            ),
            span,
        ));
    }
    let binding = &builtin_args[2];
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: binding_args,
        ..
    } = binding
    else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "generator binding must be an assignment expression".to_string(),
            ),
            span,
        ));
    };
    if binding_args.len() < 3
        || extract_symbol_from_constructor(&binding_args[0])
            .ok()
            .and_then(|head| ExprHead::from_name(&head))
            != Some(ExprHead::Assign)
    {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "generator binding must be a `var = iterable` assignment".to_string(),
            ),
            span,
        ));
    }
    let var_name = hygiene.resolve(&extract_symbol_from_constructor(&binding_args[1])?);
    let iter = quote_constructor_to_code_with_hygiene(
        &binding_args[2],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    let body = quote_constructor_to_code_with_hygiene(
        &builtin_args[1],
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    Ok((body, var_name, iter))
}

/// `Expr(:generator, body, Expr(:(=), :var, iter))` — a generator expression
/// inside a stdlib macro's `quote` body (Issue #10916). The binding variable
/// resolves through the Pass-1 hygiene map (its `Expr(:(=), ...)` shape is
/// registered by the generic `Assign` recursion, exactly like a `for`
/// binding — see `handle_for_expr`), producing the same `Expr::Generator` IR
/// as the dynamic path's Issue #10626 arm.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_generator_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    let (body, var, iter) = generator_parts_from_constructor(
        builtin_args,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    Ok(Expr::Generator {
        body: Box::new(body),
        var: var.into(),
        iter: Box::new(iter),
        filter: None,
        span,
    })
}

/// `Expr(:comprehension, Expr(:generator, ...))` — a comprehension inside a
/// stdlib macro's `quote` body (Issue #10916). Same single-binding,
/// unfiltered constraint (Issue #10923) and hygiene behavior as
/// [`handle_generator_expr`], producing `Expr::Comprehension` — identical IR
/// to the dynamic path's Issue #10626 arm.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_comprehension_expr<'a>(
    builtin_args: &[Expr],
    params: &[String],
    args: &[Node<'a>],
    span: crate::span::Span,
    walker: &CstWalker<'a>,
    lambda_ctx: &LambdaContext,
    hygiene: &HygieneContext,
    has_varargs: bool,
) -> LowerResult<Expr> {
    if builtin_args.len() != 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "comprehension expression requires exactly one generator argument".to_string(),
            ),
            span,
        ));
    }
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args: gen_args,
        ..
    } = &builtin_args[1]
    else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "comprehension argument must be a generator expression".to_string(),
            ),
            span,
        ));
    };
    if gen_args
        .first()
        .and_then(|head| extract_symbol_from_constructor(head).ok())
        .and_then(|head| ExprHead::from_name(&head))
        != Some(ExprHead::Generator)
    {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "comprehension argument must be a generator expression".to_string(),
            ),
            span,
        ));
    }
    let (body, var, iter) = generator_parts_from_constructor(
        gen_args,
        params,
        args,
        span,
        walker,
        lambda_ctx,
        hygiene,
        has_varargs,
    )?;
    Ok(Expr::Comprehension {
        body: Box::new(body),
        var: var.into(),
        iter: Box::new(iter),
        filter: None,
        span,
    })
}

/// Extract a symbol name from a SymbolNew constructor expression.
pub(in crate::lowering::expr) fn extract_symbol_from_constructor(
    expr: &Expr,
) -> Result<String, UnsupportedFeature> {
    match expr {
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args,
            span,
        } => {
            if let Some(Expr::Literal(Literal::Str(name), _)) = args.first() {
                Ok(name.clone())
            } else {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedExpression(
                        "SymbolNew with non-string argument".to_string(),
                    ),
                    *span,
                ))
            }
        }
        Expr::Builtin { span, .. } | Expr::Literal(_, span) | Expr::Var(_, span) => {
            Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedExpression(
                    "expected SymbolNew expression".to_string(),
                ),
                *span,
            ))
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "expected SymbolNew expression".to_string(),
            ),
            crate::span::Span::new(0, 0, 0, 0, 0, 0),
        )),
    }
}

/// Collect variables introduced in a quote constructor that need hygiene renaming.
/// This is the first pass of macro hygiene - we collect all local variable declarations
/// that are NOT inside an esc() call.
pub(in crate::lowering::expr) fn collect_introduced_vars(
    constructor: &Expr,
    hygiene: &mut HygieneContext,
    in_esc: bool,
) {
    match constructor {
        Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args: builtin_args,
            ..
        } => {
            // Note: use early-return rather than a `if builtin_args.len() >= 2` guard on the
            // match arm: the next arm `Expr::Builtin { args, .. }` would otherwise silently
            // match and recurse on len-0/1 ExprNew, which is a behavior change. (#3626)
            if builtin_args.len() < 2 {
                return;
            }
            let Ok(head) = extract_symbol_from_constructor(&builtin_args[0]) else {
                return;
            };
            match ExprHead::from_name(&head) {
                Some(ExprHead::Call) => {
                    // Check if this is esc(...) call. `esc` detection here is
                    // by CALLEE NAME on an un-evaluated `Expr(:call, :esc, ...)`
                    // constructor node, unlike the dynamic path (which detects
                    // the *already-evaluated* `Expr(:escape, ...)`/
                    // `Expr(:hygienic-scope, ...)` runtime head directly) --
                    // an inherent consequence of this path working on the
                    // pre-execution constructor tree rather than a runtime
                    // value, not something `quote_binding_role` classifies.
                    if builtin_args.len() >= 3 {
                        if let Ok(func) = extract_symbol_from_constructor(&builtin_args[1]) {
                            if func == "esc" {
                                // Inside esc() - recurse with in_esc=true
                                for arg in &builtin_args[2..] {
                                    collect_introduced_vars(arg, hygiene, true);
                                }
                                return;
                            }
                        }
                    }
                    // Regular call - recurse into arguments
                    for arg in &builtin_args[2..] {
                        collect_introduced_vars(arg, hygiene, in_esc);
                    }
                }
                // Issue #10627: the remaining heads all dispatch through the
                // shared `quote_binding_role` classifier (`expr_heads.rs`),
                // which is also consulted by the dynamic path's
                // `collect_quote_local_names` (`macro_runtime.rs`) -- the
                // per-head "does this introduce a binding, where" decision now
                // has one source of truth instead of two independently
                // hand-maintained matches.
                Some(other_head) => match quote_binding_role(other_head) {
                    QuoteBindingRole::LocalDecl => {
                        // local declaration - collect variable names (unless in esc)
                        if !in_esc {
                            for inner in &builtin_args[1..] {
                                collect_local_var_name(inner, hygiene);
                            }
                        }
                    }
                    QuoteBindingRole::Assign => {
                        // Assignment - collect the target variable(s) (unless in
                        // esc). Mirrors the dynamic path's
                        // `collect_assignment_target_names` (`macro_runtime.rs`):
                        // a `Tuple`/`TypeAssert` destructuring target such as
                        // `(a, b) = f()` or `x::Int = 1` registers every bare
                        // name inside it, not just a bare-`Symbol` target
                        // (Issue #10980).
                        if !in_esc && builtin_args.len() >= 3 {
                            register_assignment_target_names(&builtin_args[1], hygiene);
                        }
                        // Recurse into value
                        if builtin_args.len() >= 3 {
                            collect_introduced_vars(&builtin_args[2], hygiene, in_esc);
                        }
                    }
                    QuoteBindingRole::TryCatchVar => {
                        // Expr(:try, try_block, catch_var_or_false, catch_block_or_false,
                        //      [finally_block_or_false[, else_block]])
                        // The `catch` variable is itself a local introduced by the
                        // quote (scoped to the catch block) and must be
                        // hygiene-renamed like `local`/assignment targets —
                        // otherwise it leaks into the surrounding scope under its
                        // literal name and can shadow a same-named user/global
                        // variable for the rest of that scope (Issue #10242).
                        if let Some(try_block) = builtin_args.get(1) {
                            collect_introduced_vars(try_block, hygiene, in_esc);
                        }
                        if !in_esc {
                            if let Some(catch_var) = builtin_args.get(2) {
                                if let Ok(var_name) = extract_symbol_from_constructor(catch_var) {
                                    hygiene.register_local(&var_name);
                                }
                            }
                        }
                        if let Some(catch_block) = builtin_args.get(3) {
                            collect_introduced_vars(catch_block, hygiene, in_esc);
                        }
                        if let Some(finally_block) = builtin_args.get(4) {
                            collect_introduced_vars(finally_block, hygiene, in_esc);
                        }
                        if let Some(else_block) = builtin_args.get(5) {
                            collect_introduced_vars(else_block, hygiene, in_esc);
                        }
                    }
                    // `FunctionName` (Issue #10916): a nested named function
                    // definition's own NAME is a local the quote introduces,
                    // registered for rename exactly like the dynamic path's
                    // #8064 behavior — same-expansion call sites of that name
                    // rename consistently through the shared flat map. The
                    // signature's PARAMETER names (and `where` binders) stay
                    // deliberately unregistered (#10626/#10925: a flat rename
                    // would clobber an unrelated sibling reference sharing the
                    // bare name; call-frame scoping isolates them at runtime).
                    // An anonymous signature (`Expr(:tuple, ...)`) has no name
                    // to register. Generic recursion still covers the body.
                    QuoteBindingRole::FunctionName => {
                        if !in_esc {
                            if let Some(name) =
                                function_name_from_signature_constructor(&builtin_args[1])
                            {
                                hygiene.register_local(&name);
                            }
                        }
                        for arg in &builtin_args[1..] {
                            collect_introduced_vars(arg, hygiene, in_esc);
                        }
                    }
                    // `None` -- covers `Block`/`While`/`For`/`Let` (a
                    // `for`/`let` binding is itself a nested `Assign` node,
                    // registered when recursion reaches it) and every other
                    // non-binding-introducing head.
                    QuoteBindingRole::None => {
                        for arg in &builtin_args[1..] {
                            collect_introduced_vars(arg, hygiene, in_esc);
                        }
                    }
                },
                None => {
                    // Unrecognized head - recurse into arguments
                    for arg in &builtin_args[1..] {
                        collect_introduced_vars(arg, hygiene, in_esc);
                    }
                }
            }
        }
        Expr::Builtin {
            args: builtin_args, ..
        } => {
            for arg in builtin_args {
                collect_introduced_vars(arg, hygiene, in_esc);
            }
        }
        _ => {}
    }
}

/// Best-effort literal name of a named function-definition signature
/// constructor — unwraps an optional `Expr(:where, ...)` wrapper and reads
/// the `Expr(:call, :name, ...)` callee. Returns `None` for an anonymous
/// (`Expr(:tuple, ...)`) or unrecognized signature, which then simply
/// registers nothing (under-registering is always safe: the name stays
/// un-renamed, the pre-#10916 behavior).
fn function_name_from_signature_constructor(signature: &Expr) -> Option<String> {
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = signature
    else {
        return None;
    };
    let head = extract_symbol_from_constructor(args.first()?).ok()?;
    match ExprHead::from_name(&head) {
        Some(ExprHead::Where) => args
            .get(1)
            .and_then(function_name_from_signature_constructor),
        Some(ExprHead::Call) => extract_symbol_from_constructor(args.get(1)?).ok(),
        _ => None,
    }
}

/// Register every bare name bound by an assignment target constructor,
/// recursively unwrapping `Tuple` / `TypeAssert` destructuring targets —
/// `(a, b) = f()` registers `a` and `b`, `x::Int = 1` registers `x` — the
/// static-path mirror of the dynamic path's `collect_assignment_target_names`
/// (`macro_runtime.rs`), so the two engines' `QuoteBindingRole::Assign`
/// handling stays symmetric (Issue #10980). Non-symbol leaves (`obj.field`,
/// `arr[i]`, splices) introduce no quote-local binding and are skipped, same
/// as the dynamic path.
fn register_assignment_target_names(target: &Expr, hygiene: &mut HygieneContext) {
    if let Ok(name) = extract_symbol_from_constructor(target) {
        hygiene.register_local(&name);
        return;
    }
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = target
    else {
        return;
    };
    if args.len() < 2 {
        return;
    }
    let Ok(head) = extract_symbol_from_constructor(&args[0]) else {
        return;
    };
    match ExprHead::from_name(&head) {
        Some(ExprHead::Tuple) => {
            for arg in &args[1..] {
                register_assignment_target_names(arg, hygiene);
            }
        }
        Some(ExprHead::TypeAssert) => {
            if let Some(inner) = args.get(1) {
                register_assignment_target_names(inner, hygiene);
            }
        }
        _ => {}
    }
}

/// Collect the set of names a quote constructor introduces as its own locals
/// (assignment targets, `local` declarations, `catch` variables), i.e. the
/// Pass-1 hygiene collection reused as a standalone query. Names inside
/// `esc(...)` are excluded, and names that only enter the expansion through a
/// `$param` splice never appear here (a splice position is a lowered value
/// expression, not a literal `Symbol` constructor). Used by the dynamic
/// engine (`macro_runtime.rs`) to rename module-owned macros' OWN
/// quote-introduced locals without touching caller-spliced names
/// (Issue #10977).
pub fn collect_quote_constructor_introduced_names(
    constructor: &Expr,
) -> std::collections::HashSet<String> {
    let mut hygiene = HygieneContext::new();
    collect_introduced_vars(constructor, &mut hygiene, false);
    hygiene.registered_names()
}

/// Helper to extract variable name from local declaration inner expression.
fn collect_local_var_name(inner: &Expr, hygiene: &mut HygieneContext) {
    match inner {
        // local x (just a symbol)
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args,
            ..
        } => {
            if let Some(Expr::Literal(Literal::Str(name), _)) = args.first() {
                hygiene.register_local(name);
            }
        }
        // local x = value (assignment inside local)
        Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args: inner_args,
            ..
        } if inner_args.len() >= 2 => {
            if let Ok(head) = extract_symbol_from_constructor(&inner_args[0]) {
                if head == "=" && inner_args.len() >= 3 {
                    if let Ok(var_name) = extract_symbol_from_constructor(&inner_args[1]) {
                        hygiene.register_local(&var_name);
                    }
                } else if head == "call" && inner_args.len() >= 4 {
                    // Expr(:call, :(=), :var, value) pattern
                    if let Ok(op) = extract_symbol_from_constructor(&inner_args[1]) {
                        if op == "=" {
                            if let Ok(var_name) = extract_symbol_from_constructor(&inner_args[2]) {
                                hygiene.register_local(&var_name);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod collect_introduced_vars_tests {
    //! Issue #10626: regression coverage for the explicit `For`/`While`/`Let`
    //! arms added to `collect_introduced_vars`. These construct the exact
    //! `Expr::Builtin(ExprNew, ...)` shapes `cst_to_expr_constructor` builds
    //! for each source form and drive `collect_introduced_vars` directly,
    //! since none of these forms are reachable end-to-end through any current
    //! real stdlib/Base macro (the static quote-expansion path is used only
    //! by stdlib macros — see the "Static Stdlib/Base Macro Quote-Expansion
    //! Hygiene" section of docs/vm/LOWERING.md).

    use super::*;
    use crate::span::Span;

    fn span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn symbol(name: &str) -> Expr {
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args: vec![Expr::Literal(Literal::Str(name.to_string()), span())],
            span: span(),
        }
    }

    fn expr_new(args: Vec<Expr>) -> Expr {
        Expr::Builtin {
            name: BuiltinOp::ExprNew,
            args,
            span: span(),
        }
    }

    fn int_literal(n: i64) -> Expr {
        Expr::Literal(Literal::Int(n), span())
    }

    /// `Expr(:for, Expr(:(=), var, iterable), body)`.
    fn for_constructor(var: &str, iterable: Expr, body: Expr) -> Expr {
        expr_new(vec![
            symbol("for"),
            expr_new(vec![symbol("="), symbol(var), iterable]),
            body,
        ])
    }

    /// `Expr(:let, Expr(:(=), var, value), body)` (single binding).
    fn let_constructor(var: &str, value: Expr, body: Expr) -> Expr {
        expr_new(vec![
            symbol("let"),
            expr_new(vec![symbol("="), symbol(var), value]),
            body,
        ])
    }

    /// `Expr(:while, condition, body)`.
    fn while_constructor(condition: Expr, body: Expr) -> Expr {
        expr_new(vec![symbol("while"), condition, body])
    }

    /// `esc(inner)` -> `Expr(:call, :esc, inner)`.
    fn esc_call(inner: Expr) -> Expr {
        expr_new(vec![symbol("call"), symbol("esc"), inner])
    }

    #[test]
    fn for_loop_variable_is_registered_and_renamed() {
        let mut hygiene = HygieneContext::new();
        let constructor = for_constructor("i", int_literal(3), symbol("i"));
        collect_introduced_vars(&constructor, &mut hygiene, false);

        let resolved = hygiene.resolve("i");
        assert_ne!(resolved, "i", "for-loop variable should be gensym'd");
        assert!(resolved.starts_with("#i#"), "got {:?}", resolved);
    }

    #[test]
    fn for_loop_variable_inside_esc_is_not_renamed() {
        let mut hygiene = HygieneContext::new();
        let constructor = esc_call(for_constructor("i", int_literal(3), symbol("i")));
        collect_introduced_vars(&constructor, &mut hygiene, false);

        assert_eq!(
            hygiene.resolve("i"),
            "i",
            "an escaped for-loop should not have its variable renamed"
        );
    }

    #[test]
    fn let_binding_is_registered_and_renamed() {
        let mut hygiene = HygieneContext::new();
        let constructor = let_constructor("val", int_literal(10), symbol("val"));
        collect_introduced_vars(&constructor, &mut hygiene, false);

        let resolved = hygiene.resolve("val");
        assert_ne!(resolved, "val", "let binding should be gensym'd");
        assert!(resolved.starts_with("#val#"), "got {:?}", resolved);
    }

    #[test]
    fn let_binding_inside_esc_is_not_renamed() {
        let mut hygiene = HygieneContext::new();
        let constructor = esc_call(let_constructor("val", int_literal(10), symbol("val")));
        collect_introduced_vars(&constructor, &mut hygiene, false);

        assert_eq!(
            hygiene.resolve("val"),
            "val",
            "an escaped let binding should not have its variable renamed"
        );
    }

    /// `Expr(:(=), Expr(:tuple, :a, :b), value)` — tuple destructuring target.
    fn tuple_assign_constructor(names: &[&str], value: Expr) -> Expr {
        let mut tuple_args = vec![symbol("tuple")];
        tuple_args.extend(names.iter().map(|n| symbol(n)));
        expr_new(vec![symbol("="), expr_new(tuple_args), value])
    }

    /// `Expr(:(=), Expr(:(::), :x, :T), value)` — type-asserted target.
    fn type_assert_assign_constructor(name: &str, ty: &str, value: Expr) -> Expr {
        expr_new(vec![
            symbol("="),
            expr_new(vec![symbol("::"), symbol(name), symbol(ty)]),
            value,
        ])
    }

    // Issue #10980: the `Assign` arm mirrors the dynamic path's
    // `collect_assignment_target_names` recursion — `Tuple`/`TypeAssert`
    // destructuring targets register every bare name, not just a bare-`Symbol`
    // target. Not reachable end-to-end today (no shipped stdlib/Base macro
    // quote body contains a destructuring assignment target), so covered here
    // as direct unit tests on the collector.

    #[test]
    fn tuple_destructuring_assignment_targets_are_all_registered_10980() {
        let mut hygiene = HygieneContext::new();
        let constructor = tuple_assign_constructor(&["a", "b"], int_literal(1));
        collect_introduced_vars(&constructor, &mut hygiene, false);

        for name in ["a", "b"] {
            let resolved = hygiene.resolve(name);
            assert_ne!(
                resolved, name,
                "tuple destructuring target {name:?} should be gensym'd"
            );
            assert!(
                resolved.starts_with(&format!("#{name}#")),
                "got {resolved:?}"
            );
        }
    }

    #[test]
    fn nested_tuple_destructuring_assignment_targets_are_registered_10980() {
        // Expr(:(=), Expr(:tuple, :a, Expr(:tuple, :b, :c)), 1)
        let mut hygiene = HygieneContext::new();
        let inner = expr_new(vec![symbol("tuple"), symbol("b"), symbol("c")]);
        let target = expr_new(vec![symbol("tuple"), symbol("a"), inner]);
        let constructor = expr_new(vec![symbol("="), target, int_literal(1)]);
        collect_introduced_vars(&constructor, &mut hygiene, false);

        for name in ["a", "b", "c"] {
            assert_ne!(
                hygiene.resolve(name),
                name,
                "nested tuple destructuring target {name:?} should be gensym'd"
            );
        }
    }

    #[test]
    fn type_assert_assignment_target_is_registered_10980() {
        let mut hygiene = HygieneContext::new();
        let constructor = type_assert_assign_constructor("x", "Int", int_literal(1));
        collect_introduced_vars(&constructor, &mut hygiene, false);

        let resolved = hygiene.resolve("x");
        assert_ne!(resolved, "x", "type-asserted target should be gensym'd");
        assert_eq!(
            hygiene.resolve("Int"),
            "Int",
            "the ASSERTED TYPE name must not be registered as a local"
        );
    }

    #[test]
    fn tuple_destructuring_inside_esc_is_not_renamed_10980() {
        let mut hygiene = HygieneContext::new();
        let constructor = esc_call(tuple_assign_constructor(&["a", "b"], int_literal(1)));
        collect_introduced_vars(&constructor, &mut hygiene, false);

        for name in ["a", "b"] {
            assert_eq!(
                hygiene.resolve(name),
                name,
                "an escaped tuple destructuring target must not be renamed"
            );
        }
    }

    #[test]
    fn collect_quote_constructor_introduced_names_reports_exact_set_10980() {
        // block( (a, b) = 1; x::Int = 2; plain = 3 ) — the standalone Pass-1
        // query (consumed by the dynamic engine, Issue #10977) reports exactly
        // the introduced names, excluding the asserted type name.
        let constructor = expr_new(vec![
            symbol("block"),
            tuple_assign_constructor(&["a", "b"], int_literal(1)),
            type_assert_assign_constructor("x", "Int", int_literal(2)),
            expr_new(vec![symbol("="), symbol("plain"), int_literal(3)]),
        ]);
        let names = collect_quote_constructor_introduced_names(&constructor);
        let expected: std::collections::HashSet<String> = ["a", "b", "x", "plain"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(names, expected);
    }

    // Issue #10916: the `FunctionName` role arm registers a nested named
    // function definition's own NAME (mirroring the dynamic path's #8064
    // behavior) — parameters and `where` binders stay unregistered
    // (#10626/#10925 flat-map safety).

    #[test]
    fn named_function_definition_name_is_registered_params_are_not_10916() {
        // Expr(:function, Expr(:call, :helper, :n), Expr(:call, :+, :n, 1))
        let mut hygiene = HygieneContext::new();
        let constructor = expr_new(vec![
            symbol("function"),
            expr_new(vec![symbol("call"), symbol("helper"), symbol("n")]),
            expr_new(vec![
                symbol("call"),
                symbol("+"),
                symbol("n"),
                int_literal(1),
            ]),
        ]);
        collect_introduced_vars(&constructor, &mut hygiene, false);

        let resolved = hygiene.resolve("helper");
        assert_ne!(resolved, "helper", "function name should be gensym'd");
        assert!(resolved.starts_with("#helper#"), "got {resolved:?}");
        assert_eq!(
            hygiene.resolve("n"),
            "n",
            "function parameters must NOT be registered (flat-map safety, #10626/#10925)"
        );
    }

    #[test]
    fn where_wrapped_function_name_is_registered_binder_is_not_10916() {
        // Expr(:function, Expr(:where, Expr(:call, :f, :x), :T), :x)
        let mut hygiene = HygieneContext::new();
        let constructor = expr_new(vec![
            symbol("function"),
            expr_new(vec![
                symbol("where"),
                expr_new(vec![symbol("call"), symbol("f"), symbol("x")]),
                symbol("T"),
            ]),
            symbol("x"),
        ]);
        collect_introduced_vars(&constructor, &mut hygiene, false);

        assert_ne!(hygiene.resolve("f"), "f");
        assert_eq!(
            hygiene.resolve("T"),
            "T",
            "where binder must NOT be registered"
        );
        assert_eq!(
            hygiene.resolve("x"),
            "x",
            "parameter must NOT be registered"
        );
    }

    #[test]
    fn escaped_function_definition_name_is_not_registered_10916() {
        let mut hygiene = HygieneContext::new();
        let constructor = esc_call(expr_new(vec![
            symbol("function"),
            expr_new(vec![symbol("call"), symbol("helper")]),
            int_literal(1),
        ]));
        collect_introduced_vars(&constructor, &mut hygiene, false);
        assert_eq!(
            hygiene.resolve("helper"),
            "helper",
            "an escaped function definition's name must not be renamed"
        );
    }

    #[test]
    fn anonymous_function_signature_registers_nothing_10916() {
        // Expr(:function, Expr(:tuple, :x), :x) — no name to register.
        let mut hygiene = HygieneContext::new();
        let constructor = expr_new(vec![
            symbol("function"),
            expr_new(vec![symbol("tuple"), symbol("x")]),
            symbol("x"),
        ]);
        collect_introduced_vars(&constructor, &mut hygiene, false);
        assert_eq!(hygiene.resolve("x"), "x");
    }

    #[test]
    fn while_body_assignment_is_still_registered() {
        // Expr(:while, cond, Expr(:block, Expr(:(=), :acc, 1)))
        let mut hygiene = HygieneContext::new();
        let body = expr_new(vec![
            symbol("block"),
            expr_new(vec![symbol("="), symbol("acc"), int_literal(1)]),
        ]);
        let constructor = while_constructor(symbol("cond"), body);
        collect_introduced_vars(&constructor, &mut hygiene, false);

        let resolved = hygiene.resolve("acc");
        assert_ne!(
            resolved, "acc",
            "a plain assignment inside a while body should still be gensym'd"
        );
    }
}
