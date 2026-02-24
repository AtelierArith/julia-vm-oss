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

        match &token.token {
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
    fn is_juxtaposition_context(&self, left: &CstNode, token: &crate::lexer::SpannedToken<'a>) -> bool {
        // Left must be a numeric literal
        let is_numeric = matches!(left.kind, NodeKind::IntegerLiteral | NodeKind::FloatLiteral);
        if !is_numeric {
            return false;
        }

        // Token must immediately follow left (no whitespace)
        left.span.end == token.span.start
    }

    /// Parse juxtaposition expression: 3.0im, 2x, 2f(x)
    /// In Julia, `2f(x)` means `2 * f(x)`, so we need to parse the full
    /// postfix expression (including function calls) on the right side.
    fn parse_juxtaposition(&mut self, left: CstNode) -> ParseResult<CstNode> {
        let start = left.span.start;
        let mut right = self.parse_identifier()?;

        // Check for postfix operations on the identifier (e.g., function calls)
        // This handles cases like 2f(x) which should be 2 * f(x)
        while let Some(postfix) = self.try_parse_postfix(&right)? {
            right = postfix;
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

        let span = self.source_map.span(start, string.span.end);
        Ok(CstNode::with_children(
            NodeKind::PrefixedStringLiteral,
            span,
            vec![prefix, string],
        ))
    }
}
