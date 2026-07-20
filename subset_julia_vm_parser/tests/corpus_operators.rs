//! Tests migrated from tree-sitter-julia/test/corpus/operators.txt

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

// =============================================================================
// Assignment Operators
// =============================================================================

// Assignment is parsed as Assignment node (not BinaryExpression)
#[test]
fn test_assignment_simple() {
    assert_root_child_kind("x = 1", NodeKind::Assignment);
    assert_root_child_kind("x = y = z", NodeKind::Assignment);
}

#[test]
fn test_assignment_to_bare_operator_before_separator_issue_6394() {
    let source = "f = +; println(f(1, 2))";
    let cst = parse(source).expect("parse semicolon-separated assignment to bare operator");
    assert_eq!(cst.children.len(), 2);
    let assignment = &cst.children[0];
    assert_eq!(assignment.kind, NodeKind::Assignment);
    assert_eq!(assignment.children[2].kind, NodeKind::Operator);
    assert_eq!(assignment.children[2].text_from_source(source), "+");

    let source = "f = +\nprintln(f(1, 2))";
    let cst = parse(source).expect("parse newline-separated assignment to bare operator");
    assert_eq!(cst.children.len(), 2);
    let assignment = &cst.children[0];
    assert_eq!(assignment.kind, NodeKind::Assignment);
    assert_eq!(assignment.children[2].kind, NodeKind::Operator);
    assert_eq!(assignment.children[2].text_from_source(source), "+");

    let cst = parse("x = + 1").expect("parse unary plus expression");
    let assignment = &cst.children[0];
    assert_eq!(assignment.kind, NodeKind::Assignment);
    assert_eq!(assignment.children[2].kind, NodeKind::UnaryExpression);
}

// Compound assignment is parsed as CompoundAssignmentExpression
#[test]
fn test_assignment_compound() {
    assert_root_child_kind("x += 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x -= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x *= 2", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x /= 2", NodeKind::CompoundAssignmentExpression);
}

#[test]
fn test_assignment_compound_more() {
    assert_root_child_kind("x ^= 2", NodeKind::CompoundAssignmentExpression);
    assert_parses("x ÷= 2"); // Unicode operator
    assert_root_child_kind("x %= 3", NodeKind::CompoundAssignmentExpression);
}

#[test]
fn test_assignment_bitwise() {
    assert_root_child_kind("x |= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x &= 1", NodeKind::CompoundAssignmentExpression);
    assert_parses("x ⊻= 1"); // Unicode operator
}

#[test]
fn test_assignment_shift() {
    assert_root_child_kind("x <<= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x >>= 1", NodeKind::CompoundAssignmentExpression);
    assert_root_child_kind("x >>>= 1", NodeKind::CompoundAssignmentExpression);
}

// =============================================================================
// Binary Arithmetic Operators
// =============================================================================

#[test]
fn test_binary_addition() {
    assert_root_child_kind("a + b", NodeKind::BinaryExpression);
    assert_root_child_kind("a - b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_multiplication() {
    assert_root_child_kind("a * b", NodeKind::BinaryExpression);
    assert_root_child_kind("a / b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ÷ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a % b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_power() {
    assert_root_child_kind("a ^ b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_rational() {
    assert_root_child_kind("a // b", NodeKind::BinaryExpression);
}

#[test]
fn test_binary_matrix() {
    assert_root_child_kind("a \\ b", NodeKind::BinaryExpression);
}

// =============================================================================
// Comparison Operators
// =============================================================================

#[test]
fn test_comparison_basic() {
    assert_root_child_kind("a < b", NodeKind::BinaryExpression);
    assert_root_child_kind("a > b", NodeKind::BinaryExpression);
    assert_root_child_kind("a <= b", NodeKind::BinaryExpression);
    assert_root_child_kind("a >= b", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_equality() {
    assert_root_child_kind("a == b", NodeKind::BinaryExpression);
    assert_root_child_kind("a != b", NodeKind::BinaryExpression);
    assert_root_child_kind("a === b", NodeKind::BinaryExpression);
    assert_root_child_kind("a !== b", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_unicode() {
    assert_root_child_kind("a ≤ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ≥ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ≠ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ≲ b", NodeKind::BinaryExpression);
}

#[test]
fn test_extended_unicode_operators_issue_8751() {
    assert_parses("⟷(a, b) = a === b");
    assert_parses("⊊(a, b) = a ⊆ b");
    assert_root_child_kind("a ⟷ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ± b", NodeKind::BinaryExpression);
    assert_root_child_kind("a +̂ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a +̂′ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a +⁽¹⁾ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a +₍₀₎ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ⁝ b", NodeKind::BinaryExpression);
    assert_parses("(·)");
    assert_parses("(·)");
}

#[test]
fn test_dotted_extended_operators_issue_8759() {
    assert_root_child_kind("a .=== b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .!== b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .∈ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .≈ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .<< b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .>>> b", NodeKind::BinaryExpression);
}

#[test]
fn test_adjacent_composition_operator_issue_8759() {
    assert_root_child_kind("!isempty∘last", NodeKind::BinaryExpression);
    assert_root_child_kind("textwidth∘last", NodeKind::BinaryExpression);
    assert_parses("(@inferred (g∘g)(1)) == 1");
}

#[test]
fn test_comparison_chained() {
    // Chained comparisons: a < b < c
    assert_parses("a < b < c");
    assert_parses("1 <= x <= 10");
}

#[test]
fn test_comparison_in() {
    assert_root_child_kind("a in b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ∈ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ∉ b", NodeKind::BinaryExpression);
    assert_root_child_kind("in(a, b)", NodeKind::CallExpression);
    assert_root_child_kind("∈(a, b)", NodeKind::CallExpression);
    assert_root_child_kind("∉(a, b)", NodeKind::CallExpression);
    assert_root_child_kind("∋(b, a)", NodeKind::CallExpression);
    assert_root_child_kind("∌(b, a)", NodeKind::CallExpression);
    assert_root_child_kind("Base.:∈(a, b)", NodeKind::CallExpression);
    assert_root_child_kind("Base.:(∈)(a, b)", NodeKind::CallExpression);
}

#[test]
fn test_comparison_isa() {
    assert_root_child_kind("a isa T", NodeKind::BinaryExpression);
}

#[test]
fn test_comparison_subtype() {
    assert_root_child_kind("A <: B", NodeKind::BinaryExpression);
    assert_root_child_kind("A >: B", NodeKind::BinaryExpression);
}

// =============================================================================
// Logical Operators
// =============================================================================

#[test]
fn test_logical_and() {
    assert_root_child_kind("a && b", NodeKind::BinaryExpression);
}

#[test]
fn test_logical_or() {
    assert_root_child_kind("a || b", NodeKind::BinaryExpression);
}

#[test]
fn test_logical_not() {
    assert_root_child_kind("!a", NodeKind::UnaryExpression);
}

// =============================================================================
// Bitwise Operators
// =============================================================================

#[test]
fn test_bitwise_and() {
    assert_root_child_kind("a & b", NodeKind::BinaryExpression);
}

#[test]
fn test_bitwise_or() {
    assert_root_child_kind("a | b", NodeKind::BinaryExpression);
}

// Unicode xor operator
#[test]
fn test_bitwise_xor() {
    assert_parses("a ⊻ b"); // Unicode xor
    assert_parses("xor(a, b)");
}

#[test]
fn test_bitwise_not() {
    assert_root_child_kind("~a", NodeKind::UnaryExpression);
}

#[test]
fn test_bitwise_shift() {
    assert_root_child_kind("a << b", NodeKind::BinaryExpression);
    assert_root_child_kind("a >> b", NodeKind::BinaryExpression);
    assert_root_child_kind("a >>> b", NodeKind::BinaryExpression);
}

// =============================================================================
// Unary Operators
// =============================================================================

#[test]
fn test_unary_plus_minus() {
    assert_root_child_kind("-x", NodeKind::UnaryExpression);
    assert_root_child_kind("+x", NodeKind::UnaryExpression);
}

#[test]
fn test_unary_not() {
    assert_root_child_kind("!x", NodeKind::UnaryExpression);
}

#[test]
fn test_unary_dotted_not_broadcast() {
    assert_root_child_kind(".!x", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".!Bool[true, false]", NodeKind::BroadcastCallExpression);
}

#[test]
fn test_unary_sqrt() {
    assert_root_child_kind("√x", NodeKind::UnaryExpression);
    assert_root_child_kind("∛x", NodeKind::UnaryExpression);
    assert_root_child_kind("∜x", NodeKind::UnaryExpression);
}

// =============================================================================
// Broadcasting Operators
// =============================================================================

#[test]
fn test_broadcast_binary() {
    assert_root_child_kind("a .+ b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .- b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .* b", NodeKind::BinaryExpression);
    assert_root_child_kind("a ./ b", NodeKind::BinaryExpression);
}

#[test]
fn test_broadcast_comparison() {
    assert_root_child_kind("a .< b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .> b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .== b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .<= b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .>= b", NodeKind::BinaryExpression);
    assert_root_child_kind("a .!= b", NodeKind::BinaryExpression);
}

#[test]
fn test_broadcast_power() {
    assert_root_child_kind("a .^ b", NodeKind::BinaryExpression);
}

// =============================================================================
// Pipe Operators
// =============================================================================

#[test]
fn test_pipe_right() {
    assert_root_child_kind("a |> f", NodeKind::BinaryExpression);
    assert_root_child_kind("x |> f |> g", NodeKind::BinaryExpression);
}

#[test]
fn test_pipe_left() {
    assert_root_child_kind("f <| a", NodeKind::BinaryExpression);
}

// =============================================================================
// Ternary Operator
// =============================================================================

#[test]
fn test_ternary() {
    assert_root_child_kind("a ? b : c", NodeKind::TernaryExpression);
}

// Nested ternary
#[test]
fn test_ternary_nested() {
    assert_parses("a ? b ? c : d : e");
    assert_parses("a ? b : c ? d : e");
}

// =============================================================================
// Operator Precedence
// =============================================================================

#[test]
fn test_precedence_arithmetic() {
    // * binds tighter than +
    assert_parses("a + b * c");
    assert_parses("a * b + c");
}

#[test]
fn test_precedence_power() {
    // ^ binds tighter than *
    assert_parses("a * b ^ c");
    assert_parses("a ^ b * c");
}

// `^`/`.^` bind TIGHTER than a prefix unary operator: Julia parses `-x^2` as
// `-(x^2)`, not `(-x)^2` (julia/src/julia-parser.scm `parse-unary`:
// "-2^3 is parsed as -(2^3)"). The unary must wrap the whole power expression.
// Issue #7232.
#[test]
fn test_unary_minus_binds_looser_than_power() {
    for src in ["-x^2", "-2^2", "-3.0^2", "-x .^ 2", "!x^2", "~x^2"] {
        let cst = parse(src).unwrap_or_else(|_| panic!("Failed to parse: {}", src));
        let root_child = &cst.children[0];
        assert_eq!(
            root_child.kind,
            NodeKind::UnaryExpression,
            "`{}` should parse as a UnaryExpression wrapping the power, got {:?}",
            src,
            root_child.kind
        );
        // The unary's operand (second child) must be the power BinaryExpression.
        assert_eq!(
            root_child.children[1].kind,
            NodeKind::BinaryExpression,
            "`{}` unary operand should be the power BinaryExpression, got {:?}",
            src,
            root_child.children[1].kind
        );
    }
}

// The RHS of `^` keeps its own unary sign: `2^-3` is `2^(-3)` (left side is a
// plain operand, unchanged by Issue #7232). `-x^-2` is `-(x^(-2))`.
#[test]
fn test_power_rhs_keeps_unary_sign() {
    let cst = parse("2^-3").expect("parse 2^-3");
    let root = &cst.children[0];
    assert_eq!(root.kind, NodeKind::BinaryExpression);
    assert_eq!(
        root.children[2].kind,
        NodeKind::UnaryExpression,
        "RHS of `2^-3` should stay a UnaryExpression"
    );

    let cst = parse("-x^-2").expect("parse -x^-2");
    let root = &cst.children[0];
    assert_eq!(root.kind, NodeKind::UnaryExpression, "-x^-2 == -(x^(-2))");
    assert_eq!(root.children[1].kind, NodeKind::BinaryExpression);
}

#[test]
fn test_precedence_comparison() {
    // + binds tighter than <
    assert_parses("a + b < c + d");
}

#[test]
fn test_precedence_logical() {
    // && binds tighter than ||
    assert_parses("a || b && c");
    assert_parses("a && b || c");
}

// =============================================================================
// Operator Associativity
// =============================================================================

#[test]
fn test_associativity_left() {
    // Left associative: +, -, *, /
    assert_parses("a + b + c");
    assert_parses("a - b - c");
    assert_parses("a * b * c");
    assert_parses("a / b / c");
}

#[test]
fn test_associativity_right() {
    // Right associative: ^, =
    assert_parses("a ^ b ^ c");
    assert_parses("a = b = c");
}

// =============================================================================
// Operators as Values
// =============================================================================

#[test]
fn test_operator_as_value() {
    assert_root_child_kind("(+)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(-)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(*)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(/)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(√)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(∛)", NodeKind::ParenthesizedExpression);
    assert_root_child_kind("(∪)", NodeKind::ParenthesizedExpression);
}

#[test]
fn test_bare_arrow_is_not_an_unquoted_operator_identifier_issue_10917() {
    for (source, expected_span, expected_message) in [
        ("->", 0..2, "invalid identifier"),
        ("(->)", 1..3, "invalid identifier"),
        ("f = ->", 4..6, "invalid identifier"),
        ("[->]", 1..3, "invalid identifier"),
        ("map(->, xs)", 4..6, "invalid identifier"),
        ("->{T}", 0..2, "invalid identifier"),
        ("->(x)", 0..2, "invalid identifier"),
        ("- ->", 2..4, "invalid identifier"),
        ("x -> ->", 5..7, "invalid identifier"),
        ("->\nx", 0..2, "invalid identifier"),
        ("->(x) = x", 0..2, "invalid identifier"),
        ("function ->(x)\nend", 9..11, "invalid identifier"),
        ("const -> = f", 6..8, "invalid identifier"),
        ("import Base: ->", 13..15, "invalid identifier"),
        ("import Base: (->)", 13..17, "expected identifier"),
        ("using Base: (->)", 12..16, "expected identifier"),
        ("export (->)", 7..11, "expected identifier"),
    ] {
        let error = parse(source).expect_err("bare `->` must not be a first-class value");
        let span = error
            .span()
            .expect("syntax error must carry the arrow span");
        assert_eq!(span.start..span.end, expected_span, "source: {source:?}");
        if source == "->" {
            assert_eq!(
                (
                    span.start_line,
                    span.start_column,
                    span.end_line,
                    span.end_column,
                ),
                (1, 1, 1, 3)
            );
        }
        assert!(
            matches!(
                &error,
                ParseError::InvalidSyntax { message, .. } if message == expected_message
            ),
            "source: {source:?}, error: {error:?}"
        );
        assert!(!error.is_incomplete_input(), "source: {source:?}");
    }

    // Quoting an operator-like symbol is a separate grammar path and remains valid.
    assert_parses(":->");
    assert_parses(":(->)");
    assert_parses("Base.:->");
    assert_parses("Base.:(->)");
    assert_parses("function Base.:(->)(x)\n    x\nend");

    for source in [
        "x -> x + 1",
        "(x, y) -> x + y",
        "() -> 1",
        "a |> x -> b",
        "+",
        "(==)",
        "map(|>, xs)",
        "import Base: (:)",
        "..",
        "=>",
        "-->",
        "(-->)",
    ] {
        assert_parses(source);
    }
}

// =============================================================================
// Syntactic-operator role inventory (Issues #10932 / #10940)
//
// Derived from upstream `julia/src/julia-parser.scm`:
//   (define syntactic-operators
//     (append! (add-dots '(&& || = += -= *= /= //= \= ^= ÷= %= <<= >>= >>>= |= &= ⊻=))
//              '(:= $= |.| ... ->)))
//
// In this lexer the assignment members, `:=`, `$=`, `.`, and `...` are not
// operator-classified tokens at all (see the token-level contract
// `test_syntactic_operator_role_split_is_exhaustive_issue_10940`), so the
// operator-token members are exactly `->`, `&&`, `||`, `.&&`, `.||`.
// Every unquoted-name shortcut routes through the single authority
// `Token::is_operator_identifier` / `reject_invalid_operator_identifier`;
// this inventory pins the roles per token:
//   (a) infix/prefix grammar participation
//   (b) unquoted operator identifier/value            -> rejected
//   (c) operator method-name definition               -> rejected
//   (d) import/export names incl. parenthesized forms -> rejected
//   (e) quoted symbol forms                           -> accepted
// =============================================================================

/// One syntactic operator's full role classification. All expectations were
/// verified against upstream Julia 1.12.6 (`Meta.parse`).
struct SyntacticOperatorRoles {
    /// Operator spelling, for constructing sources and error messages.
    op: &'static str,
    /// (a) valid grammar participation — must keep parsing.
    grammar_ok: &'static [&'static str],
    /// (b)-(d) unquoted identifier/value forms — must reject with
    /// `invalid identifier` at exactly the operator's span.
    invalid_identifier: &'static [(&'static str, std::ops::Range<usize>)],
    /// (d) parenthesized import/export forms — must reject with
    /// `expected identifier` spanning the parenthesized name.
    expected_identifier: &'static [(&'static str, std::ops::Range<usize>)],
    /// (e) quoted symbol forms — must keep parsing.
    quoted_ok: &'static [&'static str],
}

const SYNTACTIC_OPERATOR_ROLES: &[SyntacticOperatorRoles] = &[
    SyntacticOperatorRoles {
        op: "&&",
        grammar_ok: &[
            "a && b",
            "a && b && c",
            "x > 0 && return 1",
            "if a && b\nend",
            "a && b || c",
        ],
        invalid_identifier: &[
            ("&&", 0..2),
            ("(&&)", 1..3),
            ("f = &&", 4..6),
            ("[&&]", 1..3),
            ("map(&&, xs)", 4..6),
            ("&&{T}", 0..2),
            ("&&(x, y) = 1", 0..2),
            ("function &&(x)\nend", 9..11),
            ("const && = f", 6..8),
            ("quote && end", 6..8),
            ("import Base: &&", 13..15),
            ("export &&", 7..9),
        ],
        expected_identifier: &[
            ("import Base: (&&)", 13..17),
            ("using Base: (&&)", 12..16),
            ("export (&&)", 7..11),
        ],
        quoted_ok: &[
            ":&&",
            ":(&&)",
            "Base.:&&",
            "Base.:(&&)",
            "import Base.:&&",
            "import Base.:(&&)",
            "function Base.:(&&)(x)\n    x\nend",
            "Expr(:&&, a, b)",
        ],
    },
    SyntacticOperatorRoles {
        op: "||",
        grammar_ok: &[
            "a || b",
            "a || b || c",
            "x < 0 || return 1",
            "if a || b\nend",
        ],
        invalid_identifier: &[
            ("||", 0..2),
            ("(||)", 1..3),
            ("f = ||", 4..6),
            ("[||]", 1..3),
            ("map(||, xs)", 4..6),
            ("||{T}", 0..2),
            ("||(x, y) = 1", 0..2),
            ("function ||(x)\nend", 9..11),
            ("const || = f", 6..8),
            ("import Base: ||", 13..15),
        ],
        expected_identifier: &[("import Base: (||)", 13..17), ("export (||)", 7..11)],
        quoted_ok: &[":||", ":(||)", "Base.:||", "Base.:(||)", "Expr(:||, a, b)"],
    },
    SyntacticOperatorRoles {
        op: ".&&",
        grammar_ok: &["a .&& b", "xs .&& ys .&& zs"],
        invalid_identifier: &[
            (".&&", 0..3),
            ("(.&&)", 1..4),
            ("f = .&&", 4..7),
            (".&&(a, b)", 0..3),
            ("import Base: .&&", 13..16),
        ],
        expected_identifier: &[("import Base: (.&&)", 13..18)],
        quoted_ok: &[":(.&&)"],
    },
    SyntacticOperatorRoles {
        op: ".||",
        grammar_ok: &["a .|| b", "xs .|| ys"],
        invalid_identifier: &[
            (".||", 0..3),
            ("(.||)", 1..4),
            ("f = .||", 4..7),
            (".||(a, b)", 0..3),
            ("import Base: .||", 13..16),
        ],
        expected_identifier: &[("import Base: (.||)", 13..18)],
        quoted_ok: &[":(.||)"],
    },
    SyntacticOperatorRoles {
        op: "->",
        grammar_ok: &["x -> x + 1", "(x, y) -> x + y", "() -> 1", "a |> x -> b"],
        invalid_identifier: &[
            ("->", 0..2),
            ("(->)", 1..3),
            ("f = ->", 4..6),
            ("[->]", 1..3),
            ("map(->, xs)", 4..6),
            ("->{T}", 0..2),
            ("->(x) = x", 0..2),
            ("function ->(x)\nend", 9..11),
            ("const -> = f", 6..8),
            ("import Base: ->", 13..15),
        ],
        expected_identifier: &[("import Base: (->)", 13..17), ("export (->)", 7..11)],
        quoted_ok: &[
            ":->",
            ":(->)",
            "Base.:->",
            "Base.:(->)",
            "import Base.:->",
            "import Base.:(->)",
            "function Base.:(->)(x)\n    x\nend",
        ],
    },
];

#[test]
fn test_syntactic_operator_role_inventory_issue_10940() {
    for roles in SYNTACTIC_OPERATOR_ROLES {
        for source in roles.grammar_ok {
            let result = parse(source);
            assert!(
                result.is_ok(),
                "[{}] grammar participation must keep parsing: {source:?}\nError: {:?}",
                roles.op,
                result.err()
            );
        }

        for (source, expected_span) in roles.invalid_identifier {
            let error = parse(source).expect_err(&format!(
                "[{}] unquoted operator-identifier form must be rejected: {source:?}",
                roles.op
            ));
            let span = error
                .span()
                .unwrap_or_else(|| panic!("[{}] error must carry a span: {source:?}", roles.op));
            assert_eq!(
                span.start..span.end,
                *expected_span,
                "[{}] source: {source:?}",
                roles.op
            );
            assert!(
                matches!(
                    &error,
                    ParseError::InvalidSyntax { message, .. } if message == "invalid identifier"
                ),
                "[{}] source: {source:?}, error: {error:?}",
                roles.op
            );
            assert!(
                !error.is_incomplete_input(),
                "[{}] source: {source:?}",
                roles.op
            );
        }

        for (source, expected_span) in roles.expected_identifier {
            let error = parse(source).expect_err(&format!(
                "[{}] parenthesized import/export form must be rejected: {source:?}",
                roles.op
            ));
            let span = error
                .span()
                .unwrap_or_else(|| panic!("[{}] error must carry a span: {source:?}", roles.op));
            assert_eq!(
                span.start..span.end,
                *expected_span,
                "[{}] source: {source:?}",
                roles.op
            );
            assert!(
                matches!(
                    &error,
                    ParseError::InvalidSyntax { message, .. } if message == "expected identifier"
                ),
                "[{}] source: {source:?}, error: {error:?}",
                roles.op
            );
        }

        for source in roles.quoted_ok {
            let result = parse(source);
            assert!(
                result.is_ok(),
                "[{}] quoted symbol form must keep parsing: {source:?}\nError: {:?}",
                roles.op,
                result.err()
            );
        }
    }
}

/// Non-operator-lexed syntactic operators (`=`-family, `:=`, `$=`, `.`,
/// `...`): quoted forms stay valid, unquoted parenthesized value forms are
/// rejected (upstream: `invalid identifier` / `unexpected =`). Their tokens
/// never enter `is_operator()` (see the token contract test), so only the
/// parse-level expectations are pinned here (Issue #10940).
#[test]
fn test_non_operator_syntactic_tokens_roles_issue_10940() {
    for source in [
        ":(=)",
        ":(+=)",
        ":(-=)",
        ":(*=)",
        ":(/=)",
        ":(//=)",
        ":(^=)",
        ":(÷=)",
        ":(%=)",
        ":(<<=)",
        ":(>>=)",
        ":(>>>=)",
        ":(|=)",
        ":(&=)",
        ":(⊻=)",
        ":(:=)",
        ":($=)",
        ":(.)",
        ":(...)",
        ":...",
        ":.",
        ":(.+=)",
        "x = 1",
        "x += 1",
        "x .+= 1",
        "a.b",
        "f(xs...)",
        "import Base: (:)",
    ] {
        let result = parse(source);
        assert!(
            result.is_ok(),
            "quoted/grammar form must keep parsing: {source:?}\nError: {:?}",
            result.err()
        );
    }

    // Unquoted value forms rejected by upstream. The error class is a
    // ParseError; exact message parity for the assignment family is not
    // required (upstream mixes "invalid identifier" and "unexpected `=`").
    for source in ["(=)", "(+=)", "(:=)", "(...)", "f = +=", "x = (=)"] {
        assert!(
            parse(source).is_err(),
            "unquoted syntactic-token value form must be rejected: {source:?}"
        );
    }
}

#[test]
fn test_const_operator_aliases_issue_8756() {
    assert_root_child_kind("const (√) = sqrt", NodeKind::ConstDeclaration);
    assert_root_child_kind("const (∛) = cbrt", NodeKind::ConstDeclaration);
    assert_root_child_kind("const ∪ = union", NodeKind::ConstDeclaration);
}

#[test]
fn test_operator_in_call() {
    assert_parses("map(+, a, b)");
    assert_parses("reduce(*, xs)");
}

// =============================================================================
// Quote Expressions (Symbol Operators)
// =============================================================================

#[test]
fn test_quote_expression_simple() {
    // :symbol syntax
    assert_root_child_kind(":foo", NodeKind::QuoteExpression);
    assert_root_child_kind(":bar", NodeKind::QuoteExpression);
}

#[test]
fn test_quote_expression_operator() {
    // :(operator) syntax - quoting operators as symbols
    assert_root_child_kind(":(+)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(++)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(-)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(*)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(==)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(<=)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(.)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(:)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(..)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(…)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(÷=)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(⊻=)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(≔)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(⩴)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(≕)", NodeKind::QuoteExpression);
    assert_root_child_kind(":()", NodeKind::QuoteExpression);
    assert_root_child_kind(":(.÷=)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(.>>>=)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(.>>=)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(.<<=)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(;)", NodeKind::QuoteExpression);
    assert_root_child_kind(":((a,;b))", NodeKind::QuoteExpression);
    assert_root_child_kind(":(g(a,; b))", NodeKind::QuoteExpression);
    assert_root_child_kind(":(1 ≔ 2)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(1 ⩴ 2)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(1 ≕ 2)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(∓ 1)", NodeKind::QuoteExpression);
    assert_root_child_kind(":(± 1)", NodeKind::QuoteExpression);
}

#[test]
fn test_quote_expression_in_context() {
    // Using quoted operators in expressions
    assert_parses("f(:(+))");
    assert_parses("Dict(:add => (+), :mul => (*))");
    assert_parses("getfield(Base, :(+))");
    assert_parses("if isexpr(f, :(.))\nend");
    assert_parses(":maximum!_fast");
    assert_parses("Dict(:+= => :+)");
    assert_parses("s == :?");
    assert_parses("s in (:+, :++, :*)");
    assert_parses("Expr(:tuple,:())");
}

#[test]
fn test_broadcast_dotop_as_function() {
    // Broadcast operators used as functions: .+(a, b) is equivalent to (+).(a, b)
    assert_parses(".+([1, 2, 3])");
    assert_parses(".-([1, 2, 3])");
    assert_parses(".*([1, 2], [3, 4])");
    assert_parses(".+(x)");
    assert_parses(".-(x, y)");
}

// =============================================================================
// Keyword Symbols
// =============================================================================

#[test]
fn test_keyword_symbols() {
    // Keyword symbols: :if, :for, :quote, :end, etc.
    assert_root_child_kind(":if", NodeKind::QuoteExpression);
    assert_root_child_kind(":for", NodeKind::QuoteExpression);
    assert_root_child_kind(":while", NodeKind::QuoteExpression);
    assert_root_child_kind(":end", NodeKind::QuoteExpression);
    assert_root_child_kind(":quote", NodeKind::QuoteExpression);
    assert_root_child_kind(":begin", NodeKind::QuoteExpression);
    assert_root_child_kind(":let", NodeKind::QuoteExpression);
    assert_root_child_kind(":function", NodeKind::QuoteExpression);
    assert_root_child_kind(":macro", NodeKind::QuoteExpression);
    assert_root_child_kind(":return", NodeKind::QuoteExpression);
    assert_root_child_kind(":break", NodeKind::QuoteExpression);
    assert_root_child_kind(":continue", NodeKind::QuoteExpression);
    assert_root_child_kind(":try", NodeKind::QuoteExpression);
    assert_root_child_kind(":catch", NodeKind::QuoteExpression);
    assert_root_child_kind(":finally", NodeKind::QuoteExpression);
    assert_root_child_kind(":else", NodeKind::QuoteExpression);
    assert_root_child_kind(":elseif", NodeKind::QuoteExpression);
    assert_root_child_kind(":module", NodeKind::QuoteExpression);
    assert_root_child_kind(":struct", NodeKind::QuoteExpression);
    assert_root_child_kind(":mutable", NodeKind::QuoteExpression);
    assert_root_child_kind(":abstract", NodeKind::QuoteExpression);
    assert_root_child_kind(":primitive", NodeKind::QuoteExpression);
    assert_root_child_kind(":type", NodeKind::QuoteExpression);
    assert_root_child_kind(":const", NodeKind::QuoteExpression);
    assert_root_child_kind(":global", NodeKind::QuoteExpression);
    assert_root_child_kind(":local", NodeKind::QuoteExpression);
    assert_root_child_kind(":using", NodeKind::QuoteExpression);
    assert_root_child_kind(":import", NodeKind::QuoteExpression);
    assert_root_child_kind(":export", NodeKind::QuoteExpression);
    assert_root_child_kind(":in", NodeKind::QuoteExpression);
    assert_root_child_kind(":isa", NodeKind::QuoteExpression);
    assert_root_child_kind(":where", NodeKind::QuoteExpression);
    assert_root_child_kind(":do", NodeKind::QuoteExpression);
    assert_root_child_kind(":true", NodeKind::QuoteExpression);
    assert_root_child_kind(":false", NodeKind::QuoteExpression);
}

#[test]
fn test_keyword_symbols_in_context() {
    // Using keyword symbols in expressions
    assert_parses("Meta.isexpr(ex, :if)");
    assert_parses("Meta.isexpr(ex, :for)");
    assert_parses("ex.head == :call");
    assert_parses("ex.head == :quote");
    assert_parses("Dict(:if => 1, :for => 2)");
    assert_parses("[:if, :for, :while, :end]");
}

// =============================================================================
// Parser Dispatch Tests for Operator Classification (Issue #1578)
// =============================================================================

#[test]
fn test_dotted_operator_not_parsed_as_method_definition() {
    // Issue #1574/#1578: .+(x) should be BroadcastCallExpression, NOT ShortFunctionDefinition
    assert_root_child_kind(".+(x)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".-(x)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".*(x, y)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind("./(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".^(x, 2)", NodeKind::BroadcastCallExpression);
}

#[test]
fn test_regular_operator_method_definition() {
    // Regular operators CAN be used in method definitions
    assert_root_child_kind("+(x, y) = x + y", NodeKind::ShortFunctionDefinition);
    assert_root_child_kind("-(x, y) = x - y", NodeKind::ShortFunctionDefinition);
    assert_root_child_kind("*(x, y) = x * y", NodeKind::ShortFunctionDefinition);
}

#[test]
fn test_comparison_dotted_operators_as_calls() {
    // Dotted comparison operators as broadcast call expressions
    assert_root_child_kind(".<(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".>(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".<=(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".>=(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".==(a, b)", NodeKind::BroadcastCallExpression);
    assert_root_child_kind(".!=(a, b)", NodeKind::BroadcastCallExpression);
}
