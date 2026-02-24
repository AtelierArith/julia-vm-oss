//! Tests migrated from tree-sitter-julia/test/corpus/literals.txt

use subset_julia_vm_parser::{parse, NodeKind};

fn assert_parses(source: &str) {
    let result = parse(source);
    assert!(
        result.is_ok(),
        "Failed to parse: {}\nError: {:?}",
        source,
        result.err()
    );
}

fn assert_root_child_kind(source: &str, expected_kind: NodeKind) {
    let cst = parse(source).unwrap_or_else(|_| panic!("Failed to parse: {}", source));
    assert_eq!(cst.kind, NodeKind::SourceFile);
    assert!(!cst.children.is_empty(), "Expected at least one child");
    assert_eq!(
        cst.children[0].kind, expected_kind,
        "Expected {:?}, got {:?} for source: {}",
        expected_kind, cst.children[0].kind, source
    );
}

// =============================================================================
// Boolean Literals
// =============================================================================

#[test]
fn test_boolean_true() {
    assert_root_child_kind("true", NodeKind::BooleanLiteral);
}

#[test]
fn test_boolean_false() {
    assert_root_child_kind("false", NodeKind::BooleanLiteral);
}

// =============================================================================
// Integer Literals
// =============================================================================

#[test]
fn test_integer_decimal() {
    assert_root_child_kind("1", NodeKind::IntegerLiteral);
    assert_root_child_kind("123", NodeKind::IntegerLiteral);
    assert_root_child_kind("1_000_000", NodeKind::IntegerLiteral);
}

#[test]
fn test_integer_binary() {
    assert_root_child_kind("0b01", NodeKind::IntegerLiteral);
    assert_root_child_kind("0b1010", NodeKind::IntegerLiteral);
}

#[test]
fn test_integer_octal() {
    assert_root_child_kind("0o01234567", NodeKind::IntegerLiteral);
}

#[test]
fn test_integer_hex() {
    assert_root_child_kind("0x0123456789abcdef", NodeKind::IntegerLiteral);
    assert_root_child_kind("0x0123456789ABCDEF", NodeKind::IntegerLiteral);
}

// =============================================================================
// Float Literals
// =============================================================================

#[test]
fn test_float_basic() {
    assert_root_child_kind("1.0", NodeKind::FloatLiteral);
    assert_root_child_kind(".5", NodeKind::FloatLiteral);
    assert_root_child_kind("1.", NodeKind::FloatLiteral);
}

#[test]
fn test_float_exponent() {
    assert_root_child_kind("1e10", NodeKind::FloatLiteral);
    assert_root_child_kind("1.0e10", NodeKind::FloatLiteral);
    assert_root_child_kind("1.0e-10", NodeKind::FloatLiteral);
    assert_root_child_kind("1.0E10", NodeKind::FloatLiteral);
}

#[test]
fn test_float_hex() {
    assert_root_child_kind("0x1p0", NodeKind::FloatLiteral);
    assert_root_child_kind("0x1.8p3", NodeKind::FloatLiteral);
}

// =============================================================================
// Character Literals
// =============================================================================

#[test]
fn test_character_simple() {
    assert_root_child_kind("'a'", NodeKind::CharacterLiteral);
    assert_root_child_kind("'1'", NodeKind::CharacterLiteral);
}

#[test]
fn test_character_escape() {
    assert_root_child_kind("'\\n'", NodeKind::CharacterLiteral);
    assert_root_child_kind("'\\t'", NodeKind::CharacterLiteral);
    assert_root_child_kind("'\\''", NodeKind::CharacterLiteral);
}

#[test]
fn test_character_unicode() {
    assert_root_child_kind("'α'", NodeKind::CharacterLiteral);
    assert_root_child_kind("'日'", NodeKind::CharacterLiteral);
}

// =============================================================================
// String Literals
// =============================================================================

#[test]
fn test_string_simple() {
    assert_root_child_kind(r#""hello""#, NodeKind::StringLiteral);
}

#[test]
fn test_string_escape() {
    assert_root_child_kind(r#""hello\nworld""#, NodeKind::StringLiteral);
    assert_root_child_kind(r#""hello\tworld""#, NodeKind::StringLiteral);
}

#[test]
fn test_string_interpolation() {
    assert_root_child_kind(r#""hello $name""#, NodeKind::StringLiteral);
    assert_root_child_kind(r#""value: $(x + 1)""#, NodeKind::StringLiteral);
}

#[test]
fn test_string_triple_quoted() {
    assert_root_child_kind(
        r#""""
    multiline
    string
    """"#,
        NodeKind::StringLiteral,
    );
}

#[test]
fn test_string_raw() {
    // Raw strings use raw"..."
    // Currently parsed as identifier + string (juxtaposition)
    assert_parses(r#"raw"hello\nworld""#);
}

// =============================================================================
// Non-standard String Literals
// =============================================================================

#[test]
fn test_prefixed_string_regex() {
    // r"..." for regex strings
    assert_parses(r#"r"^[a-z]+$""#);
    assert_parses(r#"r"hello\nworld""#);
}

#[test]
fn test_prefixed_string_byte_array() {
    // b"..." for byte arrays
    assert_parses(r#"b"hello""#);
    assert_parses(r#"b"\x00\xff""#);
}

#[test]
fn test_prefixed_string_in_context() {
    // Using prefixed strings in expressions
    assert_parses(r#"match(r"pattern", text)"#);
    assert_parses(r#"write(file, b"bytes")"#);
}

// =============================================================================
// Command Literals
// =============================================================================

#[test]
fn test_command_literal() {
    assert_root_child_kind("`ls -la`", NodeKind::CommandLiteral);
}

// Command with interpolation
#[test]
fn test_command_with_interpolation() {
    assert_root_child_kind("`echo $x`", NodeKind::CommandLiteral);
}

// =============================================================================
// Comments
// =============================================================================

#[test]
fn test_line_comment() {
    // Comments are typically ignored by the parser
    assert_parses("# this is a comment");
    assert_parses("x = 1 # inline comment");
}

#[test]
fn test_block_comment() {
    assert_parses("#= block comment =#");
    assert_parses("x = 1 #= inline block comment =#");
}

// Note: Nested block comments are partially supported
// Nested block comments
#[test]
fn test_nested_block_comment() {
    assert_parses("#= outer #= inner =# outer =#");
}
