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
        Self { token, span, text }
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

    /// Create a lexer for bounded lookahead where only the token *kind* is
    /// read back (spans are discarded by the caller).
    ///
    /// `Lexer::new` pays for `SourceMap::new`, an `O(source length)` scan for
    /// newlines, on every call. Parser lookahead helpers such as
    /// `peek_non_newline_token` construct a throwaway lexer on every newline
    /// encountered inside `(...)`/`[...]`/`{...}` groupings, so for deeply
    /// nested/grouped expressions that scan was repeated dozens-to-hundreds
    /// of times per parse. Since these callers only match on `token.token`
    /// and never read `token.span`, the source map's line/column bookkeeping
    /// is wasted work; this constructor swaps in a cheap stub so building the
    /// lookahead lexer is `O(1)` instead (Issue #10128).
    pub(crate) fn new_for_token_peek(source: &'a str) -> Self {
        Self {
            source,
            inner: Token::lexer(source),
            source_map: SourceMap::stub(),
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

            Ok(Token::Identifier)
                if end > start + 1
                    && self.source.as_bytes()[end - 1] == b'!'
                    && self.source.as_bytes().get(end) == Some(&b'=') =>
            {
                // The greedy identifier regex folded the `!` of a following `!=`
                // / `!==` into the name (e.g. `a!=b` lexed as `a!`). Give the `!`
                // back so it lexes as the operator: emit the name without the
                // trailing `!` and rewind the lexer to that `!` (Issue #8194).
                // `push!(x)` / `in!(...)` / `f! = 3` are unaffected because the
                // char after their `!` is not `=`.
                let name_end = end - 1;
                let span = self.make_span(start, name_end);
                let text = &self.source[start..name_end];
                self.restart_from(name_end);
                Some(Ok(SpannedToken::new(Token::Identifier, span, text)))
            }

            Ok(Token::Identifier)
                if self.source[start..end].starts_with('∘') && end > start + '∘'.len_utf8() =>
            {
                // The identifier regex admits several mathematical-symbol code
                // points as leading identifier characters. For composition
                // chains like `g∘g` or `!isempty∘last`, logos can therefore
                // greedily emit `∘last` as one Identifier. Julia treats `∘` as
                // an infix operator even without whitespace, so split the
                // leading ring operator and let the following identifier lex on
                // the next step (Issue #8759).
                let op_end = start + '∘'.len_utf8();
                let span = self.make_span(start, op_end);
                let text = &self.source[start..op_end];
                self.restart_from(op_end);
                Some(Ok(SpannedToken::new(Token::RingOperator, span, text)))
            }

            Ok(Token::FloatLiteral)
                if end > start + 1
                    && self.source.as_bytes()[end - 1] == b'.'
                    && self.source.as_bytes().get(end) == Some(&b'.') =>
            {
                // `10...` is `10` followed by splat `...`, not float `10.`
                // followed by range `..` (Issue #8759). Give the trailing dot
                // back so the next lexer step can emit `Ellipsis`.
                let number_end = end - 1;
                let span = self.make_span(start, number_end);
                let text = &self.source[start..number_end];
                self.restart_from(number_end);
                Some(Ok(SpannedToken::new(Token::DecimalLiteral, span, text)))
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
    fn test_adjacent_composition_operator_issue_8759() {
        let tokens: Vec<_> = tokenize("g∘g !isempty∘last textwidth∘last")
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|t| (t.token, t.text.to_string()))
            .collect();

        assert_eq!(
            tokens,
            vec![
                (Token::Identifier, "g".to_string()),
                (Token::RingOperator, "∘".to_string()),
                (Token::Identifier, "g".to_string()),
                (Token::Not, "!".to_string()),
                (Token::Identifier, "isempty".to_string()),
                (Token::RingOperator, "∘".to_string()),
                (Token::Identifier, "last".to_string()),
                (Token::Identifier, "textwidth".to_string()),
                (Token::RingOperator, "∘".to_string()),
                (Token::Identifier, "last".to_string()),
            ]
        );
    }

    #[test]
    fn test_mid_identifier_bang_and_not_equal_boundary_issue_10713() {
        let tokens: Vec<_> = tokenize("foo!bar name!x is!valid! a!=b a!b!=c a!b!==c .!x")
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|t| (t.token, t.text.to_string()))
            .collect();

        assert_eq!(
            tokens,
            vec![
                (Token::Identifier, "foo!bar".to_string()),
                (Token::Identifier, "name!x".to_string()),
                (Token::Identifier, "is!valid!".to_string()),
                (Token::Identifier, "a".to_string()),
                (Token::NotEq, "!=".to_string()),
                (Token::Identifier, "b".to_string()),
                (Token::Identifier, "a!b".to_string()),
                (Token::NotEq, "!=".to_string()),
                (Token::Identifier, "c".to_string()),
                (Token::Identifier, "a!b".to_string()),
                (Token::NotEqEq, "!==".to_string()),
                (Token::Identifier, "c".to_string()),
                (Token::DotNot, ".!".to_string()),
                (Token::Identifier, "x".to_string()),
            ]
        );
    }

    /// Paired boundary coverage (Issue #10848 convention): broadening the operator
    /// character set must not swallow, or be swallowed by, the neighbouring operator
    /// boundaries — `!=` / `!==` after an identifier, dotted operators, `::`, and the
    /// syntactic `&&` / `||` rejection (Issue #10932).
    #[test]
    fn test_unicode_operator_boundaries_issue_11083() {
        let cases: &[(&str, &[(Token, &str)])] = &[
            // Newly-recognized operator, tight spacing on both sides.
            (
                "a⊛b",
                &[
                    (Token::Identifier, "a"),
                    (Token::UnicodeOpTimes, "⊛"),
                    (Token::Identifier, "b"),
                ],
            ),
            // Suffixed operator immediately after an identifier that itself ends in
            // an identifier-continuation character.
            (
                "a₁⊗ᵢb",
                &[
                    (Token::Identifier, "a₁"),
                    (Token::UnicodeOpTimes, "⊗ᵢ"),
                    (Token::Identifier, "b"),
                ],
            ),
            // `!=` / `!==` boundaries stay intact next to the new operators.
            (
                "a⊛b!=c",
                &[
                    (Token::Identifier, "a"),
                    (Token::UnicodeOpTimes, "⊛"),
                    (Token::Identifier, "b"),
                    (Token::NotEq, "!="),
                    (Token::Identifier, "c"),
                ],
            ),
            (
                "a⊞b!==c",
                &[
                    (Token::Identifier, "a"),
                    (Token::UnicodeOpPlus, "⊞"),
                    (Token::Identifier, "b"),
                    (Token::NotEqEq, "!=="),
                    (Token::Identifier, "c"),
                ],
            ),
            // Dotted boundaries: `.!` (unary broadcast not) and `.⊛` stay distinct.
            (
                ".!a⊛b",
                &[
                    (Token::DotNot, ".!"),
                    (Token::Identifier, "a"),
                    (Token::UnicodeOpTimes, "⊛"),
                    (Token::Identifier, "b"),
                ],
            ),
            (
                "a.⊛b.+c",
                &[
                    (Token::Identifier, "a"),
                    (Token::DotUnicodeOpTimes, ".⊛"),
                    (Token::Identifier, "b"),
                    (Token::DotPlus, ".+"),
                    (Token::Identifier, "c"),
                ],
            ),
            // `::` type annotation next to an operator name.
            (
                "x::Int⊛y",
                &[
                    (Token::Identifier, "x"),
                    (Token::DoubleColon, "::"),
                    (Token::Identifier, "Int"),
                    (Token::UnicodeOpTimes, "⊛"),
                    (Token::Identifier, "y"),
                ],
            ),
            // `&&` / `||` keep their own (syntactic) tokens — they are NOT folded
            // into the generic operator classes (Issue #10932).
            (
                "a⊛b&&c||d",
                &[
                    (Token::Identifier, "a"),
                    (Token::UnicodeOpTimes, "⊛"),
                    (Token::Identifier, "b"),
                    (Token::AndAnd, "&&"),
                    (Token::Identifier, "c"),
                    (Token::OrOr, "||"),
                    (Token::Identifier, "d"),
                ],
            ),
            // Bare `&` / `|` (times/plus class ASCII) keep their dedicated tokens.
            (
                "a&b|c",
                &[
                    (Token::Identifier, "a"),
                    (Token::Amp, "&"),
                    (Token::Identifier, "b"),
                    (Token::Pipe, "|"),
                    (Token::Identifier, "c"),
                ],
            ),
        ];

        for (source, expected) in cases {
            let got: Vec<(Token, String)> = tokenize(source)
                .into_iter()
                .map(|r| {
                    r.unwrap_or_else(|e| panic!("source {source:?} must lex without error: {e:?}"))
                })
                .map(|t| (t.token, t.text.to_string()))
                .collect();
            let want: Vec<(Token, String)> = expected
                .iter()
                .map(|(t, s)| (t.clone(), (*s).to_string()))
                .collect();
            assert_eq!(got, want, "source {source:?}");
        }
    }

    /// Table-driven operator-boundary coverage for identifier-continuation
    /// characters (Issue #10848, prevention for Issue #10713): any expansion
    /// of the identifier-continuation set must keep the greedy identifier
    /// regex rewinding correctly before `!=` / `!==` and their dotted forms.
    /// One representative is enumerated per continuation-character class of
    /// the identifier regex in `token/mod.rs` (`_`, ASCII digit, XID_Continue
    /// letter, `!`, acute accent, modifier letter, prime, subscript,
    /// superscript, and the `²`/`³`/`¹` singles). Expected tokenizations
    /// verified against upstream Julia 1.12.6 (`Meta.parse`).
    #[test]
    fn test_identifier_continuation_operator_boundary_table_issue_10848() {
        // (label, continuation character as identifier tail)
        const CONTINUATION_CHARS: &[(&str, &str)] = &[
            ("underscore", "_"),
            ("ascii digit", "1"),
            ("xid-continue letter", "α"),
            ("bang", "!"),
            ("acute accent U+00B4", "\u{00B4}"),
            ("modifier letter U+02B9", "\u{02B9}"),
            ("prime U+2032", "′"),
            ("subscript U+2081", "₁"),
            ("superscript U+207F", "\u{207F}"),
            ("superscript two U+00B2", "²"),
        ];
        // (operator source suffix, expected operator token, operator text)
        const OPERATORS: &[(&str, Token, &str)] = &[
            ("!=b", Token::NotEq, "!="),
            ("!==b", Token::NotEqEq, "!=="),
            (".!=b", Token::DotNotEq, ".!="),
            (".!==b", Token::DotNotEqEq, ".!=="),
        ];

        for (label, cont) in CONTINUATION_CHARS {
            let ident = format!("a{cont}");
            for (suffix, op_token, op_text) in OPERATORS {
                let source = format!("{ident}{suffix}");
                let tokens: Vec<_> = tokenize(&source)
                    .into_iter()
                    .filter_map(|r| r.ok())
                    .map(|t| (t.token, t.text.to_string()))
                    .collect();
                assert_eq!(
                    tokens,
                    vec![
                        (Token::Identifier, ident.clone()),
                        (op_token.clone(), op_text.to_string()),
                        (Token::Identifier, "b".to_string()),
                    ],
                    "continuation char class {label:?}, source {source:?}"
                );
            }

            // Dotted unary form after the identifier boundary: `.!` applies
            // to a following identifier that itself uses the continuation
            // character (`.!aα`), staying `DotNot` + Identifier.
            let source = format!(".!{ident}");
            let tokens: Vec<_> = tokenize(&source)
                .into_iter()
                .filter_map(|r| r.ok())
                .map(|t| (t.token, t.text.to_string()))
                .collect();
            assert_eq!(
                tokens,
                vec![
                    (Token::DotNot, ".!".to_string()),
                    (Token::Identifier, ident.clone()),
                ],
                "continuation char class {label:?}, source {source:?}"
            );
        }

        // Only ONE trailing `!` is given back before `=`: `f!!=g` is
        // `f!` `!=` `g` upstream, not `f!!` `=` `g`.
        let tokens: Vec<_> = tokenize("f!!=g")
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|t| (t.token, t.text.to_string()))
            .collect();
        assert_eq!(
            tokens,
            vec![
                (Token::Identifier, "f!".to_string()),
                (Token::NotEq, "!=".to_string()),
                (Token::Identifier, "g".to_string()),
            ]
        );
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
