//! Where clause and type parameter handling.
//!
//! Parses type constraints in `where` clauses and converts
//! parametric type variable bindings.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{KwParam, TypedParam};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::{JuliaType, TypeParam};

use super::signature::{parse_signature, parse_signature_call};

pub(super) fn parse_where_expression<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypedParam>, Vec<KwParam>, bool, Vec<TypeParam>)> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    // where_expression has two children: left (signature) and right (type constraints)
    if children.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("invalid where expression".to_string()),
            span,
        ));
    }

    let left = children[0];
    let right = children[children.len() - 1];

    // Parse the left side (signature) - may be nested where_expression
    let (name, params, kwparams, is_base_extension, mut type_params) = match walker.kind(&left) {
        NodeKind::WhereExpression => {
            // Chained where clause: f(x) where T where S
            parse_where_expression(walker, left)?
        }
        NodeKind::CallExpression => {
            let (n, p, kw, is_base) = parse_signature_call(walker, left)?;
            (n, p, kw, is_base, Vec::new())
        }
        NodeKind::Signature => {
            let (n, p, kw, is_base) = parse_signature(walker, left)?;
            (n, p, kw, is_base, Vec::new())
        }
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::Other(format!(
                    "unexpected node in where expression left side: {:?}",
                    walker.kind(&left)
                )),
                span,
            ));
        }
    };

    // Parse the right side (type constraints)
    let new_type_params = parse_type_constraints(walker, right)?;
    type_params.extend(new_type_params);

    // Convert parameter types to TypeVar when they match type parameters
    let params = convert_params_with_type_vars(params, &type_params);

    Ok((name, params, kwparams, is_base_extension, type_params))
}

/// Parse type constraints from the right side of a where expression.
/// Handles: `T`, `T<:Real`, `{T, S<:Number}`, etc.
pub(super) fn parse_type_constraints<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<TypeParam>> {
    let span = walker.span(&node);

    match walker.kind(&node) {
        NodeKind::Identifier => {
            // Simple unbounded: where T
            let name = walker.text(&node).to_string();
            Ok(vec![TypeParam::new(name)])
        }
        NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
            // Bounded: where T<:Real
            // tree-sitter may return either subtype_expression or binary_expression
            parse_subtype_expression(walker, node).map(|tp| vec![tp])
        }
        NodeKind::CurlyExpression | NodeKind::TypeParameterList => {
            // Multiple constraints: where {T, S<:Number}
            // TypeParameterList is from Pure Rust parser for braced where clauses
            let mut type_params = Vec::new();
            for child in walker.named_children(&node) {
                match walker.kind(&child) {
                    NodeKind::Identifier => {
                        let name = walker.text(&child).to_string();
                        type_params.push(TypeParam::new(name));
                    }
                    NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
                        // tree-sitter may return either subtype_expression or binary_expression
                        type_params.push(parse_subtype_expression(walker, child)?);
                    }
                    NodeKind::SubtypeConstraint | NodeKind::SupertypeConstraint => {
                        // Pure Rust parser constraint nodes
                        type_params.push(parse_subtype_expression(walker, child)?);
                    }
                    _ => {
                        // Skip other nodes (e.g., commas)
                    }
                }
            }
            Ok(type_params)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(format!(
                "unexpected node in where clause: {:?}",
                walker.kind(&node)
            )),
            span,
        )),
    }
}

/// Parse a type constraint expression: T<:Number (upper bound) or T>:Integer (lower bound)
pub(super) fn parse_subtype_expression<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypeParam> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);
    let kind = walker.kind(&node);

    if children.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("invalid type constraint expression".to_string()),
            span,
        ));
    }

    let name_node = children[0];
    let bound_node = children[children.len() - 1];

    let name = walker.text(&name_node).to_string();
    let bound = walker.text(&bound_node).to_string();

    // Determine constraint type based on node kind
    match kind {
        NodeKind::SupertypeConstraint => {
            // T>:Integer - lower bound (contravariant)
            Ok(TypeParam::with_lower_bound(name, bound))
        }
        _ => {
            // SubtypeConstraint, SubtypeExpression, BinaryExpression with <:
            // T<:Number - upper bound (covariant)
            Ok(TypeParam::with_upper_bound(name, bound))
        }
    }
}

/// Convert parameter types to TypeVar when they match type parameters from where clause.
/// For example, if type_params contains TypeParam { name: "T", upper_bound: Some("Real") },
/// and a parameter has type JuliaType::Struct("T"), it will be converted to
/// JuliaType::TypeVar("T", Some("Real")).
pub(super) fn convert_params_with_type_vars(
    params: Vec<TypedParam>,
    type_params: &[TypeParam],
) -> Vec<TypedParam> {
    params
        .into_iter()
        .map(|param| {
            if let Some(JuliaType::Struct(name)) = &param.type_annotation {
                // Check if this struct name matches a type parameter
                if let Some(tp) = type_params.iter().find(|tp| &tp.name == name) {
                    // Use get_upper_bound() to get the effective upper bound
                    let bound = tp.get_upper_bound().cloned();
                    let new_type = JuliaType::TypeVar(tp.name.clone(), bound);
                    return TypedParam::new(param.name, Some(new_type), param.span);
                }
            }
            param
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn s() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    // ── convert_params_with_type_vars ─────────────────────────────────────────

    #[test]
    fn test_convert_struct_matching_type_param_becomes_typevar() {
        // Param "x" typed as Struct("T"), type param "T" (no bound) → TypeVar("T", None)
        let params = vec![TypedParam::new("x".to_string(), Some(JuliaType::Struct("T".to_string())), s())];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::TypeVar(name, None)) if name == "T"),
            "Expected TypeVar(T, None), got {:?}", result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_struct_with_bound_becomes_typevar_with_bound() {
        // Param typed as Struct("T"), type param T <: Number → TypeVar("T", Some("Number"))
        let mut tp = TypeParam::new("T".to_string());
        tp.upper_bound = Some("Number".to_string());
        tp.bound = Some("Number".to_string());

        let params = vec![TypedParam::new("x".to_string(), Some(JuliaType::Struct("T".to_string())), s())];
        let type_params = vec![tp];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::TypeVar(name, Some(_))) if name == "T"),
            "Expected TypeVar(T, Some(...)), got {:?}", result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_struct_not_matching_type_param_unchanged() {
        // Param typed as Struct("Foo"), type params only contain "T" → unchanged
        let params = vec![TypedParam::new("x".to_string(), Some(JuliaType::Struct("Foo".to_string())), s())];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::Struct(name)) if name == "Foo"),
            "Expected Struct(Foo) unchanged, got {:?}", result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_non_struct_type_unchanged() {
        // Param typed as Int64 (primitive) → unchanged
        let params = vec![TypedParam::new("x".to_string(), Some(JuliaType::Int64), s())];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::Int64)),
            "Expected Int64 unchanged, got {:?}", result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_untyped_param_unchanged() {
        // Param with no type annotation → unchanged
        let params = vec![TypedParam::new("x".to_string(), None, s())];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(result[0].type_annotation.is_none(), "Expected None type annotation");
    }

    #[test]
    fn test_convert_multiple_params_only_matching_converted() {
        // Params: (x: T, y: Float64). Type param: T. Only x should be converted.
        let params = vec![
            TypedParam::new("x".to_string(), Some(JuliaType::Struct("T".to_string())), s()),
            TypedParam::new("y".to_string(), Some(JuliaType::Float64), s()),
        ];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 2);
        assert!(matches!(&result[0].type_annotation, Some(JuliaType::TypeVar(name, _)) if name == "T"),
            "x should be TypeVar, got {:?}", result[0].type_annotation);
        assert!(matches!(&result[1].type_annotation, Some(JuliaType::Float64)),
            "y should remain Float64, got {:?}", result[1].type_annotation);
    }
}
