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

    // Parse the right side (type constraints) FIRST so the `where`-clause
    // parameter names are known before the signature is parsed. A same-named
    // top-level binding (`T = Int64`, registered as a type alias) must not
    // freeze a `where T` parameter annotation to the global's concrete type:
    // the method's `where` parameter lexically shadows the global, exactly as
    // upstream Julia scopes it. Excluding these names from alias expansion for
    // the duration of signature parsing keeps `f(x::T) where T` lowering `x`'s
    // annotation as the type variable `T` rather than `Int64` (Issue #7847).
    let new_type_params = parse_type_constraints(walker, right)?;
    let excluded_names: Vec<String> = new_type_params.iter().map(|tp| tp.name.clone()).collect();
    let _exclusion = crate::lowering::type_alias::ScopedExclusion::new(&excluded_names);

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

    type_params.extend(new_type_params);

    // Convert parameter types to TypeVar when they match type parameters
    let params = convert_params_with_type_vars(params, &type_params);

    Ok((name, params, kwparams, is_base_extension, type_params))
}

/// Parse type constraints from the right side of a where expression.
/// Handles: `T`, `T<:Real`, `{T, S<:Number}`, etc.
pub(crate) fn parse_type_constraints<'a>(
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
        NodeKind::SubtypeExpression
        | NodeKind::BinaryExpression
        | NodeKind::SubtypeConstraint
        | NodeKind::SupertypeConstraint => {
            // Bounded: where T<:Real / where T>:Int / where Int<:T<:Real.
            // tree-sitter returns subtype_expression / binary_expression; the
            // pure-Rust parser returns Subtype/SupertypeConstraint (the latter
            // carries the `>:` lower-bound direction, Issue #5650).
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

/// Parse the type parameters out of a pure-parser `WhereClause` node.
///
/// A `WhereClause` (produced by `parse_where_clause` in the pure-Rust parser)
/// contains one of:
/// - `Identifier` — unbraced unbounded: `where T`
/// - `SubtypeExpression` / `BinaryExpression` / `SubtypeConstraint` /
///   `SupertypeConstraint` — unbraced bounded: `where T<:Real`
/// - `TypeParameters` — braced list: `where {T, S<:Number}`, whose children are
///   again identifiers or constraint expressions
///
/// Shared by the long form (`function *(...) where {...} ... end`) and the
/// assignment-form operator path (`*(...) where {...} = ...`). The operator
/// path previously had a divergent hand-rolled copy that did not recognize the
/// `BinaryExpression`/`SubtypeConstraint` shapes the pure parser emits for
/// braced bounds, silently dropping `where {T<:Real}` bounds (Issue #6537).
pub(crate) fn parse_where_clause_type_params<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<TypeParam>> {
    let mut type_params = Vec::new();
    for where_child in walker.named_children(&node) {
        match walker.kind(&where_child) {
            NodeKind::Identifier => {
                type_params.push(TypeParam::new(walker.text(&where_child).to_string()));
            }
            NodeKind::TypeParameters | NodeKind::TypeParameterList | NodeKind::CurlyExpression => {
                for tp_child in walker.named_children(&where_child) {
                    match walker.kind(&tp_child) {
                        NodeKind::TypeParameter => {
                            // TypeParameter contains children describing the param
                            let tp_children = walker.named_children(&tp_child);
                            if tp_children.len() >= 2 {
                                // Bounded: T<:Real
                                let param_name = walker.text(&tp_children[0]).to_string();
                                let bound =
                                    expand_where_bound(&param_name, walker.text(&tp_children[1]));
                                type_params.push(TypeParam::with_upper_bound(param_name, bound));
                            } else if !tp_children.is_empty() {
                                // Unbounded: T
                                let param_name = walker.text(&tp_children[0]).to_string();
                                type_params.push(TypeParam::new(param_name));
                            }
                        }
                        NodeKind::Identifier => {
                            type_params.push(TypeParam::new(walker.text(&tp_child).to_string()));
                        }
                        NodeKind::SubtypeExpression
                        | NodeKind::BinaryExpression
                        | NodeKind::SubtypeConstraint
                        | NodeKind::SupertypeConstraint => {
                            // The pure parser builds `T<:Bound` as a BinaryExpression
                            // `[T, <:, Bound]` whose operator is the middle named child,
                            // so the bound is the LAST child, not `children[1]` (which is
                            // the bare `<:`). Delegate to the shared parser (Issue #5374).
                            type_params.push(parse_subtype_expression(walker, tp_child)?);
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::SubtypeExpression
            | NodeKind::BinaryExpression
            | NodeKind::SubtypeConstraint
            | NodeKind::SupertypeConstraint => {
                // Unbraced `where T<:Bound`. Same operator-is-middle-child layout as
                // the braced case above (Issue #5374).
                type_params.push(parse_subtype_expression(walker, where_child)?);
            }
            _ => {}
        }
    }
    Ok(type_params)
}

/// Parse a type constraint expression:
/// - `T<:Number` - upper bound (covariant)
/// - `T>:Integer` - lower bound (contravariant)
/// - `Integer<:T<:Real` - both bounds (Issue #5051), emitted by the pure-Rust
///   parser as a `SubtypeConstraint` with three children `[name, upper, lower]`.
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

    // Double-bounded form `Lower<:T<:Upper`: children are [name, upper, lower].
    if matches!(kind, NodeKind::SubtypeConstraint) && children.len() >= 3 {
        let name = walker.text(&children[0]).to_string();
        let upper = expand_where_bound(&name, walker.text(&children[1]));
        let lower = expand_where_bound(&name, walker.text(&children[2]));
        return Ok(TypeParam::with_both_bounds(name, lower, upper));
    }

    let name_node = children[0];
    let bound_node = children[children.len() - 1];

    let name = walker.text(&name_node).to_string();
    let bound = expand_where_bound(&name, walker.text(&bound_node));

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

fn expand_where_bound(param_name: &str, bound: &str) -> String {
    crate::lowering::type_alias::expand_excluding(bound, &[param_name.to_string()])
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
            let Some(type_annotation) = param.type_annotation else {
                return param;
            };
            let converted = convert_type_with_type_vars(type_annotation, type_params);
            TypedParam {
                name: param.name,
                type_annotation: Some(converted),
                is_varargs: param.is_varargs,
                vararg_count: param.vararg_count,
                span: param.span,
            }
        })
        .collect()
}

fn convert_type_with_type_vars(ty: JuliaType, type_params: &[TypeParam]) -> JuliaType {
    match ty {
        JuliaType::Struct(name) => {
            if let Some(tp) = type_params.iter().find(|tp| tp.name == name) {
                let bound = tp.get_upper_bound().cloned();
                JuliaType::TypeVar(tp.name.clone(), bound)
            } else {
                JuliaType::Struct(name)
            }
        }
        JuliaType::TypeOf(inner) => {
            JuliaType::TypeOf(Box::new(convert_type_with_type_vars(*inner, type_params)))
        }
        JuliaType::VectorOf(elem) => {
            JuliaType::VectorOf(Box::new(convert_type_with_type_vars(*elem, type_params)))
        }
        JuliaType::MatrixOf(elem) => {
            JuliaType::MatrixOf(Box::new(convert_type_with_type_vars(*elem, type_params)))
        }
        JuliaType::TupleOf(types) => JuliaType::TupleOf(
            types
                .into_iter()
                .map(|ty| convert_type_with_type_vars(ty, type_params))
                .collect(),
        ),
        JuliaType::Union(types) => JuliaType::Union(
            types
                .into_iter()
                .map(|ty| convert_type_with_type_vars(ty, type_params))
                .collect(),
        ),
        other => other,
    }
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
        let params = vec![TypedParam::new(
            "x".to_string(),
            Some(JuliaType::Struct("T".to_string())),
            s(),
        )];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::TypeVar(name, None)) if name == "T"),
            "Expected TypeVar(T, None), got {:?}",
            result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_struct_with_bound_becomes_typevar_with_bound() {
        // Param typed as Struct("T"), type param T <: Number → TypeVar("T", Some("Number"))
        let mut tp = TypeParam::new("T".to_string());
        tp.upper_bound = Some("Number".to_string());
        tp.bound = Some("Number".to_string());

        let params = vec![TypedParam::new(
            "x".to_string(),
            Some(JuliaType::Struct("T".to_string())),
            s(),
        )];
        let type_params = vec![tp];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::TypeVar(name, Some(_))) if name == "T"),
            "Expected TypeVar(T, Some(...)), got {:?}",
            result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_typeof_struct_matching_type_param_becomes_typeof_typevar() {
        // Param typed as Type{T}, type param "T" → Type{TypeVar("T")}
        let params = vec![TypedParam::new(
            "x".to_string(),
            Some(JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "T".to_string(),
            )))),
            s(),
        )];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(
                &result[0].type_annotation,
                Some(JuliaType::TypeOf(inner))
                    if matches!(inner.as_ref(), JuliaType::TypeVar(name, None) if name == "T")
            ),
            "Expected TypeOf(TypeVar(T, None)), got {:?}",
            result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_struct_not_matching_type_param_unchanged() {
        // Param typed as Struct("Foo"), type params only contain "T" → unchanged
        let params = vec![TypedParam::new(
            "x".to_string(),
            Some(JuliaType::Struct("Foo".to_string())),
            s(),
        )];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::Struct(name)) if name == "Foo"),
            "Expected Struct(Foo) unchanged, got {:?}",
            result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_non_struct_type_unchanged() {
        // Param typed as Int64 (primitive) → unchanged
        let params = vec![TypedParam::new(
            "x".to_string(),
            Some(JuliaType::Int64),
            s(),
        )];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::Int64)),
            "Expected Int64 unchanged, got {:?}",
            result[0].type_annotation
        );
    }

    #[test]
    fn test_convert_untyped_param_unchanged() {
        // Param with no type annotation → unchanged
        let params = vec![TypedParam::new("x".to_string(), None, s())];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 1);
        assert!(
            result[0].type_annotation.is_none(),
            "Expected None type annotation"
        );
    }

    #[test]
    fn test_convert_multiple_params_only_matching_converted() {
        // Params: (x: T, y: Float64). Type param: T. Only x should be converted.
        let params = vec![
            TypedParam::new(
                "x".to_string(),
                Some(JuliaType::Struct("T".to_string())),
                s(),
            ),
            TypedParam::new("y".to_string(), Some(JuliaType::Float64), s()),
        ];
        let type_params = vec![TypeParam::new("T".to_string())];
        let result = convert_params_with_type_vars(params, &type_params);
        assert_eq!(result.len(), 2);
        assert!(
            matches!(&result[0].type_annotation, Some(JuliaType::TypeVar(name, _)) if name == "T"),
            "x should be TypeVar, got {:?}",
            result[0].type_annotation
        );
        assert!(
            matches!(&result[1].type_annotation, Some(JuliaType::Float64)),
            "y should remain Float64, got {:?}",
            result[1].type_annotation
        );
    }
}
