//! Tests migrated from tree-sitter-julia/test/corpus/operators.txt

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
// Assignment Operators
// =============================================================================

// Assignment is parsed as Assignment node (not BinaryExpression)
#[test]
fn test_assignment_simple() {
    assert_root_child_kind("x = 1", NodeKind::Assignment);
    assert_root_child_kind("x = y = z", NodeKind::Assignment);
}

// Compound assignment is parsed as CompoundAssignmentExpression
#[test]
fn test_assignment_compound() {
    assert_root_child_kind("x += 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x -= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x *= 2", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x /= 2", NodeKind::CompoundAssignmentExpression);
}

#[test]
fn test_assignment_compound_more() {
    assert_root_child_kind("x ^= 2", NodeKind::CompoundAssignmentExpression);
    assert_parses("x ÷= 2"); // Unicode operator
    assert_root_child_kind("x %= 3", NodeKind::CompoundAssignmentExpression);
}

#[test]
fn test_assignment_bitwise() {
    assert_root_child_kind("x |= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x &= 1", NodeKind::CompoundAssignmentExpression);
    assert_parses("x ⊻= 1"); // Unicode operator
}

#[test]
fn test_assignment_shift() {
    assert_root_child_kind("x <<= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x >>= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x >>>= 1", NodeKind::CompoundAssignmentExpression);
}

// =============================================================================
// Binary Arithmetic Operators
// =============================================================================

#[test]
fn test_binary_addition() {
    assert_root_child_kind("a + b", NodeKind::BinaryExpression);
    assert_root_child_kind("a - b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_multiplication() {
    assert_root_child_kind("a * b", NodeKind::BinaryExpression);
    assert_root_child_kind("a / b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ÷ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a % b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_power() {
    assert_root_child_kind("a ^ b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_rational() {
    assert_root_child_kind("a // b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_matrix() {
    assert_root_child_kind("a \\ b", NodeKind::BinaryExpression);
}

// =============================================================================
// Comparison Operators
// =============================================================================

#[test]
fn test_comparison_basic() {
    assert_root_child_kind("a < b", NodeKind::BinaryExpression);
    assert_root_child_kind("a > b", NodeKind::BinaryExpression);
    assert_root_child_kind("a <= b", NodeKind::BinaryExpression);
    assert_root_child_kind("a >= b", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_equality() {
    assert_root_child_kind("a == b", NodeKind::BinaryExpression);
    assert_root_child_kind("a != b", NodeKind::BinaryExpression);
    assert_root_child_kind("a === b", NodeKind::BinaryExpression);
    assert_root_child_kind("a !== b", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_unicode() {
    assert_root_child_kind("a ≤ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ≥ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ≠ b", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_chained() {
    // Chained comparisons: a < b < c
    assert_parses("a < b < c");
    assert_parses("1 <= x <= 10");
}

#[test]
fn test_comparison_in() {
    assert_root_child_kind("a in b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ∈ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ∉ b", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_isa() {
    assert_root_child_kind("a isa T", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_subtype() {
    assert_root_child_kind("A <: B", NodeKind::BinaryExpression);
    assert_root_child_kind("A >: B", NodeKind::BinaryExpression);
}

// =============================================================================
// Logical Operators
// =============================================================================

#[test]
fn test_logical_and() {
    assert_root_child_kind("a && b", NodeKind::BinaryExpression);
}

#[test]
fn test_logical_or() {
    assert_root_child_kind("a || b", NodeKind::BinaryExpression);
}

#[test]
fn test_logical_not() {
    assert_root_child_kind("!a", NodeKind::UnaryExpression);
}

// =============================================================================
// Bitwise Operators
// =============================================================================

#[test]
fn test_bitwise_and() {
    assert_root_child_kind("a & b", NodeKind::BinaryExpression);
}

#[test]
fn test_bitwise_or() {
    assert_root_child_kind("a | b", NodeKind::BinaryExpression);
}

// Unicode xor operator
#[test]
fn test_bitwise_xor() {
    assert_parses("a ⊻ b"); // Unicode xor
    assert_parses("xor(a, b)");
}

#[test]
fn test_bitwise_not() {
    assert_root_child_kind("~a", NodeKind::UnaryExpression);
}

#[test]
fn test_bitwise_shift() {
    assert_root_child_kind("a << b", NodeKind::BinaryExpression);
    assert_root_child_kind("a >> b", NodeKind::BinaryExpression);
    assert_root_child_kind("a >>> b", NodeKind::BinaryExpression);
}

// =============================================================================
// Unary Operators
// =============================================================================

#[test]
fn test_unary_plus_minus() {
    assert_root_child_kind("-x", NodeKind::UnaryExpression);
    assert_root_child_kind("+x", NodeKind::UnaryExpression);
}

#[test]
fn test_unary_not() {
    assert_root_child_kind("!x", NodeKind::UnaryExpression);
}

#[test]
fn test_unary_sqrt() {
    assert_root_child_kind("√x", NodeKind::UnaryExpression);
    assert_root_child_kind("∛x", NodeKind::UnaryExpression);
    assert_root_child_kind("∜x", NodeKind::UnaryExpression);
}

// =============================================================================
// Broadcasting Operators
// =============================================================================

#[test]
fn test_broadcast_binary() {
    assert_root_child_kind("a .+ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .- b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .* b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ./ b", NodeKind::BinaryExpression);
}

#[test]
fn test_broadcast_comparison() {
    assert_root_child_kind("a .< b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .> b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .== b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .<= b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .>= b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .!= b", NodeKind::BinaryExpression);
}

#[test]
fn test_broadcast_power() {
    assert_root_child_kind("a .^ b", NodeKind::BinaryExpression);
}

// =============================================================================
// Pipe Operators
// =============================================================================

#[test]
fn test_pipe_right() {
    assert_root_child_kind("a |> f", NodeKind::BinaryExpression);
    assert_root_child_kind("x |> f |> g", NodeKind::BinaryExpression);
}

#[test]
fn test_pipe_left() {
    assert_root_child_kind("f <| a", NodeKind::BinaryExpression);
}

// =============================================================================
// Ternary Operator
// =============================================================================

#[test]
fn test_ternary() {
    assert_root_child_kind("a ? b : c", NodeKind::TernaryExpression);
}

// Nested ternary
#[test]
fn test_ternary_nested() {
    assert_parses("a ? b ? c : d : e");
    assert_parses("a ? b : c ? d : e");
}

// =============================================================================
// Operator Precedence
// =============================================================================

#[test]
fn test_precedence_arithmetic() {
    // * binds tighter than +
    assert_parses("a + b * c");
    assert_parses("a * b + c");
}

#[test]
fn test_precedence_power() {
    // ^ binds tighter than *
    assert_parses("a * b ^ c");
    assert_parses("a ^ b * c");
}

#[test]
fn test_precedence_comparison() {
    // + binds tighter than <
    assert_parses("a + b < c + d");
}

#[test]
fn test_precedence_logical() {
    // && binds tighter than ||
    assert_parses("a || b && c");
    assert_parses("a && b || c");
}

// =============================================================================
// Operator Associativity
// =============================================================================

#[test]
fn test_associativity_left() {
    // Left associative: +, -, *, /
    assert_parses("a + b + c");
    assert_parses("a - b - c");
    assert_parses("a * b * c");
    assert_parses("a / b / c");
}

#[test]
fn test_associativity_right() {
    // Right associative: ^, =
    assert_parses("a ^ b ^ c");
    assert_parses("a = b = c");
}

// =============================================================================
// Operators as Values
// =============================================================================

#[test]
fn test_operator_as_value() {
    assert_root_child_kind("(+)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(-)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(*)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(/)", NodeKind::ParenthesizedExpression);
}

#[test]
fn test_operator_in_call() {
    assert_parses("map(+, a, b)");
    assert_parses("reduce(*, xs)");
}

// =============================================================================
// Quote Expressions (Symbol Operators)
// =============================================================================

#[test]
fn test_quote_expression_simple() {
    // :symbol syntax
    assert_root_child_kind(":foo", NodeKind::QuoteExpression);
    assert_root_child_kind(":bar", NodeKind::QuoteExpression);
}

#[test]
fn test_quote_expression_operator() {
    // :(operator) syntax - quoting operators as symbols
    assert_root_child_kind(":(+)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(-)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(*)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(==)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(<=)", NodeKind::QuoteExpression);
}

#[test]
fn test_quote_expression_in_context() {
    // Using quoted operators in expressions
    assert_parses("f(:(+))");
    assert_parses("Dict(:add => (+), :mul => (*))");
    assert_parses("getfield(Base, :(+))");
}

#[test]
fn test_broadcast_dotop_as_function() {
    // Broadcast operators used as functions: .+(a, b) is equivalent to (+).(a, b)
    assert_parses(".+([1, 2, 3])");
    assert_parses(".-([1, 2, 3])");
    assert_parses(".*([1, 2], [3, 4])");
    assert_parses(".+(x)");
    assert_parses(".-(x, y)");
}

// =============================================================================
// Keyword Symbols
// =============================================================================

#[test]
fn test_keyword_symbols() {
    // Keyword symbols: :if, :for, :quote, :end, etc.
    assert_root_child_kind(":if", NodeKind::QuoteExpression);
    assert_root_child_kind(":for", NodeKind::QuoteExpression);
    assert_root_child_kind(":while", NodeKind::QuoteExpression);
    assert_root_child_kind(":end", NodeKind::QuoteExpression);
    assert_root_child_kind(":quote", NodeKind::QuoteExpression);
    assert_root_child_kind(":begin", NodeKind::QuoteExpression);
    assert_root_child_kind(":let", NodeKind::QuoteExpression);
    assert_root_child_kind(":function", NodeKind::QuoteExpression);
    assert_root_child_kind(":macro", NodeKind::QuoteExpression);
    assert_root_child_kind(":return", NodeKind::QuoteExpression);
    assert_root_child_kind(":break", NodeKind::QuoteExpression);
    assert_root_child_kind(":continue", NodeKind::QuoteExpression);
    assert_root_child_kind(":try", NodeKind::QuoteExpression);
    assert_root_child_kind(":catch", NodeKind::QuoteExpression);
    assert_root_child_kind(":finally", NodeKind::QuoteExpression);
    assert_root_child_kind(":else", NodeKind::QuoteExpression);
    assert_root_child_kind(":elseif", NodeKind::QuoteExpression);
    assert_root_child_kind(":module", NodeKind::QuoteExpression);
    assert_root_child_kind(":struct", NodeKind::QuoteExpression);
    assert_root_child_kind(":mutable", NodeKind::QuoteExpression);
    assert_root_child_kind(":abstract", NodeKind::QuoteExpression);
    assert_root_child_kind(":primitive", NodeKind::QuoteExpression);
    assert_root_child_kind(":type", NodeKind::QuoteExpression);
    assert_root_child_kind(":const", NodeKind::QuoteExpression);
    assert_root_child_kind(":global", NodeKind::QuoteExpression);
    assert_root_child_kind(":local", NodeKind::QuoteExpression);
    assert_root_child_kind(":using", NodeKind::QuoteExpression);
    assert_root_child_kind(":import", NodeKind::QuoteExpression);
    assert_root_child_kind(":export", NodeKind::QuoteExpression);
    assert_root_child_kind(":in", NodeKind::QuoteExpression);
    assert_root_child_kind(":isa", NodeKind::QuoteExpression);
    assert_root_child_kind(":where", NodeKind::QuoteExpression);
    assert_root_child_kind(":do", NodeKind::QuoteExpression);
    assert_root_child_kind(":true", NodeKind::QuoteExpression);
    assert_root_child_kind(":false", NodeKind::QuoteExpression);
}

#[test]
fn test_keyword_symbols_in_context() {
    // Using keyword symbols in expressions
    assert_parses("Meta.isexpr(ex, :if)");
    assert_parses("Meta.isexpr(ex, :for)");
    assert_parses("ex.head == :call");
    assert_parses("ex.head == :quote");
    assert_parses("Dict(:if => 1, :for => 2)");
    assert_parses("[:if, :for, :while, :end]");
}

// =============================================================================
// Parser Dispatch Tests for Operator Classification (Issue #1578)
// =============================================================================

#[test]
fn test_dotted_operator_not_parsed_as_method_definition() {
    // Issue #1574/#1578: .+(x) should be BroadcastCallExpression, NOT ShortFunctionDefinition
    assert_root_child_kind(".+(x)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".-(x)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".*(x, y)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind("./(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".^(x, 2)", NodeKind::BroadcastCallExpression);
}

#[test]
fn test_regular_operator_method_definition() {
    // Regular operators CAN be used in method definitions
    assert_root_child_kind("+(x, y) = x + y", NodeKind::ShortFunctionDefinition);
    assert_root_child_kind("-(x, y) = x - y", NodeKind::ShortFunctionDefinition);
    assert_root_child_kind("*(x, y) = x * y", NodeKind::ShortFunctionDefinition);
}

#[test]
fn test_comparison_dotted_operators_as_calls() {
    // Dotted comparison operators as broadcast call expressions
    assert_root_child_kind(".<(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".>(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".<=(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".>=(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".==(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".!=(a, b)", NodeKind::BroadcastCallExpression);
}
