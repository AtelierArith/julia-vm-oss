//! Lexer for Julia source code
//!
//! Wraps the logos-generated lexer with additional functionality
//! for block comments, strings, and other complex tokens.

use logos::Logos;

use crate::error::{ParseError, ParseResult};
use crate::span::{SourceMap, Span};
use crate::token::Token;

/// A token with its span
#[derive(Debug, Clone)]
pub struct SpannedToken<'a> {
    pub token: Token,
    pub span: Span,
    pub text: &'a str,
}

impl<'a> SpannedToken<'a> {
    pub fn new(token: Token, span: Span, text: &'a str) -> Self {
        Self {
            token,
            span,
            text,
        }
    }
}

/// Julia lexer
pub struct Lexer<'a> {
    source: &'a str,
    inner: logos::Lexer<'a, Token>,
    source_map: SourceMap,
    /// Peeked token (for lookahead)
    peeked: Option<Result<SpannedToken<'a>, ParseError>>,
    /// Current position in source
    position: usize,
    /// Offset from original source (used after restarting lexer)
    offset: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given source code
    pub fn new(source: &'a str) -> Self {
        let source_map = SourceMap::new(source);
        Self {
            source,
            inner: Token::lexer(source),
            source_map,
            peeked: None,
            position: 0,
            offset: 0,
        }
    }

    /// Get the source code
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Get the source map
    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    /// Create a span from byte offsets
    fn make_span(&self, start: usize, end: usize) -> Span {
        self.source_map.span(start, end)
    }

    /// Peek at the next token without consuming it
    pub fn peek(&mut self) -> Option<&Result<SpannedToken<'a>, ParseError>> {
        if self.peeked.is_none() {
            self.peeked = self.next_token_internal();
        }
        self.peeked.as_ref()
    }

    /// Get the next token
    pub fn next_token(&mut self) -> Option<Result<SpannedToken<'a>, ParseError>> {
        if let Some(peeked) = self.peeked.take() {
            return Some(peeked);
        }
        self.next_token_internal()
    }

    /// Internal method to get the next token
    fn next_token_internal(&mut self) -> Option<Result<SpannedToken<'a>, ParseError>> {
        let result = self.inner.next()?;
        let span = self.inner.span();
        let start = self.offset + span.start;
        let end = self.offset + span.end;
        self.position = end;

        match result {
            Ok(Token::BlockCommentStart) => {
                // Handle nested block comments
                match self.scan_block_comment(end) {
                    Ok(comment_end) => {
                        // Restart lexer from after the block comment
                        self.restart_from(comment_end);
                        let span = self.make_span(start, comment_end);
                        let text = &self.source[start..comment_end];
                        Some(Ok(SpannedToken::new(
                            Token::LineComment, // Treat as comment
                            span,
                            text,
                        )))
                    }
                    Err(e) => {
                        // Restart lexer at end of source to prevent further tokens
                        self.restart_from(self.source.len());
                        Some(Err(e))
                    }
                }
            }

            Ok(Token::DoubleQuote) => {
                // Handle string content - scan to find closing quote
                // This prevents the lexer from trying to tokenize content inside strings
                match self.scan_string_to_close(end, false) {
                    Ok(string_end) => {
                        // Restart lexer from after the closing quote
                        self.restart_from(string_end);
                        let span = self.make_span(start, end);
                        let text = &self.source[start..end];
                        Some(Ok(SpannedToken::new(Token::DoubleQuote, span, text)))
                    }
                    Err(e) => {
                        self.restart_from(self.source.len());
                        Some(Err(e))
                    }
                }
            }

            Ok(Token::TripleDoubleQuote) => {
                // Handle triple-quoted string content
                match self.scan_string_to_close(end, true) {
                    Ok(string_end) => {
                        self.restart_from(string_end);
                        let span = self.make_span(start, end);
                        let text = &self.source[start..end];
                        Some(Ok(SpannedToken::new(Token::TripleDoubleQuote, span, text)))
                    }
                    Err(e) => {
                        self.restart_from(self.source.len());
                        Some(Err(e))
                    }
                }
            }

            Ok(token) => {
                let span = self.make_span(start, end);
                let text = &self.source[start..end];
                Some(Ok(SpannedToken::new(token, span, text)))
            }

            Err(()) => {
                // Lexer error - unrecognized token
                let span = self.make_span(start, end);
                Some(Err(ParseError::LexerError { span }))
            }
        }
    }

    /// Scan a block comment (handles nesting).
    /// Uses memchr to jump to potential delimiter positions.
    fn scan_block_comment(&self, start: usize) -> ParseResult<usize> {
        let mut depth = 1;
        let mut pos = start;
        let bytes = self.source.as_bytes();

        while pos < bytes.len() && depth > 0 {
            match memchr::memchr2(b'#', b'=', &bytes[pos..]) {
                None => {
                    pos = bytes.len();
                    break;
                }
                Some(offset) => {
                    pos += offset;
                    if pos + 1 < bytes.len() {
                        if bytes[pos] == b'#' && bytes[pos + 1] == b'=' {
                            depth += 1;
                            pos += 2;
                            continue;
                        }
                        if bytes[pos] == b'=' && bytes[pos + 1] == b'#' {
                            depth -= 1;
                            pos += 2;
                            continue;
                        }
                    }
                    pos += 1;
                }
            }
        }

        if depth > 0 {
            Err(ParseError::UnterminatedBlockComment {
                span: self.make_span(start - 2, pos),
            })
        } else {
            Ok(pos)
        }
    }

    /// Scan string content to find the closing quote.
    /// Uses memchr for SIMD-accelerated scanning.
    fn scan_string_to_close(&self, start: usize, is_triple: bool) -> ParseResult<usize> {
        let bytes = self.source.as_bytes();
        let mut pos = start;

        while pos < bytes.len() {
            match memchr::memchr2(b'\\', b'"', &bytes[pos..]) {
                None => break,
                Some(offset) => {
                    pos += offset;
                    if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
                        pos += 2;
                        continue;
                    }
                    if is_triple {
                        if pos + 3 <= bytes.len() && &bytes[pos..pos + 3] == b"\"\"\"" {
                            return Ok(pos + 3);
                        }
                        pos += 1;
                    } else {
                        return Ok(pos + 1);
                    }
                }
            }
        }

        Err(ParseError::UnterminatedString {
            span: self.make_span(start - if is_triple { 3 } else { 1 }, pos),
        })
    }

    /// Restart the lexer from a new position.
    /// Uses bump() to advance within the current logos lexer when possible.
    pub fn restart_from(&mut self, pos: usize) {
        self.peeked = None;
        self.position = pos;
        let logos_abs_pos = self.offset + self.inner.span().end;
        if pos > logos_abs_pos && pos <= self.source.len() {
            let skip = pos - logos_abs_pos;
            self.inner.bump(skip);
        } else if pos < self.source.len() {
            let remaining = &self.source[pos..];
            self.inner = Token::lexer(remaining);
            self.offset = pos;
        } else {
            self.inner = Token::lexer("");
            self.offset = pos;
        }
    }

    /// Check if we're at end of input
    pub fn is_eof(&mut self) -> bool {
        self.peek().is_none()
    }

    /// Get current position in source
    pub fn position(&self) -> usize {
        self.position
    }

    /// Collect all tokens (for debugging)
    pub fn collect_all(mut self) -> Vec<Result<SpannedToken<'a>, ParseError>> {
        let mut tokens = Vec::new();
        while let Some(token) = self.next_token() {
            tokens.push(token);
        }
        tokens
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<SpannedToken<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

/// Tokenize source code into a vector of spanned tokens
pub fn tokenize(source: &str) -> Vec<Result<SpannedToken<'_>, ParseError>> {
    Lexer::new(source).collect_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let source = "function foo(x) x + 1 end";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect();

        assert_eq!(
            tokens,
            vec![
                Token::KwFunction,
                Token::Identifier,
                Token::LParen,
                Token::Identifier,
                Token::RParen,
                Token::Identifier,
                Token::Plus,
                Token::DecimalLiteral,
                Token::KwEnd,
            ]
        );
    }

    #[test]
    fn test_block_comment() {
        let source = "#= comment =# 42";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect();

        assert_eq!(tokens, vec![Token::LineComment, Token::DecimalLiteral]);
    }

    #[test]
    fn test_nested_block_comment() {
        let source = "#= outer #= inner =# outer =# 42";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|t| t.token)
            .collect();

        assert_eq!(tokens, vec![Token::LineComment, Token::DecimalLiteral]);
    }

    #[test]
    fn test_unterminated_block_comment() {
        let source = "#= unterminated";
        let tokens: Vec<_> = tokenize(source).into_iter().collect();

        assert_eq!(tokens.len(), 1);
        assert!(tokens[0].is_err());
    }

    #[test]
    fn test_spans() {
        let source = "foo + bar";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(tokens.len(), 3);

        // "foo" at 0..3
        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, 3);
        assert_eq!(tokens[0].text, "foo");

        // "+" at 4..5
        assert_eq!(tokens[1].span.start, 4);
        assert_eq!(tokens[1].span.end, 5);

        // "bar" at 6..9
        assert_eq!(tokens[2].span.start, 6);
        assert_eq!(tokens[2].span.end, 9);
    }

    #[test]
    fn test_multiline_spans() {
        let source = "foo\nbar";
        let tokens: Vec<_> = tokenize(source)
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(tokens.len(), 3); // foo, newline, bar

        // "foo" at line 1
        assert_eq!(tokens[0].span.start_line, 1);
        assert_eq!(tokens[0].span.start_column, 1);

        // "bar" at line 2
        assert_eq!(tokens[2].span.start_line, 2);
        assert_eq!(tokens[2].span.start_column, 1);
    }

    #[test]
    fn test_peek() {
        let source = "a b c";
        let mut lexer = Lexer::new(source);

        // Peek should return the first token
        let peeked = lexer.peek().unwrap().as_ref().unwrap();
        assert_eq!(peeked.text, "a");

        // Peek again should return the same token
        let peeked = lexer.peek().unwrap().as_ref().unwrap();
        assert_eq!(peeked.text, "a");

        // Next should consume the peeked token
        let next = lexer.next_token().unwrap().unwrap();
        assert_eq!(next.text, "a");

        // Next should return the second token
        let next = lexer.next_token().unwrap().unwrap();
        assert_eq!(next.text, "b");
    }
}
