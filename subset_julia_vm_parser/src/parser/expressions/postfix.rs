//! Postfix expression parsers

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

        match &token.token {
            // Numeric coefficient followed by parentheses: `2(x + 1)` means
            // `2 * (x + 1)` in Julia, not a call with integer callee.
            Token::LParen if self.is_juxtaposition_context(left, token) => {
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

            // Prefixed string literal: r"...", b"...", raw"..."
            // Only applies when left is an identifier AND immediately adjacent (no whitespace)
            // e.g., r"..." or raw"..." - NOT r "..." (with space)
            Token::DoubleQuote | Token::TripleDoubleQuote
                if left.kind == NodeKind::Identifier && left.span.end == token.span.start =>
            {
                Ok(Some(self.parse_prefixed_string_literal(left.clone())?))
            }

            // Juxtaposition: 3.0im, 2x (implicit multiplication)
            // Only applies when left is a numeric literal and identifier is immediately adjacent
            Token::Identifier if self.is_juxtaposition_context(left, token) => {
                Ok(Some(self.parse_juxtaposition(left.clone())?))
            }

            _ => Ok(None),
        }
    }

    /// Check if we're in a juxtaposition context (number followed by identifier without whitespace)
    fn is_juxtaposition_context(
        &self,
        left: &CstNode,
        token: &crate::lexer::SpannedToken<'a>,
    ) -> bool {
        // Left must be a numeric literal
        let is_numeric = matches!(left.kind, NodeKind::IntegerLiteral | NodeKind::FloatLiteral);
        if !is_numeric {
            return false;
        }

        // Token must immediately follow left (no whitespace)
        left.span.end == token.span.start
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
            .current
            .as_ref()
            .and_then(|token| token.token.binary_precedence())
            .is_some_and(|(prec, _)| prec == crate::token::Precedence::Power)
        {
            let op_token = self.advance().unwrap();
            let exponent =
                self.parse_expression_with_precedence(crate::token::Precedence::Power)?;
            let span = self.source_map.span(right.span.start, exponent.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
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

    /// Parse a prefixed string literal: r"...", b"...", raw"..."
    pub(crate) fn parse_prefixed_string_literal(
        &mut self,
        prefix: CstNode,
    ) -> ParseResult<CstNode> {
        let start = prefix.span.start;

        // Parse the string literal
        let string = self.parse_string_literal()?;

        let mut end = string.span.end;
        let mut children = vec![prefix, string];

        // A regex literal may carry flag characters (`i`, `m`, `s`, `x`) immediately
        // after the closing quote with no whitespace: `r"abc"i`, `r"x"ims`
        // (Issue #5709). Capture them as a third `Identifier` child so lowering can
        // pass them to the `Regex` constructor. Restricted to the `r` prefix so other
        // prefixed literals (`raw"..."`, `big"..."`, ...) keep their two-child shape.
        let is_regex_prefix = &self.source[children[0].span.start..children[0].span.end] == "r";
        let adjacent_ident = self
            .current
            .as_ref()
            .is_some_and(|tok| matches!(tok.token, Token::Identifier) && tok.span.start == end);
        if is_regex_prefix && adjacent_ident {
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
