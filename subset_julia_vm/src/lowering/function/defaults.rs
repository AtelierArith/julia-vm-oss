//! Default argument extraction and stub generation.
//!
//! Extracts default parameter values from function definitions
//! and generates multiple function stubs with progressively fewer defaults.

use crate::ir::core::{Block, Expr, Function, Stmt, TypedParam};
use crate::lowering::expr::lower_expr;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

pub(super) fn extract_defaults_from_function_def<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Option<Expr>>> {
    let named = walker.named_children(&node);
    for child in &named {
        match walker.kind(child) {
            NodeKind::Signature => {
                return extract_defaults_from_signature(walker, *child);
            }
            NodeKind::ParameterList => {
                // Pure Rust parser: ParameterList is a direct child of FunctionDefinition
                return extract_defaults_from_parameter_list(walker, *child);
            }
            NodeKind::CallExpression => {
                return extract_defaults_from_call_expr(walker, *child);
            }
            _ => {}
        }
    }
    Ok(Vec::new())
}

/// Extract defaults from a ParameterList node (Pure Rust parser, full-form functions).
/// In the Pure Rust parser, `function f(a, b=10) end` produces:
///   FunctionDefinition -> ParameterList -> [Parameter(a), Parameter(b, IntegerLiteral(10))]
pub(super) fn extract_defaults_from_parameter_list<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Option<Expr>>> {
    let mut defaults: Vec<Option<Expr>> = Vec::new();
    let mut seen_semicolon = false;

    for child in walker.named_children(&node) {
        let kind = walker.kind(&child);

        if kind == NodeKind::Semicolon {
            seen_semicolon = true;
            continue;
        }
        if seen_semicolon {
            continue; // Keyword params handled separately
        }

        match kind {
            NodeKind::Parameter => {
                if let Some(default_expr) = extract_default_from_parameter_node(walker, child)? {
                    defaults.push(Some(default_expr));
                } else {
                    defaults.push(None);
                }
            }
            NodeKind::SplatParameter | NodeKind::SplatExpression => {
                // Varargs parameters never have defaults
                // Handle both SplatParameter (full-form) and SplatExpression (short-form)
                // per Issue #2253 duality requirement
                defaults.push(None);
            }
            _ => {
                defaults.push(None);
            }
        }
    }
    Ok(defaults)
}

/// Extract defaults from a Signature node.
pub(super) fn extract_defaults_from_signature<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Option<Expr>>> {
    let named = walker.named_children(&node);
    for child in &named {
        match walker.kind(child) {
            NodeKind::CallExpression => {
                return extract_defaults_from_call_expr(walker, *child);
            }
            NodeKind::WhereExpression => {
                return extract_defaults_from_where_expr(walker, *child);
            }
            NodeKind::TypedExpression => {
                let typed_children = walker.named_children(child);
                if !typed_children.is_empty() {
                    let left = typed_children[0];
                    match walker.kind(&left) {
                        NodeKind::CallExpression => {
                            return extract_defaults_from_call_expr(walker, left);
                        }
                        NodeKind::WhereExpression => {
                            return extract_defaults_from_where_expr(walker, left);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Vec::new())
}

/// Extract defaults from a WhereExpression (recurses to find the CallExpression).
pub(super) fn extract_defaults_from_where_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Option<Expr>>> {
    let children = walker.named_children(&node);
    if children.is_empty() {
        return Ok(Vec::new());
    }
    let left = children[0];
    match walker.kind(&left) {
        NodeKind::WhereExpression => extract_defaults_from_where_expr(walker, left),
        NodeKind::CallExpression => extract_defaults_from_call_expr(walker, left),
        NodeKind::Signature => extract_defaults_from_signature(walker, left),
        _ => Ok(Vec::new()),
    }
}

/// Extract defaults from a CallExpression representing a function signature.
pub(super) fn extract_defaults_from_call_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Option<Expr>>> {
    let named = walker.named_children(&node);
    let args_node = named.iter().skip(1).find(|n| {
        matches!(
            walker.kind(n),
            NodeKind::ArgumentList | NodeKind::TupleExpression
        )
    });

    let mut defaults: Vec<Option<Expr>> = Vec::new();
    let mut saw_semicolon = false;

    if let Some(args_node) = args_node {
        for child in walker.children(args_node) {
            let kind_str = child.kind();
            if kind_str == ";" {
                saw_semicolon = true;
                continue;
            }
            if !child.is_named() {
                continue;
            }
            let kind = walker.kind(&child);
            if saw_semicolon {
                continue; // Keyword params handled separately
            }
            match kind {
                NodeKind::Assignment => {
                    if let Some(default_expr) = extract_default_from_assignment_node(walker, child)?
                    {
                        defaults.push(Some(default_expr));
                    } else {
                        defaults.push(None);
                    }
                }
                NodeKind::Parameter => {
                    if let Some(default_expr) = extract_default_from_parameter_node(walker, child)?
                    {
                        defaults.push(Some(default_expr));
                    } else {
                        defaults.push(None);
                    }
                }
                NodeKind::KeywordArgument => {
                    // Pure Rust parser: b=10 in call expressions becomes KeywordArgument
                    // KeywordArgument children: [Identifier(name), value_expr]
                    let kw_children = walker.named_children(&child);
                    if kw_children.len() >= 2 {
                        let value_node = kw_children[kw_children.len() - 1];
                        defaults.push(Some(lower_expr(walker, value_node)?));
                    } else {
                        defaults.push(None);
                    }
                }
                NodeKind::Operator => {}
                _ => {
                    defaults.push(None);
                }
            }
        }
    }
    Ok(defaults)
}

/// Extract default from a Parameter node (Pure Rust parser).
pub(super) fn extract_default_from_parameter_node<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<Expr>> {
    // Quick check: if the Parameter text doesn't contain '=', it has no default value.
    // This handles the Pure Rust parser where typed params like `p::AbstractString`
    // produce [Identifier("p"), Identifier("AbstractString")] without TypeClause.
    let node_text = walker.text(&node);
    if !node_text.contains('=') {
        return Ok(None);
    }

    // Has '=': find the default value expression.
    // Children layout for `greeting::String="Hello"`: [Identifier, TypeClause, StringLiteral]
    // Children layout for `b=10`: [Identifier, IntegerLiteral]
    let children = walker.named_children(&node);
    for (i, child) in children.iter().enumerate() {
        if i == 0 {
            continue; // Skip parameter name
        }
        let kind = walker.kind(child);
        if kind == NodeKind::TypeClause || kind == NodeKind::Identifier {
            // Skip type annotation: TypeClause (tree-sitter) or bare Identifier (Pure Rust parser)
            // For typed params without defaults, the 2nd+ children are type nodes.
            // The '=' check above ensures we only reach here if there IS a default,
            // so we skip type-related children and find the actual default value expression.
            continue;
        }
        return Ok(Some(lower_expr(walker, *child)?));
    }
    Ok(None)
}

/// Extract default from an Assignment node (tree-sitter parser).
pub(super) fn extract_default_from_assignment_node<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<Expr>> {
    let children: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();
    if children.len() >= 2 {
        let value_node = children[children.len() - 1];
        return Ok(Some(lower_expr(walker, value_node)?));
    }
    Ok(None)
}

/// Generate stub methods for parameters with default values.
pub(super) fn generate_default_arg_stubs(
    func: &Function,
    defaults: &[Option<Expr>],
) -> Vec<Function> {
    let first_default_idx = defaults.iter().position(|d| d.is_some());
    let first_default_idx = match first_default_idx {
        Some(idx) => idx,
        None => return Vec::new(),
    };

    let mut stubs = Vec::new();
    let total_params = func.params.len();

    for num_provided in (first_default_idx..total_params).rev() {
        if num_provided >= defaults.len() || defaults[num_provided].is_none() {
            continue;
        }

        let stub_params: Vec<TypedParam> = func.params[..num_provided].to_vec();

        let mut call_args: Vec<Expr> = stub_params
            .iter()
            .map(|p| Expr::Var(p.name.clone(), func.span))
            .collect();
        for default in defaults.iter().take(total_params).skip(num_provided) {
            if let Some(default) = default {
                call_args.push(default.clone());
            } else {
                break;
            }
        }

        let call_expr = Expr::Call {
            function: func.name.clone(),
            args: call_args,
            kwargs: Vec::new(),
            splat_mask: Vec::new(),
            kwargs_splat_mask: Vec::new(),
            span: func.span,
        };

        let return_stmt = Stmt::Return {
            value: Some(call_expr),
            span: func.span,
        };

        let body = Block {
            stmts: vec![return_stmt],
            span: func.span,
        };

        stubs.push(Function {
            name: func.name.clone(),
            params: stub_params,
            kwparams: func.kwparams.clone(),
            type_params: func.type_params.clone(),
            return_type: func.return_type.clone(),
            body,
            is_base_extension: func.is_base_extension,
            span: func.span,
        });
    }

    stubs
}

/// Wrap all return values in a function body with convert(ReturnType, value) calls.
/// This implements Julia's return type annotation semantics.
pub(super) fn extract_defaults_from_short_function<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<Option<Expr>>> {
    let named = walker.named_children(&node);
    if named.is_empty() {
        return Ok(Vec::new());
    }
    let lhs = named[0];
    match walker.kind(&lhs) {
        NodeKind::CallExpression => extract_defaults_from_call_expr(walker, lhs),
        NodeKind::WhereExpression => extract_defaults_from_where_expr(walker, lhs),
        NodeKind::Signature => extract_defaults_from_signature(walker, lhs),
        NodeKind::TypedExpression => {
            let typed_children = walker.named_children(&lhs);
            if !typed_children.is_empty() {
                let inner = typed_children[0];
                match walker.kind(&inner) {
                    NodeKind::CallExpression => extract_defaults_from_call_expr(walker, inner),
                    NodeKind::WhereExpression => extract_defaults_from_where_expr(walker, inner),
                    _ => Ok(Vec::new()),
                }
            } else {
                Ok(Vec::new())
            }
        }
        _ => Ok(Vec::new()),
    }
}
