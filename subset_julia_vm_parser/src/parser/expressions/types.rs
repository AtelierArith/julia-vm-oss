//! Type-related expression parsers

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

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
        // Type-parameter interiors parse without macro-argument whitespace
        // sensitivity; it is restored before returning (Issue #5494). A
        // matrix-row context also does not extend into `{...}` (Issue #7196).
        let saved_space_sensitive = std::mem::replace(&mut self.macro_arg_space_sensitive, false);
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, false);
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        let result = self.parse_parametric_type_inner(base);
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
        self.in_ternary_then = saved_in_ternary_then;
        result
    }

    fn parse_parametric_type_inner(&mut self, base: CstNode) -> ParseResult<CstNode> {
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
                self.skip_newlines();

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

        // Issue #4862: `?` is a line-continuation token in Julia — a newline
        // immediately after `?` is part of the same ternary expression, e.g.
        //     cond ?
        //         then : else
        // The binary-operator continuation loop in `expressions/mod.rs`
        // already lists `?` as a continuation token, but the ternary
        // dispatch path (`mod.rs` ~L48) bypasses that loop and goes
        // straight here. Eat the newlines explicitly. Same treatment for
        // the inner `:` so `cond ? then :\n else` also parses.
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Parse then-expression at assignment level to allow Julia forms like
        // `cond ? x = y : z` and `cond ? x=>y : z`.
        // The nested ternary will consume its own `:`, so we can still expect
        // the outer `:` after the then-expression completes. `in_ternary_then`
        // makes a whitespace-preceded `:` end the then-branch even inside a
        // higher-precedence operator's right operand (e.g. `cond ? a > b : c`),
        // where the `:` would otherwise be consumed as a range (Issue #8314).
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, true);
        let then_expr = self.parse_expression_with_precedence(Precedence::Assign)?;
        self.in_ternary_then = saved_in_ternary_then;
        if self.check(&Token::Newline)
            && self.grouping_depth > 0
            && self.peek_non_newline_token() == Some(Token::Colon)
        {
            self.skip_newlines();
        }
        self.expect(Token::Colon)?;
        while self.check(&Token::Newline) {
            self.advance();
        }
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
        // `where {...}` interiors parse without macro-argument whitespace
        // sensitivity; it is restored before returning (Issue #5494). A
        // matrix-row context also does not extend into `{...}` (Issue #7196).
        let saved_space_sensitive = std::mem::replace(&mut self.macro_arg_space_sensitive, false);
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, false);
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        let result = self.parse_braced_type_params_inner();
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
        self.in_ternary_then = saved_in_ternary_then;
        result
    }

    fn parse_braced_type_params_inner(&mut self) -> ParseResult<CstNode> {
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
                self.skip_newlines();

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

    /// Parse a type constraint: `T`, `T <: Number`, `T >: Integer`, or the
    /// double-bounded form `Integer <: T <: Real` (Issue #5051).
    pub(crate) fn parse_type_constraint(&mut self) -> ParseResult<CstNode> {
        self.parse_type_constraint_inner(true)
    }

    /// Parse an unbraced where-clause constraint and stop before a following
    /// chained `where`, e.g. `where S<:Number where T`.
    pub(crate) fn parse_type_constraint_before_chained_where(&mut self) -> ParseResult<CstNode> {
        self.parse_type_constraint_inner(false)
    }

    fn parse_type_constraint_inner(&mut self, allow_postfix_where: bool) -> ParseResult<CstNode> {
        let first = self.parse_type_expression_inner(allow_postfix_where)?;
        let start = first.span.start;

        // Check for subtype constraint: `T <: Number` or the leading half of a
        // double bound `Integer <: T <: Real`.
        if self.check(&Token::Subtype) {
            self.advance(); // consume first <:
            let second = self.parse_type_expression_inner(allow_postfix_where)?;

            // Double bound: `Lower <: T <: Upper`. The middle element is the
            // type variable, the first is the lower bound and the third is the
            // upper bound. Emit a SubtypeConstraint with three children
            // [name, upper, lower] so lowering can recover both bounds.
            if self.check(&Token::Subtype) {
                self.advance(); // consume second <:
                let upper = self.parse_type_expression_inner(allow_postfix_where)?;
                let span = self.source_map.span(start, upper.span.end);
                let lower = first;
                let name = second;
                return Ok(CstNode::with_children(
                    NodeKind::SubtypeConstraint,
                    span,
                    vec![name, upper, lower],
                ));
            }

            // Single upper bound: `T <: Number`.
            let span = self.source_map.span(start, second.span.end);
            return Ok(CstNode::with_children(
                NodeKind::SubtypeConstraint,
                span,
                vec![first, second],
            ));
        }

        // Check for supertype constraint: T >: Integer
        if self.check(&Token::Supertype) {
            self.advance(); // consume >:
            let bound = self.parse_type_expression_inner(allow_postfix_where)?;

            // Mirrored double bound `Hi >: T >: Lo` (Issue #10644): upstream
            // parses the reversed comparison chain too. Normalize to the same
            // `SubtypeConstraint` shape `[name, upper, lower]` as the
            // `Lo <: T <: Hi` form above.
            if self.check(&Token::Supertype) {
                self.advance(); // consume second >:
                let lower = self.parse_type_expression_inner(allow_postfix_where)?;
                let span = self.source_map.span(start, lower.span.end);
                let upper = first;
                let name = bound;
                return Ok(CstNode::with_children(
                    NodeKind::SubtypeConstraint,
                    span,
                    vec![name, upper, lower],
                ));
            }

            let span = self.source_map.span(start, bound.span.end);
            return Ok(CstNode::with_children(
                NodeKind::SupertypeConstraint,
                span,
                vec![first, bound],
            ));
        }

        // Just a type variable name
        Ok(first)
    }

    /// Parse type expression (handles Type, Type{T}, Mod.Type{T}, etc.)
    pub(crate) fn parse_type_expression(&mut self) -> ParseResult<CstNode> {
        self.parse_type_expression_inner(true)
    }

    fn parse_type_expression_inner(&mut self, allow_postfix_where: bool) -> ParseResult<CstNode> {
        let mut left = self.parse_prefix()?;

        // Handle type-related postfix operations
        while !self.is_at_end() {
            if allow_postfix_where && self.check_where_keyword() {
                self.advance(); // consume contextual `where`
                let right = if self.check(&Token::LBrace) {
                    self.parse_braced_type_params()?
                } else {
                    self.parse_type_constraint_before_chained_where()?
                };
                let span = self.source_map.span(left.span.start, right.span.end);
                left = CstNode::with_children(NodeKind::WhereExpression, span, vec![left, right]);
                continue;
            }

            match self.current.as_ref().map(|token| &token.token) {
                // Parametric type: Type{T}
                Some(Token::LBrace) => {
                    left = self.parse_parametric_type(left)?;
                }
                // Qualified type: Mod.Type
                Some(Token::Dot) => {
                    left = self.parse_field_expression(left)?;
                }
                _ => break,
            }
        }

        Ok(left)
    }
}
