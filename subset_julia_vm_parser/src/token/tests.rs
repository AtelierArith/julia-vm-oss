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
    let mut lexer = Token::lexer("+ - * / ^ .+ .* |>");
    assert_eq!(lexer.next(), Some(Ok(Token::Plus)));
    assert_eq!(lexer.next(), Some(Ok(Token::Minus)));
    assert_eq!(lexer.next(), Some(Ok(Token::Star)));
    assert_eq!(lexer.next(), Some(Ok(Token::Slash)));
    assert_eq!(lexer.next(), Some(Ok(Token::Caret)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotPlus)));
    assert_eq!(lexer.next(), Some(Ok(Token::DotStar)));
    assert_eq!(lexer.next(), Some(Ok(Token::PipeRight)));
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
    let mut lexer = Token::lexer("foo bar_baz α β γ");
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
    assert_eq!(lexer.next(), Some(Ok(Token::Identifier)));
}

#[test]
fn test_unicode_operators() {
    let mut lexer = Token::lexer("≤ ≥ ≠ ∈ ⊆ √");
    assert_eq!(lexer.next(), Some(Ok(Token::LessEqual)));
    assert_eq!(lexer.next(), Some(Ok(Token::GreaterEqual)));
    assert_eq!(lexer.next(), Some(Ok(Token::NotEqual)));
    assert_eq!(lexer.next(), Some(Ok(Token::ElementOf)));
    assert_eq!(lexer.next(), Some(Ok(Token::SubsetEq)));
    assert_eq!(lexer.next(), Some(Ok(Token::SquareRoot)));
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
        Token::DotLt,
        Token::DotGt,
        Token::DotLtEq,
        Token::DotGtEq,
        Token::DotEqEq,
        Token::DotNotEq,
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
