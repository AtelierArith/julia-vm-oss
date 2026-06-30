//! Tests migrated from tree-sitter-julia/test/corpus/statements.txt

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

/// Assert `source` parses and some leaf in the resulting tree has the given text.
/// Used to confirm a contextual keyword's payload (e.g. an import alias) is
/// actually captured in the CST, not silently dropped.
fn assert_tree_contains_text(source: &str, text: &str) {
    let cst = parse(source).unwrap_or_else(|_| panic!("Failed to parse: {}", source));
    fn has_text(node: &subset_julia_vm_parser::CstNode, text: &str) -> bool {
        node.text.as_deref() == Some(text) || node.children.iter().any(|c| has_text(c, text))
    }
    assert!(
        has_text(&cst, text),
        "Expected a leaf with text {:?} in parse of: {}",
        text,
        source
    );
}

// =============================================================================
// Compound Statement (begin...end)
// =============================================================================

#[test]
fn test_begin_block_empty() {
    assert_root_child_kind("begin\nend", NodeKind::BeginBlock);
}

#[test]
fn test_begin_block_single() {
    assert_root_child_kind("begin\n  x = 1\nend", NodeKind::BeginBlock);
}

#[test]
fn test_begin_block_multiple() {
    assert_root_child_kind("begin\n  x = 1\n  y = 2\nend", NodeKind::BeginBlock);
}

#[test]
fn test_begin_block_inline() {
    assert_root_child_kind("begin x = 1; y = 2 end", NodeKind::BeginBlock);
}

// =============================================================================
// Quote Statement
// =============================================================================

// Quote is parsed as QuoteExpression
#[test]
fn test_quote_block() {
    assert_root_child_kind("quote\n  x + 1\nend", NodeKind::QuoteExpression);
}

// Quote with interpolation
#[test]
fn test_quote_with_interpolation() {
    assert_root_child_kind("quote\n  $x + 1\nend", NodeKind::QuoteExpression);
}

// =============================================================================
// Let Statement
// =============================================================================

#[test]
fn test_let_single() {
    assert_root_child_kind("let x = 1\n  x + 1\nend", NodeKind::LetExpression);
}

#[test]
fn test_let_multiple() {
    assert_root_child_kind("let x = 1, y = 2\n  x + y\nend", NodeKind::LetExpression);
}

#[test]
fn test_let_typed() {
    assert_root_child_kind("let x::Int = 1\n  x\nend", NodeKind::LetExpression);
}

#[test]
fn test_let_empty_bindings() {
    assert_root_child_kind("let\n  1 + 1\nend", NodeKind::LetExpression);
}

// =============================================================================
// If Statement
// =============================================================================

#[test]
fn test_if_simple() {
    assert_root_child_kind("if x\n  1\nend", NodeKind::IfStatement);
}

#[test]
fn test_if_else() {
    assert_root_child_kind("if x\n  1\nelse\n  2\nend", NodeKind::IfStatement);
}

#[test]
fn test_if_elseif() {
    assert_root_child_kind("if x\n  1\nelseif y\n  2\nend", NodeKind::IfStatement);
}

#[test]
fn test_if_elseif_else() {
    assert_root_child_kind(
        "if x\n  1\nelseif y\n  2\nelse\n  3\nend",
        NodeKind::IfStatement,
    );
}

#[test]
fn test_if_multiple_elseif() {
    assert_root_child_kind(
        "if a\n  1\nelseif b\n  2\nelseif c\n  3\nend",
        NodeKind::IfStatement,
    );
}

#[test]
fn test_if_inline() {
    assert_root_child_kind("if x 1 else 2 end", NodeKind::IfStatement);
}

// =============================================================================
// Try Statement
// =============================================================================

#[test]
fn test_try_catch() {
    assert_root_child_kind("try\n  f()\ncatch\n  g()\nend", NodeKind::TryStatement);
}

#[test]
fn test_try_catch_named() {
    assert_root_child_kind(
        "try\n  f()\ncatch e\n  println(e)\nend",
        NodeKind::TryStatement,
    );
}

#[test]
fn test_try_finally() {
    assert_root_child_kind(
        "try\n  f()\nfinally\n  cleanup()\nend",
        NodeKind::TryStatement,
    );
}

#[test]
fn test_try_catch_finally() {
    assert_root_child_kind(
        "try\n  f()\ncatch\n  g()\nfinally\n  cleanup()\nend",
        NodeKind::TryStatement,
    );
}

// try...else (Julia 1.8+)
#[test]
fn test_try_else() {
    assert_parses("try\n  f()\ncatch\n  g()\nelse\n  h()\nend");
}

// =============================================================================
// For Statement
// =============================================================================

#[test]
fn test_for_simple() {
    assert_root_child_kind("for i in 1:10\n  println(i)\nend", NodeKind::ForStatement);
}

#[test]
fn test_for_equals() {
    assert_root_child_kind("for i = 1:10\n  println(i)\nend", NodeKind::ForStatement);
}

// Multiple for bindings
#[test]
fn test_for_multiple() {
    assert_root_child_kind(
        "for i in 1:10, j in 1:10\n  println(i, j)\nend",
        NodeKind::ForStatement,
    );
}

#[test]
fn test_for_nested() {
    assert_parses("for i in 1:10\n  for j in 1:10\n    println(i, j)\n  end\nend");
}

// for outer
#[test]
fn test_for_outer() {
    assert_root_child_kind(
        "for outer i in 1:10\n  println(i)\nend",
        NodeKind::ForStatement,
    );
    assert_root_child_kind(
        "for outer in 1:10\n  println(outer)\nend",
        NodeKind::ForStatement,
    );
}

// =============================================================================
// While Statement
// =============================================================================

#[test]
fn test_while_simple() {
    assert_root_child_kind("while x > 0\n  x -= 1\nend", NodeKind::WhileStatement);
}

#[test]
fn test_while_true() {
    assert_root_child_kind("while true\n  break\nend", NodeKind::WhileStatement);
}

// =============================================================================
// Break and Continue
// =============================================================================

#[test]
fn test_break() {
    assert_parses("for i in 1:10\n  if i > 5\n    break\n  end\nend");
}

#[test]
fn test_continue() {
    assert_parses("for i in 1:10\n  if i % 2 == 0\n    continue\n  end\n  println(i)\nend");
}

// =============================================================================
// Return Statement
// =============================================================================

#[test]
fn test_return_nothing() {
    assert_root_child_kind("return", NodeKind::ReturnStatement);
}

#[test]
fn test_return_value() {
    assert_root_child_kind("return x", NodeKind::ReturnStatement);
}

#[test]
fn test_return_expression() {
    assert_root_child_kind("return x + 1", NodeKind::ReturnStatement);
}

// =============================================================================
// Export Statement
// =============================================================================

#[test]
fn test_export_single() {
    assert_root_child_kind("export foo", NodeKind::ExportStatement);
}

#[test]
fn test_export_multiple() {
    assert_root_child_kind("export foo, bar", NodeKind::ExportStatement);
}

#[test]
fn test_export_operator() {
    assert_root_child_kind("export +, -, *", NodeKind::ExportStatement);
}

// =============================================================================
// Public Statement (Julia 1.11+)
// =============================================================================

#[test]
fn test_public_single() {
    assert_root_child_kind("public foo", NodeKind::PublicStatement);
}

#[test]
fn test_public_multiple() {
    assert_root_child_kind("public foo, bar", NodeKind::PublicStatement);
}

// =============================================================================
// Import Statement
// =============================================================================

#[test]
fn test_import_module() {
    assert_root_child_kind("import Base", NodeKind::ImportStatement);
}

#[test]
fn test_import_submodule() {
    assert_root_child_kind("import Base.Math", NodeKind::ImportStatement);
}

#[test]
fn test_import_specific() {
    assert_root_child_kind("import Base: sin, cos", NodeKind::ImportStatement);
    assert_root_child_kind("import Base: *, ==, +", NodeKind::ImportStatement);
    assert_parses("import Base: *,\n ==,\n +");
}

// import as
#[test]
fn test_import_as() {
    assert_root_child_kind("import Base as B", NodeKind::ImportStatement);
    assert_parses("import Base: sin as s, cos as c");
    // `as` is lexed as a plain identifier (Issue #8108); the alias payload must
    // still be captured in the CST in import/using position.
    assert_tree_contains_text("import Base as B", "B");
    assert_tree_contains_text("import Base: sin as s, cos as c", "s");
    assert_tree_contains_text("using LinearAlgebra: norm as n", "n");
}

// =============================================================================
// Contextual keywords (`type`, `as`) as ordinary identifiers (Issue #8108)
//
// `type` and `as` are contextual keywords: `type` is significant only after
// `abstract`/`primitive`, and `as` only in import/using aliasing. Everywhere
// else they are plain identifiers, exactly as upstream Julia parses them.
// =============================================================================

#[test]
fn test_type_as_function_name() {
    assert_root_child_kind("function type()\n  1\nend", NodeKind::FunctionDefinition);
    assert_root_child_kind("function as()\n  1\nend", NodeKind::FunctionDefinition);
    // Short form lowers to an Assignment whose LHS is a call (see definitions).
    assert_root_child_kind("type() = 7", NodeKind::Assignment);
    assert_root_child_kind("as() = 7", NodeKind::Assignment);
}

#[test]
fn test_type_as_variable_name() {
    assert_root_child_kind("type = 5", NodeKind::Assignment);
    assert_root_child_kind("as = 7", NodeKind::Assignment);
    assert_parses("println(type)");
    assert_parses("println(as)");
}

#[test]
fn test_type_field_name() {
    assert_root_child_kind("struct S\n  type::Int\nend", NodeKind::StructDefinition);
}

#[test]
fn test_type_keyword_still_contextual() {
    // `type` is still the keyword half of `abstract`/`primitive type`.
    assert_root_child_kind("abstract type Foo end", NodeKind::AbstractDefinition);
    assert_root_child_kind("primitive type Bar 8 end", NodeKind::PrimitiveDefinition);
}

// Relative import
#[test]
fn test_import_relative() {
    assert_root_child_kind("import .Foo", NodeKind::ImportStatement);
    assert_root_child_kind("import ..Foo", NodeKind::ImportStatement);
    assert_root_child_kind("import ...Foo", NodeKind::ImportStatement);
    assert_parses("import .Foo: bar");
    assert_parses("import ..Foo: bar, baz");
}

// =============================================================================
// Using Statement
// =============================================================================

#[test]
fn test_using_module() {
    assert_root_child_kind("using LinearAlgebra", NodeKind::UsingStatement);
}

#[test]
fn test_using_multiple() {
    assert_root_child_kind("using LinearAlgebra, Statistics", NodeKind::UsingStatement);
}

#[test]
fn test_using_specific() {
    assert_root_child_kind("using LinearAlgebra: norm, dot", NodeKind::UsingStatement);
}

// =============================================================================
// Const Declaration
// =============================================================================

#[test]
fn test_const_simple() {
    assert_root_child_kind("const x = 1", NodeKind::ConstDeclaration);
}

#[test]
fn test_const_typed() {
    assert_root_child_kind("const x::Int = 1", NodeKind::ConstDeclaration);
}

// Multiple const declaration with destructuring
#[test]
fn test_const_multiple() {
    assert_root_child_kind("const x, y = 1, 2", NodeKind::ConstDeclaration);
}

// Const with parenthesized destructuring
#[test]
fn test_const_destructure() {
    assert_parses("const (a, b) = (1, 2)");
    assert_parses("const (x, y, z) = foo()");
}

// =============================================================================
// Global Declaration
// =============================================================================

#[test]
fn test_global_simple() {
    assert_root_child_kind("global x", NodeKind::GlobalDeclaration);
}

// Global declaration with initialization
#[test]
fn test_global_with_value() {
    assert_root_child_kind("global x = 1", NodeKind::GlobalDeclaration);
}

#[test]
fn test_global_multiple() {
    assert_root_child_kind("global x, y", NodeKind::GlobalDeclaration);
}

// =============================================================================
// Local Declaration
// =============================================================================

#[test]
fn test_local_simple() {
    assert_root_child_kind("local x", NodeKind::LocalDeclaration);
}

// Local declaration with initialization
#[test]
fn test_local_with_value() {
    assert_root_child_kind("local x = 1", NodeKind::LocalDeclaration);
}

#[test]
fn test_local_multiple() {
    assert_root_child_kind("local x, y", NodeKind::LocalDeclaration);
}

// Local declaration with type annotation
#[test]
fn test_local_typed() {
    assert_root_child_kind("local x::Int", NodeKind::LocalDeclaration);
}

// Local declaration with type annotation and value
#[test]
fn test_local_typed_with_value() {
    assert_root_child_kind("local x::Int = 1", NodeKind::LocalDeclaration);
    assert_parses("local x::Float64 = 3.14");
}

#[test]
fn test_return_in_short_circuit() {
    // Test that return/break/continue can be used in short-circuit expressions
    // e.g., x > 0 && return 42
    let source = "true && return nothing";
    let (cst, errors) = subset_julia_vm_parser::parser::parse(source);

    assert!(
        errors.is_empty(),
        "Should have no parse errors: {:?}",
        errors
    );

    // Verify structure: SourceFile > BinaryExpression > (BooleanLiteral, Operator, ReturnStatement)
    assert_eq!(cst.kind, subset_julia_vm_parser::NodeKind::SourceFile);
    assert_eq!(cst.children.len(), 1);
    let binary = &cst.children[0];
    assert_eq!(
        binary.kind,
        subset_julia_vm_parser::NodeKind::BinaryExpression
    );
    // Find return statement child
    let return_stmt = binary
        .children
        .iter()
        .find(|c| c.kind == subset_julia_vm_parser::NodeKind::ReturnStatement);
    assert!(
        return_stmt.is_some(),
        "Should find ReturnStatement in BinaryExpression"
    );
}
