//! Tests migrated from tree-sitter-julia/test/corpus/statements.txt

use subset_julia_vm_parser::{parse, NodeKind, ParseError};

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
    fn has_text(node: &subset_julia_vm_parser::CstNode, text: &str, source: &str) -> bool {
        (node.children.is_empty() && node.text_from_source(source) == text)
            || node.children.iter().any(|c| has_text(c, text, source))
    }
    assert!(
        has_text(&cst, text, source),
        "Expected a leaf with text {:?} in parse of: {}",
        text,
        source
    );
}

#[test]
fn test_additional_statement_gaps_issue_8759() {
    assert_root_child_kind(
        "for (; match) in eachmatch(r\"julia>\", s)\nend",
        NodeKind::ForStatement,
    );
    assert_root_child_kind(
        "for ((r1, d1, f1), (r2, d2, f2)) in zip(walkdir(), walkdir(pwd()))\nend",
        NodeKind::ForStatement,
    );
    assert_root_child_kind(
        "for (i::Int, c::AbstractChar) in pairs(s)\nend",
        NodeKind::ForStatement,
    );
    assert_root_child_kind("for h = head, t = tail\nend", NodeKind::ForStatement);
    assert_root_child_kind(
        "for i = n:size(grid, 1) - n + 1,\n    j = n:size(grid, 2) - n + 1,\n    di = -1:1, dj = -1:1\nend",
        NodeKind::ForStatement,
    );
    assert_root_child_kind(
        "ys, = afoldl(((), init), xs...)",
        NodeKind::BinaryExpression,
    );
    assert_root_child_kind("import Base.<", NodeKind::ImportStatement);
    assert_parses(
        "@label outer for i in 1:5\n    for j in 1:5\n        if i == 3 && j == 2\n            break outer\n        end\n    end\nend",
    );
    assert_parses(
        "@label outer for i in 1:5\n    for j in 1:5\n        if j > 2\n            continue outer\n        end\n    end\nend",
    );
    assert_parses("@label search begin\n    break search i => v\nend");
    assert_parses("for i in 1:10\n    i > 5 ? break outer : i\nend");
}

#[test]
fn test_interpolated_for_binding_issue_8759() {
    assert_root_child_kind("for $itervar = rng\nend", NodeKind::ForStatement);
    assert_root_child_kind("for $itervar = $rng\nend", NodeKind::ForStatement);
    assert_parses("quote\n    for $itervar = $rng\n    end\nend");
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
fn test_let_multiline_bindings_issue_8753() {
    assert_root_child_kind(
        "let dest = x,\n    src = y\n    dest + src\nend",
        NodeKind::LetExpression,
    );
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

#[test]
fn test_for_newline_before_first_binding_issue_8759() {
    assert_root_child_kind("for\n x in xs\n body\nend", NodeKind::ForStatement);
    assert_parses("@testset \"x\" for\n f in fs,\n a in as\n body\nend");
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

#[test]
fn test_splatted_for_tuple_binding_issue_8756() {
    assert_parses("for (i, Is...) in zip(eachindex(dest), map(eachindex, As)...)\n  f(i, Is)\nend");
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

#[test]
fn test_const_global_modifier_combination_issue_8756() {
    assert_root_child_kind(
        "const global maxallowedprobe = 16",
        NodeKind::ConstDeclaration,
    );
    assert_root_child_kind(
        "const global ENDIAN_BOM = 0x01020304",
        NodeKind::ConstDeclaration,
    );
    assert_parses("const global letT_57470{T} = Int64");
}

#[test]
fn test_scoped_const_modifier_order_and_keyword_rejection_issue_10938() {
    for (source, scope_kind, scope_start) in [
        ("global const c = 1", NodeKind::GlobalDeclaration, 0),
        ("const global c = 1", NodeKind::GlobalDeclaration, 6),
        ("local const c = 1", NodeKind::LocalDeclaration, 0),
        ("const local c = 1", NodeKind::LocalDeclaration, 6),
        ("global const c::Int = 1", NodeKind::GlobalDeclaration, 0),
        ("const local c::Int = 1", NodeKind::LocalDeclaration, 6),
        ("global\nconst c = 1", NodeKind::GlobalDeclaration, 0),
        ("global const\nc = 1", NodeKind::GlobalDeclaration, 0),
        ("const global\nc = 1", NodeKind::GlobalDeclaration, 6),
        ("global const c = d => e", NodeKind::GlobalDeclaration, 0),
        ("const global c = d += e", NodeKind::GlobalDeclaration, 6),
        ("local const c = d -> e", NodeKind::LocalDeclaration, 0),
    ] {
        let cst = parse(source).expect("scoped const declaration must parse");
        assert_eq!(cst.children.len(), 1, "source: {source:?}");
        let const_decl = &cst.children[0];
        assert_eq!(const_decl.kind, NodeKind::ConstDeclaration);
        assert_eq!(const_decl.span.start, 0, "source: {source:?}");
        assert_eq!(const_decl.span.end, source.len(), "source: {source:?}");
        assert_eq!(const_decl.children.len(), 1, "source: {source:?}");
        let scope_decl = &const_decl.children[0];
        assert_eq!(scope_decl.kind, scope_kind, "source: {source:?}");
        assert_eq!(scope_decl.span.start, scope_start, "source: {source:?}");
        assert_eq!(scope_decl.span.end, source.len(), "source: {source:?}");
        assert_eq!(scope_decl.children.len(), 1, "source: {source:?}");
        assert_eq!(scope_decl.children[0].kind, NodeKind::BinaryExpression);
        let target = &scope_decl.children[0].children[0];
        let identifier = if target.kind == NodeKind::TypedExpression {
            &target.children[0]
        } else {
            target
        };
        assert_eq!(identifier.kind, NodeKind::Identifier);
        assert_eq!(identifier.text_from_source(source), "c");
    }

    for (source, expected_span) in [
        ("global end", 7..10),
        ("local end", 6..9),
        ("global else", 7..11),
        ("local catch", 6..11),
    ] {
        let error = parse(source).expect_err("reserved keyword must not become an Identifier");
        let span = error.span().expect("declaration error must carry a span");
        assert_eq!(span.start..span.end, expected_span, "source: {source:?}");
        assert!(
            matches!(&error, ParseError::InvalidSyntax { message, .. } if message == "invalid identifier"),
            "source: {source:?}, error: {error:?}"
        );
        assert!(!error.is_incomplete_input(), "source: {source:?}");
    }

    for (source, expected_span) in [("global =", 7..8), ("global )", 7..8), ("local ,", 6..7)] {
        let error = parse(source).expect_err("punctuation must not become an Identifier");
        let span = error.span().expect("declaration error must carry a span");
        assert_eq!(span.start..span.end, expected_span, "source: {source:?}");
        assert!(
            matches!(&error, ParseError::UnexpectedToken { expected, .. } if expected == "expression"),
            "source: {source:?}, error: {error:?}"
        );
        assert!(!error.is_incomplete_input(), "source: {source:?}");
    }

    let literal = parse("global 1").expect("scoped literal expression must parse");
    assert_eq!(literal.children[0].kind, NodeKind::GlobalDeclaration);
    assert_eq!(
        literal.children[0].children[0].kind,
        NodeKind::IntegerLiteral
    );

    let tuple = parse("global const c, d = 1").expect("scoped const tuple must parse");
    let assignment = &tuple.children[0].children[0].children[0];
    assert_eq!(assignment.kind, NodeKind::BinaryExpression);
    assert_eq!(assignment.children[0].kind, NodeKind::TupleExpression);
    assert_eq!(assignment.children[0].children.len(), 2);

    // Assignment RHS must admit pair/arrow/nested-assignment precedence (Issue #10947).
    for (source, rhs_kind) in [
        ("const c = d => e", NodeKind::BinaryExpression),
        ("global const c = d => e", NodeKind::BinaryExpression),
        (
            "const global c = d += e",
            NodeKind::CompoundAssignmentExpression,
        ),
        ("local const c = d -> e", NodeKind::ArrowFunctionExpression),
    ] {
        let cst = parse(source).expect("low-precedence const RHS must parse");
        let outer_assignment = if source.starts_with("const c") {
            &cst.children[0].children[0]
        } else {
            &cst.children[0].children[0].children[0]
        };
        assert_eq!(outer_assignment.kind, NodeKind::BinaryExpression);
        assert_eq!(outer_assignment.children[2].kind, rhs_kind);
        assert_eq!(outer_assignment.span.end, source.len());
    }

    for (source, expected_span) in [
        ("global const c", 0..14),
        ("local const in", 0..14),
        ("const global c", 0..14),
        ("global const", 0..12),
        ("const global const c = 1", 0..24),
        ("global const end", 0..16),
        ("global const =", 0..14),
        ("global const;", 0..12),
        ("global const c += 1", 0..19),
        ("global const c +=", 0..17),
        ("global const c := 1", 0..19),
        ("global const c => d", 0..19),
        ("global const c -> d", 0..19),
        ("global const c +=;", 0..17),
        ("const global const c += 1", 0..25),
        ("const global const c => d", 0..25),
        ("const global const c +=;", 0..23),
        ("const global const;", 0..18),
        ("global const c => d => e", 0..24),
        ("global const c += d += e", 0..24),
        ("global const c => d += e", 0..24),
        ("const global const c += d => e", 0..30),
    ] {
        let error = parse(source).expect_err("const declaration requires one assignment");
        let span = error.span().expect("const error must carry a span");
        assert_eq!(span.start..span.end, expected_span, "source: {source:?}");
        assert!(
            matches!(&error, ParseError::InvalidSyntax { message, .. } if message == "expected assignment after `const`"),
            "source: {source:?}, error: {error:?}"
        );
        assert!(!error.is_incomplete_input(), "source: {source:?}");
    }

    for (source, expected_offset) in [("global", 6), ("local", 5)] {
        let error = parse(source).expect_err("bare scope declaration must not panic");
        let span = error.span().expect("EOF error must carry a span");
        assert_eq!(
            span.start..span.end,
            expected_offset..expected_offset,
            "source: {source:?}"
        );
        assert!(
            matches!(
                &error,
                ParseError::UnexpectedEof { expected, .. }
                    if expected == "variable declaration"
            ),
            "source: {source:?}, error: {error:?}"
        );
        assert!(error.is_incomplete_input(), "source: {source:?}");
    }

    let incomplete_rhs = parse("global const c =")
        .expect_err("a real assignment with a missing RHS must remain incomplete");
    assert!(
        matches!(incomplete_rhs, ParseError::UnexpectedEof { .. }),
        "error: {incomplete_rhs:?}"
    );
    assert!(incomplete_rhs.is_incomplete_input());

    for (source, scope_kind, name) in [
        ("global in", NodeKind::GlobalDeclaration, "in"),
        ("local isa", NodeKind::LocalDeclaration, "isa"),
    ] {
        let cst = parse(source).expect("operator keyword declaration must parse");
        assert_eq!(cst.children[0].kind, scope_kind);
        assert_eq!(cst.children[0].children[0].kind, NodeKind::Identifier);
        assert_eq!(cst.children[0].children[0].text_from_source(source), name);
    }
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

#[test]
fn test_return_multiline_tuple_issue_8753() {
    assert_root_child_kind("return a,\n       b", NodeKind::ReturnStatement);
}

#[test]
fn test_bare_tuple_expression_statement_issue_8908() {
    assert_root_child_kind("function f()\n    r, f\nend", NodeKind::FunctionDefinition);
    assert_root_child_kind(
        "function f()\n    xy, fma(x, y, -xy)\nend",
        NodeKind::FunctionDefinition,
    );
}

// =============================================================================
// Export Statement
// =============================================================================

#[test]
fn test_export_single() {
    assert_root_child_kind("export foo", NodeKind::ExportStatement);
    assert_root_child_kind("export var\"#\"", NodeKind::ExportStatement);
}

#[test]
fn test_export_multiple() {
    assert_root_child_kind("export foo, bar", NodeKind::ExportStatement);
    assert_root_child_kind(
        "export\n    AbstractTerminal,\n    TextTerminal,\n    raw!",
        NodeKind::ExportStatement,
    );
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
    assert_root_child_kind("public var\"#\"", NodeKind::PublicStatement);
}

#[test]
fn test_public_multiple() {
    assert_root_child_kind("public foo, bar", NodeKind::PublicStatement);
    assert_root_child_kind("public\n    foo,\n    bar", NodeKind::PublicStatement);
}

// =============================================================================
// Import Statement
// =============================================================================

#[test]
fn test_import_module() {
    assert_root_child_kind("import Base", NodeKind::ImportStatement);
    assert_root_child_kind(
        "import Base,\n    Core,\n    Main",
        NodeKind::ImportStatement,
    );
}

#[test]
fn test_import_submodule() {
    assert_root_child_kind("import Base.Math", NodeKind::ImportStatement);
}

#[test]
fn test_import_specific() {
    assert_root_child_kind("import Base: sin, cos", NodeKind::ImportStatement);
    assert_root_child_kind("import Base: *, ==, +", NodeKind::ImportStatement);
    assert_root_child_kind("import A: var\"#\"", NodeKind::ImportStatement);
    assert_root_child_kind("import A.B: c.@d", NodeKind::ImportStatement);
    assert_root_child_kind("using A: (..)", NodeKind::UsingStatement);
    assert_root_child_kind("using A: (..) as twodots", NodeKind::UsingStatement);
    assert_parses("import Base: *,\n ==,\n +");
    assert_root_child_kind(
        "import Base:\n    raw!,\n    pipe_writer",
        NodeKind::ImportStatement,
    );
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
    assert_parses("import ..no_op_err, ..@inline, ..@noinline, ..checked_length");
    assert_parses("import Base.Foo.:(==).bar");
    assert_parses("import .Mod.func as @notmacro");
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
    assert_parses("global $(esc(:x)) = 1");
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
fn test_local_tuple_with_value() {
    assert_root_child_kind(
        "local (p, s) = listenany(testport)",
        NodeKind::LocalDeclaration,
    );
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

// ==================== Implicit line continuation (Issue #8753) ====================
//
// Julia allows a newline inside several delimited contexts without terminating
// the current statement.  These tests are oracle-verified against JuliaSyntax
// (JuliaSyntax.parseall / upstream Julia 1.12) and serve as regression guards
// for the fixes in literals.rs and expressions/primary.rs.

/// `@macro(\n args\n)` — macro call with newlines inside the `(...)`.
/// Previously failed with "unexpected token 'newline', expected RParen" because
/// the macro call parenthesized path did not increment grouping_depth and did
/// not skip newlines before the closing `)` (Issue #8753).
#[test]
fn test_macro_call_multiline_args_8753() {
    // Basic: single arg, trailing newline before )
    assert_parses("@logmsg(\n    deplevel\n)");
    // Multiple args, last arg on its own line before )
    assert_parses("@macro(\n    a,\n    b,\n    c\n)");
    // Keyword argument (assignment-style) followed by newline
    assert_parses("@logmsg(\n    maxlog=1\n)");
    // Ternary inside keyword arg + newline before )
    assert_parses("@logmsg(\n    maxlog=x === nothing ? nothing : 1\n)");
    // begin...end block as argument, closing ) on its own line
    assert_parses("@test_logs(\n    (Info, \"msg\"),\n    begin\n        f()\n    end\n)");
    // Nested macro call with multiline args (timing.jl @__tryfinally pattern)
    assert_parses(
        "@__tryfinally(\n    $(esc(ex)),\n    Base.Threads.atomic_sub!(Base.TIMING_IMPORTS, 1)\n)",
    );
}

/// `:(  \n  expr  \n)` — parenthesized quote expression spanning multiple lines.
/// Previously failed with "unexpected token 'newline', expected expression"
/// because the `:(` parser did not skip the initial newline (Issue #8753).
#[test]
fn test_quote_expr_multiline_8753() {
    // Simple: newline after opening :(
    assert_parses("callexpr = :(\n    f(x)\n)");
    // Nested: write(..., :( module ... end ))
    assert_parses("write(path, :(\nmodule Foo\n    x = 1\nend\n))");
    // Quoted statement on its own lines
    assert_parses(":(\n    a = 1\n)");
}

/// Multi-line arguments inside regular `(...)` should already work
/// (grouping_depth is incremented by the tuple/paren parser), but
/// confirm they continue to work after the #8753 changes.
#[test]
fn test_regular_call_multiline_still_works_8753() {
    // Non-macro call with newline before )
    assert_parses("foo(\n    a,\n    b\n)");
    // Keyword arg in non-macro call
    assert_parses("foo(\n    maxlog=1\n)");
    // Two separate statements must NOT be merged
    let (cst, errs) = subset_julia_vm_parser::parse_with_errors("a = 1\nb = 2");
    assert!(errs.is_empty(), "Two statements should parse cleanly");
    assert_eq!(
        cst.children.len(),
        2,
        "Two top-level statements expected, got {}",
        cst.children.len()
    );
}

// ============================================================================
// Issue #10951 — differential scoped-declaration grammar matrix
// ============================================================================
//
// Derived from upstream `julia/src/julia-parser.scm`'s `(global local)` arm
// (it routes through `parse-eq`, so a FULL expression follows the keyword)
// and verified against julia 1.12.6. Every token after `global`/`local` is
// classified into exactly one role:
//
//   Modifier    `const`                          → ConstDeclaration wrapper
//   Delegated   reserved-word constructs         → normal CST node wrapped in
//                                                  the Global/Local declaration
//                                                  (invalid combinations are
//                                                  rejected AT LOWERING with
//                                                  upstream's `invalid syntax
//                                                  in "global" declaration`)
//   ValidName   identifiers / operator names     → Identifier item
//   Expression  literals / operator tails        → expression item (rejected
//                                                  at lowering, never split
//                                                  into a second statement)
//   Error       structural keywords/punctuation  → exact ParseError class+span
//
// MUTATION CONTRACT — each grammar authority is proven necessary because
// bypassing it turns at least one row below red:
//   1. Token-role classification: parsing the next token as an identifier
//      (the pre-#10927 escape hatch) turns every Delegated row into
//      `GlobalDeclaration(Identifier)` with a dangling `end` → the child-kind
//      assertions AND the incomplete-prefix assertions fail.
//   2. Modifier normalization: treating `const` after `global`/`local` as a
//      declared name breaks the ConstDeclaration wrapper rows (#10938 test
//      above) and the newline-placement rows here.
//   3. Assignment enforcement: skipping the scoped-const assignment check
//      makes the exact-span error rows in the #10938 test parse → red.
//   4. Assignment-precedence RHS parsing: parsing the scoped RHS below
//      assignment precedence loses the pair/arrow/ternary/nested-assignment
//      RHS rows (#10947 test above and the tier rows here).
//   5. Operator-tail consumption: dropping it splits `global c + 1` into two
//      top-level statements → the statement-count row fails (Issue #10945).
//   6. Comma-list distribution: parsing declaration items one by one binds
//      `= rhs` to the last name only → the tuple-assignment shape row fails
//      (Issue #11009).

/// Delegated reserved-word constructs after `global`/`local` parse into the
/// declaration wrapper around their ordinary CST node (Issues #10945/#10937).
#[test]
fn test_scoped_declaration_delegated_construct_matrix_issue_10951() {
    for (source, wrapper, child) in [
        // definitions
        (
            "global function f()\n1\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::FunctionDefinition,
        ),
        (
            "local function f()\n1\nend",
            NodeKind::LocalDeclaration,
            NodeKind::FunctionDefinition,
        ),
        (
            "global macro m()\n1\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::MacroDefinition,
        ),
        (
            "local macro m()\n1\nend",
            NodeKind::LocalDeclaration,
            NodeKind::MacroDefinition,
        ),
        (
            "global module M\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::ModuleDefinition,
        ),
        (
            "local module M\nend",
            NodeKind::LocalDeclaration,
            NodeKind::ModuleDefinition,
        ),
        (
            "global baremodule M\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::BaremoduleDefinition,
        ),
        (
            "global struct S\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::StructDefinition,
        ),
        (
            "global mutable struct S\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::MutableStructDefinition,
        ),
        (
            "global abstract type T end",
            NodeKind::GlobalDeclaration,
            NodeKind::AbstractDefinition,
        ),
        (
            "global primitive type P 8 end",
            NodeKind::GlobalDeclaration,
            NodeKind::PrimitiveDefinition,
        ),
        // control flow
        (
            "global while false\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::WhileStatement,
        ),
        (
            "local while false\nend",
            NodeKind::LocalDeclaration,
            NodeKind::WhileStatement,
        ),
        (
            "global for i in 1:1\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::ForStatement,
        ),
        (
            "global if true\n1\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::IfStatement,
        ),
        (
            "global try\n1\ncatch\nend",
            NodeKind::GlobalDeclaration,
            NodeKind::TryStatement,
        ),
        // jumps and import-likes
        (
            "global return 1",
            NodeKind::GlobalDeclaration,
            NodeKind::ReturnStatement,
        ),
        (
            "global break",
            NodeKind::GlobalDeclaration,
            NodeKind::BreakStatement,
        ),
        (
            "global continue",
            NodeKind::GlobalDeclaration,
            NodeKind::ContinueStatement,
        ),
        (
            "global using Foo",
            NodeKind::GlobalDeclaration,
            NodeKind::UsingStatement,
        ),
        (
            "global import Foo",
            NodeKind::GlobalDeclaration,
            NodeKind::ImportStatement,
        ),
        (
            "global export foo",
            NodeKind::GlobalDeclaration,
            NodeKind::ExportStatement,
        ),
        // nested scope modifiers
        (
            "global global x",
            NodeKind::GlobalDeclaration,
            NodeKind::GlobalDeclaration,
        ),
        (
            "global local x",
            NodeKind::GlobalDeclaration,
            NodeKind::LocalDeclaration,
        ),
        (
            "local global x",
            NodeKind::LocalDeclaration,
            NodeKind::GlobalDeclaration,
        ),
    ] {
        let cst = parse(source)
            .unwrap_or_else(|e| panic!("delegated construct must parse: {source:?}, error: {e:?}"));
        assert_eq!(cst.children.len(), 1, "source: {source:?}");
        let decl = &cst.children[0];
        assert_eq!(decl.kind, wrapper, "source: {source:?}");
        assert_eq!(decl.span.start, 0, "source: {source:?}");
        assert_eq!(decl.span.end, source.len(), "source: {source:?}");
        assert_eq!(decl.children.len(), 1, "source: {source:?}");
        assert_eq!(decl.children[0].kind, child, "source: {source:?}");
    }

    // `begin`/`let`/`quote` after the keyword parse into the declaration
    // wrapper too; their exact node kind is the same one the construct
    // produces at statement level, so assert wrapper + single child only.
    for source in [
        "global begin\n1\nend",
        "global let x = 1\nx\nend",
        "global quote\n1\nend",
    ] {
        let cst = parse(source)
            .unwrap_or_else(|e| panic!("block construct must parse: {source:?}, error: {e:?}"));
        assert_eq!(cst.children.len(), 1, "source: {source:?}");
        assert_eq!(
            cst.children[0].kind,
            NodeKind::GlobalDeclaration,
            "source: {source:?}"
        );
        assert_eq!(cst.children[0].children.len(), 1, "source: {source:?}");
    }
}

/// Valid prefixes of delegated constructs classify as INCOMPLETE input (the
/// REPL can finish them), not as permanent invalid-keyword errors
/// (Issue #10945). The scoped prefix must classify exactly like the
/// unprefixed construct — the `global`/`local` wrapper never changes the
/// incomplete-input verdict. (Bare `function`/`macro` currently classify as
/// non-incomplete parser-wide; the equality assertion below keeps the scoped
/// form in lockstep with whatever the construct itself reports.)
#[test]
fn test_scoped_declaration_incomplete_prefix_matrix_issue_10951() {
    for construct in [
        "module",
        "module M",
        "while",
        "while false",
        "function",
        "function f()",
        "macro",
        "struct S",
        "for i in 1:1",
        "begin",
        "try",
        "if true",
    ] {
        let bare = parse(construct).expect_err("construct prefix must not parse");
        for keyword in ["global", "local"] {
            let source = format!("{keyword} {construct}");
            let error = parse(&source).expect_err("scoped prefix must not parse");
            assert_eq!(
                error.is_incomplete_input(),
                bare.is_incomplete_input(),
                "source: {source:?} must classify like {construct:?}, got: {error:?}"
            );
        }
    }

    // These specific prefixes are the #10945 acceptance rows: they MUST be
    // incomplete (a REPL continuation can complete them).
    for source in [
        "global module",
        "global module M",
        "local while",
        "local while false",
        "global function f()",
        "global struct S",
        "global for i in 1:1",
        "global begin",
        "global try",
        "local if true",
        "global const c =",
    ] {
        let error = parse(source).expect_err("prefix must not parse");
        assert!(
            error.is_incomplete_input(),
            "source: {source:?} must classify as incomplete, got: {error:?}"
        );
    }
}

/// Non-name expression items stay INSIDE the declaration (one top-level
/// statement, upstream `(global (call ...))` shape) instead of silently
/// splitting into `global c` plus a stray expression statement
/// (Issues #10945/#11009).
#[test]
fn test_scoped_declaration_expression_item_matrix_issue_10951() {
    for (source, child) in [
        ("global c + 1", NodeKind::BinaryExpression),
        ("global c => 2", NodeKind::BinaryExpression),
        ("local c * d", NodeKind::BinaryExpression),
        ("global 2 + 3", NodeKind::BinaryExpression),
        ("global 1", NodeKind::IntegerLiteral),
    ] {
        let cst = parse(source)
            .unwrap_or_else(|e| panic!("expression item must parse: {source:?}, error: {e:?}"));
        assert_eq!(
            cst.children.len(),
            1,
            "declaration must stay one statement, source: {source:?}"
        );
        let decl = &cst.children[0];
        assert_eq!(decl.span.end, source.len(), "source: {source:?}");
        assert_eq!(decl.children.len(), 1, "source: {source:?}");
        assert_eq!(decl.children[0].kind, child, "source: {source:?}");
    }
}

/// Comma-list `= rhs` distribution: `global x, y = 1, 2` is one destructuring
/// assignment over the whole name list (upstream
/// `(global (= (tuple x y) (tuple 1 2)))`), and a bare comma list keeps its
/// per-name items (Issue #11009).
#[test]
fn test_scoped_declaration_comma_distribution_issue_10951() {
    for source in ["global x, y = 1, 2", "local x, y = 1, 2"] {
        let cst = parse(source).unwrap_or_else(|e| panic!("must parse: {source:?}, error: {e:?}"));
        assert_eq!(cst.children.len(), 1, "source: {source:?}");
        let decl = &cst.children[0];
        assert_eq!(decl.children.len(), 1, "source: {source:?}");
        let assignment = &decl.children[0];
        assert_eq!(
            assignment.kind,
            NodeKind::BinaryExpression,
            "source: {source:?}"
        );
        assert_eq!(
            assignment.children[0].kind,
            NodeKind::TupleExpression,
            "LHS must be the whole name list, source: {source:?}"
        );
        assert_eq!(
            assignment.children[0].children.len(),
            2,
            "source: {source:?}"
        );
        assert_eq!(
            assignment.children[2].kind,
            NodeKind::TupleExpression,
            "RHS must keep the whole tuple, source: {source:?}"
        );
    }

    // Bare comma list: per-name Identifier items (established CST shape).
    let cst = parse("global x, y").expect("bare list must parse");
    let decl = &cst.children[0];
    assert_eq!(decl.kind, NodeKind::GlobalDeclaration);
    assert_eq!(decl.children.len(), 2);
    assert!(decl.children.iter().all(|c| c.kind == NodeKind::Identifier));
}

/// Scoped assignment RHS admits every relevant precedence tier: pair, arrow,
/// ternary, nested assignment, chained low-precedence tails, and comma tuples
/// (Issues #10947/#10951).
#[test]
fn test_scoped_declaration_rhs_precedence_tier_matrix_issue_10951() {
    for (source, rhs_kind) in [
        ("global c = d => e", NodeKind::BinaryExpression),
        ("global c = x -> x", NodeKind::ArrowFunctionExpression),
        ("local c = true ? 1 : 2", NodeKind::TernaryExpression),
        ("global c = d = 1", NodeKind::Assignment),
        ("global c = d => e => f", NodeKind::BinaryExpression),
    ] {
        let cst = parse(source).unwrap_or_else(|e| panic!("must parse: {source:?}, error: {e:?}"));
        assert_eq!(cst.children.len(), 1, "source: {source:?}");
        let decl = &cst.children[0];
        let assignment = &decl.children[0];
        assert_eq!(
            assignment.kind,
            NodeKind::BinaryExpression,
            "source: {source:?}"
        );
        assert_eq!(assignment.span.end, source.len(), "source: {source:?}");
        assert_eq!(assignment.children[2].kind, rhs_kind, "source: {source:?}");
    }
}

/// Modifier orders × newline placements for scoped-const declarations.
/// Upstream (JuliaSyntax 1.12) rejects a newline directly after a leading
/// `const` but accepts one after the scope keyword or between the modifier
/// pair and the name (Issues #10938/#10943/#10951).
#[test]
fn test_scoped_const_newline_placement_matrix_issue_10951() {
    for (source, scope_kind) in [
        ("global const c = 1", NodeKind::GlobalDeclaration),
        ("const global c = 1", NodeKind::GlobalDeclaration),
        ("local const c = 1", NodeKind::LocalDeclaration),
        ("const local c = 1", NodeKind::LocalDeclaration),
        ("global\nconst c = 1", NodeKind::GlobalDeclaration),
        ("local\nconst c = 1", NodeKind::LocalDeclaration),
        ("global const\nc = 1", NodeKind::GlobalDeclaration),
        ("local const\nc = 1", NodeKind::LocalDeclaration),
        ("const global\nc = 1", NodeKind::GlobalDeclaration),
        ("const local\nc = 1", NodeKind::LocalDeclaration),
        ("global\nconst\nc = 1", NodeKind::GlobalDeclaration),
    ] {
        let cst = parse(source).unwrap_or_else(|e| panic!("must parse: {source:?}, error: {e:?}"));
        assert_eq!(cst.children.len(), 1, "source: {source:?}");
        let const_decl = &cst.children[0];
        assert_eq!(
            const_decl.kind,
            NodeKind::ConstDeclaration,
            "source: {source:?}"
        );
        assert_eq!(const_decl.children.len(), 1, "source: {source:?}");
        assert_eq!(
            const_decl.children[0].kind, scope_kind,
            "source: {source:?}"
        );
    }
}
