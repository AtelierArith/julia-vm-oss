//! Index expression parsers

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse an index expression or typed array/matrix
    /// Handles: obj[i], obj[i, j], Type[1, 2, 3] (typed vector), Type[1 2; 3 4] (typed matrix)
    pub(crate) fn parse_index_expression(&mut self, object: CstNode) -> ParseResult<CstNode> {
        let start = object.span.start;
        self.expect(Token::LBracket)?;

        // Check for empty: Type[]
        if self.check(&Token::RBracket) {
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            // Empty typed array: Type[]
            let inner = CstNode::new(
                NodeKind::VectorExpression,
                self.source_map
                    .span(end_token.span.start, end_token.span.start),
            );
            return Ok(CstNode::with_children(
                NodeKind::TypedExpression,
                span,
                vec![object, inner],
            ));
        }

        // Parse first element
        let first = self.parse_expression()?;

        // Check what follows to determine the type
        if self.check(&Token::Comma) {
            // Comma-separated: either index or typed vector
            let mut elements = vec![first];
            while self.check(&Token::Comma) {
                self.advance(); // consume comma

                // Skip newlines
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

            // For now, treat as index expression (Type[i, j] for indexing)
            // Typed vectors like Int[1, 2, 3] are also valid as indexing syntax
            let mut children = vec![object];
            children.extend(elements);
            Ok(CstNode::with_children(
                NodeKind::IndexExpression,
                span,
                children,
            ))
        } else if self.check(&Token::RBracket) {
            // Single element: obj[i] or Type[expr]
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::IndexExpression,
                span,
                vec![object, first],
            ))
        } else if self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            // Matrix-like: Type[a b; c d] or Type[a\n b]
            // First element is already parsed, now parse rest as matrix
            self.parse_typed_matrix_rest(start, object, first)
        } else {
            // Could be matrix row: Type[a b c]
            self.parse_typed_matrix_row_rest(start, object, first)
        }
    }

    /// Parse rest of typed matrix: Type[first ...; ...]
    pub(crate) fn parse_typed_matrix_rest(
        &mut self,
        start: usize,
        type_node: CstNode,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let matrix_start = first.span.start;

        // Build first row from 'first' element
        let mut first_row_elements = vec![first];

        // Parse rest of first row (space-separated elements until ; or newline)
        while !self.check(&Token::Semicolon)
            && !self.check(&Token::Newline)
            && !self.check(&Token::RBracket)
        {
            first_row_elements.push(self.parse_expression()?);
        }

        let first_row_span = self.source_map.span(
            first_row_elements[0].span.start,
            first_row_elements.last().unwrap().span.end,
        );
        let first_row =
            CstNode::with_children(NodeKind::MatrixRow, first_row_span, first_row_elements);

        let mut rows = vec![first_row];

        // Parse additional rows
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            self.advance(); // consume ; or newline

            // Skip additional newlines
            while self.check(&Token::Newline) {
                self.advance();
            }

            if self.check(&Token::RBracket) {
                break;
            }

            rows.push(self.parse_matrix_row()?);
        }

        let end_token = self.expect(Token::RBracket)?;
        let matrix_span = self.source_map.span(matrix_start, end_token.span.end);
        let matrix = CstNode::with_children(NodeKind::MatrixExpression, matrix_span, rows);

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TypedExpression,
            span,
            vec![type_node, matrix],
        ))
    }

    /// Parse rest of typed matrix row: Type[first elem2 elem3]
    pub(crate) fn parse_typed_matrix_row_rest(
        &mut self,
        start: usize,
        type_node: CstNode,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let matrix_start = first.span.start;
        let mut elements = vec![first];

        // Parse remaining space-separated elements in the row
        while !self.check(&Token::RBracket)
            && !self.check(&Token::Semicolon)
            && !self.check(&Token::Newline)
        {
            elements.push(self.parse_expression()?);
        }

        // If we hit semicolon or newline, there are more rows
        if self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            let first_row_span = self
                .source_map
                .span(elements[0].span.start, elements.last().unwrap().span.end);
            let first_row = CstNode::with_children(NodeKind::MatrixRow, first_row_span, elements);

            let mut rows = vec![first_row];

            while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
                self.advance();

                while self.check(&Token::Newline) {
                    self.advance();
                }

                if self.check(&Token::RBracket) {
                    break;
                }

                rows.push(self.parse_matrix_row()?);
            }

            let end_token = self.expect(Token::RBracket)?;
            let matrix_span = self.source_map.span(matrix_start, end_token.span.end);
            let matrix = CstNode::with_children(NodeKind::MatrixExpression, matrix_span, rows);

            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::TypedExpression,
                span,
                vec![type_node, matrix],
            ))
        } else {
            // Single row matrix: Type[a b c]
            let end_token = self.expect(Token::RBracket)?;
            let row_span = self
                .source_map
                .span(elements[0].span.start, elements.last().unwrap().span.end);
            let row = CstNode::with_children(NodeKind::MatrixRow, row_span, elements);
            let matrix_span = self.source_map.span(matrix_start, end_token.span.end);
            let matrix = CstNode::with_children(NodeKind::MatrixExpression, matrix_span, vec![row]);

            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::TypedExpression,
                span,
                vec![type_node, matrix],
            ))
        }
    }
}
