//! Literal parsing for Julia subset
//!
//! Handles parsing of identifiers, macro calls, and literal values.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::Token;

use super::Parser;

impl<'a> Parser<'a> {
    /// Parse identifier, possibly as part of a symbol or qualified name
    pub(crate) fn parse_identifier_or_symbol(&mut self) -> ParseResult<CstNode> {
        self.parse_identifier_like_name()
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

        self.finish_macro_call(start, name.span.end, macro_id)
    }

    pub(crate) fn finish_macro_call(
        &mut self,
        start: usize,
        name_end: usize,
        macro_id: CstNode,
    ) -> ParseResult<CstNode> {
        let is_doc_macro = Self::is_doc_macro_identifier(&macro_id, self.source);
        let mut children = vec![macro_id];

        // Check if immediately followed by '{' (braces argument style).
        // This is used by type-construction macros like `@NamedTuple{a::Int, b}`
        // (mirrors upstream Julia's `:braces` macro argument). The braces are
        // parsed into a CurlyExpression node whose children are the field
        // declarations (`a::Int`, `b`, ...).
        if self.check(&Token::LBrace) {
            let braces = self.parse_macro_braces()?;
            let end = braces.span.end;
            children.push(braces);
            let span = self.source_map.span(start, end);
            return Ok(CstNode::with_children(
                NodeKind::MacrocallExpression,
                span,
                children,
            ));
        }

        // Check if immediately followed by '(' (parenthesized call style)
        // We detect this by checking if the LParen starts right after the macro name
        if self.check(&Token::LParen) {
            if let Some(lparen_token) = self.current.as_ref() {
                // Check for no gap between macro name and '('
                if lparen_token.span.start == name_end {
                    // Parenthesized call style: @macro(args)
                    self.advance(); // consume '('

                    // Inside the parentheses of @macro(...) newlines are
                    // insignificant, just like in any other delimited context.
                    // Increment grouping_depth so expression-level newline
                    // continuation (binary-operator on next line, ternary `:`)
                    // works inside macro argument expressions (Issue #8753).
                    let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
                    self.grouping_depth += 1;

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
                            let arg = if let Some(arg) = self.parse_macro_statement_arg()? {
                                arg
                            } else {
                                self.parse_expression()?
                            };
                            children.push(arg);
                            if !self.check(&Token::Comma) {
                                break;
                            }
                            self.advance();
                        }
                    }

                    // Skip newlines before the closing paren so multi-line
                    // no-trailing-comma calls like:
                    //   @macro(
                    //     arg1,
                    //     arg2
                    //   )
                    // parse correctly (Issue #8753).
                    while self.check(&Token::Newline) {
                        self.advance();
                    }

                    self.grouping_depth -= 1;
                    self.in_ternary_then = saved_in_ternary_then;

                    let rparen = self.expect(Token::RParen)?;
                    let mut end = rparen.span.end;
                    if self.check(&Token::KwDo) {
                        let do_clause = self.parse_do_clause()?;
                        end = do_clause.span.end;
                        children.push(do_clause);
                    }
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
        let mut saw_comma = false;
        while !self.is_at_end()
            && !self.check(&Token::Newline)
            && !self.check(&Token::Semicolon)
            && !self.check(&Token::KwEnd)
            && !self.check(&Token::Comma)
            && !self.check(&Token::RParen)
            && !self.check(&Token::RBracket)
        {
            if self.macro_arg_stops_before_comprehension_for
                && self.check(&Token::KwFor)
                && children.len() > 1
            {
                break;
            }

            if let Some(arg) = self.parse_macro_statement_arg()? {
                let can_have_trailing_args =
                    matches!(arg.kind, NodeKind::ForStatement | NodeKind::WhileStatement);
                children.push(arg);
                if can_have_trailing_args {
                    continue;
                }
                break;
            }

            // Parse expression as argument.
            //
            // Macro arguments are space-separated, so a space before `(` or `[`
            // must NOT fuse into a call/index: `@m foo (bar)` is two arguments
            // (`foo`, `bar`), `@m foo(bar)` is one call argument (Issue #5494).
            // Enable whitespace sensitivity for the top level of this argument;
            // it is cleared again inside any grouping (see `enter_grouping`).
            let saved_space_sensitive = self.macro_arg_space_sensitive;
            self.macro_arg_space_sensitive = true;
            let arg = self.parse_expression();
            self.macro_arg_space_sensitive = saved_space_sensitive;
            let arg = arg?;
            children.push(arg);

            // Consume optional comma between arguments
            if self.check(&Token::Comma) {
                saw_comma = true;
                self.advance();
                while self.check(&Token::Newline) {
                    self.advance();
                }
            }
            // Don't break - let the while condition handle when to stop
            // Julia macros are space-separated, so continue parsing
        }

        if is_doc_macro && self.check(&Token::Newline) {
            self.skip_newlines();
            if let Some(arg) = self.parse_macro_statement_arg()? {
                children.push(arg);
            }
        }

        if saw_comma && children.len() > 1 {
            let args = children.split_off(1);
            let tuple_start = args.first().map(|arg| arg.span.start).unwrap_or(start);
            let tuple_end = args.last().map(|arg| arg.span.end).unwrap_or(tuple_start);
            let tuple = CstNode::with_children(
                NodeKind::TupleExpression,
                self.source_map.span(tuple_start, tuple_end),
                args,
            );
            children.push(tuple);
        }

        let end = self.last_span_end(&children, "macro call always pushes macro_id above")?;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::MacrocallExpression,
            span,
            children,
        ))
    }

    fn is_doc_macro_identifier(macro_id: &CstNode, source: &str) -> bool {
        let Some(name) = macro_id.children.first() else {
            return false;
        };
        Self::rightmost_identifier_text(name, source) == Some("doc")
    }

    fn rightmost_identifier_text<'s>(node: &CstNode, source: &'s str) -> Option<&'s str> {
        match node.kind {
            NodeKind::Identifier => Some(node.text_from_source(source)),
            NodeKind::FieldExpression => node
                .children
                .last()
                .and_then(|child| Self::rightmost_identifier_text(child, source)),
            _ => None,
        }
    }

    fn parse_macro_statement_arg(&mut self) -> ParseResult<Option<CstNode>> {
        let arg = match self.current.as_ref().map(|token| &token.token) {
            Some(Token::KwBegin) => self.parse_begin_block()?,
            Some(Token::KwStruct | Token::KwMutable) => self.parse_struct_definition()?,
            Some(Token::KwPrimitive) => self.parse_primitive_definition()?,
            Some(Token::KwFor) => self.parse_for_statement()?,
            Some(Token::KwWhile) => self.parse_while_statement()?,
            Some(Token::KwIf) => self.parse_if_statement()?,
            Some(Token::KwFunction) => self.parse_function_definition()?,
            Some(Token::KwMacro) => self.parse_macro_definition()?,
            Some(Token::KwConst) => self.parse_const_declaration()?,
            Some(Token::KwUsing) => self.parse_using_statement()?,
            Some(Token::KwImport) => self.parse_import_statement()?,
            Some(Token::KwModule | Token::KwBaremodule) => self.parse_module_definition()?,
            _ => return Ok(None),
        };
        Ok(Some(arg))
    }

    /// Parse a braced macro argument: `{ decl, decl, ... }`.
    ///
    /// Each declaration is parsed as an ordinary expression (e.g. a typed
    /// expression `a::Int` or a bare identifier `b`). The result is a
    /// `CurlyExpression` node whose children are the declarations, which the
    /// lowering phase interprets per-macro (currently only `@NamedTuple`).
    pub(crate) fn parse_macro_braces(&mut self) -> ParseResult<CstNode> {
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, false);
        let start_token = self.expect(Token::LBrace)?;
        let start = start_token.span.start;

        let mut children = Vec::new();
        if !self.check(&Token::RBrace) {
            loop {
                // Skip newlines/semicolons inside the braces.
                while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
                    self.advance();
                }
                if self.check(&Token::RBrace) {
                    break;
                }

                children.push(self.parse_expression()?);

                // Field declarations may be separated by commas, newlines, or
                // semicolons (the latter for forms split across lines).
                if self.check(&Token::Comma)
                    || self.check(&Token::Semicolon)
                    || self.check(&Token::Newline)
                {
                    self.advance();
                } else if !self.check(&Token::RBrace) {
                    break;
                }
            }
        }

        let end_token = self.expect(Token::RBrace)?;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::CurlyExpression,
            span,
            children,
        ))
    }

    // ==================== Literal Parsing ====================

    /// Parse an integer literal
    pub(crate) fn parse_integer_literal(&mut self) -> ParseResult<CstNode> {
        let token = self
            .advance_checked("integer literal token already matched by parse_primary's dispatch")?;
        Ok(CstNode::leaf(NodeKind::IntegerLiteral, token.span))
    }

    /// Parse a float literal
    pub(crate) fn parse_float_literal(&mut self) -> ParseResult<CstNode> {
        let token = self
            .advance_checked("float literal token already matched by parse_primary's dispatch")?;
        Ok(CstNode::leaf(NodeKind::FloatLiteral, token.span))
    }

    /// Parse a boolean literal (true/false)
    pub(crate) fn parse_boolean_literal(&mut self) -> ParseResult<CstNode> {
        let token = self
            .advance_checked("boolean literal token already matched by parse_primary's dispatch")?;
        Ok(CstNode::leaf(NodeKind::BooleanLiteral, token.span))
    }

    /// Parse a character literal
    pub(crate) fn parse_character_literal(&mut self) -> ParseResult<CstNode> {
        let token = self.advance_checked(
            "character literal token already matched by parse_primary's dispatch",
        )?;
        Ok(CstNode::leaf(NodeKind::CharacterLiteral, token.span))
    }

    /// Parse a string literal
    pub(crate) fn parse_string_literal(&mut self) -> ParseResult<CstNode> {
        let start_token =
            self.advance_checked("string quote token already matched by parse_primary's dispatch")?;
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
        let start_token =
            self.advance_checked("backtick token already matched by parse_primary's dispatch")?;
        let is_triple = matches!(start_token.token, Token::TripleBacktick);
        let start = start_token.span.start;

        let content_start = start_token.span.end;

        // Scan for command content until closing backtick, mirroring
        // `scan_string_content`: the outer node's span covers the FULL
        // literal including delimiters (callers such as
        // `parse_prefixed_string_literal` rely on `span.end` pointing
        // exactly past the closing delimiter to detect an adjacent suffix
        // flag, e.g. `x`s`flag`), while the delimiter-free content is
        // carried by a `Content` child (Issue #10126 review: a leaf's
        // `text()` is derived from its span, so without this child a
        // delimiter-inclusive outer span would leak backticks into
        // `--dump-ast --json` output, regressing the pre-#10126
        // delimiter-free command-literal text).
        let mut children = Vec::new();
        let end = self.scan_command_content(content_start, is_triple, &mut children)?;

        // Restart lexer from after the command to synchronize
        self.lexer.restart_from(end);
        self.current = None;
        self.advance(); // Prime with next token

        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::CommandLiteral,
            span,
            children,
        ))
    }

    /// Scan command content until closing backtick, pushing a `Content` leaf
    /// (content only, delimiters excluded) for the scanned text.
    pub(crate) fn scan_command_content(
        &mut self,
        start: usize,
        is_triple: bool,
        children: &mut Vec<CstNode>,
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
                if pos > start {
                    let span = self.source_map.span(start, pos);
                    children.push(CstNode::leaf(NodeKind::Content, span));
                }
                return Ok(pos + delim_len);
            }

            pos += 1;
        }

        // Unterminated command literal
        let span = self.source_map.span(start, bytes.len());
        Err(ParseError::UnterminatedCommand { span })
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
                    children.push(CstNode::leaf(NodeKind::Content, span));
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
                    children.push(CstNode::leaf(NodeKind::Content, span));
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
            Ok(CstNode::leaf(NodeKind::StringInterpolation, span))
        } else {
            // $identifier — upstream Julia treats `!` as a valid identifier
            // continuation character, so `"$name!"` interpolates `name!` and
            // `"$name!x"` interpolates `name!x`. The identifier stops before
            // `!=` (which lexes as the operator), so `"$name!="` interpolates
            // `name`. Issue #2130 previously asserted the opposite; corrected
            // to match upstream julia 1.12 (Issue #10322).
            let ident_start = pos;
            while pos < bytes.len() {
                let b = bytes[pos];
                // A `!` continues the identifier only when it is not the first
                // character and is not the start of `!=`.
                let bang_continues =
                    b == b'!' && pos > ident_start && bytes.get(pos + 1) != Some(&b'=');
                if is_interpolation_ident_continue(b) || bang_continues {
                    pos += 1;
                } else {
                    break;
                }
            }
            let span = self.source_map.span(start, pos);
            Ok(CstNode::leaf(NodeKind::StringInterpolation, span))
        }
    }

    /// Parse an identifier
    ///
    /// Unlike most `self.advance()` call sites in this crate, this one is
    /// genuinely reachable at end-of-input: `parse_identifier` is called
    /// wherever the grammar expects a name (function/struct/module names,
    /// parameters, ...) right after consuming a keyword, with no lookahead
    /// check that a token actually follows (e.g. a truncated `struct` or
    /// `abstract type` with no name). Upstream Julia reports this as a
    /// "premature end of input" `ParseError`; do the same instead of
    /// panicking (Issue #10904).
    pub(crate) fn parse_identifier(&mut self) -> ParseResult<CstNode> {
        let token = self
            .advance()
            .ok_or_else(|| ParseError::unexpected_eof("identifier", self.current_span()))?;
        Ok(CstNode::leaf(NodeKind::Identifier, token.span))
    }

    /// Parse an identifier-like name, including Julia's `var"..."` quoted
    /// identifier spelling.
    pub(crate) fn parse_identifier_like_name(&mut self) -> ParseResult<CstNode> {
        if self.check_adjacent_prefixed_string("var") {
            let prefix = self.parse_identifier()?;
            let prefixed = self.parse_prefixed_string_literal(prefix)?;
            Ok(self.merge_var_quoted_identifier(prefixed))
        } else {
            self.parse_identifier()
        }
    }

    /// Merge a parsed `var"..."` prefixed-string literal into a plain
    /// `Identifier` leaf (Issue #8754). Julia's `var"..."` non-standard
    /// identifier syntax names a binding by the quoted string's exact
    /// content — JuliaSyntax merges the `var` prefix and the string into a
    /// single identifier token. The merged leaf's span covers the FULL
    /// `var"..."` source range so that span-derived reconstruction of
    /// enclosing expressions (e.g. macro-argument re-parsing) stays intact;
    /// name extraction strips the wrapper via [`strip_var_quotes`], which
    /// downstream text readers apply to `Identifier` leaves.
    ///
    /// Returns the node unchanged (a string-macro call downstream) when the
    /// literal is not a plain mergeable string: interpolation children, a
    /// trailing flag identifier, escape sequences, or empty content.
    pub(crate) fn merge_var_quoted_identifier(&self, prefixed: CstNode) -> CstNode {
        if prefixed.kind != NodeKind::PrefixedStringLiteral || prefixed.children.len() != 2 {
            return prefixed;
        }
        let prefix = &prefixed.children[0];
        let string = &prefixed.children[1];
        if prefix.kind != NodeKind::Identifier
            || &self.source[prefix.span.start..prefix.span.end] != "var"
            || string.kind != NodeKind::StringLiteral
            || string.children.iter().any(|c| c.kind != NodeKind::Content)
        {
            return prefixed;
        }
        let text = &self.source[string.span.start..string.span.end];
        let delim = if text.starts_with("\"\"\"") { 3 } else { 1 };
        let content_start = string.span.start + delim;
        let content_end = string.span.end - delim;
        if content_start >= content_end || self.source[content_start..content_end].contains('\\') {
            return prefixed;
        }
        CstNode::leaf(NodeKind::Identifier, prefixed.span)
    }
}

/// Strip the `var"..."` wrapper from a merged var-quoted identifier's raw
/// source text, yielding the identifier name (the quoted content). Ordinary
/// identifier text can never contain a `"`, so this only rewrites leaves
/// produced by [`Parser::merge_var_quoted_identifier`] (Issue #8754).
pub fn strip_var_quotes(text: &str) -> &str {
    if let Some(rest) = text.strip_prefix("var\"\"\"") {
        if let Some(inner) = rest.strip_suffix("\"\"\"") {
            return inner;
        }
    }
    if let Some(rest) = text.strip_prefix("var\"") {
        if let Some(inner) = rest.strip_suffix('"') {
            return inner;
        }
    }
    text
}

/// Check if a byte is an unconditional identifier continuation in string
/// interpolation context. `!` is handled separately at the call site: it
/// continues the identifier only when it is not the first character and is
/// not followed by `=` (matching the upstream lexer, Issue #10322).
fn is_interpolation_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
