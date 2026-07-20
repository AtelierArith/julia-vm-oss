//! Full-form function definition lowering.
//!
//! Handles `function f(...) ... end` style definitions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Expr, Function, KwParam, Literal, Stmt, TypedParam};
use crate::lowering::stmt;
use crate::lowering::{
    contains_value_parametric_base, requires_nested_lambda_lowering, LambdaContext, LowerResult,
};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::{JuliaType, TypeParam};

use super::defaults::{extract_defaults_from_function_def, generate_default_arg_stubs};
use super::signature::{
    inject_callable_self_alias_prologue, inject_parameter_destructuring_prologue,
    module_extension_function_name, parse_callable_self_param, parse_kwarg_splat_parameter,
    parse_kwparam, parse_kwparam_from_kw_node, parse_parameter, parse_signature_call,
    parse_signature_with_where, parse_type_name,
};
use super::where_clause::{
    convert_params_with_type_vars, parse_where_clause_type_params, parse_where_expression,
};

pub fn lower_function<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Function> {
    lower_function_impl(walker, node, None, None)
}

pub fn lower_function_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Function> {
    let _runtime_signature_scope = lambda_ctx
        .in_function_body()
        .then(crate::lowering::type_alias::RuntimeSignatureScope::new);
    lower_function_impl(walker, node, None, Some(lambda_ctx))
}

pub fn lower_anonymous_function_value<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let (name, func) = lower_anonymous_function_named(walker, node, lambda_ctx)?;
    let func = func.into_lowering_helper();

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::FunctionDef {
                    func: Box::new(func),
                    span,
                },
                Stmt::Expr {
                    expr: Expr::Var(name.into(), span),
                    span,
                },
            ],
            span,
        },
        span,
    })
}

pub fn lower_anonymous_function_named<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<(String, Function)> {
    let span = walker.span(&node);
    let name = lambda_ctx
        .map(LambdaContext::next_lambda_name)
        .unwrap_or_else(|| format!("__anonymous_function_{}", span.start));
    let func = lower_function_impl(walker, node, Some(name.clone()), lambda_ctx)?;
    // Named full-form functions pass through `lower_function`, whose caller
    // injects keyword-default guards. Anonymous full-form values and IIFEs
    // return directly from this helper, so establish the same entry prologue
    // here for every anonymous call path (Issue #11135).
    let func = super::kw_defaults::inject_kwarg_default_prologues(func);
    Ok((name, func))
}

/// Collect the names of the `where`-clause type parameters declared on a
/// full-form function definition, without fully parsing the signature.
///
/// The pure-Rust parser lays out `function f(x::T) where T ... end` as sibling
/// children `[Identifier, ParameterList, WhereClause, Block]`, so the parameter
/// annotations are parsed before the `WhereClause` is seen. This helper does a
/// cheap forward scan so those names can be excluded from type-alias expansion
/// while the parameters are parsed, ensuring a `where T` parameter shadows a
/// same-named top-level binding (Issue #7847). Errors during the scan are
/// swallowed (returning the names found so far); the authoritative parse happens
/// later in the main loop.
fn collect_where_param_names(walker: &CstWalker<'_>, node: Node<'_>) -> Vec<String> {
    let mut names = Vec::new();
    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::WhereClause => {
                if let Ok(tps) = parse_where_clause_type_params(walker, child) {
                    names.extend(tps.into_iter().map(|tp| tp.name));
                }
            }
            NodeKind::WhereExpression => {
                if let Ok((_, _, _, _, tps)) = parse_where_expression(walker, child) {
                    names.extend(tps.into_iter().map(|tp| tp.name));
                }
            }
            NodeKind::Signature => {
                if let Ok((_, _, _, _, tps)) = parse_signature_with_where(walker, child) {
                    names.extend(tps.into_iter().map(|tp| tp.name));
                }
            }
            NodeKind::TypedExpression => {
                // `function f(x::T)::R where T` nests the where expression in the
                // left child of the return-type-annotated typed expression.
                for inner in walker.named_children(&child) {
                    if walker.kind(&inner) == NodeKind::WhereExpression {
                        if let Ok((_, _, _, _, tps)) = parse_where_expression(walker, inner) {
                            names.extend(tps.into_iter().map(|tp| tp.name));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    names
}

fn lower_function_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    fallback_name: Option<String>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Function> {
    let span = walker.span(&node);
    let requires_nested_lambdas = requires_nested_lambda_lowering(walker, node);
    let mut name: Option<String> = None;
    let mut params: Vec<TypedParam> = Vec::new();
    let mut kwparams: Vec<KwParam> = Vec::new();
    let mut type_params: Vec<TypeParam> = Vec::new();
    let mut return_type: Option<JuliaType> = None;
    let mut is_base_extension = false;
    let mut body = None;
    // Bound callable struct `function (self::Type)(args) ... end`: the struct
    // instance binds to `self` as a synthetic leading parameter, injected after
    // the parameter list is parsed (Issue #5126).
    let mut callable_self_param: Option<TypedParam> = None;

    // Track whether we've seen the parameter list - any Identifier after it is the return type
    let mut seen_params = false;

    // Pre-pass: collect the `where`-clause type-parameter names so they shadow
    // any same-named top-level binding (`T = Int64`, registered as a type alias)
    // while the parameter annotations are parsed below. The pure-Rust parser
    // emits the parameter list as a sibling `ParameterList` node BEFORE the
    // `WhereClause` node, so without this pre-pass `f(x::T) where T` would freeze
    // `x`'s annotation to the alias target (`Int64`) instead of the type
    // variable `T`. A method's `where` parameter lexically shadows the global,
    // exactly as upstream Julia scopes it (Issue #7847; mirrors the struct
    // parameter exclusion of Issue #7840 and the `parse_where_expression` fix).
    let prepass_where_names = collect_where_param_names(walker, node);
    let _binder_scope = crate::lowering::type_binder_env::Scope::from_names(&prepass_where_names);

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                } else if seen_params && return_type.is_none() {
                    // After parameter list, an Identifier is the return type annotation
                    // e.g., `function calc_pi(N::Int64)::Float64 ... end`
                    // Parser emits: Identifier(calc_pi), ParameterList, Identifier(Float64), Block
                    let type_name = walker.text(&child);
                    if let Some(rt) = parse_type_name(type_name, walker.span(&child))? {
                        return_type = Some(rt);
                    }
                }
            }
            NodeKind::Operator if name.is_none() => {
                // Handle operator as function name: function +(a, b) ... end
                name = Some(walker.text(&child).to_string());
            }
            NodeKind::FieldExpression if name.is_none() => {
                // Handle `Base.:+`/`Base.show` (Base extension) and `Inner.f`
                // (extend another module's function, Issue #8052). For non-Base
                // modules the method is registered under the module-qualified name
                // so it joins the target module's existing generic function.
                let children = walker.named_children_vec(&child);
                if children.len() >= 2 {
                    let module = walker.text(&children[0]);
                    let field_text = walker.text(&children[1]);
                    let (resolved_name, base_ext) =
                        module_extension_function_name(module, field_text);
                    name = Some(resolved_name);
                    is_base_extension = base_ext;
                }
            }
            NodeKind::ParenthesizedExpression if name.is_none() => {
                // Callable struct definition (full form):
                //   function (::Type)(args) body end        (anonymous self)
                //   function (self::Type)(args) body end     (bound self)
                // The parenthesized expression contains a UnaryTypedExpression
                // (anonymous) or a TypedExpression (`self::Type`).
                let inner_children = walker.named_children_vec(&child);
                if inner_children.len() == 1 {
                    let inner = inner_children[0];
                    match walker.kind(&inner) {
                        NodeKind::UnaryTypedExpression => {
                            // Extract type name from ::Type
                            let type_children = walker.named_children_vec(&inner);
                            if !type_children.is_empty() {
                                let type_name = walker.text(&type_children[0]).to_string();
                                name = Some(format!("__callable_{}", type_name));
                            } else {
                                let type_text = walker.text(&inner);
                                let type_name = type_text.strip_prefix("::").unwrap_or(type_text);
                                name = Some(format!("__callable_{}", type_name));
                            }
                        }
                        NodeKind::TypedExpression => {
                            // Bound form `(self::Type)(args)`: bind the struct
                            // instance to `self` so the body can read its fields.
                            // The runtime prepends the instance to the call
                            // arguments (Issue #5126 / Issue #5127).
                            let (param, type_name) = parse_callable_self_param(walker, inner)?;
                            callable_self_param = Some(param);
                            name = Some(format!("__callable_{}", type_name));
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::ParameterList => {
                // Handle parameter list when it's a direct child of function definition
                // This happens with Pure Rust parser for typed signatures
                //
                // Track whether we've seen a semicolon - the parser includes Semicolon nodes as markers
                let mut seen_semicolon = false;
                for param_node in walker.named_children(&child) {
                    let kind = walker.kind(&param_node);

                    // Check for semicolon marker (added by parser)
                    if kind == NodeKind::Semicolon {
                        seen_semicolon = true;
                        continue;
                    }

                    match kind {
                        NodeKind::Assignment if seen_semicolon => {
                            // Anonymous full-form functions are parsed through an
                            // assignment expression (`f = function (...) ... end`).
                            // Their optional keywords therefore retain the generic
                            // `Assignment` CST shape rather than the `KwParameter`
                            // shape used by named full-form definitions.
                            if let Some(kwparam) = parse_kwparam(walker, param_node)? {
                                kwparams.push(kwparam);
                            }
                        }
                        NodeKind::KwParameter => {
                            // Keyword parameter (after semicolon): y=10 or y::T=10
                            if let Some(kwparam) = parse_kwparam_from_kw_node(walker, param_node)? {
                                kwparams.push(kwparam);
                            }
                        }
                        NodeKind::SplatParameter | NodeKind::SplatExpression => {
                            // Could be positional varargs or kwargs varargs
                            // It's kwargs splat if: we've seen a semicolon
                            // Handle both SplatParameter (full-form) and SplatExpression (short-form)
                            // per Issue #2253 duality requirement
                            if seen_semicolon {
                                // This is kwargs varargs: function f(; kwargs...)
                                if let Some(kwparam) =
                                    parse_kwarg_splat_parameter(walker, param_node)?
                                {
                                    kwparams.push(kwparam);
                                }
                            } else {
                                // Regular positional varargs: f(args...)
                                if let Ok(param) = parse_parameter(walker, param_node) {
                                    params.push(param);
                                }
                            }
                        }
                        NodeKind::TypedExpression | NodeKind::TypedParameter if seen_semicolon => {
                            // A typed keyword without `=` is required. Reuse the
                            // positional parameter parser for its name/type, then
                            // encode requiredness with the standard Undef marker.
                            if let Ok(param) = parse_parameter(walker, param_node) {
                                kwparams.push(KwParam::new(
                                    param.name,
                                    Expr::Literal(Literal::Undef, param.span),
                                    param.type_annotation,
                                    param.span,
                                ));
                            }
                        }
                        NodeKind::Identifier if seen_semicolon => {
                            let param_span = walker.span(&param_node);
                            kwparams.push(KwParam::new(
                                walker.text(&param_node).to_string(),
                                Expr::Literal(Literal::Undef, param_span),
                                None,
                                param_span,
                            ));
                        }
                        _ => {
                            // Regular positional parameter (only before semicolon)
                            // After semicolon, unmatched nodes should be skipped
                            if !seen_semicolon {
                                if let Ok(param) = parse_parameter(walker, param_node) {
                                    params.push(param);
                                }
                            }
                        }
                    }
                }
                seen_params = true;
            }
            NodeKind::TypeParameters => {
                // Handle type parameters from Pure Rust parser: {T} after function name
                // Just append to name to form "Rational{T}" - type params are extracted from WhereClause
                if let Some(ref mut n) = name {
                    n.push_str(walker.text(&child));
                }
            }
            NodeKind::CallExpression => {
                let (sig_name, sig_params, sig_kwparams, sig_is_base) =
                    parse_signature_call(walker, child)?;
                name = Some(sig_name);
                params = sig_params;
                kwparams = sig_kwparams;
                is_base_extension = sig_is_base;
                seen_params = true;
            }
            NodeKind::Signature => {
                // Handle signature node (generated for typed parameters)
                // Use parse_signature_with_where to handle where clauses inside signatures
                let (sig_name, sig_params, sig_kwparams, sig_is_base, sig_type_params) =
                    parse_signature_with_where(walker, child)?;
                name = Some(sig_name);
                params = sig_params;
                kwparams = sig_kwparams;
                is_base_extension = sig_is_base;
                type_params = sig_type_params;
                seen_params = true;
            }
            NodeKind::WhereExpression => {
                // Handle where clause: function f(x::T) where T<:Real
                let (sig_name, sig_params, sig_kwparams, sig_is_base, sig_type_params) =
                    parse_where_expression(walker, child)?;
                name = Some(sig_name);
                params = sig_params;
                kwparams = sig_kwparams;
                is_base_extension = sig_is_base;
                type_params = sig_type_params;
            }
            NodeKind::WhereClause => {
                // Handle where clause from Pure Rust parser. Shared with the
                // assignment-form operator path (Issue #6537); see
                // `parse_where_clause_type_params` for the recognized shapes.
                type_params.extend(parse_where_clause_type_params(walker, child)?);
            }
            NodeKind::TypedExpression => {
                // Handle return type annotation: function f(x)::ReturnType
                // Extract the call expression from the left side of the typed expression
                // and the return type from the right side
                let typed_children = walker.named_children_vec(&child);
                if typed_children.len() >= 2 {
                    let left = typed_children[0];
                    let right = typed_children[typed_children.len() - 1];

                    // Extract the return type from the right side
                    let type_name = walker.text(&right);
                    if let Some(rt) = parse_type_name(type_name, walker.span(&right))? {
                        return_type = Some(rt);
                    }

                    // Extract signature from the left side
                    match walker.kind(&left) {
                        NodeKind::CallExpression => {
                            let (sig_name, sig_params, sig_kwparams, sig_is_base) =
                                parse_signature_call(walker, left)?;
                            name = Some(sig_name);
                            params = sig_params;
                            kwparams = sig_kwparams;
                            is_base_extension = sig_is_base;
                        }
                        NodeKind::WhereExpression => {
                            // Return type with where clause: function f(x::T)::R where T
                            let (sig_name, sig_params, sig_kwparams, sig_is_base, sig_type_params) =
                                parse_where_expression(walker, left)?;
                            name = Some(sig_name);
                            params = sig_params;
                            kwparams = sig_kwparams;
                            is_base_extension = sig_is_base;
                            type_params = sig_type_params;
                        }
                        _ => {}
                    }
                } else if !typed_children.is_empty() {
                    // Single child - try to extract signature
                    let left = typed_children[0];
                    match walker.kind(&left) {
                        NodeKind::CallExpression => {
                            let (sig_name, sig_params, sig_kwparams, sig_is_base) =
                                parse_signature_call(walker, left)?;
                            name = Some(sig_name);
                            params = sig_params;
                            kwparams = sig_kwparams;
                            is_base_extension = sig_is_base;
                        }
                        NodeKind::WhereExpression => {
                            let (sig_name, sig_params, sig_kwparams, sig_is_base, sig_type_params) =
                                parse_where_expression(walker, left)?;
                            name = Some(sig_name);
                            params = sig_params;
                            kwparams = sig_kwparams;
                            is_base_extension = sig_is_base;
                            type_params = sig_type_params;
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::Block => {
                // A `global` method definition inside a function body is an
                // upstream lowering error (Issue #10937); a top-level `let`
                // body is not a function body, so the check lives here.
                crate::lowering::function::reject_global_method_definitions_in_body(walker, child)?;
                let requires_value_context =
                    contains_value_parametric_base(walker, child, &params, &kwparams);
                body = Some(match lambda_ctx {
                    Some(ctx)
                        if requires_nested_lambdas
                            || requires_value_context
                            || ctx.has_active_type_params() =>
                    {
                        ctx.with_function_body_scope(|| {
                            ctx.with_active_type_params(&type_params, || {
                                ctx.with_active_value_params(&params, &kwparams, || {
                                    let prefer_nested =
                                        ctx.prefer_nested_lambdas() || requires_nested_lambdas;
                                    ctx.with_prefer_nested_lambdas(prefer_nested, || {
                                        stmt::lower_block_with_ctx(walker, child, ctx)
                                    })
                                })
                            })
                        })?
                    }
                    _ => stmt::lower_block(walker, child)?,
                });
            }
            NodeKind::ParametrizedTypeExpression if return_type.is_none() => {
                // This might be a return type annotation like `::Complex{Float64}`
                // Pure Rust parser might put this as a direct child of FunctionDefinition
                // Only handle it if we haven't already extracted a return type
                let type_name = walker.text(&child);
                if let Some(rt) = parse_type_name(type_name, walker.span(&child))? {
                    // Only set if this looks like it's after the signature (return type)
                    // We detect this by checking if we already have a name
                    if name.is_some() {
                        return_type = Some(rt);
                    }
                }
            }
            _ => {}
        }
    }

    let name = name.or(fallback_name).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing function name".to_string()),
            span,
        )
    })?;

    let body = body.unwrap_or(Block {
        stmts: Vec::new(),
        span,
    });

    // Wrap return values with convert() for return type annotations.
    // This implements Julia's return type annotation semantics: f(x)::T = expr
    // becomes equivalent to f(x) = convert(T, expr)
    let body = if let Some(ref rt) = return_type {
        wrap_returns_with_convert(body, rt, span)
    } else {
        body
    };

    // Bound callable struct (full form): inject `self` as the leading parameter
    // so the body can read the struct's fields. The runtime prepends the struct
    // instance to the call arguments (Issue #5126 / Issue #5127).
    if let Some(self_param) = callable_self_param {
        params.insert(0, self_param);
    }

    let params = convert_params_with_type_vars(params, &type_params);
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

/// Lower a function definition, producing the main function plus stub methods
/// for any parameters with default values.
///
/// In Julia, `function f(a, b=10, c=20) ... end` desugars into three methods:
///   - `f(a, b, c)` — the full method with the original body
///   - `f(a, b) = f(a, b, 20)` — stub forwarding c's default
///   - `f(a) = f(a, 10, 20)` — stub forwarding b and c's defaults
pub fn lower_function_all<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Function>> {
    let func = lower_function(walker, node)?;
    lower_function_all_from_function(walker, node, func)
}

pub fn lower_function_all_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Vec<Function>> {
    let func = lower_function_with_ctx(walker, node, lambda_ctx)?;
    lower_function_all_from_function(walker, node, func)
}

fn lower_function_all_from_function<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    func: Function,
) -> LowerResult<Vec<Function>> {
    // Re-evaluate side-effecting keyword-argument defaults per call by injecting
    // a guarded prologue into the body (Issue #5121). Done before stub
    // generation so positional-default stubs forward to the prologue-bearing
    // method and inherit the updated kwparam flags.
    let func = super::kw_defaults::inject_kwarg_default_prologues(func);
    let defaults = extract_defaults_from_function_def(walker, node)?;
    let stubs = generate_default_arg_stubs(&func, &defaults);
    let mut result = vec![func];
    result.extend(stubs);
    Ok(result)
}

/// Extract default value expressions from a full-form function definition CST node.
pub(super) fn wrap_returns_with_convert(
    mut body: Block,
    return_type: &JuliaType,
    fn_span: crate::span::Span,
) -> Block {
    // Process all statements in the body
    body.stmts = body
        .stmts
        .into_iter()
        .map(|stmt| wrap_stmt_returns(stmt, return_type, fn_span))
        .collect();

    // Handle implicit return (last expression)
    // In Julia, the last expression in a function is its return value
    if let Some(last_stmt) = body.stmts.last_mut() {
        if let Stmt::Expr { expr, span } = last_stmt {
            // Wrap the last expression's value with convert
            *last_stmt = Stmt::Expr {
                expr: make_convert_call(expr.clone(), return_type, *span),
                span: *span,
            };
        }
    }

    body
}

/// Recursively wrap return values in a statement with convert calls.
pub(super) fn wrap_stmt_returns(
    stmt: Stmt,
    return_type: &JuliaType,
    fn_span: crate::span::Span,
) -> Stmt {
    use crate::ir::core::Stmt;

    match stmt {
        Stmt::Return {
            value: Some(expr),
            span,
        } => {
            // Wrap the return value with convert(ReturnType, value)
            Stmt::Return {
                value: Some(make_convert_call(expr, return_type, span)),
                span,
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => {
            // Recursively process both branches
            Stmt::If {
                condition,
                then_branch: Block {
                    stmts: then_branch
                        .stmts
                        .into_iter()
                        .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                        .collect(),
                    span: then_branch.span,
                },
                else_branch: else_branch.map(|eb| Block {
                    stmts: eb
                        .stmts
                        .into_iter()
                        .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                        .collect(),
                    span: eb.span,
                }),
                span,
            }
        }
        Stmt::For {
            var,
            start,
            end,
            step,
            body: for_body,
            span,
        } => Stmt::For {
            var,
            start,
            end,
            step,
            body: Block {
                stmts: for_body
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: for_body.span,
            },
            span,
        },
        Stmt::ForEach {
            var,
            iterable,
            body: for_body,
            span,
        } => Stmt::ForEach {
            var,
            iterable,
            body: Block {
                stmts: for_body
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: for_body.span,
            },
            span,
        },
        Stmt::ForEachTuple {
            vars,
            iterable,
            body: for_body,
            span,
        } => Stmt::ForEachTuple {
            vars,
            iterable,
            body: Block {
                stmts: for_body
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: for_body.span,
            },
            span,
        },
        Stmt::While {
            condition,
            body: while_body,
            span,
        } => Stmt::While {
            condition,
            body: Block {
                stmts: while_body
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: while_body.span,
            },
            span,
        },
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            span,
        } => Stmt::Try {
            try_block: Block {
                stmts: try_block
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: try_block.span,
            },
            catch_var,
            catch_block: catch_block.map(|cb| Block {
                stmts: cb
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: cb.span,
            }),
            else_block: else_block.map(|eb| Block {
                stmts: eb
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: eb.span,
            }),
            finally_block: finally_block.map(|fb| Block {
                stmts: fb
                    .stmts
                    .into_iter()
                    .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                    .collect(),
                span: fb.span,
            }),
            span,
        },
        Stmt::Block(block) => Stmt::Block(Block {
            stmts: block
                .stmts
                .into_iter()
                .map(|s| wrap_stmt_returns(s, return_type, fn_span))
                .collect(),
            span: block.span,
        }),
        // Other statements don't contain returns
        other => other,
    }
}

/// Create a convert(Type, value) call expression.
/// For parametric types (containing '{'), we skip the convert call since
/// the VM doesn't support using parametric type names as variables.
pub(super) fn make_convert_call(
    value: crate::ir::core::Expr,
    return_type: &JuliaType,
    span: crate::span::Span,
) -> crate::ir::core::Expr {
    use crate::ir::core::Expr;

    // Create a DataType literal for the return type
    let type_name = return_type.to_string();

    // Skip convert for parametric types (e.g., Vector{Int64}, Array{Float64,2})
    // These type names contain '{' and cannot be used as variables directly
    if type_name.contains('{') {
        return value;
    }

    let type_expr = Expr::Var(type_name.into(), span);

    // Create the convert(Type, value) call
    Expr::Call {
        function: "convert".to_string().into(),
        args: vec![type_expr, value],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    }
}
