//! Nested macro expansion with pre-computed Expr arguments.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::lowering::LowerResult;
use super::super::quote::{collect_introduced_vars, extract_symbol_from_constructor, HygieneContext};

/// Expand a nested macro call with pre-computed Expr arguments.
/// This is used when a macro calls another macro in its body.
pub(crate) fn expand_nested_macro_from_expr_args(
    macro_def: &crate::lowering::StoredMacroDef,
    args: &[Expr],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let stmts = &macro_def.body.stmts;

    if stmts.is_empty() {
        return Ok(Expr::Literal(Literal::Nothing, span));
    }

    if stmts.len() == 1 {
        return match &stmts[0] {
            crate::ir::core::Stmt::Expr { expr, .. } => {
                substitute_expr_args_in_macro(expr, &macro_def.params, args, span, lambda_ctx)
            }
            crate::ir::core::Stmt::Return {
                value: Some(expr), ..
            } => substitute_expr_args_in_macro(expr, &macro_def.params, args, span, lambda_ctx),
            _ => Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("nested macro expansion only supports expression macros"),
            ),
        };
    }

    let mut expanded_stmts = Vec::new();
    for stmt in stmts {
        match stmt {
            crate::ir::core::Stmt::Expr {
                expr,
                span: stmt_span,
            } => {
                let expanded =
                    substitute_expr_args_in_macro(expr, &macro_def.params, args, span, lambda_ctx)?;
                expanded_stmts.push(crate::ir::core::Stmt::Expr {
                    expr: expanded,
                    span: *stmt_span,
                });
            }
            crate::ir::core::Stmt::Assign {
                var,
                value,
                span: stmt_span,
            } => {
                let expanded_value = substitute_expr_args_in_macro(
                    value,
                    &macro_def.params,
                    args,
                    span,
                    lambda_ctx,
                )?;
                expanded_stmts.push(crate::ir::core::Stmt::Assign {
                    var: var.clone(),
                    value: expanded_value,
                    span: *stmt_span,
                });
            }
            crate::ir::core::Stmt::Return {
                value: Some(expr),
                span: stmt_span,
            } => {
                let expanded =
                    substitute_expr_args_in_macro(expr, &macro_def.params, args, span, lambda_ctx)?;
                expanded_stmts.push(crate::ir::core::Stmt::Expr {
                    expr: expanded,
                    span: *stmt_span,
                });
            }
            _ => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                        "nested macro expansion only supports expression and assignment statements",
                    ),
                );
            }
        }
    }

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: crate::ir::core::Block {
            stmts: expanded_stmts,
            span,
        },
        span,
    })
}

/// Substitute parameters in a macro expression with pre-computed Expr values.
pub(crate) fn substitute_expr_args_in_macro(
    expr: &Expr,
    params: &[String],
    args: &[Expr],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    match expr {
        Expr::Var(name, var_span) => {
            if let Some(idx) = params.iter().position(|p| p == name) {
                Ok(args[idx].clone())
            } else {
                Ok(Expr::Var(name.clone(), *var_span))
            }
        }
        Expr::QuoteLiteral {
            constructor,
            span: quote_span,
        } => {
            quote_constructor_to_code_with_expr_args(constructor, params, args, *quote_span, lambda_ctx)
        }
        Expr::BinaryOp { op, left, right, span: op_span } => {
            let new_left = substitute_expr_args_in_macro(left, params, args, span, lambda_ctx)?;
            let new_right = substitute_expr_args_in_macro(right, params, args, span, lambda_ctx)?;
            Ok(Expr::BinaryOp { op: *op, left: Box::new(new_left), right: Box::new(new_right), span: *op_span })
        }
        Expr::UnaryOp { op, operand, span: op_span } => {
            let new_operand = substitute_expr_args_in_macro(operand, params, args, span, lambda_ctx)?;
            Ok(Expr::UnaryOp { op: *op, operand: Box::new(new_operand), span: *op_span })
        }
        Expr::Call { function, args: call_args, kwargs, splat_mask, kwargs_splat_mask, span: call_span } => {
            let new_args: Result<Vec<_>, _> = call_args.iter().map(|a| substitute_expr_args_in_macro(a, params, args, span, lambda_ctx)).collect();
            let new_kwargs: Result<Vec<_>, _> = kwargs.iter().map(|(k, v)| substitute_expr_args_in_macro(v, params, args, span, lambda_ctx).map(|nv| (k.clone(), nv))).collect();
            Ok(Expr::Call { function: function.clone(), args: new_args?, kwargs: new_kwargs?, splat_mask: splat_mask.clone(), kwargs_splat_mask: kwargs_splat_mask.clone(), span: *call_span })
        }
        Expr::Builtin { name, args: builtin_args, span: builtin_span } => {
            let new_args: Result<Vec<_>, _> = builtin_args.iter().map(|a| substitute_expr_args_in_macro(a, params, args, span, lambda_ctx)).collect();
            Ok(Expr::Builtin { name: *name, args: new_args?, span: *builtin_span })
        }
        _ => Ok(expr.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Literal, UnaryOp};
    use crate::lowering::LambdaContext;
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    // ── substitute_expr_args_in_macro ─────────────────────────────────────────

    #[test]
    fn test_substitute_var_matching_param_is_replaced() {
        // Var("x") with param ["x"] and arg Literal::Int(42) → Int(42)
        let ctx = LambdaContext::new();
        let expr = Expr::Var("x".to_string(), s());
        let params = vec!["x".to_string()];
        let args = vec![Expr::Literal(Literal::Int(42), s())];
        let result = substitute_expr_args_in_macro(&expr, &params, &args, s(), &ctx).unwrap();
        assert!(matches!(result, Expr::Literal(Literal::Int(42), _)));
    }

    #[test]
    fn test_substitute_var_not_matching_param_unchanged() {
        // Var("y") with param ["x"] → Var("y") unchanged
        let ctx = LambdaContext::new();
        let expr = Expr::Var("y".to_string(), s());
        let params = vec!["x".to_string()];
        let args = vec![Expr::Literal(Literal::Int(1), s())];
        let result = substitute_expr_args_in_macro(&expr, &params, &args, s(), &ctx).unwrap();
        assert!(matches!(&result, Expr::Var(name, _) if name == "y"),
            "Expected Var(y), got {:?}", result);
    }

    #[test]
    fn test_substitute_literal_passes_through_unchanged() {
        let ctx = LambdaContext::new();
        let expr = Expr::Literal(Literal::Bool(true), s());
        let params: Vec<String> = vec![];
        let args: Vec<Expr> = vec![];
        let result = substitute_expr_args_in_macro(&expr, &params, &args, s(), &ctx).unwrap();
        assert!(matches!(result, Expr::Literal(Literal::Bool(true), _)));
    }

    #[test]
    fn test_substitute_binary_op_substitutes_in_operands() {
        // (x + 1) with param ["x"] → (42 + 1)
        let ctx = LambdaContext::new();
        let expr = Expr::BinaryOp {
            op: BinaryOp::Add,
            left: Box::new(Expr::Var("x".to_string(), s())),
            right: Box::new(Expr::Literal(Literal::Int(1), s())),
            span: s(),
        };
        let params = vec!["x".to_string()];
        let args = vec![Expr::Literal(Literal::Int(42), s())];
        let result = substitute_expr_args_in_macro(&expr, &params, &args, s(), &ctx).unwrap();
        assert!(matches!(result, Expr::BinaryOp { .. }), "Expected BinaryOp, got {:?}", result);
        if let Expr::BinaryOp { op, left, right, .. } = result {
            assert_eq!(op, BinaryOp::Add);
            assert!(matches!(*left, Expr::Literal(Literal::Int(42), _)));
            assert!(matches!(*right, Expr::Literal(Literal::Int(1), _)));
        }
    }

    #[test]
    fn test_substitute_unary_op_substitutes_in_operand() {
        // -x with param ["x"] → -42
        let ctx = LambdaContext::new();
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand: Box::new(Expr::Var("x".to_string(), s())),
            span: s(),
        };
        let params = vec!["x".to_string()];
        let args = vec![Expr::Literal(Literal::Int(42), s())];
        let result = substitute_expr_args_in_macro(&expr, &params, &args, s(), &ctx).unwrap();
        assert!(matches!(result, Expr::UnaryOp { .. }), "Expected UnaryOp, got {:?}", result);
        if let Expr::UnaryOp { op, operand, .. } = result {
            assert_eq!(op, UnaryOp::Neg);
            assert!(matches!(*operand, Expr::Literal(Literal::Int(42), _)));
        }
    }

    #[test]
    fn test_substitute_call_substitutes_in_args() {
        // f(x) with param ["x"] → f(42)
        let ctx = LambdaContext::new();
        let expr = Expr::Call {
            function: "f".to_string(),
            args: vec![Expr::Var("x".to_string(), s())],
            kwargs: vec![],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![],
            span: s(),
        };
        let params = vec!["x".to_string()];
        let args = vec![Expr::Literal(Literal::Int(99), s())];
        let result = substitute_expr_args_in_macro(&expr, &params, &args, s(), &ctx).unwrap();
        assert!(matches!(result, Expr::Call { .. }), "Expected Call, got {:?}", result);
        if let Expr::Call { function, args: call_args, .. } = result {
            assert_eq!(function, "f");
            assert_eq!(call_args.len(), 1);
            assert!(matches!(call_args[0], Expr::Literal(Literal::Int(99), _)));
        }
    }

    #[test]
    fn test_substitute_multiple_params_second_param_replaced() {
        // Var("y") with params ["x", "y"] → arg for "y" (Str("hello"))
        let ctx = LambdaContext::new();
        let expr = Expr::Var("y".to_string(), s());
        let params = vec!["x".to_string(), "y".to_string()];
        let args = vec![
            Expr::Literal(Literal::Int(1), s()),
            Expr::Literal(Literal::Str("hello".to_string()), s()),
        ];
        let result = substitute_expr_args_in_macro(&expr, &params, &args, s(), &ctx).unwrap();
        assert!(matches!(&result, Expr::Literal(Literal::Str(s), _) if s == "hello"),
            "Expected Str(hello), got {:?}", result);
    }
}

/// Convert a quote constructor to executable code with pre-computed Expr arguments.
fn quote_constructor_to_code_with_expr_args(
    constructor: &Expr,
    params: &[String],
    args: &[Expr],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let mut hygiene = HygieneContext::new();
    collect_introduced_vars(constructor, &mut hygiene, false);
    qctc(constructor, params, args, span, lambda_ctx, &hygiene)
}

/// Internal function that does the actual conversion with hygiene context and Expr args.
fn qctc(
    c: &Expr,
    params: &[String],
    args: &[Expr],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
    hygiene: &HygieneContext,
) -> LowerResult<Expr> {
    use crate::ir::core::BinaryOp;
    match c {
        Expr::Builtin { name: BuiltinOp::ExprNew, args: ba, .. } => {
            if ba.len() < 2 { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("ExprNew with too few arguments".into()), span)); }
            let head = extract_symbol_from_constructor(&ba[0])?;
            match head.as_str() {
                "call" => {
                    let fs = extract_symbol_from_constructor(&ba[1])?;
                    if fs == "esc" && ba.len() >= 3 { return qctc(&ba[2], params, args, span, lambda_ctx, &hygiene.enter_escaped()); }
                    let ca: Result<Vec<_>, _> = ba[2..].iter().map(|a| qctc(a, params, args, span, lambda_ctx, hygiene)).collect();
                    let ca = ca?;
                    if ca.len() == 2 { if let Some(op) = match fs.as_str() { "+" => Some(BinaryOp::Add), "-" => Some(BinaryOp::Sub), "*" => Some(BinaryOp::Mul), "/" => Some(BinaryOp::Div), "^" => Some(BinaryOp::Pow), "%" => Some(BinaryOp::Mod), "==" => Some(BinaryOp::Eq), "!=" => Some(BinaryOp::Ne), "<" => Some(BinaryOp::Lt), "<=" => Some(BinaryOp::Le), ">" => Some(BinaryOp::Gt), ">=" => Some(BinaryOp::Ge), "&&" => Some(BinaryOp::And), "||" => Some(BinaryOp::Or), _ => None } { return Ok(Expr::BinaryOp { op, left: Box::new(ca[0].clone()), right: Box::new(ca[1].clone()), span }); } }
                    if ca.len() == 1 { if let Some(op) = match fs.as_str() { "!" => Some(crate::ir::core::UnaryOp::Not), "-" => Some(crate::ir::core::UnaryOp::Neg), "+" => Some(crate::ir::core::UnaryOp::Pos), _ => None } { return Ok(Expr::UnaryOp { op, operand: Box::new(ca[0].clone()), span }); } }
                    if fs == ":" { if ca.len() == 2 { return Ok(Expr::Range { start: Box::new(ca[0].clone()), stop: Box::new(ca[1].clone()), step: None, span }); } else if ca.len() == 3 { return Ok(Expr::Range { start: Box::new(ca[0].clone()), stop: Box::new(ca[2].clone()), step: Some(Box::new(ca[1].clone())), span }); } }
                    Ok(Expr::Call { function: fs, args: ca, kwargs: vec![], splat_mask: vec![], kwargs_splat_mask: vec![], span })
                }
                "block" => {
                    let mut stmts = Vec::new();
                    for sc in &ba[1..] {
                        if let Expr::Builtin { name: BuiltinOp::LineNumberNodeNew, .. } = sc { continue; }
                        if let Expr::Builtin { name: BuiltinOp::ExprNew, args: ia, .. } = sc {
                            if ia.len() >= 2 { if let Ok(ih) = extract_symbol_from_constructor(&ia[0]) { match ih.as_str() {
                                "=" if ia.len() >= 3 => { let vn = hygiene.resolve(&extract_symbol_from_constructor(&ia[1])?); stmts.push(crate::ir::core::Stmt::Assign { var: vn, value: qctc(&ia[2], params, args, span, lambda_ctx, hygiene)?, span }); continue; }
                                "local" if ia.len() >= 2 => { if let Expr::Builtin { name: BuiltinOp::ExprNew, args: la, .. } = &ia[1] { if la.len() >= 3 { if let Ok(lh) = extract_symbol_from_constructor(&la[0]) { if lh == "=" { let vn = hygiene.resolve(&extract_symbol_from_constructor(&la[1])?); stmts.push(crate::ir::core::Stmt::Assign { var: vn, value: qctc(&la[2], params, args, span, lambda_ctx, hygiene)?, span }); continue; } } } } continue; }
                                _ => {}
                            } } }
                        }
                        stmts.push(crate::ir::core::Stmt::Expr { expr: qctc(sc, params, args, span, lambda_ctx, hygiene)?, span });
                    }
                    if stmts.is_empty() { Ok(Expr::Literal(Literal::Nothing, span)) }
                    else if stmts.len() == 1 { if let crate::ir::core::Stmt::Expr { expr, .. } = stmts.remove(0) { Ok(expr) } else { Ok(Expr::Literal(Literal::Nothing, span)) } }
                    else { Ok(Expr::LetBlock { bindings: vec![], body: crate::ir::core::Block { stmts, span }, span }) }
                }
                "for" => {
                    if ba.len() < 3 { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("for expression requires at least 3 arguments (head, binding, body)".into()), span)); }
                    let (vn, it) = if let Expr::Builtin { name: BuiltinOp::ExprNew, args: bba, .. } = &ba[1] {
                        if bba.len() >= 3 { if let Ok(bh) = extract_symbol_from_constructor(&bba[0]) { if bh == "=" { (hygiene.resolve(&extract_symbol_from_constructor(&bba[1])?), qctc(&bba[2], params, args, span, lambda_ctx, hygiene)?) } else { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression(format!("for binding must be an assignment, got :{}", bh)), span)); } } else { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("for binding must be an assignment expression".into()), span)); } } else { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("for binding expression has too few arguments".into()), span)); }
                    } else { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("for binding must be an Expr".into()), span)); };
                    let be = qctc(&ba[2], params, args, span, lambda_ctx, hygiene)?;
                    Ok(Expr::LetBlock { bindings: vec![], body: crate::ir::core::Block { stmts: vec![crate::ir::core::Stmt::ForEach { var: vn, iterable: it, body: crate::ir::core::Block { stmts: vec![crate::ir::core::Stmt::Expr { expr: be, span }], span }, span }], span }, span })
                }
                "while" => {
                    if ba.len() < 3 { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("while expression requires at least 3 arguments (head, condition, body)".into()), span)); }
                    let cond = qctc(&ba[1], params, args, span, lambda_ctx, hygiene)?;
                    let be = qctc(&ba[2], params, args, span, lambda_ctx, hygiene)?;
                    Ok(Expr::LetBlock { bindings: vec![], body: crate::ir::core::Block { stmts: vec![crate::ir::core::Stmt::While { condition: cond, body: crate::ir::core::Block { stmts: vec![crate::ir::core::Stmt::Expr { expr: be, span }], span }, span }], span }, span })
                }
                "symboliclabel" => {
                    if ba.len() < 2 { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("symboliclabel requires a name argument".into()), span)); }
                    Ok(Expr::LetBlock { bindings: vec![], body: crate::ir::core::Block { stmts: vec![crate::ir::core::Stmt::Label { name: extract_symbol_from_constructor(&ba[1])?, span }], span }, span })
                }
                "symbolicgoto" => {
                    if ba.len() < 2 { return Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("symbolicgoto requires a name argument".into()), span)); }
                    Ok(Expr::LetBlock { bindings: vec![], body: crate::ir::core::Block { stmts: vec![crate::ir::core::Stmt::Goto { name: extract_symbol_from_constructor(&ba[1])?, span }], span }, span })
                }
                _ => Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression(format!("nested macro quote expansion for Expr(:{}, ...) not yet supported", head)), span)),
            }
        }
        Expr::Builtin { name: BuiltinOp::SymbolNew, args: ba, .. } => {
            if let Some(Expr::Literal(Literal::Str(name), _)) = ba.first() { if let Some(i) = params.iter().position(|p| p == name) { Ok(args[i].clone()) } else { Ok(Expr::Var(hygiene.resolve(name), span)) } }
            else { Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression("SymbolNew with non-string argument".into()), span)) }
        }
        Expr::Var(name, _) => { if let Some(i) = params.iter().position(|p| p == name) { Ok(args[i].clone()) } else { Ok(Expr::Var(hygiene.resolve(name), span)) } }
        Expr::Literal(lit, _) => Ok(Expr::Literal(lit.clone(), span)),
        Expr::Builtin { name: BuiltinOp::LineNumberNodeNew, .. } => Ok(Expr::Literal(Literal::Nothing, span)),
        _ => Err(UnsupportedFeature::new(UnsupportedFeatureKind::UnsupportedExpression(format!("nested macro expansion for {:?} not yet supported", c)), span)),
    }
}
