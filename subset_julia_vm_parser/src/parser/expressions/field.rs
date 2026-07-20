//! Field expression parsers

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse a field expression (dot access)
    pub(crate) fn parse_field_expression(&mut self, object: CstNode) -> ParseResult<CstNode> {
        let start = object.span.start;
        self.expect(Token::Dot)?;

        // Check for broadcast call: obj.(args)
        if self.check(&Token::LParen) {
            return self.parse_broadcast_call(object);
        }

        if self.check(&Token::LBracket) {
            let field = self.parse_array_or_comprehension()?;
            let span = self.source_map.span(start, field.span.end);
            return Ok(CstNode::with_children(
                NodeKind::FieldExpression,
                span,
                vec![object, field],
            ));
        }

        if self.check(&Token::LBrace) {
            let field = self.parse_braced_type_params()?;
            let span = self.source_map.span(start, field.span.end);
            return Ok(CstNode::with_children(
                NodeKind::FieldExpression,
                span,
                vec![object, field],
            ));
        }

        // Qualified macro call: Base.@nospecializeinfer function f(...) ... end
        // Julia permits module-qualified macro names with the `@` after the dot.
        // Represent the invocation as a normal MacroCall whose identifier span
        // covers only the `@name` portion; lowering can then treat Base compiler
        // annotation macros the same as unqualified annotations.
        if self.check(&Token::At) {
            let at_token = self.expect(Token::At)?;
            let macro_name = self.parse_identifier()?;
            let macro_id_span = self
                .source_map
                .span(at_token.span.start, macro_name.span.end);
            let macro_id =
                CstNode::with_children(NodeKind::MacroIdentifier, macro_id_span, vec![macro_name]);
            return self.finish_macro_call(start, macro_id_span.end, macro_id);
        }

        // Check for quoted operator: Base.:+ or Base.:- or Base.:(==) etc.
        if self.check(&Token::Colon) {
            let colon_start = self
                .current_checked("Colon token already matched by check() above")?
                .span
                .start;
            self.advance(); // consume ':'

            // The next token should be an operator, identifier, or parenthesized operator
            let token = self.current.as_ref().ok_or_else(|| {
                ParseError::unexpected_eof("operator after ':'", self.current_span())
            })?;

            // Bare `:op` / `:isa` / `:in`. `isa`/`in` are keyword tokens but
            // are valid first-class operator names (Issue #5115).
            if token.token.is_quoted_operator_symbol() {
                let op_token = self.advance_checked(
                    "quoted-operator token already matched by is_quoted_operator_symbol() above",
                )?;
                let op_span = self.source_map.span(colon_start, op_token.span.end);
                let field = CstNode::leaf(NodeKind::QuoteExpression, op_span);
                let span = self.source_map.span(start, op_span.end);
                return Ok(CstNode::with_children(
                    NodeKind::FieldExpression,
                    span,
                    vec![object, field],
                ));
            }

            // Not an operator, might be :symbol
            if let Token::Identifier = &token.token {
                if self.check_adjacent_prefixed_string("var") {
                    let prefix = self.parse_identifier()?;
                    let prefixed = self.parse_prefixed_string_literal(prefix)?;
                    let prefixed = self.merge_var_quoted_identifier(prefixed);
                    let sym_span = self.source_map.span(colon_start, prefixed.span.end);
                    let field =
                        CstNode::with_children(NodeKind::QuoteExpression, sym_span, vec![prefixed]);
                    let span = self.source_map.span(start, sym_span.end);
                    return Ok(CstNode::with_children(
                        NodeKind::FieldExpression,
                        span,
                        vec![object, field],
                    ));
                }
                let ident_token = self.advance_checked(
                    "Identifier token already matched by the `if let Token::Identifier` guard above",
                )?;
                let mut end = ident_token.span.end;
                while self
                    .current
                    .as_ref()
                    .is_some_and(|next| next.token == Token::Identifier && next.span.start == end)
                {
                    let suffix = self.advance_checked(
                        "Identifier token already matched by the while condition above",
                    )?;
                    end = suffix.span.end;
                }
                let sym_span = self.source_map.span(colon_start, end);
                let field = CstNode::leaf(NodeKind::QuoteExpression, sym_span);
                let span = self.source_map.span(start, sym_span.end);
                return Ok(CstNode::with_children(
                    NodeKind::FieldExpression,
                    span,
                    vec![object, field],
                ));
            }

            // Check for parenthesized operator: :(==), :(+), etc.
            if let Token::LParen = &token.token {
                self.advance(); // consume '('

                // Expect an operator inside
                let op_token = self.current.as_ref().ok_or_else(|| {
                    ParseError::unexpected_eof("operator in :(op)", self.current_span())
                })?;

                // `:(op)` / `:(isa)` / `:(in)`. `isa`/`in` are keyword tokens
                // but are valid first-class operator names (Issue #5115).
                if op_token.token.is_quoted_operator_symbol() {
                    self.advance_checked(
                        "quoted-operator token already matched by is_quoted_operator_symbol() above",
                    )?;
                    let rparen = self.expect(Token::RParen)?;

                    let op_span = self.source_map.span(colon_start, rparen.span.end);
                    let field = CstNode::leaf(NodeKind::QuoteExpression, op_span);
                    let span = self.source_map.span(start, op_span.end);
                    return Ok(CstNode::with_children(
                        NodeKind::FieldExpression,
                        span,
                        vec![object, field],
                    ));
                }

                return Err(ParseError::unexpected_token(
                    op_token.text,
                    "operator in :(op)",
                    op_token.span,
                ));
            }

            return Err(ParseError::unexpected_token(
                token.text,
                "operator or identifier after ':'",
                token.span,
            ));
        }

        // Check for string field access: df."column name"
        if self.check(&Token::DoubleQuote) || self.check(&Token::TripleDoubleQuote) {
            let field = self.parse_string_literal()?;
            let span = self.source_map.span(start, field.span.end);
            return Ok(CstNode::with_children(
                NodeKind::FieldExpression,
                span,
                vec![object, field],
            ));
        }

        if self.check(&Token::Dollar) {
            let field = self.parse_prefix()?;
            let span = self.source_map.span(start, field.span.end);
            return Ok(CstNode::with_children(
                NodeKind::FieldExpression,
                span,
                vec![object, field],
            ));
        }

        // Regular field access
        let field = self.parse_field_identifier()?;
        let span = self.source_map.span(start, field.span.end);
        Ok(CstNode::with_children(
            NodeKind::FieldExpression,
            span,
            vec![object, field],
        ))
    }

    /// Parse the field name after a `.` in a field expression (`obj.field`).
    ///
    /// This is genuinely reachable at end-of-input, unlike most
    /// `self.advance()` call sites in this crate: `parse_field_expression`'s
    /// fallthrough case reaches this function for *any* token after the dot,
    /// including none at all (e.g. a truncated `a.`), with no lookahead
    /// check first. Upstream Julia reports this as a "premature end of
    /// input" `ParseError`; do the same instead of panicking (Issue #10904).
    fn parse_field_identifier(&mut self) -> ParseResult<CstNode> {
        if self.check_adjacent_prefixed_string("var") {
            return self.parse_identifier_like_name();
        }

        let token = self.advance().ok_or_else(|| {
            ParseError::unexpected_eof("field name after '.'", self.current_span())
        })?;
        let start = token.span.start;
        let mut end = token.span.end;
        while self
            .current
            .as_ref()
            .is_some_and(|next| next.token == Token::Identifier && next.span.start == end)
        {
            let suffix = self
                .advance_checked("Identifier token already matched by the while condition above")?;
            end = suffix.span.end;
        }
        let span = self.source_map.span(start, end);
        Ok(CstNode::leaf(NodeKind::Identifier, span))
    }

    /// Parse a broadcast call expression: f.(args)
    pub(crate) fn parse_broadcast_call(&mut self, callee: CstNode) -> ParseResult<CstNode> {
        let start = callee.span.start;
        self.expect(Token::LParen)?;

        let mut args = vec![callee];

        if !self.check(&Token::RParen) {
            loop {
                while self.check(&Token::Newline) {
                    self.advance();
                }
                if self.check(&Token::RParen) {
                    break;
                }
                args.push(self.parse_expression()?);
                if !self.check(&Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        let end_token = self.expect(Token::RParen)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::BroadcastCallExpression,
            span,
            args,
        ))
    }
}
