//! Control flow statement parsers (if, for, while, try, begin, let, quote)

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    // ==================== Control Flow Statements ====================

    /// Parse if statement: if cond body [elseif cond body]* [else body] end
    pub(crate) fn parse_if_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwIf)?;
        let start = start_token.span.start;

        let condition = self.parse_expression()?;

        // Skip optional newline/semicolon after condition
        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let then_block = self.parse_block_until(&[Token::KwElseif, Token::KwElse, Token::KwEnd])?;

        let mut children = vec![condition, then_block];

        // Parse elseif clauses
        while self.check(&Token::KwElseif) {
            children.push(self.parse_elseif_clause()?);
        }

        // Parse else clause
        if self.check(&Token::KwElse) {
            children.push(self.parse_else_clause()?);
        }

        let end_token = self.expect(Token::KwEnd)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::IfStatement,
            span,
            children,
        ))
    }

    /// Parse elseif clause
    pub(crate) fn parse_elseif_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwElseif)?;
        let start = start_token.span.start;

        let condition = self.parse_expression()?;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until(&[Token::KwElseif, Token::KwElse, Token::KwEnd])?;

        let end = body.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ElseifClause,
            span,
            vec![condition, body],
        ))
    }

    /// Parse else clause
    pub(crate) fn parse_else_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwElse)?;
        let start = start_token.span.start;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until(&[Token::KwEnd])?;

        let end = body.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ElseClause,
            span,
            vec![body],
        ))
    }

    /// Parse for statement: for var in iter[, var in iter]* body end
    pub(crate) fn parse_for_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwFor)?;
        let start = start_token.span.start;

        // Parse first binding
        let mut bindings = vec![self.parse_for_binding()?];

        // Parse additional bindings separated by comma
        while self.check(&Token::Comma) {
            self.advance(); // consume comma
            bindings.push(self.parse_for_binding()?);
        }

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        let end_token = self.expect(Token::KwEnd)?;

        let span = self.source_map.span(start, end_token.span.end);
        let mut children = bindings;
        children.push(body);
        Ok(CstNode::with_children(
            NodeKind::ForStatement,
            span,
            children,
        ))
    }

    /// Parse while statement: while cond body end
    pub(crate) fn parse_while_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwWhile)?;
        let start = start_token.span.start;

        let condition = self.parse_expression()?;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        let end_token = self.expect(Token::KwEnd)?;

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::WhileStatement,
            span,
            vec![condition, body],
        ))
    }

    /// Parse try statement: try body [catch [var] body] [else body] [finally body] end
    /// The 'else' clause (Julia 1.8+) runs if no exception was thrown
    pub(crate) fn parse_try_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwTry)?;
        let start = start_token.span.start;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let try_block = self.parse_block_until(&[
            Token::KwCatch,
            Token::KwElse,
            Token::KwFinally,
            Token::KwEnd,
        ])?;
        let mut children = vec![try_block];

        // Parse catch clause
        if self.check(&Token::KwCatch) {
            children.push(self.parse_try_catch_clause()?);
        }

        // Parse else clause (Julia 1.8+)
        if self.check(&Token::KwElse) {
            children.push(self.parse_try_else_clause()?);
        }

        // Parse finally clause
        if self.check(&Token::KwFinally) {
            children.push(self.parse_finally_clause()?);
        }

        let end_token = self.expect(Token::KwEnd)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TryStatement,
            span,
            children,
        ))
    }

    /// Parse catch clause in try statement
    pub(crate) fn parse_try_catch_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwCatch)?;
        let start = start_token.span.start;

        let mut children = Vec::new();

        // Optional exception variable
        if self.check(&Token::Identifier) {
            children.push(self.parse_identifier()?);
        }

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until(&[Token::KwElse, Token::KwFinally, Token::KwEnd])?;
        children.push(body);

        let end = children.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::CatchClause,
            span,
            children,
        ))
    }

    /// Parse else clause in try statement (Julia 1.8+)
    pub(crate) fn parse_try_else_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwElse)?;
        let start = start_token.span.start;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until(&[Token::KwFinally, Token::KwEnd])?;

        let end = body.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ElseClause,
            span,
            vec![body],
        ))
    }

    /// Parse finally clause
    pub(crate) fn parse_finally_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwFinally)?;
        let start = start_token.span.start;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until(&[Token::KwEnd])?;

        let end = body.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::FinallyClause,
            span,
            vec![body],
        ))
    }

    /// Parse begin block: begin body end
    pub(crate) fn parse_begin_block(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwBegin)?;
        let start = start_token.span.start;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        let end_token = self.expect(Token::KwEnd)?;

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::BeginBlock,
            span,
            vec![body],
        ))
    }

    /// Parse let expression: let bindings body end
    pub(crate) fn parse_let_expression(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwLet)?;
        let start = start_token.span.start;

        let mut children = Vec::new();

        // Parse bindings (var = value, ...)
        if !self.check(&Token::Newline)
            && !self.check(&Token::Semicolon)
            && !self.check(&Token::KwEnd)
        {
            children.push(self.parse_let_bindings()?);
        }

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        children.push(body);

        let end_token = self.expect(Token::KwEnd)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::LetExpression,
            span,
            children,
        ))
    }

    /// Parse let bindings: var = value, var2 = value2
    pub(crate) fn parse_let_bindings(&mut self) -> ParseResult<CstNode> {
        let first = self.parse_expression()?;
        let start = first.span.start;
        let mut bindings = vec![first];

        while self.check(&Token::Comma) {
            self.advance();
            bindings.push(self.parse_expression()?);
        }

        let end = bindings.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::LetBindings,
            span,
            bindings,
        ))
    }

    /// Parse quote expression: quote body end
    pub(crate) fn parse_quote_expression(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwQuote)?;
        let start = start_token.span.start;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        let end_token = self.expect(Token::KwEnd)?;

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::QuoteExpression,
            span,
            vec![body],
        ))
    }
}
