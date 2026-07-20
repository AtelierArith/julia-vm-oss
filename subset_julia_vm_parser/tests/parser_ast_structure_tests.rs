//! AST Structure Validation Tests
//!
//! These tests verify the documented structure of each AST node type.
//! See `docs/vm/AST_STRUCTURE.md` for the complete structure documentation.
//!
//! Each test validates:
//! 1. The correct NodeKind is produced
//! 2. The expected number of children
//! 3. Each child's NodeKind matches documentation
//!
//! When tests fail, the actual structure is printed for debugging.

use subset_julia_vm_parser::{parse_with_errors, CstNode, NodeKind};

// ==================== Helper Functions ====================

fn parse_expr(source: &str) -> CstNode {
    let (root, errors) = parse_with_errors(source);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors.errors());
    assert_eq!(root.kind, NodeKind::SourceFile);
    root.children
        .into_iter()
        .next()
        .expect("no expression parsed")
}

fn parse_stmt(source: &str) -> CstNode {
    let (root, errors) = parse_with_errors(source);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors.errors());
    assert_eq!(root.kind, NodeKind::SourceFile);
    root.children
        .into_iter()
        .next()
        .expect("no statement parsed")
}

/// Debug helper: prints actual node structure on failure.
///
/// Kind/child-shape only (no leaf text) — `CstNode` no longer stores an
/// owned `text` copy, and this helper's ~21 call sites throughout the file
/// don't otherwise need the original source string threaded in just for a
/// diagnostic dump (Issue #10126).
fn debug_structure(node: &CstNode) -> String {
    fn inner(node: &CstNode, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        let mut result = format!("{}[{}] {:?}\n", prefix, indent, node.kind);
        for (i, child) in node.children.iter().enumerate() {
            result.push_str(&format!("{}  Child {}:\n", prefix, i));
            result.push_str(&inner(child, indent + 2));
        }
        result
    }
    inner(node, 0)
}

/// Assert node structure matches expected child kinds
fn assert_structure(node: &CstNode, expected_kind: NodeKind, expected_children: &[NodeKind]) {
    assert_eq!(
        node.kind,
        expected_kind,
        "Wrong node kind.\nActual structure:\n{}",
        debug_structure(node)
    );

    assert_eq!(
        node.children.len(),
        expected_children.len(),
        "{:?} should have {} children, got {}.\nActual structure:\n{}",
        expected_kind,
        expected_children.len(),
        node.children.len(),
        debug_structure(node)
    );

    for (i, expected) in expected_children.iter().enumerate() {
        assert_eq!(
            node.children[i].kind,
            *expected,
            "Child {} of {:?} should be {:?}, got {:?}.\nActual structure:\n{}",
            i,
            expected_kind,
            expected,
            node.children[i].kind,
            debug_structure(node)
        );
    }
}

// ==================== CallExpression Structure Tests ====================
// Documentation: CallExpression has [callee: Expression, arguments: ArgumentList]

#[test]
fn test_structure_call_expression_with_args() {
    // CallExpression: [callee, ArgumentList] - always 2 children
    let source = "foo(1, 2)";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::CallExpression,
        &[NodeKind::Identifier, NodeKind::ArgumentList],
    );
    // Verify callee name
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("foo"));
    // Verify argument count
    assert_eq!(node.children[1].children.len(), 2);
}

#[test]
fn test_structure_call_expression_empty() {
    // IMPORTANT: Empty calls still have ArgumentList child
    let source = "bar()";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::CallExpression,
        &[NodeKind::Identifier, NodeKind::ArgumentList],
    );
    // ArgumentList exists but is empty
    assert_eq!(node.children[1].children.len(), 0);
}

#[test]
fn test_structure_call_expression_chained() {
    // Method call on field: obj.method(x)
    let source = "obj.method(x)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::CallExpression);
    assert_eq!(node.children.len(), 2);
    assert_eq!(node.children[0].kind, NodeKind::FieldExpression);
    assert_eq!(node.children[1].kind, NodeKind::ArgumentList);
}

// ==================== BinaryExpression Structure Tests ====================
// Documentation: BinaryExpression has [left, operator, right] - 3 children

#[test]
fn test_structure_binary_expression_simple() {
    let source = "1 + 2";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::BinaryExpression,
        &[
            NodeKind::IntegerLiteral,
            NodeKind::Operator,
            NodeKind::IntegerLiteral,
        ],
    );
    // Verify operator
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("+"));
}

#[test]
fn test_structure_binary_expression_nested() {
    // a * b + c -> BinaryExpression[BinaryExpression[a, *, b], +, c]
    let source = "a * b + c";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children.len(), 3);
    // Left child is nested BinaryExpression (due to precedence)
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[1].kind, NodeKind::Operator);
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("+"));
    assert_eq!(node.children[2].kind, NodeKind::Identifier);
}

#[test]
fn test_structure_binary_expression_comparison() {
    let source = "x < y";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::BinaryExpression,
        &[
            NodeKind::Identifier,
            NodeKind::Operator,
            NodeKind::Identifier,
        ],
    );
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("<"));
}

// ==================== UnaryExpression Structure Tests ====================
// Documentation: UnaryExpression has [operator, operand] - 2 children

#[test]
fn test_structure_unary_expression_minus() {
    let source = "-x";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::UnaryExpression,
        &[NodeKind::Operator, NodeKind::Identifier],
    );
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("-"));
}

#[test]
fn test_structure_unary_expression_not() {
    let source = "!flag";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::UnaryExpression,
        &[NodeKind::Operator, NodeKind::Identifier],
    );
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("!"));
}

// ==================== TernaryExpression Structure Tests ====================
// Documentation: TernaryExpression has [condition, then, else] - 3 children

#[test]
fn test_structure_ternary_expression() {
    let source = "x > 0 ? 1 : 0";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::TernaryExpression,
        &[
            NodeKind::BinaryExpression, // condition
            NodeKind::IntegerLiteral,   // then
            NodeKind::IntegerLiteral,   // else
        ],
    );
}

#[test]
fn test_structure_ternary_nested() {
    let source = "a ? b : c ? d : e";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TernaryExpression);
    assert_eq!(node.children.len(), 3);
    // Right side is nested ternary
    assert_eq!(node.children[2].kind, NodeKind::TernaryExpression);
}

// ==================== IndexExpression Structure Tests ====================
// Documentation: IndexExpression has [object, index1, index2, ...] - 2+ children

#[test]
fn test_structure_index_expression_single() {
    let source = "arr[1]";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::IndexExpression,
        &[NodeKind::Identifier, NodeKind::IntegerLiteral],
    );
}

#[test]
fn test_structure_index_expression_multi() {
    let source = "matrix[i, j]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 3); // object + 2 indices
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
    assert_eq!(node.children[2].kind, NodeKind::Identifier);
}

#[test]
fn test_structure_index_expression_chained() {
    let source = "a[1][2]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 2);
    // First child is nested IndexExpression
    assert_eq!(node.children[0].kind, NodeKind::IndexExpression);
}

// ==================== FieldExpression Structure Tests ====================
// Documentation: FieldExpression has [object, field] - 2 children

#[test]
fn test_structure_field_expression() {
    let source = "obj.field";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::FieldExpression,
        &[NodeKind::Identifier, NodeKind::Identifier],
    );
    assert_eq!(
        Some(node.children[1].text_from_source(source)),
        Some("field")
    );
}

#[test]
fn test_structure_field_expression_chained() {
    let source = "a.b.c";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::FieldExpression);
    assert_eq!(node.children.len(), 2);
    // First child is nested FieldExpression
    assert_eq!(node.children[0].kind, NodeKind::FieldExpression);
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("c"));
}

// ==================== RangeExpression Structure Tests ====================
// Documentation: RangeExpression has [start, end] or [start, step, end]
// IMPORTANT: Not BinaryExpression!

#[test]
fn test_structure_range_expression_two_part() {
    let source = "1:10";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::RangeExpression,
        &[NodeKind::IntegerLiteral, NodeKind::IntegerLiteral],
    );
}

#[test]
fn test_structure_range_expression_three_part() {
    // Three-part ranges are nested: 1:2:10 -> RangeExpression[RangeExpression[1,2], 10]
    let source = "1:2:10";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::RangeExpression);
    assert_eq!(node.children.len(), 2);
    // First child is nested RangeExpression (1:2)
    assert_eq!(node.children[0].kind, NodeKind::RangeExpression);
    // Second child is the end (10)
    assert_eq!(node.children[1].kind, NodeKind::IntegerLiteral);
}

// ==================== TypedExpression Structure Tests ====================
// Documentation: TypedExpression has [expression, type] - 2 children

#[test]
fn test_structure_typed_expression() {
    let source = "x::Int";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::TypedExpression,
        &[NodeKind::Identifier, NodeKind::Identifier],
    );
}

#[test]
fn test_structure_typed_expression_parametric() {
    let source = "y::Vector{T}";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TypedExpression);
    assert_eq!(node.children.len(), 2);
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::ParametrizedTypeExpression);
}

// ==================== BroadcastCallExpression Structure Tests ====================
// Documentation: BroadcastCallExpression has [callee, arg1, arg2, ...] - NO ArgumentList!

#[test]
fn test_structure_broadcast_call_expression() {
    let source = "f.(x, y)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 3); // callee + 2 args (no ArgumentList wrapper!)
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
    assert_eq!(node.children[2].kind, NodeKind::Identifier);
}

#[test]
fn test_structure_broadcast_dotted_operator() {
    let source = ".+([1, 2])";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 2); // operator + 1 arg
                                        // First child is the operator
    assert_eq!(node.children[0].kind, NodeKind::Operator);
}

#[test]
fn test_structure_broadcast_dotted_not_prefix() {
    let source = ".!flags";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 2); // operator + operand
    assert_eq!(node.children[0].kind, NodeKind::Operator);
    assert_eq!(Some(node.children[0].text_from_source(source)), Some(".!"));
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
}

// ==================== Collection Structure Tests ====================

#[test]
fn test_structure_vector_expression() {
    let source = "[1, 2, 3]";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::VectorExpression,
        &[
            NodeKind::IntegerLiteral,
            NodeKind::IntegerLiteral,
            NodeKind::IntegerLiteral,
        ],
    );
}

#[test]
fn test_structure_vector_expression_empty() {
    let source = "[]";
    let node = parse_expr(source);
    assert_structure(&node, NodeKind::VectorExpression, &[]);
}

#[test]
fn test_structure_tuple_expression() {
    let source = "(1, 2, 3)";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::TupleExpression,
        &[
            NodeKind::IntegerLiteral,
            NodeKind::IntegerLiteral,
            NodeKind::IntegerLiteral,
        ],
    );
}

#[test]
fn test_structure_tuple_expression_empty() {
    let source = "()";
    let node = parse_expr(source);
    assert_structure(&node, NodeKind::TupleExpression, &[]);
}

#[test]
fn test_structure_matrix_expression() {
    // A single `;` separator now surfaces as an explicit `Semicolon` CST leaf
    // between the two `MatrixRow`s (not just "2 adjacent rows"), so lowering
    // can recover the separator's dimension level for N-D literals like
    // `;;`/`;;;`/... (Issue #10190).
    let source = "[1 2; 3 4]";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::MatrixExpression,
        &[
            NodeKind::MatrixRow,
            NodeKind::Semicolon,
            NodeKind::MatrixRow,
        ],
    );
    // Each row has 2 elements
    assert_eq!(node.children[0].children.len(), 2);
    assert_eq!(node.children[2].children.len(), 2);
}

#[test]
fn test_structure_matrix_symbol_literals_are_separate_elements_issue_4576() {
    // See `test_structure_matrix_expression`: the `;` separator is now an
    // explicit `Semicolon` leaf between the two `MatrixRow`s (Issue #10190).
    let source = "[:x :y; :z :w]";
    let node = parse_expr(source);
    assert_structure(
        &node,
        NodeKind::MatrixExpression,
        &[
            NodeKind::MatrixRow,
            NodeKind::Semicolon,
            NodeKind::MatrixRow,
        ],
    );
    assert_eq!(node.children[0].children.len(), 2);
    assert_eq!(node.children[2].children.len(), 2);
    assert_eq!(node.children[0].children[0].kind, NodeKind::QuoteExpression);
    assert_eq!(node.children[0].children[1].kind, NodeKind::QuoteExpression);
    assert_eq!(node.children[2].children[0].kind, NodeKind::QuoteExpression);
    assert_eq!(node.children[2].children[1].kind, NodeKind::QuoteExpression);
}

// ==================== Statement Structure Tests ====================

#[test]
fn test_structure_if_statement_simple() {
    let source = "if x y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::IfStatement);
    assert!(node.children.len() >= 2); // condition + body
                                       // First child is condition (expression)
                                       // Second child is then body (Block)
    assert_eq!(node.children[1].kind, NodeKind::Block);
}

#[test]
fn test_structure_if_statement_with_else() {
    let source = "if x y else z end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::IfStatement);
    // Should have ElseClause
    let has_else = node.children.iter().any(|c| c.kind == NodeKind::ElseClause);
    assert!(
        has_else,
        "IfStatement with else should have ElseClause child"
    );
}

#[test]
fn test_structure_for_statement() {
    let source = "for i in 1:10 x end";
    let node = parse_stmt(source);
    assert_structure(
        &node,
        NodeKind::ForStatement,
        &[NodeKind::ForBinding, NodeKind::Block],
    );
}

#[test]
fn test_structure_while_statement() {
    let source = "while x > 0 x -= 1 end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::WhileStatement);
    assert_eq!(node.children.len(), 2);
    // First child is condition (BinaryExpression)
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    // Second child is body (Block)
    assert_eq!(node.children[1].kind, NodeKind::Block);
}

#[test]
fn test_structure_return_statement_empty() {
    let source = "return";
    let node = parse_stmt(source);
    assert_structure(&node, NodeKind::ReturnStatement, &[]);
}

#[test]
fn test_structure_return_statement_with_value() {
    let source = "return x + 1";
    let node = parse_stmt(source);
    assert_structure(
        &node,
        NodeKind::ReturnStatement,
        &[NodeKind::BinaryExpression],
    );
}

#[test]
fn test_structure_function_definition() {
    let source = "function add(x, y) x + y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    assert!(node.children.len() >= 2);
    // First child is name
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("add"));
}

#[test]
fn test_structure_struct_definition() {
    let source = "struct Point x y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::StructDefinition);
    assert!(!node.children.is_empty());
    // First child is name
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(
        Some(node.children[0].text_from_source(source)),
        Some("Point")
    );
}

// ==================== Regression Tests ====================

#[test]
fn test_regression_call_expression_always_has_argument_list() {
    // Issue #1581: Developers assumed empty calls have 1 child
    // Correct behavior: Always 2 children [callee, ArgumentList]

    for source in &["f()", "g(1)", "h(1, 2)", "i(a, b, c)"] {
        let node = parse_expr(source);
        assert_eq!(node.kind, NodeKind::CallExpression);
        assert_eq!(
            node.children.len(),
            2,
            "CallExpression '{}' should always have 2 children",
            source
        );
        assert_eq!(
            node.children[1].kind,
            NodeKind::ArgumentList,
            "Second child of CallExpression '{}' should be ArgumentList",
            source
        );
    }
}

#[test]
fn test_regression_range_is_not_binary_expression() {
    // Issue #1581: Developers assumed 1:10 uses BinaryExpression
    // Correct behavior: RangeExpression
    let source = "1:10";
    let node = parse_expr(source);
    assert_eq!(
        node.kind,
        NodeKind::RangeExpression,
        "Range should be RangeExpression, not BinaryExpression"
    );
}
