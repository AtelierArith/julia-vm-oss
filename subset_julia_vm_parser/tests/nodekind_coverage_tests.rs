//! Exhaustive NodeKind Coverage Tests
//!
//! This file ensures every NodeKind variant has at least one test that verifies
//! the kind is produced by parsing the expected Julia syntax.
//!
//! **Prevention (Issue #1627):** When adding a new NodeKind variant, you MUST:
//! 1. Add it to `NodeKind::all_variants()` and `variant_count()` in `node_kind/mod.rs`
//! 2. Add it to `COVERAGE_EXAMPLES` below (with a syntax example that produces it), OR
//! 3. Add it to `NOT_PRODUCED_BY_PARSER` below (with an explanation of why)
//!
//! Failure to do so will cause `test_every_nodekind_variant_is_accounted_for` to fail.
//!
//! Related: Issue #1584, Issue #1627

use std::collections::HashSet;
use subset_julia_vm_parser::{parse_with_errors, CstNode, NodeKind};

/// Parse source and return the first top-level node
fn parse_first(source: &str) -> CstNode {
    let (root, errors) = parse_with_errors(source);
    assert!(
        errors.is_empty(),
        "Parse errors for '{}': {:?}",
        source,
        errors.errors()
    );
    assert_eq!(root.kind, NodeKind::SourceFile);
    root.children
        .into_iter()
        .next()
        .expect("no expression parsed")
}

/// Find a node of the given kind anywhere in the tree
fn find_kind(node: &CstNode, kind: NodeKind) -> Option<CstNode> {
    if node.kind == kind {
        return Some(node.clone());
    }
    for child in &node.children {
        if let Some(found) = find_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

/// Coverage entry: (source, expected_kind, description)
/// The expected_kind should appear somewhere in the parse tree
type CoverageEntry = (&'static str, NodeKind, &'static str);

/// All NodeKind coverage examples
/// Each entry documents a syntactic example that produces the NodeKind
const COVERAGE_EXAMPLES: &[CoverageEntry] = &[
    // ==================== Top Level ====================
    // SourceFile is tested implicitly by every parse operation
    ("begin x; y end", NodeKind::Block, "Block inside begin"),
    // ==================== Definitions ====================
    (
        "module M end",
        NodeKind::ModuleDefinition,
        "Module definition",
    ),
    (
        "abstract type T end",
        NodeKind::AbstractDefinition,
        "Abstract type",
    ),
    (
        "primitive type T 8 end",
        NodeKind::PrimitiveDefinition,
        "Primitive type",
    ),
    (
        "struct S x end",
        NodeKind::StructDefinition,
        "Struct definition",
    ),
    (
        "function f() end",
        NodeKind::FunctionDefinition,
        "Function definition",
    ),
    (
        "macro m() end",
        NodeKind::MacroDefinition,
        "Macro definition",
    ),
    (
        "mutable struct S x end",
        NodeKind::MutableStructDefinition,
        "Mutable struct",
    ),
    (
        "baremodule M end",
        NodeKind::BaremoduleDefinition,
        "Baremodule",
    ),
    // Short function definition (operator method form)
    (
        "*(x, y) = x * y",
        NodeKind::ShortFunctionDefinition,
        "Operator method short function definition",
    ),
    // Definition components
    (
        "function f(x, y) end",
        NodeKind::ParameterList,
        "Parameter list",
    ),
    ("function f(x) end", NodeKind::Parameter, "Single parameter"),
    (
        "function f(; x=1) end",
        NodeKind::KwParameter,
        "Keyword parameter",
    ),
    (
        "function f(x...) end",
        NodeKind::SplatParameter,
        "Splat parameter",
    ),
    (
        "struct S{T} x end",
        NodeKind::TypeParameters,
        "Type parameters",
    ),
    (
        "struct S{T <: Number} x end",
        NodeKind::TypeParameter,
        "Type parameter with bound",
    ),
    (
        "f(x) where {T <: Number} = x",
        NodeKind::SubtypeConstraint,
        "Subtype constraint in braced where",
    ),
    (
        "f(x) where {T >: Integer} = x",
        NodeKind::SupertypeConstraint,
        "Supertype constraint in braced where",
    ),
    (
        "f(x) where {T <: Number} = x",
        NodeKind::TypeParameterList,
        "Type parameter list in braced where",
    ),
    (
        "function f(x) where T end",
        NodeKind::WhereClause,
        "Where clause",
    ),
    // ==================== Statements ====================
    ("begin x end", NodeKind::BeginBlock, "Begin block"),
    (
        "let x = 1; x end",
        NodeKind::LetExpression,
        "Let expression",
    ),
    ("let x = 1; x end", NodeKind::LetBindings, "Let bindings"),
    ("if x y end", NodeKind::IfStatement, "If statement"),
    (
        "if x y elseif z w end",
        NodeKind::ElseifClause,
        "Elseif clause",
    ),
    ("if x y else z end", NodeKind::ElseClause, "Else clause"),
    ("try x catch end", NodeKind::TryStatement, "Try statement"),
    ("try x catch e end", NodeKind::CatchClause, "Catch clause"),
    (
        "try x finally y end",
        NodeKind::FinallyClause,
        "Finally clause",
    ),
    (
        "for i in 1:10 i end",
        NodeKind::ForStatement,
        "For statement",
    ),
    ("while x x end", NodeKind::WhileStatement, "While statement"),
    ("break", NodeKind::BreakStatement, "Break"),
    ("continue", NodeKind::ContinueStatement, "Continue"),
    ("return", NodeKind::ReturnStatement, "Return"),
    (
        "const x = 1",
        NodeKind::ConstDeclaration,
        "Const declaration",
    ),
    (
        "global x",
        NodeKind::GlobalDeclaration,
        "Global declaration",
    ),
    ("local x", NodeKind::LocalDeclaration, "Local declaration"),
    ("export f", NodeKind::ExportStatement, "Export"),
    (
        "public f",
        NodeKind::PublicStatement,
        "Public (Julia 1.11+)",
    ),
    ("import Base", NodeKind::ImportStatement, "Import"),
    ("using Test", NodeKind::UsingStatement, "Using"),
    (
        "import A: f as g",
        NodeKind::ImportAlias,
        "Import function alias",
    ),
    ("import .A", NodeKind::ImportPath, "Import path"),
    ("import A, B", NodeKind::ImportList, "Import list"),
    // Note: SelectedImport NodeKind exists but is not currently used by parser
    // The parser creates ImportPath with child items instead

    // ==================== Expressions ====================
    ("x = 1", NodeKind::Assignment, "Assignment"),
    (
        "x += 1",
        NodeKind::CompoundAssignmentExpression,
        "Compound assignment",
    ),
    // Note: OpenTuple NodeKind exists but parser uses TupleExpression instead
    ("a + b", NodeKind::BinaryExpression, "Binary expression"),
    ("-x", NodeKind::UnaryExpression, "Unary expression"),
    ("a ? b : c", NodeKind::TernaryExpression, "Ternary"),
    ("f(x)", NodeKind::CallExpression, "Call expression"),
    ("f.(x)", NodeKind::BroadcastCallExpression, "Broadcast call"),
    ("map(x) do y y end", NodeKind::DoClause, "Do clause"),
    (
        "x -> x^2",
        NodeKind::ArrowFunctionExpression,
        "Arrow function",
    ),
    ("2x", NodeKind::JuxtapositionExpression, "Juxtaposition"),
    ("1:10", NodeKind::RangeExpression, "Range"),
    ("x...", NodeKind::SplatExpression, "Splat"),
    ("x::Int", NodeKind::TypedExpression, "Typed expression"),
    (
        "Vector{T} where T",
        NodeKind::WhereExpression,
        "Where expression",
    ),
    ("@test x", NodeKind::MacrocallExpression, "Macro call"),
    // Note: MacroArgumentList exists but parser puts args directly in MacrocallExpression

    // ==================== Primary Expressions ====================
    ("x", NodeKind::Identifier, "Identifier"),
    ("true", NodeKind::BooleanLiteral, "Boolean literal"),
    ("(x)", NodeKind::ParenthesizedExpression, "Parenthesized"),
    ("(1, 2)", NodeKind::TupleExpression, "Tuple"),
    // Note: CurlyExpression exists but is not produced by parser (Julia syntax error at top-level)
    ("x'", NodeKind::AdjointExpression, "Adjoint"),
    ("a.b", NodeKind::FieldExpression, "Field access"),
    ("a[1]", NodeKind::IndexExpression, "Index expression"),
    (
        "Point{T}",
        NodeKind::ParametrizedTypeExpression,
        "Parametrized type",
    ),
    // Note: InterpolationExpression exists but parser uses StringInterpolation
    (":x", NodeKind::QuoteExpression, "Quote expression"),
    // ==================== Arrays ====================
    ("[1, 2]", NodeKind::VectorExpression, "Vector"),
    ("[1 2; 3 4]", NodeKind::MatrixExpression, "Matrix"),
    ("[1 2; 3 4]", NodeKind::MatrixRow, "Matrix row"),
    (
        "[x for x in 1:10]",
        NodeKind::ComprehensionExpression,
        "Comprehension",
    ),
    ("(x for x in 1:10)", NodeKind::Generator, "Generator"),
    ("[x for x in 1:10]", NodeKind::ForClause, "For clause"),
    (
        "[x for x in 1:10 if x > 5]",
        NodeKind::IfClause,
        "If clause",
    ),
    ("[x for x in 1:10]", NodeKind::ForBinding, "For binding"),
    // ==================== Literals ====================
    ("42", NodeKind::IntegerLiteral, "Integer"),
    ("3.14", NodeKind::FloatLiteral, "Float"),
    ("\"hello\"", NodeKind::StringLiteral, "String"),
    ("'a'", NodeKind::CharacterLiteral, "Character"),
    ("`ls`", NodeKind::CommandLiteral, "Command"),
    (
        "r\"raw\"",
        NodeKind::PrefixedStringLiteral,
        "Prefixed string",
    ),
    ("\"hello\"", NodeKind::Content, "String content"),
    (
        "\"$x\"",
        NodeKind::StringInterpolation,
        "String interpolation",
    ),
    // ==================== Operators ====================
    ("a + b", NodeKind::Operator, "Operator"),
    ("f(x; y=1)", NodeKind::Semicolon, "Semicolon"),
    // ==================== Macro ====================
    ("@test x", NodeKind::MacroIdentifier, "Macro identifier"),
    // ==================== Type System ====================
    (
        "function f(::Int) end",
        NodeKind::TypedParameter,
        "Anonymous typed parameter",
    ),
    // ==================== Arguments ====================
    ("f(x, y)", NodeKind::ArgumentList, "Argument list"),
    // ==================== Other ====================
    ("f(x=1)", NodeKind::KeywordArgument, "Keyword argument"),
    // Note: NamedField exists but parser uses Assignment for named tuple elements
];

/// Test that all coverage examples parse correctly with expected NodeKind somewhere in tree
#[test]
fn test_coverage_examples_produce_expected_kinds() {
    for (source, expected_kind, description) in COVERAGE_EXAMPLES {
        let (root, errors) = parse_with_errors(source);
        assert!(
            errors.is_empty(),
            "Parse errors for '{}' ({}): {:?}",
            source,
            description,
            errors.errors()
        );

        let found = find_kind(&root, *expected_kind);
        assert!(
            found.is_some(),
            "Expected {:?} in parse tree of '{}' ({})",
            expected_kind,
            source,
            description
        );
    }
}

/// NodeKind variants that are NOT produced by the parser.
///
/// Each entry documents why the variant exists but is never produced.
/// When adding a new NodeKind variant that is not produced by the parser,
/// add it here with an explanation. (Issue #1627)
const NOT_PRODUCED_BY_PARSER: &[(NodeKind, &str)] = &[
    // Definition components reserved for tree-sitter compatibility
    (
        NodeKind::Signature,
        "Parser handles signatures inline within FunctionDefinition",
    ),
    (
        NodeKind::TypeHead,
        "Parser handles type heads inline within struct definitions",
    ),
    // Statement variants where parser uses a different NodeKind
    (
        NodeKind::CompoundStatement,
        "Parser uses BeginBlock instead",
    ),
    (NodeKind::LetStatement, "Parser uses LetExpression instead"),
    (
        NodeKind::ConstStatement,
        "Parser uses ConstDeclaration instead",
    ),
    (
        NodeKind::GlobalStatement,
        "Parser uses GlobalDeclaration instead",
    ),
    (
        NodeKind::LocalStatement,
        "Parser uses LocalDeclaration instead",
    ),
    // Expression variants
    (
        NodeKind::MacroArgumentList,
        "Parser puts args directly in MacrocallExpression",
    ),
    (
        NodeKind::CurlyExpression,
        "Not valid Julia syntax at top-level; reserved for tree-sitter compatibility",
    ),
    (
        NodeKind::UnaryTypedExpression,
        "Parser handles standalone type annotations differently",
    ),
    (
        NodeKind::SubtypeExpression,
        "Parser uses SubtypeConstraint in where clauses instead",
    ),
    // Comments are skipped by the parser
    (
        NodeKind::LineComment,
        "Parser skips comments during parsing",
    ),
    (
        NodeKind::BlockComment,
        "Parser skips comments during parsing",
    ),
];

/// NodeKind variants that are implicitly tested or are special cases
/// (not syntax-producible but still valid in the enum).
const IMPLICITLY_COVERED: &[(NodeKind, &str)] = &[
    (
        NodeKind::SourceFile,
        "Implicitly tested by every parse operation",
    ),
    (
        NodeKind::Error,
        "Only produced on parse errors — not testable with valid syntax",
    ),
    (
        NodeKind::Unknown,
        "Fallback for unrecognized tree-sitter node types",
    ),
];

/// **Prevention test (Issue #1627):** Ensure every NodeKind variant is accounted for.
///
/// Every variant from `NodeKind::all_variants()` must appear in exactly one of:
/// - `COVERAGE_EXAMPLES` (syntax that produces it)
/// - `NOT_PRODUCED_BY_PARSER` (documented reason it's unused)
/// - `IMPLICITLY_COVERED` (special cases like SourceFile, Error, Unknown)
///
/// When you add a new NodeKind variant, this test will FAIL until you add
/// the variant to one of these lists.
#[test]
fn test_every_nodekind_variant_is_accounted_for() {
    let covered: HashSet<NodeKind> = COVERAGE_EXAMPLES.iter().map(|(_, kind, _)| *kind).collect();
    let not_produced: HashSet<NodeKind> = NOT_PRODUCED_BY_PARSER
        .iter()
        .map(|(kind, _)| *kind)
        .collect();
    let implicit: HashSet<NodeKind> = IMPLICITLY_COVERED.iter().map(|(kind, _)| *kind).collect();

    let mut missing = Vec::new();
    let mut double_listed = Vec::new();

    for &kind in NodeKind::all_variants() {
        let in_covered = covered.contains(&kind);
        let in_not_produced = not_produced.contains(&kind);
        let in_implicit = implicit.contains(&kind);
        let count = [in_covered, in_not_produced, in_implicit]
            .iter()
            .filter(|&&b| b)
            .count();

        if count == 0 {
            missing.push(kind);
        } else if count > 1 {
            double_listed.push(kind);
        }
    }

    assert!(
        double_listed.is_empty(),
        "NodeKind variants listed in multiple places (fix by keeping in only one list): {:?}",
        double_listed
    );

    assert!(
        missing.is_empty(),
        "NodeKind variants not accounted for! Add to COVERAGE_EXAMPLES, \
         NOT_PRODUCED_BY_PARSER, or IMPLICITLY_COVERED.\n\
         Missing variants: {:?}\n\
         See Issue #1627 for the required checklist.",
        missing
    );
}

/// Test that we have coverage for the most important NodeKind variants
#[test]
fn test_coverage_completeness() {
    // Collect all NodeKinds that have coverage examples
    let covered: HashSet<NodeKind> = COVERAGE_EXAMPLES.iter().map(|(_, kind, _)| *kind).collect();

    // List of NodeKinds that are important and should have coverage
    let important_kinds = vec![
        // Definitions
        NodeKind::ModuleDefinition,
        NodeKind::StructDefinition,
        NodeKind::FunctionDefinition,
        NodeKind::MacroDefinition,
        NodeKind::MutableStructDefinition,
        NodeKind::AbstractDefinition,
        // Statements
        NodeKind::IfStatement,
        NodeKind::ForStatement,
        NodeKind::WhileStatement,
        NodeKind::TryStatement,
        NodeKind::ReturnStatement,
        NodeKind::BreakStatement,
        NodeKind::ContinueStatement,
        // Expressions
        NodeKind::Assignment,
        NodeKind::BinaryExpression,
        NodeKind::UnaryExpression,
        NodeKind::TernaryExpression,
        NodeKind::CallExpression,
        NodeKind::BroadcastCallExpression,
        NodeKind::ArrowFunctionExpression,
        NodeKind::RangeExpression,
        // Primary
        NodeKind::Identifier,
        NodeKind::BooleanLiteral,
        NodeKind::ParenthesizedExpression,
        NodeKind::TupleExpression,
        NodeKind::FieldExpression,
        NodeKind::IndexExpression,
        // Arrays
        NodeKind::VectorExpression,
        NodeKind::MatrixExpression,
        NodeKind::ComprehensionExpression,
        // Literals
        NodeKind::IntegerLiteral,
        NodeKind::FloatLiteral,
        NodeKind::StringLiteral,
        NodeKind::CharacterLiteral,
        // Import/Export
        NodeKind::ImportStatement,
        NodeKind::UsingStatement,
        NodeKind::ExportStatement,
    ];

    let missing: Vec<_> = important_kinds
        .into_iter()
        .filter(|k| !covered.contains(k))
        .collect();

    assert!(
        missing.is_empty(),
        "Important NodeKinds missing coverage: {:?}",
        missing
    );
}

/// Test specific NodeKind child structure to catch regressions
#[test]
fn test_binary_expression_children() {
    let node = parse_first("a + b");
    assert_eq!(node.kind, NodeKind::BinaryExpression);
    assert_eq!(
        node.children.len(),
        3,
        "BinaryExpression should have 3 children: lhs, op, rhs"
    );
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::Operator);
    assert_eq!(node.children[2].kind, NodeKind::Identifier);
}

#[test]
fn test_call_expression_children() {
    let node = parse_first("f(x, y)");
    assert_eq!(node.kind, NodeKind::CallExpression);
    assert_eq!(
        node.children.len(),
        2,
        "CallExpression: callee + argument_list"
    );
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[1].kind, NodeKind::ArgumentList);
}

#[test]
fn test_if_statement_children() {
    let node = parse_first("if x y else z end");
    assert_eq!(node.kind, NodeKind::IfStatement);
    assert!(
        node.children.len() >= 3,
        "IfStatement: condition, body, else clause"
    );
    // Last child is else clause
    assert_eq!(node.children.last().unwrap().kind, NodeKind::ElseClause);
}

#[test]
fn test_function_definition_children() {
    let node = parse_first("function foo(x) x end");
    assert_eq!(node.kind, NodeKind::FunctionDefinition);
    // First child is the function name (Identifier)
    assert_eq!(node.children[0].kind, NodeKind::Identifier);
    assert_eq!(node.children[0].text.as_deref(), Some("foo"));
    // Second child is parameter list
    assert_eq!(node.children[1].kind, NodeKind::ParameterList);
}

#[test]
fn test_range_expression_children() {
    // Two-element range
    let node = parse_first("1:10");
    assert_eq!(node.kind, NodeKind::RangeExpression);
    assert!(
        node.children.len() >= 2,
        "Range 1:10 should have at least 2 children"
    );

    // Three-element range
    let node = parse_first("1:2:10");
    assert_eq!(node.kind, NodeKind::RangeExpression);
    assert!(
        node.children.len() >= 2,
        "Range 1:2:10 should have children"
    );
}

#[test]
fn test_comprehension_children() {
    let node = parse_first("[x^2 for x in 1:10]");
    assert_eq!(node.kind, NodeKind::ComprehensionExpression);
    // Should have: body expression and for clause(s)
    let has_for_clause = find_kind(&node, NodeKind::ForClause).is_some();
    assert!(has_for_clause, "Comprehension should have ForClause");
}

// =============================================================================
// Predicate Consistency Tests (Issue #2265)
// =============================================================================
//
// These tests verify that predicate functions (is_statement, is_expression, etc.)
// only match NodeKind variants that are actually produced by the parser.
// This prevents bugs where predicates reference obsolete tree-sitter variant names.

/// Get all NodeKinds that are NOT produced by the parser.
fn get_not_produced_kinds() -> HashSet<NodeKind> {
    NOT_PRODUCED_BY_PARSER
        .iter()
        .map(|(kind, _)| *kind)
        .collect()
}

/// **Prevention test (Issue #2265):** Verify is_statement() only matches parser-produced variants.
///
/// When the parser uses different NodeKind names than tree-sitter (e.g., BeginBlock vs CompoundStatement),
/// predicates must match the parser-produced names, not the tree-sitter names.
#[test]
fn test_is_statement_only_matches_produced_variants() {
    let not_produced = get_not_produced_kinds();
    let mut problematic = Vec::new();

    for &kind in NodeKind::all_variants() {
        if kind.is_statement() && not_produced.contains(&kind) {
            problematic.push(kind);
        }
    }

    assert!(
        problematic.is_empty(),
        "is_statement() matches NodeKinds that are NOT produced by parser.\n\
         Problematic variants: {:?}\n\
         These predicates reference tree-sitter names instead of parser-produced names.\n\
         Fix by updating is_statement() in predicates.rs to use the parser-produced variant.",
        problematic
    );
}

/// **Prevention test (Issue #2265):** Verify is_expression() only matches parser-produced variants.
#[test]
fn test_is_expression_only_matches_produced_variants() {
    let not_produced = get_not_produced_kinds();
    let mut problematic = Vec::new();

    for &kind in NodeKind::all_variants() {
        if kind.is_expression() && not_produced.contains(&kind) {
            problematic.push(kind);
        }
    }

    assert!(
        problematic.is_empty(),
        "is_expression() matches NodeKinds that are NOT produced by parser.\n\
         Problematic variants: {:?}\n\
         These predicates reference tree-sitter names instead of parser-produced names.\n\
         Fix by updating is_expression() in predicates.rs to use the parser-produced variant.",
        problematic
    );
}

/// **Prevention test (Issue #2265):** Verify is_definition() only matches parser-produced variants.
#[test]
fn test_is_definition_only_matches_produced_variants() {
    let not_produced = get_not_produced_kinds();
    let mut problematic = Vec::new();

    for &kind in NodeKind::all_variants() {
        if kind.is_definition() && not_produced.contains(&kind) {
            problematic.push(kind);
        }
    }

    assert!(
        problematic.is_empty(),
        "is_definition() matches NodeKinds that are NOT produced by parser.\n\
         Problematic variants: {:?}\n\
         These predicates reference tree-sitter names instead of parser-produced names.\n\
         Fix by updating is_definition() in predicates.rs to use the parser-produced variant.",
        problematic
    );
}

/// **Prevention test (Issue #2265):** Verify is_literal() only matches parser-produced variants.
#[test]
fn test_is_literal_only_matches_produced_variants() {
    let not_produced = get_not_produced_kinds();
    let mut problematic = Vec::new();

    for &kind in NodeKind::all_variants() {
        if kind.is_literal() && not_produced.contains(&kind) {
            problematic.push(kind);
        }
    }

    assert!(
        problematic.is_empty(),
        "is_literal() matches NodeKinds that are NOT produced by parser.\n\
         Problematic variants: {:?}\n\
         These predicates reference tree-sitter names instead of parser-produced names.\n\
         Fix by updating is_literal() in predicates.rs to use the parser-produced variant.",
        problematic
    );
}

/// **Comprehensive test (Issue #2265):** Verify all predicates are consistent with parser output.
///
/// This is a combined test that checks all predicates at once and provides a summary
/// of any inconsistencies found.
#[test]
fn test_all_predicates_consistent_with_parser_output() {
    let not_produced = get_not_produced_kinds();

    let predicates: [(&str, fn(&NodeKind) -> bool); 4] = [
        ("is_statement", |k| k.is_statement()),
        ("is_expression", |k| k.is_expression()),
        ("is_definition", |k| k.is_definition()),
        ("is_literal", |k| k.is_literal()),
    ];

    let mut all_problems: Vec<(String, Vec<NodeKind>)> = Vec::new();

    for (name, predicate) in predicates {
        let problems: Vec<NodeKind> = NodeKind::all_variants()
            .iter()
            .copied()
            .filter(|kind| predicate(kind) && not_produced.contains(kind))
            .collect();

        if !problems.is_empty() {
            all_problems.push((name.to_string(), problems));
        }
    }

    assert!(
        all_problems.is_empty(),
        "Predicates reference NodeKinds NOT produced by parser (Issue #2265):\n{}",
        all_problems
            .iter()
            .map(|(name, kinds)| format!("  {}(): {:?}", name, kinds))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
