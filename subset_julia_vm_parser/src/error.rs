//! Parse error types

use crate::span::Span;
use thiserror::Error;

/// Parse error type
#[derive(Error, Debug, Clone)]
pub enum ParseError {
    /// Unexpected token
    #[error("unexpected token '{found}' at {span:?}, expected {expected}")]
    UnexpectedToken {
        found: String,
        expected: String,
        span: Span,
    },

    /// Unexpected end of input
    #[error("unexpected end of input at {span:?}, expected {expected}")]
    UnexpectedEof { expected: String, span: Span },

    /// Invalid escape sequence
    #[error("invalid escape sequence '{sequence}' at {span:?}")]
    InvalidEscape { sequence: String, span: Span },

    /// Unterminated string
    #[error("unterminated string literal starting at {span:?}")]
    UnterminatedString { span: Span },

    /// Unterminated block comment
    #[error("unterminated block comment starting at {span:?}")]
    UnterminatedBlockComment { span: Span },

    /// Invalid number literal
    #[error("invalid number literal '{literal}' at {span:?}")]
    InvalidNumber { literal: String, span: Span },

    /// Invalid character literal
    #[error("invalid character literal at {span:?}")]
    InvalidCharacter { span: Span },

    /// Mismatched brackets
    #[error("mismatched brackets: expected '{expected}', found '{found}' at {span:?}")]
    MismatchedBrackets {
        expected: char,
        found: char,
        span: Span,
    },

    /// Unclosed bracket
    #[error("unclosed bracket '{bracket}' at {span:?}")]
    UnclosedBracket { bracket: char, span: Span },

    /// Invalid syntax
    #[error("{message} at {span:?}")]
    InvalidSyntax { message: String, span: Span },

    /// Lexer error
    #[error("unrecognized token at {span:?}")]
    LexerError { span: Span },
}

impl ParseError {
    /// Get the span of the error
    pub fn span(&self) -> Option<&Span> {
        match self {
            ParseError::UnexpectedToken { span, .. } => Some(span),
            ParseError::UnexpectedEof { span, .. } => Some(span),
            ParseError::InvalidEscape { span, .. } => Some(span),
            ParseError::UnterminatedString { span } => Some(span),
            ParseError::UnterminatedBlockComment { span } => Some(span),
            ParseError::InvalidNumber { span, .. } => Some(span),
            ParseError::InvalidCharacter { span } => Some(span),
            ParseError::MismatchedBrackets { span, .. } => Some(span),
            ParseError::UnclosedBracket { span, .. } => Some(span),
            ParseError::InvalidSyntax { span, .. } => Some(span),
            ParseError::LexerError { span } => Some(span),
        }
    }

    /// Create an unexpected token error
    pub fn unexpected_token(
        found: impl Into<String>,
        expected: impl Into<String>,
        span: Span,
    ) -> Self {
        ParseError::UnexpectedToken {
            found: found.into(),
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
        let line_idx = span.start_line.saturating_sub(1);

        if line_idx >= lines.len() {
            return String::new();
        }

        let line = lines[line_idx];
        let col = span.start_column.saturating_sub(1);
        let len = if span.start_line == span.end_line {
            span.end_column.saturating_sub(span.start_column).max(1)
        } else {
            1
        };

        // Build the error marker
        let spaces = " ".repeat(col);
        let marker = "^".repeat(len.min(line.len().saturating_sub(col)).max(1));

        format!(
            "  {} | {}\n  {} | {}{}",
            span.start_line,
            line,
            " ".repeat(span.start_line.to_string().len()),
            spaces,
            marker
        )
    }
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
    fn test_unexpected_eof_with_span() {
        let span = Span::new(10, 10, 1, 11, 1, 11);
        let err = ParseError::unexpected_eof("expression", span);

        assert!(err.span().is_some());
        assert!(err.to_string().contains("expression"));
    }

    #[test]
    fn test_format_with_context() {
        let source = "let x = \nlet y = 2";
        let span = Span::new(8, 8, 1, 9, 1, 9);
        let err = ParseError::unexpected_eof("value", span);

        let context = err.format_with_context(source);
        assert!(context.contains("let x ="));
        assert!(context.contains("^"));
    }

    #[test]
    fn test_format_all() {
        let source = "let x = \nlet y = 2";
        let span1 = Span::new(8, 8, 1, 9, 1, 9);
        let span2 = Span::new(9, 18, 2, 1, 2, 10);

        let mut errors = ParseErrors::new();
        errors.push(ParseError::unexpected_eof("value", span1));
        errors.push(ParseError::unexpected_token("let", "end", span2));

        let formatted = errors.format_all(source);
        assert!(formatted.contains("Error 1:"));
        assert!(formatted.contains("Error 2:"));
    }
}
