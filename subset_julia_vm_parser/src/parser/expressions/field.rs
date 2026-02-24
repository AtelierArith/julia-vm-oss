//! Field expression parsers

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

        // Check for quoted operator: Base.:+ or Base.:- or Base.:(==) etc.
        if self.check(&Token::Colon) {
            let colon_start = self.current.as_ref().unwrap().span.start;
            self.advance(); // consume ':'

            // The next token should be an operator, identifier, or parenthesized operator
            let token = self.current.as_ref().ok_or_else(|| {
                ParseError::unexpected_eof("operator after ':'", self.current_span())
            })?;

            if token.token.is_operator() {
                let op_token = self.advance().unwrap();
                let op_span = self.source_map.span(colon_start, op_token.span.end);
                let field = CstNode::leaf(NodeKind::QuoteExpression, op_span, op_token.text);
                let span = self.source_map.span(start, op_span.end);
                return Ok(CstNode::with_children(
                    NodeKind::FieldExpression,
                    span,
                    vec![object, field],
                ));
            }

            // Not an operator, might be :symbol
            if let Token::Identifier = &token.token {
                let ident_token = self.advance().unwrap();
                let sym_span = self.source_map.span(colon_start, ident_token.span.end);
                let field = CstNode::leaf(NodeKind::QuoteExpression, sym_span, ident_token.text);
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

                if op_token.token.is_operator() {
                    let op = self.advance().unwrap();
                    let rparen = self.expect(Token::RParen)?;

                    let op_span = self.source_map.span(colon_start, rparen.span.end);
                    let field = CstNode::leaf(NodeKind::QuoteExpression, op_span, op.text);
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

        // Regular field access
        let field = self.parse_identifier()?;
        let span = self.source_map.span(start, field.span.end);
        Ok(CstNode::with_children(
            NodeKind::FieldExpression,
            span,
            vec![object, field],
        ))
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
