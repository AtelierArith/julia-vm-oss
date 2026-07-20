//! Jump statement parsers (return, break, continue)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    // ==================== Jump Statements ====================

    /// Parse return statement: return [expr] or return [expr, expr, ...]
    /// In Julia, `return a, b` is equivalent to `return (a, b)` (implicit tuple).
    pub(crate) fn parse_return_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwReturn)?;
        let start = start_token.span.start;

        let mut children = Vec::new();

        // Optional return value
        // Issue #8759: `return` with no value inside `(expr; return)` — the `)` closes
        // the parenthesized block and must terminate the `return` statement without an
        // expression.  Also guard `]` / `}` for symmetry.
        if !self.check(&Token::Newline)
            && !self.check(&Token::Semicolon)
            && !self.check(&Token::RParen)
            && !self.check(&Token::RBracket)
            && !self.check(&Token::RBrace)
            && !self.is_at_end()
            && !self.check(&Token::KwEnd)
            && !self.check(&Token::KwElse)
            && !self.check(&Token::KwElseif)
            && !self.check(&Token::KwCatch)
            && !self.check(&Token::KwFinally)
            && !self.check(&Token::RParen)
            && !self.check(&Token::RBracket)
            && !self.check(&Token::RBrace)
        {
            let first = self.parse_expression()?;

            // Check for comma — bare comma return: return a, b => return (a, b)
            // In Julia, `return a, b` is syntactic sugar for `return (a, b)`.
            if self.check(&Token::Comma) {
                let tuple_start = first.span.start;
                let mut elements = vec![first];
                while self.check(&Token::Comma) {
                    self.advance(); // consume comma
                    while self.check(&Token::Newline) {
                        self.advance();
                    }
                    if self.check(&Token::Semicolon)
                        || self.check(&Token::RParen)
                        || self.check(&Token::RBracket)
                        || self.check(&Token::RBrace)
                        || self.is_at_end()
                        || self.check(&Token::KwEnd)
                        || self.check(&Token::KwElse)
                        || self.check(&Token::KwElseif)
                        || self.check(&Token::KwCatch)
                        || self.check(&Token::KwFinally)
                    {
                        break;
                    }
                    elements.push(self.parse_expression()?);
                }
                let tuple_end = elements.last().map(|e| e.span.end).unwrap_or(tuple_start);
                let tuple_span = self.source_map.span(tuple_start, tuple_end);
                children.push(CstNode::with_children(
                    NodeKind::TupleExpression,
                    tuple_span,
                    elements,
                ));
            } else {
                children.push(first);
            }
        }

        let end = children
            .last()
            .map(|c| c.span.end)
            .unwrap_or(start_token.span.end);
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ReturnStatement,
            span,
            children,
        ))
    }

    fn jump_argument_terminates(&self) -> bool {
        self.check(&Token::Newline)
            || self.check(&Token::Semicolon)
            || self.check(&Token::Comma)
            || self.check(&Token::RParen)
            || self.check(&Token::RBracket)
            || self.check(&Token::RBrace)
            || self.check(&Token::Colon)
            || self.check(&Token::KwEnd)
            || self.check(&Token::KwElse)
            || self.check(&Token::KwElseif)
            || self.check(&Token::KwCatch)
            || self.check(&Token::KwFinally)
            || self.is_at_end()
    }

    /// Parse break statement: `break`, `break label`, or `break label value`.
    pub(crate) fn parse_break_statement(&mut self) -> ParseResult<CstNode> {
        let token = self.expect(Token::KwBreak)?;
        let mut children = Vec::new();

        if self.check(&Token::Identifier) {
            children.push(self.parse_identifier()?);

            if !self.jump_argument_terminates() {
                children.push(self.parse_expression()?);
            }
        }

        if children.is_empty() {
            Ok(CstNode::leaf(NodeKind::BreakStatement, token.span))
        } else {
            let end = children
                .last()
                .map(|child| child.span.end)
                .unwrap_or(token.span.end);
            let span = self.source_map.span(token.span.start, end);
            Ok(CstNode::with_children(
                NodeKind::BreakStatement,
                span,
                children,
            ))
        }
    }

    /// Parse continue statement: `continue` or `continue label`.
    pub(crate) fn parse_continue_statement(&mut self) -> ParseResult<CstNode> {
        let token = self.expect(Token::KwContinue)?;
        if self.check(&Token::Identifier) {
            let label = self.parse_identifier()?;
            let span = self.source_map.span(token.span.start, label.span.end);
            Ok(CstNode::with_children(
                NodeKind::ContinueStatement,
                span,
                vec![label],
            ))
        } else {
            Ok(CstNode::leaf(NodeKind::ContinueStatement, token.span))
        }
    }
}
