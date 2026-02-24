//! Tests migrated from tree-sitter-julia/test/corpus/collections.txt

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
// Tuple Expression
// =============================================================================

#[test]
fn test_tuple_empty() {
    assert_root_child_kind("()", NodeKind::TupleExpression);
}

#[test]
fn test_tuple_single_trailing_comma() {
    assert_root_child_kind("(1,)", NodeKind::TupleExpression);
}

#[test]
fn test_tuple_two_elements() {
    assert_root_child_kind("(1, 2)", NodeKind::TupleExpression);
}

#[test]
fn test_tuple_three_elements() {
    assert_root_child_kind("(1, 2, 3)", NodeKind::TupleExpression);
}

#[test]
fn test_tuple_trailing_comma() {
    assert_root_child_kind("(1, 2, 3,)", NodeKind::TupleExpression);
}

#[test]
fn test_tuple_nested() {
    assert_parses("((1, 2), (3, 4))");
}

// =============================================================================
// Named Tuple
// =============================================================================

#[test]
fn test_named_tuple() {
    assert_parses("(a=1, b=2)");
}

#[test]
fn test_named_tuple_single() {
    assert_parses("(a=1,)");
}

#[test]
fn test_named_tuple_mixed() {
    // In Julia, this is actually a syntax error, but some parsers may accept it
    // We'll just test pure named tuples
    assert_parses("(x=1, y=2, z=3)");
}

// =============================================================================
// Vector Expression
// =============================================================================

#[test]
fn test_vector_empty() {
    assert_root_child_kind("[]", NodeKind::VectorExpression);
}

#[test]
fn test_vector_single() {
    assert_root_child_kind("[1]", NodeKind::VectorExpression);
}

#[test]
fn test_vector_multiple() {
    assert_root_child_kind("[1, 2, 3]", NodeKind::VectorExpression);
}

#[test]
fn test_vector_trailing_comma() {
    assert_root_child_kind("[1, 2, 3,]", NodeKind::VectorExpression);
}

#[test]
fn test_vector_nested() {
    assert_root_child_kind("[[1, 2], [3, 4]]", NodeKind::VectorExpression);
}

#[test]
fn test_vector_typed() {
    assert_parses("Int[1, 2, 3]");
    assert_parses("Float64[]");
}

// =============================================================================
// Matrix Expression
// =============================================================================

#[test]
fn test_matrix_row() {
    // Row vector with spaces
    assert_root_child_kind("[1 2 3]", NodeKind::MatrixExpression);
}

#[test]
fn test_matrix_2x2() {
    assert_root_child_kind("[1 2; 3 4]", NodeKind::MatrixExpression);
}

#[test]
fn test_matrix_3x3() {
    assert_root_child_kind("[1 2 3; 4 5 6; 7 8 9]", NodeKind::MatrixExpression);
}

#[test]
fn test_matrix_multiline() {
    assert_root_child_kind("[1 2\n 3 4]", NodeKind::MatrixExpression);
}

// Typed matrix
#[test]
fn test_matrix_typed() {
    assert_parses("Int[1 2; 3 4]");
}

// =============================================================================
// Comprehension Expression
// =============================================================================

#[test]
fn test_comprehension_simple() {
    assert_root_child_kind("[x for x in 1:10]", NodeKind::ComprehensionExpression);
}

#[test]
fn test_comprehension_expression() {
    assert_root_child_kind("[x^2 for x in 1:10]", NodeKind::ComprehensionExpression);
}

#[test]
fn test_comprehension_with_condition() {
    assert_root_child_kind(
        "[x for x in 1:10 if x > 5]",
        NodeKind::ComprehensionExpression,
    );
}

#[test]
fn test_comprehension_nested() {
    assert_root_child_kind(
        "[x + y for x in 1:3 for y in 1:3]",
        NodeKind::ComprehensionExpression,
    );
}

#[test]
fn test_comprehension_nested_with_condition() {
    assert_root_child_kind(
        "[x + y for x in 1:3 for y in 1:3 if x != y]",
        NodeKind::ComprehensionExpression,
    );
}

// 2D comprehension with comma separator
#[test]
fn test_comprehension_2d() {
    assert_parses("[(i, j) for i in 1:3, j in 1:3]");
    assert_parses("[i + j for i in 1:3, j in 1:3]");
    assert_parses("[i * j for i = 1:3, j = 1:3]"); // using = instead of in
}

// =============================================================================
// Generator Expression
// =============================================================================

#[test]
fn test_generator_simple() {
    assert_root_child_kind("(x for x in 1:10)", NodeKind::Generator);
}

#[test]
fn test_generator_expression() {
    assert_root_child_kind("(x^2 for x in 1:10)", NodeKind::Generator);
}

#[test]
fn test_generator_with_condition() {
    assert_root_child_kind("(x for x in 1:10 if x > 5)", NodeKind::Generator);
}

#[test]
fn test_generator_nested() {
    assert_root_child_kind("(x + y for x in 1:3 for y in 1:3)", NodeKind::Generator);
}

// =============================================================================
// Generator in Function Call
// =============================================================================

#[test]
fn test_generator_in_sum() {
    assert_parses("sum(x^2 for x in 1:10)");
}

#[test]
fn test_generator_in_map() {
    assert_parses("collect(x^2 for x in 1:10)");
}

#[test]
fn test_generator_in_any() {
    assert_parses("any(x > 5 for x in 1:10)");
}

// =============================================================================
// Dictionary Expression
// =============================================================================

// Dict literal with quote expressions
#[test]
fn test_dict_literal() {
    assert_parses("Dict(:a => 1, :b => 2)");
}

#[test]
fn test_dict_comprehension() {
    assert_parses("Dict(x => x^2 for x in 1:5)");
}

// =============================================================================
// Set Expression
// =============================================================================

#[test]
fn test_set_literal() {
    assert_parses("Set([1, 2, 3])");
}

// =============================================================================
// Range in Collections
// =============================================================================

#[test]
fn test_range_in_vector() {
    assert_parses("[1:10]");
    assert_parses("[1:2:10]");
}

#[test]
fn test_collect_range() {
    assert_parses("collect(1:10)");
}

// =============================================================================
// Splat in Collections
// =============================================================================

// Splat in vector/tuple collections
#[test]
fn test_splat_in_vector() {
    assert_parses("[x..., y]");
    assert_parses("[1, 2, rest...]");
}

#[test]
fn test_splat_in_tuple() {
    assert_parses("(x..., y)");
}

// =============================================================================
// Mixed Collections
// =============================================================================

#[test]
fn test_vector_of_tuples() {
    assert_parses("[(1, 2), (3, 4)]");
}

#[test]
fn test_tuple_of_vectors() {
    assert_parses("([1, 2], [3, 4])");
}

#[test]
fn test_matrix_of_expressions() {
    assert_parses("[a+b c*d; e/f g-h]");
}
