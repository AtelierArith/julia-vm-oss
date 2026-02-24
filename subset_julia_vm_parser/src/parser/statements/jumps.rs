//! Jump statement parsers (return, break, continue)

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
        if !self.check(&Token::Newline)
            && !self.check(&Token::Semicolon)
            && !self.is_at_end()
            && !self.check(&Token::KwEnd)
            && !self.check(&Token::KwElse)
            && !self.check(&Token::KwElseif)
            && !self.check(&Token::KwCatch)
            && !self.check(&Token::KwFinally)
        {
            let first = self.parse_expression()?;

            // Check for comma — bare comma return: return a, b => return (a, b)
            // In Julia, `return a, b` is syntactic sugar for `return (a, b)`.
            if self.check(&Token::Comma) {
                let tuple_start = first.span.start;
                let mut elements = vec![first];
                while self.check(&Token::Comma) {
                    self.advance(); // consume comma
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

    /// Parse break statement
    pub(crate) fn parse_break_statement(&mut self) -> ParseResult<CstNode> {
        let token = self.expect(Token::KwBreak)?;
        Ok(CstNode::leaf(NodeKind::BreakStatement, token.span, "break"))
    }

    /// Parse continue statement
    pub(crate) fn parse_continue_statement(&mut self) -> ParseResult<CstNode> {
        let token = self.expect(Token::KwContinue)?;
        Ok(CstNode::leaf(
            NodeKind::ContinueStatement,
            token.span,
            "continue",
        ))
    }
}
