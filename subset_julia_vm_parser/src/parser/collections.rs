//! Collection parsing for Julia subset
//!
//! Handles parsing of tuples, arrays, comprehensions, and matrices.

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::Token;

use super::Parser;

impl<'a> Parser<'a> {
    /// Parse parenthesized expression or tuple
    pub(crate) fn parse_parenthesized_or_tuple(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LParen)?;
        let start = start_token.span.start;

        // Check for empty tuple
        if self.check(&Token::RParen) {
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            return Ok(CstNode::new(NodeKind::TupleExpression, span));
        }

        // Check for operator as value: (+), (-), (*), etc.
        // Look ahead: is it `(operator)`?
        if let Some(token) = &self.current {
            if token.token.is_operator() {
                // Peek at next token to see if it's )
                if let Some(next) = self.peek_next() {
                    if next == Token::RParen {
                        // It's an operator as value
                        let op_token = self.advance().unwrap();
                        let end_token = self.advance().unwrap();
                        let span = self.source_map.span(start, end_token.span.end);
                        let op_span = op_token.span;
                        let op_node = CstNode::leaf(NodeKind::Operator, op_span, op_token.text);
                        return Ok(CstNode::with_children(
                            NodeKind::ParenthesizedExpression,
                            span,
                            vec![op_node],
                        ));
                    }
                }
            }
        }

        // Parse first expression
        let first = self.parse_expression()?;

        // Check for generator expression: (expr for x in iter)
        if self.check(&Token::KwFor) {
            return self.parse_generator_rest(start, first);
        }

        // Check for comma (tuple) or closing paren (parenthesized)
        if self.check(&Token::Comma) {
            // It's a tuple
            let mut elements = vec![first];
            while self.check(&Token::Comma) {
                self.advance(); // consume comma

                // Allow trailing comma
                if self.check(&Token::RParen) {
                    break;
                }

                // Skip newlines
                while self.check(&Token::Newline) {
                    self.advance();
                }

                elements.push(self.parse_expression()?);
            }

            let end_token = self.expect(Token::RParen)?;
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::TupleExpression,
                span,
                elements,
            ))
        } else {
            // It's a parenthesized expression
            let end_token = self.expect(Token::RParen)?;
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::ParenthesizedExpression,
                span,
                vec![first],
            ))
        }
    }

    /// Parse rest of generator expression (expr for ...)
    pub(crate) fn parse_generator_rest(
        &mut self,
        start: usize,
        expr: CstNode,
    ) -> ParseResult<CstNode> {
        let mut children = vec![expr];

        // Parse for clause(s)
        while self.check(&Token::KwFor) {
            children.push(self.parse_for_clause()?);
        }

        // Parse optional if clause
        if self.check(&Token::KwIf) {
            children.push(self.parse_if_clause()?);
        }

        let end_token = self.expect(Token::RParen)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(NodeKind::Generator, span, children))
    }

    /// Parse array literal or comprehension
    pub(crate) fn parse_array_or_comprehension(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LBracket)?;
        let start = start_token.span.start;

        // Check for empty array
        if self.check(&Token::RBracket) {
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            return Ok(CstNode::new(NodeKind::VectorExpression, span));
        }

        // Parse first element
        let first = self.parse_expression()?;

        // Check what follows
        if self.check(&Token::KwFor) {
            // Comprehension: [expr for x in iter]
            self.parse_comprehension_rest(start, first)
        } else if self.check(&Token::Comma) {
            // Vector: [a, b, c]
            self.parse_vector_rest(start, first)
        } else if self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            // Matrix: [a b; c d] or [a b\n c d]
            self.parse_matrix_rest(start, first)
        } else if self.check(&Token::RBracket) {
            // Single element vector
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::VectorExpression,
                span,
                vec![first],
            ))
        } else {
            // Could be matrix row: [a b c]
            self.parse_matrix_row_rest(start, first)
        }
    }

    /// Parse rest of vector [first, ...]
    pub(crate) fn parse_vector_rest(
        &mut self,
        start: usize,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let mut elements = vec![first];

        while self.check(&Token::Comma) {
            self.advance(); // consume comma

            // Skip newlines after comma (line continuation in arrays)
            while self.check(&Token::Newline) {
                self.advance();
            }

            // Allow trailing comma
            if self.check(&Token::RBracket) {
                break;
            }

            elements.push(self.parse_expression()?);
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::VectorExpression,
            span,
            elements,
        ))
    }

    /// Parse rest of comprehension [first for ...]
    pub(crate) fn parse_comprehension_rest(
        &mut self,
        start: usize,
        expr: CstNode,
    ) -> ParseResult<CstNode> {
        let mut children = vec![expr];

        // Parse for clause(s)
        while self.check(&Token::KwFor) {
            children.push(self.parse_for_clause()?);
        }

        // Parse optional if clause
        if self.check(&Token::KwIf) {
            children.push(self.parse_if_clause()?);
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::ComprehensionExpression,
            span,
            children,
        ))
    }

    /// Parse for clause in comprehension
    /// Supports both:
    /// - Single binding: for x in iter
    /// - Multiple bindings (2D): for x in iter, y in iter
    pub(crate) fn parse_for_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwFor)?;
        let start = start_token.span.start;

        let mut bindings = vec![self.parse_for_binding()?];

        // Parse additional comma-separated bindings (2D comprehension)
        // Check if comma is followed by identifier (next binding) vs end of clause
        while self.check(&Token::Comma) {
            // Peek at token after comma to see if it looks like another binding
            if let Some(next) = self.peek_next() {
                if next == Token::Identifier {
                    self.advance(); // consume comma
                    bindings.push(self.parse_for_binding()?);
                    continue;
                }
            }
            break;
        }

        let end = bindings.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::ForClause, span, bindings))
    }

    /// Parse if clause in comprehension
    pub(crate) fn parse_if_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwIf)?;
        let start = start_token.span.start;

        let condition = self.parse_expression()?;

        let end = condition.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::IfClause,
            span,
            vec![condition],
        ))
    }

    /// Parse for binding: [outer] var in/= expr
    /// Also supports tuple destructuring: for (a, b) in expr
    pub(crate) fn parse_for_binding(&mut self) -> ParseResult<CstNode> {
        // Check for 'outer' modifier
        let has_outer = self.check(&Token::KwOuter);
        let start = self.current.as_ref().map(|t| t.span.start).unwrap_or(0);

        if has_outer {
            self.advance(); // consume 'outer'
        }

        // Check for tuple pattern: (a, b, ...)
        let var = if self.check(&Token::LParen) {
            self.parse_tuple_pattern()?
        } else {
            self.parse_identifier()?
        };

        // Expect 'in' or '=' or '∈'
        if !self.check_any(&[Token::KwIn, Token::Eq, Token::ElementOf]) {
            return Err(ParseError::unexpected_token(
                self.current
                    .as_ref()
                    .map(|t| t.text.to_string())
                    .unwrap_or_default(),
                "'in' or '='",
                self.current_span(),
            ));
        }
        self.advance(); // consume in/=/∈

        let iter = self.parse_expression()?;
        let end = iter.span.end;

        let span = self.source_map.span(start, end);
        let mut children = vec![var, iter];

        // If outer, add a marker node at the beginning
        if has_outer {
            let outer_marker = CstNode::leaf(
                NodeKind::Identifier,
                self.source_map.span(start, start + 5), // "outer" is 5 chars
                "outer".to_string(),
            );
            children.insert(0, outer_marker);
        }

        Ok(CstNode::with_children(NodeKind::ForBinding, span, children))
    }

    /// Parse tuple pattern for destructuring: (a, b, ...)
    /// Returns a TupleExpression containing Identifiers
    pub(crate) fn parse_tuple_pattern(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LParen)?;
        let start = start_token.span.start;
        let mut elements = Vec::new();

        // Parse first identifier
        elements.push(self.parse_identifier()?);

        // Parse remaining comma-separated identifiers
        while self.check(&Token::Comma) {
            self.advance(); // consume comma

            // Allow trailing comma
            if self.check(&Token::RParen) {
                break;
            }

            elements.push(self.parse_identifier()?);
        }

        let end_token = self.expect(Token::RParen)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TupleExpression,
            span,
            elements,
        ))
    }

    /// Parse rest of matrix [first row; ...]
    pub(crate) fn parse_matrix_rest(
        &mut self,
        start: usize,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        // First element is part of first row
        let mut first_row = vec![first];

        // Parse rest of first row (space-separated)
        while !self.check_any(&[Token::Semicolon, Token::Newline, Token::RBracket])
            && !self.is_at_end()
        {
            first_row.push(self.parse_expression()?);
        }

        let mut rows = vec![CstNode::with_children(
            NodeKind::MatrixRow,
            self.source_map
                .span(first_row[0].span.start, first_row.last().unwrap().span.end),
            first_row,
        )];

        // Parse remaining rows
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            self.advance(); // consume ; or newline

            // Skip extra newlines
            while self.check(&Token::Newline) {
                self.advance();
            }

            if self.check(&Token::RBracket) {
                break;
            }

            rows.push(self.parse_matrix_row()?);
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::MatrixExpression,
            span,
            rows,
        ))
    }

    /// Parse matrix row (single row starting with first element already parsed)
    pub(crate) fn parse_matrix_row_rest(
        &mut self,
        start: usize,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let mut elements = vec![first];

        // Parse space-separated elements
        while !self.check_any(&[Token::Semicolon, Token::Newline, Token::RBracket])
            && !self.is_at_end()
        {
            elements.push(self.parse_expression()?);
        }

        // If just one row ending with ], it's a row vector
        if self.check(&Token::RBracket) {
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            let row = CstNode::with_children(
                NodeKind::MatrixRow,
                self.source_map
                    .span(elements[0].span.start, elements.last().unwrap().span.end),
                elements,
            );
            return Ok(CstNode::with_children(
                NodeKind::MatrixExpression,
                span,
                vec![row],
            ));
        }

        // Create first row from all collected elements
        let first_row = CstNode::with_children(
            NodeKind::MatrixRow,
            self.source_map
                .span(elements[0].span.start, elements.last().unwrap().span.end),
            elements,
        );

        let mut rows = vec![first_row];

        // Parse remaining rows
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            self.advance(); // consume ; or newline

            // Skip extra newlines
            while self.check(&Token::Newline) {
                self.advance();
            }

            if self.check(&Token::RBracket) {
                break;
            }

            rows.push(self.parse_matrix_row()?);
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::MatrixExpression,
            span,
            rows,
        ))
    }

    /// Parse a single matrix row
    pub(crate) fn parse_matrix_row(&mut self) -> ParseResult<CstNode> {
        let first = self.parse_expression()?;
        let start = first.span.start;
        let mut elements = vec![first];

        // Parse space-separated elements
        while !self.check_any(&[Token::Semicolon, Token::Newline, Token::RBracket])
            && !self.is_at_end()
        {
            elements.push(self.parse_expression()?);
        }

        let end = elements.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::MatrixRow, span, elements))
    }
}
