//! Lowering for abstract type definitions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::AbstractTypeDef;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::TypeParam;

/// Lower an abstract type definition node to AbstractTypeDef IR.
///
/// Handles:
/// - `abstract type Name end` (simple abstract type)
/// - `abstract type Name <: Parent end` (abstract type with parent)
/// - `abstract type Name{T} end` (parametric abstract type)
/// - `abstract type Name{T} <: Parent end` (parametric with parent)
pub fn lower_abstract_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<AbstractTypeDef> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut parent: Option<String> = None;
    let mut type_params: Vec<TypeParam> = Vec::new();

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                // First identifier is the name, second is the parent (for Pure Rust parser)
                // In Pure Rust parser: AbstractDefinition contains [Identifier(name), Identifier(parent)]
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                } else if parent.is_none() {
                    parent = Some(walker.text(&child).to_string());
                }
            }
            NodeKind::TypeHead => {
                // Type head wraps the type definition
                for type_child in walker.named_children(&child) {
                    match walker.kind(&type_child) {
                        NodeKind::Identifier => {
                            if name.is_none() {
                                name = Some(walker.text(&type_child).to_string());
                            }
                        }
                        NodeKind::ParametrizedTypeExpression => {
                            // abstract type Container{T} end
                            let (parsed_name, params) =
                                parse_parametrized_abstract_head(walker, type_child)?;
                            name = Some(parsed_name);
                            type_params = params;
                        }
                        NodeKind::BinaryExpression | NodeKind::SubtypeExpression => {
                            // abstract type A <: B end
                            // abstract type A{T} <: B end (parametric with parent)
                            if let Some((n, p, params)) =
                                try_parse_subtype_clause(walker, type_child)?
                            {
                                name = Some(n);
                                parent = p;
                                if !params.is_empty() {
                                    type_params = params;
                                }
                            }
                        }
                        _ => {
                            // Try to extract name from other nodes
                            if name.is_none() {
                                let text = walker.text(&type_child).trim();
                                if !text.is_empty() && !text.contains('<') && !text.contains('{') {
                                    name = Some(text.to_string());
                                }
                            }
                        }
                    }
                }
            }
            NodeKind::TypeParameters => {
                // Pure Rust parser: TypeParameters node contains TypeParameter children
                // e.g., abstract type AbstractDict{K,V} <: Any end
                type_params = parse_type_parameters_node(walker, child)?;
            }
            NodeKind::ParametrizedTypeExpression => {
                // Parametric abstract type: Container{T}
                let (parsed_name, params) = parse_parametrized_abstract_head(walker, child)?;
                name = Some(parsed_name);
                type_params = params;
            }
            NodeKind::BinaryExpression | NodeKind::SubtypeExpression => {
                // Subtype clause: A <: B
                // Also handles parametric: A{T} <: B (via tree-sitter path)
                if let Some((n, p, params)) = try_parse_subtype_clause(walker, child)? {
                    name = Some(n);
                    parent = p;
                    if !params.is_empty() {
                        type_params = params;
                    }
                }
            }
            _ => {
                // Try to extract from other node types
                if name.is_none() {
                    let text = walker.text(&child).trim();
                    if !text.is_empty() && !text.contains("end") {
                        // Might be a complex expression, try to parse
                        if let Some((n, p)) = parse_from_text(text) {
                            name = Some(n);
                            parent = p;
                        }
                    }
                }
            }
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing abstract type name".to_string()),
            span,
        )
    })?;

    Ok(AbstractTypeDef {
        name,
        parent,
        type_params,
        span,
    })
}

/// Parse a parametrized abstract type head like `Container{T}`.
fn parse_parametrized_abstract_head<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypeParam>)> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut type_params: Vec<TypeParam> = Vec::new();

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                }
            }
            NodeKind::CurlyExpression => {
                type_params = parse_curly_type_params(walker, child)?;
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(
                "missing abstract type name in parametrized type".to_string(),
            ),
            span,
        )
    })?;

    Ok((name, type_params))
}

/// Parse type parameters from a `TypeParameters` node (Pure Rust parser).
/// The node contains `TypeParameter` children, each with an `Identifier` and optional bound.
fn parse_type_parameters_node<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<TypeParam>> {
    let mut params = Vec::new();

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::TypeParameter => {
                // TypeParameter contains [Identifier] or [Identifier, TypeExpression] for bounds
                let named = walker.named_children(&child);
                let identifiers: Vec<_> = named
                    .iter()
                    .filter(|n| matches!(walker.kind(n), NodeKind::Identifier))
                    .collect();
                if let Some(first) = identifiers.first() {
                    let param_name = walker.text(first).to_string();
                    if identifiers.len() >= 2 {
                        let bound_name = walker.text(identifiers[1]).to_string();
                        params.push(TypeParam::with_bound(param_name, bound_name));
                    } else {
                        params.push(TypeParam::new(param_name));
                    }
                }
            }
            NodeKind::Identifier => {
                let param_name = walker.text(&child).to_string();
                params.push(TypeParam::new(param_name));
            }
            _ => {}
        }
    }

    Ok(params)
}

/// Parse type parameters from a curly expression like `{T}` or `{T, S}`.
fn parse_curly_type_params<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<TypeParam>> {
    let mut params = Vec::new();

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                let param_name = walker.text(&child).to_string();
                params.push(TypeParam::new(param_name));
            }
            NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
                // Bounded type parameter: T <: Number
                if let Some(param) = parse_bounded_type_param(walker, child)? {
                    params.push(param);
                }
            }
            _ => {
                let text = walker.text(&child).trim();
                if !text.is_empty() && !text.contains('<') {
                    params.push(TypeParam::new(text.to_string()));
                }
            }
        }
    }

    Ok(params)
}

/// Parse a bounded type parameter like `T <: Number`.
fn parse_bounded_type_param<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<TypeParam>> {
    // Check if this is a <: operator
    let mut is_subtype_op = false;
    for child in walker.children(&node) {
        if child.kind() == "operator" && walker.text(&child) == "<:" {
            is_subtype_op = true;
            break;
        }
    }

    if !is_subtype_op {
        return Ok(None);
    }

    let named = walker.named_children(&node);
    let identifiers: Vec<_> = named
        .iter()
        .filter(|n| matches!(walker.kind(n), NodeKind::Identifier))
        .collect();

    if identifiers.len() >= 2 {
        let param_name = walker.text(identifiers[0]).to_string();
        let bound_name = walker.text(identifiers[1]).to_string();
        // Store bound as string to support user-defined abstract types
        Ok(Some(TypeParam::with_bound(param_name, bound_name)))
    } else if identifiers.len() == 1 {
        let param_name = walker.text(identifiers[0]).to_string();
        Ok(Some(TypeParam::new(param_name)))
    } else {
        Ok(None)
    }
}

/// Try to parse a subtype clause like `A <: B` from a binary expression or subtype expression.
/// Returns (name, parent, type_params) to preserve type parameters from parametric LHS.
fn try_parse_subtype_clause<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<(String, Option<String>, Vec<TypeParam>)>> {
    // Check if this is a <: operator
    let mut is_subtype_op = false;
    for child in walker.children(&node) {
        if child.kind() == "operator" && walker.text(&child) == "<:" {
            is_subtype_op = true;
            break;
        }
    }

    if !is_subtype_op {
        // Not a subtype expression
        return Ok(None);
    }

    let named = walker.named_children(&node);
    let mut name: Option<String> = None;
    let mut parent: Option<String> = None;
    let mut params: Vec<TypeParam> = Vec::new();

    for (i, child) in named.iter().enumerate() {
        match walker.kind(child) {
            NodeKind::Identifier => {
                if i == 0 || name.is_none() {
                    name = Some(walker.text(child).to_string());
                } else {
                    parent = Some(walker.text(child).to_string());
                }
            }
            NodeKind::ParametrizedTypeExpression => {
                // Subtype with parametric child or parent
                if let Ok((parsed_name, parsed_params)) =
                    parse_parametrized_abstract_head(walker, *child)
                {
                    if name.is_none() {
                        name = Some(parsed_name);
                        params = parsed_params;
                    } else {
                        parent = Some(parsed_name);
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(n) = name {
        Ok(Some((n, parent, params)))
    } else {
        Ok(None)
    }
}

/// Parse abstract type definition from raw text (fallback).
fn parse_from_text(s: &str) -> Option<(String, Option<String>)> {
    let s = s.trim();

    // Check for subtype syntax: A <: B
    if let Some(pos) = s.find("<:") {
        let name = s[..pos].trim().to_string();
        let parent = s[pos + 2..].trim().to_string();
        if !name.is_empty() && !parent.is_empty() {
            return Some((name, Some(parent)));
        } else if !name.is_empty() {
            return Some((name, None));
        }
    }

    // Simple identifier
    if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some((s.to_string(), None));
    }

    None
}
