//! Function signature and parameter parsing.
//!
//! Parses function signatures, typed parameters, keyword parameters,
//! splat parameters, and type name resolution.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, Expr, KwParam, Literal, Stmt, TypedParam};
use crate::lowering::expr::lower_expr;
use crate::lowering::{internal_lowering_error, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::{JuliaType, TypeParam};
use subset_julia_vm_bytecode::CALLABLE_SELF_BOUND_MARKER;

use super::where_clause::parse_where_expression;

const PARAM_DESTRUCTURING_PREFIX: &str = "__param_destructure__";

/// Resolve the function name for a `Module.field` definition callee
/// (`function Base.:+(...)`, `function Inner.f(...)`, `Inner.f(...) = ...`).
///
/// For `Base`, returns the bare operator/function name with
/// `is_base_extension = true` so the method joins Base's generic function. For
/// any other module, returns the module-qualified name `"Module.field"` (with
/// `is_base_extension = false`) so the method extends that module's existing
/// function — `build_method_tables` registers it under both `Module.field` and
/// the bare `field` table so a later `Inner.f(2.0)` AND the unqualified `f(2.0)`
/// brought in by `using .Inner` both dispatch across the module-owned methods
/// (Issue #8052).
///
/// `module_text` is the left side of the field expression (may be a dotted path
/// like `A.B`); `field_text` is the field, possibly a quoted operator (`:+`,
/// `:(==)`) or parenthesized operator (`(==)`).
pub(super) fn module_extension_function_name(
    module_text: &str,
    field_text: &str,
) -> (String, bool) {
    let mut field = field_text
        .strip_prefix(':')
        .unwrap_or(field_text)
        .to_string();
    if field.starts_with('(') && field.ends_with(')') {
        field = field[1..field.len() - 1].to_string();
    }
    if module_text == "Base" {
        (field, true)
    } else {
        (format!("{}.{}", module_text, field), false)
    }
}

pub fn inject_parameter_destructuring_prologue(
    params: Vec<TypedParam>,
    mut body: Block,
) -> (Vec<TypedParam>, Block) {
    let mut prologue = Vec::new();

    for param in &params {
        if let Some(targets) = decode_destructuring_param_name(&param.name) {
            let temp = format!(
                "{}tmp_{}_{}",
                PARAM_DESTRUCTURING_PREFIX, param.span.start, param.span.end
            );
            prologue.push(Stmt::Assign {
                var: temp.clone(),
                value: Expr::Var(param.name.clone().into(), param.span),
                span: param.span,
            });

            for (idx, target) in targets.into_iter().enumerate() {
                prologue.push(Stmt::Assign {
                    var: target,
                    value: Expr::Index {
                        array: Box::new(Expr::Var(temp.clone().into(), param.span)),
                        indices: vec![Expr::Literal(Literal::Int((idx + 1) as i64), param.span)],
                        span: param.span,
                    },
                    span: param.span,
                });
            }
        }
    }

    if !prologue.is_empty() {
        prologue.extend(body.stmts);
        body.stmts = prologue;
    }

    (params, body)
}

/// Re-introduce a bound-form callable struct's `self` parameter under its
/// original identifier as an aliasing local at the top of the body.
///
/// `parse_callable_self_param` renames the synthesized receiver parameter to
/// `CALLABLE_SELF_BOUND_MARKER` + the user's chosen identifier (e.g. `self`,
/// `callable`) so the marker rides unchanged into the compiled
/// `FunctionInfo::params[0].0` as a ground-truth bound-ness signal (Issue
/// #11553). The function body, however, was parsed from source and still
/// references the plain identifier directly (`self.tag`), so this prologue
/// assigns the original name from the marked parameter as the very first
/// statement — the same alias-via-prologue technique
/// `inject_parameter_destructuring_prologue` uses for tuple-destructured
/// parameters. A no-op unless `params[0]` actually carries the marker.
pub fn inject_callable_self_alias_prologue(
    params: Vec<TypedParam>,
    mut body: Block,
) -> (Vec<TypedParam>, Block) {
    if let Some(first) = params.first() {
        if let Some(original_name) = first.name.strip_prefix(CALLABLE_SELF_BOUND_MARKER) {
            let alias = Stmt::Assign {
                var: original_name.to_string(),
                value: Expr::Var(first.name.clone().into(), first.span),
                span: first.span,
            };
            body.stmts.insert(0, alias);
        }
    }
    (params, body)
}

fn encode_destructuring_param_name(span: crate::span::Span, targets: &[String]) -> String {
    format!(
        "{}{}_{}:{}",
        PARAM_DESTRUCTURING_PREFIX,
        span.start,
        span.end,
        targets.join(",")
    )
}

fn decode_destructuring_param_name(name: &str) -> Option<Vec<String>> {
    let rest = name.strip_prefix(PARAM_DESTRUCTURING_PREFIX)?;
    let (_, targets) = rest.split_once(':')?;
    let targets: Vec<String> = targets
        .split(',')
        .filter(|target| !target.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    (!targets.is_empty()).then_some(targets)
}

pub fn parameter_binding_names(param: &TypedParam) -> Vec<String> {
    if let Some(original_name) = param.name.strip_prefix(CALLABLE_SELF_BOUND_MARKER) {
        return vec![original_name.to_string()];
    }
    decode_destructuring_param_name(&param.name).unwrap_or_else(|| vec![param.name.clone()])
}

fn parse_destructuring_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<TypedParam>> {
    let span = walker.span(&node);
    let pattern = match walker.kind(&node) {
        NodeKind::TupleExpression => Some(node),
        NodeKind::Parameter => walker
            .named_children(&node)
            .find(|child| walker.kind(child) == NodeKind::TupleExpression),
        _ => None,
    };

    let Some(pattern) = pattern else {
        return Ok(None);
    };

    let mut targets = Vec::new();
    for child in walker.named_children(&pattern) {
        match walker.kind(&child) {
            NodeKind::Identifier => targets.push(walker.text(&child).to_string()),
            _ => {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::Other(
                        "nested destructuring function parameters are not yet supported"
                            .to_string(),
                    ),
                    walker.span(&child),
                )
                .with_hint(
                    "parameter destructuring currently supports a flat tuple of identifiers",
                ));
            }
        }
    }

    if targets.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty destructuring function parameter".to_string()),
            span,
        ));
    }

    Ok(Some(TypedParam::untyped(
        encode_destructuring_param_name(span, &targets),
        span,
    )))
}

pub(super) fn parse_signature_call<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(String, Vec<TypedParam>, Vec<KwParam>, bool)> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);
    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("empty function signature".to_string()),
            span,
        ));
    }

    let callee = named[0];
    // For callable struct definitions `(self::Type)(args)`, the struct instance
    // binds to `self` as a synthetic leading parameter (Issue #5127).
    let mut self_param: Option<TypedParam> = None;
    let (name, is_base_extension) = match walker.kind(&callee) {
        NodeKind::Identifier => (walker.text(&callee).to_string(), false),
        NodeKind::Operator => {
            // Allow operator overloading: function +(a, b) ... end
            (walker.text(&callee).to_string(), false)
        }
        NodeKind::ParametrizedTypeExpression => {
            // Parametric constructor: function Complex{Float64}(x, y) ... end
            (type_name_text(walker, &callee), false)
        }
        NodeKind::FieldExpression => {
            // Handle `Base.:+`/`Base.show` (Base extension) and `Inner.f` (extend
            // another module's function, Issue #8052). For non-Base modules the
            // method is registered under the module-qualified name so it joins the
            // target module's existing generic function.
            let children = walker.named_children_vec(&callee);
            if children.len() >= 2 {
                let module = walker.text(&children[0]);
                let field_text = walker.text(&children[1]);
                module_extension_function_name(module, field_text)
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
            // Callable struct syntax: (::Type)(args) = body  (anonymous self)
            //                     or: (self::Type)(args) = body  (bound self; field access)
            // The parenthesized expression contains a UnaryTypedExpression (anonymous)
            // or a TypedExpression (`self::Type`).
            let inner_children = walker.named_children_vec(&callee);
            if inner_children.len() == 1 {
                let inner = inner_children[0];
                match walker.kind(&inner) {
                    NodeKind::UnaryTypedExpression => {
                        // Extract type name from ::Type
                        let type_children = walker.named_children_vec(&inner);
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
                    NodeKind::TypedExpression => {
                        // Bound form `(self::Type)(args)`: bind the struct instance to `self`
                        // so the method body can read its fields (Issue #5127).
                        let (param, type_name) = parse_callable_self_param(walker, inner)?;
                        self_param = Some(param);
                        (format!("__callable_{}", type_name), false)
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
                if let Some(kwparam) = parse_kwparam_node(walker, child)? {
                    kwparams.push(kwparam);
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
                        let kw_children = walker.named_children_vec(&child);
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

    // Bound callable struct: inject `self` as the leading parameter so the body
    // can read the struct's fields. The runtime prepends the struct instance to
    // the call arguments (Issue #5127).
    if let Some(self_param) = self_param {
        params.insert(0, self_param);
    }

    Ok((name, params, kwparams, is_base_extension))
}

/// Parse the `self::Type` binding for a callable struct definition `(self::Type)(args)`.
///
/// Returns the `self` parameter (typed by the struct type) and the type *name*
/// used to form the `__callable_<TypeName>` dispatch name. The type name strips
/// any `{...}` parameter list so `(f::Fix2{F,T})(x)` dispatches on `Fix2`
/// (Issue #5127).
///
/// The returned parameter's *name* is marked with `CALLABLE_SELF_BOUND_MARKER`
/// (prefixed ahead of the user's chosen identifier, e.g. `self`, `callable`)
/// rather than left as the bare identifier. This is the ground-truth
/// structural signal the runtime uses to distinguish a genuine bound-form
/// receiver from an anonymous-form method whose own first parameter merely
/// happens to be annotated with the struct's own type — arity and type shape
/// alone cannot (Issue #11553; see also Issue #11386). The caller must run
/// `inject_callable_self_alias_prologue` once `params`/`body` are both
/// finalized so ordinary body references to the original identifier still
/// resolve.
pub(super) fn parse_callable_self_param<'a>(
    walker: &CstWalker<'a>,
    inner: Node<'a>,
) -> LowerResult<(TypedParam, String)> {
    let mut param = parse_parameter(walker, inner)?;
    // The dispatch table keys callable methods by the bare type name, so strip
    // any parametric `{...}` suffix from the annotated type.
    let type_name = match &param.type_annotation {
        Some(t) => {
            let full = t.to_string();
            full.split('{').next().unwrap_or(&full).to_string()
        }
        None => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::Other(
                    "callable struct binding `(self::Type)` requires a struct type".to_string(),
                ),
                walker.span(&inner),
            ))
        }
    };
    param.name = format!("{CALLABLE_SELF_BOUND_MARKER}{}", param.name);
    Ok((param, type_name))
}

/// Parse a kwargs varargs parameter from a SplatParameter node (e.g., `kwargs...`)
/// This is used for functions like `function f(; kwargs...)` where kwargs collects all keyword arguments.
pub(super) fn parse_kwarg_splat_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

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

/// Lower ONE post-`;` node of a parameter list into a `KwParam`.
///
/// THE single authority for every keyword-parameter CST shape the parser can
/// emit after a `;`, in ANY parameter list: named/short-form signatures
/// (`parse_signature_call` above) AND `->` arrow lambdas (the two collectors in
/// `lowering/function/short_form.rs` and `lowering/expr/call.rs`).
///
/// Issue #10354: the arrow collectors used to open-code their own PARTIAL copy
/// of this match, recognizing only the bare-`Identifier` `KwParameter` shape.
/// An ANNOTATED keyword (`(y; k::Integer = 3) -> (y, k)`) parses as an
/// `Assignment` whose LHS is a `TypedExpression`, matched NO arm, and was
/// silently dropped by their `_ => {}` catch-all — the keyword vanished from
/// the lambda's signature entirely, so the body's `k` compiled to a global load
/// (`UndefVarError: k not defined`) and a supplied `k = 3` was rejected as an
/// "unsupported keyword argument". Upstream runs both forms. The two copies
/// also silently skipped the required-annotated (#11081) and declared-type
/// (#11024) handling this function grew, so they could only ever re-diverge.
/// Routing every arrow kwarg through this one function is what keeps the arrow
/// and named-function forms from drifting apart again.
///
/// Returns `Ok(None)` for nodes that carry no keyword parameter (e.g. a comma
/// `Operator`), never silently discarding a shape it does not recognize — the
/// catch-all re-attempts `parse_kwparam` rather than dropping (Issue #2244).
pub fn parse_kwparam_node<'a>(
    walker: &CstWalker<'a>,
    child: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    match walker.kind(&child) {
        NodeKind::Assignment => parse_kwparam(walker, child),
        NodeKind::KwParameter => {
            // Pure Rust parser format: KwParameter { Identifier, [TypeClause,] [default_value] }
            // This handles both required kwargs (no default) and kwargs with defaults
            parse_kwparam_from_kw_node(walker, child)
        }
        NodeKind::SplatParameter => {
            // kwargs varargs: function f(; kwargs...)
            parse_kwarg_splat_parameter(walker, child)
        }
        NodeKind::SplatExpression => {
            // kwargs varargs from short-form: f(; kwargs...) = expr
            // SplatExpression is emitted instead of SplatParameter when the
            // parser treats the definition as a call expression (Issue #2242)
            parse_kwarg_splat_parameter(walker, child)
        }
        NodeKind::KeywordArgument => {
            // Call expression parser format: KeywordArgument { Identifier name, Expression value }
            // This handles kwargs like `a=1` or shorthand `a` (which becomes a=a) in function signatures
            parse_kwparam_from_keyword_arg(walker, child)
        }
        NodeKind::Operator => {
            // Skip operator nodes (separators like commas)
            Ok(None)
        }
        NodeKind::Identifier => {
            // Bare identifier after semicolon: shorthand kwarg `f(;a)` means `f(;a=a)`
            // This can happen when the parser doesn't wrap it in KeywordArgument/KwParameter
            let name = walker.text(&child).to_string();
            let span = walker.span(&child);
            Ok(Some(KwParam::new(
                name,
                Expr::Literal(Literal::Nothing, span),
                None,
                span,
            )))
        }
        NodeKind::TypedExpression | NodeKind::TypedParameter => {
            // A REQUIRED keyword with a type annotation and no default
            // (`f(; x::Int64)`) reaches the signature as a bare typed node,
            // NOT an assignment. Issue #11081: it used to fall into the
            // catch-all below, where `parse_kwparam` read the node's two
            // children as `[name, value]` and lowered the TYPE EXPRESSION as
            // the keyword's DEFAULT — so the keyword became optional and
            // `f()` silently produced a value instead of upstream's
            // `UndefKeywordError`. Lower it as required (an `Undef` default
            // marks a required keyword) and carry its declared type
            // (Issue #11024).
            let span = walker.span(&child);
            let name = walker
                .named_children(&child)
                .find(|n| walker.kind(n) == NodeKind::Identifier)
                .map(|ident| walker.text(&ident).to_string());
            let Some(name) = name else {
                return Ok(None);
            };
            let type_annotation = kwparam_type_annotation(walker, &child)?;
            Ok(Some(KwParam::new(
                name,
                Expr::Literal(Literal::Undef, span),
                type_annotation,
                span,
            )))
        }
        _ => {
            // Attempt to parse unrecognized nodes as keyword parameters.
            // This catches parser-produced node kinds we haven't explicitly handled,
            // preventing silent data loss (Issue #2244).
            parse_kwparam(walker, child)
        }
    }
}

/// Extract a keyword parameter's declared type from its annotated name node
/// (`n::Int = 0` parses the name as a `TypedExpression`/`TypedParameter` whose
/// second named child is the type). Issue #11024: both kwparam lowering paths
/// used to parse the annotation only to SKIP it, so `KwParam.type_annotation`
/// was always `None` — the declared type could neither be validated at
/// definition time (the #10582 probes read exactly this field) nor enforced at
/// the call site. Mirrors the positional-parameter path's `parse_type_name`.
fn kwparam_type_annotation<'a>(
    walker: &CstWalker<'a>,
    name_node: &Node<'a>,
) -> LowerResult<Option<JuliaType>> {
    if !matches!(
        walker.kind(name_node),
        NodeKind::TypedExpression | NodeKind::TypedParameter
    ) {
        return Ok(None);
    }
    // Children are [name, type] (the `::` operator node is not named); the type
    // may itself be a parametric/`Type{...}` expression, which `parse_type_name`
    // resolves the same way it does for a positional annotation.
    let mut children = walker
        .named_children(name_node)
        .filter(|n| walker.kind(n) != NodeKind::Operator);
    let _name = children.next();
    let Some(type_node) = children.next() else {
        return Ok(None);
    };
    parse_type_name(walker.text(&type_node), walker.span(&type_node))
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
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if children.len() < 2 {
        return Ok(None);
    }

    let name_node = children[0];
    let value_node = children[1];

    // The kwarg name is a plain `Identifier` for an unannotated kwarg (`n = 0`),
    // but a `TypedExpression` / `TypedParameter` for an annotated one
    // (`n::Int = 0`), whose first `Identifier` child is the name. Issue #5422:
    // the short-form `f(; n::Int = 0) = ...` path previously bailed out here for
    // the annotated case, silently dropping the keyword parameter and leaving
    // `n` undefined in the body. Match the long-form path
    // (`parse_kwparam_from_kw_node`), which registers the name (the type
    // annotation is not carried on kwparams in either path).
    let name = match walker.kind(&name_node) {
        NodeKind::Identifier => walker.text(&name_node).to_string(),
        NodeKind::TypedExpression | NodeKind::TypedParameter => {
            match walker
                .named_children(&name_node)
                .find(|n| walker.kind(n) == NodeKind::Identifier)
            {
                Some(ident) => walker.text(&ident).to_string(),
                None => return Ok(None),
            }
        }
        _ => return Ok(None),
    };
    let type_annotation = kwparam_type_annotation(walker, &name_node)?;
    let default = lower_expr(walker, value_node)?;

    Ok(Some(KwParam::new(name, default, type_annotation, span)))
}

/// Parse a keyword parameter from a KwParameter node (Pure Rust parser).
///
/// Structure (positional in the child list):
///   KwParameter { Identifier name, [type_expr,] [default_value] }
///
/// The type expression has no surrounding wrapper node — it's whatever
/// `parse_type_expression` produced (typically an `Identifier` for plain
/// types like `Bool` / `Int64`, or `ParametrizedTypeExpression` for
/// parametric types like `Vector{Int64}`). That makes it impossible to
/// distinguish `Identifier` (type) from `Identifier` (default value
/// like `true`/`false`/`nothing`) by node-kind alone, so we use a
/// text-based heuristic on the node source — `name::Type=default` has
/// both `::` and `=`, while `name=default` has only `=`. (Issue #3653.)
pub(super) fn parse_kwparam_from_kw_node<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    if children.is_empty() {
        return Ok(None);
    }

    // Detect whether the source text contains `::` and `=`. The parser
    // consumed both before producing the children, so the node's text
    // span is a reliable signal.
    let node_text = walker.text(&node);
    let has_type_annotation = node_text.contains("::");
    let has_default = node_text.contains('=');

    let mut name: Option<String> = None;
    let mut default_value: Option<crate::ir::core::Expr> = None;
    // `seen_type_identifier`: when the kwarg has a type annotation that
    // produced a bare Identifier (e.g. `::Bool`, `::Int64`), we need to
    // skip exactly one Identifier-after-the-name before treating the
    // next child as the default. Without this, a kwarg like
    // `x::Bool=true` would lower as `default = expression "Bool"` —
    // dropping the actual `true` literal entirely (Issue #3653).
    let mut seen_type_identifier = false;
    // Issue #11024: the kwparam's declared type (`n::Int = 0`), previously dropped.
    let mut type_annotation: Option<JuliaType> = None;

    for child in children {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if name.is_none() {
                    name = Some(walker.text(&child).to_string());
                } else if has_type_annotation && !seen_type_identifier {
                    // Second Identifier when `::` was consumed: this is
                    // the type expression (e.g. `Bool`), not a default.
                    // Issue #11024: carry it as the kwparam's declared type
                    // instead of dropping it.
                    type_annotation = parse_type_name(walker.text(&child), walker.span(&child))?;
                    seen_type_identifier = true;
                } else if default_value.is_none() {
                    // Otherwise this is the default value (e.g.
                    // identifier-defaults like `nothing` / `true` /
                    // `false`, or the third child after a type when a
                    // default is also present).
                    default_value = Some(lower_expr(walker, child)?);
                }
            }
            NodeKind::TypeClause
            | NodeKind::TypedParameter
            | NodeKind::ParametrizedTypeExpression => {
                // Type annotation wrapped in a recognisable node. Issue #11024:
                // record the declared type (a `TypeClause` wraps the bare type
                // name; a parametric expression resolves through the same
                // `parse_type_name` the positional path uses) instead of only
                // skipping it. Mark `seen_type_identifier` so any later
                // bare-Identifier child is still treated as the default value.
                let type_node = walker
                    .named_children(&child)
                    .find(|n| walker.kind(n) != NodeKind::Operator)
                    .unwrap_or(child);
                type_annotation =
                    parse_type_name(walker.text(&type_node), walker.span(&type_node))?;
                seen_type_identifier = true;
            }
            _ => {
                // Anything else is the default value expression
                // (Literals, function calls, etc.).
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

    // Use Undef to mark required keyword parameters (no default value).
    let default = match default_value {
        Some(v) => v,
        None => {
            if has_default {
                // The source had `=` but we somehow failed to extract a
                // default. Fall through to Undef so the call site will
                // surface the missing-default error rather than
                // silently using an unrelated value.
                crate::ir::core::Expr::Literal(crate::ir::core::Literal::Undef, span)
            } else {
                // No default value - mark as required with Undef.
                crate::ir::core::Expr::Literal(crate::ir::core::Literal::Undef, span)
            }
        }
    };

    Ok(Some(KwParam::new(name, default, type_annotation, span)))
}

/// Parse a keyword parameter from a KeywordArgument node (from call expression parser).
/// KeywordArgument { Identifier name, Expression value } - used for `x=1` or shorthand `x` (which becomes `x=x`)
pub(super) fn parse_kwparam_from_keyword_arg<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<Option<KwParam>> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

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

    // The parser may represent bare shorthand `f(; x)` as a KeywordArgument
    // with repeated identifier children. Only that no-`=` form is required;
    // an explicit `x=x` default must resolve the outer binding (Issue #8378).
    let is_shorthand = !walker.text(&node).contains('=')
        && walker.kind(&value_node) == NodeKind::Identifier
        && walker.text(&value_node) == name;

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
    let named = walker.named_children_vec(&node);

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
            NodeKind::Identifier if name.is_none() => {
                name = Some(walker.text(child).to_string());
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
                let typed_children = walker.named_children_vec(child);
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
    let named = walker.named_children_vec(&node);

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
            NodeKind::Identifier if name.is_none() => {
                name = Some(walker.text(child).to_string());
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
                let typed_children = walker.named_children_vec(child);
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
pub fn parse_parameter<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<TypedParam> {
    let span = walker.span(&node);

    if let Some(param) = parse_destructuring_parameter(walker, node)? {
        return Ok(param);
    }

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
        NodeKind::MacroCall => {
            // Argument-position specialization annotations such as
            // `f(@nospecialize(x)) = ...` or `f(@specialize(x::T)) = ...`
            // (Issue #5122). Upstream Julia accepts `@nospecialize`/`@specialize`
            // as argument annotations that control inference specialization while
            // the parameter still binds the value with its declared type. The
            // full-form `function f(@nospecialize x) ... end` already unwraps to a
            // plain parameter in the parser; this handles the short-form path where
            // the annotation survives as a MacrocallExpression inside the argument
            // list. SubsetJuliaVM has no JIT/specialization, so the annotation is a
            // no-op: unwrap to the inner parameter node and parse that.
            parse_specialization_annotated_parameter(walker, node)
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

/// Parse a parameter wrapped in a `@nospecialize` / `@specialize` argument
/// annotation, e.g. `@nospecialize(x)` or `@specialize(x::Number)` (Issue #5122).
///
/// The macro node has a leading `MacroIdentifier` naming the macro followed by
/// the actual parameter node (an `Identifier`, `TypedExpression`, etc.). Only the
/// specialization-control macros are accepted here; any other macro in argument
/// position is rejected with a precise span. The annotation has no runtime effect
/// in SubsetJuliaVM (no inference specialization), so the inner parameter is
/// parsed and returned unchanged.
fn parse_specialization_annotated_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypedParam> {
    let span = walker.span(&node);

    let macro_name = walker
        .find_child(&node, NodeKind::MacroIdentifier)
        .map(|ident| walker.text(&ident).trim_start_matches('@').to_string());

    match macro_name.as_deref() {
        Some("nospecialize") | Some("specialize") => {}
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::Other(
                    "only @nospecialize/@specialize are supported as argument annotations"
                        .to_string(),
                ),
                span,
            ))
        }
    }

    // The inner parameter is the first named child that is not the macro name.
    // tree-sitter wraps the arguments in a MacroArgumentList; the Pure Rust parser
    // emits them as direct children. Handle both by unwrapping a MacroArgumentList
    // when present.
    let inner = if let Some(arg_list) = walker.find_child(&node, NodeKind::MacroArgumentList) {
        walker.named_children(&arg_list).next()
    } else {
        walker
            .named_children(&node)
            .find(|child| walker.kind(child) != NodeKind::MacroIdentifier)
    };

    let inner = inner.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(
                "@nospecialize/@specialize argument annotation requires a parameter".to_string(),
            ),
            span,
        )
    })?;

    parse_parameter(walker, inner)
}

/// Parse a typed parameter (x::Int64).
/// Also handles varargs typed parameters (x::Int64...) when the parser emits them as Parameter nodes.
pub(super) fn parse_typed_parameter<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<TypedParam> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);
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
    let text_type_annotation = if has_type_annotation {
        parameter_type_text(node_text)
            .filter(|text| text.contains('{'))
            .map(|text| parse_type_name(&text, span))
            .transpose()?
            .flatten()
    } else {
        None
    };
    let flat_type_annotation = if has_type_annotation && !has_default {
        flat_parametric_parameter_type_text(walker, &named, is_anonymous)
            .map(|text| parse_type_name(&text, span))
            .transpose()?
            .flatten()
    } else {
        None
    };

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
                let type_name = type_name_text(walker, &child);
                type_annotation = parse_type_name(&type_name, walker.span(&child))?;
            }
            NodeKind::SplatExpression => {
                if let Some(type_name) = splat_element_type_name(walker, &child) {
                    type_annotation = parse_type_name(&type_name, walker.span(&child))?;
                }
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
                            let mut type_name = type_name_text(walker, &type_child);
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
    if let Some(text_ty) = text_type_annotation {
        type_annotation = Some(text_ty);
    }
    if let Some(flat_ty) = flat_type_annotation {
        type_annotation = Some(flat_ty);
    }

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
    let named = walker.named_children_vec(&node);

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
                            let type_name = type_name_text(walker, &type_child);
                            type_annotation =
                                parse_type_name(&type_name, walker.span(&type_child))?;
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
                let type_name = type_name_text(walker, &type_node);
                type_annotation = parse_type_name(&type_name, walker.span(&type_node))?;
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
    let named = walker.named_children_vec(&node);

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
            let inner_children = walker.named_children_vec(&inner);
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
                        let type_name = type_name_text(walker, child);
                        type_annotation = parse_type_name(&type_name, walker.span(child))?;
                        break;
                    }
                    NodeKind::SplatExpression => {
                        if let Some(type_name) = splat_element_type_name(walker, child) {
                            type_annotation = parse_type_name(&type_name, walker.span(child))?;
                            break;
                        }
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
                let type_name = type_name_text(walker, &child);
                let type_annotation = parse_type_name(&type_name, walker.span(&child))?;
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
    span: crate::parser::span::Span,
) -> LowerResult<Option<JuliaType>> {
    // Issue #10407: a `where`-clause binder lexically shadows a same-named
    // builtin/global type over the whole method signature (upstream Julia
    // rebinds `Float64` in `h(x::Float64) where {Float64}` to a fresh method
    // TypeVar). Resolve the name as a plain `Struct` here so the post-pass
    // `convert_params_with_type_vars` rewrites it into the TypeVar with the
    // binder's declared bounds, instead of freezing it to the builtin type.
    // The check is name-based (exact identifier match) so `x::Int` under
    // `where {Int64}` still resolves to the global `Int` alias — the binder
    // shadows the *name* as written, not the type it aliases.
    if crate::lowering::type_alias::is_scoped_type_param(type_name) {
        return Ok(Some(JuliaType::Struct(type_name.to_string())));
    }
    // Issue #10942: the lexical shadow must also hold INSIDE composite
    // annotations. `f(::Type{Float64}) where Float64` rebinds the spelling
    // `Float64` over the whole signature, so `Type{Float64}` must lower to
    // `Type{TypeVar(Float64)}` — not freeze to the builtin — exactly as
    // `Type{T}` does for an ordinary binder name. The structural path only
    // activates when the annotation actually spells an active scoped binder,
    // so signatures without a colliding binder are untouched.
    if contains_shadowed_builtin_identifier(type_name) {
        let use_position = signature_source_position(span)?;
        return Ok(Some(parse_scoped_type_expr(type_name, use_position)));
    }
    // Issue #5055: expand user-defined type aliases (e.g. `MyVec{Int}` ->
    // `Vector{Int}`) before parsing so parameter annotations dispatch on the
    // aliased target type.
    let expanded = crate::lowering::type_alias::expand_for_signature(
        type_name,
        signature_source_position(span)?,
    );
    // Use from_name_or_struct to treat unknown types as user-defined structs
    Ok(Some(JuliaType::from_name_or_struct(&expanded)))
}

/// Whether `text` spells an active lexically scoped type parameter (a `where`
/// binder of the signature currently being parsed) whose name COLLIDES with a
/// known builtin/static type name, as a standalone identifier token anywhere —
/// including inside `{...}` type arguments (Issue #10942).
///
/// Ordinary binder names (`T`, `N`) are unknown to the static tables, so the
/// existing grammar already keeps them symbolic (including special forms like
/// the `Tuple{Vararg{T,N}}` → `NTuple` translation of Issue #4841); only a
/// builtin-colliding spelling would be frozen and needs the structural path.
fn contains_shadowed_builtin_identifier(text: &str) -> bool {
    identifier_tokens(text).any(is_shadowed_builtin_identifier)
}

/// One identifier token that is both an active scoped binder AND a known
/// builtin/static type name (the freeze hazard of Issue #10942).
fn is_shadowed_builtin_identifier(token: &str) -> bool {
    crate::lowering::type_alias::is_scoped_type_param(token)
        && JuliaType::from_name(token).is_some()
}

/// Iterate the identifier tokens of a type-name string (`Type{Float64}` →
/// `Type`, `Float64`). Numeric tokens (e.g. the `2` of `Array{T, 2}`) are
/// skipped: they can never name a `where` binder.
fn identifier_tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '!'))
        .filter(|tok| !tok.is_empty() && !tok.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// Parse a type-name string in which at least one identifier is an active
/// scoped `where` binder, keeping every binder spelling lexical so the
/// post-pass `convert_params_with_type_vars` (and, for struct-string forms,
/// the runtime dispatcher's name-based TypeVar matching) rebinds it to the
/// method TypeVar instead of the same-spelled builtin (Issue #10942).
///
/// Subtrees that do not mention any scoped binder delegate to the ordinary
/// alias-expansion + [`JuliaType::from_parametric_arg`] grammar, so behavior
/// is unchanged wherever no shadowing is in effect. The composite arms mirror
/// the parametric arms of `JuliaType::from_name` that would otherwise resolve
/// their type arguments through static builtin tables.
fn signature_source_position(
    span: crate::parser::span::Span,
) -> LowerResult<crate::lowering::type_alias::SourcePosition> {
    crate::lowering::type_alias::current_source_position(span.start).ok_or_else(|| {
        internal_lowering_error(
            span,
            "function signature parsing requires an active source identity",
        )
    })
}

fn parse_scoped_type_expr(
    name: &str,
    use_position: crate::lowering::type_alias::SourcePosition,
) -> JuliaType {
    let name = name.trim();
    if crate::lowering::type_alias::is_scoped_type_param(name) {
        return JuliaType::Struct(name.to_string());
    }
    if !contains_shadowed_builtin_identifier(name) {
        let expanded = crate::lowering::type_alias::expand_for_signature(name, use_position);
        return JuliaType::from_parametric_arg(&expanded);
    }
    let Some((base, args)) = split_parametric_application(name) else {
        // Not a plain `Base{args}` application (e.g. a variance bound whose
        // spelling mentions a binder). Keep the pre-#10942 grammar.
        let expanded = crate::lowering::type_alias::expand(name);
        return JuliaType::from_parametric_arg(&expanded);
    };
    let parsed: Vec<JuliaType> = args
        .iter()
        .map(|arg| parse_scoped_type_expr(arg, use_position))
        .collect();
    match base {
        "Type" if parsed.len() == 1 => {
            let mut parsed = parsed;
            JuliaType::TypeOf(Box::new(parsed.swap_remove(0)))
        }
        "Vector" if parsed.len() == 1 => {
            let mut parsed = parsed;
            JuliaType::VectorOf(Box::new(parsed.swap_remove(0)))
        }
        "Matrix" if parsed.len() == 1 => {
            let mut parsed = parsed;
            JuliaType::MatrixOf(Box::new(parsed.swap_remove(0)))
        }
        "Array" if parsed.len() == 2 => {
            // Array{T, N}: rank 1/2 fold to the Vector/Matrix aliases like
            // `JuliaType::from_name`; other ranks keep the struct-string form.
            let mut parsed = parsed;
            let dim = args[1].trim();
            let elem = Box::new(parsed.swap_remove(0));
            match dim {
                "1" => JuliaType::VectorOf(elem),
                "2" => JuliaType::MatrixOf(elem),
                _ => JuliaType::Struct(format!("Array{{{}, {}}}", elem.name(), dim)),
            }
        }
        "Tuple" => JuliaType::TupleOf(parsed),
        "Union" => JuliaType::Union(parsed),
        _ => {
            // Other bases (Complex{...}, Dict{...}, user parametric types)
            // keep the struct-string spelling — the same form an ordinary
            // binder produces (`Complex{T}`), which dispatch matches against
            // the method's type parameters by name.
            let args = parsed
                .iter()
                .map(|ty| ty.name())
                .collect::<Vec<_>>()
                .join(", ");
            JuliaType::Struct(format!("{}{{{}}}", base, args))
        }
    }
}

/// Split `Base{arg1, arg2, ...}` into its base name and top-level type
/// arguments, respecting nested braces. Returns `None` when `name` is not a
/// simple parametric application of an identifier base.
fn split_parametric_application(name: &str) -> Option<(&str, Vec<&str>)> {
    let open = name.find('{')?;
    if !name.ends_with('}') || open == 0 {
        return None;
    }
    let base = &name[..open];
    if !base
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '!' || c == '.')
    {
        return None;
    }
    let inner = &name[open + 1..name.len() - 1];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, c) in inner.char_indices() {
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(inner[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        args.push(last);
    }
    if args.is_empty() {
        return None;
    }
    Some((base, args))
}

fn type_name_text<'a>(walker: &CstWalker<'a>, node: &Node<'a>) -> String {
    if walker.kind(node) != NodeKind::ParametrizedTypeExpression {
        return walker.text(node).to_string();
    }

    let children = walker.named_children_vec(node);
    let Some((base, args)) = children.split_first() else {
        return walker.text(node).to_string();
    };
    if args.is_empty() {
        return walker.text(node).to_string();
    }

    let base = walker.text(base);
    let args = args
        .iter()
        .map(|arg| match walker.kind(arg) {
            NodeKind::ParametrizedTypeExpression => type_name_text(walker, arg),
            _ => walker.text(arg).to_string(),
        })
        .collect::<Vec<_>>();
    format!("{}{{{}}}", base, args.join(", "))
}

fn splat_element_type_name<'a>(walker: &CstWalker<'a>, node: &Node<'a>) -> Option<String> {
    let child = walker.named_children(node).next()?;
    match walker.kind(&child) {
        NodeKind::Identifier | NodeKind::ParametrizedTypeExpression => {
            Some(type_name_text(walker, &child))
        }
        _ => {
            let text = walker.text(&child).trim().trim_end_matches("...").trim();
            (!text.is_empty()).then(|| text.to_string())
        }
    }
}

fn parameter_type_text(node_text: &str) -> Option<String> {
    let (_, rhs) = node_text.split_once("::")?;
    let type_part = rhs
        .split_once('=')
        .map_or(rhs, |(before_default, _)| before_default);
    let type_part = type_part.trim().trim_end_matches("...").trim();
    (!type_part.is_empty()).then(|| type_part.to_string())
}

fn flat_parametric_parameter_type_text<'a>(
    walker: &CstWalker<'a>,
    children: &[Node<'a>],
    is_anonymous: bool,
) -> Option<String> {
    let start = if is_anonymous { 0 } else { 1 };
    let base = children.get(start)?;
    if walker.kind(base) != NodeKind::Identifier {
        return None;
    }
    let args = children.get(start + 1..)?;
    if args.is_empty() {
        return None;
    }
    if !args
        .iter()
        .all(|arg| is_flat_parametric_type_arg(walker.kind(arg)))
    {
        return None;
    }

    let base = walker.text(base);
    let args = args
        .iter()
        .map(|arg| match walker.kind(arg) {
            NodeKind::ParametrizedTypeExpression => type_name_text(walker, arg),
            _ => walker.text(arg).to_string(),
        })
        .collect::<Vec<_>>();
    Some(format!("{}{{{}}}", base, args.join(", ")))
}

fn is_flat_parametric_type_arg(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Identifier
            | NodeKind::UnaryExpression
            | NodeKind::ParametrizedTypeExpression
            | NodeKind::SubtypeExpression
            | NodeKind::BinaryExpression
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    fn lower_first_function(source: &str) -> crate::ir::core::Function {
        let mut parser = Parser::new().unwrap();
        let parsed = parser.parse(source).unwrap();
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parsed).unwrap();
        let func = program.functions.into_iter().next().unwrap();
        std::sync::Arc::try_unwrap(func).unwrap_or_else(|arc| (*arc).clone())
    }

    #[test]
    fn typed_vararg_where_param_survives_short_and_full_form_issue_8565() {
        for source in [
            "diag8565(xs::T...) where {T} = 1",
            "function diag8565(xs::T...) where {T}\n1\nend",
        ] {
            let func = lower_first_function(source);
            let param = func.params.first().unwrap();
            assert!(param.is_varargs, "{source}");
            assert!(
                matches!(
                    param.type_annotation.as_ref(),
                    Some(JuliaType::TypeVar(name, None)) if name == "T"
                ),
                "{source}: {:?}",
                param.type_annotation
            );
        }
    }

    /// A `where`-clause binder colliding with a builtin type name must lower
    /// the parameter annotation as the method-local TypeVar, not the builtin
    /// concrete type — `h(x::Float64) where {Float64}` behaves like
    /// `h(x::T) where {T}` (Issue #10407). A non-binder annotation that merely
    /// ALIASES the shadowed name (`x::Int` under `where {Int64}`) must keep
    /// resolving to the builtin: the shadow is name-based, not type-based.
    #[test]
    fn where_binder_shadowing_builtin_type_name_lowers_as_typevar_issue_10407() {
        for source in [
            "h10407(x::Float64) where {Float64} = Float64(2)",
            "function h10407(x::Float64) where {Float64}\nFloat64(2)\nend",
        ] {
            let func = lower_first_function(source);
            assert!(
                func.type_params.iter().any(|tp| tp.name == "Float64"),
                "{source}: {:?}",
                func.type_params
            );
            let param = func.params.first().unwrap();
            assert!(
                matches!(
                    param.type_annotation.as_ref(),
                    Some(JuliaType::TypeVar(name, None)) if name == "Float64"
                ),
                "{source}: {:?}",
                param.type_annotation
            );
        }

        // Name-based shadowing only: `Int` still resolves to the builtin
        // Int64 even though the binder shadows the NAME `Int64`.
        let func = lower_first_function("hint10407(x::Int) where {Int64} = 1");
        assert!(
            matches!(
                func.params.first().unwrap().type_annotation.as_ref(),
                Some(JuliaType::Int64)
            ),
            "{:?}",
            func.params.first().unwrap().type_annotation
        );

        // Bounded colliding binder keeps its declared upper bound.
        let func =
            lower_first_function("hb10407(x::Float64) where {Float64<:Integer} = Float64(2)");
        assert!(
            matches!(
                func.params.first().unwrap().type_annotation.as_ref(),
                Some(JuliaType::TypeVar(name, Some(bound))) if name == "Float64" && bound == "Integer"
            ),
            "{:?}",
            func.params.first().unwrap().type_annotation
        );
    }
}
