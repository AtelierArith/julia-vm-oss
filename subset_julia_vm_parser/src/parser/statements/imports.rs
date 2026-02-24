//! Import/export statement parsers (using, import, export, public)

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    // ==================== Import/Export Statements ====================

    /// Parse using statement: using Module, Module2
    pub(crate) fn parse_using_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwUsing)?;
        let start = start_token.span.start;

        let imports = self.parse_import_list()?;
        let end = imports.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::UsingStatement,
            span,
            vec![imports],
        ))
    }

    /// Parse import statement: import Module: func1, func2
    pub(crate) fn parse_import_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwImport)?;
        let start = start_token.span.start;

        let imports = self.parse_import_list()?;
        let end = imports.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ImportStatement,
            span,
            vec![imports],
        ))
    }

    /// Parse import list: Module, Module2 or Module: func1, func2
    pub(crate) fn parse_import_list(&mut self) -> ParseResult<CstNode> {
        let first = self.parse_import_path()?;
        let start = first.span.start;
        let mut items = vec![first];

        while self.check(&Token::Comma) {
            self.advance();
            items.push(self.parse_import_path()?);
        }

        let end = items.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::ImportList, span, items))
    }

    /// Parse import path: Module or Module.SubModule or Module: func
    /// Also handles `as` aliases: import Base as B, import Base: sin as s
    /// Also handles relative imports: .My, ..Parent.My
    pub(crate) fn parse_import_path(&mut self) -> ParseResult<CstNode> {
        let start = self.current_span().start;
        let mut path = Vec::new();

        // Handle leading dots for relative imports: .My, ..Parent
        // Create a synthetic identifier for the relative path prefix
        let mut leading_dots = String::new();
        while self.check(&Token::Dot) {
            let dot_token = self.advance().unwrap();
            leading_dots.push('.');
            // If next token is not an identifier or another dot, we have just dots
            if !self.check(&Token::Identifier) && !self.check(&Token::Dot) {
                // Just dots - create identifier node for them
                let span = self.source_map.span(start, dot_token.span.end);
                return Ok(CstNode::with_children(
                    NodeKind::ImportPath,
                    span,
                    vec![CstNode::leaf(NodeKind::Identifier, span, &leading_dots)],
                ));
            }
        }

        // Parse the first identifier
        let first = self.parse_identifier()?;

        // If we had leading dots, prefix them to the first identifier
        if !leading_dots.is_empty() {
            let prefixed_name = format!("{}{}", leading_dots, first.text.as_deref().unwrap_or(""));
            let span = self.source_map.span(start, first.span.end);
            path.push(CstNode::leaf(NodeKind::Identifier, span, &prefixed_name));
        } else {
            path.push(first);
        }

        // Parse dotted path: Module.SubModule
        while self.check(&Token::Dot) {
            self.advance();
            path.push(self.parse_identifier()?);
        }

        // Check for module-level alias: import Base as B
        if self.check(&Token::KwAs) {
            self.advance(); // consume 'as'
            let alias = self.parse_identifier()?;
            path.push(alias);
        }

        // Parse selective import: Module: func1, func2
        if self.check(&Token::Colon) {
            self.advance();
            let func = self.parse_import_item()?;
            path.push(func);

            while self.check(&Token::Comma) && !self.check(&Token::Newline) {
                self.advance();
                if self.check(&Token::Identifier) {
                    path.push(self.parse_import_item()?);
                } else {
                    break;
                }
            }
        }

        let end = path.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::ImportPath, span, path))
    }

    /// Parse a single import item, optionally with alias: name or name as alias
    pub(crate) fn parse_import_item(&mut self) -> ParseResult<CstNode> {
        let name = self.parse_identifier()?;
        let start = name.span.start;

        if self.check(&Token::KwAs) {
            self.advance(); // consume 'as'
            let alias = self.parse_identifier()?;
            let end = alias.span.end;
            let span = self.source_map.span(start, end);
            Ok(CstNode::with_children(
                NodeKind::ImportAlias,
                span,
                vec![name, alias],
            ))
        } else {
            Ok(name)
        }
    }

    /// Parse export statement: export func1, func2
    /// Supports line continuation after commas
    pub(crate) fn parse_export_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwExport)?;
        let start = start_token.span.start;

        let first = self.parse_identifier()?;
        let mut names = vec![first];

        while self.check(&Token::Comma) {
            self.advance(); // consume comma

            // Skip newlines after comma (line continuation in export)
            while self.check(&Token::Newline) {
                self.advance();
            }

            names.push(self.parse_identifier()?);
        }

        let end = names.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ExportStatement,
            span,
            names,
        ))
    }

    /// Parse public statement: public foo, bar (Julia 1.11+)
    pub(crate) fn parse_public_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwPublic)?;
        let start = start_token.span.start;

        let first = self.parse_identifier()?;
        let mut names = vec![first];

        while self.check(&Token::Comma) {
            self.advance();
            names.push(self.parse_identifier()?);
        }

        let end = names.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::PublicStatement,
            span,
            names,
        ))
    }
}
