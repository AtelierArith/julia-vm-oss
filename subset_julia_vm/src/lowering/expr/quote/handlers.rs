//! Expression head handlers for quote expansion.
//!
//! Each handler processes a specific Expr head type (call, block, if, for, while, etc.)
//! during macro expansion. Also includes hygiene helpers for variable collection.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal};
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node};

use super::super::lower_expr_with_ctx;
use super::super::macros::expand_nested_macro_from_expr_args;
use super::code_generation::{expr_to_block, quote_constructor_to_code_with_hygiene};
use super::HygieneContext;

// Helper functions for handling different expression heads

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
    // Second arg is the function/operator symbol
    let func_symbol = extract_symbol_from_constructor(&builtin_args[1])?;

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
                        let expanded = lower_expr_with_ctx(walker, *arg_node, lambda_ctx)?;
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
        function: func_symbol,
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
                    match head.as_str() {
                        "=" => {
                            if inner_args.len() >= 3 {
                                let orig_var_name =
                                    extract_symbol_from_constructor(&inner_args[1])?;
                                // Apply hygiene renaming to the variable name
                                let var_name = hygiene.resolve(&orig_var_name);
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
                        }
                        "local" => {
                            if inner_args.len() >= 2 {
                                let local_inner = &inner_args[1];
                                if let Expr::Builtin {
                                    name: BuiltinOp::ExprNew,
                                    args: local_assign_args,
                                    ..
                                } = local_inner
                                {
                                    if local_assign_args.len() >= 3 {
                                        if let Ok(local_head) =
                                            extract_symbol_from_constructor(&local_assign_args[0])
                                        {
                                            // Check for Expr(:(=), :var, value) pattern
                                            if local_head == "=" {
                                                let orig_var_name =
                                                    extract_symbol_from_constructor(
                                                        &local_assign_args[1],
                                                    )?;
                                                // Apply hygiene renaming
                                                let var_name = hygiene.resolve(&orig_var_name);
                                                let value = quote_constructor_to_code_with_hygiene(
                                                    &local_assign_args[2],
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
                                            // Check for Expr(:call, :(=), :var, value) pattern
                                            if local_head == "call" && local_assign_args.len() >= 4
                                            {
                                                if let Ok(op) = extract_symbol_from_constructor(
                                                    &local_assign_args[1],
                                                ) {
                                                    if op == "=" {
                                                        let orig_var_name =
                                                            extract_symbol_from_constructor(
                                                                &local_assign_args[2],
                                                            )?;
                                                        // Apply hygiene renaming
                                                        let var_name =
                                                            hygiene.resolve(&orig_var_name);
                                                        let value =
                                                            quote_constructor_to_code_with_hygiene(
                                                                &local_assign_args[3],
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
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Expr::Builtin {
                                    name: BuiltinOp::SymbolNew,
                                    ..
                                } = local_inner
                                {
                                    continue;
                                }
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
    } else if crate::base_loader::has_base_macro(&macro_name) {
        crate::base_loader::get_base_macro(&macro_name)
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
    let mut named_fields: Vec<(String, Expr)> = Vec::new();

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
                            named_fields.push((field_name, field_value));
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
                            let expanded = lower_expr_with_ctx(walker, *arg_node, lambda_ctx)?;
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
    // Expr(:try, try_block, catch_var_or_false, catch_block_or_false[, finally_block])
    // Convert to Stmt::Try wrapped in LetBlock

    if builtin_args.len() < 4 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "try expression requires at least 4 arguments (head, try_block, catch_var, catch_block)"
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
    let catch_var = if let Expr::Literal(Literal::Bool(false), _) = &builtin_args[2] {
        None
    } else if let Ok(var_name) = extract_symbol_from_constructor(&builtin_args[2]) {
        Some(hygiene.resolve(&var_name))
    } else {
        None
    };

    // Parse catch block (builtin_args[3]) - can be false or a block
    let catch_block = if let Expr::Literal(Literal::Bool(false), _) = &builtin_args[3] {
        None
    } else {
        let catch_block_expr = quote_constructor_to_code_with_hygiene(
            &builtin_args[3],
            params,
            args,
            span,
            walker,
            lambda_ctx,
            hygiene,
            has_varargs,
        )?;
        Some(expr_to_block(catch_block_expr, span))
    };

    // Parse finally block (builtin_args[4]) if present
    let finally_block = if builtin_args.len() > 4 {
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

    // Create Stmt::Try and wrap in LetBlock
    let try_stmt = crate::ir::core::Stmt::Try {
        try_block,
        catch_var,
        catch_block,
        else_block: None,
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
            if builtin_args.len() >= 2 {
                if let Ok(head) = extract_symbol_from_constructor(&builtin_args[0]) {
                    match head.as_str() {
                        "call" => {
                            // Check if this is esc(...) call
                            if builtin_args.len() >= 3 {
                                if let Ok(func) = extract_symbol_from_constructor(&builtin_args[1])
                                {
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
                        "local" => {
                            // local declaration - collect variable names (unless in esc)
                            if !in_esc {
                                for inner in &builtin_args[1..] {
                                    collect_local_var_name(inner, hygiene);
                                }
                            }
                        }
                        "=" => {
                            // Assignment - collect the target variable (unless in esc)
                            if !in_esc && builtin_args.len() >= 3 {
                                if let Ok(var_name) =
                                    extract_symbol_from_constructor(&builtin_args[1])
                                {
                                    hygiene.register_local(&var_name);
                                }
                            }
                            // Recurse into value
                            if builtin_args.len() >= 3 {
                                collect_introduced_vars(&builtin_args[2], hygiene, in_esc);
                            }
                        }
                        "block" => {
                            // Block - recurse into all statements
                            for stmt in &builtin_args[1..] {
                                collect_introduced_vars(stmt, hygiene, in_esc);
                            }
                        }
                        _ => {
                            // Other expression heads - recurse into arguments
                            for arg in &builtin_args[1..] {
                                collect_introduced_vars(arg, hygiene, in_esc);
                            }
                        }
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
        } => {
            if inner_args.len() >= 2 {
                if let Ok(head) = extract_symbol_from_constructor(&inner_args[0]) {
                    if head == "=" && inner_args.len() >= 3 {
                        if let Ok(var_name) = extract_symbol_from_constructor(&inner_args[1]) {
                            hygiene.register_local(&var_name);
                        }
                    } else if head == "call" && inner_args.len() >= 4 {
                        // Expr(:call, :(=), :var, value) pattern
                        if let Ok(op) = extract_symbol_from_constructor(&inner_args[1]) {
                            if op == "=" {
                                if let Ok(var_name) =
                                    extract_symbol_from_constructor(&inner_args[2])
                                {
                                    hygiene.register_local(&var_name);
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
