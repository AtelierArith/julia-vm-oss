//! Parse error types

use crate::span::Span;
use thiserror::Error;

/// Parse error type
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    /// Unexpected token
    #[error("unexpected token '{found}' at {span}, expected {expected}")]
    UnexpectedToken {
        found: String,
        expected: String,
        span: Span,
    },

    /// Unexpected end of input
    #[error("unexpected end of input at {span}, expected {expected}")]
    UnexpectedEof { expected: String, span: Span },

    /// Invalid escape sequence
    #[error("invalid escape sequence '{sequence}' at {span}")]
    InvalidEscape { sequence: String, span: Span },

    /// Unterminated string
    #[error("unterminated string literal starting at {span}")]
    UnterminatedString { span: Span },

    /// Unterminated command literal
    #[error("unterminated command literal starting at {span}")]
    UnterminatedCommand { span: Span },

    /// Unterminated character literal
    #[error("unterminated character literal starting at {span}")]
    UnterminatedCharacter { span: Span },

    /// Unterminated block comment
    #[error("unterminated block comment starting at {span}")]
    UnterminatedBlockComment { span: Span },

    /// Invalid number literal
    #[error("invalid number literal '{literal}' at {span}")]
    InvalidNumber { literal: String, span: Span },

    /// Invalid character literal
    #[error("invalid character literal at {span}")]
    InvalidCharacter { span: Span },

    /// Mismatched brackets
    #[error("mismatched brackets: expected '{expected}', found '{found}' at {span}")]
    MismatchedBrackets {
        expected: char,
        found: char,
        span: Span,
    },

    /// Unclosed bracket
    #[error("unclosed bracket '{bracket}' at {span}")]
    UnclosedBracket { bracket: char, span: Span },

    /// Invalid syntax
    #[error("{message} at {span}")]
    InvalidSyntax { message: String, span: Span },

    /// Lexer error
    #[error("unrecognized token at {span}")]
    LexerError { span: Span },
}

impl ParseError {
    /// Whether this diagnostic means a REPL can become valid by appending more
    /// source, rather than by editing the text already entered.
    ///
    /// This classification belongs to the parser because it is defined by
    /// lexer/parser recovery state, not by a second list of Julia block
    /// keywords maintained by a CLI consumer (Issues #10235/#10262/#10862).
    pub fn is_incomplete_input(&self) -> bool {
        match self {
            ParseError::UnexpectedEof { .. }
            | ParseError::UnterminatedString { .. }
            | ParseError::UnterminatedCommand { .. }
            | ParseError::UnterminatedCharacter { .. }
            | ParseError::UnterminatedBlockComment { .. }
            | ParseError::UnclosedBracket { .. } => true,
            ParseError::UnexpectedToken { found, .. } => found == "end of input",
            _ => false,
        }
    }

    /// Get the span of the error
    pub fn span(&self) -> Option<&Span> {
        match self {
            ParseError::UnexpectedToken { span, .. } => Some(span),
            ParseError::UnexpectedEof { span, .. } => Some(span),
            ParseError::InvalidEscape { span, .. } => Some(span),
            ParseError::UnterminatedString { span } => Some(span),
            ParseError::UnterminatedCommand { span } => Some(span),
            ParseError::UnterminatedCharacter { span } => Some(span),
            ParseError::UnterminatedBlockComment { span } => Some(span),
            ParseError::InvalidNumber { span, .. } => Some(span),
            ParseError::InvalidCharacter { span } => Some(span),
            ParseError::MismatchedBrackets { span, .. } => Some(span),
            ParseError::UnclosedBracket { span, .. } => Some(span),
            ParseError::InvalidSyntax { span, .. } => Some(span),
            ParseError::LexerError { span } => Some(span),
        }
    }

    /// Format the diagnostic payload without rendering its source span.
    ///
    /// `Display` remains the human-facing parser error (and therefore includes
    /// `at <span>`).  `Base.JuliaSyntax.Diagnostic.message`, however, stores
    /// only the diagnostic text; its byte bounds live in sibling fields
    /// (Issue #11572).
    pub fn diagnostic_message(&self) -> String {
        match self {
            ParseError::UnexpectedToken {
                found, expected, ..
            } => format!("unexpected token '{found}', expected {expected}"),
            ParseError::UnexpectedEof { expected, .. } => {
                format!("unexpected end of input, expected {expected}")
            }
            ParseError::InvalidEscape { sequence, .. } => {
                format!("invalid escape sequence '{sequence}'")
            }
            ParseError::UnterminatedString { .. } => "unterminated string literal".to_string(),
            ParseError::UnterminatedCommand { .. } => "unterminated string literal".to_string(),
            ParseError::UnterminatedCharacter { .. } => {
                "unterminated character literal".to_string()
            }
            ParseError::UnterminatedBlockComment { .. } => "unterminated block comment".to_string(),
            ParseError::InvalidNumber { literal, .. } => {
                format!("invalid number literal '{literal}'")
            }
            ParseError::InvalidCharacter { .. } => "invalid character literal".to_string(),
            ParseError::MismatchedBrackets {
                expected, found, ..
            } => format!("mismatched brackets: expected '{expected}', found '{found}'"),
            ParseError::UnclosedBracket { bracket, .. } => {
                format!("unclosed bracket '{bracket}'")
            }
            ParseError::InvalidSyntax { message, .. } => message.clone(),
            ParseError::LexerError { .. } => "unrecognized token".to_string(),
        }
    }

    /// Create an unexpected token error
    pub fn unexpected_token(
        found: impl Into<String>,
        expected: impl Into<String>,
        span: Span,
    ) -> Self {
        let found = found.into();
        ParseError::UnexpectedToken {
            found: if found.is_empty() {
                "end of input".to_string()
            } else {
                display_token_text(found)
            },
            expected: expected.into(),
            span,
        }
    }

    /// Create an unexpected EOF error
    pub fn unexpected_eof(expected: impl Into<String>, span: Span) -> Self {
        ParseError::UnexpectedEof {
            expected: expected.into(),
            span,
        }
    }

    /// Create an invalid syntax error
    pub fn invalid_syntax(message: impl Into<String>, span: Span) -> Self {
        ParseError::InvalidSyntax {
            message: message.into(),
            span,
        }
    }

    /// Format error with source context
    ///
    /// Returns a string showing the source line with an error marker.
    pub fn format_with_context(&self, source: &str) -> String {
        let Some(span) = self.span() else {
            return String::new();
        };

        let lines: Vec<&str> = source.lines().collect();
        if span.start_line == 0 || span.start_line > lines.len() {
            return String::new();
        }

        let end_line = span.end_line.max(span.start_line).min(lines.len());
        let line_width = end_line.to_string().len();
        let mut rendered = Vec::new();

        for line_no in span.start_line..=end_line {
            let line = lines[line_no - 1];
            let start_col = if line_no == span.start_line {
                span.start_column.saturating_sub(1)
            } else {
                0
            };
            let marker_len = if span.start_line == span.end_line {
                span.end_column
                    .saturating_sub(span.start_column)
                    .max(1)
                    .min(line.len().saturating_sub(start_col).max(1))
            } else if line_no == span.start_line {
                line.len().saturating_sub(start_col).max(1)
            } else if line_no == end_line {
                span.end_column.saturating_sub(1).min(line.len()).max(1)
            } else {
                line.len().max(1)
            };

            rendered.push(format!("  {line_no:>line_width$} | {line}"));
            rendered.push(format!(
                "  {} | {}{}",
                " ".repeat(line_width),
                " ".repeat(start_col),
                "^".repeat(marker_len)
            ));
        }

        rendered.join("\n")
    }
}

fn display_token_text(text: String) -> String {
    match text.as_str() {
        "\n" | "\r\n" | "\r" => "newline".to_string(),
        "\t" => "tab".to_string(),
        _ if text.chars().any(char::is_control) => text.escape_default().to_string(),
        _ => text,
    }
}

fn expected_is_block_end(expected: &str) -> bool {
    expected == "KwEnd" || expected == "end" || expected.contains("`end`")
}

/// Result type for parsing operations
pub type ParseResult<T> = Result<T, ParseError>;

/// Collection of parse errors for error recovery
#[derive(Debug, Default)]
pub struct ParseErrors {
    errors: Vec<ParseError>,
}

impl ParseErrors {
    /// Create a new empty error collection
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add an error
    pub fn push(&mut self, error: ParseError) {
        self.errors.push(error);
    }

    /// Check if there are any errors
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Whether every reported error is an appendable end-of-input condition.
    /// A mixed or ordinary syntax error is submitted immediately so the REPL
    /// can report it instead of waiting for continuation lines forever.
    pub fn is_incomplete_input(&self) -> bool {
        !self.errors.is_empty() && self.errors.iter().all(ParseError::is_incomplete_input)
    }

    /// Upstream-shaped reason for an appendable parser failure.
    ///
    /// This is structural parser state, not a consumer-side keyword/message
    /// heuristic.  It feeds `Base.JuliaSyntax.ParseError.incomplete_tag`
    /// (Issue #11572) and refines the existing boolean classification used by
    /// the REPL (Issues #10235/#10262/#10862).
    pub fn incomplete_tag(&self) -> &'static str {
        if !self.is_incomplete_input() {
            return "none";
        }
        if self
            .errors
            .iter()
            .any(|error| matches!(error, ParseError::UnterminatedCommand { .. }))
        {
            return "cmd";
        }
        if self
            .errors
            .iter()
            .any(|error| matches!(error, ParseError::UnterminatedString { .. }))
        {
            return "string";
        }
        if self
            .errors
            .iter()
            .any(|error| matches!(error, ParseError::UnterminatedCharacter { .. }))
        {
            return "char";
        }
        if self
            .errors
            .iter()
            .any(|error| matches!(error, ParseError::UnterminatedBlockComment { .. }))
        {
            return "comment";
        }
        if self.errors.iter().any(|error| match error {
            ParseError::UnexpectedEof { expected, .. } => expected_is_block_end(expected),
            ParseError::UnexpectedToken {
                found, expected, ..
            } => found == "end of input" && expected_is_block_end(expected),
            _ => false,
        }) {
            return "block";
        }
        "other"
    }

    /// Get the number of errors
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Get all errors
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    /// Take all errors
    pub fn take(self) -> Vec<ParseError> {
        self.errors
    }

    /// Iterate over errors
    pub fn iter(&self) -> impl Iterator<Item = &ParseError> {
        self.errors.iter()
    }

    /// Get the first error (for backward compatibility)
    pub fn first(&self) -> Option<&ParseError> {
        self.errors.first()
    }

    /// Format all errors as a single message
    pub fn format_all(&self, source: &str) -> String {
        if self.errors.is_empty() {
            return String::new();
        }

        self.errors
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let context = e.format_with_context(source);
                format!("Error {}: {}\n{}", i + 1, e, context)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl IntoIterator for ParseErrors {
    type Item = ParseError;
    type IntoIter = std::vec::IntoIter<ParseError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

impl<'a> IntoIterator for &'a ParseErrors {
    type Item = &'a ParseError;
    type IntoIter = std::slice::Iter<'a, ParseError>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_and_parse_error_display_uses_line_columns_issue_8454() {
        let span = Span::new(0, 5, 1, 1, 1, 6);
        assert_eq!(span.to_string(), "1:1..1:6");

        let err = ParseError::unexpected_token("foo", "bar", span);
        assert_eq!(
            err.to_string(),
            "unexpected token 'foo' at 1:1..1:6, expected bar"
        );
    }

    #[test]
    fn test_unexpected_token() {
        let span = Span::new(0, 5, 1, 1, 1, 6);
        let err = ParseError::unexpected_token("foo", "bar", span);

        assert!(err.span().is_some());
        assert!(err.to_string().contains("foo"));
        assert!(err.to_string().contains("bar"));
    }

    #[test]
    fn test_parse_errors() {
        let mut errors = ParseErrors::new();
        assert!(errors.is_empty());

        let span = Span::new(0, 5, 1, 1, 1, 6);
        let span2 = Span::new(10, 13, 1, 11, 1, 14);
        errors.push(ParseError::unexpected_token("a", "b", span));
        errors.push(ParseError::unexpected_eof("end", span2));

        assert_eq!(errors.len(), 2);
        assert!(!errors.is_empty());
        assert!(errors.first().is_some());
    }

    #[test]
    fn incomplete_input_classification_uses_parser_recovery_state_issue_10262() {
        for source in [
            "function f(x)",
            "if true\n1",
            "x = (1 +",
            "\"unterminated",
            "#= unterminated",
            "'",
            "'a",
            "f('a",
        ] {
            let (_, errors) = crate::parse_with_errors(source);
            assert!(
                errors.is_incomplete_input(),
                "expected appendable EOF diagnostics for {source:?}: {errors:?}"
            );
        }

        for source in ["x = )", "if )", "end", "''", "'ab'"] {
            let (_, errors) = crate::parse_with_errors(source);
            assert!(
                !errors.is_incomplete_input(),
                "ordinary syntax error must be submitted for reporting: {source:?}"
            );
        }
    }

    #[test]
    fn diagnostic_message_omits_span_issue_11572() {
        let error = ParseError::unexpected_token(")", "expression", Span::new(0, 1, 1, 1, 1, 2));
        assert_eq!(
            error.diagnostic_message(),
            "unexpected token ')', expected expression"
        );
        assert!(!error.diagnostic_message().contains("1:1"));
    }

    #[test]
    fn incomplete_tag_is_structural_issue_11572() {
        for (source, expected) in [
            ("", "none"),
            ("\"", "string"),
            ("'", "char"),
            ("#=", "comment"),
            ("`", "cmd"),
            ("begin;", "block"),
            ("quote;", "block"),
            ("let;", "block"),
            ("for i=1;", "block"),
            ("function f();", "block"),
            ("f() do x;", "block"),
            ("module X;", "block"),
            ("mutable struct X;", "block"),
            ("struct X;", "block"),
            ("(", "other"),
            ("[", "other"),
            ("for", "other"),
            ("function", "other"),
            ("f() do", "other"),
            ("module", "other"),
            ("mutable struct", "other"),
            ("struct", "other"),
            ("quote", "block"),
            ("let", "block"),
            ("begin", "block"),
            ("x = )", "none"),
        ] {
            let (_, errors) = crate::parse_with_errors(source);
            assert_eq!(
                errors.incomplete_tag(),
                expected,
                "source={source:?}, errors={errors:?}"
            );
        }
    }

    #[test]
    fn parse_one_error_extent_stops_after_recovery_newline_11634() {
        let (_, errors, consumed) = crate::Parser::new(")\nx").parse_one();
        assert!(!errors.is_empty());
        assert_eq!(consumed, 2);
        assert_eq!(
            errors
                .first()
                .and_then(ParseError::span)
                .map(|span| (span.start, span.end)),
            Some((0, 1))
        );
    }

    #[test]
    fn parse_one_success_stops_before_next_line_11637() {
        let (node, errors, consumed) = crate::Parser::new("x\n)").parse_one();
        assert!(errors.is_empty());
        assert!(node.is_some());
        assert_eq!(consumed, 2);
    }

    #[test]
    fn parse_one_consumes_only_one_newline_separator_11636() {
        let (node, errors, consumed) = crate::Parser::new("x\n\n)").parse_one();
        assert!(errors.is_empty());
        assert!(node.is_some());
        assert_eq!(consumed, 2);
    }

    #[test]
    fn parse_one_groups_same_line_semicolon_expressions_11636() {
        let (node, errors, consumed) = crate::Parser::new("x;y\n)").parse_one();
        assert!(errors.is_empty());
        assert!(node.is_some(), "semicolon group was not returned");
        if let Some(node) = node {
            assert_eq!(node.kind, crate::NodeKind::SourceFile);
            assert_eq!(node.children.len(), 2);
        }
        assert_eq!(consumed, 4);
    }

    #[test]
    fn parse_one_extra_token_span_includes_intervening_whitespace_11634() {
        let (node, errors, consumed) = crate::Parser::new("é )").parse_one();
        assert!(node.is_none());
        assert_eq!(consumed, 4);
        assert!(matches!(
            errors.first(),
            Some(ParseError::InvalidSyntax { message, .. })
                if message == "extra tokens after end of expression"
        ));
        assert_eq!(
            errors
                .first()
                .and_then(ParseError::span)
                .map(|span| (span.start, span.end)),
            Some((2, 4))
        );
    }

    #[test]
    fn test_unexpected_eof_with_span() {
        let span = Span::new(10, 10, 1, 11, 1, 11);
        let err = ParseError::unexpected_eof("expression", span);

        assert!(err.span().is_some());
        assert!(err.to_string().contains("expression"));
    }

    #[test]
    fn test_format_with_context() {
        let source = "let x = \nlet y = 2";
        let span = Span::new(8, 8, 1, 1, 9, 9);
        let err = ParseError::unexpected_eof("value", span);

        let context = err.format_with_context(source);
        assert!(context.contains("let x ="));
        assert!(context.contains("^"));
    }

    #[test]
    fn format_with_context_underlines_multiline_spans_issue_8454() {
        let source = "let xs = [\n    1,\n    2\n]\n";
        let span = Span::new(9, 23, 1, 3, 10, 6);
        let err = ParseError::invalid_syntax("array literal is incomplete", span);

        assert_eq!(
            err.format_with_context(source),
            "  1 | let xs = [\n    |          ^\n  2 |     1,\n    | ^^^^^^\n  3 |     2\n    | ^^^^^"
        );
    }

    #[test]
    fn test_format_all() {
        let source = "let x = \nlet y = 2";
        let span1 = Span::new(8, 8, 1, 1, 9, 9);
        let span2 = Span::new(9, 18, 2, 2, 1, 10);

        let mut errors = ParseErrors::new();
        errors.push(ParseError::unexpected_eof("value", span1));
        errors.push(ParseError::unexpected_token("let", "end", span2));

        let formatted = errors.format_all(source);
        assert!(formatted.contains("Error 1:"));
        assert!(formatted.contains("Error 2:"));
    }
}
