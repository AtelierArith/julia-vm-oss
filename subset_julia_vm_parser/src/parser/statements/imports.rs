//! Import/export statement parsers (using, import, export, public)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
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
            self.skip_newlines();
            items.push(self.parse_import_path()?);
        }

        let end = self.last_span_end(&items, "import list always pushes `first` above")?;
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
        while self.check(&Token::Dot) || self.check(&Token::DotDot) || self.check(&Token::Ellipsis)
        {
            let dot_token = self.advance_checked(
                "Dot/DotDot/Ellipsis token already matched by the while condition above",
            )?;
            match dot_token.token {
                Token::Dot => leading_dots.push('.'),
                Token::DotDot => leading_dots.push_str(".."),
                Token::Ellipsis => leading_dots.push_str("..."),
                _ => unreachable!(),
            }
            // If next token is not an importable name or another dot, we have
            // just dots. Parent-relative imports may target macros
            // (`import ..@inline`) or interpolated/operator names too, so use
            // the same start predicate as selective import items here.
            if !self.is_import_name_start()
                && !self.check(&Token::Dot)
                && !self.check(&Token::DotDot)
                && !self.check(&Token::Ellipsis)
            {
                // Just dots - create identifier node for them
                let span = self.source_map.span(start, dot_token.span.end);
                return Ok(CstNode::with_children(
                    NodeKind::ImportPath,
                    span,
                    vec![CstNode::leaf(NodeKind::Identifier, span)],
                ));
            }
        }

        // Parse the first importable name.
        let first = self.parse_import_name()?;

        // If we had leading dots, prefix them to the first identifier. The
        // combined text ("..." + name) is recovered from `span` (contiguous
        // dots + identifier in source), so it no longer needs to be built
        // here (Issue #10126).
        if !leading_dots.is_empty() {
            let span = self.source_map.span(start, first.span.end);
            path.push(CstNode::leaf(NodeKind::Identifier, span));
        } else {
            path.push(first);
        }

        // Parse dotted path: Module.SubModule, including dotted operator
        // components such as `Base.Foo.==.bar`.
        loop {
            if self.check(&Token::Dot) {
                self.advance();
                path.push(self.parse_import_name()?);
                continue;
            }
            if self
                .current
                .as_ref()
                .is_some_and(|token| token.token.is_dotted_operator())
            {
                let token = self.advance_checked(
                    "dotted-operator token already confirmed by is_dotted_operator() above",
                )?;
                // Dotted-operator import components (e.g. `import Base.==`)
                // use the operator's base name (without the leading dot) as
                // the path segment's identifier text. `dotted_operator_base`
                // always strips exactly one leading ASCII `.` byte, so
                // recover it by starting the span one byte past the token's
                // leading `.` instead of storing a normalized copy (Issue
                // #10126).
                let base_span = if token.token.dotted_operator_base().is_some() {
                    self.source_map.span(token.span.start + 1, token.span.end)
                } else {
                    token.span
                };
                path.push(CstNode::leaf(NodeKind::Identifier, base_span));
                continue;
            }
            break;
        }

        // Check for module-level alias: import Base as B
        // `as` is lexed as a plain identifier (Issue #8108); it is only the
        // alias keyword here, in import/using position.
        if self.check_contextual_keyword("as") {
            self.advance(); // consume 'as'
            let alias = self.parse_import_name()?;
            path.push(alias);
        }

        // Parse selective import: Module: func1, func2
        if self.check(&Token::Colon) {
            self.advance();
            self.skip_newlines();
            let func = self.parse_import_item()?;
            path.push(func);

            while self.check(&Token::Comma) {
                self.advance(); // consume comma

                // Skip newlines after comma (line continuation in import)
                while self.check(&Token::Newline) {
                    self.advance();
                }

                if self.is_import_name_start() {
                    path.push(self.parse_import_item()?);
                } else {
                    break;
                }
            }
        }

        let end = self.last_span_end(
            &path,
            "import path always pushes at least one segment above",
        )?;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::ImportPath, span, path))
    }

    /// Parse a single import item, optionally with alias: name or name as alias.
    /// Handles macro names like `@printf` by treating `@` followed by an identifier
    /// as a single name (text becomes `@printf`).
    pub(crate) fn parse_import_item(&mut self) -> ParseResult<CstNode> {
        let name = self.parse_import_dotted_name()?;
        let start = name.span.start;

        // `as` is lexed as a plain identifier (Issue #8108); the alias keyword
        // only when it follows an import item.
        if self.check_contextual_keyword("as") {
            self.advance(); // consume 'as'
            let alias = self.parse_import_name()?;
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

    fn parse_import_dotted_name(&mut self) -> ParseResult<CstNode> {
        let mut name = self.parse_import_name()?;

        while self.check(&Token::Dot) {
            let start = name.span.start;
            self.advance();
            let field = self.parse_import_name()?;
            let span = self.source_map.span(start, field.span.end);
            name = CstNode::with_children(NodeKind::FieldExpression, span, vec![name, field]);
        }

        Ok(name)
    }

    fn is_import_name_start(&self) -> bool {
        self.current
            .as_ref()
            .map(|token| {
                token.token == Token::Identifier
                    || token.token == Token::Dollar
                    || token.token == Token::At
                    || token.token == Token::LParen // Issue #8759: parenthesized operator, e.g. (..)
                    || token.token.is_operator()
                    || token.token.is_operator_keyword()
            })
            .unwrap_or(false)
    }

    /// Parse a single name in an import/export list — a plain identifier, an
    /// operator, or a macro name (`@foo`). Returns an Identifier CstNode whose
    /// text includes the leading `@` for macros.
    pub(crate) fn parse_import_name(&mut self) -> ParseResult<CstNode> {
        self.reject_invalid_operator_identifier()?;

        if self.check(&Token::Dollar) {
            self.parse_prefix()
        } else if self.check(&Token::LParen) {
            let lparen = self.advance_checked("LParen token already matched by check() above")?;
            let start = lparen.span.start;
            // Parenthesized syntactic operators (`import Base: (->)`,
            // `import Base: (&&)`) are rejected by upstream with "expected
            // identifier" spanning the whole parenthesized form
            // (Issues #10917, #10932).
            if self
                .current
                .as_ref()
                .is_some_and(|token| token.token.is_syntactic_operator())
            {
                let op_span = self.current_span();
                self.advance();
                if !self.check(&Token::RParen) {
                    return Err(ParseError::invalid_syntax("invalid identifier", op_span));
                }
                let rparen = self.advance().ok_or_else(|| {
                    ParseError::unexpected_eof("closing parenthesis", self.current_span())
                })?;
                let span = self.source_map.span(start, rparen.span.end);
                return Err(ParseError::invalid_syntax("expected identifier", span));
            }
            let name = if self.current.as_ref().is_some_and(|token| {
                token.token.is_quoted_operator_symbol() || token.token.is_assignment()
            }) {
                let token = self
                    .advance_checked("quoted-operator/assignment token already confirmed above")?;
                CstNode::leaf(NodeKind::Identifier, token.span)
            } else {
                self.parse_import_name()?
            };
            let rparen = self.expect(Token::RParen)?;
            let span = self.source_map.span(start, rparen.span.end);
            Ok(CstNode::with_children(
                NodeKind::ParenthesizedExpression,
                span,
                vec![name],
            ))
        } else if self.check(&Token::Colon) {
            let colon = self.advance_checked("Colon token already matched by check() above")?;
            let start = colon.span.start;
            // Qualified quoted names (`import Base.:(&&)`, `import Base.:->`)
            // are the quoted-symbol grammar path: syntactic operators are
            // valid here even though their unquoted forms are rejected
            // (Issues #10917, #10932).
            let quoted_operator = |parser: &mut Self| -> ParseResult<Option<CstNode>> {
                if parser.current.as_ref().is_some_and(|token| {
                    token.token.is_quoted_operator_symbol() || token.token.is_assignment()
                }) {
                    let token = parser
                        .advance_checked("quoted-operator/assignment token confirmed above")?;
                    Ok(Some(CstNode::leaf(NodeKind::Identifier, token.span)))
                } else {
                    Ok(None)
                }
            };
            let name = if self.check(&Token::LParen) {
                self.advance();
                let inner = match quoted_operator(self)? {
                    Some(inner) => inner,
                    None => self.parse_import_name()?,
                };
                let rparen = self.expect(Token::RParen)?;
                let span = self.source_map.span(start, rparen.span.end);
                CstNode::with_children(NodeKind::ParenthesizedExpression, span, vec![inner])
            } else {
                let inner = match quoted_operator(self)? {
                    Some(inner) => inner,
                    None => self.parse_import_name()?,
                };
                let span = self.source_map.span(start, inner.span.end);
                CstNode::with_children(NodeKind::QuoteExpression, span, vec![inner])
            };
            Ok(name)
        } else if self.check(&Token::At) {
            let at_token = self.advance_checked("At token already matched by check() above")?;
            let start = at_token.span.start;
            let ident = self.parse_identifier()?;
            let end = ident.span.end;
            // Text ("@name") is recovered from `span` (contiguous `@`+identifier
            // in source), so it no longer needs to be built here (Issue #10126).
            let span = self.source_map.span(start, end);
            Ok(CstNode::leaf(NodeKind::Identifier, span))
        } else if self
            .current
            .as_ref()
            .map(|token| token.token.is_operator() || token.token.is_operator_keyword())
            .unwrap_or(false)
        {
            let token =
                self.advance_checked("operator/operator-keyword token already confirmed above")?;
            Ok(CstNode::leaf(NodeKind::Identifier, token.span))
        } else {
            self.parse_identifier_like_name()
        }
    }

    /// Parse export statement: export func1, func2
    /// Supports line continuation after commas and macro names like `@printf`.
    pub(crate) fn parse_export_statement(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwExport)?;
        let start = start_token.span.start;

        self.skip_newlines();
        let first = self.parse_import_name()?;
        let mut names = vec![first];

        while self.check(&Token::Comma) {
            self.advance(); // consume comma

            // Skip newlines after comma (line continuation in export)
            while self.check(&Token::Newline) {
                self.advance();
            }

            names.push(self.parse_import_name()?);
        }

        let end = self.last_span_end(&names, "export statement always pushes `first` above")?;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ExportStatement,
            span,
            names,
        ))
    }

    /// Parse public statement: public foo, bar (Julia 1.11+)
    pub(crate) fn parse_public_statement(&mut self) -> ParseResult<CstNode> {
        // `public` is lexed as a plain Identifier; it is recognized as the
        // public-statement introducer only in statement contexts where it is
        // not followed by `(`, `=`, or `[` — Issue #9637.
        let start_token = self.expect(Token::Identifier)?;
        debug_assert_eq!(start_token.text, "public");
        let start = start_token.span.start;

        self.skip_newlines();
        let first = self.parse_import_name()?;
        let mut names = vec![first];

        while self.check(&Token::Comma) {
            self.advance();

            // Skip newlines after comma (line continuation)
            while self.check(&Token::Newline) {
                self.advance();
            }

            names.push(self.parse_import_name()?);
        }

        let end = self.last_span_end(&names, "public statement always pushes `first` above")?;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::PublicStatement,
            span,
            names,
        ))
    }
}
