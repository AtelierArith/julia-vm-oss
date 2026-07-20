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

#[test]
fn test_tuple_multiline_elements() {
    assert_root_child_kind(
        "(\n    (\"x\", 1),\n    (\"y\", 2)\n)",
        NodeKind::TupleExpression,
    );
}

#[test]
fn test_parenthesized_multiline_expression() {
    assert_root_child_kind("(\n    1 + 2\n)", NodeKind::ParenthesizedExpression);
}

#[test]
fn test_parenthesized_statement_item_issue_8759() {
    assert_parses("(global c = c + 1; 0)");
    assert_parses("(print(io); return)");
    assert_parses("rows < 2 && (print(io, \" ...\"); return)");
    assert_parses(
        "isempty(s) && return interpolate ? (Expr(:tuple,:()), last_arg) : ([], last_arg)",
    );
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

#[test]
fn test_additional_collection_gaps_issue_8759() {
    assert_root_child_kind(
        "_cat5b = [randn(3) ;; randn(3) ;; randn(3)]",
        NodeKind::Assignment,
    );
    assert_root_child_kind(
        "g = ((x + y) % 5 == 2 for x = 1:n1, y = 1:n2)",
        NodeKind::Assignment,
    );
    assert_root_child_kind("push!(s, 1:2:10...)", NodeKind::CallExpression);
}

#[test]
fn test_vector_typed_comprehension() {
    assert_root_child_kind("Float64[i / 10.0 for i in 1:10]", NodeKind::TypedExpression);
}

#[test]
fn test_typed_comprehension_quote_body_newline_before_for_issue_8759() {
    assert_root_child_kind(
        "Expr[\n quote\n  x = y\n end\n for y in ys\n]",
        NodeKind::TypedExpression,
    );
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

// Issue #7196: whitespace-sensitive `+`/`-` element disambiguation in a
// matrix/`hcat` row. Returns the element count of each row of the first
// top-level matrix in `source`.
fn matrix_row_lengths(source: &str) -> Vec<usize> {
    let cst = parse(source).unwrap_or_else(|_| panic!("Failed to parse: {}", source));
    // The matrix may be the top-level child or nested under a TypedExpression
    // (`T[...]`); walk down to the first MatrixExpression.
    fn find_matrix(
        node: &subset_julia_vm_parser::CstNode,
    ) -> Option<&subset_julia_vm_parser::CstNode> {
        if node.kind == NodeKind::MatrixExpression {
            return Some(node);
        }
        node.children.iter().find_map(find_matrix)
    }
    let matrix = find_matrix(&cst)
        .unwrap_or_else(|| panic!("No MatrixExpression found for source: {}", source));
    matrix
        .children
        .iter()
        .filter(|row| row.kind == NodeKind::MatrixRow)
        .map(|row| row.children.len())
        .collect()
}

#[test]
fn test_matrix_negative_element_space_before_no_space_after() {
    // `[1 -2]` is two elements (Issue #7196), not `[1 - 2]`.
    assert_eq!(matrix_row_lengths("[1 -2]"), vec![2]);
    assert_eq!(matrix_row_lengths("[0.20 -0.26]"), vec![2]);
    // The exact repro from the issue: 2x2.
    assert_eq!(matrix_row_lengths("[0.20 -0.26; 0.23 0.22]"), vec![2, 2]);
    // Unary `+` element.
    assert_eq!(matrix_row_lengths("[1 +2]"), vec![2]);
    // Three elements with leading signs.
    assert_eq!(matrix_row_lengths("[1 -2 +3]"), vec![3]);
}

#[test]
fn test_matrix_binary_minus_stays_single_element() {
    // Spaces on both sides => binary subtraction => single element.
    // `[1 - 2 3]` is `(1 - 2)` then `3`.
    assert_eq!(matrix_row_lengths("[1 - 2 3]"), vec![2]);
    // `*` is NOT subject to the rule: `[1 *2 3]` is `1*2`, `3` => 2 elements.
    assert_eq!(matrix_row_lengths("[1 *2 3]"), vec![2]);
    // Per-row independence.
    assert_eq!(matrix_row_lengths("[1 -2; 3 4]"), vec![2, 2]);
    assert_eq!(matrix_row_lengths("[1 1; 2 -3]"), vec![2, 2]);
}

#[test]
fn test_typed_matrix_negative_element() {
    // The same rule applies inside `T[...]`.
    assert_eq!(matrix_row_lengths("Float64[1 -2]"), vec![2]);
    assert_eq!(
        matrix_row_lengths("Float64[0.20 -0.26; 0.23 0.22]"),
        vec![2, 2]
    );
}

#[test]
fn test_typed_matrix_trailing_ncat_separator_issue_8759() {
    assert_root_child_kind("Float32[4.0; 7.0;;]", NodeKind::TypedExpression);
}

#[test]
fn test_matrix_row_trailing_ncat_separator_issue_8759() {
    assert_root_child_kind("[v v; v v;;;]", NodeKind::MatrixExpression);
    assert_root_child_kind("[v3 ;;; v1 v1]", NodeKind::MatrixExpression);
    assert_root_child_kind("[1 1 ;;; 1 1 ;;;;]", NodeKind::MatrixExpression);
}

#[test]
fn test_empty_ncat_issue_9046() {
    assert_root_child_kind("[;]", NodeKind::VectorExpression);
    assert_root_child_kind("[;;]", NodeKind::VectorExpression);
    assert_root_child_kind("[;;;]", NodeKind::VectorExpression);
    assert_parses("T[;]");
    assert_parses("T[;;]");
    assert_parses("[1, 2;]");
    assert_parses("[ ;;\n;;\n]");
}

#[test]
fn test_matrix_minus_inside_call_arg_is_not_new_element() {
    // The matrix-row context does not extend into a call's argument list: the
    // `-` inside `[f(1 -2) 3]` is ordinary binary subtraction, so the row has
    // two elements `f(1 -2)` and `3` (not three). This exercises the
    // matrix-row flag being cleared on entering a call (Issue #7196).
    assert_eq!(matrix_row_lengths("[f(1 -2) 3]"), vec![2]);
    assert_eq!(matrix_row_lengths("[f(1 - 2) 3]"), vec![2]);
}

#[test]
fn test_matrix_tuple_elements_space_before_paren_issue_9437() {
    // In a matrix row, whitespace before `(` / `[` is an element separator,
    // not a spaced call/index postfix. Adjacent `f(1)` remains a call.
    assert_eq!(matrix_row_lengths("[(1, 2) (3, 4)]"), vec![2]);
    assert_eq!(matrix_row_lengths("[f (1)]"), vec![2]);
    assert_eq!(matrix_row_lengths("[f(1) 2]"), vec![2]);
    assert_eq!(matrix_row_lengths("[[1] [2]]"), vec![2]);
}

// =============================================================================
// Comprehension Expression
// =============================================================================

#[test]
fn test_comprehension_simple() {
    assert_root_child_kind("[x for x in 1:10]", NodeKind::ComprehensionExpression);
}

#[test]
fn test_comprehension_newline_before_for() {
    assert_root_child_kind("[x\n for x in 1:10]", NodeKind::ComprehensionExpression);
}

#[test]
fn test_comprehension_expression() {
    assert_root_child_kind("[x^2 for x in 1:10]", NodeKind::ComprehensionExpression);
}

#[test]
fn test_comprehension_macrocall_body() {
    assert_root_child_kind(
        r#"[@async @test String(recv(s)) == "hello" for s in (a, b)]"#,
        NodeKind::ComprehensionExpression,
    );
}

// Newlines inside `[...]` are insignificant, so a multi-line comprehension whose
// `if`/`for` guard sits on a following line must parse identically to the
// single-line form (Issue #8008).
#[test]
fn test_comprehension_newline_before_if() {
    assert_root_child_kind(
        "[x for x in 1:10\n if x > 5]",
        NodeKind::ComprehensionExpression,
    );
}

#[test]
fn test_comprehension_newline_before_second_for() {
    assert_root_child_kind(
        "[x + y for x in 1:3\n for y in 1:3]",
        NodeKind::ComprehensionExpression,
    );
}

#[test]
fn test_comprehension_newline_before_for_and_if() {
    assert_root_child_kind(
        "[x + y for x in 1:3\n for y in 1:3\n if x != y]",
        NodeKind::ComprehensionExpression,
    );
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

// A 2D comprehension whose binding-separating comma is followed by a newline
// parses identically to the single-line form (Issue #8008).
#[test]
fn test_comprehension_2d_newline_after_comma() {
    assert_root_child_kind(
        "[i + j for i in 1:3,\n j in 1:3]",
        NodeKind::ComprehensionExpression,
    );
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

// A multi-line generator with the `if` guard on a following line parses the same
// as the single-line form (Issue #8008).
#[test]
fn test_generator_newline_before_if() {
    assert_root_child_kind("(x for x in 1:10\n if x > 5)", NodeKind::Generator);
}

#[test]
fn test_generator_line_continuation_after_in_issue_8753() {
    assert_root_child_kind(
        "(floor(Int, (tar - lo) * k2ln + lo + off) for (tar, off) in\n    ((minimum(target), -offset), (maximum(target), offset)))",
        NodeKind::Generator,
    );
    assert_parses(
        "f(floor(Int, tar) for (tar, off) in\n    ((minimum(target), -offset), (maximum(target), offset)))",
    );
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
