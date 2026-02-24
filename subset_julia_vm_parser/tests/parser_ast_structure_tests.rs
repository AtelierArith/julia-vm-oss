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

/// Debug helper: prints actual node structure on failure
fn debug_structure(node: &CstNode) -> String {
    fn inner(node: &CstNode, indent: usize) -> String {
        let prefix = "  ".repeat(indent);
        let mut result = format!("{}[{}] {:?} = {:?}\n", prefix, indent, node.kind, node.text);
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
    let node = parse_expr("foo(1, 2)");
    assert_structure(
        &node,
        NodeKind::CallExpression,
        &[NodeKind::Identifier, NodeKind::ArgumentList],
    );
    // Verify callee name
    assert_eq!(node.children[0].text.as_deref(), Some("foo"));
    // Verify argument count
    assert_eq!(node.children[1].children.len(), 2);
}

#[test]
fn test_structure_call_expression_empty() {
    // IMPORTANT: Empty calls still have ArgumentList child
    let node = parse_expr("bar()");
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
    let node = parse_expr("obj.method(x)");
    assert_eq!(node.kind, NodeKind::CallExpression);
    assert_eq!(node.children.len(), 2);
    assert_eq!(node.children[0].kind, NodeKind::FieldExpression);
    assert_eq!(node.children[1].kind, NodeKind::ArgumentList);
}

// ==================== BinaryExpression Structure Tests ====================
// Documentation: BinaryExpression has [left, operator, right] - 3 children

#[test]
fn test_structure_binary_expression_simple() {
    let node = parse_expr("1 + 2");
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
    assert_eq!(node.children[1].text.as_deref(), Some("+"));
}

#[test]
fn test_structure_binary_expression_nested() {
    // a * b + c -> BinaryExpression[BinaryExpression[a, *, b], +, c]
    let node = parse_expr("a * b + c");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children.len(), 3);
    // Left child is nested BinaryExpression (due to precedence)
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[1].kind, NodeKind::Operator);
    assert_eq!(node.children[1].text.as_deref(), Some("+"));
    assert_eq!(node.children[2].kind, NodeKind::Identifier);
}

#[test]
fn test_structure_binary_expression_comparison() {
    let node = parse_expr("x < y");
    assert_structure(
        &node,
        NodeKind::BinaryExpression,
        &[
            NodeKind::Identifier,
            NodeKind::Operator,
            NodeKind::Identifier,
        ],
    );
    assert_eq!(node.children[1].text.as_deref(), Some("<"));
}

// ==================== UnaryExpression Structure Tests ====================
// Documentation: UnaryExpression has [operator, operand] - 2 children

#[test]
fn test_structure_unary_expression_minus() {
    let node = parse_expr("-x");
    assert_structure(
        &node,
        NodeKind::UnaryExpression,
        &[NodeKind::Operator, NodeKind::Identifier],
    );
    assert_eq!(node.children[0].text.as_deref(), Some("-"));
}

#[test]
fn test_structure_unary_expression_not() {
    let node = parse_expr("!flag");
    assert_structure(
        &node,
        NodeKind::UnaryExpression,
        &[NodeKind::Operator, NodeKind::Identifier],
    );
    assert_eq!(node.children[0].text.as_deref(), Some("!"));
}

// ==================== TernaryExpression Structure Tests ====================
// Documentation: TernaryExpression has [condition, then, else] - 3 children

#[test]
fn test_structure_ternary_expression() {
    let node = parse_expr("x > 0 ? 1 : 0");
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
    let node = parse_expr("a ? b : c ? d : e");
    assert_eq!(node.kind, NodeKind::TernaryExpression);
    assert_eq!(node.children.len(), 3);
    // Right side is nested ternary
    assert_eq!(node.children[2].kind, NodeKind::TernaryExpression);
}

// ==================== IndexExpression Structure Tests ====================
// Documentation: IndexExpression has [object, index1, index2, ...] - 2+ children

#[test]
fn test_structure_index_expression_single() {
    let node = parse_expr("arr[1]");
    assert_structure(
        &node,
        NodeKind::IndexExpression,
        &[NodeKind::Identifier, NodeKind::IntegerLiteral],
    );
}

#[test]
fn test_structure_index_expression_multi() {
    let node = parse_expr("matrix[i, j]");
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 3); // object + 2 indices
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
    assert_eq!(node.children[2].kind, NodeKind::Identifier);
}

#[test]
fn test_structure_index_expression_chained() {
    let node = parse_expr("a[1][2]");
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 2);
    // First child is nested IndexExpression
    assert_eq!(node.children[0].kind, NodeKind::IndexExpression);
}

// ==================== FieldExpression Structure Tests ====================
// Documentation: FieldExpression has [object, field] - 2 children

#[test]
fn test_structure_field_expression() {
    let node = parse_expr("obj.field");
    assert_structure(
        &node,
        NodeKind::FieldExpression,
        &[NodeKind::Identifier, NodeKind::Identifier],
    );
    assert_eq!(node.children[1].text.as_deref(), Some("field"));
}

#[test]
fn test_structure_field_expression_chained() {
    let node = parse_expr("a.b.c");
    assert_eq!(node.kind, NodeKind::FieldExpression);
    assert_eq!(node.children.len(), 2);
    // First child is nested FieldExpression
    assert_eq!(node.children[0].kind, NodeKind::FieldExpression);
    assert_eq!(node.children[1].text.as_deref(), Some("c"));
}

// ==================== RangeExpression Structure Tests ====================
// Documentation: RangeExpression has [start, end] or [start, step, end]
// IMPORTANT: Not BinaryExpression!

#[test]
fn test_structure_range_expression_two_part() {
    let node = parse_expr("1:10");
    assert_structure(
        &node,
        NodeKind::RangeExpression,
        &[NodeKind::IntegerLiteral, NodeKind::IntegerLiteral],
    );
}

#[test]
fn test_structure_range_expression_three_part() {
    // Three-part ranges are nested: 1:2:10 -> RangeExpression[RangeExpression[1,2], 10]
    let node = parse_expr("1:2:10");
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
    let node = parse_expr("x::Int");
    assert_structure(
        &node,
        NodeKind::TypedExpression,
        &[NodeKind::Identifier, NodeKind::Identifier],
    );
}

#[test]
fn test_structure_typed_expression_parametric() {
    let node = parse_expr("y::Vector{T}");
    assert_eq!(node.kind, NodeKind::TypedExpression);
    assert_eq!(node.children.len(), 2);
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::ParametrizedTypeExpression);
}

// ==================== BroadcastCallExpression Structure Tests ====================
// Documentation: BroadcastCallExpression has [callee, arg1, arg2, ...] - NO ArgumentList!

#[test]
fn test_structure_broadcast_call_expression() {
    let node = parse_expr("f.(x, y)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 3); // callee + 2 args (no ArgumentList wrapper!)
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
    assert_eq!(node.children[2].kind, NodeKind::Identifier);
}

#[test]
fn test_structure_broadcast_dotted_operator() {
    let node = parse_expr(".+([1, 2])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 2); // operator + 1 arg
                                        // First child is the operator
    assert_eq!(node.children[0].kind, NodeKind::Operator);
}

// ==================== Collection Structure Tests ====================

#[test]
fn test_structure_vector_expression() {
    let node = parse_expr("[1, 2, 3]");
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
    let node = parse_expr("[]");
    assert_structure(&node, NodeKind::VectorExpression, &[]);
}

#[test]
fn test_structure_tuple_expression() {
    let node = parse_expr("(1, 2, 3)");
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
    let node = parse_expr("()");
    assert_structure(&node, NodeKind::TupleExpression, &[]);
}

#[test]
fn test_structure_matrix_expression() {
    let node = parse_expr("[1 2; 3 4]");
    assert_structure(
        &node,
        NodeKind::MatrixExpression,
        &[NodeKind::MatrixRow, NodeKind::MatrixRow],
    );
    // Each row has 2 elements
    assert_eq!(node.children[0].children.len(), 2);
    assert_eq!(node.children[1].children.len(), 2);
}

// ==================== Statement Structure Tests ====================

#[test]
fn test_structure_if_statement_simple() {
    let node = parse_stmt("if x y end");
    assert_eq!(node.kind, NodeKind::IfStatement);
    assert!(node.children.len() >= 2); // condition + body
                                       // First child is condition (expression)
                                       // Second child is then body (Block)
    assert_eq!(node.children[1].kind, NodeKind::Block);
}

#[test]
fn test_structure_if_statement_with_else() {
    let node = parse_stmt("if x y else z end");
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
    let node = parse_stmt("for i in 1:10 x end");
    assert_structure(
        &node,
        NodeKind::ForStatement,
        &[NodeKind::ForBinding, NodeKind::Block],
    );
}

#[test]
fn test_structure_while_statement() {
    let node = parse_stmt("while x > 0 x -= 1 end");
    assert_eq!(node.kind, NodeKind::WhileStatement);
    assert_eq!(node.children.len(), 2);
    // First child is condition (BinaryExpression)
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    // Second child is body (Block)
    assert_eq!(node.children[1].kind, NodeKind::Block);
}

#[test]
fn test_structure_return_statement_empty() {
    let node = parse_stmt("return");
    assert_structure(&node, NodeKind::ReturnStatement, &[]);
}

#[test]
fn test_structure_return_statement_with_value() {
    let node = parse_stmt("return x + 1");
    assert_structure(
        &node,
        NodeKind::ReturnStatement,
        &[NodeKind::BinaryExpression],
    );
}

#[test]
fn test_structure_function_definition() {
    let node = parse_stmt("function add(x, y) x + y end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    assert!(node.children.len() >= 2);
    // First child is name
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[0].text.as_deref(), Some("add"));
}

#[test]
fn test_structure_struct_definition() {
    let node = parse_stmt("struct Point x y end");
    assert_eq!(node.kind, NodeKind::StructDefinition);
    assert!(!node.children.is_empty());
    // First child is name
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[0].text.as_deref(), Some("Point"));
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
    let node = parse_expr("1:10");
    assert_eq!(
        node.kind,
        NodeKind::RangeExpression,
        "Range should be RangeExpression, not BinaryExpression"
    );
}
