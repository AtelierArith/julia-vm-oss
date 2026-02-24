//! Full-form function definition lowering.
//!
//! Handles `function f(...) ... end` style definitions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Function, KwParam, Stmt, TypedParam};
use crate::lowering::stmt;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::{JuliaType, TypeParam};

use super::defaults::{extract_defaults_from_function_def, generate_default_arg_stubs};
use super::signature::{
    parse_kwarg_splat_parameter, parse_kwparam_from_kw_node, parse_parameter, parse_signature_call,
    parse_signature_with_where, parse_type_name,
};
use super::where_clause::parse_where_expression;

pub fn lower_function<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Function> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut params: Vec<TypedParam> = Vec::new();
    let mut kwparams: Vec<KwParam> = Vec::new();
    let mut type_params: Vec<TypeParam> = Vec::new();
    let mut return_type: Option<JuliaType> = None;
    let mut is_base_extension = false;
    let mut body = None;

    // Track whether we've seen the parameter list - any Identifier after it is the return type
    let mut seen_params = false;

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
            NodeKind::Operator => {
                // Handle operator as function name: function +(a, b) ... end
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                }
            }
            NodeKind::FieldExpression => {
                // Handle Base.:+ syntax: function Base.:+(a, b) ... end
                // Also handle Base.show syntax: function Base.show(io, x) ... end
                // Extract the operator/function name from the field expression
                if name.is_none() {
                    let children = walker.named_children(&child);
                    if children.len() >= 2 {
                        let module = walker.text(&children[0]);
                        let field_text = walker.text(&children[1]);
                        if module == "Base" {
                            // Extract operator name from quoted expression
                            let mut op_name = if let Some(stripped) = field_text.strip_prefix(':')
                            {
                                stripped.to_string()
                            } else {
                                field_text.to_string()
                            };
                            // Strip parentheses if present (e.g., "(==)" -> "==")
                            if op_name.starts_with('(') && op_name.ends_with(')') {
                                op_name = op_name[1..op_name.len() - 1].to_string();
                            }
                            name = Some(op_name);
                            is_base_extension = true;
                        }
                    }
                }
            }
            NodeKind::ParenthesizedExpression => {
                // Callable struct definition: function (::Type)(args) body end
                // The parenthesized expression contains a UnaryTypedExpression
                if name.is_none() {
                    let inner_children = walker.named_children(&child);
                    if inner_children.len() == 1 {
                        let inner = inner_children[0];
                        if walker.kind(&inner) == NodeKind::UnaryTypedExpression {
                            // Extract type name from ::Type
                            let type_children = walker.named_children(&inner);
                            if !type_children.is_empty() {
                                let type_name = walker.text(&type_children[0]).to_string();
                                name = Some(format!("__callable_{}", type_name));
                            } else {
                                let type_text = walker.text(&inner);
                                let type_name =
                                    type_text.strip_prefix("::").unwrap_or(type_text);
                                name = Some(format!("__callable_{}", type_name));
                            }
                        }
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
                // Handle where clause from Pure Rust parser
                // WhereClause can contain:
                // - Identifier (simple: where T)
                // - TypeParameters (multiple: where {T, S} or where {T<:Real})
                // - SubtypeExpression (bounded: where T<:Real)
                for where_child in walker.named_children(&child) {
                    match walker.kind(&where_child) {
                        NodeKind::Identifier => {
                            let param_name = walker.text(&where_child).to_string();
                            type_params.push(TypeParam::new(param_name));
                        }
                        NodeKind::TypeParameters => {
                            // Handle where {T, S} or where {T<:Real}
                            for tp_child in walker.named_children(&where_child) {
                                match walker.kind(&tp_child) {
                                    NodeKind::TypeParameter => {
                                        // TypeParameter contains children describing the param
                                        let tp_children = walker.named_children(&tp_child);
                                        if tp_children.len() >= 2 {
                                            // Bounded: T<:Real
                                            let param_name =
                                                walker.text(&tp_children[0]).to_string();
                                            let bound = walker.text(&tp_children[1]).to_string();
                                            type_params
                                                .push(TypeParam::with_bound(param_name, bound));
                                        } else if !tp_children.is_empty() {
                                            // Unbounded: T
                                            let param_name =
                                                walker.text(&tp_children[0]).to_string();
                                            type_params.push(TypeParam::new(param_name));
                                        }
                                    }
                                    NodeKind::Identifier => {
                                        let param_name = walker.text(&tp_child).to_string();
                                        type_params.push(TypeParam::new(param_name));
                                    }
                                    NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
                                        let children = walker.named_children(&tp_child);
                                        if children.len() >= 2 {
                                            let param_name = walker.text(&children[0]).to_string();
                                            let bound = walker.text(&children[1]).to_string();
                                            type_params
                                                .push(TypeParam::with_bound(param_name, bound));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
                            let children = walker.named_children(&where_child);
                            if children.len() >= 2 {
                                let param_name = walker.text(&children[0]).to_string();
                                let bound = walker.text(&children[1]).to_string();
                                type_params.push(TypeParam::with_bound(param_name, bound));
                            } else if !children.is_empty() {
                                let param_name = walker.text(&children[0]).to_string();
                                type_params.push(TypeParam::new(param_name));
                            }
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::TypedExpression => {
                // Handle return type annotation: function f(x)::ReturnType
                // Extract the call expression from the left side of the typed expression
                // and the return type from the right side
                let typed_children = walker.named_children(&child);
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
                body = Some(stmt::lower_block(walker, child)?);
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

    let name = name.ok_or_else(|| {
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

    Ok(Function {
        name,
        params,
        kwparams,
        type_params,
        return_type,
        body,
        is_base_extension,
        span,
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

    let type_expr = Expr::Var(type_name, span);

    // Create the convert(Type, value) call
    Expr::Call {
        function: "convert".to_string(),
        args: vec![type_expr, value],
        kwargs: vec![],
        splat_mask: vec![false, false],
        kwargs_splat_mask: vec![],
        span,
    }
}
