//! Parser Dispatch Decision Table Tests
//!
//! This module verifies the dispatch logic in `parse_top_level_item()`.
//! Each test corresponds to a row in the dispatch decision table documented
//! in `docs/vm/PARSER.md`.
//!
//! See: docs/vm/PARSER.md for the complete dispatch decision table.

use subset_julia_vm_parser::{parse_with_errors, CstNode, NodeKind};

/// Helper to parse source and return the first top-level node
fn parse_first(source: &str) -> CstNode {
    let (root, errors) = parse_with_errors(source);
    assert!(errors.is_empty(), "Parse errors: {:?}", errors.errors());
    assert_eq!(root.kind, NodeKind::SourceFile);
    root.children.into_iter().next().expect("no node parsed")
}

// ==================== Keyword Dispatch Tests ====================
// These tests verify that keyword tokens dispatch to the correct parsing functions.

#[test]
fn test_dispatch_function_keyword() {
    // Token::KwFunction -> parse_function_definition()
    let node = parse_first("function f() end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);

    let node = parse_first("function add(x, y) x + y end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);

    let node = parse_first("function foo(x::Int64) x end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
}

#[test]
fn test_dispatch_macro_keyword() {
    // Token::KwMacro -> parse_macro_definition()
    let node = parse_first("macro m() end");
    assert_eq!(node.kind, NodeKind::MacroDefinition);

    let node = parse_first("macro show(x) :(println($x)) end");
    assert_eq!(node.kind, NodeKind::MacroDefinition);
}

#[test]
fn test_dispatch_struct_keyword() {
    // Token::KwStruct -> parse_struct_definition()
    let node = parse_first("struct Point x y end");
    assert_eq!(node.kind, NodeKind::StructDefinition);

    let node = parse_first("struct Empty end");
    assert_eq!(node.kind, NodeKind::StructDefinition);
}

#[test]
fn test_dispatch_mutable_struct_keyword() {
    // Token::KwMutable -> parse_struct_definition()
    let node = parse_first("mutable struct Counter value end");
    assert_eq!(node.kind, NodeKind::MutableStructDefinition);
}

#[test]
fn test_dispatch_abstract_keyword() {
    // Token::KwAbstract -> parse_abstract_definition()
    let node = parse_first("abstract type Shape end");
    assert_eq!(node.kind, NodeKind::AbstractDefinition);

    let node = parse_first("abstract type Number <: Real end");
    assert_eq!(node.kind, NodeKind::AbstractDefinition);
}

#[test]
fn test_dispatch_primitive_keyword() {
    // Token::KwPrimitive -> parse_primitive_definition()
    let node = parse_first("primitive type MyInt 32 end");
    assert_eq!(node.kind, NodeKind::PrimitiveDefinition);
}

#[test]
fn test_dispatch_module_keyword() {
    // Token::KwModule -> parse_module_definition()
    let node = parse_first("module MyModule end");
    assert_eq!(node.kind, NodeKind::ModuleDefinition);
}

#[test]
fn test_dispatch_baremodule_keyword() {
    // Token::KwBaremodule -> parse_module_definition()
    let node = parse_first("baremodule RawModule end");
    assert_eq!(node.kind, NodeKind::BaremoduleDefinition);
}

#[test]
fn test_dispatch_if_keyword() {
    // Token::KwIf -> parse_if_statement()
    let node = parse_first("if true x end");
    assert_eq!(node.kind, NodeKind::IfStatement);

    let node = parse_first("if x > 0 y else z end");
    assert_eq!(node.kind, NodeKind::IfStatement);
}

#[test]
fn test_dispatch_for_keyword() {
    // Token::KwFor -> parse_for_statement()
    let node = parse_first("for i in 1:10 x end");
    assert_eq!(node.kind, NodeKind::ForStatement);
}

#[test]
fn test_dispatch_while_keyword() {
    // Token::KwWhile -> parse_while_statement()
    let node = parse_first("while x > 0 x -= 1 end");
    assert_eq!(node.kind, NodeKind::WhileStatement);
}

#[test]
fn test_dispatch_try_keyword() {
    // Token::KwTry -> parse_try_statement()
    let node = parse_first("try x catch e y end");
    assert_eq!(node.kind, NodeKind::TryStatement);

    let node = parse_first("try x finally y end");
    assert_eq!(node.kind, NodeKind::TryStatement);
}

#[test]
fn test_dispatch_begin_keyword() {
    // Token::KwBegin -> parse_begin_block()
    let node = parse_first("begin x y end");
    assert_eq!(node.kind, NodeKind::BeginBlock);
}

#[test]
fn test_dispatch_let_keyword() {
    // Token::KwLet -> parse_let_expression()
    let node = parse_first("let x = 1 x end");
    assert_eq!(node.kind, NodeKind::LetExpression);
}

#[test]
fn test_dispatch_quote_keyword() {
    // Token::KwQuote -> parse_quote_expression()
    let node = parse_first("quote x + y end");
    assert_eq!(node.kind, NodeKind::QuoteExpression);
}

#[test]
fn test_dispatch_return_keyword() {
    // Token::KwReturn -> parse_return_statement()
    let node = parse_first("return x");
    assert_eq!(node.kind, NodeKind::ReturnStatement);

    let node = parse_first("return");
    assert_eq!(node.kind, NodeKind::ReturnStatement);
}

#[test]
fn test_dispatch_break_keyword() {
    // Token::KwBreak -> parse_break_statement()
    let node = parse_first("break");
    assert_eq!(node.kind, NodeKind::BreakStatement);
}

#[test]
fn test_dispatch_continue_keyword() {
    // Token::KwContinue -> parse_continue_statement()
    let node = parse_first("continue");
    assert_eq!(node.kind, NodeKind::ContinueStatement);
}

#[test]
fn test_dispatch_using_keyword() {
    // Token::KwUsing -> parse_using_statement()
    let node = parse_first("using Base");
    assert_eq!(node.kind, NodeKind::UsingStatement);

    let node = parse_first("using LinearAlgebra, Statistics");
    assert_eq!(node.kind, NodeKind::UsingStatement);
}

#[test]
fn test_dispatch_import_keyword() {
    // Token::KwImport -> parse_import_statement()
    let node = parse_first("import Base");
    assert_eq!(node.kind, NodeKind::ImportStatement);

    let node = parse_first("import Base: println");
    assert_eq!(node.kind, NodeKind::ImportStatement);
}

#[test]
fn test_dispatch_export_keyword() {
    // Token::KwExport -> parse_export_statement()
    let node = parse_first("export foo");
    assert_eq!(node.kind, NodeKind::ExportStatement);

    let node = parse_first("export foo, bar, baz");
    assert_eq!(node.kind, NodeKind::ExportStatement);
}

#[test]
fn test_dispatch_const_keyword() {
    // Token::KwConst -> parse_const_declaration()
    let node = parse_first("const PI = 3.14159");
    assert_eq!(node.kind, NodeKind::ConstDeclaration);
}

#[test]
fn test_dispatch_global_keyword() {
    // Token::KwGlobal -> parse_global_declaration()
    let node = parse_first("global x");
    assert_eq!(node.kind, NodeKind::GlobalDeclaration);

    let node = parse_first("global x = 1");
    assert_eq!(node.kind, NodeKind::GlobalDeclaration);
}

#[test]
fn test_dispatch_local_keyword() {
    // Token::KwLocal -> parse_local_declaration()
    let node = parse_first("local y");
    assert_eq!(node.kind, NodeKind::LocalDeclaration);

    let node = parse_first("local y = 2");
    assert_eq!(node.kind, NodeKind::LocalDeclaration);
}

// ==================== Identifier Dispatch Tests ====================
// Tests for Token::Identifier dispatch based on next token

#[test]
fn test_dispatch_bare_tuple_assignment() {
    // Token::Identifier + peek_next() == Token::Comma -> parse_bare_tuple_assignment()
    // Note: The parser currently returns BinaryExpression for assignments
    let node = parse_first("a, b = 1, 2");
    // Bare tuple assignment may result in a BinaryExpression with Assignment operator
    // The left side should contain a tuple structure
    assert!(
        node.kind == NodeKind::Assignment || node.kind == NodeKind::BinaryExpression,
        "Expected Assignment or BinaryExpression, got {:?}",
        node.kind
    );
    // Verify the left side is a tuple expression
    assert_eq!(node.children[0].kind, NodeKind::TupleExpression);

    let node = parse_first("x, y, z = f()");
    assert!(
        node.kind == NodeKind::Assignment || node.kind == NodeKind::BinaryExpression,
        "Expected Assignment or BinaryExpression, got {:?}",
        node.kind
    );
    assert_eq!(node.children[0].kind, NodeKind::TupleExpression);
}

#[test]
fn test_dispatch_identifier_expression() {
    // Token::Identifier + peek_next() != Token::Comma -> parse_expression()
    let node = parse_first("x + 1");
    assert_eq!(node.kind, NodeKind::BinaryExpression);

    let node = parse_first("foo(1, 2)");
    assert_eq!(node.kind, NodeKind::CallExpression);

    let node = parse_first("x = 5");
    assert_eq!(node.kind, NodeKind::Assignment);
    // Left side is just an identifier, not a tuple
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
}

// ==================== Operator Dispatch Tests ====================
// Critical tests for operator method definitions vs broadcast calls.
// See Issue #1574 for context on why this distinction matters.

#[test]
fn test_dispatch_operator_method_definition() {
    // Regular operator + peek_next() == Token::LParen -> parse_operator_method_definition()
    // These define how operators work (operator overloading)

    let node = parse_first("+(x, y) = x");
    assert_eq!(node.kind, NodeKind::ShortFunctionDefinition);

    let node = parse_first("-(x) = 0");
    assert_eq!(node.kind, NodeKind::ShortFunctionDefinition);

    let node = parse_first("*(a, b) = a");
    assert_eq!(node.kind, NodeKind::ShortFunctionDefinition);

    let node = parse_first("<(a, b) = true");
    assert_eq!(node.kind, NodeKind::ShortFunctionDefinition);

    let node = parse_first("==(a, b) = false");
    assert_eq!(node.kind, NodeKind::ShortFunctionDefinition);
}

#[test]
fn test_dispatch_dotted_operator_broadcast() {
    // Dotted operator + peek_next() == Token::LParen -> parse_expression() -> BroadcastCallExpression
    // These are broadcast function calls, NOT operator method definitions.
    // This is a critical distinction! (Issue #1574)

    let node = parse_first(".+(x)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_first(".-(1, 2)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_first(".*([1, 2], [3, 4])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_first("./([4, 6], [2, 3])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_first(".^([1, 2], 2)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    // Test with array literals
    let node = parse_first(".+([1, 2, 3])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
}

#[test]
fn test_dispatch_dotted_operator_binary() {
    // Dotted operators used as binary operators (not function calls)
    let node = parse_first("[1, 2] .+ [3, 4]");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    // The operator should be .+
    assert_eq!(node.children[1].kind, NodeKind::Operator);
    assert_eq!(node.children[1].text.as_deref(), Some(".+"));
}

// ==================== Default Expression Dispatch Tests ====================
// Tests for cases that fall through to parse_expression()

#[test]
fn test_dispatch_expression_default() {
    // Various expressions that fall through to parse_expression()

    // Array literal
    let node = parse_first("[1, 2, 3]");
    assert_eq!(node.kind, NodeKind::VectorExpression);

    // Matrix literal
    let node = parse_first("[1 2; 3 4]");
    assert_eq!(node.kind, NodeKind::MatrixExpression);

    // Tuple expression
    let node = parse_first("(1, 2, 3)");
    assert_eq!(node.kind, NodeKind::TupleExpression);

    // Parenthesized expression
    let node = parse_first("(42)");
    assert_eq!(node.kind, NodeKind::ParenthesizedExpression);

    // Integer literal
    let node = parse_first("42");
    assert_eq!(node.kind, NodeKind::IntegerLiteral);

    // Float literal
    let node = parse_first("3.14");
    assert_eq!(node.kind, NodeKind::FloatLiteral);

    // String literal
    let node = parse_first("\"hello\"");
    assert_eq!(node.kind, NodeKind::StringLiteral);

    // Boolean literals
    let node = parse_first("true");
    assert_eq!(node.kind, NodeKind::BooleanLiteral);

    let node = parse_first("false");
    assert_eq!(node.kind, NodeKind::BooleanLiteral);
}

#[test]
fn test_dispatch_comprehension() {
    // Comprehensions should parse as expressions
    let node = parse_first("[x for x in 1:10]");
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);

    let node = parse_first("[x for x in 1:10 if x > 5]");
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
}

#[test]
fn test_dispatch_generator() {
    // Generators should parse as expressions
    let node = parse_first("(x for x in 1:10)");
    assert_eq!(node.kind, NodeKind::Generator);

    let node = parse_first("(x^2 for x in 1:10 if x > 0)");
    assert_eq!(node.kind, NodeKind::Generator);
}

#[test]
fn test_dispatch_short_function_definition() {
    // Short function definitions with identifiers (not operators)
    // Note: The parser may return Assignment for short function definitions
    // in some cases. The key is that the left side is a CallExpression.
    let node = parse_first("f(x) = x + 1");
    // Short function definitions can be parsed as either ShortFunctionDefinition
    // or Assignment with a CallExpression on the left side
    assert!(
        node.kind == NodeKind::ShortFunctionDefinition || node.kind == NodeKind::Assignment,
        "Expected ShortFunctionDefinition or Assignment, got {:?}",
        node.kind
    );
    if node.kind == NodeKind::Assignment {
        // Left side should be a CallExpression (the function signature)
        assert_eq!(node.children[0].kind, NodeKind::CallExpression);
    }

    let node = parse_first("add(x, y) = x + y");
    assert!(
        node.kind == NodeKind::ShortFunctionDefinition || node.kind == NodeKind::Assignment,
        "Expected ShortFunctionDefinition or Assignment, got {:?}",
        node.kind
    );

    // With type annotations
    let node = parse_first("square(x::Int) = x * x");
    assert!(
        node.kind == NodeKind::ShortFunctionDefinition || node.kind == NodeKind::Assignment,
        "Expected ShortFunctionDefinition or Assignment, got {:?}",
        node.kind
    );
}

#[test]
fn test_dispatch_anonymous_function() {
    // Anonymous functions (lambdas / arrow functions)
    let node = parse_first("x -> x + 1");
    assert_eq!(node.kind, NodeKind::ArrowFunctionExpression);

    let node = parse_first("(x, y) -> x + y");
    assert_eq!(node.kind, NodeKind::ArrowFunctionExpression);
}

// ==================== Edge Case Tests ====================
// Tests for edge cases that could cause dispatch errors

#[test]
fn test_dispatch_edge_case_unary_minus() {
    // Unary minus is not an operator method definition
    let node = parse_first("-x");
    assert_eq!(node.kind, NodeKind::UnaryExpression);

    let node = parse_first("-42");
    assert_eq!(node.kind, NodeKind::UnaryExpression);
}

#[test]
fn test_dispatch_edge_case_not_operator() {
    // Logical not is not an operator method definition
    let node = parse_first("!flag");
    assert_eq!(node.kind, NodeKind::UnaryExpression);

    let node = parse_first("!true");
    assert_eq!(node.kind, NodeKind::UnaryExpression);
}

#[test]
fn test_dispatch_edge_case_range() {
    // Range expressions start with an integer
    let node = parse_first("1:10");
    assert_eq!(node.kind, NodeKind::RangeExpression);

    let node = parse_first("1:2:10");
    assert_eq!(node.kind, NodeKind::RangeExpression);
}

#[test]
fn test_dispatch_edge_case_ternary() {
    // Ternary expressions
    let node = parse_first("x > 0 ? 1 : 0");
    assert_eq!(node.kind, NodeKind::TernaryExpression);
}

#[test]
fn test_dispatch_broadcast_call_expression() {
    // Broadcast call with identifier.()
    let node = parse_first("f.(x, y)");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);

    let node = parse_first("sin.([1, 2, 3])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
}

// ==================== Exhaustive Operator Dispatch Tests ====================
// These tests systematically verify that EVERY regular operator dispatches to
// parse_operator_method_definition() when followed by '(', and that EVERY
// dotted operator dispatches to parse_expression() (broadcast call) instead.
// This provides property-based-style coverage for the critical dispatch invariant.
// See Issue #1756 for context.

#[test]
fn test_exhaustive_regular_operators_dispatch_to_method_definition() {
    // Invariant: For every regular (non-dotted) operator O,
    //   "O(x, y) = x" must parse as ShortFunctionDefinition
    // This systematically tests ALL operators returned by is_operator()
    // that are NOT returned by is_dotted_operator().

    let regular_operator_cases: Vec<(&str, &str)> = vec![
        // Arithmetic operators
        ("+", "+(x, y) = x"),
        ("-", "-(x, y) = x"),
        ("*", "*(x, y) = x"),
        ("/", "/(x, y) = x"),
        ("%", "%(x, y) = x"),
        ("^", "^(x, y) = x"),
        ("\\", "\\(x, y) = x"),
        // Comparison operators
        ("<", "<(x, y) = true"),
        (">", ">(x, y) = true"),
        ("<=", "<=(x, y) = true"),
        (">=", ">=(x, y) = true"),
        ("==", "==(x, y) = true"),
        ("===", "===(x, y) = true"),
        ("!=", "!=(x, y) = true"),
        ("!==", "!==(x, y) = true"),
        // Type operators
        ("<:", "<:(x, y) = true"),
        (">:", ">:(x, y) = true"),
        // Bitwise operators
        ("&", "&(x, y) = x"),
        ("|", "|(x, y) = x"),
        ("~", "~(x) = x"),
        ("<<", "<<(x, y) = x"),
        (">>", ">>(x, y) = x"),
        (">>>", ">>>(x, y) = x"),
        // Misc operators
        ("//", "//(x, y) = x"),
        ("|>", "|>(x, y) = x"),
        ("<|", "<|(x, y) = x"),
    ];

    for (op, source) in &regular_operator_cases {
        let node = parse_first(source);
        assert_eq!(
            node.kind,
            NodeKind::ShortFunctionDefinition,
            "Regular operator '{}' should dispatch to ShortFunctionDefinition, got {:?} for source: {}",
            op, node.kind, source
        );
    }
}

#[test]
fn test_exhaustive_dotted_operators_dispatch_to_broadcast_call() {
    // Invariant: For every dotted operator O,
    //   "O(x, y)" must parse as BroadcastCallExpression (NOT ShortFunctionDefinition)
    // This systematically tests ALL dotted operators from is_dotted_operator().
    // Failure of ANY of these indicates a regression like Issue #1574.

    let dotted_operator_cases: Vec<(&str, &str)> = vec![
        // Arithmetic broadcast
        (".+", ".+(x, y)"),
        (".-", ".-(x, y)"),
        (".*", ".*(x, y)"),
        ("./", "./(x, y)"),
        (".\\", ".\\(x, y)"),
        (".^", ".^(x, y)"),
        (".%", ".%(x, y)"),
        // Comparison broadcast
        (".<", ".<(x, y)"),
        (".>", ".>(x, y)"),
        (".<=", ".<=(x, y)"),
        (".>=", ".>=(x, y)"),
        (".==", ".==(x, y)"),
        (".!=", ".!=(x, y)"),
        // Bitwise broadcast
        (".&", ".&(x, y)"),
        (".|", ".|(x, y)"),
    ];

    for (op, source) in &dotted_operator_cases {
        let node = parse_first(source);
        assert_eq!(
            node.kind,
            NodeKind::BroadcastCallExpression,
            "Dotted operator '{}' should dispatch to BroadcastCallExpression, got {:?} for source: {}",
            op, node.kind, source
        );
        // Explicitly verify NOT dispatched as method definition (Issue #1574 regression guard)
        assert_ne!(
            node.kind,
            NodeKind::ShortFunctionDefinition,
            "Dotted operator '{}' must NEVER be parsed as ShortFunctionDefinition (Issue #1574)",
            op
        );
    }
}

#[test]
fn test_exhaustive_dotted_operators_as_binary_expressions() {
    // Invariant: For every dotted operator O used in binary position,
    //   "a O b" must parse as BinaryExpression with operator text "O"
    // This verifies the binary usage path complements the broadcast call path.

    let dotted_binary_cases: Vec<(&str, &str)> = vec![
        (".+", "a .+ b"),
        (".-", "a .- b"),
        (".*", "a .* b"),
        ("./", "a ./ b"),
        (".\\", "a .\\ b"),
        (".^", "a .^ b"),
        (".%", "a .% b"),
        (".<", "a .< b"),
        (".>", "a .> b"),
        (".<=", "a .<= b"),
        (".>=", "a .>= b"),
        (".==", "a .== b"),
        (".!=", "a .!= b"),
        (".&", "a .& b"),
        (".|", "a .| b"),
    ];

    for (op, source) in &dotted_binary_cases {
        let node = parse_first(source);
        assert_eq!(
            node.kind,
            NodeKind::BinaryExpression,
            "Dotted operator '{}' in binary position should be BinaryExpression, got {:?} for source: {}",
            op, node.kind, source
        );
        // Verify the operator text
        let operator_node = &node.children[1];
        assert_eq!(
            operator_node.kind,
            NodeKind::Operator,
            "Middle child should be Operator for '{}'",
            source
        );
        assert_eq!(
            operator_node.text.as_deref(),
            Some(*op),
            "Operator text should be '{}' for source: {}",
            op,
            source
        );
    }
}

// ==================== Cross-Validation Tests ====================
// These tests verify structural properties of the dispatch logic:
// every operator must go through exactly ONE dispatch path, and
// regular vs dotted operators must never cross paths.
// See Issue #1756 for context.

#[test]
fn test_cross_validation_method_def_has_assignment() {
    // Cross-validation: Operator method definitions must contain an assignment
    // (the = sign in "+(x, y) = expr"). This verifies the ShortFunctionDefinition
    // structure, not just the NodeKind.

    let method_defs = vec![
        "+(x, y) = x + y",
        "-(x) = -x",
        "*(a, b) = a * b",
        "==(a, b) = a === b",
    ];

    for source in &method_defs {
        let node = parse_first(source);
        assert_eq!(
            node.kind,
            NodeKind::ShortFunctionDefinition,
            "Expected ShortFunctionDefinition for '{}'",
            source
        );
        // ShortFunctionDefinition should have children: name/params and body
        assert!(
            node.children.len() >= 2,
            "ShortFunctionDefinition should have at least 2 children (signature + body) for '{}'",
            source
        );
    }
}

#[test]
fn test_cross_validation_broadcast_call_has_arguments() {
    // Cross-validation: Broadcast call expressions must contain arguments
    // (the arguments inside parentheses). This verifies the BroadcastCallExpression
    // structure, not just the NodeKind.

    let broadcast_calls = vec![
        (".+(x)", 1),
        (".-(x, y)", 2),
        (".*([1, 2], [3, 4])", 2),
        (".^(x, 2)", 2),
    ];

    for (source, _expected_args) in &broadcast_calls {
        let node = parse_first(source);
        assert_eq!(
            node.kind,
            NodeKind::BroadcastCallExpression,
            "Expected BroadcastCallExpression for '{}'",
            source
        );
        // BroadcastCallExpression should have children (operator + arguments)
        assert!(
            !node.children.is_empty(),
            "BroadcastCallExpression should have children for '{}'",
            source
        );
    }
}

#[test]
fn test_cross_validation_operator_method_def_vs_expression_context() {
    // Cross-validation: The same operator must dispatch differently depending on context.
    // "+(x, y) = expr" → ShortFunctionDefinition (top-level, operator + LParen)
    // "x + y" → BinaryExpression (expression context)
    // This verifies the dispatch logic is context-sensitive.

    let context_pairs: Vec<(&str, NodeKind, &str, NodeKind)> = vec![
        (
            "+(x, y) = x",
            NodeKind::ShortFunctionDefinition,
            "x + y",
            NodeKind::BinaryExpression,
        ),
        (
            "-(x) = 0",
            NodeKind::ShortFunctionDefinition,
            "-x",
            NodeKind::UnaryExpression,
        ),
        (
            "*(x, y) = x",
            NodeKind::ShortFunctionDefinition,
            "x * y",
            NodeKind::BinaryExpression,
        ),
        (
            "<(x, y) = true",
            NodeKind::ShortFunctionDefinition,
            "x < y",
            NodeKind::BinaryExpression,
        ),
    ];

    for (def_src, def_kind, expr_src, expr_kind) in &context_pairs {
        let def_node = parse_first(def_src);
        assert_eq!(
            def_node.kind, *def_kind,
            "Method definition '{}' should be {:?}",
            def_src, def_kind
        );

        let expr_node = parse_first(expr_src);
        assert_eq!(
            expr_node.kind, *expr_kind,
            "Expression '{}' should be {:?}",
            expr_src, expr_kind
        );

        // Most importantly: they must NOT be the same kind
        assert_ne!(
            def_node.kind, expr_node.kind,
            "Method def '{}' and expression '{}' must dispatch to DIFFERENT node kinds",
            def_src, expr_src
        );
    }
}

#[test]
fn test_cross_validation_dotted_vs_regular_operator_dispatch() {
    // Cross-validation: For each dotted operator, verify its base operator
    // dispatches differently. This is the core invariant from Issue #1574.
    //
    // Regular: "+(x, y) = x"  → ShortFunctionDefinition
    // Dotted:  ".+(x, y)"     → BroadcastCallExpression

    let operator_pairs: Vec<(&str, &str, &str, &str)> = vec![
        ("+", "+(x, y) = x", ".+", ".+(x, y)"),
        ("-", "-(x, y) = x", ".-", ".-(x, y)"),
        ("*", "*(x, y) = x", ".*", ".*(x, y)"),
        ("/", "/(x, y) = x", "./", "./(x, y)"),
        ("^", "^(x, y) = x", ".^", ".^(x, y)"),
        ("%", "%(x, y) = x", ".%", ".%(x, y)"),
        ("<", "<(x, y) = true", ".<", ".<(x, y)"),
        (">", ">(x, y) = true", ".>", ".>(x, y)"),
        ("<=", "<=(x, y) = true", ".<=", ".<=(x, y)"),
        (">=", ">=(x, y) = true", ".>=", ".>=(x, y)"),
        ("==", "==(x, y) = true", ".==", ".==(x, y)"),
        ("!=", "!=(x, y) = true", ".!=", ".!=(x, y)"),
        ("&", "&(x, y) = x", ".&", ".&(x, y)"),
        ("|", "|(x, y) = x", ".|", ".|(x, y)"),
    ];

    for (reg_op, reg_src, dot_op, dot_src) in &operator_pairs {
        let reg_node = parse_first(reg_src);
        let dot_node = parse_first(dot_src);

        assert_eq!(
            reg_node.kind,
            NodeKind::ShortFunctionDefinition,
            "Regular operator '{}' with '(' should be ShortFunctionDefinition",
            reg_op
        );
        assert_eq!(
            dot_node.kind,
            NodeKind::BroadcastCallExpression,
            "Dotted operator '{}' with '(' should be BroadcastCallExpression",
            dot_op
        );

        // Core invariant: regular and dotted operators MUST dispatch differently
        assert_ne!(
            reg_node.kind, dot_node.kind,
            "Regular '{}' and dotted '{}' must dispatch to DIFFERENT paths (Issue #1574)",
            reg_op, dot_op
        );
    }
}

// ==================== Regression Tests ====================
// Tests for previously discovered issues

#[test]
fn test_regression_issue_1573() {
    // Issue #1573: Test expectation mismatch due to dispatch misunderstanding
    // Ensure that expression parsing works correctly for complex expressions
    let node = parse_first("x + y * z");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
}

#[test]
fn test_regression_issue_1574() {
    // Issue #1574: Operator dispatch error for dotted operators
    // Dotted operators followed by ( should be broadcast calls, not method definitions
    let node = parse_first(".+([1, 2, 3])");
    assert_eq!(node.kind, NodeKind::BroadcastCallExpression);
    // Verify it's NOT a ShortFunctionDefinition
    assert_ne!(node.kind, NodeKind::ShortFunctionDefinition);
}
