//! Macro expansion logic for expression lowering.
//!
//! This module handles:
//! - User-defined macro expansion
//! - Base macro expansion
//! - `@macroexpand` macro
//! - Macro parameter substitution
//! - Nested macro expansion
//! - Macro hygiene (gensym counter, HygieneContext)

mod expand;
mod namedtuple;
mod nested;
mod static_eval;
mod views;

use std::sync::atomic::AtomicU64;
#[cfg(debug_assertions)]
use std::sync::OnceLock;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, BuiltinOp, Expr, Function, Literal, Stmt, TypedParam};
use crate::lowering::expr::quote::cst_to_macro_arg_constructor;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

// Re-exports for quote submodules (accessed via super::super::macros::)
pub(super) static GENSYM_COUNTER: AtomicU64 = AtomicU64::new(0);
pub(super) use nested::expand_nested_macro_from_expr_args;

// Re-export for parent mod.rs
pub(super) use views::lower_expr_with_views;

// Internal re-exports
use expand::{
    expand_base_macro_expr, expand_bundled_package_macro_expr, expand_user_defined_macro_expr,
    lower_macroexpand_expr,
};
use static_eval::lower_static_macro_expr;
use views::dotify_expr;

// Re-export for the statement-context macro path (lowering/stmt/macros).
pub(crate) use namedtuple::lower_namedtuple_macro_expr;

#[cfg(debug_assertions)]
fn macro_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("SJULIA_MACRO_DEBUG").is_ok())
}

#[cfg(debug_assertions)]
pub(super) fn macro_debug_log(args: std::fmt::Arguments<'_>) {
    if macro_debug_enabled() {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{args}");
    }
}

fn require_single_task_macro_arg<'a>(
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
) -> LowerResult<Node<'a>> {
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint(format!("@{} requires exactly one expression", macro_name)),
        );
    }
    Ok(args[0])
}

fn unsupported_opaque_closure_macro(span: crate::span::Span) -> UnsupportedFeature {
    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
        "opaque closures via Base.Experimental.@opaque are not supported yet (Issue #4289)",
    )
}

fn macro_symbol_constructor(name: &str, span: crate::span::Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
        span,
    }
}

fn macro_globalref_constructor(module: &str, name: &str, span: crate::span::Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::GlobalRefNew,
        args: vec![
            macro_symbol_constructor(module, span),
            macro_symbol_constructor(name, span),
        ],
        span,
    }
}

fn macro_expr_constructor(head: &str, mut args: Vec<Expr>, span: crate::span::Span) -> Expr {
    args.insert(0, macro_symbol_constructor(head, span));
    Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        span,
    }
}

fn lower_eval_module_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
) -> LowerResult<Option<Expr>> {
    if args.len() != 2 {
        return Ok(None);
    }

    let module_arg = cst_to_macro_arg_constructor(walker, args[0])?;
    let body = cst_to_macro_arg_constructor(walker, args[1])?;
    let quoted_body = macro_expr_constructor("quote", vec![body], span);
    let core_eval = macro_globalref_constructor("Core", "eval", span);
    let call = macro_expr_constructor("call", vec![core_eval, module_arg, quoted_body], span);

    Ok(Some(Expr::Builtin {
        name: BuiltinOp::Eval,
        args: vec![call],
        span,
    }))
}

fn lower_assume_effects_expr_compat<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let Some(target) = args.last().copied() else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@assume_effects requires at least one metadata setting"),
        );
    };

    if walker.kind(&target) == NodeKind::QuoteExpression {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@assume_effects expression compatibility requires a wrapped expression",
            ),
        );
    }

    super::lower_expr_with_ctx(walker, target, lambda_ctx)
}

fn lift_zero_arg_task_thunk<'a>(
    walker: &CstWalker<'a>,
    body_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let body_expr = super::lower_expr_with_ctx(walker, body_node, lambda_ctx)?;
    let lambda_name = lambda_ctx.next_lambda_name();
    let func = Function {
        name: lambda_name.clone(),
        params: Vec::<TypedParam>::new(),
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body_expr),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
    };
    lambda_ctx.add_lifted_function(func);

    Ok(Expr::FunctionRef {
        name: lambda_name,
        span,
    })
}

fn lower_invoke_macro_call<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke currently supports the form @invoke f(arg::Type, ...)"),
        );
    }

    match walker.kind(&args[0]) {
        NodeKind::CallExpression => lower_invoke_call_expression(walker, args[0], span, lambda_ctx),
        NodeKind::FieldExpression => {
            lower_invoke_field_expression(walker, args[0], span, lambda_ctx)
        }
        NodeKind::IndexExpression => {
            lower_invoke_index_expression(walker, args[0], span, lambda_ctx)
        }
        NodeKind::BinaryExpression => {
            lower_invoke_binary_expression(walker, args[0], span, lambda_ctx)
        }
        NodeKind::Assignment => {
            lower_invoke_assignment_expression(walker, args[0], span, lambda_ctx)
        }
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke requires a function call, property access, or index access"),
        ),
    }
}

fn invoke_type_expr_for_names(type_names: &[String], span: crate::span::Span) -> Expr {
    let tuple_type = if type_names.is_empty() {
        "Tuple".to_string()
    } else {
        format!("Tuple{{{}}}", type_names.join(", "))
    };
    Expr::Builtin {
        name: BuiltinOp::TypeOf,
        args: vec![Expr::Literal(Literal::Str(tuple_type), span)],
        span,
    }
}

fn invoke_tuple_type_expr(
    type_exprs: Vec<Expr>,
    static_type_names: Vec<Option<String>>,
    span: crate::span::Span,
) -> Expr {
    if static_type_names.iter().all(Option::is_some) {
        let names: Vec<String> = static_type_names.into_iter().flatten().collect();
        invoke_type_expr_for_names(&names, span)
    } else {
        Expr::DynamicTypeConstruct {
            base: "Tuple".to_string(),
            base_expr: None,
            type_args: type_exprs,
            // `@invoke` signature tuples never splat their type arguments.
            splat_mask: Vec::new(),
            span,
        }
    }
}

fn lower_invoke_value_and_type<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<(Expr, Expr, Option<String>)> {
    if walker.kind(&node) == NodeKind::ParenthesizedExpression {
        let children = walker.named_children(&node);
        if children.len() == 1 {
            return lower_invoke_value_and_type(walker, children[0], span, lambda_ctx);
        }
    }

    if walker.kind(&node) == NodeKind::TypedExpression {
        let typed_children = walker.named_children(&node);
        if typed_children.len() != 2 {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::MacroCall,
                walker.span(&node),
            )
            .with_hint("@invoke arguments must be annotated as value::Type"));
        }
        let value = super::lower_expr_with_ctx(walker, typed_children[0], lambda_ctx)?;
        let type_name = walker.text(&typed_children[1]).to_string();
        let type_expr = Expr::Builtin {
            name: BuiltinOp::TypeOf,
            args: vec![Expr::Literal(Literal::Str(type_name.clone()), span)],
            span,
        };
        return Ok((value, type_expr, Some(type_name)));
    }

    let value = super::lower_expr_with_ctx(walker, node, lambda_ctx)?;
    let type_expr = Expr::Builtin {
        name: BuiltinOp::TypeOf,
        args: vec![value.clone()],
        span,
    };
    Ok((value, type_expr, None))
}

fn invoke_call_expr(
    callee_expr: Expr,
    values: Vec<Expr>,
    type_exprs: Vec<Expr>,
    static_type_names: Vec<Option<String>>,
    kwargs: Vec<(String, Expr)>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
) -> Expr {
    let mut invoke_args = Vec::with_capacity(values.len() + 2);
    invoke_args.push(callee_expr);
    invoke_args.push(invoke_tuple_type_expr(type_exprs, static_type_names, span));
    invoke_args.extend(values);

    Expr::Call {
        function: "invoke".to_string(),
        args: invoke_args,
        kwargs,
        splat_mask: Vec::new(),
        kwargs_splat_mask,
        span,
    }
}

fn lower_invoke_call_expression<'a>(
    walker: &CstWalker<'a>,
    call_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let call_children = walker.named_children(&call_node);
    if call_children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke requires a function call expression"),
        );
    }

    let callee = call_children[0];
    let callee_expr = super::lower_expr_with_ctx(walker, callee, lambda_ctx)?;
    let args_node = call_children
        .iter()
        .copied()
        .find(|child| walker.kind(child) == NodeKind::ArgumentList)
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke requires a function call argument list")
        })?;

    let mut declared_type_exprs = Vec::new();
    let mut static_type_names = Vec::new();
    let mut values = Vec::new();
    let mut kwargs = Vec::new();
    let mut kwargs_splat_mask = Vec::new();
    let mut saw_semicolon = false;
    for arg in walker.named_children(&args_node) {
        match walker.kind(&arg) {
            NodeKind::Semicolon => {
                saw_semicolon = true;
                continue;
            }
            NodeKind::Assignment | NodeKind::KeywordArgument => {
                let children: Vec<_> = walker
                    .named_children(&arg)
                    .into_iter()
                    .filter(|node| walker.kind(node) != NodeKind::Operator)
                    .collect();
                if children.len() >= 2 && walker.kind(&children[0]) == NodeKind::Identifier {
                    kwargs.push((
                        walker.text(&children[0]).to_string(),
                        super::lower_expr_with_ctx(walker, children[1], lambda_ctx)?,
                    ));
                    kwargs_splat_mask.push(false);
                }
                continue;
            }
            NodeKind::SplatExpression if saw_semicolon => {
                let inner_children = walker.named_children(&arg);
                if let Some(inner) = inner_children.first() {
                    kwargs.push((
                        "".to_string(),
                        super::lower_expr_with_ctx(walker, *inner, lambda_ctx)?,
                    ));
                    kwargs_splat_mask.push(true);
                }
                continue;
            }
            NodeKind::SplatExpression => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::MacroCall,
                    walker.span(&arg),
                )
                .with_hint("@invoke positional splats are not yet supported"));
            }
            _ => {}
        }

        let (value, type_expr, static_type_name) =
            lower_invoke_value_and_type(walker, arg, span, lambda_ctx)?;
        values.push(value);
        declared_type_exprs.push(type_expr);
        static_type_names.push(static_type_name);
    }

    Ok(invoke_call_expr(
        callee_expr,
        values,
        declared_type_exprs,
        static_type_names,
        kwargs,
        kwargs_splat_mask,
        span,
    ))
}

fn invoke_symbol_literal(name: String, span: crate::span::Span) -> Expr {
    Expr::Literal(Literal::Symbol(name), span)
}

fn lower_invoke_field_expression<'a>(
    walker: &CstWalker<'a>,
    field_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&field_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke property access requires object.field"),
        );
    }

    let (object, object_type, object_static_type) =
        lower_invoke_value_and_type(walker, children[0], span, lambda_ctx)?;
    let field_name = walker.text(&children[1]).to_string();
    let field = invoke_symbol_literal(field_name, span);
    let field_type = Expr::Builtin {
        name: BuiltinOp::TypeOf,
        args: vec![field.clone()],
        span,
    };

    Ok(invoke_call_expr(
        Expr::FunctionRef {
            name: "getproperty".to_string(),
            span,
        },
        vec![object, field],
        vec![object_type, field_type],
        vec![object_static_type, None],
        Vec::new(),
        Vec::new(),
        span,
    ))
}

fn lower_invoke_index_expression<'a>(
    walker: &CstWalker<'a>,
    index_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&index_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke index access requires xs[i]"),
        );
    }

    let mut values = Vec::new();
    let mut type_exprs = Vec::new();
    let mut static_type_names = Vec::new();
    for child in children {
        let (value, type_expr, static_type_name) =
            lower_invoke_value_and_type(walker, child, span, lambda_ctx)?;
        values.push(value);
        type_exprs.push(type_expr);
        static_type_names.push(static_type_name);
    }

    Ok(invoke_call_expr(
        Expr::FunctionRef {
            name: "getindex".to_string(),
            span,
        },
        values,
        type_exprs,
        static_type_names,
        Vec::new(),
        Vec::new(),
        span,
    ))
}

fn lower_invoke_binary_expression<'a>(
    walker: &CstWalker<'a>,
    binary_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let raw_op_name = walker
        .named_children(&binary_node)
        .into_iter()
        .find(|node| walker.kind(node) == NodeKind::Operator)
        .map(|node| walker.text(&node).to_string())
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke binary operator form requires an operator")
        })?;
    let op_name = if raw_op_name == "%" {
        // Julia defines `%` as a constant alias for `rem` in base/operators.jl.
        // sjulia's callable registry currently stores the underlying `rem` method name.
        "rem".to_string()
    } else {
        raw_op_name
    };

    let operands: Vec<_> = walker
        .named_children(&binary_node)
        .into_iter()
        .filter(|node| walker.kind(node) != NodeKind::Operator)
        .collect();
    if operands.len() != 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke binary operator form requires lhs op rhs"),
        );
    }

    let mut values = Vec::new();
    let mut type_exprs = Vec::new();
    let mut static_type_names = Vec::new();
    for operand in operands {
        let (value, type_expr, static_type_name) =
            lower_invoke_value_and_type(walker, operand, span, lambda_ctx)?;
        values.push(value);
        type_exprs.push(type_expr);
        static_type_names.push(static_type_name);
    }

    Ok(invoke_call_expr(
        Expr::FunctionRef {
            name: op_name,
            span,
        },
        values,
        type_exprs,
        static_type_names,
        Vec::new(),
        Vec::new(),
        span,
    ))
}

fn lower_invoke_assignment_expression<'a>(
    walker: &CstWalker<'a>,
    assign_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children: Vec<_> = walker
        .named_children(&assign_node)
        .into_iter()
        .filter(|node| walker.kind(node) != NodeKind::Operator)
        .collect();
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke assignment requires lhs = rhs"),
        );
    }
    let lhs = children[0];
    let rhs = children[1];

    match walker.kind(&lhs) {
        NodeKind::FieldExpression => {
            lower_invoke_field_assignment(walker, lhs, rhs, span, lambda_ctx)
        }
        NodeKind::IndexExpression => {
            lower_invoke_index_assignment(walker, lhs, rhs, span, lambda_ctx)
        }
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke assignment currently supports property and index assignment"),
        ),
    }
}

fn lower_invoke_field_assignment<'a>(
    walker: &CstWalker<'a>,
    field_node: Node<'a>,
    rhs: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&field_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke property assignment requires object.field = value"),
        );
    }

    let (object, object_type, object_static_type) =
        lower_invoke_value_and_type(walker, children[0], span, lambda_ctx)?;
    let field_name = walker.text(&children[1]).to_string();
    let field = invoke_symbol_literal(field_name, span);
    let field_type = Expr::Builtin {
        name: BuiltinOp::TypeOf,
        args: vec![field.clone()],
        span,
    };
    let (value, value_type, value_static_type) =
        lower_invoke_value_and_type(walker, rhs, span, lambda_ctx)?;

    Ok(invoke_call_expr(
        Expr::FunctionRef {
            name: "setproperty!".to_string(),
            span,
        },
        vec![object, field, value],
        vec![object_type, field_type, value_type],
        vec![object_static_type, None, value_static_type],
        Vec::new(),
        Vec::new(),
        span,
    ))
}

fn lower_invoke_index_assignment<'a>(
    walker: &CstWalker<'a>,
    index_node: Node<'a>,
    rhs: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&index_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invoke index assignment requires xs[i] = value"),
        );
    }

    let mut values = Vec::new();
    let mut type_exprs = Vec::new();
    let mut static_type_names = Vec::new();

    let (collection, collection_type, collection_static_type) =
        lower_invoke_value_and_type(walker, children[0], span, lambda_ctx)?;
    let (value, value_type, value_static_type) =
        lower_invoke_value_and_type(walker, rhs, span, lambda_ctx)?;
    values.push(collection);
    values.push(value);
    type_exprs.push(collection_type);
    type_exprs.push(value_type);
    static_type_names.push(collection_static_type);
    static_type_names.push(value_static_type);

    for child in children.into_iter().skip(1) {
        let (index, index_type, index_static_type) =
            lower_invoke_value_and_type(walker, child, span, lambda_ctx)?;
        values.push(index);
        type_exprs.push(index_type);
        static_type_names.push(index_static_type);
    }

    Ok(invoke_call_expr(
        Expr::FunctionRef {
            name: "setindex!".to_string(),
            span,
        },
        values,
        type_exprs,
        static_type_names,
        Vec::new(),
        Vec::new(),
        span,
    ))
}

fn lower_invokelatest_macro_call<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest currently supports the form @invokelatest f(args...)"),
        );
    }

    match walker.kind(&args[0]) {
        NodeKind::CallExpression => {
            lower_invokelatest_call_expression(walker, args[0], span, lambda_ctx)
        }
        NodeKind::FieldExpression => {
            lower_invokelatest_field_expression(walker, args[0], span, lambda_ctx)
        }
        NodeKind::IndexExpression => {
            lower_invokelatest_index_expression(walker, args[0], span, lambda_ctx)
        }
        NodeKind::Assignment => {
            lower_invokelatest_assignment_expression(walker, args[0], span, lambda_ctx)
        }
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@invokelatest requires a function call, property access, or index access",
            ),
        ),
    }
}

fn invokelatest_call_expr(callee_expr: Expr, mut args: Vec<Expr>, span: crate::span::Span) -> Expr {
    args.insert(0, callee_expr);
    let splat_mask = vec![false; args.len()];

    Expr::Call {
        function: "invokelatest".to_string(),
        args,
        kwargs: vec![],
        splat_mask,
        kwargs_splat_mask: vec![],
        span,
    }
}

fn lower_invokelatest_call_expression<'a>(
    walker: &CstWalker<'a>,
    call_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let call_children = walker.named_children(&call_node);
    if call_children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest requires a function call expression"),
        );
    }

    let callee = call_children[0];
    let callee_expr = super::lower_expr_with_ctx(walker, callee, lambda_ctx)?;
    let args_node = call_children
        .iter()
        .copied()
        .find(|child| walker.kind(child) == NodeKind::ArgumentList)
        .ok_or_else(|| {
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest requires a function call argument list")
        })?;
    let (mut lowered_args, kwargs, mut splat_mask, kwargs_splat_mask) =
        super::call::lower_argument_list_with_ctx(walker, args_node, lambda_ctx)?;

    lowered_args.insert(0, callee_expr);
    splat_mask.insert(0, false);

    Ok(Expr::Call {
        function: "invokelatest".to_string(),
        args: lowered_args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    })
}

fn lower_invokelatest_field_expression<'a>(
    walker: &CstWalker<'a>,
    field_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&field_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest property access requires object.field"),
        );
    }

    let object = super::lower_expr_with_ctx(walker, children[0], lambda_ctx)?;
    let field_name = walker.text(&children[1]).to_string();
    Ok(invokelatest_call_expr(
        Expr::FunctionRef {
            name: "getproperty".to_string(),
            span,
        },
        vec![object, invoke_symbol_literal(field_name, span)],
        span,
    ))
}

fn lower_invokelatest_index_expression<'a>(
    walker: &CstWalker<'a>,
    index_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&index_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest index access requires xs[i]"),
        );
    }

    let values = children
        .into_iter()
        .map(|child| super::lower_expr_with_ctx(walker, child, lambda_ctx))
        .collect::<LowerResult<Vec<_>>>()?;

    Ok(invokelatest_call_expr(
        Expr::FunctionRef {
            name: "getindex".to_string(),
            span,
        },
        values,
        span,
    ))
}

fn lower_invokelatest_assignment_expression<'a>(
    walker: &CstWalker<'a>,
    assign_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children: Vec<_> = walker
        .named_children(&assign_node)
        .into_iter()
        .filter(|node| walker.kind(node) != NodeKind::Operator)
        .collect();
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest assignment requires lhs = rhs"),
        );
    }
    let lhs = children[0];
    let rhs = children[1];

    match walker.kind(&lhs) {
        NodeKind::FieldExpression => {
            lower_invokelatest_field_assignment(walker, lhs, rhs, span, lambda_ctx)
        }
        NodeKind::IndexExpression => {
            lower_invokelatest_index_assignment(walker, lhs, rhs, span, lambda_ctx)
        }
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@invokelatest assignment currently supports property and index assignment",
            ),
        ),
    }
}

fn lower_invokelatest_field_assignment<'a>(
    walker: &CstWalker<'a>,
    field_node: Node<'a>,
    rhs: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&field_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest property assignment requires object.field = value"),
        );
    }

    let object = super::lower_expr_with_ctx(walker, children[0], lambda_ctx)?;
    let field_name = walker.text(&children[1]).to_string();
    let value = super::lower_expr_with_ctx(walker, rhs, lambda_ctx)?;
    Ok(invokelatest_call_expr(
        Expr::FunctionRef {
            name: "setproperty!".to_string(),
            span,
        },
        vec![object, invoke_symbol_literal(field_name, span), value],
        span,
    ))
}

fn lower_invokelatest_index_assignment<'a>(
    walker: &CstWalker<'a>,
    index_node: Node<'a>,
    rhs: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let children = walker.named_children(&index_node);
    if children.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@invokelatest index assignment requires xs[i] = value"),
        );
    }

    let mut values = Vec::new();
    values.push(super::lower_expr_with_ctx(walker, children[0], lambda_ctx)?);
    values.push(super::lower_expr_with_ctx(walker, rhs, lambda_ctx)?);
    for child in children.into_iter().skip(1) {
        values.push(super::lower_expr_with_ctx(walker, child, lambda_ctx)?);
    }

    Ok(invokelatest_call_expr(
        Expr::FunctionRef {
            name: "setindex!".to_string(),
            span,
        },
        values,
        span,
    ))
}

fn lower_world_macro_call<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@world requires a global binding and a world expression"),
        );
    }

    let function_name = world_macro_binding_name(walker, args[0], span)?;
    let world_value = super::lower_expr_with_ctx(walker, args[1], lambda_ctx)?;
    let world_temp = format!("__sjulia_world_arg_{}", span.start);

    Ok(Expr::LetBlock {
        bindings: vec![(world_temp, world_value)],
        body: Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::FunctionRef {
                    name: function_name,
                    span,
                },
                span,
            }],
            span,
        },
        span,
    })
}

fn world_macro_binding_name<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
) -> LowerResult<String> {
    match walker.kind(&node) {
        NodeKind::Identifier | NodeKind::Operator => Ok(walker.text(&node).to_string()),
        NodeKind::FieldExpression => {
            let children = walker.named_children(&node);
            if children.len() < 2 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@world dotted bindings require module.name"),
                );
            }

            let mut parts = Vec::with_capacity(children.len());
            for child in children {
                parts.push(world_macro_binding_name(walker, child, span)?);
            }
            Ok(parts.join("."))
        }
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@world currently supports identifier and dotted global bindings"),
        ),
    }
}

fn lower_inbounds_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@inbounds requires exactly one expression"),
        );
    }
    if let Some(expr) = lower_inbounds_tuple_assignment_expr(walker, args[0], span, lambda_ctx)? {
        return Ok(expr);
    }
    let inner = super::lower_expr_with_ctx(walker, args[0], lambda_ctx)?;
    Ok(Expr::Call {
        function: "#__sjulia_inbounds__".to_string(),
        args: vec![inner],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    })
}

fn lower_inbounds_tuple_assignment_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Option<Expr>> {
    if walker.kind(&node) != NodeKind::TupleExpression {
        return Ok(None);
    }
    let elements = walker.named_children(&node);
    if elements.len() != 3 || walker.kind(&elements[1]) != NodeKind::Assignment {
        return Ok(None);
    }
    let Some((second_lhs, first_rhs)) = assignment_lhs_rhs(walker, elements[1]) else {
        return Ok(None);
    };
    if walker.kind(&elements[0]) != NodeKind::Identifier
        || walker.kind(&second_lhs) != NodeKind::Identifier
    {
        return Ok(None);
    }

    let first_var = walker.text(&elements[0]).to_string();
    let second_var = walker.text(&second_lhs).to_string();
    let first_temp = format!("__inbounds_tuple_expr_tmp_{}_0", span.start);
    let second_temp = format!("__inbounds_tuple_expr_tmp_{}_1", span.start);
    let first_rhs = super::lower_expr_with_ctx(walker, first_rhs, lambda_ctx)?;
    let second_rhs = super::lower_expr_with_ctx(walker, elements[2], lambda_ctx)?;

    let inbounds = |expr| Expr::Call {
        function: "#__sjulia_inbounds__".to_string(),
        args: vec![expr],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    Ok(Some(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::Assign {
                    var: first_temp.clone(),
                    value: inbounds(first_rhs),
                    span,
                },
                Stmt::Assign {
                    var: second_temp.clone(),
                    value: inbounds(second_rhs),
                    span,
                },
                Stmt::Assign {
                    var: first_var,
                    value: Expr::Var(first_temp.clone(), span),
                    span,
                },
                Stmt::Assign {
                    var: second_var,
                    value: Expr::Var(second_temp.clone(), span),
                    span,
                },
                Stmt::Expr {
                    expr: Expr::TupleLiteral {
                        elements: vec![Expr::Var(first_temp, span), Expr::Var(second_temp, span)],
                        span,
                    },
                    span,
                },
            ],
            span,
        },
        span,
    }))
}

fn assignment_lhs_rhs<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    let all_children = walker.children(&node);
    for (i, child) in all_children.iter().enumerate() {
        if child.kind() == "operator"
            && walker.text(child) == "="
            && i > 0
            && i + 1 < all_children.len()
        {
            return Some((all_children[i - 1], all_children[i + 1]));
        }
    }

    let named = walker.named_children(&node);
    if named.len() >= 2 {
        Some((named[0], named[named.len() - 1]))
    } else {
        None
    }
}

fn lower_inline_policy_macro_expr<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint(format!("@{} requires exactly one expression", macro_name)),
        );
    }
    let inner = super::lower_expr_with_ctx(walker, args[0], lambda_ctx)?;
    Ok(Expr::Call {
        function: format!("#__sjulia_{}__", macro_name),
        args: vec![inner],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    })
}

fn lower_task_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let body_node = require_single_task_macro_arg("task", args, span)?;
    let thunk = lift_zero_arg_task_thunk(walker, body_node, span, lambda_ctx)?;
    Ok(Expr::Call {
        function: "Task".to_string(),
        args: vec![thunk],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    })
}

fn lower_async_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let body_node = require_single_task_macro_arg("async", args, span)?;
    let thunk = lift_zero_arg_task_thunk(walker, body_node, span, lambda_ctx)?;
    let task_var = format!("__async_task_{}_{}", span.start, span.end);

    let task_expr = Expr::Call {
        function: "Task".to_string(),
        args: vec![thunk],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![(task_var.clone(), task_expr)],
        body: Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "schedule".to_string(),
                        args: vec![Expr::Var(task_var.clone(), span)],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span,
                    },
                    span,
                },
                Stmt::Expr {
                    expr: Expr::Var(task_var, span),
                    span,
                },
            ],
            span,
        },
        span,
    })
}

fn macro_name_for_node<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier)?;
    Some(
        walker
            .text(&macro_ident)
            .trim_start_matches('@')
            .to_string(),
    )
}

fn macro_args_for_node<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Vec<Node<'a>> {
    walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
        .collect()
}

fn sync_async_assignment_var<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    if walker.kind(&node) != NodeKind::Assignment {
        return None;
    }

    let named = walker.named_children(&node);
    if named.len() < 2 {
        return None;
    }

    let lhs = named[0];
    let rhs = named[named.len() - 1];
    if walker.kind(&lhs) != NodeKind::Identifier
        || walker.kind(&rhs) != NodeKind::MacroCall
        || macro_name_for_node(walker, rhs).as_deref() != Some("async")
    {
        return None;
    }

    Some(walker.text(&lhs).to_string())
}

fn var(name: &str, span: crate::span::Span) -> Expr {
    Expr::Var(name.to_string(), span)
}

fn call(function: &str, args: Vec<Expr>, span: crate::span::Span) -> Expr {
    Expr::Call {
        function: function.to_string(),
        args,
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    }
}

/// Generate: `push!(exceptions_var, catch_var)` statement
fn push_to_exceptions(exceptions_var: &str, catch_var: &str, span: crate::span::Span) -> Stmt {
    Stmt::Expr {
        expr: call(
            "push!",
            vec![var(exceptions_var, span), var(catch_var, span)],
            span,
        ),
        span,
    }
}

/// Wrap `body_expr` in try/catch that pushes any exception into `exceptions_var`.
fn wrap_sync_async_body(
    body_expr: Expr,
    exceptions_var: &str,
    catch_var: &str,
    span: crate::span::Span,
) -> Stmt {
    Stmt::Try {
        try_block: Block {
            stmts: vec![Stmt::Expr {
                expr: body_expr,
                span,
            }],
            span,
        },
        catch_var: Some(catch_var.to_string()),
        catch_block: Some(Block {
            stmts: vec![push_to_exceptions(exceptions_var, catch_var, span)],
            span,
        }),
        else_block: None,
        finally_block: None,
        span,
    }
}

/// Generate: `if !isempty(exceptions_var); throw(CompositeException(exceptions_var)); end`
fn sync_throw_if_failed(exceptions_var: &str, span: crate::span::Span) -> Stmt {
    use crate::ir::core::UnaryOp;
    Stmt::If {
        condition: Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(call("isempty", vec![var(exceptions_var, span)], span)),
            span,
        },
        then_branch: Block {
            stmts: vec![Stmt::Expr {
                expr: call(
                    "throw",
                    vec![call(
                        "CompositeException",
                        vec![var(exceptions_var, span)],
                        span,
                    )],
                    span,
                ),
                span,
            }],
            span,
        },
        else_branch: None,
        span,
    }
}

/// Generate try/catch that waits on `task_var` and collects failures into `exceptions_var`.
fn sync_wait_assigned_async(
    task_var: &str,
    exceptions_var: &str,
    catch_var: &str,
    span: crate::span::Span,
) -> Stmt {
    wrap_sync_async_body(
        call("wait", vec![var(task_var, span)], span),
        exceptions_var,
        catch_var,
        span,
    )
}

/// `result_var = value` — plain assignment used to capture the `@sync` body's
/// last-expression value before the throw-if-failed guard (Issue #7813).
fn sync_assign_result(result_var: &str, value: Expr, span: crate::span::Span) -> Stmt {
    Stmt::Assign {
        var: result_var.to_string(),
        value,
        span,
    }
}

/// `result_var` — final statement that makes the surrounding `LetBlock` yield the
/// captured body value instead of the value-less throw-if-failed guard (#7813).
fn sync_yield_result(result_var: &str, span: crate::span::Span) -> Stmt {
    Stmt::Expr {
        expr: var(result_var, span),
        span,
    }
}

/// Lower an `@async` node to a real scheduled `Task` expression (the value that
/// upstream `@async` produces), mirroring `lower_async_macro_expr`. Used so that
/// expression-position `@sync` can yield the body's last-expression Task value
/// (Issue #7813) while still aggregating failures via `wait`.
fn lower_sync_async_task<'a>(
    walker: &CstWalker<'a>,
    async_node: Node<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let async_args = macro_args_for_node(walker, async_node);
    let async_span = walker.span(&async_node);
    lower_async_macro_expr(walker, &async_args, async_span, lambda_ctx)
}

fn lower_sync_single_async_expr<'a>(
    walker: &CstWalker<'a>,
    async_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let exceptions_var = format!("__sync_exceptions_{}_{}", span.start, span.end);
    let result_var = format!("__sync_result_{}_{}", span.start, span.end);
    let async_span = walker.span(&async_node);
    // Produce the actual `Task` value (upstream `@sync @async e` returns the Task)
    // and bind it to a result temp so the LetBlock yields it (Issue #7813).
    let task_expr = lower_sync_async_task(walker, async_node, lambda_ctx)?;
    let task_var = format!("__sync_task_{}_{}", async_span.start, async_span.end);
    let catch_var = format!("__sync_async_error_{}_{}", async_span.start, async_span.end);

    // Any[] — typed empty array for exception collection
    let empty_any_vec = Expr::TypedEmptyArray {
        element_type: "Any".to_string(),
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![(exceptions_var.clone(), empty_any_vec)],
        body: Block {
            stmts: vec![
                sync_assign_result(&task_var, task_expr, async_span),
                sync_wait_assigned_async(&task_var, &exceptions_var, &catch_var, async_span),
                sync_assign_result(&result_var, var(&task_var, async_span), async_span),
                sync_throw_if_failed(&exceptions_var, span),
                sync_yield_result(&result_var, span),
            ],
            span,
        },
        span,
    })
}

fn lower_sync_block_expr<'a>(
    walker: &CstWalker<'a>,
    body_node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let exceptions_var = format!("__sync_exceptions_{}_{}", span.start, span.end);
    let result_var = format!("__sync_result_{}_{}", span.start, span.end);

    let actual_block = walker
        .named_children(&body_node)
        .into_iter()
        .find(|child| walker.kind(child) == NodeKind::Block)
        .unwrap_or(body_node);

    // Any[] — typed empty array for exception collection
    let empty_any_vec = Expr::TypedEmptyArray {
        element_type: "Any".to_string(),
        span,
    };

    let children = walker.named_children(&actual_block);
    let last_idx = children.len().checked_sub(1);
    // Upstream `@sync begin ... end` yields the value of the block's last
    // expression. Bind that value to a span-unique result temp before the
    // throw-if-failed guard so the LetBlock yields it instead of `nothing`
    // (Issue #7813).
    let mut have_result = false;

    let mut stmts = vec![];
    for (idx, child) in children.into_iter().enumerate() {
        let is_last = Some(idx) == last_idx;
        if walker.kind(&child) == NodeKind::MacroCall
            && macro_name_for_node(walker, child).as_deref() == Some("async")
        {
            let async_span = walker.span(&child);
            let catch_var = format!("__sync_async_error_{}_{}", async_span.start, async_span.end);
            if is_last {
                // Last expression is a standalone `@async` — its value is the
                // scheduled `Task`. Build the real Task, wait on it to collect
                // failures, then capture the Task as the block result (#7813).
                let task_expr = lower_sync_async_task(walker, child, lambda_ctx)?;
                let task_var = format!("__sync_task_{}_{}", async_span.start, async_span.end);
                stmts.push(sync_assign_result(&task_var, task_expr, async_span));
                stmts.push(sync_wait_assigned_async(
                    &task_var,
                    &exceptions_var,
                    &catch_var,
                    async_span,
                ));
                stmts.push(sync_assign_result(
                    &result_var,
                    var(&task_var, async_span),
                    async_span,
                ));
                have_result = true;
            } else {
                let async_args = macro_args_for_node(walker, child);
                let async_body = require_single_task_macro_arg("async", &async_args, async_span)?;
                let expr = super::lower_expr_with_ctx(walker, async_body, lambda_ctx)?;
                stmts.push(wrap_sync_async_body(
                    expr,
                    &exceptions_var,
                    &catch_var,
                    async_span,
                ));
            }
        } else if let Some(task_var) = sync_async_assignment_var(walker, child) {
            let assignment_span = walker.span(&child);
            stmts.push(crate::lowering::stmt::lower_stmt_with_ctx(
                walker, child, lambda_ctx,
            )?);
            let catch_var = format!(
                "__sync_async_error_{}_{}",
                assignment_span.start, assignment_span.end
            );
            stmts.push(sync_wait_assigned_async(
                &task_var,
                &exceptions_var,
                &catch_var,
                assignment_span,
            ));
            if is_last {
                // Value of `lhs = @async ...` as a trailing expression is the
                // assigned Task (#7813).
                stmts.push(sync_assign_result(
                    &result_var,
                    var(&task_var, assignment_span),
                    assignment_span,
                ));
                have_result = true;
            }
        } else if is_last && walker.kind(&child) != NodeKind::Assignment {
            // Plain trailing expression: capture its value directly (#7813).
            let child_span = walker.span(&child);
            let expr = super::lower_expr_with_ctx(walker, child, lambda_ctx)?;
            stmts.push(sync_assign_result(&result_var, expr, child_span));
            have_result = true;
        } else {
            stmts.push(crate::lowering::stmt::lower_stmt_with_ctx(
                walker, child, lambda_ctx,
            )?);
            if is_last {
                // Trailing assignment / control statement: its value is the
                // assigned variable when it is a simple `lhs = rhs` (#7813).
                if let Some(lhs) = simple_assignment_lhs(walker, child) {
                    let child_span = walker.span(&child);
                    stmts.push(sync_assign_result(
                        &result_var,
                        var(&lhs, child_span),
                        child_span,
                    ));
                    have_result = true;
                }
            }
        }
    }

    stmts.push(sync_throw_if_failed(&exceptions_var, span));
    if have_result {
        stmts.push(sync_yield_result(&result_var, span));
    }

    Ok(Expr::LetBlock {
        bindings: vec![(exceptions_var.clone(), empty_any_vec)],
        body: Block { stmts, span },
        span,
    })
}

/// Return the LHS identifier of a simple `lhs = rhs` assignment node, or `None`
/// for compound/destructuring/non-assignment forms. Used to capture the value of
/// a trailing assignment as the `@sync` block result (Issue #7813).
fn simple_assignment_lhs<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    if walker.kind(&node) != NodeKind::Assignment {
        return None;
    }
    let named = walker.named_children(&node);
    if named.len() < 2 {
        return None;
    }
    let lhs = named[0];
    if walker.kind(&lhs) != NodeKind::Identifier {
        return None;
    }
    Some(walker.text(&lhs).to_string())
}

fn lower_sync_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let body_node = require_single_task_macro_arg("sync", args, span)?;
    if walker.kind(&body_node) == NodeKind::Block {
        return lower_sync_block_expr(walker, body_node, span, lambda_ctx);
    }
    if walker.kind(&body_node) == NodeKind::MacroCall
        && macro_name_for_node(walker, body_node).as_deref() == Some("async")
    {
        return lower_sync_single_async_expr(walker, body_node, span, lambda_ctx);
    }
    super::lower_expr_with_ctx(walker, body_node, lambda_ctx)
}

/// Build `exceptions_var = Any[]` as a plain assignment statement.
///
/// Unlike the expression-position `@sync` (which binds the accumulator inside an
/// isolated `Expr::LetBlock`), statement-position `@sync` must keep its body in
/// the enclosing scope so that assignments such as `t = @async ...` update the
/// surrounding local. The accumulator name is span-unique, so this assignment
/// cannot clash with a user local (Issue #7768).
fn sync_init_exceptions_stmt(exceptions_var: &str, span: crate::span::Span) -> Stmt {
    Stmt::Assign {
        var: exceptions_var.to_string(),
        value: Expr::TypedEmptyArray {
            element_type: "Any".to_string(),
            span,
        },
        span,
    }
}

/// Statement-position `@sync begin ... end` / `@sync @async ...` / `@sync expr`.
///
/// Produces a `Stmt::Block` whose statements run inline in the enclosing scope
/// (no isolated `let` scope), so assignments to surrounding locals inside the
/// sync body are preserved, while still aggregating `@async` exceptions into a
/// `CompositeException` and waiting on scheduled tasks. The expression-position
/// path (`lower_sync_macro_expr`) is intentionally left unchanged (Issue #7768).
fn lower_sync_macro_stmt<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Stmt> {
    let body_node = require_single_task_macro_arg("sync", args, span)?;
    let exceptions_var = format!("__sync_exceptions_{}_{}", span.start, span.end);

    // `@sync begin ... end` — execute each body statement in the enclosing scope.
    if walker.kind(&body_node) == NodeKind::Block {
        let actual_block = walker
            .named_children(&body_node)
            .into_iter()
            .find(|child| walker.kind(child) == NodeKind::Block)
            .unwrap_or(body_node);

        let mut stmts = vec![sync_init_exceptions_stmt(&exceptions_var, span)];
        stmts.extend(lower_sync_body_stmts(
            walker,
            &actual_block,
            &exceptions_var,
            lambda_ctx,
        )?);
        stmts.push(sync_throw_if_failed(&exceptions_var, span));
        return Ok(Stmt::Block(Block { stmts, span }));
    }

    // `@sync @async expr` — a single async expression.
    if walker.kind(&body_node) == NodeKind::MacroCall
        && macro_name_for_node(walker, body_node).as_deref() == Some("async")
    {
        let async_span = walker.span(&body_node);
        let async_args = macro_args_for_node(walker, body_node);
        let async_body = require_single_task_macro_arg("async", &async_args, async_span)?;
        let expr = super::lower_expr_with_ctx(walker, async_body, lambda_ctx)?;
        let catch_var = format!("__sync_async_error_{}_{}", async_span.start, async_span.end);
        let stmts = vec![
            sync_init_exceptions_stmt(&exceptions_var, span),
            wrap_sync_async_body(expr, &exceptions_var, &catch_var, async_span),
            sync_throw_if_failed(&exceptions_var, span),
        ];
        return Ok(Stmt::Block(Block { stmts, span }));
    }

    // `@sync for ... @async ... end` — recurse into the loop body so each spawned
    // `@async` is collected into the shared exceptions accumulator and awaited
    // (in the no-JIT VM, `@async` inside `@sync` runs its body inline). Without
    // this the for-loop would lower as a plain statement with no await, silently
    // dropping every `@async` result (Issue #7831).
    if walker.kind(&body_node) == NodeKind::ForStatement {
        if let Some(for_body) = walker
            .named_children(&body_node)
            .into_iter()
            .find(|child| walker.kind(child) == NodeKind::Block)
        {
            let loop_body_stmts =
                lower_sync_body_stmts(walker, &for_body, &exceptions_var, lambda_ctx)?;
            let for_span = walker.span(&for_body);
            let loop_stmt = crate::lowering::stmt::lower_for_stmt_with_body(
                walker,
                body_node,
                Some(lambda_ctx),
                Block {
                    stmts: loop_body_stmts,
                    span: for_span,
                },
            )?;
            let stmts = vec![
                sync_init_exceptions_stmt(&exceptions_var, span),
                loop_stmt,
                sync_throw_if_failed(&exceptions_var, span),
            ];
            return Ok(Stmt::Block(Block { stmts, span }));
        }
    }

    // `@sync expr` — any other single expression executes as a plain statement.
    crate::lowering::stmt::lower_stmt_with_ctx(walker, body_node, lambda_ctx)
}

/// Lower the statements of a `@sync` body block, rewriting each direct `@async`
/// child (and `t = @async ...` assignment) into an inline try/catch that
/// accumulates exceptions into `exceptions_var`. Shared by the `@sync begin
/// ... end` block path and the `@sync for ... @async ... end` loop-body path so
/// both collect and await spawned tasks identically (Issue #7831).
fn lower_sync_body_stmts<'a>(
    walker: &CstWalker<'a>,
    block_node: &Node<'a>,
    exceptions_var: &str,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Vec<Stmt>> {
    let mut stmts = Vec::new();
    for child in walker.named_children(block_node) {
        if walker.kind(&child) == NodeKind::MacroCall
            && macro_name_for_node(walker, child).as_deref() == Some("async")
        {
            let async_args = macro_args_for_node(walker, child);
            let async_span = walker.span(&child);
            let async_body = require_single_task_macro_arg("async", &async_args, async_span)?;
            let expr = super::lower_expr_with_ctx(walker, async_body, lambda_ctx)?;
            let catch_var = format!("__sync_async_error_{}_{}", async_span.start, async_span.end);
            stmts.push(wrap_sync_async_body(
                expr,
                exceptions_var,
                &catch_var,
                async_span,
            ));
        } else if let Some(task_var) = sync_async_assignment_var(walker, child) {
            let assignment_span = walker.span(&child);
            // Lower the `t = @async ...` assignment as a statement so it
            // writes to the surrounding local rather than a let-local.
            stmts.push(crate::lowering::stmt::lower_stmt_with_ctx(
                walker, child, lambda_ctx,
            )?);
            let catch_var = format!(
                "__sync_async_error_{}_{}",
                assignment_span.start, assignment_span.end
            );
            stmts.push(sync_wait_assigned_async(
                &task_var,
                exceptions_var,
                &catch_var,
                assignment_span,
            ));
        } else {
            stmts.push(crate::lowering::stmt::lower_stmt_with_ctx(
                walker, child, lambda_ctx,
            )?);
        }
    }
    Ok(stmts)
}

/// Public entry for statement-position `@sync` lowering (Issue #7768).
pub(crate) fn lower_sync_macro_stmt_entry<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Stmt> {
    let args = macro_args_for_node(walker, node);
    lower_sync_macro_stmt(walker, &args, span, lambda_ctx)
}

/// Lower a macro call in expression context.
/// This handles the Pure Rust parser format where arguments are direct children.
pub(crate) fn lower_macro_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);

    // Find the macro identifier
    let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier);
    let macro_name = match macro_ident {
        Some(ident) => {
            let text = walker.text(&ident);
            text.trim_start_matches('@').to_string()
        }
        None => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::MacroCall,
                span,
            ))
        }
    };

    // Get arguments (all children except MacroIdentifier)
    let args: Vec<Node<'a>> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
        .collect();

    // Handle special compiler macros
    match macro_name.as_str() {
        // @isdefined(x) - check if variable x is defined
        "isdefined" => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@isdefined requires exactly one argument (a variable name)"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            let var_name = match arg_kind {
                NodeKind::Identifier => walker.text(&arg).to_string(),
                _ => {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint("@isdefined argument must be a variable name (identifier)"),
                    );
                }
            };

            return Ok(Expr::Builtin {
                name: crate::ir::core::BuiltinOp::IsDefined,
                args: vec![Expr::Literal(Literal::Str(var_name), span)],
                span,
            });
        }
        // @invoke f(x::T, ...) -> invoke(f, Tuple{T, ...}, x, ...)
        "invoke" => {
            let temp_ctx = crate::lowering::LambdaContext::new();
            return lower_invoke_macro_call(walker, &args, span, &temp_ctx);
        }
        // @invokelatest f(args...; kwargs...) -> invokelatest(f, args...; kwargs...)
        "invokelatest" => {
            let temp_ctx = crate::lowering::LambdaContext::new();
            return lower_invokelatest_macro_call(walker, &args, span, &temp_ctx);
        }
        // @world f world -> resolve f in the current single-world runtime.
        "world" => {
            let temp_ctx = crate::lowering::LambdaContext::new();
            return lower_world_macro_call(walker, &args, span, &temp_ctx);
        }
        "opaque" => return Err(unsupported_opaque_closure_macro(span)),
        "inline" | "noinline" => {
            let temp_ctx = crate::lowering::LambdaContext::new();
            return lower_inline_policy_macro_expr(walker, &macro_name, &args, span, &temp_ctx);
        }
        "inbounds" => {
            let temp_ctx = crate::lowering::LambdaContext::new();
            return lower_inbounds_macro_expr(walker, &args, span, &temp_ctx);
        }
        "assume_effects" => {
            let temp_ctx = crate::lowering::LambdaContext::new();
            return lower_assume_effects_expr_compat(walker, &args, span, &temp_ctx);
        }
        "eval" => {
            if let Some(expr) = lower_eval_module_macro_expr(walker, &args, span)? {
                return Ok(expr);
            }
        }
        // @__dot__ / @. - broadcast all operations (Issue #2547)
        "__dot__" | "." => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@. / @__dot__ requires exactly one argument"),
                );
            }
            let inner = super::lower_expr(walker, args[0])?;
            return Ok(dotify_expr(inner, span));
        }
        // @NamedTuple{a::Int, b} / @NamedTuple begin ... end -> NamedTuple type (Issue #5120)
        "NamedTuple" => {
            return lower_namedtuple_macro_expr(walker, &args, span);
        }
        _ => {}
    }

    // Check for Base macro (from base/macros.jl)
    if crate::base_loader::has_base_macro(&macro_name) {
        let temp_ctx = crate::lowering::LambdaContext::new();
        return expand_base_macro_expr(walker, &macro_name, &args, span, &temp_ctx);
    }

    Err(UnsupportedFeature::new(
        UnsupportedFeatureKind::MacroCall,
        span,
    ))
}

/// Lower a macro call in expression context with lambda context.
pub(crate) fn lower_macro_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);

    // Find the macro identifier
    let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier);
    let macro_name = match macro_ident {
        Some(ident) => {
            let text = walker.text(&ident);
            text.trim_start_matches('@').to_string()
        }
        None => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::MacroCall,
                span,
            ))
        }
    };

    // Get arguments (all children except MacroIdentifier)
    let args: Vec<Node<'a>> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
        .collect();

    match macro_name.as_str() {
        "macroexpand" => lower_macroexpand_expr(walker, &args, span, lambda_ctx),
        // @invoke f(x::T, ...) -> invoke(f, Tuple{T, ...}, x, ...)
        "invoke" => lower_invoke_macro_call(walker, &args, span, lambda_ctx),
        // @invokelatest f(args...; kwargs...) -> invokelatest(f, args...; kwargs...)
        "invokelatest" => lower_invokelatest_macro_call(walker, &args, span, lambda_ctx),
        // @world f world -> resolve f in the current single-world runtime.
        "world" => lower_world_macro_call(walker, &args, span, lambda_ctx),
        "opaque" => Err(unsupported_opaque_closure_macro(span)),
        "inline" | "noinline" => {
            lower_inline_policy_macro_expr(walker, &macro_name, &args, span, lambda_ctx)
        }
        "inbounds" => lower_inbounds_macro_expr(walker, &args, span, lambda_ctx),
        "assume_effects" => lower_assume_effects_expr_compat(walker, &args, span, lambda_ctx),
        "eval" => {
            if let Some(expr) = lower_eval_module_macro_expr(walker, &args, span)? {
                Ok(expr)
            } else if crate::base_loader::has_base_macro(&macro_name) {
                expand_base_macro_expr(walker, &macro_name, &args, span, lambda_ctx)
            } else {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::MacroCall,
                    span,
                ))
            }
        }
        // @__FILE__ - returns the current file path as a string literal
        "__FILE__" => {
            let file_path = lambda_ctx.get_current_file();
            Ok(Expr::Literal(Literal::Str(file_path), span))
        }
        // @__DIR__ - returns the directory of the current file as a string literal
        "__DIR__" => {
            let dir_path = lambda_ctx.get_current_dir();
            Ok(Expr::Literal(Literal::Str(dir_path), span))
        }
        // @__LINE__ - returns the current line number as an integer literal
        "__LINE__" => {
            let line_number = span.start_line as i64;
            Ok(Expr::Literal(Literal::Int(line_number), span))
        }
        // @__MODULE__ - returns the enclosing module as a module literal. Use the
        // lowering-time module stack (Issue #7919) so it resolves to the calling
        // module inside `module M ... end`, not always `Main`.
        "__MODULE__" => Ok(Expr::Literal(
            Literal::Module(
                lambda_ctx
                    .current_module()
                    .unwrap_or_else(|| "Main".to_string()),
            ),
            span,
        )),
        // @code_warntype f(args...) in expression context (Issue #5145).
        "code_warntype" => lower_code_warntype_macro_expr(walker, &args, span, lambda_ctx),
        // @view A[i:j] - create a view of an array slice
        "view" => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view requires exactly one argument: @view A[indices...]"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);

            if arg_kind == NodeKind::IndexExpression {
                let sub_children: Vec<Node<'a>> = walker.named_children(&arg);

                if sub_children.is_empty() {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint("@view argument must be an indexing expression like A[i:j]"),
                    );
                }

                let array_node = sub_children[0];
                let index_nodes = &sub_children[1..];

                let array_expr = super::lower_expr_with_ctx(walker, array_node, lambda_ctx)?;

                let mut call_args = vec![array_expr];
                for index_node in index_nodes {
                    let index_expr = super::lower_expr_with_ctx(walker, *index_node, lambda_ctx)?;
                    call_args.push(index_expr);
                }

                Ok(Expr::Call {
                    function: "view".to_string(),
                    args: call_args,
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                })
            } else {
                Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view argument must be an indexing expression like A[i:j]"),
                )
            }
        }
        // @views expression - convert all array slicing to views within expression
        "views" => {
            if args.is_empty() {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@views requires an expression argument"),
                );
            }

            let arg = args[0];
            lower_expr_with_views(walker, arg, lambda_ctx)
        }
        // @isdefined(x) - check if variable x is defined
        "isdefined" => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@isdefined requires exactly one argument (a variable name)"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            let var_name = match arg_kind {
                NodeKind::Identifier => walker.text(&arg).to_string(),
                _ => {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint("@isdefined argument must be a variable name (identifier)"),
                    );
                }
            };

            Ok(Expr::Builtin {
                name: crate::ir::core::BuiltinOp::IsDefined,
                args: vec![Expr::Literal(Literal::Str(var_name), span)],
                span,
            })
        }
        // @__dot__ / @. - broadcast all operations (Issue #2547)
        "__dot__" | "." => {
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@. / @__dot__ requires exactly one argument"),
                );
            }
            let inner = super::lower_expr_with_ctx(walker, args[0], lambda_ctx)?;
            Ok(dotify_expr(inner, span))
        }
        // @NamedTuple{a::Int, b} / @NamedTuple begin ... end -> NamedTuple type (Issue #5120)
        "NamedTuple" => lower_namedtuple_macro_expr(walker, &args, span),
        // @static - compile-time conditional evaluation
        "static" => lower_static_macro_expr(walker, &args, span, lambda_ctx),
        // Task macros for SubsetJuliaVM's sequential cooperative compatibility model.
        "task" => lower_task_macro_expr(walker, &args, span, lambda_ctx),
        "async" => lower_async_macro_expr(walker, &args, span, lambda_ctx),
        "sync" => lower_sync_macro_expr(walker, &args, span, lambda_ctx),
        _ => {
            // Check for user-defined macro (local context first)
            if lambda_ctx.has_macro(&macro_name) {
                expand_user_defined_macro_expr(walker, &macro_name, &args, span, lambda_ctx)
            } else if crate::base_loader::has_base_macro(&macro_name) {
                expand_base_macro_expr(walker, &macro_name, &args, span, lambda_ctx)
            } else if let Some(module_name) = lambda_ctx
                .get_usings()
                .iter()
                .find(|m| crate::stdlib_loader::has_bundled_package_macro(m, &macro_name))
                .cloned()
            {
                // Bundled-package macro in value position, e.g. Plots' @animate/@gif
                // (Issue #6355).
                expand_bundled_package_macro_expr(
                    walker,
                    &module_name,
                    &macro_name,
                    &args,
                    span,
                    lambda_ctx,
                )
            } else if macro_name == "test"
                || macro_name == "testset"
                || macro_name == "test_throws"
                || macro_name == "test_broken"
            {
                Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint(format!("@{} macro requires `using Test`", macro_name)),
                )
            } else {
                Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint(format!("unknown macro @{}", macro_name)),
                )
            }
        }
    }
}

/// Lower `@code_warntype f(args...)` in expression context into
/// `code_warntype(f, typeof((args...,)))` (Issue #5145). Mirrors the
/// statement-context handler in `lowering::stmt::macros`.
fn lower_code_warntype_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &crate::lowering::LambdaContext,
) -> LowerResult<Expr> {
    if !lambda_ctx
        .get_usings()
        .iter()
        .any(|u| u == "InteractiveUtils")
    {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@code_warntype macro requires `using InteractiveUtils`"),
        );
    }

    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@code_warntype expects a single function call: @code_warntype f(args...)",
            ),
        );
    }

    let call_node = args[0];
    if walker.kind(&call_node) != NodeKind::CallExpression {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@code_warntype expects a function call: @code_warntype f(args...)"),
        );
    }

    let named = walker.named_children(&call_node);
    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@code_warntype: could not find the called function"),
        );
    }
    let func_expr = super::lower_expr_with_ctx(walker, named[0], lambda_ctx)?;

    let arg_exprs: Vec<Expr> = named
        .iter()
        .skip(1)
        .filter(|n| {
            matches!(
                walker.kind(n),
                NodeKind::ArgumentList | NodeKind::TupleExpression
            )
        })
        .flat_map(|n| walker.named_children(n))
        .map(|n| super::lower_expr_with_ctx(walker, n, lambda_ctx))
        .collect::<Result<Vec<_>, _>>()?;

    let tuple_expr = Expr::TupleLiteral {
        elements: arg_exprs,
        span,
    };
    let types_expr = Expr::Call {
        function: "typeof".to_string(),
        args: vec![tuple_expr],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    Ok(Expr::Call {
        function: "code_warntype".to_string(),
        args: vec![func_expr, types_expr],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    })
}

#[cfg(test)]
mod tests {
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    fn lower_error(source: &str) -> String {
        let mut parser = Parser::new().expect("parser");
        let parse_outcome = parser.parse(source).expect("parse");
        let mut lowering = Lowering::new(source);
        let err = lowering
            .lower(parse_outcome)
            .expect_err("lowering should fail");
        err.to_string()
    }

    #[test]
    fn qualified_opaque_macro_reports_explicit_unsupported_expression_diagnostic() {
        let err = lower_error("f() = Base.Experimental.@opaque x -> x + 1");
        assert!(err.contains("opaque closures"), "{err}");
        assert!(err.contains("Issue #4289"), "{err}");
        assert!(!err.contains("unknown macro @opaque"), "{err}");
    }
}
