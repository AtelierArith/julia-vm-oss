//! @enum macro implementation for statement context.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::Stmt;
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
    _lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    // Get the macro arguments
    let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
        walker.named_children(&args_node)
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
            let children: Vec<Node<'a>> = walker.named_children(&first_arg);
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

    for (idx, arg) in args.iter().enumerate().skip(1) {
        let arg_kind = walker.kind(arg);
        let arg_span = walker.span(arg);

        let (member_name, member_value) = match arg_kind {
            NodeKind::Identifier => {
                // member - auto-increment value
                let name = walker.text(arg).to_string();
                let value = next_value;
                next_value = value + 1;
                (name, value)
            }
            NodeKind::Assignment => {
                // member=value
                let children: Vec<Node<'a>> = walker.named_children(arg);
                if children.len() >= 2 {
                    let name = walker.text(&children[0]).to_string();
                    let value_text = walker.text(&children[1]);
                    let value: i64 = value_text.parse().map_err(|_| {
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, arg_span)
                            .with_hint(format!(
                                "@enum member value must be an integer: {}",
                                value_text
                            ))
                    })?;
                    next_value = value + 1;
                    (name, value)
                } else {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::MacroCall,
                        arg_span,
                    )
                    .with_hint("@enum member assignment must be name=value"));
                }
            }
            NodeKind::BinaryExpression => {
                // Could be member = value as binary expression
                let children: Vec<Node<'a>> = walker.named_children(arg);
                if children.len() >= 2 {
                    let name = walker.text(&children[0]).to_string();
                    let value_text = walker.text(&children[1]);
                    let value: i64 = value_text.parse().map_err(|_| {
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, arg_span)
                            .with_hint(format!(
                                "@enum member value must be an integer: {}",
                                value_text
                            ))
                    })?;
                    next_value = value + 1;
                    (name, value)
                } else {
                    return Err(UnsupportedFeature::new(
                        UnsupportedFeatureKind::MacroCall,
                        arg_span,
                    )
                    .with_hint(format!(
                        "@enum argument {} must be a member name or name=value",
                        idx
                    )));
                }
            }
            _ => {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, arg_span).with_hint(
                        format!("@enum argument {} must be a member name or name=value", idx),
                    ),
                );
            }
        };

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

    Ok(Stmt::EnumDef { enum_def, span })
}
