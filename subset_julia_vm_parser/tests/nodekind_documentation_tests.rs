//! NodeKind Documentation Tests
//!
//! This file serves as living documentation for which NodeKind is produced
//! by each type of Julia syntax. Each test demonstrates the expected NodeKind
//! for a specific syntactic construct.
//!
//! These tests help prevent issues like #1573 where migrated tests expected
//! incorrect NodeKind values (e.g., expecting BinaryExpression for assignment).
//!
//! Related: Issue #1577

use subset_julia_vm_parser::{parse_with_errors, NodeKind};

/// Helper: parse a single expression and return its kind
fn parse_and_get_kind(source: &str) -> NodeKind {
    let (root, errors) = parse_with_errors(source);
    assert!(
        errors.is_empty(),
        "Parse errors for '{}': {:?}",
        source,
        errors.errors()
    );
    assert_eq!(root.kind, NodeKind::SourceFile);
    root.children
        .into_iter()
        .next()
        .expect("no expression parsed")
        .kind
}

/// Helper macro: assert that code parses to expected NodeKind
macro_rules! assert_parses_as {
    ($source:expr, $expected:expr) => {
        let actual = parse_and_get_kind($source);
        assert_eq!(
            actual, $expected,
            "Expected '{}' to parse as {:?}, but got {:?}",
            $source, $expected, actual
        );
    };
}

// ==================== Assignment vs BinaryExpression ====================
// IMPORTANT: Assignment (=) produces Assignment, NOT BinaryExpression!
// This was the root cause of Issue #1573.

#[test]
fn assignment_produces_assignment_node() {
    // Simple assignment
    assert_parses_as!("x = 1", NodeKind::Assignment);
    assert_parses_as!("x = y", NodeKind::Assignment);
    assert_parses_as!("x = 1 + 2", NodeKind::Assignment);

    // Note: Short function definitions like f(x) = x^2 parse as Assignment
    // because the parser treats them as assignments at the expression level.
    // The lowering phase distinguishes them based on the LHS pattern.
    assert_parses_as!("f(x) = x^2", NodeKind::Assignment);
}

#[test]
fn compound_assignment_produces_compound_assignment() {
    // Common compound assignments produce CompoundAssignmentExpression
    assert_parses_as!("x += 1", NodeKind::CompoundAssignmentExpression);
    assert_parses_as!("x -= 1", NodeKind::CompoundAssignmentExpression);
    assert_parses_as!("x *= 2", NodeKind::CompoundAssignmentExpression);
    assert_parses_as!("x /= 2", NodeKind::CompoundAssignmentExpression);
    assert_parses_as!("x ^= 2", NodeKind::CompoundAssignmentExpression);

    // Note: Some compound operators may parse as BinaryExpression
    // depending on operator precedence handling
}

#[test]
fn binary_operators_produce_binary_expression() {
    // Arithmetic operators
    assert_parses_as!("a + b", NodeKind::BinaryExpression);
    assert_parses_as!("a - b", NodeKind::BinaryExpression);
    assert_parses_as!("a * b", NodeKind::BinaryExpression);
    assert_parses_as!("a / b", NodeKind::BinaryExpression);
    assert_parses_as!("a ^ b", NodeKind::BinaryExpression);

    // Comparison operators
    assert_parses_as!("a == b", NodeKind::BinaryExpression);
    assert_parses_as!("a != b", NodeKind::BinaryExpression);
    assert_parses_as!("a < b", NodeKind::BinaryExpression);
    assert_parses_as!("a > b", NodeKind::BinaryExpression);
    assert_parses_as!("a <= b", NodeKind::BinaryExpression);
    assert_parses_as!("a >= b", NodeKind::BinaryExpression);

    // Logical operators
    assert_parses_as!("a && b", NodeKind::BinaryExpression);
    assert_parses_as!("a || b", NodeKind::BinaryExpression);

    // Bitwise operators
    assert_parses_as!("a & b", NodeKind::BinaryExpression);
    assert_parses_as!("a | b", NodeKind::BinaryExpression);

    // Pair expression (=>) is actually a BinaryExpression
    assert_parses_as!("a => b", NodeKind::BinaryExpression);
}

// ==================== Literals ====================

#[test]
fn literals_produce_expected_kinds() {
    // Integer literals
    assert_parses_as!("42", NodeKind::IntegerLiteral);
    assert_parses_as!("0xff", NodeKind::IntegerLiteral);
    assert_parses_as!("0b1010", NodeKind::IntegerLiteral);
    assert_parses_as!("0o777", NodeKind::IntegerLiteral);

    // Float literals
    assert_parses_as!("3.14", NodeKind::FloatLiteral);
    assert_parses_as!("1e10", NodeKind::FloatLiteral);
    assert_parses_as!("1.5e-3", NodeKind::FloatLiteral);

    // Boolean literals
    assert_parses_as!("true", NodeKind::BooleanLiteral);
    assert_parses_as!("false", NodeKind::BooleanLiteral);

    // Character literals
    assert_parses_as!("'a'", NodeKind::CharacterLiteral);
    assert_parses_as!("'\\n'", NodeKind::CharacterLiteral);

    // String literals
    assert_parses_as!("\"hello\"", NodeKind::StringLiteral);
}

// ==================== Identifiers ====================

#[test]
fn identifiers_produce_identifier() {
    assert_parses_as!("x", NodeKind::Identifier);
    assert_parses_as!("foo", NodeKind::Identifier);
    assert_parses_as!("bar_baz", NodeKind::Identifier);
    assert_parses_as!("α", NodeKind::Identifier);
    assert_parses_as!("∂x", NodeKind::Identifier);
}

// ==================== Unary Expressions ====================

#[test]
fn unary_operators_produce_unary_expression() {
    assert_parses_as!("-x", NodeKind::UnaryExpression);
    assert_parses_as!("+x", NodeKind::UnaryExpression);
    assert_parses_as!("!x", NodeKind::UnaryExpression);
    assert_parses_as!("~x", NodeKind::UnaryExpression);
}

// ==================== Ternary Expression ====================

#[test]
fn ternary_produces_ternary_expression() {
    assert_parses_as!("a ? b : c", NodeKind::TernaryExpression);
    assert_parses_as!("x > 0 ? x : -x", NodeKind::TernaryExpression);
}

// ==================== Function Definitions ====================

#[test]
fn function_definition_kinds() {
    // Full function definition
    assert_parses_as!("function f(x) x end", NodeKind::FunctionDefinition);

    // Anonymous function (arrow function)
    assert_parses_as!("x -> x^2", NodeKind::ArrowFunctionExpression);
}

// ==================== Control Flow ====================

#[test]
fn control_flow_kinds() {
    assert_parses_as!("if x 1 end", NodeKind::IfStatement);
    assert_parses_as!("for i in 1:10 i end", NodeKind::ForStatement);
    assert_parses_as!("while x x end", NodeKind::WhileStatement);
    assert_parses_as!("try x catch end", NodeKind::TryStatement);
    // begin...end produces BeginBlock (not CompoundStatement)
    assert_parses_as!("begin x end", NodeKind::BeginBlock);
    // let produces LetExpression (not LetStatement)
    assert_parses_as!("let x = 1 x end", NodeKind::LetExpression);
}

#[test]
fn control_flow_statements() {
    assert_parses_as!("return", NodeKind::ReturnStatement);
    assert_parses_as!("return x", NodeKind::ReturnStatement);
    assert_parses_as!("break", NodeKind::BreakStatement);
    assert_parses_as!("continue", NodeKind::ContinueStatement);
}

// ==================== Type Definitions ====================

#[test]
fn type_definition_kinds() {
    assert_parses_as!("struct Point x end", NodeKind::StructDefinition);
    assert_parses_as!(
        "mutable struct Point x end",
        NodeKind::MutableStructDefinition
    );
    assert_parses_as!("abstract type Number end", NodeKind::AbstractDefinition);
    assert_parses_as!("primitive type UInt8 8 end", NodeKind::PrimitiveDefinition);
}

// ==================== Call Expressions ====================

#[test]
fn call_expression_kinds() {
    assert_parses_as!("f()", NodeKind::CallExpression);
    assert_parses_as!("f(x)", NodeKind::CallExpression);
    assert_parses_as!("f(x, y)", NodeKind::CallExpression);
    assert_parses_as!("f(x; y=1)", NodeKind::CallExpression);
}

#[test]
fn broadcast_call_expression() {
    assert_parses_as!("f.(x)", NodeKind::BroadcastCallExpression);
    assert_parses_as!("sin.(x)", NodeKind::BroadcastCallExpression);
}

// ==================== Index and Field Access ====================

#[test]
fn access_expression_kinds() {
    assert_parses_as!("a[1]", NodeKind::IndexExpression);
    assert_parses_as!("a[1, 2]", NodeKind::IndexExpression);
    assert_parses_as!("a.b", NodeKind::FieldExpression);
    assert_parses_as!("a.b.c", NodeKind::FieldExpression);
}

// ==================== Collections ====================

#[test]
fn collection_expression_kinds() {
    assert_parses_as!("[1, 2, 3]", NodeKind::VectorExpression);
    assert_parses_as!("[1 2; 3 4]", NodeKind::MatrixExpression);
    assert_parses_as!("(1, 2, 3)", NodeKind::TupleExpression);
    assert_parses_as!("[x^2 for x in 1:10]", NodeKind::ComprehensionExpression);
}

// ==================== Range Expressions ====================

#[test]
fn range_expression_kinds() {
    assert_parses_as!("1:10", NodeKind::RangeExpression);
    assert_parses_as!("1:2:10", NodeKind::RangeExpression);
}

// ==================== Type Annotations ====================

#[test]
fn type_annotation_kinds() {
    assert_parses_as!("x::Int", NodeKind::TypedExpression);
    // Note: Standalone type annotation (::Int) without LHS is not valid syntax
    // at the expression level - it's only valid in function signatures
}

// ==================== Splat ====================

#[test]
fn splat_expression_kinds() {
    assert_parses_as!("x...", NodeKind::SplatExpression);
}

// ==================== Quote and Macro ====================

#[test]
fn quote_and_macro_kinds() {
    assert_parses_as!(":x", NodeKind::QuoteExpression);
    assert_parses_as!(":(x + 1)", NodeKind::QuoteExpression);
    // quote...end also produces QuoteExpression (not QuoteStatement)
    assert_parses_as!("quote x end", NodeKind::QuoteExpression);
    assert_parses_as!("@test x", NodeKind::MacrocallExpression);
}

// ==================== Module and Import ====================

#[test]
fn module_and_import_kinds() {
    assert_parses_as!("module M end", NodeKind::ModuleDefinition);
    assert_parses_as!("baremodule M end", NodeKind::BaremoduleDefinition);
    assert_parses_as!("using Test", NodeKind::UsingStatement);
    assert_parses_as!("import Base", NodeKind::ImportStatement);
    assert_parses_as!("export f, g", NodeKind::ExportStatement);
}

// ==================== Declaration Statements ====================

#[test]
fn declaration_kinds() {
    // const produces ConstDeclaration (not ConstStatement)
    assert_parses_as!("const x = 1", NodeKind::ConstDeclaration);
    // global/local produce GlobalDeclaration/LocalDeclaration
    assert_parses_as!("global x", NodeKind::GlobalDeclaration);
    assert_parses_as!("local x", NodeKind::LocalDeclaration);
}

// ==================== Special Expressions ====================

#[test]
fn special_expression_kinds() {
    // Juxtaposition (implicit multiplication)
    assert_parses_as!("2x", NodeKind::JuxtapositionExpression);
    assert_parses_as!("3im", NodeKind::JuxtapositionExpression);

    // Parenthesized expression
    assert_parses_as!("(x)", NodeKind::ParenthesizedExpression);
    assert_parses_as!("(x + y)", NodeKind::ParenthesizedExpression);
}
