//! Parser tests
//!
//! Tests for the Julia subset parser, migrated from parser.rs inline tests.

use subset_julia_vm_parser::{parse_with_errors, CstNode, NodeKind};

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

// ==================== Literal Tests ====================

#[test]
fn test_integer_literals() {
    let node = parse_expr("42");
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(node.text.as_deref(), Some("42"));

    let node = parse_expr("0xff");
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(node.text.as_deref(), Some("0xff"));

    let node = parse_expr("0b1010");
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(node.text.as_deref(), Some("0b1010"));

    let node = parse_expr("1_000_000");
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(node.text.as_deref(), Some("1_000_000"));
}

#[test]
fn test_float_literals() {
    let node = parse_expr("3.14");
    assert_eq!(node.kind, NodeKind::FloatLiteral);
    assert_eq!(node.text.as_deref(), Some("3.14"));

    let node = parse_expr("1e-5");
    assert_eq!(node.kind, NodeKind::FloatLiteral);
    assert_eq!(node.text.as_deref(), Some("1e-5"));

    let node = parse_expr(".5");
    assert_eq!(node.kind, NodeKind::FloatLiteral);
    assert_eq!(node.text.as_deref(), Some(".5"));
}

#[test]
fn test_boolean_literals() {
    let node = parse_expr("true");
    assert_eq!(node.kind, NodeKind::BooleanLiteral);
    assert_eq!(node.text.as_deref(), Some("true"));

    let node = parse_expr("false");
    assert_eq!(node.kind, NodeKind::BooleanLiteral);
    assert_eq!(node.text.as_deref(), Some("false"));
}

#[test]
fn test_character_literals() {
    let node = parse_expr("'a'");
    assert_eq!(node.kind, NodeKind::CharacterLiteral);
    assert_eq!(node.text.as_deref(), Some("'a'"));

    let node = parse_expr("'\\n'");
    assert_eq!(node.kind, NodeKind::CharacterLiteral);
    assert_eq!(node.text.as_deref(), Some("'\\n'"));
}

#[test]
fn test_identifiers() {
    let node = parse_expr("foo");
    assert_eq!(node.kind, NodeKind::Identifier);
    assert_eq!(node.text.as_deref(), Some("foo"));

    let node = parse_expr("bar_baz");
    assert_eq!(node.kind, NodeKind::Identifier);
    assert_eq!(node.text.as_deref(), Some("bar_baz"));
}

#[test]
fn test_string_literals() {
    let node = parse_expr("\"hello\"");
    assert_eq!(node.kind, NodeKind::StringLiteral);
    assert_eq!(node.children.len(), 1); // content
    assert_eq!(node.children[0].text.as_deref(), Some("hello"));
}

#[test]
fn test_string_interpolation() {
    let node = parse_expr("\"hello $name\"");
    assert_eq!(node.kind, NodeKind::StringLiteral);
    assert_eq!(node.children.len(), 2); // "hello " and $name
    assert_eq!(node.children[0].kind, NodeKind::Content);
    assert_eq!(node.children[1].kind, NodeKind::StringInterpolation);
}

// ==================== Collection Tests ====================

#[test]
fn test_tuple() {
    let node = parse_expr("(1, 2, 3)");
    assert_eq!(node.kind, NodeKind::TupleExpression);
    assert_eq!(node.children.len(), 3);
}

#[test]
fn test_empty_tuple() {
    let node = parse_expr("()");
    assert_eq!(node.kind, NodeKind::TupleExpression);
    assert_eq!(node.children.len(), 0);
}

#[test]
fn test_parenthesized() {
    let node = parse_expr("(42)");
    assert_eq!(node.kind, NodeKind::ParenthesizedExpression);
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_vector() {
    let node = parse_expr("[1, 2, 3]");
    assert_eq!(node.kind, NodeKind::VectorExpression);
    assert_eq!(node.children.len(), 3);
}

#[test]
fn test_empty_vector() {
    let node = parse_expr("[]");
    assert_eq!(node.kind, NodeKind::VectorExpression);
    assert_eq!(node.children.len(), 0);
}

// ==================== Expression Tests ====================

#[test]
fn test_binary_expression_add() {
    let node = parse_expr("1 + 2");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children.len(), 3); // left, op, right
    assert_eq!(node.children[0].kind, NodeKind::IntegerLiteral);
    assert_eq!(node.children[1].kind, NodeKind::Operator);
    assert_eq!(node.children[1].text.as_deref(), Some("+"));
    assert_eq!(node.children[2].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_binary_expression_precedence() {
    // 1 + 2 * 3 should parse as 1 + (2 * 3)
    let node = parse_expr("1 + 2 * 3");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[1].text.as_deref(), Some("+")); // outer is +
    assert_eq!(node.children[2].kind, NodeKind::BinaryExpression); // right is binary
    assert_eq!(node.children[2].children[1].text.as_deref(), Some("*"));
}

#[test]
fn test_binary_expression_left_assoc() {
    // 1 - 2 - 3 should parse as (1 - 2) - 3
    let node = parse_expr("1 - 2 - 3");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression); // left is binary
    assert_eq!(node.children[1].text.as_deref(), Some("-"));
    assert_eq!(node.children[2].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_binary_expression_right_assoc() {
    // 2 ^ 3 ^ 4 should parse as 2 ^ (3 ^ 4)
    let node = parse_expr("2 ^ 3 ^ 4");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[0].kind, NodeKind::IntegerLiteral);
    assert_eq!(node.children[1].text.as_deref(), Some("^"));
    assert_eq!(node.children[2].kind, NodeKind::BinaryExpression); // right is binary
}

#[test]
fn test_unary_expression() {
    let node = parse_expr("-x");
    assert_eq!(node.kind, NodeKind::UnaryExpression);
    assert_eq!(node.children.len(), 2); // op, operand
    assert_eq!(node.children[0].kind, NodeKind::Operator);
    assert_eq!(node.children[0].text.as_deref(), Some("-"));
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
}

#[test]
fn test_unary_not() {
    let node = parse_expr("!flag");
    assert_eq!(node.kind, NodeKind::UnaryExpression);
    assert_eq!(node.children[0].text.as_deref(), Some("!"));
}

#[test]
fn test_call_expression() {
    let node = parse_expr("foo(1, 2)");
    assert_eq!(node.kind, NodeKind::CallExpression);
    assert_eq!(node.children.len(), 2); // callee, ArgumentList
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[0].text.as_deref(), Some("foo"));
    assert_eq!(node.children[1].kind, NodeKind::ArgumentList);
}

#[test]
fn test_call_expression_empty() {
    let node = parse_expr("bar()");
    assert_eq!(node.kind, NodeKind::CallExpression);
    assert_eq!(node.children.len(), 2); // callee, ArgumentList (even if empty)
    assert_eq!(node.children[1].kind, NodeKind::ArgumentList);
}

#[test]
fn test_index_expression() {
    let node = parse_expr("arr[1]");
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 2); // object, index
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_index_expression_multi() {
    let node = parse_expr("matrix[i, j]");
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 3); // object, idx1, idx2
}

#[test]
fn test_field_expression() {
    let node = parse_expr("obj.field");
    assert_eq!(node.kind, NodeKind::FieldExpression);
    assert_eq!(node.children.len(), 2); // object, field
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].text.as_deref(), Some("field"));
}

#[test]
fn test_chained_field() {
    let node = parse_expr("a.b.c");
    assert_eq!(node.kind, NodeKind::FieldExpression);
    assert_eq!(node.children[0].kind, NodeKind::FieldExpression); // nested
    assert_eq!(node.children[1].text.as_deref(), Some("c"));
}

#[test]
fn test_ternary_expression() {
    let node = parse_expr("x > 0 ? 1 : 0");
    assert_eq!(node.kind, NodeKind::TernaryExpression);
    assert_eq!(node.children.len(), 3); // condition, then, else
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression); // x > 0
    assert_eq!(node.children[1].kind, NodeKind::IntegerLiteral); // 1
    assert_eq!(node.children[2].kind, NodeKind::IntegerLiteral); // 0
}

#[test]
fn test_type_declaration() {
    let node = parse_expr("x::Int");
    assert_eq!(node.kind, NodeKind::TypedExpression);
    assert_eq!(node.children.len(), 2);
    assert_eq!(node.children[0].kind, NodeKind::Identifier); // x
    assert_eq!(node.children[1].kind, NodeKind::Identifier); // Int
}

#[test]
fn test_broadcast_call() {
    let node = parse_expr("f.(x, y)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 3); // callee, x, y
}

#[test]
fn test_complex_expression() {
    // Test combination: call, index, binary
    let node = parse_expr("arr[1] + foo(2)");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[0].kind, NodeKind::IndexExpression);
    assert_eq!(node.children[2].kind, NodeKind::CallExpression);
}

#[test]
fn test_comparison_chain() {
    // Julia allows chained comparisons: a < b < c
    let node = parse_expr("a < b");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[1].text.as_deref(), Some("<"));
}

#[test]
fn test_logical_operators() {
    let node = parse_expr("a && b || c");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    // || has lower precedence, so it's the outer operator
    assert_eq!(node.children[1].text.as_deref(), Some("||"));
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[0].children[1].text.as_deref(), Some("&&"));
}

#[test]
fn test_pipe_operator() {
    let node = parse_expr("x |> f |> g");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    // |> is left associative
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[1].text.as_deref(), Some("|>"));
}

#[test]
fn test_range_expression() {
    let node = parse_expr("1:10");
    assert_eq!(node.kind, NodeKind::RangeExpression);
}

#[test]
fn test_range_with_step() {
    let node = parse_expr("1:2:10");
    assert_eq!(node.kind, NodeKind::RangeExpression);
}

// ==================== Statement Tests ====================

#[test]
fn test_function_simple() {
    let node = parse_stmt("function foo() end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    assert!(node.children.len() >= 2); // name, [params], body
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[0].text.as_deref(), Some("foo"));
}

#[test]
fn test_function_with_params() {
    let node = parse_stmt("function add(x, y) x + y end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    // Check params
    let params = &node.children[1];
    assert_eq!(params.kind, NodeKind::ParameterList);
    assert_eq!(params.children.len(), 2);
}

#[test]
fn test_function_with_typed_params() {
    let node = parse_stmt("function foo(x::Int) x end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    let params = &node.children[1];
    assert_eq!(params.kind, NodeKind::ParameterList);
    // Parameter should have type annotation
    assert_eq!(params.children[0].kind, NodeKind::Parameter);
    assert!(params.children[0].children.len() >= 2);
}

#[test]
fn test_if_simple() {
    let node = parse_stmt("if true x end");
    assert_eq!(node.kind, NodeKind::IfStatement);
    assert!(node.children.len() >= 2); // condition, body
}

#[test]
fn test_if_else() {
    let node = parse_stmt("if x y else z end");
    assert_eq!(node.kind, NodeKind::IfStatement);
    // Should have: condition, then-block, else-clause
    assert!(node.children.len() >= 3);
    // Last child should be else clause
    let last = node.children.last().unwrap();
    assert_eq!(last.kind, NodeKind::ElseClause);
}

#[test]
fn test_if_elseif_else() {
    let node = parse_stmt("if a b elseif c d else e end");
    assert_eq!(node.kind, NodeKind::IfStatement);
    // Should have: condition, then-block, elseif-clause, else-clause
    let has_elseif = node
        .children
        .iter()
        .any(|c| c.kind == NodeKind::ElseifClause);
    let has_else = node.children.iter().any(|c| c.kind == NodeKind::ElseClause);
    assert!(has_elseif);
    assert!(has_else);
}

#[test]
fn test_for_loop() {
    let node = parse_stmt("for i in 1:10 x end");
    assert_eq!(node.kind, NodeKind::ForStatement);
    assert_eq!(node.children.len(), 2); // binding, body
    assert_eq!(node.children[0].kind, NodeKind::ForBinding);
}

#[test]
fn test_while_loop() {
    let node = parse_stmt("while x > 0 x -= 1 end");
    assert_eq!(node.kind, NodeKind::WhileStatement);
    assert_eq!(node.children.len(), 2); // condition, body
}

#[test]
fn test_try_catch() {
    let node = parse_stmt("try x catch e y end");
    assert_eq!(node.kind, NodeKind::TryStatement);
    let has_catch = node
        .children
        .iter()
        .any(|c| c.kind == NodeKind::CatchClause);
    assert!(has_catch);
}

#[test]
fn test_try_finally() {
    let node = parse_stmt("try x finally y end");
    assert_eq!(node.kind, NodeKind::TryStatement);
    let has_finally = node
        .children
        .iter()
        .any(|c| c.kind == NodeKind::FinallyClause);
    assert!(has_finally);
}

#[test]
fn test_try_catch_finally() {
    let node = parse_stmt("try x catch y finally z end");
    assert_eq!(node.kind, NodeKind::TryStatement);
    let has_catch = node
        .children
        .iter()
        .any(|c| c.kind == NodeKind::CatchClause);
    let has_finally = node
        .children
        .iter()
        .any(|c| c.kind == NodeKind::FinallyClause);
    assert!(has_catch);
    assert!(has_finally);
}

#[test]
fn test_return_with_value() {
    let node = parse_stmt("return x + 1");
    assert_eq!(node.kind, NodeKind::ReturnStatement);
    assert_eq!(node.children.len(), 1);
}

#[test]
fn test_return_empty() {
    let node = parse_stmt("return");
    assert_eq!(node.kind, NodeKind::ReturnStatement);
    assert_eq!(node.children.len(), 0);
}

#[test]
fn test_break() {
    let node = parse_stmt("break");
    assert_eq!(node.kind, NodeKind::BreakStatement);
}

#[test]
fn test_continue() {
    let node = parse_stmt("continue");
    assert_eq!(node.kind, NodeKind::ContinueStatement);
}

#[test]
fn test_let_expression() {
    let node = parse_stmt("let x = 1 x end");
    assert_eq!(node.kind, NodeKind::LetExpression);
}

#[test]
fn test_begin_block() {
    let node = parse_stmt("begin x y end");
    assert_eq!(node.kind, NodeKind::BeginBlock);
}

#[test]
fn test_struct_simple() {
    let node = parse_stmt("struct Point x y end");
    assert_eq!(node.kind, NodeKind::StructDefinition);
}

#[test]
fn test_mutable_struct() {
    let node = parse_stmt("mutable struct Counter value end");
    assert_eq!(node.kind, NodeKind::MutableStructDefinition);
}

#[test]
fn test_abstract_type() {
    let node = parse_stmt("abstract type Shape end");
    assert_eq!(node.kind, NodeKind::AbstractDefinition);
}

#[test]
fn test_module() {
    let node = parse_stmt("module MyMod end");
    assert_eq!(node.kind, NodeKind::ModuleDefinition);
}

#[test]
fn test_using() {
    let node = parse_stmt("using LinearAlgebra");
    assert_eq!(node.kind, NodeKind::UsingStatement);
}

#[test]
fn test_import() {
    let node = parse_stmt("import Base");
    assert_eq!(node.kind, NodeKind::ImportStatement);
}

#[test]
fn test_export() {
    let node = parse_stmt("export foo, bar");
    assert_eq!(node.kind, NodeKind::ExportStatement);
    assert_eq!(node.children.len(), 2);
}

#[test]
fn test_const_declaration() {
    let node = parse_stmt("const PI = 3.14");
    assert_eq!(node.kind, NodeKind::ConstDeclaration);
}

#[test]
fn test_global_declaration() {
    let node = parse_stmt("global x");
    assert_eq!(node.kind, NodeKind::GlobalDeclaration);
}

#[test]
fn test_local_declaration() {
    let node = parse_stmt("local y");
    assert_eq!(node.kind, NodeKind::LocalDeclaration);
}

// ==================== Matrix/Comprehension Tests ====================

#[test]
fn test_matrix_simple() {
    let node = parse_expr("[1 2; 3 4]");
    assert_eq!(node.kind, NodeKind::MatrixExpression);
    assert_eq!(node.children.len(), 2); // 2 rows
    assert_eq!(node.children[0].kind, NodeKind::MatrixRow);
    assert_eq!(node.children[1].kind, NodeKind::MatrixRow);
}

#[test]
fn test_row_vector() {
    let node = parse_expr("[1 2 3]");
    assert_eq!(node.kind, NodeKind::MatrixExpression);
    assert_eq!(node.children.len(), 1); // 1 row
    assert_eq!(node.children[0].kind, NodeKind::MatrixRow);
    assert_eq!(node.children[0].children.len(), 3);
}

#[test]
fn test_comprehension_simple() {
    let node = parse_expr("[x for x in 1:10]");
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
    assert_eq!(node.children.len(), 2); // expr + for clause
}

#[test]
fn test_comprehension_with_if() {
    let node = parse_expr("[x for x in 1:10 if x > 5]");
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
    assert_eq!(node.children.len(), 3); // expr + for clause + if clause
}

#[test]
fn test_comprehension_nested() {
    let node = parse_expr("[x + y for x in 1:3 for y in 1:3]");
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
    assert_eq!(node.children.len(), 3); // expr + 2 for clauses
}

#[test]
fn test_generator_simple() {
    let node = parse_expr("(x for x in 1:10)");
    assert_eq!(node.kind, NodeKind::Generator);
    assert_eq!(node.children.len(), 2); // expr + for clause
}

#[test]
fn test_generator_with_if() {
    let node = parse_expr("(x^2 for x in 1:10 if x % 2 == 0)");
    assert_eq!(node.kind, NodeKind::Generator);
    assert_eq!(node.children.len(), 3); // expr + for clause + if clause
}

#[test]
fn test_generator_nested() {
    let node = parse_expr("((i, j) for i in 1:3 for j in 1:3)");
    assert_eq!(node.kind, NodeKind::Generator);
    assert_eq!(node.children.len(), 3); // tuple expr + 2 for clauses
}

#[test]
fn test_sum_with_generator() {
    let node = parse_expr("sum(x^2 for x in 1:10)");
    assert_eq!(node.kind, NodeKind::CallExpression);
    // The argument should be a generator
    assert!(node.children.len() >= 2);
}

#[test]
fn test_broadcast_dotop_as_function() {
    // Test broadcast operators used as functions: .+(x), .-([1,2,3]), etc.
    let node = parse_expr(".+(x)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_expr(".-(1, 2)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_expr(".*([1, 2], [3, 4])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_expr(".+([1, 2, 3])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 2); // operator + one argument (the array)
}
