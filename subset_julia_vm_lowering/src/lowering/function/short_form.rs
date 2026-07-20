//! Short-form function definition lowering.
//!
//! Handles `f(x) = expr` style definitions, lambda assignments,
//! arrow functions, and operator method definitions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Expr, Function, KwParam, Stmt, TypedParam};
use crate::lowering::expr::{lower_arrow_expr_as_nested_with_ctx, lower_expr, lower_expr_with_ctx};
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::JuliaType;

use super::defaults::{extract_defaults_from_short_function, generate_default_arg_stubs};
use super::full_form::make_convert_call;
use super::signature::{
    inject_callable_self_alias_prologue, inject_parameter_destructuring_prologue,
    parse_kwparam_from_kw_node, parse_parameter, parse_signature, parse_signature_call,
    parse_type_name,
};
use super::where_clause::{
    convert_params_with_type_vars, parse_where_clause_type_params, parse_where_expression,
};

pub fn is_short_function_definition<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    if walker.kind(&node) != NodeKind::Assignment {
        return false;
    }

    let named = walker.named_children_vec(&node);
    if named.is_empty() {
        return false;
    }

    let lhs = named[0];
    let lhs_kind = walker.kind(&lhs);

    // Check if the left-hand side is a call expression (e.g., f(x))
    // or a where expression (e.g., f(x::T) where T)
    // or a typed expression (e.g., f(x)::Int64 for return type annotation)
    match lhs_kind {
        NodeKind::CallExpression | NodeKind::WhereExpression | NodeKind::Signature => true,
        NodeKind::TypedExpression => {
            // Check if the inner left side is a call expression
            // e.g., f(x)::Int64 - the TypedExpression wraps CallExpression and type
            let inner_children = walker.named_children_vec(&lhs);
            if !inner_children.is_empty() {
                matches!(
                    walker.kind(&inner_children[0]),
                    NodeKind::CallExpression | NodeKind::WhereExpression | NodeKind::Signature
                )
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if an assignment node represents a lambda assignment.
/// Lambda assignments have the form: f = x -> expr or f = (x, y) -> expr
pub fn is_lambda_assignment<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    if walker.kind(&node) != NodeKind::Assignment {
        return false;
    }

    let named = walker.named_children_vec(&node);
    if named.len() < 2 {
        return false;
    }

    // Check if the right-hand side is an arrow function expression
    let rhs = named[named.len() - 1];
    matches!(walker.kind(&rhs), NodeKind::ArrowFunctionExpression)
}

/// Lower a lambda assignment: f = x -> expr
/// This converts the lambda to a named function.
pub fn lower_lambda_assignment<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Function>> {
    lower_lambda_assignment_impl(walker, node, None)
}

pub fn lower_lambda_assignment_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Function>> {
    lower_lambda_assignment_impl(walker, node, Some(lambda_ctx))
}

fn lower_lambda_assignment_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Vec<Function>> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

    if named.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("invalid lambda assignment".to_string()),
            span,
        ));
    }

    // Get the function name from the left-hand side
    let lhs = named[0];
    let name = match walker.kind(&lhs) {
        NodeKind::Identifier => walker.text(&lhs).to_string(),
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::Other(
                    "lambda assignment target must be identifier".to_string(),
                ),
                walker.span(&lhs),
            ));
        }
    };

    // Get the lambda from the right-hand side
    let rhs = named[named.len() - 1];
    lower_arrow_function_with_name_impl(walker, rhs, name, lambda_ctx)
}

/// Lower an arrow function expression with a given name.
/// arrow_function_expression has structure: param(s) -> body
pub fn lower_arrow_function_with_name<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    name: String,
) -> LowerResult<Vec<Function>> {
    lower_arrow_function_with_name_impl(walker, node, name, None)
}

fn lower_arrow_function_with_name_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    name: String,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Vec<Function>> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty arrow function".to_string()),
            span,
        ));
    }

    // Parse parameters and body
    // Structure: identifier(s) or tuple -> expression
    let mut params = Vec::new();
    let mut kwparams = Vec::new();
    // Optional positional default values, index-aligned with `params` (Issue #8047).
    let mut defaults: Vec<Option<Expr>> = Vec::new();
    let mut body_node = None;

    // The child after `->` is ALWAYS the body, regardless of its node kind. The
    // body may be a `ParenthesizedExpression` (`x -> (x + 1)`, `x -> (x[2] = v)`)
    // — these must NOT be routed into the parameter-collection arm below, which
    // would drop the body and raise "missing lambda body" (Issue #8007). This
    // mirrors `lower_arrow_function` in `expr/call.rs`, where the last child is
    // unconditionally treated as the body.
    let child_count = named.len();
    for (i, child) in named.iter().enumerate() {
        if i == child_count - 1 {
            // Last child = body expression (handles zero-arg lambdas, identity
            // lambdas `x -> x`, and parenthesized/assignment bodies).
            body_node = Some(*child);
        } else {
            match walker.kind(child) {
                NodeKind::Identifier => {
                    // A parameter: x in `x -> ...`.
                    let param_name = walker.text(child).to_string();
                    params.push(TypedParam::untyped(param_name, walker.span(child)));
                    defaults.push(None);
                }
                NodeKind::TupleExpression
                | NodeKind::ParenthesizedExpression
                | NodeKind::ParameterList
                | NodeKind::ArgumentList => {
                    // Multiple parameters: (x, y)
                    collect_arrow_parameters(
                        walker,
                        *child,
                        &mut params,
                        &mut kwparams,
                        &mut defaults,
                    )?;
                }
                _ => {}
            }
        }
    }

    let body_node = body_node.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing lambda body".to_string()),
            span,
        )
    })?;

    // Lower the body expression. A live top-level context alone must not
    // change ordinary arrow lowering; use it only when this arrow's own value
    // parameters can shadow a parametric base (Issue #10948), or when an
    // enclosing lexical type-parameter frame must remain visible (Issue
    // #11031).
    let requires_value_context =
        crate::lowering::contains_value_parametric_base(walker, body_node, &params, &kwparams);
    let requires_type_context = lambda_ctx.is_some_and(LambdaContext::has_active_type_params);
    let body_expr = match lambda_ctx.filter(|_| requires_value_context || requires_type_context) {
        Some(ctx) => ctx.with_function_body_scope(|| {
            ctx.with_active_value_params(&params, &kwparams, || {
                lower_expr_with_ctx(walker, body_node, ctx)
            })
        })?,
        None => lower_expr(walker, body_node)?,
    };
    let body_span = walker.span(&body_node);

    // Create a return statement from the body expression
    let return_stmt = Stmt::Return {
        value: Some(body_expr),
        span: body_span,
    };

    let body = Block {
        stmts: vec![return_stmt],
        span: body_span,
    };
    let (params, body) = inject_parameter_destructuring_prologue(params, body);

    let func = Function {
        name,
        params,
        kwparams,
        type_params: Vec::new(),
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    };

    let func = super::kw_defaults::inject_kwarg_default_prologues(func);

    // Emit reduced-arity stub methods so optional positional defaults bind in
    // shorter calls, e.g. `a = (x, d=2) -> ...` called as `a(1)` (Issue #8047).
    let stubs = generate_default_arg_stubs(&func, &defaults);
    let mut result = vec![func];
    result.extend(stubs);
    Ok(result)
}

fn collect_arrow_parameters<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    params: &mut Vec<TypedParam>,
    kwparams: &mut Vec<KwParam>,
    defaults: &mut Vec<Option<Expr>>,
) -> LowerResult<()> {
    let mut in_kwargs = false;
    for param_child in walker.named_children(&node) {
        match walker.kind(&param_child) {
            NodeKind::Semicolon => {
                in_kwargs = true;
            }
            NodeKind::Identifier if !in_kwargs => {
                let param_name = walker.text(&param_child).to_string();
                params.push(TypedParam::untyped(param_name, walker.span(&param_child)));
                defaults.push(None);
            }
            // Optional positional default `d=2` / typed `d::Int=2` parses as an
            // `Assignment` inside the arrow parameter tuple (Issue #8047). The LHS
            // is the parameter, the RHS the default expression.
            NodeKind::Assignment if !in_kwargs => {
                let assign_children: Vec<_> = walker
                    .named_children(&param_child)
                    .filter(|n| walker.kind(n) != NodeKind::Operator)
                    .collect();
                if assign_children.len() >= 2 {
                    let lhs = assign_children[0];
                    let rhs = assign_children[assign_children.len() - 1];
                    params.push(parse_parameter(walker, lhs)?);
                    defaults.push(Some(lower_expr(walker, rhs)?));
                }
            }
            NodeKind::Parameter | NodeKind::TypedExpression if !in_kwargs => {
                params.push(parse_parameter(walker, param_child)?);
                defaults.push(None);
            }
            NodeKind::TupleExpression if !in_kwargs => {
                params.push(parse_parameter(walker, param_child)?);
                defaults.push(None);
            }
            NodeKind::SplatExpression | NodeKind::SplatParameter if !in_kwargs => {
                params.push(parse_parameter(walker, param_child)?);
                defaults.push(None);
            }
            // Every post-`;` node is a keyword parameter, and they all go through
            // the SAME authority the named-function signature path uses
            // (Issue #10354). This arm used to be an ad-hoc partial copy that
            // recognized only the bare-Identifier `KwParameter` shape, so an
            // ANNOTATED keyword (`(y; k::Integer = 3) -> (y, k)`, which parses as
            // an `Assignment` with a `TypedExpression` LHS) matched nothing and was
            // silently dropped by the `_ => {}` below — the keyword disappeared from
            // the lambda's signature, `k` in the body compiled to a global load
            // (`UndefVarError`), and a supplied `k = 3` was rejected as an
            // "unsupported keyword argument", all of which upstream accepts. The
            // copy also missed the required-annotated (#11081) and declared-type
            // (#11024) handling. See `signature::parse_kwparam_node`.
            _ if in_kwargs => {
                if let Some(kw) = super::signature::parse_kwparam_node(walker, param_child)? {
                    kwparams.push(kw);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Lower a short function definition: f(x) = expr
/// Also supports where clause: f(x::T) where T = x
/// Also supports return type annotation: f(x)::Int64 = expr
/// This is syntactic sugar for: function f(x) return expr end
pub fn lower_short_function<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Function> {
    lower_short_function_impl(walker, node, None)
}

pub fn lower_short_function_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Function> {
    let _runtime_signature_scope = lambda_ctx
        .in_function_body()
        .then(crate::lowering::type_alias::RuntimeSignatureScope::new);
    lower_short_function_impl(walker, node, Some(lambda_ctx))
}

fn lower_short_function_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Function> {
    let span = walker.span(&node);
    let requires_nested_lambdas = crate::lowering::requires_nested_lambda_lowering(walker, node);
    let named = walker.named_children_vec(&node);

    if named.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("invalid short function definition".to_string()),
            span,
        ));
    }

    let lhs = named[0]; // f(x) or f(x::T) where T or f(x)::Int64
    let rhs = named[named.len() - 1]; // expr - the body

    // Parse the signature from the call expression, where expression, or typed expression
    let mut return_type: Option<JuliaType> = None;
    let (name, params, kwparams, is_base_extension, type_params) = match walker.kind(&lhs) {
        NodeKind::WhereExpression => parse_where_expression(walker, lhs)?,
        NodeKind::CallExpression => {
            let (n, p, kw, is_base) = parse_signature_call(walker, lhs)?;
            (n, p, kw, is_base, Vec::new())
        }
        NodeKind::Signature => {
            let (n, p, kw, is_base) = parse_signature(walker, lhs)?;
            (n, p, kw, is_base, Vec::new())
        }
        NodeKind::TypedExpression => {
            // Handle return type annotation: f(x)::Int64 = expr
            // TypedExpression has children: [signature, return_type]
            let typed_children = walker.named_children_vec(&lhs);
            if typed_children.len() >= 2 {
                let signature_node = typed_children[0];
                let type_node = typed_children[typed_children.len() - 1];

                // Extract return type
                let type_name = walker.text(&type_node);
                if let Some(rt) = parse_type_name(type_name, walker.span(&type_node))? {
                    return_type = Some(rt);
                }

                // Parse signature from inner node
                match walker.kind(&signature_node) {
                    NodeKind::WhereExpression => parse_where_expression(walker, signature_node)?,
                    NodeKind::CallExpression => {
                        let (n, p, kw, is_base) = parse_signature_call(walker, signature_node)?;
                        (n, p, kw, is_base, Vec::new())
                    }
                    NodeKind::Signature => {
                        let (n, p, kw, is_base) = parse_signature(walker, signature_node)?;
                        (n, p, kw, is_base, Vec::new())
                    }
                    _ => {
                        return Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::Other(format!(
                                "unexpected inner node in typed expression: {:?}",
                                walker.kind(&signature_node)
                            )),
                            span,
                        ));
                    }
                }
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::Other(
                        "invalid typed expression in short function".to_string(),
                    ),
                    span,
                ));
            }
        }
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::Other(format!(
                    "unexpected node in short function lhs: {:?}",
                    walker.kind(&lhs)
                )),
                span,
            ));
        }
    };

    let body_expr = lower_short_body_expr(
        walker,
        rhs,
        lambda_ctx,
        &type_params,
        &params,
        &kwparams,
        requires_nested_lambdas,
    )?;
    let body_span = walker.span(&rhs);

    // Create a return statement from the body expression
    // If there's a return type, wrap it with convert()
    let return_value = if let Some(ref rt) = return_type {
        make_convert_call(body_expr, rt, body_span)
    } else {
        body_expr
    };

    let return_stmt = Stmt::Return {
        value: Some(return_value),
        span: body_span,
    };

    let body = Block {
        stmts: vec![return_stmt],
        span: body_span,
    };
    let (params, body) = inject_parameter_destructuring_prologue(params, body);
    let (params, body) = inject_callable_self_alias_prologue(params, body);

    Ok(Function {
        name,
        params,
        kwparams,
        type_params,
        return_type,
        body,
        is_base_extension,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    })
}

fn lower_short_body_expr<'a>(
    walker: &CstWalker<'a>,
    body_node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
    type_params: &[crate::types::TypeParam],
    params: &[TypedParam],
    kwparams: &[KwParam],
    requires_nested_lambdas: bool,
) -> LowerResult<Expr> {
    // A `global` method definition inside a (short-form) function body is an
    // upstream lowering error (Issue #10937).
    crate::lowering::function::reject_global_method_definitions_in_body(walker, body_node)?;
    let requires_value_context =
        crate::lowering::contains_value_parametric_base(walker, body_node, params, kwparams);
    let requires_type_context = lambda_ctx.is_some_and(LambdaContext::has_active_type_params);
    let lambda_ctx = lambda_ctx
        .filter(|_| requires_nested_lambdas || requires_value_context || requires_type_context);
    let prefer_nested =
        |ctx: &LambdaContext| ctx.prefer_nested_lambdas() || requires_nested_lambdas;
    match walker.kind(&body_node) {
        NodeKind::ArrowFunctionExpression => match lambda_ctx {
            Some(ctx) => ctx.with_function_body_scope(|| {
                ctx.with_prefer_nested_lambdas(prefer_nested(ctx), || {
                    ctx.with_active_type_params(type_params, || {
                        ctx.with_active_value_params(params, kwparams, || {
                            lower_arrow_expr_as_nested_with_ctx(walker, body_node, ctx)
                        })
                    })
                })
            }),
            None => lower_expr(walker, body_node),
        },
        NodeKind::GlobalStatement => match lambda_ctx {
            Some(ctx) => ctx.with_function_body_scope(|| {
                ctx.with_active_type_params(type_params, || {
                    ctx.with_active_value_params(params, kwparams, || {
                        ctx.with_prefer_nested_lambdas(prefer_nested(ctx), || {
                            crate::lowering::stmt::lower_global_value_expr(
                                walker,
                                body_node,
                                Some(ctx),
                            )
                        })
                    })
                })
            }),
            None => crate::lowering::stmt::lower_global_value_expr(walker, body_node, None),
        },
        _ => match lambda_ctx {
            Some(ctx) => ctx.with_function_body_scope(|| {
                ctx.with_active_type_params(type_params, || {
                    ctx.with_active_value_params(params, kwparams, || {
                        ctx.with_prefer_nested_lambdas(prefer_nested(ctx), || {
                            lower_expr_with_ctx(walker, body_node, ctx)
                        })
                    })
                })
            }),
            None => lower_expr(walker, body_node),
        },
    }
}

/// Lower a short function definition, producing the main function plus stub methods
/// for any parameters with default values.
pub fn lower_short_function_all<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Function>> {
    let func = lower_short_function(walker, node)?;
    lower_short_function_all_from_function(walker, node, func)
}

pub fn lower_short_function_all_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Function>> {
    let func = lower_short_function_with_ctx(walker, node, lambda_ctx)?;
    lower_short_function_all_from_function(walker, node, func)
}

fn lower_short_function_all_from_function<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    func: Function,
) -> LowerResult<Vec<Function>> {
    // Re-evaluate side-effecting keyword-argument defaults per call by injecting
    // a guarded prologue into the body (Issue #5121). See `lower_function_all`.
    let func = super::kw_defaults::inject_kwarg_default_prologues(func);
    let defaults = extract_defaults_from_short_function(walker, node)?;
    let stubs = generate_default_arg_stubs(&func, &defaults);
    let mut result = vec![func];
    result.extend(stubs);
    Ok(result)
}

/// Extract default value expressions from a short function definition.
pub fn lower_operator_method<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Function> {
    lower_operator_method_impl(walker, node, None)
}

pub fn lower_operator_method_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Function> {
    lower_operator_method_impl(walker, node, Some(lambda_ctx))
}

fn lower_operator_method_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Function> {
    let span = walker.span(&node);
    let requires_nested_lambdas = crate::lowering::requires_nested_lambda_lowering(walker, node);
    let children = walker.named_children_vec(&node);

    if children.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("invalid operator method definition".to_string()),
            span,
        ));
    }

    // First child is the operator name
    let name_node = children[0];
    let name = walker.text(&name_node).to_string();

    // Determine whether there's a where clause. The parser builds children as
    // [operator, params, (return_type), (where_clause), body], so locate the
    // WhereClause by kind instead of assuming a fixed index.
    //
    // The where clause itself is parsed by the same shared helper as the
    // long form (`function *(...) where {...} ... end`). A previous hand-rolled
    // copy here did not recognize the BinaryExpression/SubtypeConstraint shapes
    // the pure parser emits for braced bounds, so `where {T<:Real}` silently
    // lowered as `where {T}` (Issue #6537).
    //
    // Extract the where-clause type parameters BEFORE parsing the parameter list
    // so a `where T` name shadows a same-named top-level alias (`T = Int64`)
    // during annotation parsing (Issue #7847, mirroring parse_where_expression).
    let mut type_params = Vec::new();
    for child in children.iter().skip(2) {
        if walker.kind(child) == NodeKind::WhereClause {
            type_params = parse_where_clause_type_params(walker, *child)?;
            break;
        }
    }
    let excluded_names: Vec<String> = type_params.iter().map(|tp| tp.name.clone()).collect();

    // Second child is the parameter list
    let params_node = children[1];
    let mut params = Vec::new();
    let mut kwparams = Vec::new();
    {
        let _binder_scope = crate::lowering::type_binder_env::Scope::from_names(&excluded_names);
        for param_node in walker.named_children(&params_node) {
            match walker.kind(&param_node) {
                NodeKind::KwParameter => {
                    if let Some(kwparam) = parse_kwparam_from_kw_node(walker, param_node)? {
                        kwparams.push(kwparam);
                    }
                }
                _ => {
                    if let Ok(param) = parse_parameter(walker, param_node) {
                        params.push(param);
                    }
                }
            }
        }
    }

    // Convert parameter annotations that name a type parameter into TypeVars,
    // mirroring the non-operator paths (parse_where_expression).
    let params = convert_params_with_type_vars(params, &type_params);

    // The last child is the body expression
    let body_node = children[children.len() - 1];
    let body_expr = lower_short_body_expr(
        walker,
        body_node,
        lambda_ctx,
        &type_params,
        &params,
        &kwparams,
        requires_nested_lambdas,
    )?;
    let body_span = walker.span(&body_node);

    // Create a return statement from the body expression
    let return_stmt = Stmt::Return {
        value: Some(body_expr),
        span: body_span,
    };

    let body = Block {
        stmts: vec![return_stmt],
        span: body_span,
    };

    Ok(Function {
        name,
        params,
        kwparams,
        type_params,
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    })
}
