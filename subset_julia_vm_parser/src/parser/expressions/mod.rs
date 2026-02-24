//! Expression parsing (Pratt parser)
//!
//! Handles:
//! - Binary and unary expressions with precedence climbing
//! - Postfix operations (call, index, field access)
//! - Ternary expressions
//! - Type declarations and parametric types

mod calls;
mod field;
mod index;
mod postfix;
mod primary;
mod types;

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::{Associativity, Precedence, Token};

use super::Parser;

impl<'a> Parser<'a> {
    // ==================== Expression Parsing (Pratt Parser) ====================

    /// Parse an expression (top-level entry point)
    pub(crate) fn parse_expression(&mut self) -> ParseResult<CstNode> {
        self.parse_expression_with_precedence(Precedence::MacroArg)
    }

    /// Parse an expression with minimum precedence (Pratt parser core)
    pub(crate) fn parse_expression_with_precedence(
        &mut self,
        min_prec: Precedence,
    ) -> ParseResult<CstNode> {
        // Parse prefix expression (unary or primary)
        let mut left = self.parse_prefix()?;

        // Parse infix and postfix expressions
        while !self.is_at_end() {
            // Check for postfix operations (call, index, field access)
            if let Some(postfix) = self.try_parse_postfix(&left)? {
                left = postfix;
                continue;
            }

            // Check for ternary operator
            if self.check(&Token::Question) && min_prec <= Precedence::Conditional {
                left = self.parse_ternary(left)?;
                continue;
            }

            // Check for binary operator
            let Some(token) = self.current.as_ref() else {
                break;
            };

            let Some((prec, assoc)) = token.token.binary_precedence() else {
                break;
            };

            // Special case: Don't consume : as range operator inside ternary parsing
            // When min_prec is Conditional, we're parsing a ternary branch and : marks the end
            if token.token == Token::Colon && min_prec == Precedence::Conditional {
                break;
            }

            // Check precedence
            if prec < min_prec {
                break;
            }

            // Consume the operator
            let op_token = self.advance().unwrap();

            // Line continuation: skip newlines after assignment operators
            if op_token.token == Token::Eq || op_token.token.is_compound_assignment() {
                while self.check(&Token::Newline) {
                    self.advance();
                }
            }

            // Special case for 'where' with braced type params: expr where {T, S}
            if op_token.token == Token::KwWhere && self.check(&Token::LBrace) {
                let right = self.parse_braced_type_params()?;
                let span = self.source_map.span(left.span.start, right.span.end);
                left = CstNode::with_children(NodeKind::WhereExpression, span, vec![left, right]);
                continue;
            }

            // Calculate next precedence based on associativity
            let next_prec = match assoc {
                Associativity::Left => Precedence::try_from((prec as i8) + 1).unwrap_or(prec),
                Associativity::Right | Associativity::None => prec,
            };

            // Parse right-hand side
            let right = self.parse_expression_with_precedence(next_prec)?;

            // Create expression node based on operator
            let span = self.source_map.span(left.span.start, right.span.end);

            // Special case for 'where' - create WhereExpression
            if op_token.token == Token::KwWhere {
                left = CstNode::with_children(NodeKind::WhereExpression, span, vec![left, right]);
            } else if op_token.token == Token::Eq {
                // Simple assignment: lhs = rhs
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
                left =
                    CstNode::with_children(NodeKind::Assignment, span, vec![left, op_node, right]);
            } else if op_token.token.is_compound_assignment() {
                // Compound assignment: lhs += rhs, etc.
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
                left = CstNode::with_children(
                    NodeKind::CompoundAssignmentExpression,
                    span,
                    vec![left, op_node, right],
                );
            } else if op_token.token == Token::Colon {
                // Range expression: start:end or start:step:end
                left = CstNode::with_children(NodeKind::RangeExpression, span, vec![left, right]);
            } else if op_token.token == Token::Arrow {
                // Arrow function: x -> expr or (x, y) -> expr
                // left is parameter(s), right is body
                left = CstNode::with_children(
                    NodeKind::ArrowFunctionExpression,
                    span,
                    vec![left, right],
                );
            } else {
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
                left = CstNode::with_children(
                    NodeKind::BinaryExpression,
                    span,
                    vec![left, op_node, right],
                );
            }
        }

        Ok(left)
    }

    /// Parse a prefix expression (unary operator or primary)
    pub(crate) fn parse_prefix(&mut self) -> ParseResult<CstNode> {
        let token = self
            .current
            .as_ref()
            .ok_or_else(|| ParseError::unexpected_eof("expression", self.current_span()))?;

        // Check for dotted operators as expression start: .+([1,2,3]), .-(x, y)
        // This is the broadcast function call syntax where the operator is used as a function
        if token.token.is_dotted_operator() {
            let op_token = self.advance().unwrap();

            // Check if followed by parenthesis - this means it's a broadcast function call
            if self.check(&Token::LParen) {
                let start = op_token.span.start;
                self.advance(); // consume '('

                let mut args = vec![];

                // Parse arguments
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

                // Create the callee node as an Operator node (the base operator)
                let callee = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);

                // Insert callee at the front of args
                let mut all_children = vec![callee];
                all_children.extend(args);

                return Ok(CstNode::with_children(
                    NodeKind::BroadcastCallExpression,
                    span,
                    all_children,
                ));
            } else {
                // Dotted operator not followed by paren is an error
                return Err(ParseError::unexpected_token(
                    op_token.text,
                    "expression (dotted operators require parentheses when used as functions)",
                    op_token.span,
                ));
            }
        }

        // Check for unary operators
        if let Some(_prec) = token.token.unary_precedence() {
            let op_token = self.advance().unwrap();
            // Parse operand: unary binds tighter than binary, but postfix binds tightest
            // So -abs(x) should be -(abs(x)), not (-abs)(x)
            let operand = self.parse_prefix_with_postfix()?;

            let span = self.source_map.span(op_token.span.start, operand.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
            return Ok(CstNode::with_children(
                NodeKind::UnaryExpression,
                span,
                vec![op_node, operand],
            ));
        }

        // Parse primary expression with postfix operations
        self.parse_primary_with_postfix()
    }

    /// Parse a primary expression followed by any postfix operations (call, index, field)
    fn parse_primary_with_postfix(&mut self) -> ParseResult<CstNode> {
        let mut left = self.parse_primary()?;

        // Apply postfix operations
        while !self.is_at_end() {
            if let Some(postfix) = self.try_parse_postfix(&left)? {
                left = postfix;
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse a prefix expression (possibly with nested unary ops) followed by postfix operations
    fn parse_prefix_with_postfix(&mut self) -> ParseResult<CstNode> {
        let token = self
            .current
            .as_ref()
            .ok_or_else(|| ParseError::unexpected_eof("expression", self.current_span()))?;

        // Check for unary operators (handles chained unary: --x, !!x)
        if let Some(_prec) = token.token.unary_precedence() {
            let op_token = self.advance().unwrap();
            let operand = self.parse_prefix_with_postfix()?;

            let span = self.source_map.span(op_token.span.start, operand.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
            return Ok(CstNode::with_children(
                NodeKind::UnaryExpression,
                span,
                vec![op_node, operand],
            ));
        }

        // Parse primary with postfix
        self.parse_primary_with_postfix()
    }
}
