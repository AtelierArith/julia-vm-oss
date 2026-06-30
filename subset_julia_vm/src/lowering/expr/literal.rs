//! Literal parsing and lowering.
//!
//! This module handles parsing of integer, float, string, and character literals.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Expr, Literal};
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node};
use crate::parser::span::Span;

use super::{lower_expr, lower_expr_with_ctx};

/// Parse an interpolation expression from text like "$(expr)" or "$var"
/// This handles the Pure Rust parser's leaf node format.
fn parse_interpolation_expr(
    text: &str,
    span: Span,
    _lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Option<Expr>> {
    // Skip the leading $
    let inner = text.strip_prefix('$').unwrap_or(text);

    // Check for parenthesized expression: $(expr)
    let expr_text =
        if let Some(inner_expr) = inner.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            inner_expr.trim()
        } else {
            // Simple variable: $var
            inner.trim()
        };

    if expr_text.is_empty() {
        return Ok(None);
    }

    // Parse and lower the expression
    // We need to re-parse this substring
    use crate::parser::parse_and_lower_expr;

    match parse_and_lower_expr(expr_text) {
        Ok(expr) => Ok(Some(expr)),
        Err(e) => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::Other(format!("interpolation parse error: {}", e)),
            span,
        )),
    }
}

#[derive(Debug, Clone)]
pub enum ParsedInt {
    I64(i64),
    I128(i128),
    BigInt(String),
}

/// Width tag for typed integer literals (hex `0x…`, binary `0b…`, octal `0o…`).
///
/// In Julia these literal forms produce unsigned integers whose bit width is
/// determined by the literal's *digit count* (hex/binary) or *bits required*
/// (octal). Decimal literals carry no width tag — they default to `Int64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedIntKind {
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    UInt128,
}

impl TypedIntKind {
    /// Constructor function name to call when lowering this typed literal.
    pub fn constructor_name(self) -> &'static str {
        match self {
            TypedIntKind::UInt8 => "UInt8",
            TypedIntKind::UInt16 => "UInt16",
            TypedIntKind::UInt32 => "UInt32",
            TypedIntKind::UInt64 => "UInt64",
            TypedIntKind::UInt128 => "UInt128",
        }
    }
}

/// Result of parsing an integer literal, including the typed-int width tag
/// for hex / binary / octal literals.
#[derive(Debug, Clone)]
pub struct ParsedIntWithKind {
    pub value: ParsedInt,
    /// `Some(kind)` for hex/binary/octal literals; `None` for decimal.
    pub kind: Option<TypedIntKind>,
}

/// Untyped integer-literal parser. Kept for the in-module test suite
/// that asserts the `ParsedInt` variant directly; production lowering
/// uses `parse_int_typed` (Issue #4927) to preserve hex/binary/octal
/// type tags.
#[cfg(test)]
pub fn parse_int(text: &str) -> Option<ParsedInt> {
    parse_int_typed(text).map(|p| p.value)
}

/// Parse an integer literal, preserving the typed-integer width tag for
/// hex / binary / octal forms (Issue #3559).
///
/// Width rules (matching `julia 1.12`):
/// - **Hex** (`0x…`): width by hex-digit count (excluding underscores). 1–2 → `UInt8`,
///   3–4 → `UInt16`, 5–8 → `UInt32`, 9–16 → `UInt64`, 17–32 → `UInt128`.
/// - **Binary** (`0b…`): width by bit count. 1–8 → `UInt8`, 9–16 → `UInt16`,
///   17–32 → `UInt32`, 33–64 → `UInt64`, 65–128 → `UInt128`.
/// - **Octal** (`0o…`): width by total bits encoded (3 × digit count, but
///   capped at the *minimum bits needed* for the leading digit so e.g. `0o000`
///   is `UInt8` even though 3 × 3 = 9). We compute the bit width as
///   `3 * (digits - 1) + leading_bits` where `leading_bits` is the bit width
///   of the leading nonzero digit (0 for all-zero literals).
/// - Decimal literals return `kind = None`.
pub fn parse_int_typed(text: &str) -> Option<ParsedIntWithKind> {
    let cleaned = text.replace('_', "");
    if let Some(hex) = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
    {
        let value = parse_int_radix(hex, 16)?;
        let kind = hex_width_from_digits(hex.len());
        Some(ParsedIntWithKind { value, kind })
    } else if let Some(bin) = cleaned
        .strip_prefix("0b")
        .or_else(|| cleaned.strip_prefix("0B"))
    {
        let value = parse_int_radix(bin, 2)?;
        let kind = binary_width_from_digits(bin.len());
        Some(ParsedIntWithKind { value, kind })
    } else if let Some(oct) = cleaned
        .strip_prefix("0o")
        .or_else(|| cleaned.strip_prefix("0O"))
    {
        let value = parse_int_radix(oct, 8)?;
        let kind = octal_width_from_digits(oct);
        Some(ParsedIntWithKind { value, kind })
    } else {
        let value = parse_int_decimal(&cleaned)?;
        Some(ParsedIntWithKind { value, kind: None })
    }
}

/// Hex digit count → typed width (Julia rule).
fn hex_width_from_digits(ndigits: usize) -> Option<TypedIntKind> {
    match ndigits {
        0 => None,
        1..=2 => Some(TypedIntKind::UInt8),
        3..=4 => Some(TypedIntKind::UInt16),
        5..=8 => Some(TypedIntKind::UInt32),
        9..=16 => Some(TypedIntKind::UInt64),
        17..=32 => Some(TypedIntKind::UInt128),
        // > 32 hex digits is not a valid typed literal in Julia — fall back
        // to default (no width tag).
        _ => None,
    }
}

/// Binary digit count → typed width (Julia rule).
fn binary_width_from_digits(ndigits: usize) -> Option<TypedIntKind> {
    match ndigits {
        0 => None,
        1..=8 => Some(TypedIntKind::UInt8),
        9..=16 => Some(TypedIntKind::UInt16),
        17..=32 => Some(TypedIntKind::UInt32),
        33..=64 => Some(TypedIntKind::UInt64),
        65..=128 => Some(TypedIntKind::UInt128),
        _ => None,
    }
}

/// Octal digit count → typed width (Julia rule).
///
/// Julia's octal width is computed from the *bits actually needed* by the
/// leading digit, not a flat 3 bits per digit: `0o000` is `UInt8` even though
/// 3×3=9, because the leading 0 contributes 0 bits. We compute
/// `3 * (digits-1) + leading_bits` where `leading_bits` is `ceil(log2(d+1))`
/// for the leading digit `d`.
fn octal_width_from_digits(text: &str) -> Option<TypedIntKind> {
    if text.is_empty() {
        return None;
    }
    let leading = text.chars().next()?;
    let leading_value = leading.to_digit(8)?;
    let leading_bits = if leading_value == 0 {
        0
    } else {
        // 1 → 1, 2..3 → 2, 4..7 → 3
        32 - leading_value.leading_zeros()
    } as usize;
    let total_bits = 3 * (text.len() - 1) + leading_bits;
    match total_bits {
        0..=8 => Some(TypedIntKind::UInt8),
        9..=16 => Some(TypedIntKind::UInt16),
        17..=32 => Some(TypedIntKind::UInt32),
        33..=64 => Some(TypedIntKind::UInt64),
        65..=128 => Some(TypedIntKind::UInt128),
        _ => None,
    }
}

fn parse_int_radix(text: &str, radix: u32) -> Option<ParsedInt> {
    if let Ok(v) = i64::from_str_radix(text, radix) {
        return Some(ParsedInt::I64(v));
    }
    if let Ok(v) = i128::from_str_radix(text, radix) {
        return Some(ParsedInt::I128(v));
    }
    num_bigint::BigInt::parse_bytes(text.as_bytes(), radix)
        .map(|v| ParsedInt::BigInt(v.to_string()))
}
fn parse_int_decimal(text: &str) -> Option<ParsedInt> {
    if let Ok(v) = text.parse::<i64>() {
        return Some(ParsedInt::I64(v));
    }
    if let Ok(v) = text.parse::<i128>() {
        return Some(ParsedInt::I128(v));
    }
    text.parse::<num_bigint::BigInt>()
        .ok()
        .map(|v| ParsedInt::BigInt(v.to_string()))
}

/// Parsed float result - either Float64 or Float32
#[derive(Debug, Clone, Copy)]
pub enum ParsedFloat {
    F64(f64),
    F32(f32),
}

/// Parse a float literal from text.
/// Supports:
/// - Standard floats: 1.0, .5, 1., 1e10, 1.0e-5
/// - Float32 literals: 1.0f0, 1f0, 1.5f-2 (f suffix means Float32)
/// - Hex floats: 0x1.8p3 (p exponent means power of 2)
/// - Underscore separators in all formats
pub fn parse_float(text: &str) -> Option<ParsedFloat> {
    let cleaned = text.replace('_', "");

    // Float32 suffix: 1.0f0, 1.5f-2, 1f0, etc.
    // Julia's 'f' means Float32, 'e' means Float64
    if let Some(idx) = cleaned.find(['f', 'F']) {
        // hex float (0x...) case: 'f' is part of mantissa, not a suffix
        if !cleaned.starts_with("0x") && !cleaned.starts_with("0X") {
            let (mantissa, exp_part) = cleaned.split_at(idx);
            if exp_part.len() > 1 {
                let exp_str = &exp_part[1..]; // skip 'f'
                if let (Ok(m), Ok(e)) = (mantissa.parse::<f64>(), exp_str.parse::<i32>()) {
                    let value = m * 10f64.powi(e);
                    return Some(ParsedFloat::F32(value as f32));
                }
            }
        }
    }

    // Hex float: 0x1.8p3 (p/P exponent means power of 2)
    if cleaned.starts_with("0x") || cleaned.starts_with("0X") {
        return parse_hex_float(&cleaned).map(ParsedFloat::F64);
    }

    // Standard float
    cleaned.parse::<f64>().ok().map(ParsedFloat::F64)
}

/// Parse hex float literal: 0x1.8p3 = 1.5 * 2^3 = 12.0
fn parse_hex_float(text: &str) -> Option<f64> {
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))?;

    // Split at 'p' or 'P' (binary exponent)
    let (mantissa_str, exp_str) = if let Some(idx) = text.find(['p', 'P']) {
        let (m, e) = text.split_at(idx);
        (m, &e[1..]) // skip 'p'
    } else {
        return None; // hex float requires 'p' exponent
    };

    // Parse mantissa (may have decimal point)
    let mantissa = if let Some(dot_idx) = mantissa_str.find('.') {
        let (int_part, frac_part) = mantissa_str.split_at(dot_idx);
        let frac_part = &frac_part[1..]; // skip '.'
        let int_val = if int_part.is_empty() {
            0u64
        } else {
            u64::from_str_radix(int_part, 16).ok()?
        };
        let frac_val = if frac_part.is_empty() {
            0.0
        } else {
            let frac_int = u64::from_str_radix(frac_part, 16).ok()?;
            frac_int as f64 / 16f64.powi(frac_part.len() as i32)
        };
        int_val as f64 + frac_val
    } else {
        u64::from_str_radix(mantissa_str, 16).ok()? as f64
    };

    // Parse binary exponent
    let exp: i32 = exp_str.parse().ok()?;

    Some(mantissa * 2f64.powi(exp))
}

/// Parse a string literal, handling quotes and escape sequences.
fn parse_string_literal(text: &str) -> String {
    let content = if let Some(stripped) = text
        .strip_prefix("\"\"\"")
        .and_then(|s| s.strip_suffix("\"\"\""))
    {
        stripped
    } else if let Some(stripped) = text.strip_prefix('\"').and_then(|s| s.strip_suffix('\"')) {
        stripped
    } else {
        text
    };

    // Process escape sequences
    process_escape_sequences(content)
}

/// Process escape sequences in a string.
///
/// Supports all of Julia's string escape sequences:
/// - `\n`, `\t`, `\r`, `\\`, `\"`, `\'`, `\$` — simple character escapes
/// - `\a`, `\b`, `\f`, `\v`, `\e`, `\0` — control character escapes
/// - `\xNN` — 1-2 hex digits, codepoint U+0000..U+00FF
/// - `\uNNNN` — 1-4 hex digits, Unicode codepoint
/// - `\UNNNNNNNN` — 1-8 hex digits, Unicode codepoint
/// - `\NNN` — 1-3 octal digits (first digit `0`-`3` if 3 digits), codepoint U+0000..U+00FF
pub(crate) fn process_escape_sequences(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut result = String::with_capacity(content.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            match next {
                b'n' => {
                    result.push('\n');
                    i += 2;
                }
                b't' => {
                    result.push('\t');
                    i += 2;
                }
                b'r' => {
                    result.push('\r');
                    i += 2;
                }
                b'\\' => {
                    result.push('\\');
                    i += 2;
                }
                b'"' => {
                    result.push('"');
                    i += 2;
                }
                b'\'' => {
                    result.push('\'');
                    i += 2;
                }
                b'$' => {
                    result.push('$');
                    i += 2;
                }
                b'a' => {
                    result.push('\x07');
                    i += 2;
                }
                b'b' => {
                    result.push('\x08');
                    i += 2;
                }
                b'f' => {
                    result.push('\x0c');
                    i += 2;
                }
                b'v' => {
                    result.push('\x0b');
                    i += 2;
                }
                b'e' => {
                    result.push('\x1b');
                    i += 2;
                }
                b'x' => {
                    // Hex escape: \xNN — 1-2 hex digits, greedy
                    let start = i + 2;
                    let mut end = start;
                    while end < bytes.len() && end - start < 2 && bytes[end].is_ascii_hexdigit() {
                        end += 1;
                    }
                    if end > start {
                        let hex = &content[start..end];
                        if let Ok(n) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(n) {
                                result.push(ch);
                                i = end;
                                continue;
                            }
                        }
                    }
                    // Invalid hex escape — keep as-is
                    result.push('\\');
                    result.push('x');
                    i += 2;
                }
                b'u' => {
                    // Unicode escape: \uNNNN — 1-4 hex digits, greedy
                    let start = i + 2;
                    let mut end = start;
                    while end < bytes.len() && end - start < 4 && bytes[end].is_ascii_hexdigit() {
                        end += 1;
                    }
                    if end > start {
                        let hex = &content[start..end];
                        if let Ok(n) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(n) {
                                result.push(ch);
                                i = end;
                                continue;
                            }
                        }
                    }
                    // Invalid unicode escape — keep as-is
                    result.push('\\');
                    result.push('u');
                    i += 2;
                }
                b'U' => {
                    // Unicode escape: \UNNNNNNNN — 1-8 hex digits, greedy
                    let start = i + 2;
                    let mut end = start;
                    while end < bytes.len() && end - start < 8 && bytes[end].is_ascii_hexdigit() {
                        end += 1;
                    }
                    if end > start {
                        let hex = &content[start..end];
                        if let Ok(n) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(n) {
                                result.push(ch);
                                i = end;
                                continue;
                            }
                        }
                    }
                    // Invalid unicode escape — keep as-is
                    result.push('\\');
                    result.push('U');
                    i += 2;
                }
                b'0'..=b'7' => {
                    // Octal escape: \NNN — 1-3 octal digits, greedy
                    let start = i + 1;
                    let mut end = start;
                    while end < bytes.len()
                        && end - start < 3
                        && (b'0'..=b'7').contains(&bytes[end])
                    {
                        end += 1;
                    }
                    if end > start {
                        let oct = &content[start..end];
                        if let Ok(n) = u32::from_str_radix(oct, 8) {
                            if let Some(ch) = char::from_u32(n) {
                                result.push(ch);
                                i = end;
                                continue;
                            }
                        }
                    }
                    // Shouldn't be reachable, but keep as-is on failure
                    result.push('\\');
                    i += 1;
                }
                _ => {
                    // Unknown escape sequence — keep as-is
                    result.push('\\');
                    result.push(next as char);
                    i += 2;
                }
            }
        } else {
            // Push the next char (handles multi-byte UTF-8 properly via char iteration)
            // Find char boundary
            let ch_start = i;
            // Advance i by the char's UTF-8 length
            let ch_len = utf8_char_len(b);
            let ch_end = (ch_start + ch_len).min(bytes.len());
            // Use the substring as &str safely (assuming valid UTF-8 input)
            result.push_str(&content[ch_start..ch_end]);
            i = ch_end;
        }
    }
    result
}

/// Return the UTF-8 byte length of the character starting with the given lead byte.
fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b < 0xC0 {
        // Continuation byte — shouldn't be a leading byte; fall back to 1.
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Lower character literal: 'a', '\n', '\u0041'
/// Julia's Char is a 32-bit Unicode codepoint.
pub fn lower_char_literal<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let text = walker.text(&node);

    // Remove surrounding quotes: 'x' -> x
    let content = if text.len() >= 2 && text.starts_with('\'') && text.ends_with('\'') {
        &text[1..text.len() - 1]
    } else {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "invalid char literal: {}",
                text
            )),
            span,
        ));
    };

    // Parse the character content
    let ch = parse_char_content(content).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(format!(
                "invalid char literal: {}",
                text
            )),
            span,
        )
    })?;

    Ok(Expr::Literal(Literal::Char(ch), span))
}

/// Parse the content of a character literal, handling escape sequences.
///
/// Supports the same escape forms as string literals (greedy hex/unicode/octal).
fn parse_char_content(content: &str) -> Option<char> {
    let bytes = content.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    if bytes[0] != b'\\' {
        // Non-escape: must be exactly one character (possibly multi-byte UTF-8)
        let mut chars = content.chars();
        let first = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        return Some(first);
    }

    if bytes.len() < 2 {
        return None;
    }

    let next = bytes[1];
    match next {
        b'n' if bytes.len() == 2 => Some('\n'),
        b'r' if bytes.len() == 2 => Some('\r'),
        b't' if bytes.len() == 2 => Some('\t'),
        b'\\' if bytes.len() == 2 => Some('\\'),
        b'\'' if bytes.len() == 2 => Some('\''),
        b'"' if bytes.len() == 2 => Some('"'),
        b'a' if bytes.len() == 2 => Some('\x07'),
        b'b' if bytes.len() == 2 => Some('\x08'),
        b'f' if bytes.len() == 2 => Some('\x0c'),
        b'v' if bytes.len() == 2 => Some('\x0b'),
        b'e' if bytes.len() == 2 => Some('\x1b'),
        b'$' if bytes.len() == 2 => Some('$'),
        b'x' => {
            // Hex escape: \xNN — 1-2 hex digits, greedy
            let hex_part = &content[2..];
            if hex_part.is_empty() || hex_part.len() > 2 {
                return None;
            }
            if !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(hex_part, 16)
                .ok()
                .and_then(char::from_u32)
        }
        b'u' => {
            // Unicode escape: \uNNNN — 1-4 hex digits, greedy
            let hex_part = &content[2..];
            if hex_part.is_empty() || hex_part.len() > 4 {
                return None;
            }
            if !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(hex_part, 16)
                .ok()
                .and_then(char::from_u32)
        }
        b'U' => {
            // Unicode escape: \UNNNNNNNN — 1-8 hex digits, greedy
            let hex_part = &content[2..];
            if hex_part.is_empty() || hex_part.len() > 8 {
                return None;
            }
            if !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            u32::from_str_radix(hex_part, 16)
                .ok()
                .and_then(char::from_u32)
        }
        b'0'..=b'7' => {
            // Octal escape: \NNN — 1-3 octal digits
            let oct_part = &content[1..];
            if oct_part.is_empty() || oct_part.len() > 3 {
                return None;
            }
            if !oct_part.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
                return None;
            }
            u32::from_str_radix(oct_part, 8)
                .ok()
                .and_then(char::from_u32)
        }
        _ => None,
    }
}

/// Lower string literal, handling interpolation if present.
/// Returns either a simple Literal::Str or a StringConcat expression.
pub fn lower_string_literal<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&node);

    // Get all children (including non-named nodes like quotes)
    let child_count = node.child_count();

    // Check if this string has interpolation by looking for string_interpolation nodes
    let mut has_interpolation = false;
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            if child.kind() == "string_interpolation" {
                has_interpolation = true;
                break;
            }
        }
    }

    if !has_interpolation {
        // No interpolation, use simple string literal parsing
        let value = parse_string_literal(walker.text(&node));
        return Ok(Expr::Literal(Literal::Str(value), span));
    }

    // Has interpolation - build StringConcat expression
    let mut parts: Vec<Expr> = Vec::new();

    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            let child_kind = child.kind();
            let child_span = walker.span(&child);

            match child_kind {
                "content" => {
                    // Plain text content - process escape sequences
                    let text = walker.text(&child);
                    let processed = process_escape_sequences(text);
                    if !processed.is_empty() {
                        parts.push(Expr::Literal(Literal::Str(processed), child_span));
                    }
                }
                "string_interpolation" | "interpolation_expression" => {
                    // $(expr) or $var - find the expression inside
                    // Pure Rust parser: leaf node with text like "$(x)" or "$x"
                    // Tree-sitter: has child nodes for the expression
                    let interp_children = walker.named_children(&child);
                    if let Some(expr_node) = interp_children.first() {
                        // Tree-sitter style: has child nodes
                        let expr = if let Some(ctx) = lambda_ctx {
                            lower_expr_with_ctx(walker, *expr_node, ctx)?
                        } else {
                            lower_expr(walker, *expr_node)?
                        };
                        parts.push(expr);
                    } else {
                        // Pure Rust parser style: leaf node with text
                        // Need to parse the expression from the text
                        let text = walker.text(&child);
                        if let Some(expr) = parse_interpolation_expr(text, child_span, lambda_ctx)?
                        {
                            parts.push(expr);
                        }
                    }
                }
                // Skip quote characters and other tokens
                _ => {}
            }
        }
    }

    // Optimize: if only one string part, return it directly
    if parts.len() == 1 {
        if let Expr::Literal(Literal::Str(_), _) = &parts[0] {
            return Ok(parts.remove(0));
        }
    }

    // Optimize: if empty, return empty string
    if parts.is_empty() {
        return Ok(Expr::Literal(Literal::Str(String::new()), span));
    }

    Ok(Expr::StringConcat { parts, span })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_int ─────────────────────────────────────────────────────────────

    #[test]
    fn test_parse_int_decimal_small() {
        assert!(matches!(parse_int("42"), Some(ParsedInt::I64(42))));
    }

    #[test]
    fn test_parse_int_decimal_max_i64() {
        // i64::MAX should parse as I64
        let max = i64::MAX.to_string();
        assert!(matches!(parse_int(&max), Some(ParsedInt::I64(_))));
    }

    #[test]
    fn test_parse_int_decimal_zero() {
        assert!(matches!(parse_int("0"), Some(ParsedInt::I64(0))));
    }

    #[test]
    fn test_parse_int_with_underscore_separator() {
        // Julia allows 1_000_000 as integer literal
        assert!(matches!(parse_int("1_000"), Some(ParsedInt::I64(1000))));
    }

    #[test]
    fn test_parse_int_hex() {
        assert!(matches!(parse_int("0xff"), Some(ParsedInt::I64(255))));
        assert!(matches!(parse_int("0xFF"), Some(ParsedInt::I64(255))));
        assert!(matches!(parse_int("0x10"), Some(ParsedInt::I64(16))));
    }

    #[test]
    fn test_parse_int_binary() {
        assert!(matches!(parse_int("0b1010"), Some(ParsedInt::I64(10))));
        assert!(matches!(parse_int("0B1111"), Some(ParsedInt::I64(15))));
    }

    #[test]
    fn test_parse_int_octal() {
        assert!(matches!(parse_int("0o17"), Some(ParsedInt::I64(15))));
        assert!(matches!(parse_int("0O10"), Some(ParsedInt::I64(8))));
    }

    #[test]
    fn test_parse_int_invalid_returns_none() {
        assert!(parse_int("").is_none());
        assert!(parse_int("abc").is_none());
        assert!(parse_int("1.5").is_none());
    }

    #[test]
    fn test_parse_int_large_becomes_i128() {
        // i64::MAX + 1 overflows to i128
        let large = "9223372036854775808"; // i64::MAX + 1
        assert!(matches!(parse_int(large), Some(ParsedInt::I128(_))));
    }

    // ── parse_int_typed (Issue #3559) ─────────────────────────────────────────

    #[test]
    fn test_parse_int_typed_decimal_has_no_kind() {
        let parsed = parse_int_typed("42").expect("should parse");
        assert!(matches!(parsed.value, ParsedInt::I64(42)));
        assert_eq!(parsed.kind, None);
    }

    #[test]
    fn test_parse_int_typed_hex_widths() {
        // 1-2 hex digits → UInt8
        assert_eq!(
            parse_int_typed("0x1").unwrap().kind,
            Some(TypedIntKind::UInt8)
        );
        assert_eq!(
            parse_int_typed("0xff").unwrap().kind,
            Some(TypedIntKind::UInt8)
        );
        // 3-4 hex digits → UInt16
        assert_eq!(
            parse_int_typed("0x100").unwrap().kind,
            Some(TypedIntKind::UInt16)
        );
        assert_eq!(
            parse_int_typed("0xffff").unwrap().kind,
            Some(TypedIntKind::UInt16)
        );
        // 5-8 hex digits → UInt32
        assert_eq!(
            parse_int_typed("0x10000").unwrap().kind,
            Some(TypedIntKind::UInt32)
        );
        assert_eq!(
            parse_int_typed("0xffffffff").unwrap().kind,
            Some(TypedIntKind::UInt32)
        );
        // 9-16 hex digits → UInt64
        assert_eq!(
            parse_int_typed("0x100000000").unwrap().kind,
            Some(TypedIntKind::UInt64),
        );
        // 17-32 hex digits → UInt128
        assert_eq!(
            parse_int_typed("0x10000000000000000").unwrap().kind,
            Some(TypedIntKind::UInt128),
        );
    }

    #[test]
    fn test_parse_int_typed_binary_widths() {
        // 1-8 bits → UInt8
        assert_eq!(
            parse_int_typed("0b1").unwrap().kind,
            Some(TypedIntKind::UInt8)
        );
        assert_eq!(
            parse_int_typed("0b11111111").unwrap().kind,
            Some(TypedIntKind::UInt8)
        );
        // 9-16 bits → UInt16
        assert_eq!(
            parse_int_typed("0b100000000").unwrap().kind,
            Some(TypedIntKind::UInt16),
        );
        // 17-32 bits → UInt32
        assert_eq!(
            parse_int_typed("0b10000000000000000").unwrap().kind,
            Some(TypedIntKind::UInt32),
        );
    }

    #[test]
    fn test_parse_int_typed_octal_widths() {
        // Leading zero(s) keep the typed-width small.
        assert_eq!(
            parse_int_typed("0o0").unwrap().kind,
            Some(TypedIntKind::UInt8)
        );
        assert_eq!(
            parse_int_typed("0o000").unwrap().kind,
            Some(TypedIntKind::UInt8)
        );
        assert_eq!(
            parse_int_typed("0o377").unwrap().kind,
            Some(TypedIntKind::UInt8)
        );
        // 0o400 = 256, leading digit 4 → 3 bits, total = 3*2+3 = 9 → UInt16
        assert_eq!(
            parse_int_typed("0o400").unwrap().kind,
            Some(TypedIntKind::UInt16)
        );
    }

    #[test]
    fn test_parse_int_typed_underscore_separator_ignored_in_width() {
        // Underscores must be stripped before width is computed.
        // `0b1_00000000` is 9 binary digits → UInt16.
        assert_eq!(
            parse_int_typed("0b1_00000000").unwrap().kind,
            Some(TypedIntKind::UInt16),
        );
        // `0x00_00` is still 4 hex digits → UInt16.
        assert_eq!(
            parse_int_typed("0x00_00").unwrap().kind,
            Some(TypedIntKind::UInt16),
        );
    }

    #[test]
    fn test_parse_int_typed_max_uint128_hex_uses_bigint_value() {
        // 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF overflows i128 and lands in
        // BigInt; the width tag should still be UInt128.
        let parsed = parse_int_typed("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF").expect("parse");
        assert!(matches!(parsed.value, ParsedInt::BigInt(_)));
        assert_eq!(parsed.kind, Some(TypedIntKind::UInt128));
    }

    // ── parse_float ───────────────────────────────────────────────────────────

    #[test]
    fn test_parse_float_standard() {
        assert!(
            matches!(parse_float("1.5"), Some(ParsedFloat::F64(_))),
            "Expected F64(1.5)"
        );
        if let Some(ParsedFloat::F64(v)) = parse_float("1.5") {
            assert!((v - 1.5).abs() < 1e-10);
        }
    }

    #[test]
    fn test_parse_float_scientific_notation() {
        assert!(
            matches!(parse_float("1e3"), Some(ParsedFloat::F64(_))),
            "Expected F64(1000.0)"
        );
        if let Some(ParsedFloat::F64(v)) = parse_float("1e3") {
            assert!((v - 1000.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_parse_float_f32_suffix() {
        // Julia 1.0f0 is Float32 (f suffix + 0 exponent = * 10^0 = 1.0 as f32)
        assert!(matches!(parse_float("1.0f0"), Some(ParsedFloat::F32(_))));
    }

    #[test]
    fn test_parse_float_with_underscore() {
        assert!(
            matches!(parse_float("1_000.0"), Some(ParsedFloat::F64(_))),
            "Expected F64(1000.0)"
        );
        if let Some(ParsedFloat::F64(v)) = parse_float("1_000.0") {
            assert!((v - 1000.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_parse_float_hex() {
        // 0x1.8p3 = (1 + 8/16) * 2^3 = 1.5 * 8 = 12.0
        assert!(
            matches!(parse_float("0x1.8p3"), Some(ParsedFloat::F64(_))),
            "Expected F64(12.0)"
        );
        if let Some(ParsedFloat::F64(v)) = parse_float("0x1.8p3") {
            assert!((v - 12.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_parse_float_invalid_returns_none() {
        assert!(parse_float("abc").is_none());
        assert!(parse_float("").is_none());
    }

    // ── parse_char_content ────────────────────────────────────────────────────

    #[test]
    fn test_parse_char_content_simple_ascii() {
        assert_eq!(parse_char_content("a"), Some('a'));
        assert_eq!(parse_char_content("Z"), Some('Z'));
        assert_eq!(parse_char_content("5"), Some('5'));
    }

    #[test]
    fn test_parse_char_content_escape_newline() {
        assert_eq!(parse_char_content("\\n"), Some('\n'));
    }

    #[test]
    fn test_parse_char_content_escape_tab() {
        assert_eq!(parse_char_content("\\t"), Some('\t'));
    }

    #[test]
    fn test_parse_char_content_escape_backslash() {
        assert_eq!(parse_char_content("\\\\"), Some('\\'));
    }

    #[test]
    fn test_parse_char_content_escape_single_quote() {
        assert_eq!(parse_char_content("\\'"), Some('\''));
    }

    #[test]
    fn test_parse_char_content_hex_escape() {
        assert_eq!(parse_char_content("\\x41"), Some('A')); // 0x41 = 'A'
    }

    #[test]
    fn test_parse_char_content_unicode_escape_u() {
        assert_eq!(parse_char_content("\\u0041"), Some('A')); // U+0041 = 'A'
    }

    #[test]
    fn test_parse_char_content_empty_returns_none() {
        assert!(parse_char_content("").is_none());
    }

    #[test]
    fn test_parse_char_content_invalid_escape_returns_none() {
        // \q is not a valid escape sequence
        assert!(parse_char_content("\\q").is_none());
    }
}
