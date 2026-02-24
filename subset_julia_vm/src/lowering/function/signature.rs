//! Function signature and parameter parsing.
//!
//! Parses function signatures, typed parameters, keyword parameters,
//! splat parameters, and type name resolution.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, KwParam, Literal, TypedParam};
use crate::lowering::expr::lower_expr;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::{JuliaType, TypeParam};

use super::where_clause::parse_where_expression;

pub(super) fn parse_signature_call<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypedParam>, Vec<KwParam>, bool)> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);
    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty function signature".to_string()),
            span,
        ));
    }

    let callee = named[0];
    let (name, is_base_extension) = match walker.kind(&callee) {
        NodeKind::Identifier => (walker.text(&callee).to_string(), false),
        NodeKind::Operator => {
            // Allow operator overloading: function +(a, b) ... end
            (walker.text(&callee).to_string(), false)
        }
        NodeKind::ParametrizedTypeExpression => {
            // Parametric constructor: function Complex{Float64}(x, y) ... end
            (walker.text(&callee).to_string(), false)
        }
        NodeKind::FieldExpression => {
            // Handle Base.:+ syntax: function Base.:+(a, b) ... end
            // Also handle Base.show syntax: Base.show(io, x) = ...
            let children = walker.named_children(&callee);
            if children.len() >= 2 {
                let module = walker.text(&children[0]);
                let field_text = walker.text(&children[1]);
                // Check if it's Base module
                if module == "Base" {
                    // The field can be an operator node, a quote_expression, or a plain identifier
                    // For :+, the text might be just "+" or ":+"
                    // For :(==), the text might be ":(==)" or "(==)"
                    // For show, the text is just "show"
                    let mut op_name = if let Some(stripped) = field_text.strip_prefix(':') {
                        stripped.to_string()
                    } else {
                        field_text.to_string()
                    };
                    // Strip parentheses if present (e.g., "(==)" -> "==")
                    if op_name.starts_with('(') && op_name.ends_with(')') {
                        op_name = op_name[1..op_name.len() - 1].to_string();
                    }
                    (op_name, true)
                } else {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::Other(format!(
                            "only Base module supported for operator extension, got {}",
                            module
                        )),
                        walker.span(&callee),
                    ));
                }
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::Other(
                        "invalid field expression in function signature".to_string(),
                    ),
                    walker.span(&callee),
                ));
            }
        }
        NodeKind::ParenthesizedExpression => {
            // Callable struct syntax: (::Type)(args) = body
            // The parenthesized expression contains a UnaryTypedExpression
            let inner_children = walker.named_children(&callee);
            if inner_children.len() == 1 {
                let inner = inner_children[0];
                match walker.kind(&inner) {
                    NodeKind::UnaryTypedExpression => {
                        // Extract type name from ::Type
                        let type_children = walker.named_children(&inner);
                        if !type_children.is_empty() {
                            let type_name = walker.text(&type_children[0]).to_string();
                            // Use __callable_<TypeName> as the function name for callable struct dispatch
                            (format!("__callable_{}", type_name), false)
                        } else {
                            // Fallback: use the text content
                            let type_text = walker.text(&inner);
                            let type_name = type_text.strip_prefix("::").unwrap_or(type_text);
                            (format!("__callable_{}", type_name), false)
                        }
                    }
                    _ => {
                        return Err(UnsupportedFeature::new(
                            UnsupportedFeatureKind::Other(format!(
                                "expected ::Type in callable struct definition, got {:?}",
                                walker.kind(&inner)
                            )),
                            walker.span(&callee),
                        ))
                    }
                }
            } else {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::Other(
                        "invalid parenthesized expression in function signature".to_string(),
                    ),
                    walker.span(&callee),
                ));
            }
        }
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::Other("unsupported function signature".to_string()),
                walker.span(&callee),
            ))
        }
    };

    let args_node = named.iter().skip(1).find(|n| {
        matches!(
            walker.kind(n),
            NodeKind::ArgumentList | NodeKind::TupleExpression
        )
    });

    let mut params = Vec::new();
    let mut kwparams = Vec::new();
    let mut saw_semicolon = false;

    if let Some(args_node) = args_node {
        // Iterate through all children (including non-named like `;`)
        for child in walker.children(args_node) {
            let kind_str = child.kind();

            // Check for semicolon separator
            if kind_str == ";" {
                saw_semicolon = true;
                continue;
            }

            // Skip non-named children (parentheses, commas)
            if !child.is_named() {
                continue;
            }

            let kind = walker.kind(&child);

            if saw_semicolon {
                // After semicolon: keyword parameters (assignments, KwParameter, or kwargs splat)
                match kind {
                    NodeKind::Assignment => {
                        if let Some(kwparam) = parse_kwparam(walker, child)? {
                            kwparams.push(kwparam);
                        }
                    }
                    NodeKind::KwParameter => {
                        // Pure Rust parser format: KwParameter { Identifier, [TypeClause,] [default_value] }
                        // This handles both required kwargs (no default) and kwargs with defaults
                        if let Some(kwparam) = parse_kwparam_from_kw_node(walker, child)? {
                            kwparams.push(kwparam);
                        }
                    }
                    NodeKind::SplatParameter => {
                        // kwargs varargs: function f(; kwargs...)
                        if let Some(kwparam) = parse_kwarg_splat_parameter(walker, child)? {
                            kwparams.push(kwparam);
                        }
                    }
                    NodeKind::SplatExpression => {
                        // kwargs varargs from short-form: f(; kwargs...) = expr
                        // SplatExpression is emitted instead of SplatParameter when the
                        // parser treats the definition as a call expression (Issue #2242)
                        if let Some(kwparam) = parse_kwarg_splat_parameter(walker, child)? {
                            kwparams.push(kwparam);
                        }
                    }
                    NodeKind::KeywordArgument => {
                        // Call expression parser format: KeywordArgument { Identifier name, Expression value }
                        // This handles kwargs like `a=1` or shorthand `a` (which becomes a=a) in function signatures
                        if let Some(kwparam) = parse_kwparam_from_keyword_arg(walker, child)? {
                            kwparams.push(kwparam);
                        }
                    }
                    NodeKind::Operator => {
                        // Skip operator nodes (separators like commas)
                    }
                    NodeKind::Identifier => {
                        // Bare identifier after semicolon: shorthand kwarg `f(;a)` means `f(;a=a)`
                        // This can happen when the parser doesn't wrap it in KeywordArgument/KwParameter
                        let name = walker.text(&child).to_string();
                        let span = walker.span(&child);
                        kwparams.push(KwParam::new(
                            name,
                            Expr::Literal(Literal::Nothing, span),
                            None,
                            span,
                        ));
                    }
                    _ => {
                        // Attempt to parse unrecognized nodes as keyword parameters.
                        // This catches parser-produced node kinds we haven't explicitly handled,
                        // preventing silent data loss (Issue #2244).
                        if let Some(kwparam) = parse_kwparam(walker, child)? {
                            kwparams.push(kwparam);
                        }
                    }
                }
            } else {
                // Before semicolon: positional parameters
                match kind {
                    NodeKind::Assignment => {
                        // Positional parameter with default value: `b=10`
                        // Extract the parameter from the left side of the assignment.
                        // The default value is extracted separately by extract_defaults_from_call_expr.
                        let assign_children: Vec<_> = walker
                            .named_children(&child)
                            .into_iter()
                            .filter(|n| walker.kind(n) != NodeKind::Operator)
                            .collect();
                        if !assign_children.is_empty() {
                            let lhs = assign_children[0];
                            if let Ok(param) = parse_parameter(walker, lhs) {
                                params.push(param);
                            }
                        }
                    }
                    NodeKind::KeywordArgument => {
                        // Pure Rust parser: b=10 in call expressions becomes KeywordArgument
                        // Extract parameter name from the first child (Identifier)
                        let kw_children = walker.named_children(&child);
                        if !kw_children.is_empty() {
                            let name_node = kw_children[0];
                            if let Ok(param) = parse_parameter(walker, name_node) {
                                params.push(param);
                            }
                        }
                    }
                    NodeKind::Operator => {
                        // Skip operators
                    }
                    _ => {
                        if let Ok(param) = parse_parameter(walker, child) {
                            params.push(param);
                        }
                    }
                }
            }
        }
    }

    Ok((name, params, kwparams, is_base_extension))
}

/// Parse a kwargs varargs parameter from a SplatParameter node (e.g., `kwargs...`)
/// This is used for functions like `function f(; kwargs...)` where kwargs collects all keyword arguments.
pub(super) fn parse_kwarg_splat_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Ok(None);
    }

    // First child should be the parameter name
    let name_node = named[0];
    let name = match walker.kind(&name_node) {
        NodeKind::Identifier => walker.text(&name_node).to_string(),
        _ => return Ok(None),
    };

    // Create a varargs KwParam - these collect all remaining kwargs into a NamedTuple
    Ok(Some(KwParam::varargs(name, span)))
}

/// Parse a keyword parameter from an assignment node (e.g., `y=1`)
pub(super) fn parse_kwparam<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    let span = walker.span(&node);

    // Filter out operator nodes to get [name, value]
    let children: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if children.len() < 2 {
        return Ok(None);
    }

    let name_node = children[0];
    let value_node = children[1];

    if walker.kind(&name_node) != NodeKind::Identifier {
        return Ok(None);
    }

    let name = walker.text(&name_node).to_string();
    let default = lower_expr(walker, value_node)?;

    Ok(Some(KwParam::new(name, default, None, span)))
}

/// Parse a keyword parameter from a KwParameter node (Pure Rust parser)
/// Structure: KwParameter { Identifier, [TypeClause,] [default_value] }
pub(super) fn parse_kwparam_from_kw_node<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    if children.is_empty() {
        return Ok(None);
    }

    let mut name: Option<String> = None;
    let mut default_value: Option<crate::ir::core::Expr> = None;

    for child in children {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                } else {
                    // Second Identifier could be a default value like `nothing` or `true`/`false`
                    if default_value.is_none() {
                        default_value = Some(lower_expr(walker, child)?);
                    }
                }
            }
            NodeKind::TypeClause
            | NodeKind::TypedParameter
            | NodeKind::ParametrizedTypeExpression => {
                // Type annotation - skipped in kwarg default parsing
            }
            _ => {
                // This should be the default value expression
                if default_value.is_none() {
                    default_value = Some(lower_expr(walker, child)?);
                }
            }
        }
    }

    let name = match name {
        Some(n) => n,
        None => return Ok(None),
    };

    // Use Undef to mark required keyword parameters (no default value)
    let default = match default_value {
        Some(v) => v,
        None => {
            // No default value - mark as required with Undef
            crate::ir::core::Expr::Literal(crate::ir::core::Literal::Undef, span)
        }
    };

    Ok(Some(KwParam::new(name, default, None, span)))
}

/// Parse a keyword parameter from a KeywordArgument node (from call expression parser).
/// KeywordArgument { Identifier name, Expression value } - used for `x=1` or shorthand `x` (which becomes `x=x`)
pub(super) fn parse_kwparam_from_keyword_arg<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    // KeywordArgument has [name, value] children
    if children.len() < 2 {
        return Ok(None);
    }

    let name_node = children[0];
    let value_node = children[1];

    // Name should be an identifier
    if walker.kind(&name_node) != NodeKind::Identifier {
        return Ok(None);
    }

    let name = walker.text(&name_node).to_string();

    // Check for shorthand: if name == value text, it's a required kwarg (shorthand x -> x=x)
    // In the actual function definition, this should be a required kwarg with Undef default
    let is_shorthand =
        walker.kind(&value_node) == NodeKind::Identifier && walker.text(&value_node) == name;

    let default = if is_shorthand {
        // Shorthand like `a` in `f(; a)` - this is a required keyword argument
        crate::ir::core::Expr::Literal(crate::ir::core::Literal::Undef, span)
    } else {
        // Has explicit default value
        lower_expr(walker, value_node)?
    };

    Ok(Some(KwParam::new(name, default, None, span)))
}

/// Parse a signature node (generated for functions with typed parameters).
/// Returns (name, params, kwparams, is_base_extension).
/// Also returns type_params if the signature contains a where clause.
pub(super) fn parse_signature<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypedParam>, Vec<KwParam>, bool)> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty signature".to_string()),
            span,
        ));
    }

    // First child should be the function name (Identifier) or a call expression
    let mut name: Option<String> = None;
    let mut params: Vec<TypedParam> = Vec::new();
    let mut kwparams: Vec<KwParam> = Vec::new();
    let mut is_base_extension = false;

    for child in &named {
        match walker.kind(child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(child).to_string());
                }
            }
            NodeKind::CallExpression => {
                // The signature might be structured as a call expression inside
                let (sig_name, sig_params, sig_kwparams, sig_is_base) =
                    parse_signature_call(walker, *child)?;
                name = Some(sig_name);
                params = sig_params;
                kwparams = sig_kwparams;
                is_base_extension = sig_is_base;
            }
            NodeKind::WhereExpression => {
                // Signature contains a where clause - extract name/params from left side
                let (sig_name, sig_params, sig_kwparams, sig_is_base, _) =
                    parse_where_expression(walker, *child)?;
                name = Some(sig_name);
                params = sig_params;
                kwparams = sig_kwparams;
                is_base_extension = sig_is_base;
                // Note: type_params are extracted but parse_signature doesn't return them
                // The caller should use parse_signature_with_where instead
            }
            NodeKind::TupleExpression | NodeKind::ArgumentList => {
                // This is the parameter list
                for param_node in walker.named_children(child) {
                    let param = parse_parameter(walker, param_node)?;
                    params.push(param);
                }
            }
            NodeKind::TypedParameter | NodeKind::TypeClause => {
                // Individual typed parameter or anonymous type parameter (::Complex)
                let param = parse_parameter(walker, *child)?;
                params.push(param);
            }
            NodeKind::TypedExpression => {
                // Return type annotation: function f(x)::T - extract call from left side
                let typed_children = walker.named_children(child);
                if !typed_children.is_empty() {
                    let left = typed_children[0];
                    if walker.kind(&left) == NodeKind::CallExpression {
                        let (sig_name, sig_params, sig_kwparams, sig_is_base) =
                            parse_signature_call(walker, left)?;
                        name = Some(sig_name);
                        params = sig_params;
                        kwparams = sig_kwparams;
                        is_base_extension = sig_is_base;
                    }
                }
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing function name in signature".to_string()),
            span,
        )
    })?;

    Ok((name, params, kwparams, is_base_extension))
}

/// Parse a signature node that may contain a where clause.
/// Returns (name, params, kwparams, is_base_extension, type_params).
pub(super) fn parse_signature_with_where<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypedParam>, Vec<KwParam>, bool, Vec<TypeParam>)> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty signature".to_string()),
            span,
        ));
    }

    let mut name: Option<String> = None;
    let mut params: Vec<TypedParam> = Vec::new();
    let mut kwparams: Vec<KwParam> = Vec::new();
    let mut is_base_extension = false;
    let mut type_params: Vec<TypeParam> = Vec::new();

    for child in &named {
        match walker.kind(child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(child).to_string());
                }
            }
            NodeKind::CallExpression => {
                let (sig_name, sig_params, sig_kwparams, sig_is_base) =
                    parse_signature_call(walker, *child)?;
                name = Some(sig_name);
                params = sig_params;
                kwparams = sig_kwparams;
                is_base_extension = sig_is_base;
            }
            NodeKind::WhereExpression => {
                // Signature contains a where clause
                let (sig_name, sig_params, sig_kwparams, sig_is_base, sig_type_params) =
                    parse_where_expression(walker, *child)?;
                name = Some(sig_name);
                params = sig_params;
                kwparams = sig_kwparams;
                is_base_extension = sig_is_base;
                type_params = sig_type_params;
            }
            NodeKind::TupleExpression | NodeKind::ArgumentList => {
                for param_node in walker.named_children(child) {
                    let param = parse_parameter(walker, param_node)?;
                    params.push(param);
                }
            }
            NodeKind::TypedParameter | NodeKind::TypeClause => {
                let param = parse_parameter(walker, *child)?;
                params.push(param);
            }
            NodeKind::TypedExpression => {
                // Return type annotation: function f(x)::T - extract call from left side
                let typed_children = walker.named_children(child);
                if !typed_children.is_empty() {
                    let left = typed_children[0];
                    if walker.kind(&left) == NodeKind::CallExpression {
                        let (sig_name, sig_params, sig_kwparams, sig_is_base) =
                            parse_signature_call(walker, left)?;
                        name = Some(sig_name);
                        params = sig_params;
                        kwparams = sig_kwparams;
                        is_base_extension = sig_is_base;
                    }
                }
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing function name in signature".to_string()),
            span,
        )
    })?;

    Ok((name, params, kwparams, is_base_extension, type_params))
}

/// Parse a single parameter (typed or untyped).
///
/// # Parameter Node Types
///
/// This function handles parameters from both **full-form** and **short-form** function definitions.
/// The parser produces different CST node types depending on the context:
///
/// | Function Form                     | Varargs CST Node   | Example                      |
/// |-----------------------------------|-------------------|------------------------------|
/// | Full: `function f(args...) end`   | `SplatParameter`  | Explicit function definition |
/// | Short: `f(args...) = expr`        | `SplatExpression` | Assignment-style definition  |
///
/// ## Supported Node Kinds
///
/// - `Identifier` - Untyped parameter: `x`
/// - `TypedParameter` / `TypedExpression` / `Parameter` - Typed parameter: `x::Int64`
/// - `SplatParameter` - Varargs from full-form functions: `args...`
/// - `SplatExpression` - Varargs from short-form functions: `args...`
/// - `TypeClause` - Anonymous typed parameter: `::Complex`
/// - `UnaryTypedExpression` - Anonymous typed: `::Type{T}`
///
/// ## Issue #1721 Context
///
/// Prior to the fix, only `SplatParameter` was handled for varargs, causing short-form
/// function definitions like `sum(args...) = ...` to fail with "Undefined variable" errors.
/// The fix added `SplatExpression` handling to ensure both forms work identically.
pub(super) fn parse_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypedParam> {
    let span = walker.span(&node);

    match walker.kind(&node) {
        NodeKind::Identifier => {
            // Untyped parameter: x
            Ok(TypedParam::untyped(walker.text(&node).to_string(), span))
        }
        NodeKind::TypedParameter | NodeKind::TypedExpression | NodeKind::Parameter => {
            // Typed parameter: x::Int64 or x::Complex{Float64}
            // Parameter is used by Pure Rust parser, TypedParameter/TypedExpression by tree-sitter
            parse_typed_parameter(walker, node)
        }
        NodeKind::SplatParameter => {
            // Varargs parameter: args... or args::T...
            // SplatParameter has children: [Identifier, (optional TypeClause)]
            parse_splat_parameter(walker, node)
        }
        NodeKind::SplatExpression => {
            // Varargs parameter when parsed as call expression (short function definition)
            // SplatExpression is created when the parser treats f(args...) as a call,
            // not as a function definition. It has children: [Identifier or TypedExpression]
            parse_splat_expression_as_parameter(walker, node)
        }
        NodeKind::TypeClause => {
            // Anonymous typed parameter: ::Complex (type only, no name)
            // Extract the type from the clause and use "_" as synthetic name
            let mut type_annotation: Option<JuliaType> = None;
            for type_child in walker.named_children(&node) {
                if matches!(walker.kind(&type_child), NodeKind::Identifier) {
                    let type_name = walker.text(&type_child);
                    type_annotation = parse_type_name(type_name, walker.span(&type_child))?;
                }
            }
            Ok(TypedParam::new("_".to_string(), type_annotation, span))
        }
        NodeKind::UnaryTypedExpression => {
            // Anonymous typed parameter: ::Type{T} or ::SomeType
            // Used in promote_rule, convert signatures
            parse_unary_typed_parameter(walker, node)
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(format!(
                "unsupported function parameter: {:?}",
                walker.kind(&node)
            )),
            span,
        )),
    }
}

/// Parse a typed parameter (x::Int64).
/// Also handles varargs typed parameters (x::Int64...) when the parser emits them as Parameter nodes.
pub(super) fn parse_typed_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypedParam> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);
    let node_text = walker.text(&node);

    // Check if this is a varargs parameter by looking for trailing "..."
    // The parser sometimes emits typed varargs (x::T...) as Parameter nodes instead of SplatParameter
    let is_varargs = node_text.ends_with("...");

    // Check if this is an anonymous typed parameter (starts with ::)
    // For ::B, the single Identifier child is the TYPE, not the name
    let is_anonymous = node_text.starts_with("::");

    // First child should be the parameter name (Identifier)
    // Second child should be the type (Identifier or type expression)
    let mut name: Option<String> = None;
    let mut type_annotation: Option<JuliaType> = None;

    // For Parameter nodes with default values (e.g., `filename::AbstractString="string"` or
    // `greeting="Hello"`), the children include: [name, (optional type), default_expr].
    // We must stop processing once we've extracted name and type, to avoid treating the
    // default expression as a type annotation.
    let has_default = node_text.contains('=');
    let has_type_annotation = node_text.contains("::");

    for child in named {
        // If this Parameter has a default value, stop after extracting name (and type if present).
        // Without this, default value expressions get misidentified as type annotations.
        if has_default {
            if has_type_annotation && name.is_some() && type_annotation.is_some() {
                break;
            }
            if !has_type_annotation && name.is_some() {
                break;
            }
        }

        match walker.kind(&child) {
            NodeKind::Identifier => {
                if is_anonymous {
                    // For anonymous parameters like ::B, the identifier IS the type
                    let type_name = walker.text(&child);
                    type_annotation = parse_type_name(type_name, walker.span(&child))?;
                } else if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                } else {
                    // This is the type name
                    let type_name = walker.text(&child);
                    type_annotation = parse_type_name(type_name, walker.span(&child))?;
                }
            }
            NodeKind::ParametrizedTypeExpression => {
                // Handle parametric types like Complex{T} directly
                let type_name = walker.text(&child);
                type_annotation = parse_type_name(type_name, walker.span(&child))?;
            }
            NodeKind::TypeClause => {
                // ::Int64 or ::Complex{Float64} - extract the type from the clause
                for type_child in walker.named_children(&child) {
                    match walker.kind(&type_child) {
                        NodeKind::Identifier => {
                            let mut type_name = walker.text(&type_child);
                            // Strip trailing "..." if this is a varargs parameter
                            if is_varargs && type_name.ends_with("...") {
                                type_name = &type_name[..type_name.len() - 3];
                            }
                            type_annotation = parse_type_name(type_name, walker.span(&type_child))?;
                        }
                        NodeKind::ParametrizedTypeExpression => {
                            // Handle parametric types like Complex{Float64}
                            let mut type_name = walker.text(&type_child).to_string();
                            // Strip trailing "..." if this is a varargs parameter
                            if is_varargs && type_name.ends_with("...") {
                                type_name = type_name[..type_name.len() - 3].to_string();
                            }
                            type_annotation =
                                parse_type_name(&type_name, walker.span(&type_child))?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                // Try to extract type from other node types
                let mut type_name = walker.text(&child).to_string();
                // Strip trailing "..." if this is a varargs parameter
                if is_varargs && type_name.ends_with("...") {
                    type_name = type_name[..type_name.len() - 3].to_string();
                }
                if let Some(ty) = parse_type_name(&type_name, walker.span(&child))? {
                    type_annotation = Some(ty);
                }
            }
        }
    }

    // For anonymous typed parameters (::Complex), generate a synthetic name
    let name = name.unwrap_or_else(|| "_".to_string());

    // Detect Vararg{T} and Vararg{T,N} type annotations (Issue #2525)
    // f(x::Vararg{Int64}) is equivalent to f(x::Int64...)
    // f(x::Vararg{Int64, 2}) additionally constrains to exactly 2 arguments
    let mut is_vararg = is_varargs;
    let mut vararg_count: Option<usize> = None;
    let mut type_ann = type_annotation;
    if !is_vararg {
        // Extract Vararg info before modifying type_ann to avoid borrow issues
        let vararg_info: Option<(Option<String>, Option<String>)> = match &type_ann {
            Some(JuliaType::Struct(n)) if n == "Vararg" => {
                Some((None, None)) // bare Vararg
            }
            Some(JuliaType::Struct(n)) if n.starts_with("Vararg{") && n.ends_with('}') => {
                let inner = &n[7..n.len() - 1];
                let (elem_str, count_str) = if let Some(comma_pos) = inner.find(',') {
                    (
                        inner[..comma_pos].trim().to_string(),
                        Some(inner[comma_pos + 1..].trim().to_string()),
                    )
                } else {
                    (inner.trim().to_string(), None)
                };
                Some((Some(elem_str), count_str))
            }
            _ => None,
        };
        if let Some((elem_str_opt, count_str_opt)) = vararg_info {
            is_vararg = true;
            match elem_str_opt {
                Some(elem_str) if !elem_str.is_empty() && elem_str != "Any" => {
                    type_ann = parse_type_name(&elem_str, span)?;
                }
                _ => {
                    type_ann = None;
                }
            }
            if let Some(count_str) = count_str_opt {
                if let Ok(n) = count_str.parse::<usize>() {
                    vararg_count = Some(n);
                }
            }
        }
    }

    // Return varargs parameter if detected
    if is_vararg {
        if let Some(count) = vararg_count {
            Ok(TypedParam::varargs_fixed(name, type_ann, count, span))
        } else {
            Ok(TypedParam::varargs(name, type_ann, span))
        }
    } else {
        Ok(TypedParam::new(name, type_ann, span))
    }
}

/// Parse a splat/varargs parameter (args... or args::T...).
/// In Julia, varargs parameters collect all remaining arguments into a Tuple.
pub(super) fn parse_splat_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypedParam> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty splat parameter".to_string()),
            span,
        ));
    }

    // First child should be the parameter name
    let name_node = named[0];
    let name = match walker.kind(&name_node) {
        NodeKind::Identifier => walker.text(&name_node).to_string(),
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::Other(format!(
                    "expected identifier in splat parameter, got {:?}",
                    walker.kind(&name_node)
                )),
                span,
            ));
        }
    };

    // Optional type annotation from second child (TypeClause)
    let mut type_annotation: Option<JuliaType> = None;
    if named.len() > 1 {
        let type_node = named[1];
        match walker.kind(&type_node) {
            NodeKind::TypeClause => {
                for type_child in walker.named_children(&type_node) {
                    match walker.kind(&type_child) {
                        NodeKind::Identifier => {
                            let type_name = walker.text(&type_child);
                            type_annotation = parse_type_name(type_name, walker.span(&type_child))?;
                        }
                        NodeKind::ParametrizedTypeExpression => {
                            let type_name = walker.text(&type_child);
                            type_annotation = parse_type_name(type_name, walker.span(&type_child))?;
                        }
                        _ => {}
                    }
                }
            }
            NodeKind::Identifier => {
                // Direct type: args::Int...
                let type_name = walker.text(&type_node);
                type_annotation = parse_type_name(type_name, walker.span(&type_node))?;
            }
            NodeKind::ParametrizedTypeExpression => {
                let type_name = walker.text(&type_node);
                type_annotation = parse_type_name(type_name, walker.span(&type_node))?;
            }
            _ => {}
        }
    }

    Ok(TypedParam::varargs(name, type_annotation, span))
}

/// Parse a SplatExpression as a varargs parameter.
/// This handles cases where the parser treats f(args...) as a call expression
/// (e.g., in short function definitions: sum_all(args...) = sum(args))
/// SplatExpression has children: [Identifier] or [TypedExpression] for typed varargs
pub(super) fn parse_splat_expression_as_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypedParam> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty splat expression".to_string()),
            span,
        ));
    }

    // First (and usually only) child is the expression being splatted
    let inner = named[0];
    match walker.kind(&inner) {
        NodeKind::Identifier => {
            // Untyped varargs: args...
            let name = walker.text(&inner).to_string();
            Ok(TypedParam::varargs(name, None, span))
        }
        NodeKind::TypedExpression | NodeKind::TypedParameter => {
            // Typed varargs: args::T...
            // TypedExpression has children: [Identifier (name), Identifier/Type (type)]
            let inner_children = walker.named_children(&inner);
            if inner_children.is_empty() {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::Other("empty typed expression in splat".to_string()),
                    span,
                ));
            }

            let name_node = inner_children[0];
            let name = match walker.kind(&name_node) {
                NodeKind::Identifier => walker.text(&name_node).to_string(),
                _ => "_".to_string(), // fallback for anonymous
            };

            // Extract type annotation from remaining children
            let mut type_annotation: Option<JuliaType> = None;
            for child in inner_children.iter().skip(1) {
                match walker.kind(child) {
                    NodeKind::Identifier | NodeKind::ParametrizedTypeExpression => {
                        let type_name = walker.text(child);
                        type_annotation = parse_type_name(type_name, walker.span(child))?;
                        break;
                    }
                    _ => {}
                }
            }

            Ok(TypedParam::varargs(name, type_annotation, span))
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(format!(
                "unexpected node in splat expression: {:?}",
                walker.kind(&inner)
            )),
            span,
        )),
    }
}

/// Parse a unary typed expression (::Type{T} or ::SomeType).
/// This handles anonymous type parameters used in promote_rule, convert, etc.
pub(super) fn parse_unary_typed_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypedParam> {
    let span = walker.span(&node);

    // UnaryTypedExpression has a single child which is the type
    for child in walker.named_children(&node) {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                // Simple type: ::SomeType
                let type_name = walker.text(&child);
                let type_annotation = parse_type_name(type_name, walker.span(&child))?;
                return Ok(TypedParam::new("_".to_string(), type_annotation, span));
            }
            NodeKind::ParametrizedTypeExpression => {
                // Parametric type: ::Type{T} or ::Complex{Float64}
                let type_name = walker.text(&child);
                let type_annotation = parse_type_name(type_name, walker.span(&child))?;
                return Ok(TypedParam::new("_".to_string(), type_annotation, span));
            }
            _ => {
                // Try to use the text as type name
                let type_name = walker.text(&child);
                if let Some(ty) = parse_type_name(type_name, walker.span(&child))? {
                    return Ok(TypedParam::new("_".to_string(), Some(ty), span));
                }
            }
        }
    }

    // Fallback: use the whole node text as type
    let type_name = walker.text(&node);
    // Remove leading :: if present
    let type_name = type_name.strip_prefix("::").unwrap_or(type_name);
    let type_annotation = parse_type_name(type_name, span)?;
    Ok(TypedParam::new("_".to_string(), type_annotation, span))
}

/// Parse a type name string into a JuliaType.
/// Unknown type names are treated as user-defined struct types.
pub(super) fn parse_type_name(
    type_name: &str,
    _span: crate::parser::span::Span,
) -> LowerResult<Option<JuliaType>> {
    // Use from_name_or_struct to treat unknown types as user-defined structs
    Ok(Some(JuliaType::from_name_or_struct(type_name)))
}
