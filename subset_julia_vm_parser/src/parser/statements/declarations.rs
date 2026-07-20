//! Variable declaration parsers (const, global, local)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::{Precedence, Token};

use crate::parser::Parser;

impl<'a> Parser<'a> {
    // ==================== Variable Declarations ====================

    /// Parse const declaration: const x = value or const x, y = 1, 2
    pub(crate) fn parse_const_declaration(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwConst)?;
        let start = start_token.span.start;

        if self.check(&Token::KwGlobal) || self.check(&Token::KwLocal) {
            let scoped = if self.check(&Token::KwGlobal) {
                self.parse_scoped_declaration(
                    Token::KwGlobal,
                    NodeKind::GlobalDeclaration,
                    Some(start),
                )?
            } else {
                self.parse_scoped_declaration(
                    Token::KwLocal,
                    NodeKind::LocalDeclaration,
                    Some(start),
                )?
            };
            let span = self.source_map.span(start, scoped.span.end);
            return Ok(CstNode::with_children(
                NodeKind::ConstDeclaration,
                span,
                vec![scoped],
            ));
        }

        // Parse a bare tuple assignment expression
        // A bare `const` is also used for uninitialized const struct fields;
        // scoped const declarations are the branch above and require `=`.
        let expr = self.parse_bare_tuple_assignment()?;
        let end = expr.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ConstDeclaration,
            span,
            vec![expr],
        ))
    }

    /// Parse bare tuple assignment: x, y = 1, 2 or x = 1
    /// This handles comma-separated expressions without parentheses
    pub(crate) fn parse_bare_tuple_assignment(&mut self) -> ParseResult<CstNode> {
        self.reject_invalid_operator_identifier()?;

        // Parse first expression (stopping at comma and assignment)
        let first = if self.current.as_ref().is_some_and(|t| t.token.is_operator())
            && self.peek_next() == Some(Token::Eq)
        {
            let op = self.advance_checked(
                "operator token already confirmed by is_operator() + peek_next()==Eq above",
            )?;
            CstNode::leaf(NodeKind::Operator, op.span)
        } else {
            self.parse_expression_with_precedence(Precedence::Conditional)?
        };

        self.parse_bare_tuple_tail(first)
    }

    pub(crate) fn parse_bare_tuple_tail(&mut self, first: CstNode) -> ParseResult<CstNode> {
        if !self.check(&Token::Comma) {
            // No comma - check for simple assignment
            if self.check(&Token::Eq) {
                let op_token = self.advance_checked("Eq token already matched by check() above")?;
                let op_span = op_token.span;
                let op_node = CstNode::new(NodeKind::Operator, op_span);

                // Line continuation: skip newlines after =
                while self.check(&Token::Newline) {
                    self.advance();
                }

                // Parse right side (which might also be a bare tuple)
                let right = self.parse_bare_tuple_or_expr()?;

                let span = self.source_map.span(first.span.start, right.span.end);
                return Ok(CstNode::with_children(
                    NodeKind::BinaryExpression,
                    span,
                    vec![first, op_node, right],
                ));
            }
            return Ok(first);
        }

        // We have a comma - parse as bare tuple on left side
        let mut left_elements = vec![first];
        while self.check(&Token::Comma) {
            self.advance();
            while self.check(&Token::Newline) {
                self.advance();
            }
            if self.check(&Token::Eq) {
                break;
            }
            // Parse next element (stopping at comma and assignment). Parser
            // corpus invalid-syntax cases include tuple elements that are
            // themselves compound assignments (`if false end, b+=2`), so fold
            // that tail into the element before continuing.
            let mut elem = self.parse_expression_with_precedence(Precedence::Conditional)?;
            if self
                .current
                .as_ref()
                .is_some_and(|t| t.token.is_compound_assignment())
            {
                let op_token = self.advance_checked(
                    "compound-assignment token already confirmed by is_compound_assignment() above",
                )?;
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span);
                let value = self.parse_expression()?;
                let span = self.source_map.span(elem.span.start, value.span.end);
                elem = CstNode::with_children(
                    NodeKind::CompoundAssignmentExpression,
                    span,
                    vec![elem, op_node, value],
                );
            }
            left_elements.push(elem);
        }

        // Create tuple for left side
        let (left_start, left_end) = self.span_bounds(
            &left_elements,
            "bare-tuple left side always pushes `first` above",
        )?;
        let left_span = self.source_map.span(left_start, left_end);
        let left = CstNode::with_children(NodeKind::TupleExpression, left_span, left_elements);

        if !self.check(&Token::Eq) {
            return Ok(left);
        }

        // Expect assignment operator
        let op_token = self.expect(Token::Eq)?;
        let op_span = op_token.span;
        let op_node = CstNode::new(NodeKind::Operator, op_span);

        // Line continuation: skip newlines after =
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Parse right side (might also be a bare tuple)
        let right = self.parse_bare_tuple_or_expr()?;

        let span = self.source_map.span(left.span.start, right.span.end);
        Ok(CstNode::with_children(
            NodeKind::BinaryExpression,
            span,
            vec![left, op_node, right],
        ))
    }

    /// Parse a bare tuple or single expression for the right side of assignment
    pub(crate) fn parse_bare_tuple_or_expr(&mut self) -> ParseResult<CstNode> {
        let first = self.parse_expression_with_precedence(Precedence::Assign)?;
        self.parse_bare_tuple_tail(first)
    }

    /// Parse global declaration: global x or global x = 1 or global x, y
    pub(crate) fn parse_global_declaration(&mut self) -> ParseResult<CstNode> {
        self.parse_scoped_declaration(Token::KwGlobal, NodeKind::GlobalDeclaration, None)
    }

    /// Parse local declaration: local x or local x = 1 or local x, y
    pub(crate) fn parse_local_declaration(&mut self) -> ParseResult<CstNode> {
        self.parse_scoped_declaration(Token::KwLocal, NodeKind::LocalDeclaration, None)
    }

    fn parse_scoped_declaration(
        &mut self,
        keyword: Token,
        scope_kind: NodeKind,
        leading_const_start: Option<usize>,
    ) -> ParseResult<CstNode> {
        let start_token = self.expect(keyword)?;
        let start = start_token.span.start;
        self.skip_newlines();

        let trailing_const_start = if self.check(&Token::KwConst) {
            let const_token = self.expect(Token::KwConst)?;
            if let Some(declaration_start) = leading_const_start {
                self.skip_newlines();
                let end = if self.is_at_end() || self.check(&Token::Semicolon) {
                    const_token.span.end
                } else {
                    match self.parse_bare_tuple_assignment() {
                        Ok(expr) => self.consume_const_error_tail(expr.span.end),
                        Err(error) => error.span().map_or(const_token.span.end, |span| span.end),
                    }
                };
                return Err(ParseError::invalid_syntax(
                    "expected assignment after `const`",
                    self.source_map.span(declaration_start, end),
                ));
            }
            self.skip_newlines();
            Some(const_token.span.start)
        } else {
            None
        };
        let const_declaration_start =
            leading_const_start.or_else(|| trailing_const_start.map(|_| start));

        let items = if let Some(declaration_start) = const_declaration_start {
            vec![self.parse_const_assignment(declaration_start)?]
        } else {
            // Parse first item (identifier or assignment)
            let first = self.parse_var_declaration_item()?;

            if matches!(first.kind, NodeKind::Identifier | NodeKind::TypedExpression)
                && self.check(&Token::Comma)
            {
                // A comma after a plain declared name distributes a trailing
                // `= rhs` over the whole list upstream:
                // `global x, y = 1, 2` is `(global (= (tuple x y) (tuple 1 2)))`,
                // not `global x, (y = 1), 2` (Issue #11009). Route the tail
                // through the shared bare-tuple parser; an assignment-less
                // result unwraps back into per-name items so `global x, y`
                // keeps its established per-identifier CST shape.
                let grouped = self.parse_bare_tuple_tail(first)?;
                if grouped.kind == NodeKind::TupleExpression {
                    grouped.children
                } else {
                    vec![grouped]
                }
            } else {
                let mut items = vec![first];
                while self.check(&Token::Comma) {
                    self.advance();
                    items.push(self.parse_var_declaration_item()?);
                }
                items
            }
        };

        let end = self.last_span_end(&items, "scoped declaration always pushes `first` above")?;
        let span = self.source_map.span(start, end);
        let scoped = CstNode::with_children(scope_kind, span, items);
        if trailing_const_start.is_some() {
            Ok(CstNode::with_children(
                NodeKind::ConstDeclaration,
                span,
                vec![scoped],
            ))
        } else {
            Ok(scoped)
        }
    }

    fn parse_const_assignment(&mut self, declaration_start: usize) -> ParseResult<CstNode> {
        if self.is_at_end() {
            return Err(ParseError::invalid_syntax(
                "expected assignment after `const`",
                self.source_map
                    .span(declaration_start, self.current_span().end),
            ));
        }

        let token = self.current.as_ref().ok_or_else(|| {
            ParseError::unexpected_eof("variable declaration", self.current_span())
        })?;
        let error_end = if token.token == Token::Semicolon {
            Some(token.span.start)
        } else if (token.token.is_keyword() && !token.token.is_operator_keyword())
            || matches!(token.token, Token::Eq | Token::RParen | Token::Comma)
        {
            Some(token.span.end)
        } else {
            None
        };
        if let Some(error_end) = error_end {
            return Err(ParseError::invalid_syntax(
                "expected assignment after `const`",
                self.source_map.span(declaration_start, error_end),
            ));
        }

        let expr = self.parse_bare_tuple_assignment()?;
        let is_assignment = expr.kind == NodeKind::BinaryExpression
            && expr.children.get(1).is_some_and(|operator| {
                operator.kind == NodeKind::Operator && operator.text_from_source(self.source) == "="
            });
        if !is_assignment {
            let error_end = self.consume_const_error_tail(expr.span.end);
            return Err(ParseError::invalid_syntax(
                "expected assignment after `const`",
                self.source_map.span(declaration_start, error_end),
            ));
        }
        Ok(expr)
    }

    fn consume_const_error_tail(&mut self, parsed_end: usize) -> usize {
        let mut error_end = parsed_end;
        loop {
            let has_lower_precedence_tail = self.current.as_ref().is_some_and(|token| {
                token
                    .token
                    .binary_precedence()
                    .is_some_and(|(precedence, _)| precedence < Precedence::Conditional)
            });
            if !has_lower_precedence_tail {
                return error_end;
            }

            let Some(operator) = self.advance() else {
                return error_end;
            };
            self.skip_newlines();
            if self.is_at_end() || self.check(&Token::Semicolon) {
                return operator.span.end;
            }
            match self.parse_bare_tuple_or_expr() {
                Ok(rhs) => error_end = rhs.span.end,
                Err(error) => {
                    return error.span().map_or(operator.span.end, |span| span.end);
                }
            }
        }
    }

    /// Parse a single variable declaration item: x or x = expr or x::T or x::T = expr
    /// Also supports compound assignments: x += expr, x -= expr, etc.
    pub(crate) fn parse_var_declaration_item(&mut self) -> ParseResult<CstNode> {
        if self.check(&Token::LParen) {
            return self.parse_bare_tuple_assignment();
        }

        if self.check(&Token::Dollar) {
            let target = self.parse_prefix()?;
            return self.parse_var_declaration_item_tail(target);
        }

        // Short-form function definition as a `local`/`global` declaration item:
        // `local f(args) = body`, `local f(args) where {T} = body` (Issue #8065).
        // A `(` immediately following the declared name is a call signature, so the
        // item is a function definition, not a bare variable. Parse it with the
        // general expression parser so it yields the same `Assignment` (with a
        // `CallExpression`/`WhereExpression` target) a top-level `f(args) = body`
        // produces — a structure the local/global lowering and the quote
        // constructor already understand. Without this the parser stopped after
        // the bare name and mis-parsed the `(...) = body` remainder as a separate
        // statement.
        if matches!(
            self.current.as_ref().map(|t| &t.token),
            Some(Token::Identifier)
        ) && matches!(self.peek_next(), Some(Token::LParen))
        {
            return self.parse_expression();
        }

        // Long-form definitions are valid scoped declaration items and must
        // dispatch before the generic reserved-keyword rejection (Issue #10937).
        if self.check(&Token::KwFunction) {
            return self.parse_function_definition();
        }
        if self.check(&Token::KwMacro) {
            return self.parse_macro_definition();
        }

        // Upstream's `(global local)` parser arm routes through `parse-eq`
        // (julia/src/julia-parser.scm), so ANY expression — including reserved
        // word definitions, control flow, modules, and jump/import statements —
        // parses as the declaration's child (Issue #10945). Invalid combinations
        // are rejected AT LOWERING with `invalid syntax in "global" declaration`,
        // matching upstream's phase split. Delegating to the construct parsers
        // also keeps incomplete-input classification correct: `global module`
        // at EOF surfaces the construct parser's UnexpectedEof instead of a
        // permanent invalid-keyword error.
        if let Some(current) = self.current.as_ref().map(|t| t.token.clone()) {
            match current {
                Token::KwModule | Token::KwBaremodule => {
                    return self.parse_module_definition();
                }
                Token::KwStruct | Token::KwMutable => return self.parse_struct_definition(),
                Token::KwAbstract => return self.parse_abstract_definition(),
                Token::KwPrimitive => return self.parse_primitive_definition(),
                Token::KwIf => return self.parse_if_statement(),
                Token::KwFor => return self.parse_for_statement(),
                Token::KwWhile => return self.parse_while_statement(),
                Token::KwTry => return self.parse_try_statement(),
                Token::KwBegin => return self.parse_begin_block(),
                Token::KwLet => return self.parse_expression(),
                Token::KwQuote => return self.parse_quote_expression(),
                Token::KwReturn => return self.parse_return_statement(),
                Token::KwBreak => return self.parse_break_statement(),
                Token::KwContinue => return self.parse_continue_statement(),
                Token::KwUsing => return self.parse_using_statement(),
                Token::KwImport => return self.parse_import_statement(),
                Token::KwExport => return self.parse_export_statement(),
                // Nested scope modifiers (`global global x`, `global local x`)
                // also parse upstream and are rejected at lowering.
                Token::KwGlobal => return self.parse_global_declaration(),
                Token::KwLocal => return self.parse_local_declaration(),
                _ => {}
            }
        }

        let token = self.current.as_ref().ok_or_else(|| {
            ParseError::unexpected_eof("variable declaration", self.current_span())
        })?;
        if token.token.is_keyword() && !token.token.is_operator_keyword() {
            return Err(ParseError::invalid_syntax("invalid identifier", token.span));
        }

        let is_identifier = token.token == Token::Identifier;
        let is_operator_identifier = token.token.is_operator_keyword()
            || (token.token.is_operator_identifier() && !token.token.is_assignment());
        if !is_identifier && !is_operator_identifier {
            return self.parse_expression();
        }

        let mut target = if is_identifier {
            self.parse_identifier_like_name()?
        } else {
            self.parse_identifier()?
        };
        let ident_start = target.span.start;

        if self.check(&Token::LBrace) {
            target = self.parse_parametric_type(target)?;
        }

        // Check for type annotation
        let typed = if self.check(&Token::DoubleColon) {
            self.advance();
            let type_expr = self.parse_type_expression()?;
            let span = self.source_map.span(ident_start, type_expr.span.end);
            CstNode::with_children(NodeKind::TypedExpression, span, vec![target, type_expr])
        } else {
            target
        };

        self.parse_var_declaration_item_tail(typed)
    }

    fn parse_var_declaration_item_tail(&mut self, typed: CstNode) -> ParseResult<CstNode> {
        let ident_start = typed.span.start;
        // Check for initialization with simple assignment
        if self.check(&Token::Eq) {
            let op_token = self.advance_checked("Eq token already matched by check() above")?;
            let op_span = op_token.span;
            let op_node = CstNode::new(NodeKind::Operator, op_span);

            let value = self.parse_expression()?;
            let span = self.source_map.span(ident_start, value.span.end);
            Ok(CstNode::with_children(
                NodeKind::BinaryExpression,
                span,
                vec![typed, op_node, value],
            ))
        } else if self
            .current
            .as_ref()
            .map(|t| t.token.is_compound_assignment())
            .unwrap_or(false)
        {
            // Compound assignment: x += expr, x -= expr, etc.
            let op_token =
                self.advance_checked("compound-assignment token already confirmed above")?;
            let op_span = op_token.span;
            let op_node = CstNode::leaf(NodeKind::Operator, op_span);

            let value = self.parse_expression()?;
            let span = self.source_map.span(ident_start, value.span.end);
            Ok(CstNode::with_children(
                NodeKind::CompoundAssignmentExpression,
                span,
                vec![typed, op_node, value],
            ))
        } else if self
            .current
            .as_ref()
            .is_some_and(|t| t.token.binary_precedence().is_some())
        {
            // Upstream parses the declaration item as a full expression
            // (`parse-eq`), so an operator tail such as `global c + 1` or
            // `global c => 2` must stay part of the declaration instead of
            // silently splitting into `global c` plus a stray expression
            // statement (Issue #10945). The resulting non-name child is
            // rejected at lowering with the upstream
            // `invalid syntax in "global" declaration` error.
            let mut left = typed;
            while self
                .current
                .as_ref()
                .is_some_and(|t| t.token.binary_precedence().is_some())
            {
                let op_token =
                    self.advance_checked("binary operator confirmed by binary_precedence() above")?;
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span);
                self.skip_newlines();
                let right = self.parse_expression_with_precedence(Precedence::Pair)?;
                let span = self.source_map.span(left.span.start, right.span.end);
                left = CstNode::with_children(
                    NodeKind::BinaryExpression,
                    span,
                    vec![left, op_node, right],
                );
            }
            Ok(left)
        } else {
            Ok(typed)
        }
    }
}
