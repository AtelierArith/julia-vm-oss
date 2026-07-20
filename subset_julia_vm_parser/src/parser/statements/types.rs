//! Type definition parsers (struct, abstract, primitive, module)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    // ==================== Type Definitions ====================

    /// Parse struct definition: [mutable] struct Name ... end
    pub(crate) fn parse_struct_definition(&mut self) -> ParseResult<CstNode> {
        let start = self.current.as_ref().map(|t| t.span.start).unwrap_or(0);

        // Check for mutable
        let is_mutable = self.check(&Token::KwMutable);
        if is_mutable {
            self.advance();
        }

        self.expect(Token::KwStruct)?;

        let name = self.parse_identifier_like_name()?;

        // Optional type parameters
        let type_params = if self.check(&Token::LBrace) {
            Some(self.parse_type_parameters()?)
        } else {
            None
        };

        // Optional supertype
        let supertype = if self.check(&Token::Subtype) {
            self.advance();
            Some(self.parse_type_expression()?)
        } else {
            None
        };

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        let end_token = self.expect(Token::KwEnd)?;

        let mut children = vec![name];
        if let Some(tp) = type_params {
            children.push(tp);
        }
        if let Some(st) = supertype {
            children.push(st);
        }
        children.push(body);

        let span = self.source_map.span(start, end_token.span.end);
        let kind = if is_mutable {
            NodeKind::MutableStructDefinition
        } else {
            NodeKind::StructDefinition
        };
        Ok(CstNode::with_children(kind, span, children))
    }

    /// Parse type parameters: {T, S <: Number}
    pub(crate) fn parse_type_parameters(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LBrace)?;
        let start = start_token.span.start;
        let mut params = Vec::new();

        if !self.check(&Token::RBrace) {
            loop {
                params.push(self.parse_type_parameter()?);
                if !self.check(&Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        let end_token = self.expect(Token::RBrace)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TypeParameters,
            span,
            params,
        ))
    }

    /// Parse single type parameter: T, T <: Bound, <:Bound, or a type
    /// expression used in method heads such as Foo{T, Bar{T}}.
    pub(crate) fn parse_type_parameter(&mut self) -> ParseResult<CstNode> {
        if self.check(&Token::Subtype) || self.check(&Token::Supertype) {
            let op = self.advance_checked("Subtype/Supertype token already matched above")?;
            let bound = self.parse_type_expression()?;
            let span = self.source_map.span(op.span.start, bound.span.end);
            let kind = if op.token == Token::Subtype {
                NodeKind::SubtypeConstraint
            } else {
                NodeKind::SupertypeConstraint
            };
            let constraint = CstNode::with_children(kind, span, vec![bound]);
            return Ok(CstNode::with_children(
                NodeKind::TypeParameter,
                span,
                vec![constraint],
            ));
        }

        let name = self.parse_type_expression()?;
        let start = name.span.start;
        let mut children = vec![name];

        if self.check(&Token::Subtype) || self.check(&Token::Supertype) {
            let first_is_subtype = self.check(&Token::Subtype);
            self.advance();
            let second = self.parse_type_expression()?;

            // Double-bounded parameter (Issue #10644): `Lo <: T <: Hi` (and
            // the mirrored `Hi >: T >: Lo`), matching upstream's comparison
            // chain for struct/abstract parameters. Emit the same
            // `SubtypeConstraint` with three children `[name, upper, lower]`
            // that the `where`-clause double bound produces (Issue #5051),
            // so lowering recovers both bounds through one shape.
            let chains = if first_is_subtype {
                self.check(&Token::Subtype)
            } else {
                self.check(&Token::Supertype)
            };
            if chains {
                self.advance();
                let third = self.parse_type_expression()?;
                let span = self.source_map.span(start, third.span.end);
                let first = children.remove(0);
                let (name, upper, lower) = if first_is_subtype {
                    // Lo <: T <: Hi
                    (second, third, first)
                } else {
                    // Hi >: T >: Lo
                    (second, first, third)
                };
                let constraint = CstNode::with_children(
                    NodeKind::SubtypeConstraint,
                    span,
                    vec![name, upper, lower],
                );
                return Ok(CstNode::with_children(
                    NodeKind::TypeParameter,
                    span,
                    vec![constraint],
                ));
            }

            children.push(second);
        }

        let end = self.last_span_end(&children, "type parameter always pushes `name` above")?;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::TypeParameter,
            span,
            children,
        ))
    }

    /// Parse abstract definition: abstract type Name[{T}] [<: Supertype] end
    pub(crate) fn parse_abstract_definition(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwAbstract)?;
        let start = start_token.span.start;

        // `type` is lexed as a plain identifier (Issue #8108); it is only the
        // keyword half of `abstract type` here.
        self.expect_contextual_keyword("type")?;

        let name = self.parse_identifier_like_name()?;

        // Optional type parameters: abstract type Foo{T} end
        let type_params = if self.check(&Token::LBrace) {
            Some(self.parse_type_parameters()?)
        } else {
            None
        };

        // Optional supertype
        let supertype = if self.check(&Token::Subtype) {
            self.advance();
            Some(self.parse_type_expression()?)
        } else {
            None
        };

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let end_token = self.expect(Token::KwEnd)?;

        let mut children = vec![name];
        if let Some(tp) = type_params {
            children.push(tp);
        }
        if let Some(st) = supertype {
            children.push(st);
        }

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::AbstractDefinition,
            span,
            children,
        ))
    }

    /// Parse primitive type definition: primitive type Name bits end
    pub(crate) fn parse_primitive_definition(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwPrimitive)?;
        let start = start_token.span.start;

        // `type` is lexed as a plain identifier (Issue #8108); it is only the
        // keyword half of `primitive type` here.
        self.expect_contextual_keyword("type")?;

        // Primitive type names can be parametric or interpolated in generated
        // code, e.g. `primitive type T{N} 8 end` and
        // `primitive type $(esc(:T)) $(bits) end`.
        let saved_space_sensitive = std::mem::replace(&mut self.macro_arg_space_sensitive, true);
        let name = self.parse_type_expression();
        self.macro_arg_space_sensitive = saved_space_sensitive;
        let name = name?;

        // Optional supertype
        let supertype = if self.check(&Token::Subtype) {
            self.advance();
            Some(self.parse_type_expression()?)
        } else {
            None
        };

        // Bit size. Parenthesized constant expressions such as `(18 * 8)` must
        // stop at the closing paren and leave the following `end` as the
        // primitive declaration terminator (Issue #9050).
        let bits = if self.check(&Token::LParen) {
            self.parse_parenthesized_or_tuple()?
        } else {
            self.parse_expression()?
        };

        let end_token = self.expect(Token::KwEnd)?;

        let mut children = vec![name];
        if let Some(st) = supertype {
            children.push(st);
        }
        children.push(bits);

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::PrimitiveDefinition,
            span,
            children,
        ))
    }

    /// Parse module definition: module Name ... end
    pub(crate) fn parse_module_definition(&mut self) -> ParseResult<CstNode> {
        let start = self.current.as_ref().map(|t| t.span.start).unwrap_or(0);

        let is_bare = self.check(&Token::KwBaremodule);
        if is_bare {
            self.advance();
        } else {
            self.expect(Token::KwModule)?;
        }

        let name = self.parse_identifier_like_name()?;

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        let end_token = self.expect(Token::KwEnd)?;

        let span = self.source_map.span(start, end_token.span.end);
        let kind = if is_bare {
            NodeKind::BaremoduleDefinition
        } else {
            NodeKind::ModuleDefinition
        };
        // Use field names for tree-sitter compatibility
        let mut node = CstNode::new(kind, span);
        node.push_field("name", name);
        node.push_child(body);
        Ok(node)
    }
}
