//! @enum macro implementation for statement context.

use std::collections::{HashMap, HashSet};

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal, RuntimeNominalDef, Stmt};
use crate::lowering::LambdaContext;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

// ==================== @enum Macro Implementation ====================

/// Lower @enum macro - define an enumerated type.
///
/// Usage:
/// - `@enum Color red green blue` - auto-incremented values (0, 1, 2)
/// - `@enum Color red=1 green=2 blue=10` - explicit values
/// - `@enum Color::Int8 red green blue` - with base type
///
/// Creates:
/// - An enum type with the given name
/// - Named constants for each member
pub(super) fn lower_enum_macro_with_ctx<'a>(
    walker: &CstWalker<'a>,
    _node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    if lambda_ctx.in_function_body() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("type declarations are not allowed inside a function"),
        );
    }
    // Get the macro arguments
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children_vec(&args_node)
    } else {
        direct_args.to_vec()
    };

    if args.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@enum requires a type name and at least one member"),
        );
    }

    // Parse the first argument: TypeName or TypeName::BaseType
    let first_arg = args[0];
    let first_kind = walker.kind(&first_arg);

    let (enum_name, base_type) = match first_kind {
        NodeKind::Identifier => {
            let name = walker.text(&first_arg).to_string();
            (name, "Int32".to_string())
        }
        NodeKind::TypedExpression => {
            // TypeName::BaseType
            let children: Vec<Node<'a>> = walker.named_children_vec(&first_arg);
            if children.len() >= 2 {
                let name = walker.text(&children[0]).to_string();
                let base = walker.text(&children[1]).to_string();
                (name, base)
            } else {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@enum type annotation must be TypeName::BaseType"),
                );
            }
        }
        _ => {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint("@enum first argument must be a type name"),
            );
        }
    };

    // Parse the member arguments
    let mut members = Vec::new();
    let mut next_value: i64 = 0;
    let mut seen_values = HashMap::<i64, String>::new();
    let mut seen_names = HashSet::<String>::new();
    let mut has_explicit_value = false;

    for (idx, arg) in args.iter().enumerate().skip(1) {
        let arg_kind = walker.kind(arg);
        let arg_span = walker.span(arg);

        let (member_name, member_value, is_explicit) = match arg_kind {
            NodeKind::Identifier => {
                // member - auto-increment value
                let name = walker.text(arg).to_string();
                let value = next_value;
                next_value = value + 1;
                (name, value, false)
            }
            NodeKind::Assignment | NodeKind::BinaryExpression => {
                // member=value. The CST exposes the `=` operator as a *named*
                // child for assignment nodes, so `named_children` yields
                // `[name, =, value]`; locate the LHS/RHS by scanning for the
                // operator rather than indexing positionally (Issue #5139).
                let Some((name_node, value_node)) = enum_member_lhs_rhs(walker, arg) else {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::MacroCall,
                        arg_span,
                    )
                    .with_hint(format!(
                        "@enum argument {} must be a member name or name=value",
                        idx
                    )));
                };
                let name = walker.text(&name_node).to_string();
                let value_text = walker.text(&value_node);
                let value: i64 = value_text.trim().parse().map_err(|_| {
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, arg_span).with_hint(
                        format!("@enum member value must be an integer: {}", value_text),
                    )
                })?;
                next_value = value + 1;
                (name, value, true)
            }
            _ => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, arg_span).with_hint(
                        format!("@enum argument {} must be a member name or name=value", idx),
                    ),
                );
            }
        };

        has_explicit_value |= is_explicit;
        if has_explicit_value {
            if let Some(previous_name) = seen_values.get(&member_value) {
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::MacroCall,
                    arg_span,
                )
                .with_hint(format!(
                    "@enum members {member_name} and {previous_name} have duplicate value {member_value} (Issue #11666)"
                )));
            }
        }
        if !seen_names.insert(member_name.clone()) {
            return Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, arg_span).with_hint(
                    format!("@enum member name {member_name} is not unique (Issue #11666)"),
                ),
            );
        }
        seen_values.insert(member_value, member_name.clone());

        members.push(crate::ir::core::EnumMember {
            name: member_name,
            value: member_value,
            span: arg_span,
        });
    }

    if members.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@enum requires at least one member"),
        );
    }

    // Create the EnumDef
    let enum_def = crate::ir::core::EnumDef {
        name: enum_name,
        base_type,
        members,
        span,
    };

    if lambda_ctx.inside_top_level_for() {
        return Ok(Stmt::Expr {
            expr: Expr::Literal(Literal::Nothing, span),
            span,
        });
    }

    if lambda_ctx.inside_top_level_control_flow() {
        let mut definition = RuntimeNominalDef::Enum(enum_def);
        let mut span = span;
        span.definition_order = lambda_ctx.stamp_runtime_nominal_definition(&mut definition);
        Ok(Stmt::RuntimeNominalDef {
            definition,
            published_members: None,
            span,
        })
    } else {
        Ok(Stmt::EnumDef {
            enum_def,
            published_members: None,
            span,
        })
    }
}

/// Extract the `(name, value)` operand nodes from a `member = value` enum
/// member argument.
///
/// Mirrors the assignment-splitting logic used elsewhere in lowering: scan the
/// full (named + anonymous) children for the `=` operator and return the
/// surrounding nodes. This is robust to the grammar exposing the `=` token as a
/// named child, which previously caused positional indexing to read the
/// operator itself as the member value (Issue #5139).
fn enum_member_lhs_rhs<'a>(
    walker: &CstWalker<'a>,
    node: &Node<'a>,
) -> Option<(Node<'a>, Node<'a>)> {
    let all_children = walker.children(node);
    for (i, child) in all_children.iter().enumerate() {
        if walker.text(child) == "=" && i > 0 && i + 1 < all_children.len() {
            return Some((all_children[i - 1], all_children[i + 1]));
        }
    }

    // Fallback for grammars that omit the operator node: take the first and
    // last named children as `name` and `value`.
    let named = walker.named_children_vec(node);
    if named.len() >= 2 {
        Some((named[0], named[named.len() - 1]))
    } else {
        None
    }
}
