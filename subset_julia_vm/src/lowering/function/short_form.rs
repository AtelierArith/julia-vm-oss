//! Short-form function definition lowering.
//!
//! Handles `f(x) = expr` style definitions, lambda assignments,
//! arrow functions, and operator method definitions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Function, Stmt, TypedParam};
use crate::lowering::expr::lower_expr;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::{JuliaType, TypeParam};

use super::defaults::{extract_defaults_from_short_function, generate_default_arg_stubs};
use super::full_form::make_convert_call;
use super::signature::{
    parse_kwparam_from_kw_node, parse_parameter, parse_signature, parse_signature_call,
    parse_type_name,
};
use super::where_clause::parse_where_expression;

pub fn is_short_function_definition<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    if walker.kind(&node) != NodeKind::Assignment {
        return false;
    }

    let named = walker.named_children(&node);
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
            let inner_children = walker.named_children(&lhs);
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

    let named = walker.named_children(&node);
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
) -> LowerResult<Function> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

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
    lower_arrow_function_with_name(walker, rhs, name)
}

/// Lower an arrow function expression with a given name.
/// arrow_function_expression has structure: param(s) -> body
pub fn lower_arrow_function_with_name<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    name: String,
) -> LowerResult<Function> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty arrow function".to_string()),
            span,
        ));
    }

    // Parse parameters and body
    // Structure: identifier(s) or tuple -> expression
    let mut params = Vec::new();
    let mut body_node = None;

    for (i, child) in named.iter().enumerate() {
        match walker.kind(child) {
            NodeKind::Identifier => {
                // Single parameter or the body (if it's the last one after ->)
                if i == named.len() - 1 {
                    // Last identifier is always the body expression
                    // This handles both zero-arg lambdas and identity lambdas (x -> x)
                    body_node = Some(*child);
                } else if body_node.is_none() {
                    // This is a parameter
                    let param_name = walker.text(child).to_string();
                    params.push(TypedParam::untyped(param_name, walker.span(child)));
                } else {
                    // Already have body, this shouldn't happen
                    body_node = Some(*child);
                }
            }
            NodeKind::TupleExpression
            | NodeKind::ParenthesizedExpression
            | NodeKind::ArgumentList => {
                // Multiple parameters: (x, y)
                for param_child in walker.named_children(child) {
                    if walker.kind(&param_child) == NodeKind::Identifier {
                        let param_name = walker.text(&param_child).to_string();
                        params.push(TypedParam::untyped(param_name, walker.span(&param_child)));
                    }
                }
            }
            _ => {
                // This should be the body expression
                body_node = Some(*child);
            }
        }
    }

    let body_node = body_node.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing lambda body".to_string()),
            span,
        )
    })?;

    // Lower the body expression
    let body_expr = lower_expr(walker, body_node)?;
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
        kwparams: Vec::new(),
        type_params: Vec::new(),
        return_type: None,
        body,
        is_base_extension: false,
        span,
    })
}

/// Lower a short function definition: f(x) = expr
/// Also supports where clause: f(x::T) where T = x
/// Also supports return type annotation: f(x)::Int64 = expr
/// This is syntactic sugar for: function f(x) return expr end
pub fn lower_short_function<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Function> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

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
            let typed_children = walker.named_children(&lhs);
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

    // Lower the body expression
    let body_expr = lower_expr(walker, rhs)?;
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

/// Lower a short function definition, producing the main function plus stub methods
/// for any parameters with default values.
pub fn lower_short_function_all<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Function>> {
    let func = lower_short_function(walker, node)?;
    let defaults = extract_defaults_from_short_function(walker, node)?;
    let stubs = generate_default_arg_stubs(&func, &defaults);
    let mut result = vec![func];
    result.extend(stubs);
    Ok(result)
}

/// Extract default value expressions from a short function definition.
pub fn lower_operator_method<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Function> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    if children.len() < 3 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("invalid operator method definition".to_string()),
            span,
        ));
    }

    // First child is the operator name
    let name_node = children[0];
    let name = walker.text(&name_node).to_string();

    // Second child is the parameter list
    let params_node = children[1];
    let mut params = Vec::new();
    let mut kwparams = Vec::new();
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

    // Determine which child is the where clause and which is the body
    let mut type_params = Vec::new();

    // Check if there's a where clause (children.len() > 3 means we have where clause)
    // Structure: [operator, params, where_clause, body] or [operator, params, body]
    let body_index = if children.len() >= 4 && walker.kind(&children[2]) == NodeKind::WhereClause
    {
        // Parse where clause type parameters
        for where_child in walker.named_children(&children[2]) {
            match walker.kind(&where_child) {
                NodeKind::Identifier => {
                    let param_name = walker.text(&where_child).to_string();
                    type_params.push(TypeParam::new(param_name));
                }
                NodeKind::TypeParameters => {
                    for tp_child in walker.named_children(&where_child) {
                        match walker.kind(&tp_child) {
                            NodeKind::Identifier => {
                                type_params
                                    .push(TypeParam::new(walker.text(&tp_child).to_string()));
                            }
                            NodeKind::SubtypeExpression => {
                                let tp_children = walker.named_children(&tp_child);
                                if !tp_children.is_empty() {
                                    let tp_name = walker.text(&tp_children[0]).to_string();
                                    if tp_children.len() > 1 {
                                        let bound = walker.text(&tp_children[1]).to_string();
                                        type_params
                                            .push(TypeParam::with_upper_bound(tp_name, bound));
                                    } else {
                                        type_params.push(TypeParam::new(tp_name));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                NodeKind::SubtypeExpression => {
                    let tp_children = walker.named_children(&where_child);
                    if !tp_children.is_empty() {
                        let tp_name = walker.text(&tp_children[0]).to_string();
                        if tp_children.len() > 1 {
                            let bound = walker.text(&tp_children[1]).to_string();
                            type_params.push(TypeParam::with_upper_bound(tp_name, bound));
                        } else {
                            type_params.push(TypeParam::new(tp_name));
                        }
                    }
                }
                _ => {}
            }
        }
        3
    } else {
        children.len() - 1
    };

    // The last child is the body expression
    let body_node = children[body_index];
    let body_expr = lower_expr(walker, body_node)?;
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
        span,
    })
}
