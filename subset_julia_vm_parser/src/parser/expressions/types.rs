//! Type-related expression parsers

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse a type declaration: expr::Type
    pub(crate) fn parse_type_declaration(&mut self, expr: CstNode) -> ParseResult<CstNode> {
        let start = expr.span.start;
        self.expect(Token::DoubleColon)?;
        let type_expr = self.parse_prefix()?; // Just parse the type as a simple expression
        let span = self.source_map.span(start, type_expr.span.end);
        Ok(CstNode::with_children(
            NodeKind::TypedExpression,
            span,
            vec![expr, type_expr],
        ))
    }

    /// Parse a parametric type expression: Type{T} or Type{T, S}
    pub(crate) fn parse_parametric_type(&mut self, base: CstNode) -> ParseResult<CstNode> {
        let start = base.span.start;
        self.expect(Token::LBrace)?;

        let mut children = vec![base];

        // Parse type parameters
        if !self.check(&Token::RBrace) {
            loop {
                // Skip newlines
                while self.check(&Token::Newline) {
                    self.advance();
                }

                if self.check(&Token::RBrace) {
                    break;
                }

                children.push(self.parse_expression()?);

                if !self.check(&Token::Comma) {
                    break;
                }
                self.advance(); // consume comma
            }
        }

        let end_token = self.expect(Token::RBrace)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::ParametrizedTypeExpression,
            span,
            children,
        ))
    }

    /// Parse a splat expression: expr...
    pub(crate) fn parse_splat_postfix(&mut self, expr: CstNode) -> ParseResult<CstNode> {
        let start = expr.span.start;
        let end_token = self.expect(Token::Ellipsis)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::SplatExpression,
            span,
            vec![expr],
        ))
    }

    /// Parse an adjoint expression: A'
    pub(crate) fn parse_adjoint_expression(&mut self, expr: CstNode) -> ParseResult<CstNode> {
        let start = expr.span.start;
        let end_token = self.expect(Token::Prime)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::AdjointExpression,
            span,
            vec![expr],
        ))
    }

    /// Parse a ternary expression: cond ? then : else
    /// Supports nested ternaries like `a ? b ? c : d : e`
    pub(crate) fn parse_ternary(&mut self, condition: CstNode) -> ParseResult<CstNode> {
        use crate::token::Precedence;

        let start = condition.span.start;
        self.expect(Token::Question)?;

        // Parse then-expression at Conditional level to allow nested ternaries
        // The nested ternary will consume its own `:`, so we can still expect
        // the outer `:` after the then-expression completes
        let then_expr = self.parse_expression_with_precedence(Precedence::Conditional)?;
        self.expect(Token::Colon)?;
        let else_expr = self.parse_expression_with_precedence(Precedence::Conditional)?;

        let span = self.source_map.span(start, else_expr.span.end);
        Ok(CstNode::with_children(
            NodeKind::TernaryExpression,
            span,
            vec![condition, then_expr, else_expr],
        ))
    }

    /// Parse braced type parameter list for where clauses: {T, S} or {T <: Number, S}
    /// Used when parsing: expr where {T, S}
    pub(crate) fn parse_braced_type_params(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LBrace)?;
        let start = start_token.span.start;

        let mut children = Vec::new();

        // Parse type parameters
        if !self.check(&Token::RBrace) {
            loop {
                // Skip newlines
                while self.check(&Token::Newline) {
                    self.advance();
                }

                if self.check(&Token::RBrace) {
                    break;
                }

                // Parse a type constraint: T or T <: Number
                children.push(self.parse_type_constraint()?);

                if !self.check(&Token::Comma) {
                    break;
                }
                self.advance(); // consume comma
            }
        }

        let end_token = self.expect(Token::RBrace)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TypeParameterList,
            span,
            children,
        ))
    }

    /// Parse a type constraint: T or T <: Number or T >: Integer
    fn parse_type_constraint(&mut self) -> ParseResult<CstNode> {
        let name = self.parse_identifier()?;
        let start = name.span.start;

        // Check for subtype constraint: T <: Number
        if self.check(&Token::Subtype) {
            self.advance(); // consume <:
            let bound = self.parse_type_expression()?;
            let span = self.source_map.span(start, bound.span.end);
            return Ok(CstNode::with_children(
                NodeKind::SubtypeConstraint,
                span,
                vec![name, bound],
            ));
        }

        // Check for supertype constraint: T >: Integer
        if self.check(&Token::Supertype) {
            self.advance(); // consume >:
            let bound = self.parse_type_expression()?;
            let span = self.source_map.span(start, bound.span.end);
            return Ok(CstNode::with_children(
                NodeKind::SupertypeConstraint,
                span,
                vec![name, bound],
            ));
        }

        // Just a type variable name
        Ok(name)
    }

    /// Parse type expression (handles Type, Type{T}, Mod.Type{T}, etc.)
    pub(crate) fn parse_type_expression(&mut self) -> ParseResult<CstNode> {
        let mut left = self.parse_prefix()?;

        // Handle type-related postfix operations
        loop {
            let Some(token) = self.current.as_ref() else {
                break;
            };

            match &token.token {
                // Parametric type: Type{T}
                Token::LBrace => {
                    left = self.parse_parametric_type(left)?;
                }
                // Qualified type: Mod.Type
                Token::Dot => {
                    left = self.parse_field_expression(left)?;
                }
                _ => break,
            }
        }

        Ok(left)
    }
}
