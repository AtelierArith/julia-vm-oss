//! Type definition parsers (struct, abstract, primitive, module)

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

        let name = self.parse_identifier()?;

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

    /// Parse single type parameter: T or T <: Bound
    pub(crate) fn parse_type_parameter(&mut self) -> ParseResult<CstNode> {
        let name = self.parse_identifier()?;
        let start = name.span.start;
        let mut children = vec![name];

        if self.check(&Token::Subtype) {
            self.advance();
            children.push(self.parse_type_expression()?);
        }

        let end = children.last().unwrap().span.end;
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

        self.expect(Token::KwType)?;

        let name = self.parse_identifier()?;

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

        self.expect(Token::KwType)?;

        let name = self.parse_identifier()?;

        // Optional supertype
        let supertype = if self.check(&Token::Subtype) {
            self.advance();
            Some(self.parse_type_expression()?)
        } else {
            None
        };

        // Bit size (an integer literal)
        let bits = self.parse_expression()?;

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

        let name = self.parse_identifier()?;

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
