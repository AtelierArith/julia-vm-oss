//! Lowering for primitive type definitions (`primitive type Name Bits end`).

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::PrimitiveTypeDef;
use crate::lowering::LowerResult;
use crate::parser::cst::{CstWalker, Node, NodeKind};

/// Lower a primitive type definition node to a `PrimitiveTypeDef` IR.
///
/// Handles:
/// - `primitive type MyBits 8 end` (no explicit supertype, defaults to `Any`)
/// - `primitive type MyU8 <: Unsigned 8 end` (explicit supertype)
///
/// The parser (`parse_primitive_definition`) emits the named children in order
/// `[name, (supertype?), bits]`, where `bits` is always the last named child and
/// the optional supertype is the middle named child.
///
/// Matching upstream Julia, the bit count must be a positive multiple of 8.
pub fn lower_primitive_definition<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<PrimitiveTypeDef> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

    // The last named child is always the bit-size expression.
    let bits_node = named.last().ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing primitive type bit size".to_string()),
            span,
        )
    })?;

    let bits = parse_bits(walker, bits_node, span)?;

    // Everything before the bit-size node describes name and (optional) supertype.
    let head = &named[..named.len() - 1];
    let mut name: Option<String> = None;
    let mut parent: Option<String> = None;

    for child in head {
        match walker.kind(child) {
            NodeKind::Identifier => {
                // First identifier is the name, second (if present) is the parent.
                if name.is_none() {
                    name = Some(walker.text(child).to_string());
                } else if parent.is_none() {
                    parent = Some(walker.text(child).to_string());
                }
            }
            NodeKind::SubtypeExpression | NodeKind::BinaryExpression => {
                // `Name <: Parent` collapsed into a single subtype node.
                let (n, p) = parse_subtype_head(walker, *child);
                if name.is_none() {
                    name = n;
                }
                if parent.is_none() {
                    parent = p;
                }
            }
            _ => {
                // Fallback: try to recover name/parent from raw text.
                let text = walker.text(child).trim();
                if let Some((n, p)) = parse_from_text(text) {
                    if name.is_none() {
                        name = Some(n);
                    }
                    if parent.is_none() {
                        parent = p;
                    }
                }
            }
        }
    }

    let name = name.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other("missing primitive type name".to_string()),
            span,
        )
    })?;

    Ok(PrimitiveTypeDef {
        name,
        parent,
        bits,
        span,
    })
}

/// Parse the integer bit count from the bit-size node and validate it.
fn parse_bits<'a>(
    walker: &CstWalker<'a>,
    node: &Node<'a>,
    span: crate::span::Span,
) -> LowerResult<u32> {
    let text = walker.text(node).trim();
    // Strip an optional leading `+`; primitive bit counts are always positive.
    let digits = text.strip_prefix('+').unwrap_or(text);
    let bits: i64 = digits.parse().map_err(|_| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(format!(
                "invalid number of bits in primitive type declaration: {text}"
            )),
            span,
        )
    })?;

    // Match upstream Julia: the bit count must be a positive multiple of 8.
    // `u32::try_from` also rejects negatives and values that do not fit.
    let bits = u32::try_from(bits).ok().filter(|b| *b > 0 && b % 8 == 0);
    bits.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(format!(
                "invalid number of bits in primitive type definition; expected a positive multiple of 8, got {text}"
            )),
            span,
        )
    })
}

/// Parse a `Name <: Parent` subtype head, returning `(name, parent)`.
fn parse_subtype_head<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> (Option<String>, Option<String>) {
    let mut name: Option<String> = None;
    let mut parent: Option<String> = None;
    for (i, child) in walker.named_children(&node).iter().enumerate() {
        if matches!(walker.kind(child), NodeKind::Identifier) {
            if i == 0 || name.is_none() {
                name = Some(walker.text(child).to_string());
            } else {
                parent = Some(walker.text(child).to_string());
            }
        }
    }
    (name, parent)
}

/// Recover `(name, parent)` from raw text (fallback path).
fn parse_from_text(s: &str) -> Option<(String, Option<String>)> {
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
    if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some((s.to_string(), None));
    }
    None
}
