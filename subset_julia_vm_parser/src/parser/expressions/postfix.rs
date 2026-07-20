//! Postfix expression parsers

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Try to parse a postfix operation (call, index, field)
    pub(crate) fn try_parse_postfix(&mut self, left: &CstNode) -> ParseResult<Option<CstNode>> {
        let token = match self.current.as_ref() {
            Some(t) => t,
            None => return Ok(None),
        };

        // In space-separated macro-argument context, a space before `(` or `[`
        // separates arguments instead of fusing into a call/index. So `@m foo (bar)`
        // is two arguments, while `@m foo(bar)` is one call argument (Issue #5494).
        // This mirrors upstream Julia's whitespace sensitivity but is intentionally
        // scoped to the macro argument's own top-level postfix chain (the flag is
        // cleared inside any grouping), so sjulia's lenient `f (x)` call parsing at
        // ordinary expression position is preserved.
        if self.macro_arg_space_sensitive
            && matches!(token.token, Token::LParen | Token::LBracket)
            && left.span.end != token.span.start
        {
            return Ok(None);
        }
        if self.in_matrix_row
            && matches!(token.token, Token::LParen | Token::LBracket)
            && left.span.end != token.span.start
        {
            return Ok(None);
        }

        match &token.token {
            // Numeric coefficient followed by parentheses: `2(x + 1)` means
            // `2 * (x + 1)` in Julia, not a call with integer callee.
            Token::LParen if self.is_numeric_juxtaposition_context(left, token) => {
                Ok(Some(self.parse_parenthesized_juxtaposition(left.clone())?))
            }

            // Function call: expr(args)
            Token::LParen => Ok(Some(self.parse_call_expression(left.clone())?)),

            // Index: expr[idx]
            Token::LBracket => Ok(Some(self.parse_index_expression(left.clone())?)),

            // Parametric type: Type{T} or Type{T, S}
            Token::LBrace => Ok(Some(self.parse_parametric_type(left.clone())?)),

            // Field access: expr.field
            Token::Dot => Ok(Some(self.parse_field_expression(left.clone())?)),

            // Type declaration: expr::Type
            Token::DoubleColon => Ok(Some(self.parse_type_declaration(left.clone())?)),

            // Splat: expr...
            Token::Ellipsis => Ok(Some(self.parse_splat_postfix(left.clone())?)),

            // Adjoint/transpose: A'
            Token::Prime => Ok(Some(self.parse_adjoint_expression(left.clone())?)),

            // Julia parses `a'ᵀ` as a call to the special transpose suffix
            // operator `'ᵀ` with `a` as the argument, not as multiplication by a
            // standalone identifier (Issue #8759).
            Token::Identifier
                if left.kind == NodeKind::AdjointExpression
                    && left.span.end == token.span.start
                    && token.text == "ᵀ" =>
            {
                Ok(Some(self.parse_adjoint_suffix_call(left.clone())?))
            }

            // Prefixed string/command literal: r"...", b"...", raw"...",
            // Module.prefix"...", x`cmd`
            // Only applies when left is an identifier AND immediately adjacent (no whitespace)
            // e.g., r"..." or raw"..." - NOT r "..." (with space)
            Token::DoubleQuote
            | Token::TripleDoubleQuote
            | Token::Backtick
            | Token::TripleBacktick
                if self.is_prefixed_literal_context(left, token) =>
            {
                let prefixed = self.parse_prefixed_string_literal(left.clone())?;
                Ok(Some(self.merge_var_quoted_identifier(prefixed)))
            }

            // Juxtaposition: 3.0im, 2x, f(x)y, (x)y (implicit multiplication).
            // Only applies when the identifier is immediately adjacent.
            Token::Identifier if self.is_identifier_juxtaposition_context(left, token) => {
                Ok(Some(self.parse_juxtaposition(left.clone())?))
            }

            _ => Ok(None),
        }
    }

    fn is_prefixed_literal_context(
        &self,
        left: &CstNode,
        token: &crate::lexer::SpannedToken<'a>,
    ) -> bool {
        matches!(left.kind, NodeKind::Identifier | NodeKind::FieldExpression)
            && left.span.end == token.span.start
    }

    fn parse_adjoint_suffix_call(&mut self, left: CstNode) -> ParseResult<CstNode> {
        let Some(arg) = left.children.first().cloned() else {
            return Ok(left);
        };
        let suffix = self.parse_identifier()?;
        let op_start = arg.span.end;
        let op_span = self.source_map.span(op_start, suffix.span.end);
        let callee = CstNode::leaf(NodeKind::Operator, op_span);
        let args = CstNode::with_children(NodeKind::ArgumentList, arg.span, vec![arg.clone()]);
        let span = self.source_map.span(arg.span.start, suffix.span.end);
        Ok(CstNode::with_children(
            NodeKind::CallExpression,
            span,
            vec![callee, args],
        ))
    }

    /// Check if we're in a numeric juxtaposition context (`2(x + 1)`).
    fn is_numeric_juxtaposition_context(
        &self,
        left: &CstNode,
        token: &crate::lexer::SpannedToken<'a>,
    ) -> bool {
        matches!(left.kind, NodeKind::IntegerLiteral | NodeKind::FloatLiteral)
            && left.span.end == token.span.start
    }

    /// Check if we're in an identifier-suffix juxtaposition context (`3im`,
    /// `f(x)y`, `(x)y`, `a[1]x`).
    fn is_identifier_juxtaposition_context(
        &self,
        left: &CstNode,
        token: &crate::lexer::SpannedToken<'a>,
    ) -> bool {
        matches!(
            left.kind,
            NodeKind::IntegerLiteral
                | NodeKind::FloatLiteral
                | NodeKind::CallExpression
                | NodeKind::IndexExpression
                | NodeKind::ParenthesizedExpression
        ) && left.span.end == token.span.start
    }

    /// Parse juxtaposition expression: 3.0im, 2x, 2f(x), 4n^2
    /// In Julia, `2f(x)` means `2 * f(x)`, so we need to parse the full
    /// postfix expression (including function calls) on the right side. Numeric
    /// literal coefficients bind looser than exponentiation, so `4n^2` is
    /// `4 * (n^2)`, not `(4n)^2` (Issue #8363).
    fn parse_juxtaposition(&mut self, left: CstNode) -> ParseResult<CstNode> {
        let start = left.span.start;
        let right = self.parse_expression_with_precedence(crate::token::Precedence::Power)?;

        let span = self.source_map.span(start, right.span.end);
        Ok(CstNode::with_children(
            NodeKind::JuxtapositionExpression,
            span,
            vec![left, right],
        ))
    }

    fn parse_parenthesized_juxtaposition(&mut self, left: CstNode) -> ParseResult<CstNode> {
        let start = left.span.start;
        let mut right = self.parse_parenthesized_or_tuple()?;
        if self
            .current_binary_precedence()
            .is_some_and(|(prec, _)| prec == crate::token::Precedence::Power)
        {
            let op_token = self.advance_checked(
                "Power-precedence operator token already confirmed by current_binary_precedence() above",
            )?;
            let exponent =
                self.parse_expression_with_precedence(crate::token::Precedence::Power)?;
            let span = self.source_map.span(right.span.start, exponent.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span);
            right = CstNode::with_children(
                NodeKind::BinaryExpression,
                span,
                vec![right, op_node, exponent],
            );
        }
        let span = self.source_map.span(start, right.span.end);
        Ok(CstNode::with_children(
            NodeKind::JuxtapositionExpression,
            span,
            vec![left, right],
        ))
    }

    /// Parse a prefixed string/command literal: r"...", b"...", raw"...", x`cmd`
    pub(crate) fn parse_prefixed_string_literal(
        &mut self,
        prefix: CstNode,
    ) -> ParseResult<CstNode> {
        let start = prefix.span.start;

        let literal = if self.check(&Token::Backtick) || self.check(&Token::TripleBacktick) {
            self.parse_command_literal()?
        } else {
            self.parse_string_literal()?
        };

        let mut end = literal.span.end;
        let mut children = vec![prefix, literal];

        // Non-standard string and command literal macros may carry flag text
        // immediately after the closing delimiter: `r"abc"i`, `x"s"flag`,
        // `x`s`flag`. Capture it as a third `Identifier` child.
        let adjacent_ident = self
            .current
            .as_ref()
            .is_some_and(|tok| matches!(tok.token, Token::Identifier) && tok.span.start == end);
        if adjacent_ident {
            let flags = self.parse_identifier()?;
            end = flags.span.end;
            children.push(flags);
        }

        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::PrefixedStringLiteral,
            span,
            children,
        ))
    }
}
