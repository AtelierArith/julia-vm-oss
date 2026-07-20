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

#[test]
fn parser_recovers_and_formats_multiple_independent_errors_issue_8454() {
    let source = ")\n]\n";
    let (_root, errors) = parse_with_errors(source);

    assert_eq!(
        errors.len(),
        2,
        "expected both delimiter errors to be reported, got {:?}",
        errors.errors()
    );

    assert_eq!(
        errors.format_all(source),
        "Error 1: unexpected token ')' at 1:1..1:2, expected expression\n  1 | )\n    | ^\n\nError 2: unexpected token ']' at 2:1..2:2, expected expression\n  2 | ]\n    | ^"
    );
}

// ==================== Literal Tests ====================

#[test]
fn test_integer_literals() {
    let source = "42";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("42"));

    let source = "0xff";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("0xff"));

    let source = "0b1010";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("0b1010"));

    let source = "1_000_000";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IntegerLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("1_000_000"));
}

#[test]
fn test_float_literals() {
    let source = "3.14";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::FloatLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("3.14"));

    let source = "1e-5";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::FloatLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("1e-5"));

    let source = ".5";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::FloatLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some(".5"));
}

#[test]
fn test_boolean_literals() {
    let source = "true";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BooleanLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("true"));

    let source = "false";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BooleanLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("false"));
}

#[test]
fn test_character_literals() {
    let source = "'a'";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::CharacterLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("'a'"));

    let source = "'\\n'";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::CharacterLiteral);
    assert_eq!(Some(node.text_from_source(source)), Some("'\\n'"));
}

#[test]
fn test_identifiers() {
    let source = "foo";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::Identifier);
    assert_eq!(Some(node.text_from_source(source)), Some("foo"));

    let source = "bar_baz";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::Identifier);
    assert_eq!(Some(node.text_from_source(source)), Some("bar_baz"));
}

#[test]
fn test_string_literals() {
    let source = "\"hello\"";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::StringLiteral);
    assert_eq!(node.children.len(), 1); // content
    assert_eq!(
        Some(node.children[0].text_from_source(source)),
        Some("hello")
    );
}

#[test]
fn test_string_interpolation() {
    let source = "\"hello $name\"";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::StringLiteral);
    assert_eq!(node.children.len(), 2); // "hello " and $name
    assert_eq!(node.children[0].kind, NodeKind::Content);
    assert_eq!(node.children[1].kind, NodeKind::StringInterpolation);
}

// ==================== Collection Tests ====================

#[test]
fn test_tuple() {
    let source = "(1, 2, 3)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TupleExpression);
    assert_eq!(node.children.len(), 3);
}

#[test]
fn test_empty_tuple() {
    let source = "()";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TupleExpression);
    assert_eq!(node.children.len(), 0);
}

#[test]
fn test_parenthesized() {
    let source = "(42)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::ParenthesizedExpression);
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_vector() {
    let source = "[1, 2, 3]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::VectorExpression);
    assert_eq!(node.children.len(), 3);
}

#[test]
fn test_empty_vector() {
    let source = "[]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::VectorExpression);
    assert_eq!(node.children.len(), 0);
}

// ==================== Expression Tests ====================

#[test]
fn test_binary_expression_add() {
    let source = "1 + 2";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children.len(), 3); // left, op, right
    assert_eq!(node.children[0].kind, NodeKind::IntegerLiteral);
    assert_eq!(node.children[1].kind, NodeKind::Operator);
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("+"));
    assert_eq!(node.children[2].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_binary_expression_precedence() {
    // 1 + 2 * 3 should parse as 1 + (2 * 3)
    let source = "1 + 2 * 3";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("+")); // outer is +
    assert_eq!(node.children[2].kind, NodeKind::BinaryExpression); // right is binary
    assert_eq!(
        Some(node.children[2].children[1].text_from_source(source)),
        Some("*")
    );
}

#[test]
fn test_binary_expression_left_assoc() {
    // 1 - 2 - 3 should parse as (1 - 2) - 3
    let source = "1 - 2 - 3";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression); // left is binary
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("-"));
    assert_eq!(node.children[2].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_binary_expression_right_assoc() {
    // 2 ^ 3 ^ 4 should parse as 2 ^ (3 ^ 4)
    let source = "2 ^ 3 ^ 4";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[0].kind, NodeKind::IntegerLiteral);
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("^"));
    assert_eq!(node.children[2].kind, NodeKind::BinaryExpression); // right is binary
}

#[test]
fn test_unary_expression() {
    let source = "-x";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::UnaryExpression);
    assert_eq!(node.children.len(), 2); // op, operand
    assert_eq!(node.children[0].kind, NodeKind::Operator);
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("-"));
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
}

#[test]
fn test_unary_not() {
    let source = "!flag";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::UnaryExpression);
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("!"));
}

#[test]
fn test_call_expression() {
    let source = "foo(1, 2)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::CallExpression);
    assert_eq!(node.children.len(), 2); // callee, ArgumentList
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("foo"));
    assert_eq!(node.children[1].kind, NodeKind::ArgumentList);
}

#[test]
fn test_call_expression_empty() {
    let source = "bar()";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::CallExpression);
    assert_eq!(node.children.len(), 2); // callee, ArgumentList (even if empty)
    assert_eq!(node.children[1].kind, NodeKind::ArgumentList);
}

#[test]
fn test_index_expression() {
    let source = "arr[1]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 2); // object, index
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::IntegerLiteral);
}

#[test]
fn test_index_expression_multi() {
    let source = "matrix[i, j]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::IndexExpression);
    assert_eq!(node.children.len(), 3); // object, idx1, idx2
}

#[test]
fn test_field_expression() {
    let source = "obj.field";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::FieldExpression);
    assert_eq!(node.children.len(), 2); // object, field
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::Identifier);
    assert_eq!(
        Some(node.children[1].text_from_source(source)),
        Some("field")
    );
}

#[test]
fn test_chained_field() {
    let source = "a.b.c";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::FieldExpression);
    assert_eq!(node.children[0].kind, NodeKind::FieldExpression); // nested
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("c"));
}

#[test]
fn test_ternary_expression() {
    let source = "x > 0 ? 1 : 0";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TernaryExpression);
    assert_eq!(node.children.len(), 3); // condition, then, else
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression); // x > 0
    assert_eq!(node.children[1].kind, NodeKind::IntegerLiteral); // 1
    assert_eq!(node.children[2].kind, NodeKind::IntegerLiteral); // 0
}

#[test]
fn test_type_declaration() {
    let source = "x::Int";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TypedExpression);
    assert_eq!(node.children.len(), 2);
    assert_eq!(node.children[0].kind, NodeKind::Identifier); // x
    assert_eq!(node.children[1].kind, NodeKind::Identifier); // Int
}

#[test]
fn test_broadcast_call() {
    let source = "f.(x, y)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 3); // callee, x, y
}

#[test]
fn test_complex_expression() {
    // Test combination: call, index, binary
    let source = "arr[1] + foo(2)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(node.children[0].kind, NodeKind::IndexExpression);
    assert_eq!(node.children[2].kind, NodeKind::CallExpression);
}

#[test]
fn test_comparison_chain() {
    // Julia allows chained comparisons: a < b < c
    let source = "a < b";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("<"));
}

#[test]
fn test_logical_operators() {
    let source = "a && b || c";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    // || has lower precedence, so it's the outer operator
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("||"));
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    assert_eq!(
        Some(node.children[0].children[1].text_from_source(source)),
        Some("&&")
    );
}

#[test]
fn test_pipe_operator() {
    let source = "x |> f |> g";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    // |> is left associative
    assert_eq!(node.children[0].kind, NodeKind::BinaryExpression);
    assert_eq!(Some(node.children[1].text_from_source(source)), Some("|>"));
}

#[test]
fn test_range_expression() {
    let source = "1:10";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::RangeExpression);
}

#[test]
fn test_range_with_step() {
    let source = "1:2:10";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::RangeExpression);
}

// ==================== Statement Tests ====================

#[test]
fn test_function_simple() {
    let source = "function foo() end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    assert!(node.children.len() >= 2); // name, [params], body
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(Some(node.children[0].text_from_source(source)), Some("foo"));
}

#[test]
fn test_function_with_params() {
    let source = "function add(x, y) x + y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    // Check params
    let params = &node.children[1];
    assert_eq!(params.kind, NodeKind::ParameterList);
    assert_eq!(params.children.len(), 2);
}

#[test]
fn test_function_with_typed_params() {
    let source = "function foo(x::Int) x end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    let params = &node.children[1];
    assert_eq!(params.kind, NodeKind::ParameterList);
    // Parameter should have type annotation
    assert_eq!(params.children[0].kind, NodeKind::Parameter);
    assert!(params.children[0].children.len() >= 2);
}

#[test]
fn test_if_simple() {
    let source = "if true x end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::IfStatement);
    assert!(node.children.len() >= 2); // condition, body
}

#[test]
fn test_if_else() {
    let source = "if x y else z end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::IfStatement);
    // Should have: condition, then-block, else-clause
    assert!(node.children.len() >= 3);
    // Last child should be else clause
    let last = node.children.last().unwrap();
    assert_eq!(last.kind, NodeKind::ElseClause);
}

#[test]
fn test_if_elseif_else() {
    let source = "if a b elseif c d else e end";
    let node = parse_stmt(source);
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
    let source = "for i in 1:10 x end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ForStatement);
    assert_eq!(node.children.len(), 2); // binding, body
    assert_eq!(node.children[0].kind, NodeKind::ForBinding);
}

#[test]
fn test_for_loop_typed_variable_issue_8208() {
    // `for i::T in itr` parses the loop variable as a TypedExpression binding
    // (upstream Julia syntax). Previously this hit "expected 'in' or '='".
    let source = "for i::Int64 in 1:10 x end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ForStatement);
    let binding = &node.children[0];
    assert_eq!(binding.kind, NodeKind::ForBinding);
    assert_eq!(
        binding.children[0].kind,
        NodeKind::TypedExpression,
        "loop variable should parse as `i::Int64`"
    );
    // `=` head form parses the same way.
    let source = "for i::Int64 = 1:10 x end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ForStatement);
    assert_eq!(node.children[0].children[0].kind, NodeKind::TypedExpression);
}

#[test]
fn test_while_loop() {
    let source = "while x > 0 x -= 1 end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::WhileStatement);
    assert_eq!(node.children.len(), 2); // condition, body
}

#[test]
fn test_try_catch() {
    let source = "try x catch e y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::TryStatement);
    let has_catch = node
        .children
        .iter()
        .any(|c| c.kind == NodeKind::CatchClause);
    assert!(has_catch);
}

#[test]
fn test_try_finally() {
    let source = "try x finally y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::TryStatement);
    let has_finally = node
        .children
        .iter()
        .any(|c| c.kind == NodeKind::FinallyClause);
    assert!(has_finally);
}

#[test]
fn test_try_catch_finally() {
    let source = "try x catch y finally z end";
    let node = parse_stmt(source);
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
    let source = "return x + 1";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ReturnStatement);
    assert_eq!(node.children.len(), 1);
}

#[test]
fn test_return_empty() {
    let source = "return";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ReturnStatement);
    assert_eq!(node.children.len(), 0);
}

#[test]
fn test_break() {
    let source = "break";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::BreakStatement);
}

#[test]
fn test_continue() {
    let source = "continue";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ContinueStatement);
}

#[test]
fn test_let_expression() {
    let source = "let x = 1 x end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::LetExpression);
}

#[test]
fn test_begin_block() {
    let source = "begin x y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::BeginBlock);
}

#[test]
fn test_struct_simple() {
    let source = "struct Point x y end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::StructDefinition);
}

#[test]
fn test_mutable_struct() {
    let source = "mutable struct Counter value end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::MutableStructDefinition);
}

#[test]
fn test_abstract_type() {
    let source = "abstract type Shape end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::AbstractDefinition);
}

#[test]
fn test_module() {
    let source = "module MyMod end";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ModuleDefinition);
}

#[test]
fn test_using() {
    let source = "using LinearAlgebra";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::UsingStatement);
}

#[test]
fn test_import() {
    let source = "import Base";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ImportStatement);
}

#[test]
fn test_export() {
    let source = "export foo, bar";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ExportStatement);
    assert_eq!(node.children.len(), 2);
}

#[test]
fn test_const_declaration() {
    let source = "const PI = 3.14";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::ConstDeclaration);
}

#[test]
fn test_global_declaration() {
    let source = "global x";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::GlobalDeclaration);
}

#[test]
fn test_local_declaration() {
    let source = "local y";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::LocalDeclaration);
}

// Long-form definitions as `global`/`local` declaration items
// (Issue #10937, found via the corpus ratchet redness of Issue #10935).
// Upstream parses the reserved word through parse-eq, producing
// `(global (function ...))`; before this fix the keyword was swallowed by
// `parse_identifier` and the definition's `end` was left dangling, which
// #10927's bare-`end` rejection turned into a hard parse error on 6
// `julia/base` files (Base.jl, dict.jl, iobuffer.jl, mpfr.jl, range.jl,
// reducedim.jl).

#[test]
fn test_global_function_longform_declaration_10937() {
    let source = "global function f(x)\n    x + 1\nend";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::GlobalDeclaration);
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].kind, NodeKind::FunctionDefinition);
}

#[test]
fn test_local_function_longform_declaration_10937() {
    let source = "local function g()\n    1\nend";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::LocalDeclaration);
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].kind, NodeKind::FunctionDefinition);
}

#[test]
fn test_global_macro_longform_declaration_10937() {
    let source = "global macro m()\n    1\nend";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::GlobalDeclaration);
    assert_eq!(node.children.len(), 1);
    assert_eq!(node.children[0].kind, NodeKind::MacroDefinition);
}

#[test]
fn test_let_global_function_base_bootstrap_pattern_10935() {
    // The exact shape used by julia/base (e.g. Base.jl's relative-include
    // bootstrap): a `let` whose body declares a global long-form function.
    let source = "let SOURCE_PATH = \"\"\n    global function f(path)\n        path\n    end\nend";
    let (_root, errors) = parse_with_errors(source);
    assert!(
        errors.is_empty(),
        "let + global function must parse: {:?}",
        errors.errors()
    );
}

// ==================== Matrix/Comprehension Tests ====================

#[test]
fn test_matrix_simple() {
    // The `;` separator between the two rows now surfaces as an explicit
    // `Semicolon` CST leaf (not just "2 adjacent rows"), so lowering can
    // recover the separator's dimension level for N-D literals like
    // `;;`/`;;;`/... (Issue #10190).
    let source = "[1 2; 3 4]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::MatrixExpression);
    assert_eq!(node.children.len(), 3); // row, `;`, row
    assert_eq!(node.children[0].kind, NodeKind::MatrixRow);
    assert_eq!(node.children[1].kind, NodeKind::Semicolon);
    assert_eq!(node.children[2].kind, NodeKind::MatrixRow);
}

// Issue #10398: sjulia leniently accepted several array-literal separator
// forms that upstream `julia --startup-file=no` (1.12.6, JuliaSyntax.jl
// parser) rejects at parse time with a `ParseError`. The rule, derived from
// upstream's `parse_array_separator` (see the issue for the trace):
//
//   * A `;;` run (exactly two semicolons) that follows a row which already
//     joined >= 2 elements with a bare space ("row-major") is illegal
//     unless the run is *immediately* followed by a newline — that one
//     shape is Julia's "wrap a line" idiom and does not start a new
//     row/dimension at all (contrast case below).
//   * A `;`-run may never be split by intervening non-newline content (a
//     bare space between two semicolons), and never resumes after a
//     newline already ended it.
//
// These tests use `parse_with_errors` directly (rather than the
// panics-on-error `parse_expr`/`parse_stmt` helpers above) because the
// point is to assert a `ParseError` *is* produced.

#[test]
fn test_issue_10398_array_literal_rejects_space_and_double_semicolon_mix() {
    // Form A (`[1 2;; 3 4]`) and its close variants: a `;;` run following a
    // row-major row, not immediately followed by a newline.
    for source in [
        "[1 2;; 3 4]",
        "[1 2 ;; 3 4]",
        "[1 2;;3 4]",
        "[1 2\n;;3 4]", // a leading newline before `;;` doesn't count as wrap
        "[1 2;;]",      // trailing `;;` right before `]` is not a wrap either
        "[1 2 ;;]",
        "[1 2;; ]",
        "[1 2; 3 4;;]", // row-major state persists across an earlier `;` row
    ] {
        let (_root, errors) = parse_with_errors(source);
        assert!(
            !errors.is_empty(),
            "expected a parse error for {source:?} (upstream julia rejects it \
             with \"cannot mix space and ;; separators...\")"
        );
    }
}

#[test]
fn test_issue_10398_array_literal_rejects_semicolon_run_split_by_space() {
    // Form B (`[1 2; ; 3 4]`) and its close variants: a `;`-run split by a
    // bare space is illegal even when the exactly-two-semicolons mixing
    // rule above doesn't apply (e.g. no row-major row yet, or a 3-run).
    for source in ["[1 2; ; 3 4]", "[1; ;2]", "[1 2;; ;3 4]", "[1 ; ; 2]"] {
        let (_root, errors) = parse_with_errors(source);
        assert!(
            !errors.is_empty(),
            "expected a parse error for {source:?} (upstream julia rejects a \
             `;`-run split by whitespace)"
        );
    }
}

#[test]
fn test_issue_10398_array_literal_rejects_semicolon_run_split_by_newline() {
    // Form C (`[1 2;\n;3 4]`): a `;`-run split across a newline never
    // resumes — the dangling second `;` can't start the next row's first
    // expression, matching upstream's "Expected `]`" rejection.
    for source in ["[1 2;\n;3 4]", "[1;\n;2]"] {
        let (_root, errors) = parse_with_errors(source);
        assert!(
            !errors.is_empty(),
            "expected a parse error for {source:?} (upstream julia rejects a \
             `;`-run split across a line break)"
        );
    }
}

#[test]
fn test_issue_10398_array_literal_accepts_legal_line_wrap_and_separator_forms() {
    // Form D (the issue's contrast case) plus a battery of legal
    // `;`/`;;`/`;;;`/newline separator combinations that must keep parsing
    // without error — non-regression for the #10190/#10398 array-literal
    // separator grammar.
    for source in [
        "[1 2;;\n3 4]",   // Form D: `;;` immediately followed by a newline
        "[1 2;;  \n3 4]", // trailing spaces before the wrap newline are fine
        "[1 2;;\n\n3 4]", // multiple blank lines after the wrap
        "[1; 2;; 3; 4]",
        "[1 2;;;3 4]", // exactly 3 semicolons is never mismatch-checked upstream
        "[1 2 ;;; 3 4]",
        "[1;2;;3;4]",
        "[1 ;2]",
        "[1;;\n\n2]",
        "[1 2\n3 4]",
        "[1\n2\n3]",
        "[1 2\n\n3 4]",
        "[1;\n2]",
        "[1;\n\n2]",
        "[1 2;\n3 4]", // the single-`;`-then-newline multi-element form most
        // real Julia matrix literals use — exercises the
        // changed newline-absorption path (n_semis == 1, so
        // the mixing check never applies)
        "[1 2;\n3 4;\n5 6]",
        "[1;;]", // trailing `;;` is fine when no row ever used space
        "[1;; ]",
        "[1; 2;;]",
        "[1 2; 3 4;;;]",
        "[1; 2; 3;; 4; 5; 6]",
        "[1 2; 3 4]",
        "[1 2 3]",
        "[1, 2, 3]",
    ] {
        let (_root, errors) = parse_with_errors(source);
        assert!(
            errors.is_empty(),
            "expected {source:?} to parse without error (upstream julia \
             accepts it), got: {:?}",
            errors.errors()
        );
    }
}

#[test]
fn test_issue_10518_rejects_space_row_after_column_major_separator() {
    for source in ["[1;; 2 3]", "Int64[1;; 2 3]"] {
        let (_root, errors) = parse_with_errors(source);
        assert!(
            !errors.is_empty(),
            "expected a parse error for {source:?} (upstream julia rejects mixing `;;` and spaces)"
        );
    }
}

#[test]
fn test_issue_10519_double_semicolon_newline_continues_current_row() {
    let node = parse_expr("[1 2; 3 4;;\n5 6]");
    assert_eq!(node.kind, NodeKind::MatrixExpression);
    assert_eq!(node.children.len(), 3); // row, `;`, continued row
    assert_eq!(node.children[0].children.len(), 2);
    assert_eq!(node.children[1].kind, NodeKind::Semicolon);
    assert_eq!(node.children[2].children.len(), 4);

    let typed = parse_expr("Int64[1 2; 3 4;;\n5 6]");
    let matrix = typed
        .children
        .iter()
        .find(|child| child.kind == NodeKind::MatrixExpression)
        .expect("typed literal contains a matrix expression");
    assert_eq!(matrix.children.len(), 3);
    assert_eq!(matrix.children[2].children.len(), 4);
}

#[test]
fn test_issue_10918_end_is_rejected_outside_index_context() {
    for source in ["end", "(end)", "f(end)", "[end]", "1:end"] {
        let expected_start = source.find("end").unwrap();
        assert_issue_10918_rejects_end(source, expected_start);
    }
}

fn assert_issue_10918_rejects_end(source: &str, expected_start: usize) {
    let (_root, errors) = parse_with_errors(source);
    assert!(
        errors.errors().iter().any(|error| {
            error.span().is_some_and(|span| {
                span.start == expected_start && span.end == expected_start + "end".len()
            })
        }),
        "expected an error spanning the invalid `end` in {source:?}, got: {:?}",
        errors.errors()
    );
}

#[test]
fn test_issue_10918_end_remains_valid_through_nested_index_contexts() {
    for source in [
        "a[end]",
        "a[end - 1]",
        "a[1, end]",
        "a[(end)]",
        "a[f(end)]",
        "a[b[end]]",
        "a[[end]]",
        "a[end; 1]",
        "a[[1; end]]",
        "a[end for i = 1:1]",
        "a[[end for i = 1:1]]",
        "a[[i for i = 1:b[end]]]",
        "a[[i for i = 1:1 if b[end]]]",
        "a[(i for i = 1:end)]",
        "a[f(i for i = 1:end)]",
        "a[[i for i = 1:1], end]",
    ] {
        let (_root, errors) = parse_with_errors(source);
        assert!(
            errors.is_empty(),
            "expected {source:?} to parse without error like upstream Julia, got: {:?}",
            errors.errors()
        );
    }
}

#[test]
fn test_issue_10918_cat_and_comprehension_restore_outer_end_context() {
    for source in [
        "a[1 end]",
        "a[1; end]",
        "a[1;; end]",
        "a[; end]",
        "a[i for i = 1:end]",
        "a[[i for i = 1:end]]",
        "a[[i for i = 1:1 if end]]",
    ] {
        assert_issue_10918_rejects_end(source, source.rfind("end").unwrap());
    }
}

#[test]
fn test_issue_10918_quote_resets_end_context() {
    for (source, expected_start) in [
        ("a[:(end)]", 4),
        ("a[quote (end) end]", 9),
        ("a[quote f(end) end]", 10),
    ] {
        assert_issue_10918_rejects_end(source, expected_start);
    }

    for source in [
        "a[:end]",
        "a[:x, end]",
        "a[:(x), end]",
        "a[:(b[end])]",
        "a[quote b[end] end]",
        "a[quote x end, end]",
    ] {
        let (_root, errors) = parse_with_errors(source);
        assert!(
            errors.is_empty(),
            "expected {source:?} to parse without error like upstream Julia, got: {:?}",
            errors.errors()
        );
    }
}

#[test]
fn test_issue_10918_index_context_is_restored_after_parse_error() {
    let source = "a[)\nend";
    let (_root, errors) = parse_with_errors(source);

    assert!(
        errors.errors().iter().any(|error| {
            error
                .span()
                .is_some_and(|span| span.start == 4 && span.end == 7)
        }),
        "the top-level `end` after a malformed index must still be rejected: {:?}",
        errors.errors()
    );
}

#[test]
fn test_row_vector() {
    let source = "[1 2 3]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::MatrixExpression);
    assert_eq!(node.children.len(), 1); // 1 row
    assert_eq!(node.children[0].kind, NodeKind::MatrixRow);
    assert_eq!(node.children[0].children.len(), 3);
}

#[test]
fn test_comprehension_simple() {
    let source = "[x for x in 1:10]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
    assert_eq!(node.children.len(), 2); // expr + for clause
}

#[test]
fn test_typed_comprehension_simple() {
    let source = "Float64[i / 10.0 for i in 1:10]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TypedExpression);
    assert_eq!(node.children.len(), 2); // type + comprehension
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::ComprehensionExpression);
}

#[test]
fn test_comprehension_with_if() {
    let source = "[x for x in 1:10 if x > 5]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
    assert_eq!(node.children.len(), 3); // expr + for clause + if clause
}

#[test]
fn test_comprehension_nested() {
    let source = "[x + y for x in 1:3 for y in 1:3]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
    assert_eq!(node.children.len(), 3); // expr + 2 for clauses
}

#[test]
fn test_generator_simple() {
    let source = "(x for x in 1:10)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::Generator);
    assert_eq!(node.children.len(), 2); // expr + for clause
}

#[test]
fn test_generator_with_if() {
    let source = "(x^2 for x in 1:10 if x % 2 == 0)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::Generator);
    assert_eq!(node.children.len(), 3); // expr + for clause + if clause
}

#[test]
fn test_generator_nested() {
    let source = "((i, j) for i in 1:3 for j in 1:3)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::Generator);
    assert_eq!(node.children.len(), 3); // tuple expr + 2 for clauses
}

#[test]
fn test_sum_with_generator() {
    let source = "sum(x^2 for x in 1:10)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::CallExpression);
    // The argument should be a generator
    assert!(node.children.len() >= 2);
}

#[test]
fn test_broadcast_dotop_as_function() {
    // Test broadcast operators used as functions: .+(x), .-([1,2,3]), etc.
    let source = ".+(x)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let source = ".-(1, 2)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let source = ".*([1, 2], [3, 4])";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let source = ".+([1, 2, 3])";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    assert_eq!(node.children.len(), 2); // operator + one argument (the array)
}

// Issue #8759: additional corpus parser gap fixes

#[test]
fn test_issue_8759_quoted_plusplus_operator() {
    // `:++` — `++` treated as an operator symbol in quote context
    let source = ":++";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::QuoteExpression);
}

#[test]
fn test_issue_8759_quoted_dotdot_operator() {
    // `:.` and `..` as quoted operator symbols
    let source = ":(..)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::QuoteExpression);
}

#[test]
fn test_issue_8759_quoted_question_mark() {
    // `:?` — `?` treated as a symbol, not a ternary operator opener
    let source = ":?";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::QuoteExpression);
}

#[test]
fn test_issue_8759_quoted_dotted_assign_symbol() {
    // `:(.\=)` — compound dotted-assignment operator as quoted symbol
    let node = parse_expr(r":(.\=)");
    assert_eq!(node.kind, NodeKind::QuoteExpression);
}

#[test]
fn test_issue_8759_return_in_parenthesized_block() {
    // `(expr; return)` — return with no value inside parenthesized block
    let source = "(f(); return)";
    let node = parse_expr(source);
    // Parsed as a block expression (parenthesized statement block)
    assert!(node.kind == NodeKind::Block || node.kind == NodeKind::TupleExpression);
}

#[test]
fn test_issue_8759_named_tuple_trailing_comma_semicolon() {
    // `(a=1, ; b=2)` — positional args before `;` in named tuple;
    // parses as a Block (parenthesized statement block with assignment + kwarg)
    let source = "(a=1,; b=2)";
    let node = parse_expr(source);
    assert!(
        node.kind == NodeKind::Block || node.kind == NodeKind::TupleExpression,
        "unexpected kind: {:?}",
        node.kind
    );
}

#[test]
fn test_issue_8759_import_parenthesized_operator() {
    // `using A: (..)` — parenthesized operator in import list
    let source = "using Base: (..)";
    let node = parse_stmt(source);
    assert_eq!(node.kind, NodeKind::UsingStatement);
}

#[test]
fn test_issue_8759_unicode_assignment_operator() {
    // `≔` (ColonEquals, U+2254) — unicode assignment at assign-level precedence
    let source = "x ≔ 5";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::CompoundAssignmentExpression);
}

#[test]
fn test_issue_8759_quoted_operator_in_call_list() {
    // `func in (:+, :++, :*)` — `++` as quoted operator in set membership
    let source = "f in (:+, :++, :*)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::BinaryExpression);
}

#[test]
fn test_issue_8759_quoted_generator_single() {
    // `:(x for x in y)` — generator expression inside a quote
    let source = ":(x for x in y)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::QuoteExpression);
    assert_eq!(node.children[0].kind, NodeKind::Generator);
}

#[test]
fn test_issue_8759_quoted_generator_multi_for() {
    // `:(x for x in y for z in w)` — multi-clause generator inside a quote
    let source = ":(x for x in y for z in w)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::QuoteExpression);
    assert_eq!(node.children[0].kind, NodeKind::Generator);
}

#[test]
fn test_issue_8759_quoted_generator_comma_ranges() {
    // `:(z for z = 1:5, y = 1:5)` — comma-separated iterators in a quoted generator
    let source = ":(z for z = 1:5, y = 1:5)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::QuoteExpression);
    assert_eq!(node.children[0].kind, NodeKind::Generator);
}

#[test]
fn test_issue_8759_interleaved_for_if_generator() {
    // `(x for x in y if aa for z in w if bb)` — interleaved for/if clauses
    let source = "(x for x in y if aa for z in w if bb)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::Generator);
}

#[test]
fn test_issue_8759_interleaved_for_if_comprehension() {
    // `[x for x in y if aa for z in w if bb]` — interleaved for/if in a comprehension
    let source = "[x for x in y if aa for z in w if bb]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
}

#[test]
fn test_issue_8759_dotted_operator_as_value() {
    // `(.&, b)` — a dotted operator used as a first-class value (== `((.&), b)`)
    let source = "(.&, b)";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::TupleExpression);
    assert_eq!(node.children[0].kind, NodeKind::Operator);
}

#[test]
fn test_issue_8759_dotted_operator_value_in_array() {
    // `[.&, .|]` — dotted operators as values inside a vector literal
    let source = "[.&, .|]";
    let node = parse_expr(source);
    assert_eq!(node.kind, NodeKind::VectorExpression);
}
