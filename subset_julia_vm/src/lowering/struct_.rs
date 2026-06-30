//! Lowering for struct definitions.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, InnerConstructor, StructDef, StructField, TypedParam};
use crate::lowering::expr::lower_expr;
use crate::lowering::function::where_clause::parse_where_clause_type_params;
use crate::lowering::stmt;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::{JuliaType, TypeExpr, TypeParam};

/// Lower a struct definition node to StructDef IR.
pub fn lower_struct_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<StructDef> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut type_params: Vec<TypeParam> = Vec::new();
    let mut parent_type: Option<String> = None;
    let mut is_mutable = false;
    let mut block_node: Option<Node<'a>> = None;

    // Check if this is a mutable struct (from node kind or text)
    if walker.kind(&node) == NodeKind::MutableStructDefinition {
        is_mutable = true;
    } else {
        let text = walker.text(&node);
        if text.starts_with("mutable") {
            is_mutable = true;
        }
    }

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                } else if parent_type.is_none() {
                    // Second Identifier is the parent type (Pure Rust parser)
                    // e.g., struct Complex{T<:Real} <: Number -> "Number" is parent
                    parent_type = Some(walker.text(&child).to_string());
                }
            }
            NodeKind::ParametrizedTypeExpression => {
                if name.is_none() {
                    // First ParametrizedTypeExpression: this is the struct itself
                    // e.g., struct Complex{T<:Real} ... -> "Complex" with params [T]
                    let (parsed_name, params) = parse_parametrized_type_head(walker, child)?;
                    name = Some(parsed_name);
                    type_params = params;
                } else if parent_type.is_none() {
                    // Second ParametrizedTypeExpression: this is the parent type (Issue #2523)
                    // e.g., struct IntBox <: Container{Int64} -> parent is "Container{Int64}"
                    let full_parent = walker.text(&child).trim().to_string();
                    parent_type = Some(full_parent);
                }
            }
            NodeKind::TypeParameters => {
                // Pure Rust parser: TypeParameters contains type params directly
                // e.g., {T<:Real} -> TypeParameters with TypeParameter children
                type_params = parse_type_parameters(walker, child)?;
            }
            NodeKind::BinaryExpression | NodeKind::SubtypeExpression => {
                if let Some(result) = try_parse_struct_subtype(walker, child)? {
                    name = Some(result.struct_name);
                    type_params = result.type_params;
                    parent_type = result.parent_name;
                }
            }
            NodeKind::TypeHead => {
                for type_child in walker.named_children(&child) {
                    match walker.kind(&type_child) {
                        NodeKind::Identifier => {
                            if name.is_none() {
                                name = Some(walker.text(&type_child).to_string());
                            }
                        }
                        NodeKind::ParametrizedTypeExpression => {
                            if name.is_none() {
                                let (parsed_name, params) =
                                    parse_parametrized_type_head(walker, type_child)?;
                                name = Some(parsed_name);
                                type_params = params;
                            } else if parent_type.is_none() {
                                // Parent is parametric type (Issue #2523)
                                let full_parent = walker.text(&type_child).trim().to_string();
                                parent_type = Some(full_parent);
                            }
                        }
                        NodeKind::BinaryExpression | NodeKind::SubtypeExpression => {
                            if let Some(result) = try_parse_struct_subtype(walker, type_child)? {
                                name = Some(result.struct_name);
                                type_params = result.type_params;
                                parent_type = result.parent_name;
                            }
                        }
                        _ => {
                            if name.is_none() {
                                let text = walker.text(&type_child).trim();
                                if !text.is_empty() {
                                    if let Some((n, p)) = parse_subtype_from_text(text) {
                                        name = Some(n);
                                        parent_type = p;
                                    } else {
                                        name = Some(text.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            NodeKind::Block => {
                block_node = Some(child);
            }
            _ => {
                if name.is_none() {
                    let first_named = walker.named_children(&child).first().cloned();
                    if let Some(first) = first_named {
                        if matches!(walker.kind(&first), NodeKind::Identifier) {
                            name = Some(walker.text(&first).to_string());
                        }
                    } else {
                        let child_text = walker.text(&child).trim();
                        if !child_text.is_empty() && !child_text.contains("::") {
                            name = Some(child_text.to_string());
                        }
                    }
                }
            }
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing struct name".to_string()),
            span,
        )
    })?;

    let (fields, inner_constructors) = if let Some(block) = block_node {
        parse_struct_body(walker, block, &name, &type_params)?
    } else {
        (vec![], vec![])
    };

    // Issue #7235 (sub-case 1): a struct may declare its supertype through a
    // `const` type alias (`const CUD = Dist{Uni,Cont}; struct Norm <: CUD`).
    // Upstream resolves the alias to the underlying parametric type when the
    // subtype relation is recorded; mirror that here by expanding the parent
    // name against the alias table registered during the pre-scan. Without this
    // the recorded parent stays the bare alias name (`CUD`), so the hierarchy
    // chain walk cannot follow `Norm -> Dist` and `Norm <: Dist` / `isa Dist`
    // (and dispatch on `::Dist`) all fail.
    //
    // Issue #7840: a struct's own declared type parameters are lexically scoped
    // to the struct, so they must shadow any same-named top-level global/alias
    // when lowering the declared parent. Without excluding them, a global like
    // `T = Int64` would substitute its *value* into the parametric parent
    // template (`AbstractVector{T}` frozen to `AbstractVector{Int64}`),
    // corrupting the subtype relation (`Wrap{Float64} <: AbstractVector{Float64}`
    // wrongly false). Exclude the struct's own param names from substitution.
    let own_param_names: Vec<String> = type_params.iter().map(|p| p.name.clone()).collect();
    let parent_type =
        parent_type.map(|p| crate::lowering::type_alias::expand_excluding(&p, &own_param_names));

    Ok(StructDef {
        name,
        is_mutable,
        type_params,
        parent_type,
        fields,
        inner_constructors,
        span,
    })
}

/// Result of parsing a subtype expression like `Complex{T<:Real} <: Number`
struct SubtypeParseResult {
    struct_name: String,
    type_params: Vec<TypeParam>,
    parent_name: Option<String>,
}

fn try_parse_struct_subtype<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<SubtypeParseResult>> {
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
    let mut struct_name: Option<String> = None;
    let mut type_params: Vec<TypeParam> = Vec::new();
    let mut parent_name: Option<String> = None;

    for (i, child) in named.iter().enumerate() {
        match walker.kind(child) {
            NodeKind::Identifier => {
                if i == 0 || struct_name.is_none() {
                    struct_name = Some(walker.text(child).to_string());
                } else {
                    parent_name = Some(walker.text(child).to_string());
                }
            }
            NodeKind::ParametrizedTypeExpression => {
                if let Ok((parsed_name, params)) = parse_parametrized_type_head(walker, *child) {
                    if struct_name.is_none() {
                        struct_name = Some(parsed_name);
                        type_params = params;
                    } else {
                        // Parent type: preserve full parametric name (Issue #2523)
                        // e.g., Container{Int64} not just "Container"
                        let full_parent = walker.text(child).trim().to_string();
                        parent_name = Some(full_parent);
                        let _ = params; // params belong to the parent, not the struct
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(n) = struct_name {
        Ok(Some(SubtypeParseResult {
            struct_name: n,
            type_params,
            parent_name,
        }))
    } else {
        Ok(None)
    }
}

fn parse_subtype_from_text(s: &str) -> Option<(String, Option<String>)> {
    let s = s.trim();
    if let Some(pos) = s.find("<:") {
        let name = s[..pos].trim().to_string();
        let parent = s[pos + 2..].trim().to_string();
        if !name.is_empty() && !parent.is_empty() {
            return Some((name, Some(parent)));
        } else if !name.is_empty() {
            return Some((name, None));
        }
    }
    None
}

fn parse_parametrized_type_head<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypeParam>)> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut type_params: Vec<TypeParam> = Vec::new();

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier if name.is_none() => {
                name = Some(walker.text(&child).to_string());
            }
            NodeKind::CurlyExpression => {
                type_params = parse_curly_type_params(walker, child)?;
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing struct name in parametrized type".to_string()),
            span,
        )
    })?;

    Ok((name, type_params))
}

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
            NodeKind::SubtypeExpression => {
                let param = parse_subtype_param(walker, child)?;
                params.push(param);
            }
            NodeKind::BinaryExpression => {
                let param = parse_subtype_from_binary(walker, child)?;
                if let Some(p) = param {
                    params.push(p);
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

fn parse_subtype_from_binary<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<TypeParam>> {
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

fn parse_subtype_param<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<TypeParam> {
    let span = walker.span(&node);
    let mut param_name: Option<String> = None;
    let mut bound: Option<String> = None;

    let children = walker.named_children(&node);

    for (i, child) in children.iter().enumerate() {
        if matches!(walker.kind(child), NodeKind::Identifier) {
            let name = walker.text(child);
            if i == 0 || param_name.is_none() {
                param_name = Some(name.to_string());
            } else {
                // Store bound as string to support user-defined abstract types
                bound = Some(name.to_string());
            }
        }
    }

    let param_name = param_name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(
                "missing type parameter name in subtype expression".to_string(),
            ),
            span,
        )
    })?;

    match bound {
        Some(b) => Ok(TypeParam::with_bound(param_name, b)),
        None => Ok(TypeParam::new(param_name)),
    }
}

/// Parse struct body: fields and inner constructors.
fn parse_struct_body<'a>(
    walker: &CstWalker<'a>,
    block_node: Node<'a>,
    struct_name: &str,
    type_params: &[TypeParam],
) -> LowerResult<(Vec<StructField>, Vec<InnerConstructor>)> {
    let mut fields = Vec::new();
    let mut inner_constructors = Vec::new();

    for child in walker.named_children(&block_node) {
        match walker.kind(&child) {
            NodeKind::TypedExpression | NodeKind::TypedParameter => {
                let field = parse_typed_field(walker, child, type_params)?;
                fields.push(field);
            }
            NodeKind::Identifier => {
                let span = walker.span(&child);
                fields.push(StructField {
                    name: walker.text(&child).to_string(),
                    type_expr: None,
                    span,
                });
            }
            NodeKind::FunctionDefinition => {
                if let Some(ctor) = try_parse_inner_constructor(walker, child, struct_name)? {
                    inner_constructors.push(ctor);
                }
            }
            NodeKind::Assignment => {
                if let Some(ctor) = try_parse_short_constructor(walker, child, struct_name)? {
                    inner_constructors.push(ctor);
                } else {
                    let named = walker.named_children(&child);
                    if let Some(first) = named.first() {
                        match walker.kind(first) {
                            NodeKind::TypedExpression | NodeKind::TypedParameter => {
                                let field = parse_typed_field(walker, *first, type_params)?;
                                fields.push(field);
                            }
                            NodeKind::Identifier => {
                                let span = walker.span(first);
                                fields.push(StructField {
                                    name: walker.text(first).to_string(),
                                    type_expr: None,
                                    span,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok((fields, inner_constructors))
}

/// Try to parse a long-form inner constructor: function Point(x, y) ... end
fn try_parse_inner_constructor<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    struct_name: &str,
) -> LowerResult<Option<InnerConstructor>> {
    let span = walker.span(&node);
    let mut name: Option<String> = None;
    let mut params: Vec<TypedParam> = Vec::new();
    let mut type_params: Vec<TypeParam> = Vec::new();
    let mut body: Option<Block> = None;

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier if name.is_none() => {
                name = Some(walker.text(&child).to_string());
            }
            NodeKind::ParametrizedTypeExpression if name.is_none() => {
                // Handle parametric constructor name like Rational{T}
                name = Some(walker.text(&child).to_string());
            }
            NodeKind::TypeParameters => {
                // Handle type parameters from Pure Rust parser: {T} after function name
                // Just append to name to form "Rational{T}" - type params are extracted from WhereClause
                if let Some(ref mut n) = name {
                    n.push_str(walker.text(&child));
                }
            }
            NodeKind::ParameterList => {
                // Handle parameter list when it's a direct child of function definition
                // This happens with Pure Rust parser for typed signatures
                for param in walker.named_children(&child) {
                    match walker.kind(&param) {
                        NodeKind::Identifier => {
                            params.push(TypedParam {
                                name: walker.text(&param).to_string(),
                                type_annotation: None,
                                is_varargs: false,
                                vararg_count: None,
                                span: walker.span(&param),
                            });
                        }
                        NodeKind::TypedParameter
                        | NodeKind::TypedExpression
                        | NodeKind::Parameter => {
                            let (param_name, param_type) = parse_param_type(walker, param)?;
                            params.push(TypedParam {
                                name: param_name,
                                type_annotation: param_type,
                                is_varargs: false,
                                vararg_count: None,
                                span: walker.span(&param),
                            });
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::WhereClause => {
                // Handle where clause from Pure Rust parser via the shared
                // helper (Issue #6537). The previous hand-rolled copy read the
                // bound of a `BinaryExpression [T, <:, Real]` from `children[1]`
                // — the bare `<:` operator (the #5374 bug pattern) — and did
                // not recognize the `SubtypeConstraint` nodes the parser now
                // emits for unbraced constraints.
                type_params.extend(parse_where_clause_type_params(walker, child)?);
            }
            NodeKind::CallExpression => {
                let (sig_name, sig_params) = parse_ctor_signature(walker, child)?;
                name = Some(sig_name);
                params = sig_params;
            }
            NodeKind::Signature => {
                // Signature may contain a WhereExpression - check for it
                let sig_children = walker.named_children(&child);
                let has_where = sig_children
                    .iter()
                    .any(|c| walker.kind(c) == NodeKind::WhereExpression);
                if has_where {
                    // Find the WhereExpression child and parse it
                    if let Some(where_child) = sig_children
                        .iter()
                        .find(|c| walker.kind(c) == NodeKind::WhereExpression)
                    {
                        let (sig_name, sig_params, sig_type_params) =
                            parse_ctor_where_expression(walker, *where_child)?;
                        name = Some(sig_name);
                        params = sig_params;
                        type_params = sig_type_params;
                    }
                } else {
                    // No where clause - use simple signature parsing
                    let (sig_name, sig_params) = parse_ctor_signature(walker, child)?;
                    name = Some(sig_name);
                    params = sig_params;
                }
            }
            NodeKind::WhereExpression => {
                // Handle where clause: function Rational{T}(num, den) where T <: Integer
                let (sig_name, sig_params, sig_type_params) =
                    parse_ctor_where_expression(walker, child)?;
                name = Some(sig_name);
                params = sig_params;
                type_params = sig_type_params;
            }
            NodeKind::Block => {
                body = Some(stmt::lower_block(walker, child)?);
            }
            _ => {}
        }
    }

    let func_name = name.unwrap_or_default();
    // Strip type parameters for comparison: Rational{T} should match Rational
    let base_func_name = if let Some(idx) = func_name.find('{') {
        &func_name[..idx]
    } else {
        &func_name
    };
    if base_func_name != struct_name {
        return Ok(None);
    }

    let body = body.unwrap_or(Block {
        stmts: vec![],
        span,
    });

    Ok(Some(InnerConstructor {
        params,
        kwparams: vec![],
        type_params,
        body,
        span,
    }))
}

/// Parse a where expression in constructor signature, extracting type parameters.
fn parse_ctor_where_expression<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypedParam>, Vec<TypeParam>)> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    if children.len() < 2 {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("invalid where expression in constructor".to_string()),
            span,
        ));
    }

    let left = children[0];
    let right = children[children.len() - 1];

    // Parse the left side (signature) - may be nested where_expression
    let (name, params, mut type_params) = match walker.kind(&left) {
        NodeKind::WhereExpression => {
            // Chained where clause: f(x) where T where S
            parse_ctor_where_expression(walker, left)?
        }
        NodeKind::CallExpression | NodeKind::Signature => {
            let (n, p) = parse_ctor_signature(walker, left)?;
            (n, p, Vec::new())
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
    let new_type_params = parse_ctor_type_constraints(walker, right)?;
    type_params.extend(new_type_params);

    Ok((name, params, type_params))
}

/// Parse type constraints from the right side of a where expression.
fn parse_ctor_type_constraints<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<TypeParam>> {
    match walker.kind(&node) {
        NodeKind::Identifier => {
            // Simple unbounded: where T
            let name = walker.text(&node).to_string();
            Ok(vec![TypeParam::new(name)])
        }
        NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
            // Bounded: where T<:Real
            let children = walker.named_children(&node);
            if children.len() >= 2 {
                let name = walker.text(&children[0]).to_string();
                let bound = walker.text(&children[1]).to_string();
                Ok(vec![TypeParam::with_bound(name, bound)])
            } else {
                let name = walker.text(&node).to_string();
                Ok(vec![TypeParam::new(name)])
            }
        }
        NodeKind::CurlyExpression | NodeKind::TypeParameterList | NodeKind::TypeParameters => {
            // Multiple constraints: where {T, S<:Number}. The short-form
            // `where` clause surfaces these as a `TypeParameterList`
            // (Issue #5059); recurse into each constraint child.
            let mut type_params = Vec::new();
            for child in walker.named_children(&node) {
                let child_params = parse_ctor_type_constraints(walker, child)?;
                type_params.extend(child_params);
            }
            Ok(type_params)
        }
        NodeKind::TypeParameter => {
            // A single `where {T}` / `where {T<:Real}` constraint child.
            let children = walker.named_children(&node);
            if children.len() >= 2 {
                let name = walker.text(&children[0]).to_string();
                let bound = walker.text(&children[1]).to_string();
                Ok(vec![TypeParam::with_bound(name, bound)])
            } else if let Some(first) = children.first() {
                let name = walker.text(first).to_string();
                Ok(vec![TypeParam::new(name)])
            } else {
                let name = walker.text(&node).to_string();
                Ok(vec![TypeParam::new(name)])
            }
        }
        _ => {
            // Unknown node - try to treat as identifier
            let name = walker.text(&node).to_string();
            Ok(vec![TypeParam::new(name)])
        }
    }
}

/// Try to parse a short-form inner constructor: Point(x, y) = new(x, y)
fn try_parse_short_constructor<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    struct_name: &str,
) -> LowerResult<Option<InnerConstructor>> {
    let span = walker.span(&node);
    // Filter out operator nodes from named children
    let named: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if named.len() < 2 {
        return Ok(None);
    }

    let lhs = named[0];
    let rhs = named[1];

    // The LHS is either a plain call (`Point(x, y) = ...`) or a `where`
    // expression wrapping the call for parametric inner constructors
    // (`Foo{T}(x) where T = new{T}(x)`). The latter carries the type
    // parameters that the body's `new{T}(...)` needs in order to build the
    // correctly instantiated, field-ordered struct (Issue #5059).
    let (sig_name, params, type_params) = match walker.kind(&lhs) {
        NodeKind::CallExpression => {
            let (sig_name, params) = parse_ctor_signature(walker, lhs)?;
            (sig_name, params, Vec::new())
        }
        NodeKind::WhereExpression => parse_ctor_where_expression(walker, lhs)?,
        _ => return Ok(None),
    };

    // Strip type parameters for comparison: `Foo{T}` should match `Foo`.
    let base_sig_name = match sig_name.find('{') {
        Some(idx) => &sig_name[..idx],
        None => &sig_name,
    };
    if base_sig_name != struct_name {
        return Ok(None);
    }

    let rhs_expr = lower_expr(walker, rhs)?;
    let body = Block {
        stmts: vec![crate::ir::core::Stmt::Return {
            value: Some(rhs_expr),
            span: walker.span(&rhs),
        }],
        span: walker.span(&rhs),
    };

    Ok(Some(InnerConstructor {
        params,
        kwparams: vec![],
        type_params,
        body,
        span,
    }))
}

fn parse_ctor_signature<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypedParam>)> {
    let span = walker.span(&node);
    let mut named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty constructor signature".to_string()),
            span,
        ));
    }

    // Drill down through Signature, WhereExpression to find CallExpression
    let mut current = named[0];
    loop {
        match walker.kind(&current) {
            NodeKind::Signature | NodeKind::WhereExpression => {
                let children = walker.named_children(&current);
                if children.is_empty() {
                    break;
                }
                current = children[0];
            }
            NodeKind::CallExpression => {
                named = walker.named_children(&current);
                break;
            }
            _ => break,
        }
    }

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty call expression in signature".to_string()),
            span,
        ));
    }

    // Find the callee - may be Identifier, ParametrizedTypeExpression, etc.
    let callee = named[0];
    let name = walker.text(&callee).to_string();

    let mut params = Vec::new();

    for arg in named.iter().skip(1) {
        match walker.kind(arg) {
            NodeKind::ArgumentList => {
                let arg_children = walker.named_children(arg);
                for param in arg_children {
                    match walker.kind(&param) {
                        NodeKind::Identifier => {
                            params.push(TypedParam {
                                name: walker.text(&param).to_string(),
                                type_annotation: None,
                                is_varargs: false,
                                vararg_count: None,
                                span: walker.span(&param),
                            });
                        }
                        NodeKind::TypedParameter | NodeKind::TypedExpression => {
                            let (param_name, param_type) = parse_param_type(walker, param)?;
                            params.push(TypedParam {
                                name: param_name,
                                type_annotation: param_type,
                                is_varargs: false,
                                vararg_count: None,
                                span: walker.span(&param),
                            });
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::Identifier => {
                params.push(TypedParam {
                    name: walker.text(arg).to_string(),
                    type_annotation: None,
                    is_varargs: false,
                    vararg_count: None,
                    span: walker.span(arg),
                });
            }
            NodeKind::TypedParameter | NodeKind::TypedExpression => {
                let (param_name, param_type) = parse_param_type(walker, *arg)?;
                params.push(TypedParam {
                    name: param_name,
                    type_annotation: param_type,
                    is_varargs: false,
                    vararg_count: None,
                    span: walker.span(arg),
                });
            }
            _ => {}
        }
    }

    Ok((name, params))
}

fn parse_param_type<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Option<JuliaType>)> {
    let named = walker.named_children(&node);
    let mut name = String::new();
    let mut type_annotation = None;

    for child in named {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_empty() {
                    name = walker.text(&child).to_string();
                } else {
                    type_annotation = JuliaType::from_name(walker.text(&child));
                }
            }
            NodeKind::TypeClause => {
                for type_child in walker.named_children(&child) {
                    if walker.kind(&type_child) == NodeKind::Identifier {
                        type_annotation = JuliaType::from_name(walker.text(&type_child));
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        let text = walker.text(&node);
        if let Some(pos) = text.find("::") {
            name = text[..pos].trim().to_string();
            let type_name = text[pos + 2..].trim();
            type_annotation = JuliaType::from_name(type_name);
        } else {
            name = text.to_string();
        }
    }

    Ok((name, type_annotation))
}

fn parse_type_expr_from_node<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    type_params: &[TypeParam],
) -> TypeExpr {
    match walker.kind(&node) {
        NodeKind::Identifier => TypeExpr::from_name(walker.text(&node), type_params),
        NodeKind::ParametrizedTypeExpression => {
            parse_parameterized_type_expr(walker, node, type_params)
        }
        NodeKind::CurlyExpression => {
            let text = walker.text(&node);
            TypeExpr::TypeVar(text.to_string())
        }
        NodeKind::CallExpression => {
            // Runtime expression like Symbol(s) - needs to be evaluated at runtime
            let text = walker.text(&node).to_string();
            TypeExpr::RuntimeExpr(text)
        }
        _ => {
            let text = walker.text(&node).trim();
            // Check if this looks like a function call (contains parentheses not part of curly)
            if text.contains('(') && !text.starts_with('{') {
                return TypeExpr::RuntimeExpr(text.to_string());
            }
            if text.contains('{') {
                if let Some(parsed) = parse_type_expr_from_text(text, type_params) {
                    return parsed;
                }
            }
            TypeExpr::from_name(text, type_params)
        }
    }
}

fn parse_parameterized_type_expr<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    type_params: &[TypeParam],
) -> TypeExpr {
    let mut base_name: Option<String> = None;
    let mut params: Vec<TypeExpr> = Vec::new();

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier if base_name.is_none() => {
                base_name = Some(walker.text(&child).to_string());
            }
            NodeKind::Identifier => {
                params.push(parse_type_expr_from_node(walker, child, type_params));
            }
            NodeKind::CurlyExpression => {
                for param_child in walker.named_children(&child) {
                    params.push(parse_type_expr_from_node(walker, param_child, type_params));
                }
            }
            NodeKind::ParametrizedTypeExpression => {
                params.push(parse_type_expr_from_node(walker, child, type_params));
            }
            _ => {
                params.push(parse_type_expr_from_node(walker, child, type_params));
            }
        }
    }

    match base_name {
        Some(base) => TypeExpr::Parameterized { base, params },
        None => TypeExpr::TypeVar(walker.text(&node).to_string()),
    }
}

fn parse_type_expr_from_text(s: &str, type_params: &[TypeParam]) -> Option<TypeExpr> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    if let Some(open) = s.find('{') {
        let close = s.rfind('}')?;
        if close <= open {
            return None;
        }

        let base = s[..open].trim().to_string();
        let args_str = &s[open + 1..close];

        let args = parse_type_args_from_text(args_str, type_params)?;

        Some(TypeExpr::Parameterized { base, params: args })
    } else {
        Some(TypeExpr::from_name(s, type_params))
    }
}

fn parse_type_args_from_text(s: &str, type_params: &[TypeParam]) -> Option<Vec<TypeExpr>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in s.chars() {
        match c {
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    args.push(parse_type_expr_from_text(trimmed, type_params)?);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        args.push(parse_type_expr_from_text(trimmed, type_params)?);
    }

    Some(args)
}

fn parse_typed_field<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    type_params: &[TypeParam],
) -> LowerResult<StructField> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    let mut name: Option<String> = None;
    let mut type_expr: Option<TypeExpr> = None;

    for child in named {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                } else {
                    type_expr = Some(parse_type_expr_from_node(walker, child, type_params));
                }
            }
            NodeKind::TypeClause => {
                if let Some(type_child) = walker.named_children(&child).first() {
                    type_expr = Some(parse_type_expr_from_node(walker, *type_child, type_params));
                }
            }
            NodeKind::ParametrizedTypeExpression => {
                type_expr = Some(parse_type_expr_from_node(walker, child, type_params));
            }
            _ => {
                let type_name = walker.text(&child);
                if !type_name.is_empty() {
                    type_expr = Some(parse_type_expr_from_node(walker, child, type_params));
                }
            }
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing field name in typed field".to_string()),
            span,
        )
    })?;

    Ok(StructField {
        name,
        type_expr,
        span,
    })
}

/// Parse TypeParameters node (Pure Rust parser) to extract type params.
/// e.g., {T<:Real} -> [TypeParam::with_bound("T", "Real")]
fn parse_type_parameters<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Vec<TypeParam>> {
    let mut type_params = Vec::new();

    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::TypeParameter => {
                // TypeParameter contains children: Identifier (name) and optionally Identifier (bound)
                let children = walker.named_children(&child);
                if children.len() >= 2 {
                    // Bounded: T<:Real
                    let param_name = walker.text(&children[0]).to_string();
                    let bound = walker.text(&children[1]).to_string();
                    type_params.push(TypeParam::with_bound(param_name, bound));
                } else if !children.is_empty() {
                    // Unbounded: T
                    let param_name = walker.text(&children[0]).to_string();
                    type_params.push(TypeParam::new(param_name));
                }
            }
            NodeKind::Identifier => {
                // Unbounded type param: T
                let param_name = walker.text(&child).to_string();
                type_params.push(TypeParam::new(param_name));
            }
            NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
                // Bounded type param: T<:Real as expression
                let children = walker.named_children(&child);
                if children.len() >= 2 {
                    let param_name = walker.text(&children[0]).to_string();
                    let bound = walker.text(&children[1]).to_string();
                    type_params.push(TypeParam::with_bound(param_name, bound));
                }
            }
            _ => {}
        }
    }

    Ok(type_params)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── inner constructor where-clause bounds (Issue #6537) ──────────────────

    fn parse_struct(source: &str) -> StructDef {
        let mut parser = crate::parser::Parser::new().expect("Failed to init parser");
        let parse_outcome = parser.parse(source).expect("Failed to parse");
        let mut lowering = crate::lowering::Lowering::new(source);
        let program = lowering.lower(parse_outcome).expect("Failed to lower");
        assert!(!program.structs.is_empty(), "No struct definition found");
        program.structs[0].clone()
    }

    #[test]
    fn test_inner_ctor_unbraced_where_bound_recorded() {
        // The previous hand-rolled WhereClause copy read the bound of the
        // pure parser's `BinaryExpression [T, <:, Real]` from `children[1]`
        // (the bare `<:` operator); after the parser switched unbraced
        // constraints to SubtypeConstraint nodes it would have been dropped
        // entirely. Shared helper records the real bound (Issue #6537).
        let s = parse_struct(
            "struct Pos{T}\n    v::T\n    function Pos{T}(v) where T<:Real\n        new(v)\n    end\nend",
        );
        assert_eq!(s.inner_constructors.len(), 1);
        let tps = &s.inner_constructors[0].type_params;
        assert_eq!(tps.len(), 1, "expected one ctor type param, got {tps:?}");
        assert_eq!(tps[0].name, "T");
        assert_eq!(
            tps[0].get_upper_bound().map(String::as_str),
            Some("Real"),
            "inner ctor unbraced where bound must be the type name: {:?}",
            tps[0]
        );
        assert_eq!(tps[0].bound.as_deref(), Some("Real"));
    }

    #[test]
    fn test_inner_ctor_braced_where_bound_recorded() {
        let s = parse_struct(
            "struct Pos{T}\n    v::T\n    function Pos{T}(v) where {T<:Real}\n        new(v)\n    end\nend",
        );
        assert_eq!(s.inner_constructors.len(), 1);
        let tps = &s.inner_constructors[0].type_params;
        assert_eq!(tps.len(), 1, "expected one ctor type param, got {tps:?}");
        assert_eq!(tps[0].name, "T");
        assert_eq!(
            tps[0].get_upper_bound().map(String::as_str),
            Some("Real"),
            "inner ctor braced where bound must be the type name, not `<:`: {:?}",
            tps[0]
        );
    }

    // ── parse_subtype_from_text ───────────────────────────────────────────────

    #[test]
    fn test_parse_subtype_from_text_simple() {
        // "Foo <: Bar" → (Foo, Some(Bar))
        let result = parse_subtype_from_text("Foo <: Bar");
        assert_eq!(result, Some(("Foo".to_string(), Some("Bar".to_string()))));
    }

    #[test]
    fn test_parse_subtype_from_text_with_whitespace() {
        assert_eq!(
            parse_subtype_from_text("  MyType  <:  AbstractType  "),
            Some(("MyType".to_string(), Some("AbstractType".to_string())))
        );
    }

    #[test]
    fn test_parse_subtype_from_text_no_subtype_returns_none() {
        // No "<:" → None
        assert!(parse_subtype_from_text("Foo").is_none());
        assert!(parse_subtype_from_text("").is_none());
    }

    #[test]
    fn test_parse_subtype_from_text_empty_name_returns_none() {
        // "<:Bar" — empty name before <: → None
        assert!(parse_subtype_from_text("<: Bar").is_none());
    }

    // ── parse_type_expr_from_text ─────────────────────────────────────────────

    #[test]
    fn test_parse_type_expr_from_text_empty_returns_none() {
        assert!(parse_type_expr_from_text("", &[]).is_none());
        assert!(parse_type_expr_from_text("   ", &[]).is_none());
    }

    #[test]
    fn test_parse_type_expr_from_text_concrete_type() {
        // Known Julia type "Float64" → Concrete
        let result = parse_type_expr_from_text("Float64", &[]);
        assert!(
            matches!(&result, Some(TypeExpr::Concrete(_))),
            "Expected Concrete, got {:?}",
            result
        );
    }

    #[test]
    fn test_parse_type_expr_from_text_type_param_becomes_typevar() {
        // "T" is in type_params → TypeVar("T")
        let tp = TypeParam::new("T".to_string());
        let result = parse_type_expr_from_text("T", &[tp]);
        assert!(
            matches!(&result, Some(TypeExpr::TypeVar(name)) if name == "T"),
            "Expected TypeVar(T), got {:?}",
            result
        );
    }

    #[test]
    fn test_parse_type_expr_from_text_unknown_becomes_typevar() {
        // "Foo" not a known type and not in type_params → TypeVar("Foo")
        let result = parse_type_expr_from_text("Foo", &[]);
        assert!(
            matches!(&result, Some(TypeExpr::TypeVar(name)) if name == "Foo"),
            "Expected TypeVar(Foo), got {:?}",
            result
        );
    }

    #[test]
    fn test_parse_type_expr_from_text_parameterized() {
        // "Array{Float64}" → Parameterized { base: "Array", params: [Concrete(Float64)] }
        let result = parse_type_expr_from_text("Array{Float64}", &[]);
        assert!(
            matches!(&result, Some(TypeExpr::Parameterized { base, .. }) if base == "Array"),
            "Expected Parameterized(Array, ...), got {:?}",
            result
        );
    }

    #[test]
    fn test_parse_type_expr_from_text_unclosed_brace_returns_none() {
        // "Array{" has no closing brace → None
        assert!(parse_type_expr_from_text("Array{", &[]).is_none());
    }
}
