//! Tests for CstNode tree-sitter compatible API
//!
//! These tests verify that CstNode provides the same API as tree-sitter's Node,
//! enabling migration from tree-sitter-julia to subset_julia_vm_parser.

use subset_julia_vm_parser::{parse, NodeKind};

/// Helper to parse and get root node
fn parse_source(source: &str) -> subset_julia_vm_parser::CstNode {
    parse(source).expect("parse should succeed")
}

// ============================================================================
// CstNode API Tests
// ============================================================================

#[test]
fn test_named_children_vec() {
    let root = parse_source("x + y");
    let source_file = &root;

    // Get named children as Vec
    let children = source_file.named_children_vec();
    assert!(!children.is_empty());
}

#[test]
fn test_children_vec() {
    let root = parse_source("1 + 2");

    // Get all children as Vec
    let children = root.children_vec();
    assert!(!children.is_empty());
}

#[test]
fn test_find_child() {
    let root = parse_source("function foo() end");

    // Find function definition
    let func = root.find_child(NodeKind::FunctionDefinition);
    assert!(func.is_some());
    assert_eq!(func.unwrap().kind, NodeKind::FunctionDefinition);
}

#[test]
fn test_find_children() {
    let root = parse_source("[1, 2, 3]");

    // Find vector expression
    let vec = root.find_child(NodeKind::VectorExpression).expect("vector");

    // Find all integer literals in vector
    let literals: Vec<_> = vec.find_children(NodeKind::IntegerLiteral).collect();
    assert_eq!(literals.len(), 3);
}

#[test]
fn test_child_by_field() {
    let root = parse_source("foo(x, y)");
    let call = root.find_child(NodeKind::CallExpression).expect("call");

    // Fields are set during parsing, check if we have any
    // (depends on parser implementation)
    let _ = call.child_by_field("name");
    let _ = call.child_by_field("arguments");
}

#[test]
fn test_text_str() {
    let root = parse_source("hello");
    let children = root.named_children_vec();

    // Find identifier and check text
    for child in children {
        if child.kind == NodeKind::Identifier {
            // text_str returns Option<&str>
            if let Some(text) = child.text_str() {
                assert_eq!(text, "hello");
            }
        }
    }
}

#[test]
fn test_text_from_source() {
    let source = "hello + world";
    let root = parse_source(source);

    // text_from_source extracts text using span
    let text = root.text_from_source(source);
    assert_eq!(text, source);
}

// ============================================================================
// NodeKind Compatibility Tests
// ============================================================================

#[test]
fn test_nodekind_from_str_roundtrip() {
    let kinds = vec![
        NodeKind::SourceFile,
        NodeKind::FunctionDefinition,
        NodeKind::Assignment,
        NodeKind::BinaryExpression,
        NodeKind::CallExpression,
        NodeKind::Identifier,
        NodeKind::IntegerLiteral,
        NodeKind::FloatLiteral,
        NodeKind::StringLiteral,
        NodeKind::VectorExpression,
        NodeKind::MatrixExpression,
        NodeKind::RangeExpression,
        NodeKind::ComprehensionExpression,
        NodeKind::IfStatement,
        NodeKind::ForStatement,
        NodeKind::WhileStatement,
        NodeKind::StructDefinition,
        NodeKind::AbstractDefinition,
    ];

    for kind in kinds {
        let str_repr = kind.as_str();
        let restored: NodeKind = str_repr.parse().unwrap();
        assert_eq!(restored, kind, "roundtrip failed for {:?}", kind);
    }
}

#[test]
fn test_nodekind_is_named() {
    // Most nodes are named
    assert!(NodeKind::Identifier.is_named());
    assert!(NodeKind::FunctionDefinition.is_named());
    assert!(NodeKind::BinaryExpression.is_named());

    // Unknown is not named
    assert!(!NodeKind::Unknown.is_named());
}

#[test]
fn test_nodekind_is_expression() {
    assert!(NodeKind::BinaryExpression.is_expression());
    assert!(NodeKind::CallExpression.is_expression());
    assert!(NodeKind::Identifier.is_expression());
    assert!(NodeKind::IntegerLiteral.is_expression());

    assert!(!NodeKind::IfStatement.is_expression());
    assert!(!NodeKind::FunctionDefinition.is_expression());
}

#[test]
fn test_nodekind_is_statement() {
    assert!(NodeKind::IfStatement.is_statement());
    assert!(NodeKind::ForStatement.is_statement());
    assert!(NodeKind::WhileStatement.is_statement());
    assert!(NodeKind::ReturnStatement.is_statement());

    assert!(!NodeKind::BinaryExpression.is_statement());
    assert!(!NodeKind::Identifier.is_statement());
}

#[test]
fn test_nodekind_is_literal() {
    assert!(NodeKind::IntegerLiteral.is_literal());
    assert!(NodeKind::FloatLiteral.is_literal());
    assert!(NodeKind::StringLiteral.is_literal());
    assert!(NodeKind::BooleanLiteral.is_literal());

    assert!(!NodeKind::Identifier.is_literal());
    assert!(!NodeKind::BinaryExpression.is_literal());
}

// ============================================================================
// Parse Result Structure Tests
// ============================================================================

#[test]
fn test_parse_simple_expression() {
    let root = parse_source("1 + 2");

    // Should have SourceFile at root
    assert_eq!(root.kind, NodeKind::SourceFile);

    // Find binary expression
    let binary = root.find_child(NodeKind::BinaryExpression);
    assert!(binary.is_some());
}

#[test]
fn test_parse_function_definition() {
    let root = parse_source("function add(x, y)\n    x + y\nend");

    assert_eq!(root.kind, NodeKind::SourceFile);

    let func = root.find_child(NodeKind::FunctionDefinition);
    assert!(func.is_some());

    let func = func.unwrap();
    // Function should have children (signature elements are inline)
    assert!(func.child_count() > 0);
}

#[test]
fn test_parse_struct_definition() {
    let root = parse_source("struct Point\n    x::Float64\n    y::Float64\nend");

    let struct_def = root.find_child(NodeKind::StructDefinition);
    assert!(struct_def.is_some());
}

#[test]
fn test_parse_for_loop() {
    let root = parse_source("for i in 1:10\n    println(i)\nend");

    let for_stmt = root.find_child(NodeKind::ForStatement);
    assert!(for_stmt.is_some());

    let for_stmt = for_stmt.unwrap();
    // For statement should have for binding
    let binding = for_stmt.find_child(NodeKind::ForBinding);
    assert!(binding.is_some());
}

#[test]
fn test_parse_comprehension() {
    let root = parse_source("[x^2 for x in 1:10]");

    let comp = root.find_child(NodeKind::ComprehensionExpression);
    assert!(comp.is_some());

    let comp = comp.unwrap();
    // Comprehension should have for clause
    let for_clause = comp.find_child(NodeKind::ForClause);
    assert!(for_clause.is_some());
}

#[test]
fn test_parse_matrix() {
    let root = parse_source("[1 2; 3 4]");

    let matrix = root.find_child(NodeKind::MatrixExpression);
    assert!(matrix.is_some());

    let matrix = matrix.unwrap();
    // Matrix should have rows
    let rows: Vec<_> = matrix.find_children(NodeKind::MatrixRow).collect();
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_span_information() {
    let source = "x + y";
    let root = parse_source(source);

    // Root span should cover entire source
    assert_eq!(root.span.start, 0);
    assert_eq!(root.span.end, source.len());

    // Line/column info should be present
    assert!(root.span.start_line >= 1);
    assert!(root.span.start_column >= 1);
}

#[test]
fn test_error_handling() {
    // Parsing invalid syntax returns an error
    let result = parse("function"); // incomplete

    // Should return an error
    assert!(result.is_err());
}

#[test]
fn test_has_error_method() {
    // Parse valid code - should have no errors
    let root = parse_source("x + y");
    assert!(!root.has_error());

    // Parse code with nested structure
    let root2 = parse_source("function f() 1 + 2 end");
    assert!(!root2.has_error());
}

// ============================================================================
// Walker Tests
// ============================================================================

#[test]
fn test_walker_traversal() {
    let root = parse_source("x + y");

    // Walk should visit all nodes
    let mut count = 0;
    for _node in root.walk() {
        count += 1;
    }
    assert!(count > 1);
}

#[test]
fn test_walker_order() {
    let root = parse_source("1 + 2");

    // First node should be root
    let nodes: Vec<_> = root.walk().collect();
    assert_eq!(nodes[0].kind, NodeKind::SourceFile);
}
