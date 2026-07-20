//! Tests for token module

use super::*;
use logos::Logos;

#[test]
fn test_keywords() {
    let mut lexer = Token::lexer("function if else end");
    assert_eq!(lexer.next(), Some(Ok(Token::KwFunction)));
    assert_eq!(lexer.next(), Some(Ok(Token::KwIf)));
    assert_eq!(lexer.next(), Some(Ok(Token::KwElse)));
    assert_eq!(lexer.next(), Some(Ok(Token::KwEnd)));
}

#[test]
fn test_operators() {
    let mut lexer =
        Token::lexer("+ - * / ^ .+ .* |> .=== .!== .! .~ .<< .<: .>: .∈ .≈ .<<= .>>= .>>>= .÷=");
    assert_eq!(lexer.next(), Some(Ok(Token::Plus)));
    assert_eq!(lexer.next(), Some(Ok(Token::Minus)));
    assert_eq!(lexer.next(), Some(Ok(Token::Star)));
    assert_eq!(lexer.next(), Some(Ok(Token::Slash)));
    assert_eq!(lexer.next(), Some(Ok(Token::Caret)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotPlus)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotStar)));
    assert_eq!(lexer.next(), Some(Ok(Token::PipeRight)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotEqEqEq)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotNotEqEq)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotNot)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotTilde)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotLtLt)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotSubtype)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotSupertype)));
    // `.∈` / `.≈` now carry upstream's comparison class (Issues #11083/#11110)
    assert_eq!(lexer.next(), Some(Ok(Token::DotUnicodeOpComparison)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotUnicodeOpComparison)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotLtLtEq)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotGtGtEq)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotGtGtGtEq)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotDivisionSignEq)));
}

#[test]
fn test_numbers() {
    let mut lexer = Token::lexer("42 3.14 0xff 0b101 1_000_000");
    assert_eq!(lexer.next(), Some(Ok(Token::DecimalLiteral)));
    assert_eq!(lexer.next(), Some(Ok(Token::FloatLiteral)));
    assert_eq!(lexer.next(), Some(Ok(Token::HexLiteral)));
    assert_eq!(lexer.next(), Some(Ok(Token::BinaryLiteral)));
    assert_eq!(lexer.next(), Some(Ok(Token::DecimalLiteral)));
}

#[test]
fn test_identifiers() {
    let mut lexer = Token::lexer("foo bar_baz α β γ where 🏡 d´ tʼ");
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
}

#[test]
fn test_superscript_identifier_suffix() {
    let mut lexer = Token::lexer("dderiv⁻¹ dderiv² dderiv³");
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.slice(), "dderiv⁻¹");
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.slice(), "dderiv²");
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.slice(), "dderiv³");
}

#[test]
fn test_unicode_operators() {
    // Issue #11083: the former catch-all `OtherUnicodeOperator` is now split by
    // UPSTREAM precedence class (`julia/src/julia-parser.scm`), so each of these
    // characters lexes into the class upstream puts it in.
    let mut lexer = Token::lexer("≤ ≥ ≠ ∈ ⊆ √ ⟷ ⟂ ⤈ ⭃ ⭄ ⥺ ⥷ ⌿ ¦ ± · · ⁝ ≲");
    assert_eq!(lexer.next(), Some(Ok(Token::LessEqual)));
    assert_eq!(lexer.next(), Some(Ok(Token::GreaterEqual)));
    assert_eq!(lexer.next(), Some(Ok(Token::NotEqual)));
    assert_eq!(lexer.next(), Some(Ok(Token::ElementOf)));
    assert_eq!(lexer.next(), Some(Ok(Token::SubsetEq)));
    assert_eq!(lexer.next(), Some(Ok(Token::SquareRoot)));
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpArrow))); // ⟷ prec-arrow
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpComparison))); // ⟂ prec-comparison
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpPower))); // ⤈ prec-power
    for _ in 0..4 {
        // ⭃ ⭄ ⥺ ⥷ — prec-arrow
        assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpArrow)));
    }
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpTimes))); // ⌿ prec-times
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpPlus))); // ¦ prec-plus
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpPlus))); // ± prec-plus
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpTimes))); // · U+00B7 prec-times
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpTimes))); // · U+0387 prec-times
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpColon))); // ⁝ prec-colon
    assert_eq!(lexer.next(), Some(Ok(Token::LessSimilar)));
}

/// Table-driven coverage for the upstream-derived operator character set
/// (Issue #11083). Every character upstream Julia lists in a precedence table
/// lexes as an operator of THAT class — previously only an ad-hoc allowlist did,
/// and characters such as `⊛`, `⊞`, `⊠`, `⋆` fell through to `Identifier`, so
/// only their prefix-call spelling parsed. Operator names may also carry
/// upstream's operator suffixes (`jl_op_suffix_char`: primes, sub/superscripts,
/// combining marks) — `⊗ᵢ`. Expectations verified against upstream Julia 1.12.6.
#[test]
fn test_unicode_operator_class_table_issue_11083() {
    // (source, expected token, expected text)
    let cases: &[(&str, Token, &str)] = &[
        ("⊛", Token::UnicodeOpTimes, "⊛"),
        ("⊠", Token::UnicodeOpTimes, "⊠"),
        ("⋆", Token::UnicodeOpTimes, "⋆"),
        ("∗", Token::UnicodeOpTimes, "∗"),
        ("⊞", Token::UnicodeOpPlus, "⊞"),
        ("⊟", Token::UnicodeOpPlus, "⊟"),
        ("∓", Token::UnicodeOpPlus, "∓"),
        ("≺", Token::UnicodeOpComparison, "≺"),
        ("⟹", Token::UnicodeOpArrow, "⟹"),
        ("⇵", Token::UnicodeOpPower, "⇵"),
        ("⋮", Token::UnicodeOpColon, "⋮"),
        // Suffixed operator names keep the BASE operator's class.
        ("⊗ᵢ", Token::UnicodeOpTimes, "⊗ᵢ"),
        ("⊕ₖ", Token::UnicodeOpPlus, "⊕ₖ"),
        ("⊛′", Token::UnicodeOpTimes, "⊛′"),
        ("≤ᵃ", Token::UnicodeOpComparison, "≤ᵃ"),
        // Dotted (broadcast) forms carry the base operator's class (Issue #11110).
        (".⊛", Token::DotUnicodeOpTimes, ".⊛"),
        (".⊗", Token::DotUnicodeOpTimes, ".⊗"),
        (".⊕", Token::DotUnicodeOpPlus, ".⊕"),
        (".⊗ᵢ", Token::DotUnicodeOpTimes, ".⊗ᵢ"),
    ];

    for (source, expected, text) in cases {
        let mut lexer = Token::lexer(source);
        assert_eq!(
            lexer.next(),
            Some(Ok(expected.clone())),
            "source {source:?} must lex as {expected:?}"
        );
        assert_eq!(lexer.slice(), *text, "source {source:?} slice");
        assert_eq!(lexer.next(), None, "source {source:?} is a single token");
        assert!(
            expected.is_operator(),
            "{expected:?} must be operator-classified"
        );
        assert!(
            expected.is_operator_identifier(),
            "{expected:?} must be an ordinary (non-syntactic) operator name"
        );
        assert!(
            expected.binary_precedence().is_some(),
            "{expected:?} must have an infix precedence"
        );
    }

    // Characters that upstream does NOT classify as operators are untouched:
    // `⊝` is "unknown unicode character" upstream, and `∞` / `∇` stay
    // identifiers here as before.
    for (source, expected) in [("∞", Token::Identifier), ("∇", Token::Identifier)] {
        let mut lexer = Token::lexer(source);
        assert_eq!(lexer.next(), Some(Ok(expected)), "source {source:?}");
    }
}

#[test]
fn test_unicode_operator_suffixes() {
    // A suffixed ASCII operator keeps its BASE operator's precedence class
    // (Issue #11083): `+̂` is plus-class, `*̂` times-class, `<̂` comparison-class.
    let mut lexer = Token::lexer("+̂ +̂′ +⁽¹⁾ +₍₀₎");
    for expected in ["+̂", "+̂′", "+⁽¹⁾", "+₍₀₎"] {
        assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpPlus)));
        assert_eq!(lexer.slice(), expected);
    }

    let mut lexer = Token::lexer("*\u{0302} <\u{0302} ^\u{0302}");
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpTimes)));
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpComparison)));
    assert_eq!(lexer.next(), Some(Ok(Token::UnicodeOpPower)));

    // Bare (unsuffixed) ASCII operators keep their dedicated tokens.
    let mut lexer = Token::lexer("+ * < ^");
    assert_eq!(lexer.next(), Some(Ok(Token::Plus)));
    assert_eq!(lexer.next(), Some(Ok(Token::Star)));
    assert_eq!(lexer.next(), Some(Ok(Token::Lt)));
    assert_eq!(lexer.next(), Some(Ok(Token::Caret)));
}

// =============================================================================
// Operator Classification Tests (Issue #1578)
// =============================================================================

fn all_dotted_operators() -> Vec<Token> {
    vec![
        Token::DotPlus,
        Token::DotMinus,
        Token::DotStar,
        Token::DotSlash,
        Token::DotBackslash,
        Token::DotCaret,
        Token::DotPercent,
        Token::DotSubtype,
        Token::DotSupertype,
        Token::DotLt,
        Token::DotGt,
        Token::DotLtEq,
        Token::DotGtEq,
        Token::DotEqEq,
        Token::DotEqEqEq,
        Token::DotNotEq,
        Token::DotNotEqEq,
        Token::DotNot,
        Token::DotTilde,
        Token::DotLtLt,
        Token::DotGtGt,
        Token::DotGtGtGt,
        Token::DotAmp,
        Token::DotPipe,
        Token::DotAndAnd,
        Token::DotOrOr,
    ]
}

fn regular_operators() -> Vec<Token> {
    vec![
        Token::Plus,
        Token::Minus,
        Token::Star,
        Token::Slash,
        Token::Percent,
        Token::Caret,
        Token::Amp,
        Token::Pipe,
        Token::Backslash,
        Token::Lt,
        Token::Gt,
        Token::LtEq,
        Token::GtEq,
        Token::EqEq,
        Token::EqEqEq,
        Token::NotEq,
        Token::NotEqEq,
    ]
}

#[test]
fn test_dotted_operators_are_subset_of_operators() {
    for token in all_dotted_operators() {
        assert!(token.is_operator(), "{:?} should be an operator", token);
        assert!(token.is_dotted_operator(), "{:?} should be dotted", token);
    }
}

#[test]
fn test_regular_operators_are_not_dotted() {
    for token in regular_operators() {
        assert!(token.is_operator(), "{:?} should be an operator", token);
        assert!(
            !token.is_dotted_operator(),
            "{:?} should NOT be dotted",
            token
        );
    }
}

#[test]
fn test_arrow_is_not_an_operator_identifier_issue_10917() {
    assert!(Token::Arrow.is_operator());
    assert!(!Token::Arrow.is_operator_identifier());
    assert!(Token::Arrow.is_quoted_operator_symbol());

    assert!(Token::Plus.is_operator_identifier());
    assert!(Token::FatArrow.is_operator_identifier());
}

/// Mutation contract for the syntactic-operator role split (Issues #10917,
/// #10932, #10940): the set of operator tokens that are NOT operator
/// identifiers must be exactly upstream's operator-lexed `syntactic-operators`.
/// Widening any of these back to an identifier (or silently adding a new
/// syntactic token without a role decision) makes this test red.
#[test]
fn test_syntactic_operator_role_split_is_exhaustive_issue_10940() {
    let syntactic = [
        Token::Arrow,     // ->  (Issue #10917)
        Token::AndAnd,    // &&  (Issue #10932)
        Token::OrOr,      // ||  (Issue #10932)
        Token::DotAndAnd, // .&& (Issue #10932)
        Token::DotOrOr,   // .|| (Issue #10932)
    ];
    for token in &syntactic {
        assert!(token.is_operator(), "{token:?} stays an operator token");
        assert!(
            token.is_syntactic_operator(),
            "{token:?} must be classified syntactic"
        );
        assert!(
            !token.is_operator_identifier(),
            "{token:?} must not be an unquoted operator identifier"
        );
        assert!(
            token.is_quoted_operator_symbol(),
            "{token:?} must remain quotable (`:(&&)`, `:->`)"
        );
    }

    // Positive controls: ordinary operators keep full identifier roles.
    for token in [
        Token::Plus,
        Token::Amp,
        Token::Pipe,
        Token::FatArrow,
        Token::PipeRight,
        Token::DotDot,
        Token::EqEq,
    ] {
        assert!(token.is_operator_identifier(), "{token:?} stays a value");
        assert!(!token.is_syntactic_operator());
    }

    // Upstream syntactic operators that are NOT operator tokens in this
    // lexer (assignments, `.`, `...`, `:=`, `$=`): they must stay outside
    // `is_operator()` so the identifier/value paths never see them.
    for token in [
        Token::Eq,
        Token::PlusEq,
        Token::ColonEq,
        Token::DollarEq,
        Token::Dot,
        Token::Ellipsis,
        Token::DotPlusEq,
    ] {
        assert!(
            !token.is_operator(),
            "{token:?} must not be operator-classified"
        );
        assert!(!token.is_operator_identifier());
    }
}

#[test]
fn test_dotted_operator_has_base() {
    for token in all_dotted_operators() {
        assert!(
            token.dotted_operator_base().is_some(),
            "{:?} should have base",
            token
        );
    }
}

#[test]
fn test_dotted_operator_base_mapping() {
    assert_eq!(Token::DotPlus.dotted_operator_base(), Some("+"));
    assert_eq!(Token::DotMinus.dotted_operator_base(), Some("-"));
    assert_eq!(Token::DotStar.dotted_operator_base(), Some("*"));
    assert_eq!(Token::DotSlash.dotted_operator_base(), Some("/"));
    assert_eq!(Token::DotCaret.dotted_operator_base(), Some("^"));
    assert_eq!(Token::DotSubtype.dotted_operator_base(), Some("<:"));
    assert_eq!(Token::DotSupertype.dotted_operator_base(), Some(">:"));
    assert_eq!(Token::DotEqEqEq.dotted_operator_base(), Some("==="));
    assert_eq!(Token::DotNotEqEq.dotted_operator_base(), Some("!=="));
    assert_eq!(Token::DotNot.dotted_operator_base(), Some("!"));
    assert_eq!(Token::DotTilde.dotted_operator_base(), Some("~"));
    assert_eq!(Token::DotLtLt.dotted_operator_base(), Some("<<"));
    assert_eq!(Token::DotGtGt.dotted_operator_base(), Some(">>"));
    assert_eq!(Token::DotGtGtGt.dotted_operator_base(), Some(">>>"));
}

#[test]
fn test_non_dotted_operator_has_no_base() {
    for token in regular_operators() {
        assert!(
            token.dotted_operator_base().is_none(),
            "{:?} should NOT have base",
            token
        );
    }
}
