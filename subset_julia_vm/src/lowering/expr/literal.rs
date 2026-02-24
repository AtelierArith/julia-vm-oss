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

pub fn parse_int(text: &str) -> Option<ParsedInt> {
    let cleaned = text.replace('_', "");
    if let Some(hex) = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
    {
        parse_int_radix(hex, 16)
    } else if let Some(bin) = cleaned
        .strip_prefix("0b")
        .or_else(|| cleaned.strip_prefix("0B"))
    {
        parse_int_radix(bin, 2)
    } else if let Some(oct) = cleaned
        .strip_prefix("0o")
        .or_else(|| cleaned.strip_prefix("0O"))
    {
        parse_int_radix(oct, 8)
    } else {
        parse_int_decimal(&cleaned)
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

/// Process escape sequences in a string
fn process_escape_sequences(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('$') => result.push('$'),
                Some(other) => {
                    // Unknown escape sequence, keep as-is
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
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
fn parse_char_content(content: &str) -> Option<char> {
    let mut chars = content.chars();
    let first = chars.next()?;

    if first == '\\' {
        // Escape sequence
        let escaped = chars.next()?;
        match escaped {
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            '\\' => Some('\\'),
            '\'' => Some('\''),
            '"' => Some('"'),
            '0' => Some('\0'),
            'a' => Some('\x07'), // Bell
            'b' => Some('\x08'), // Backspace
            'f' => Some('\x0c'), // Form feed
            'v' => Some('\x0b'), // Vertical tab
            'e' => Some('\x1b'), // Escape
            'x' => {
                // Hex escape: \xNN
                let hex: String = chars.take(2).collect();
                if hex.len() == 2 {
                    u8::from_str_radix(&hex, 16).ok().map(|b| b as char)
                } else {
                    None
                }
            }
            'u' => {
                // Unicode escape: \uNNNN
                let hex: String = chars.take(4).collect();
                if hex.len() == 4 {
                    u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                } else {
                    None
                }
            }
            'U' => {
                // Unicode escape: \UNNNNNNNN
                let hex: String = chars.take(8).collect();
                if hex.len() == 8 {
                    u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                } else {
                    None
                }
            }
            _ => None, // Invalid escape
        }
    } else if chars.next().is_none() {
        // Single character (no more chars after first)
        Some(first)
    } else {
        // Multi-character literal without escape - check if it's a multi-byte UTF-8 char
        // Actually, we should return the first char if the remaining are empty
        // But we already checked chars.next().is_none() above
        None
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

    // ── parse_float ───────────────────────────────────────────────────────────

    #[test]
    fn test_parse_float_standard() {
        assert!(matches!(parse_float("1.5"), Some(ParsedFloat::F64(_))), "Expected F64(1.5)");
        if let Some(ParsedFloat::F64(v)) = parse_float("1.5") {
            assert!((v - 1.5).abs() < 1e-10);
        }
    }

    #[test]
    fn test_parse_float_scientific_notation() {
        assert!(matches!(parse_float("1e3"), Some(ParsedFloat::F64(_))), "Expected F64(1000.0)");
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
        assert!(matches!(parse_float("1_000.0"), Some(ParsedFloat::F64(_))), "Expected F64(1000.0)");
        if let Some(ParsedFloat::F64(v)) = parse_float("1_000.0") {
            assert!((v - 1000.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_parse_float_hex() {
        // 0x1.8p3 = (1 + 8/16) * 2^3 = 1.5 * 8 = 12.0
        assert!(matches!(parse_float("0x1.8p3"), Some(ParsedFloat::F64(_))), "Expected F64(12.0)");
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
