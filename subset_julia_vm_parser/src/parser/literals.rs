//! Literal parsing for Julia subset
//!
//! Handles parsing of identifiers, macro calls, and literal values.

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::Token;

use super::Parser;

impl<'a> Parser<'a> {
    /// Parse identifier, possibly as part of a symbol or qualified name
    pub(crate) fn parse_identifier_or_symbol(&mut self) -> ParseResult<CstNode> {
        self.parse_identifier()
    }

    /// Parse a macro call: @macro args or @Module.macro args
    ///
    /// Julia distinguishes between:
    /// - `@foo(x, y)` - parenthesized call style (no space before paren)
    /// - `@foo x y` - space-separated arguments
    /// - `@foo (x, y)` - single tuple argument (space before paren)
    ///
    /// In parenthesized call style, `@foo(x) * 2` parses as `(@foo(x)) * 2`.
    pub(crate) fn parse_macro_call(&mut self) -> ParseResult<CstNode> {
        let at_token = self.expect(Token::At)?;
        let start = at_token.span.start;

        // Parse macro name (identifier immediately following @)
        // Can be qualified: @Foo.bar or @Foo.Bar.baz
        let mut name = self.parse_identifier()?;

        // Handle qualified macro names like @Foo.bar
        while self.check(&Token::Dot) {
            let dot_start = name.span.start;
            self.advance(); // consume '.'
            let next_name = self.parse_identifier()?;
            let span = self.source_map.span(dot_start, next_name.span.end);
            name = CstNode::with_children(NodeKind::FieldExpression, span, vec![name, next_name]);
        }

        let macro_id_span = self.source_map.span(start, name.span.end);
        let macro_id =
            CstNode::with_children(NodeKind::MacroIdentifier, macro_id_span, vec![name.clone()]);

        let mut children = vec![macro_id];

        // Check if immediately followed by '(' (parenthesized call style)
        // We detect this by checking if the LParen starts right after the macro name
        if self.check(&Token::LParen) {
            if let Some(lparen_token) = self.current.as_ref() {
                // Check for no gap between macro name and '('
                if lparen_token.span.start == name.span.end {
                    // Parenthesized call style: @macro(args)
                    self.advance(); // consume '('

                    // Parse comma-separated arguments inside parentheses
                    if !self.check(&Token::RParen) {
                        loop {
                            // Skip newlines inside parentheses
                            while self.check(&Token::Newline) {
                                self.advance();
                            }
                            if self.check(&Token::RParen) {
                                break;
                            }
                            let arg = self.parse_expression()?;
                            children.push(arg);
                            if !self.check(&Token::Comma) {
                                break;
                            }
                            self.advance();
                        }
                    }

                    let rparen = self.expect(Token::RParen)?;
                    let end = rparen.span.end;
                    let span = self.source_map.span(start, end);
                    return Ok(CstNode::with_children(
                        NodeKind::MacrocallExpression,
                        span,
                        children,
                    ));
                }
            }
        }

        // Space-separated arguments (original behavior)
        // Parse macro arguments until end of line or newline
        while !self.is_at_end()
            && !self.check(&Token::Newline)
            && !self.check(&Token::Semicolon)
            && !self.check(&Token::KwEnd)
            && !self.check(&Token::RParen)
            && !self.check(&Token::RBracket)
        {
            // Special case: if we see `begin`, parse it as a block
            if self.check(&Token::KwBegin) {
                let block = self.parse_begin_block()?;
                children.push(block);
                break; // Block is the last argument
            }

            // Special case: if we see `struct` or `mutable`, parse as struct definition
            // This is needed for macros like @kwdef that take struct definitions
            if self.check(&Token::KwStruct) || self.check(&Token::KwMutable) {
                let struct_def = self.parse_struct_definition()?;
                children.push(struct_def);
                break; // Struct definition is the last argument
            }

            // Special case: if we see `for`, parse as for statement
            // This is needed for macros like @simd and @inbounds that take for loops
            if self.check(&Token::KwFor) {
                let for_stmt = self.parse_for_statement()?;
                children.push(for_stmt);
                break; // For statement is the last argument
            }

            // Special case: if we see `while`, parse as while statement
            // This is needed for macros like @inbounds that take while loops
            if self.check(&Token::KwWhile) {
                let while_stmt = self.parse_while_statement()?;
                children.push(while_stmt);
                break; // While statement is the last argument
            }

            // Special case: if we see `if`, parse as if statement
            // This is needed for macros like @inbounds that take if statements
            if self.check(&Token::KwIf) {
                let if_stmt = self.parse_if_statement()?;
                children.push(if_stmt);
                break; // If statement is the last argument
            }

            // Parse expression as argument
            let arg = self.parse_expression()?;
            children.push(arg);

            // Consume optional comma between arguments
            if self.check(&Token::Comma) {
                self.advance();
            }
            // Don't break - let the while condition handle when to stop
            // Julia macros are space-separated, so continue parsing
        }

        let end = children.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::MacrocallExpression,
            span,
            children,
        ))
    }

    // ==================== Literal Parsing ====================

    /// Parse an integer literal
    pub(crate) fn parse_integer_literal(&mut self) -> ParseResult<CstNode> {
        let token = self.advance().unwrap();
        Ok(CstNode::leaf(
            NodeKind::IntegerLiteral,
            token.span,
            token.text,
        ))
    }

    /// Parse a float literal
    pub(crate) fn parse_float_literal(&mut self) -> ParseResult<CstNode> {
        let token = self.advance().unwrap();
        Ok(CstNode::leaf(
            NodeKind::FloatLiteral,
            token.span,
            token.text,
        ))
    }

    /// Parse a boolean literal (true/false)
    pub(crate) fn parse_boolean_literal(&mut self) -> ParseResult<CstNode> {
        let token = self.advance().unwrap();
        Ok(CstNode::leaf(
            NodeKind::BooleanLiteral,
            token.span,
            token.text,
        ))
    }

    /// Parse a character literal
    pub(crate) fn parse_character_literal(&mut self) -> ParseResult<CstNode> {
        let token = self.advance().unwrap();
        Ok(CstNode::leaf(
            NodeKind::CharacterLiteral,
            token.span,
            token.text,
        ))
    }

    /// Parse a string literal
    pub(crate) fn parse_string_literal(&mut self) -> ParseResult<CstNode> {
        let start_token = self.advance().unwrap();
        let is_triple = matches!(start_token.token, Token::TripleDoubleQuote);
        let start = start_token.span.start;

        let mut children = Vec::new();
        let content_start = start_token.span.end;

        // Scan for string content and interpolations
        // For now, we'll do a simple scan until we find the closing quote
        let end = self.scan_string_content(content_start, is_triple, &mut children)?;

        // Restart lexer from after the string to synchronize
        self.lexer.restart_from(end);
        self.current = None;
        self.advance(); // Prime with next token

        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::StringLiteral,
            span,
            children,
        ))
    }

    /// Parse command literal: `command`
    pub(crate) fn parse_command_literal(&mut self) -> ParseResult<CstNode> {
        let start_token = self.advance().unwrap();
        let is_triple = matches!(start_token.token, Token::TripleBacktick);
        let start = start_token.span.start;

        let content_start = start_token.span.end;

        // Scan for command content until closing backtick
        let end = self.scan_command_content(content_start, is_triple)?;

        // Restart lexer from after the command to synchronize
        self.lexer.restart_from(end);
        self.current = None;
        self.advance(); // Prime with next token

        let span = self.source_map.span(start, end);
        let text = &self.source[content_start..end - if is_triple { 3 } else { 1 }];
        Ok(CstNode::leaf(NodeKind::CommandLiteral, span, text))
    }

    /// Scan command content until closing backtick
    pub(crate) fn scan_command_content(
        &mut self,
        start: usize,
        is_triple: bool,
    ) -> ParseResult<usize> {
        let bytes = self.source.as_bytes();
        let mut pos = start;
        let delimiter: &[u8] = if is_triple { b"```" } else { b"`" };
        let delim_len = delimiter.len();

        while pos < bytes.len() {
            // Check for escape sequence
            if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
                pos += 2; // Skip escape and next char
                continue;
            }

            // Check for closing delimiter
            if pos + delim_len <= bytes.len() && &bytes[pos..pos + delim_len] == delimiter {
                return Ok(pos + delim_len);
            }

            pos += 1;
        }

        // Unterminated command literal
        let span = self.source_map.span(start, bytes.len());
        Err(ParseError::UnterminatedString { span })
    }

    /// Scan string content until closing quote
    pub(crate) fn scan_string_content(
        &mut self,
        start: usize,
        is_triple: bool,
        children: &mut Vec<CstNode>,
    ) -> ParseResult<usize> {
        let bytes = self.source.as_bytes();
        let mut pos = start;
        let mut content_start = start;
        let delimiter: &[u8] = if is_triple { b"\"\"\"" } else { b"\"" };
        let delim_len = delimiter.len();

        while pos < bytes.len() {
            // Check for escape sequence
            if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
                pos += 2; // Skip escape and next char
                continue;
            }

            // Check for interpolation
            if bytes[pos] == b'$' {
                // Add content before $
                if pos > content_start {
                    let span = self.source_map.span(content_start, pos);
                    let text = &self.source[content_start..pos];
                    children.push(CstNode::leaf(NodeKind::Content, span, text));
                }

                // Parse interpolation
                let interp = self.parse_string_interpolation(pos)?;
                pos = interp.span.end;
                content_start = pos;
                children.push(interp);
                continue;
            }

            // Check for closing delimiter
            if pos + delim_len <= bytes.len() && &bytes[pos..pos + delim_len] == delimiter {
                // Add remaining content
                if pos > content_start {
                    let span = self.source_map.span(content_start, pos);
                    let text = &self.source[content_start..pos];
                    children.push(CstNode::leaf(NodeKind::Content, span, text));
                }
                return Ok(pos + delim_len);
            }

            pos += 1;
        }

        Err(ParseError::UnterminatedString {
            span: self.source_map.span(start, pos),
        })
    }

    /// Parse string interpolation ($x or $(expr))
    pub(crate) fn parse_string_interpolation(&mut self, start: usize) -> ParseResult<CstNode> {
        let bytes = self.source.as_bytes();
        let mut pos = start + 1; // Skip $

        if pos >= bytes.len() {
            return Err(ParseError::invalid_syntax(
                "unexpected end of string after $",
                self.source_map.span(start, pos),
            ));
        }

        if bytes[pos] == b'(' {
            // $(expr) - find matching )
            let mut depth = 1;
            pos += 1;
            while pos < bytes.len() && depth > 0 {
                match bytes[pos] {
                    b'(' => depth += 1,
                    b')' => depth -= 1,
                    _ => {}
                }
                pos += 1;
            }
            let span = self.source_map.span(start, pos);
            let text = &self.source[start..pos];
            Ok(CstNode::leaf(NodeKind::StringInterpolation, span, text))
        } else {
            // $identifier — In string interpolation, `!` is NOT part of the identifier.
            // Julia treats only alphanumerics and `_` as valid identifier characters
            // in $-interpolation context. (Issue #2130)
            while pos < bytes.len() && is_interpolation_ident_continue(bytes[pos]) {
                pos += 1;
            }
            let span = self.source_map.span(start, pos);
            let text = &self.source[start..pos];
            Ok(CstNode::leaf(NodeKind::StringInterpolation, span, text))
        }
    }

    /// Parse an identifier
    pub(crate) fn parse_identifier(&mut self) -> ParseResult<CstNode> {
        let token = self.advance().unwrap();
        Ok(CstNode::leaf(NodeKind::Identifier, token.span, token.text))
    }
}

/// Check if a byte is a valid identifier continuation in string interpolation context.
/// Unlike general identifiers, `!` is NOT valid in $-interpolation.
/// In Julia, `"$name!"` interpolates `name` and `!` is literal text. (Issue #2130)
fn is_interpolation_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
