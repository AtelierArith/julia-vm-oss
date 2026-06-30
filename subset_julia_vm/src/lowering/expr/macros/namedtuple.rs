//! `@NamedTuple` macro expansion (Issue #5120).
//!
//! Mirrors upstream Julia's `macro NamedTuple(ex)` in `base/namedtuple.jl`,
//! which turns
//!
//! ```julia
//! @NamedTuple{a::Int, b::String}
//! @NamedTuple begin
//!     a::Int
//!     b::String
//! end
//! ```
//!
//! into the type `NamedTuple{(:a, :b), Tuple{Int64, String}}`. A field
//! declaration without a `::Type` annotation defaults to `Any`.
//!
//! In SubsetJuliaVM the canonical printed form of a named-tuple type is
//! `@NamedTuple{a::Int64, b::String}`, which is also exactly what
//! `typeof((a=1, b="hi"))` produces. To make the macro result interchangeable
//! with runtime named-tuple types (so `isa`, field access and equality behave
//! as in upstream for the cases SubsetJuliaVM supports), the expansion builds
//! that canonical string and resolves it through the `TypeOf` builtin.
//!
//! Full type-level dispatch on `NamedTuple{names, T}` (subtype/method
//! matching) is tracked separately by Issue #5063; this macro provides the
//! surface syntax and the canonical type object.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

/// Canonicalize a field type name to the form SubsetJuliaVM prints for runtime
/// named-tuple types (e.g. `Int` -> `Int64`, `Float64` -> `Float64`).
///
/// For type names SubsetJuliaVM does not recognize statically (local type
/// variables, user struct names, nested parametric types) we keep the written
/// text so the type object is still constructed; matching such fields against
/// runtime named tuples is part of Issue #5063.
fn canonical_field_type_name(text: &str) -> String {
    match crate::types::JuliaType::from_name(text) {
        Some(jt) => jt.name().to_string(),
        None => text.to_string(),
    }
}

/// Collect the field declaration nodes from the macro argument.
///
/// Accepts the braces form (`@NamedTuple{...}`, a `CurlyExpression`) and the
/// block form (`@NamedTuple begin ... end`). In the block form the parser nests
/// the field declarations one level deeper (a `begin` block wraps an inner
/// block), so a lone `Block` child is unwrapped. Line / comment nodes are
/// ignored, matching upstream's `filter(e -> !(e isa LineNumberNode), ...)`.
fn collect_field_decls<'a>(walker: &CstWalker<'a>, arg: Node<'a>) -> Vec<Node<'a>> {
    let children: Vec<Node<'a>> = walker
        .named_children(&arg)
        .into_iter()
        .filter(|child| {
            !matches!(
                walker.kind(child),
                NodeKind::LineComment | NodeKind::BlockComment
            )
        })
        .collect();

    // `@NamedTuple begin ... end` parses as a `begin` block whose single child
    // is the inner statement block; descend into it to reach the declarations.
    if children.len() == 1 && walker.kind(&children[0]) == NodeKind::Block {
        return collect_field_decls(walker, children[0]);
    }

    children
}

/// Lower a `@NamedTuple{...}` / `@NamedTuple begin ... end` macro call to the
/// canonical named-tuple type expression.
pub(crate) fn lower_namedtuple_macro_expr<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
) -> LowerResult<Expr> {
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@NamedTuple expects a single {...} or begin...end argument \
                 (e.g. @NamedTuple{a::Int, b::String})",
            ),
        );
    }

    let arg = args[0];
    match walker.kind(&arg) {
        NodeKind::CurlyExpression | NodeKind::Block => {}
        _ => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                    "@NamedTuple expects {...} or begin...end \
                     (e.g. @NamedTuple{a::Int, b::String})",
                ),
            );
        }
    }

    let decls = collect_field_decls(walker, arg);

    let mut fields: Vec<String> = Vec::with_capacity(decls.len());
    for decl in decls {
        match walker.kind(&decl) {
            // `name::Type`
            NodeKind::TypedExpression => {
                let children = walker.named_children(&decl);
                if children.len() != 2 {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                            "@NamedTuple field declarations must be `name` or `name::type`",
                        ),
                    );
                }
                let name = walker.text(&children[0]).to_string();
                let type_text = walker.text(&children[1]);
                let canonical = canonical_field_type_name(type_text);
                // Upstream prints an explicitly `Any`-typed field without the
                // `::Any` suffix (`@NamedTuple{a::Int64, b}`), so collapse it to
                // the bare name to match.
                if canonical == "Any" {
                    fields.push(name);
                } else {
                    fields.push(format!("{}::{}", name, canonical));
                }
            }
            // bare `name` -> defaults to Any, displayed as the bare name
            NodeKind::Identifier => {
                fields.push(walker.text(&decl).to_string());
            }
            _ => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                        "@NamedTuple must contain a sequence of `name` or `name::type` expressions",
                    ),
                );
            }
        }
    }

    // Build the canonical `@NamedTuple{a::Int64, b::String}` string. `TypeOf`
    // parses this back into the same named-tuple type SubsetJuliaVM uses for
    // `typeof((a=1, b="hi"))`, so the macro result is interchangeable with
    // runtime named-tuple types.
    let type_name = format!("@NamedTuple{{{}}}", fields.join(", "));
    Ok(Expr::Builtin {
        name: crate::ir::core::BuiltinOp::TypeOf,
        args: vec![Expr::Literal(Literal::Str(type_name), span)],
        span,
    })
}

#[cfg(test)]
mod tests {
    use crate::ir::core::{BuiltinOp, Expr, Literal, Stmt};
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    /// Lower `source` and return the canonical type-name string produced by the
    /// `@NamedTuple` macro (the argument of the emitted `TypeOf` builtin).
    fn namedtuple_type_name(source: &str) -> String {
        let mut parser = Parser::new().expect("parser");
        let parse_outcome = parser.parse(source).expect("parse");
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(parse_outcome).expect("lower");

        fn find_typeof_str(expr: &Expr) -> Option<String> {
            if let Expr::Builtin {
                name: BuiltinOp::TypeOf,
                args,
                ..
            } = expr
            {
                if let Some(Expr::Literal(Literal::Str(s), _)) = args.first() {
                    return Some(s.clone());
                }
            }
            None
        }

        for stmt in &program.main.stmts {
            if let Stmt::Expr { expr, .. } = stmt {
                if let Some(s) = find_typeof_str(expr) {
                    return s;
                }
            }
        }
        panic!("no TypeOf builtin produced for source: {source}");
    }

    #[test]
    fn braces_form_canonicalizes_field_types() {
        // `Int` is canonicalized to `Int64`, matching the runtime named-tuple type.
        assert_eq!(
            namedtuple_type_name("@NamedTuple{a::Int, b::String}"),
            "@NamedTuple{a::Int64, b::String}"
        );
    }

    #[test]
    fn omitted_field_type_renders_bare_name() {
        // A field without `::Type` defaults to Any and is printed without `::Any`.
        assert_eq!(
            namedtuple_type_name("@NamedTuple{a::Int, b}"),
            "@NamedTuple{a::Int64, b}"
        );
        assert_eq!(
            namedtuple_type_name("@NamedTuple{a, b}"),
            "@NamedTuple{a, b}"
        );
    }

    #[test]
    fn empty_braces_form() {
        assert_eq!(namedtuple_type_name("@NamedTuple{}"), "@NamedTuple{}");
    }

    #[test]
    fn block_form_matches_braces_form() {
        assert_eq!(
            namedtuple_type_name("@NamedTuple begin\n    a::Int\n    b::String\nend"),
            "@NamedTuple{a::Int64, b::String}"
        );
    }
}
