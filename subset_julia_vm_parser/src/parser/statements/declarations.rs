//! Variable declaration parsers (const, global, local)

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::{Precedence, Token};

use crate::parser::Parser;

impl<'a> Parser<'a> {
    // ==================== Variable Declarations ====================

    /// Parse const declaration: const x = value or const x, y = 1, 2
    pub(crate) fn parse_const_declaration(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwConst)?;
        let start = start_token.span.start;

        // Parse a bare tuple assignment expression
        let expr = self.parse_bare_tuple_assignment()?;
        let end = expr.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ConstDeclaration,
            span,
            vec![expr],
        ))
    }

    /// Parse bare tuple assignment: x, y = 1, 2 or x = 1
    /// This handles comma-separated expressions without parentheses
    pub(crate) fn parse_bare_tuple_assignment(&mut self) -> ParseResult<CstNode> {
        // Parse first expression (stopping at comma and assignment)
        let first = self.parse_expression_with_precedence(Precedence::Conditional)?;

        if !self.check(&Token::Comma) {
            // No comma - check for simple assignment
            if self.check(&Token::Eq) {
                let op_token = self.advance().unwrap();
                let op_span = op_token.span;
                let op_node = CstNode::new(NodeKind::Operator, op_span);

                // Line continuation: skip newlines after =
                while self.check(&Token::Newline) {
                    self.advance();
                }

                // Parse right side (which might also be a bare tuple)
                let right = self.parse_bare_tuple_or_expr()?;

                let span = self.source_map.span(first.span.start, right.span.end);
                return Ok(CstNode::with_children(
                    NodeKind::BinaryExpression,
                    span,
                    vec![first, op_node, right],
                ));
            }
            return Ok(first);
        }

        // We have a comma - parse as bare tuple on left side
        let mut left_elements = vec![first];
        while self.check(&Token::Comma) {
            self.advance();
            // Parse next element (stopping at comma and assignment)
            left_elements.push(self.parse_expression_with_precedence(Precedence::Conditional)?);
        }

        // Create tuple for left side
        let left_start = left_elements.first().unwrap().span.start;
        let left_end = left_elements.last().unwrap().span.end;
        let left_span = self.source_map.span(left_start, left_end);
        let left = CstNode::with_children(NodeKind::TupleExpression, left_span, left_elements);

        // Expect assignment operator
        let op_token = self.expect(Token::Eq)?;
        let op_span = op_token.span;
        let op_node = CstNode::new(NodeKind::Operator, op_span);

        // Line continuation: skip newlines after =
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Parse right side (might also be a bare tuple)
        let right = self.parse_bare_tuple_or_expr()?;

        let span = self.source_map.span(left.span.start, right.span.end);
        Ok(CstNode::with_children(
            NodeKind::BinaryExpression,
            span,
            vec![left, op_node, right],
        ))
    }

    /// Parse a bare tuple or single expression for the right side of assignment
    pub(crate) fn parse_bare_tuple_or_expr(&mut self) -> ParseResult<CstNode> {
        let first = self.parse_expression_with_precedence(Precedence::Conditional)?;

        if !self.check(&Token::Comma) {
            return Ok(first);
        }

        // We have a comma - parse as bare tuple
        let mut elements = vec![first];
        while self.check(&Token::Comma) {
            self.advance();
            elements.push(self.parse_expression_with_precedence(Precedence::Conditional)?);
        }

        let start = elements.first().unwrap().span.start;
        let end = elements.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::TupleExpression,
            span,
            elements,
        ))
    }

    /// Parse global declaration: global x or global x = 1 or global x, y
    pub(crate) fn parse_global_declaration(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwGlobal)?;
        let start = start_token.span.start;

        // Parse first item (identifier or assignment)
        let first = self.parse_var_declaration_item()?;
        let mut items = vec![first];

        while self.check(&Token::Comma) {
            self.advance();
            items.push(self.parse_var_declaration_item()?);
        }

        let end = items.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::GlobalDeclaration,
            span,
            items,
        ))
    }

    /// Parse local declaration: local x or local x = 1 or local x, y
    pub(crate) fn parse_local_declaration(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwLocal)?;
        let start = start_token.span.start;

        // Parse first item (identifier or assignment)
        let first = self.parse_var_declaration_item()?;
        let mut items = vec![first];

        while self.check(&Token::Comma) {
            self.advance();
            items.push(self.parse_var_declaration_item()?);
        }

        let end = items.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::LocalDeclaration,
            span,
            items,
        ))
    }

    /// Parse a single variable declaration item: x or x = expr or x::T or x::T = expr
    /// Also supports compound assignments: x += expr, x -= expr, etc.
    pub(crate) fn parse_var_declaration_item(&mut self) -> ParseResult<CstNode> {
        let ident = self.parse_identifier()?;
        let ident_start = ident.span.start;

        // Check for type annotation
        let typed = if self.check(&Token::DoubleColon) {
            self.advance();
            let type_expr = self.parse_type_expression()?;
            let span = self.source_map.span(ident_start, type_expr.span.end);
            CstNode::with_children(NodeKind::TypedExpression, span, vec![ident, type_expr])
        } else {
            ident
        };

        // Check for initialization with simple assignment
        if self.check(&Token::Eq) {
            let op_token = self.advance().unwrap();
            let op_span = op_token.span;
            let op_node = CstNode::new(NodeKind::Operator, op_span);

            let value = self.parse_expression()?;
            let span = self.source_map.span(ident_start, value.span.end);
            Ok(CstNode::with_children(
                NodeKind::BinaryExpression,
                span,
                vec![typed, op_node, value],
            ))
        } else if self
            .current
            .as_ref()
            .map(|t| t.token.is_compound_assignment())
            .unwrap_or(false)
        {
            // Compound assignment: x += expr, x -= expr, etc.
            let op_token = self.advance().unwrap();
            let op_span = op_token.span;
            let op_node = CstNode::leaf(NodeKind::Operator, op_span, op_token.text);

            let value = self.parse_expression()?;
            let span = self.source_map.span(ident_start, value.span.end);
            Ok(CstNode::with_children(
                NodeKind::CompoundAssignmentExpression,
                span,
                vec![typed, op_node, value],
            ))
        } else {
            Ok(typed)
        }
    }
}
