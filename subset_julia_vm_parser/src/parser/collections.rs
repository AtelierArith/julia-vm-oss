//! Collection parsing for Julia subset
//!
//! Handles parsing of tuples, arrays, comprehensions, and matrices.

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::Token;

use super::Parser;

impl<'a> Parser<'a> {
    /// Parse parenthesized expression or tuple
    pub(crate) fn parse_parenthesized_or_tuple(&mut self) -> ParseResult<CstNode> {
        // The interior of a parenthesized group parses without macro-argument
        // whitespace sensitivity; it is restored before returning (Issue #5494).
        // Likewise, a matrix-row context does not extend into the `(...)`, so a
        // `-` inside `[1 (2 - 3)]` stays binary (Issue #7196).
        let saved_space_sensitive = std::mem::replace(&mut self.macro_arg_space_sensitive, false);
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        let result = self.parse_parenthesized_or_tuple_inner();
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.in_ternary_then = saved_in_ternary_then;
        result
    }

    fn parse_parenthesized_or_tuple_inner(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LParen)?;
        let start = start_token.span.start;

        while self.check(&Token::Newline) {
            self.advance();
        }

        // Check for empty tuple
        if self.check(&Token::RParen) {
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            return Ok(CstNode::new(NodeKind::TupleExpression, span));
        }

        // Check for operator as value: (+), (-), (*), etc.
        // Look ahead: is it `(operator)`?
        if let Some(token) = &self.current {
            if token.token.is_operator() {
                // Peek at next token to see if it's )
                if let Some(next) = self.peek_next() {
                    if next == Token::RParen {
                        // It's an operator as value
                        let op_token = self.advance().unwrap();
                        let end_token = self.advance().unwrap();
                        let span = self.source_map.span(start, end_token.span.end);
                        let op_span = op_token.span;
                        let op_node = CstNode::leaf(NodeKind::Operator, op_span, op_token.text);
                        return Ok(CstNode::with_children(
                            NodeKind::ParenthesizedExpression,
                            span,
                            vec![op_node],
                        ));
                    }
                }
            }
        }

        // Keyword-only arrow function head: (; y=1) -> body.
        if self.check(&Token::Semicolon) {
            return self.parse_arrow_parameter_list_rest(start, Vec::new());
        }

        // Parse first expression
        let first = self.parse_expression()?;
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Check for generator expression: (expr for x in iter)
        if self.check(&Token::KwFor) {
            return self.parse_generator_rest(start, first);
        }

        // Check for keyword-parameter arrow function head: (x; y=1) -> body.
        if self.check(&Token::Semicolon) {
            return self.parse_arrow_parameter_list_rest(start, vec![first]);
        }

        // Check for comma (tuple) or closing paren (parenthesized)
        if self.check(&Token::Comma) {
            // It's a tuple
            let mut elements = vec![first];
            while self.check(&Token::Comma) {
                self.advance(); // consume comma

                // Skip newlines before checking for the closing paren so
                // multi-line trailing-comma tuples like `(1,\n 2,\n)`
                // parse correctly (Issue #4776). Previously the
                // trailing-comma RParen check ran before newline-skip
                // and the parser then tried to parse_expression on the
                // RParen, failing with "expected expression".
                while self.check(&Token::Newline) {
                    self.advance();
                }

                // Allow trailing comma
                if self.check(&Token::RParen) {
                    break;
                }

                elements.push(self.parse_expression()?);
                while self.check(&Token::Newline) {
                    self.advance();
                }
            }

            if self.check(&Token::Semicolon) {
                return self.parse_arrow_parameter_list_rest(start, elements);
            }

            // Skip newlines before the closing paren so the no-
            // trailing-comma multi-line shape `(1,\n 2,\n 3\n)`
            // parses (Issue #4776). The trailing-comma form is
            // already handled by the in-loop newline skip above.
            while self.check(&Token::Newline) {
                self.advance();
            }

            let end_token = self.expect(Token::RParen)?;
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::TupleExpression,
                span,
                elements,
            ))
        } else {
            // It's a parenthesized expression
            let end_token = self.expect(Token::RParen)?;
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::ParenthesizedExpression,
                span,
                vec![first],
            ))
        }
    }

    fn parse_arrow_parameter_list_rest(
        &mut self,
        start: usize,
        mut params: Vec<CstNode>,
    ) -> ParseResult<CstNode> {
        // Whether an expression appeared before the first `;`. With one
        // (`(x = 1; ...)`) and no trailing `->`, this is a parenthesized statement
        // block, not an arrow parameter list or a named tuple (Issue #5741).
        let had_expr_before_semi = !params.is_empty();

        // Parse the content after each `;` as full EXPRESSIONS (not keyword
        // parameters), so a block statement like `x + 1` — which is not a valid
        // parameter — parses. `;` separates statements / positional-from-keyword;
        // `,` separates keyword parameters in the arrow form.
        while self.check(&Token::Semicolon) {
            let semi_token = self.expect(Token::Semicolon)?;
            params.push(CstNode::leaf(
                NodeKind::Semicolon,
                semi_token.span,
                semi_token.text,
            ));
            while self.check(&Token::Newline) {
                self.advance();
            }
            if self.check(&Token::RParen) || self.check(&Token::Semicolon) {
                continue;
            }
            loop {
                params.push(self.parse_expression()?);
                while self.check(&Token::Newline) {
                    self.advance();
                }
                if self.check(&Token::Comma) {
                    self.advance();
                    while self.check(&Token::Newline) {
                        self.advance();
                    }
                    if self.check(&Token::RParen) || self.check(&Token::Semicolon) {
                        break;
                    }
                    continue;
                }
                break;
            }
        }

        let end_token = self.expect(Token::RParen)?;
        let span = self.source_map.span(start, end_token.span.end);

        // `(expr; ...)` with no trailing `->` is a statement block: evaluate the
        // statements and yield the last (Issue #5741). A leading-`;` group
        // (`(; a = 1)`) stays a ParameterList → NamedTuple, and `(...) -> body`
        // stays a ParameterList → arrow.
        if had_expr_before_semi && !self.check(&Token::Arrow) {
            let statements: Vec<CstNode> = params
                .into_iter()
                .filter(|c| c.kind != NodeKind::Semicolon)
                .collect();
            return Ok(CstNode::with_children(NodeKind::Block, span, statements));
        }

        // Arrow form `(...; kw = v, ...) -> body`: a keyword parameter parsed as a
        // full expression is an `Assignment` node `[name, =, value]`; rewrap it as a
        // `KwParameter [name, value]` so the arrow lowering recognizes it. (The
        // leading-`;` NamedTuple lowering accepts either shape, so only the arrow
        // form needs this.)
        if self.check(&Token::Arrow) {
            params = params
                .into_iter()
                .map(|node| {
                    if node.kind == NodeKind::Assignment
                        && node.children.len() == 3
                        && node.children[0].kind == NodeKind::Identifier
                        && node.children[1].kind == NodeKind::Operator
                    {
                        // `name = value` keyword parameter
                        let kw_span = node.span;
                        let mut kids = node.children;
                        kids.remove(1);
                        CstNode::with_children(NodeKind::KwParameter, kw_span, kids)
                    } else if node.kind == NodeKind::SplatExpression {
                        // `kwargs...` keyword-varargs parameter
                        CstNode::with_children(NodeKind::SplatParameter, node.span, node.children)
                    } else {
                        node
                    }
                })
                .collect();
        }

        Ok(CstNode::with_children(
            NodeKind::ParameterList,
            span,
            params,
        ))
    }

    /// Parse rest of generator expression (expr for ...)
    pub(crate) fn parse_generator_rest(
        &mut self,
        start: usize,
        expr: CstNode,
    ) -> ParseResult<CstNode> {
        self.parse_generator_rest_opts(start, expr, true)
    }

    /// Parse the `for ... [if ...]` tail of a generator. When `consume_rparen`
    /// is true the closing `)` is required and consumed (bare parenthesized
    /// generator `(x for x in it)`); when false the closing `)` is left for the
    /// caller, so a generator used as a call argument can be followed by
    /// keyword arguments — `f(x for x in it; kw=v)` (Issue #5763).
    pub(crate) fn parse_generator_rest_opts(
        &mut self,
        start: usize,
        expr: CstNode,
        consume_rparen: bool,
    ) -> ParseResult<CstNode> {
        let mut children = vec![expr];

        // Inside `(...)` newlines are insignificant, so skip any newlines that
        // precede the trailing `for`/`if` clauses so a multi-line generator
        // parses identically to the single-line form (Issue #8008).
        self.skip_newlines();

        // Parse for clause(s)
        while self.check(&Token::KwFor) {
            children.push(self.parse_for_clause()?);
            self.skip_newlines();
        }

        // Parse optional if clause
        if self.check(&Token::KwIf) {
            children.push(self.parse_if_clause()?);
        }

        let end = if consume_rparen {
            self.skip_newlines();
            self.expect(Token::RParen)?.span.end
        } else {
            children.last().map_or(start, |child| child.span.end)
        };
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::Generator, span, children))
    }

    /// Parse array literal or comprehension
    pub(crate) fn parse_array_or_comprehension(&mut self) -> ParseResult<CstNode> {
        // The interior of an array/comprehension parses without macro-argument
        // whitespace sensitivity; it is restored before returning (Issue #5494).
        // The matrix-row context is also reset here so a nested `[...]` literal
        // starts its own fresh row context (Issue #7196); the inner function
        // re-establishes it per element/row.
        let saved_space_sensitive = std::mem::replace(&mut self.macro_arg_space_sensitive, false);
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        let result = self.parse_array_or_comprehension_inner();
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.in_ternary_then = saved_in_ternary_then;
        result
    }

    fn parse_array_or_comprehension_inner(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LBracket)?;
        let start = start_token.span.start;

        // Skip newlines after `[` so multi-line literals like
        // `[\n  1,\n  2,\n]` parse correctly (Issue #4776). The
        // parenthesized-tuple entry point already does this; the
        // vector entry point did not.
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Check for empty array
        if self.check(&Token::RBracket) {
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            return Ok(CstNode::new(NodeKind::VectorExpression, span));
        }

        // Parse first element. A `[...]` literal may turn out to be a matrix
        // row (`[a b c]`, `[a b; c d]`), so the FIRST element is also parsed in
        // the whitespace-sensitive matrix-row context: in `[0.20 -0.26; ...]`
        // the `-0.26` is a second element, not `0.20 - 0.26` (Issue #7196).
        // This only affects a space-separated `+`/`-` with no trailing space;
        // `[1, -2]` (comma) and `[1 - 2]` (binary, space after) are unchanged.
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        let first = self.parse_expression()?;
        self.in_matrix_row = saved_in_matrix_row;

        if self.check(&Token::Newline) && self.peek_next() == Some(Token::KwFor) {
            self.advance();
        }

        // Check what follows
        if self.check(&Token::KwFor)
            || (self.check(&Token::Newline) && self.peek_non_newline_token() == Some(Token::KwFor))
        {
            while self.check(&Token::Newline) {
                self.advance();
            }
            // Comprehension: [expr for x in iter]
            self.parse_comprehension_rest(start, first)
        } else if self.check(&Token::Comma) {
            // Vector: [a, b, c]
            self.parse_vector_rest(start, first)
        } else if self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            // Matrix: [a b; c d] or [a b\n c d]
            self.parse_matrix_rest(start, first)
        } else if self.check(&Token::RBracket) {
            // Single element vector
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::VectorExpression,
                span,
                vec![first],
            ))
        } else {
            // Could be matrix row: [a b c]
            self.parse_matrix_row_rest(start, first)
        }
    }

    /// Parse rest of vector [first, ...]
    pub(crate) fn parse_vector_rest(
        &mut self,
        start: usize,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let mut elements = vec![first];

        while self.check(&Token::Comma) {
            self.advance(); // consume comma

            // Skip newlines after comma (line continuation in arrays)
            while self.check(&Token::Newline) {
                self.advance();
            }

            // Allow trailing comma
            if self.check(&Token::RBracket) {
                break;
            }

            elements.push(self.parse_expression()?);
        }

        // Skip newlines before the closing bracket so the no-trailing-
        // comma multi-line shape `[1,\n 2,\n 3\n]` also parses
        // (Issue #4776). The trailing-comma form is already handled by
        // the in-loop newline skip above.
        while self.check(&Token::Newline) {
            self.advance();
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::VectorExpression,
            span,
            elements,
        ))
    }

    /// Parse rest of comprehension [first for ...]
    pub(crate) fn parse_comprehension_rest(
        &mut self,
        start: usize,
        expr: CstNode,
    ) -> ParseResult<CstNode> {
        let mut children = vec![expr];

        // Inside `[...]` newlines are insignificant, so skip any newlines that
        // precede the trailing `for`/`if` clauses (and the closing bracket).
        // This makes a multi-line comprehension such as
        // `[x for x in xs⏎ if x > 0]` parse identically to the single-line
        // form (Issue #8008).
        self.skip_newlines();

        // Parse for clause(s)
        while self.check(&Token::KwFor) {
            children.push(self.parse_for_clause()?);
            self.skip_newlines();
        }

        // Parse optional if clause
        if self.check(&Token::KwIf) {
            children.push(self.parse_if_clause()?);
            self.skip_newlines();
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::ComprehensionExpression,
            span,
            children,
        ))
    }

    /// Parse for clause in comprehension
    /// Supports both:
    /// - Single binding: for x in iter
    /// - Multiple bindings (2D): for x in iter, y in iter
    pub(crate) fn parse_for_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwFor)?;
        let start = start_token.span.start;

        let mut bindings = vec![self.parse_for_binding()?];

        // Parse additional comma-separated bindings (2D comprehension)
        // Check if comma is followed by identifier (next binding) vs end of clause.
        // Newlines are insignificant inside `[...]`/`(...)`, so look past any
        // newline after the comma and skip it before parsing the next binding so
        // `[expr for i in A,⏎ j in B]` parses like the single-line form
        // (Issue #8008).
        while self.check(&Token::Comma) {
            // Peek at the first non-newline token after the comma to see if it
            // looks like another binding.
            if self.peek_non_newline_token_after_current() == Some(Token::Identifier) {
                self.advance(); // consume comma
                self.skip_newlines();
                bindings.push(self.parse_for_binding()?);
                continue;
            }
            break;
        }

        let end = bindings.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::ForClause, span, bindings))
    }

    /// Parse if clause in comprehension
    pub(crate) fn parse_if_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwIf)?;
        let start = start_token.span.start;

        let condition = self.parse_expression()?;

        let end = condition.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::IfClause,
            span,
            vec![condition],
        ))
    }

    /// Parse for binding: [outer] var in/= expr
    /// Also supports tuple destructuring: for (a, b) in expr
    pub(crate) fn parse_for_binding(&mut self) -> ParseResult<CstNode> {
        // `outer` is contextual: it is a modifier in `for outer i in itr`,
        // but a normal loop variable in `for outer in itr` (Issue #6414).
        // It is lexed as a plain identifier (Issue #8099), so detect it by text.
        let has_outer = self.check_contextual_keyword("outer")
            && !matches!(
                self.peek_next(),
                Some(Token::KwIn | Token::Eq | Token::ElementOf)
            );
        let start = self.current.as_ref().map(|t| t.span.start).unwrap_or(0);

        if has_outer {
            self.advance(); // consume 'outer'
        }

        // Check for tuple pattern: (a, b, ...)
        let var = if self.check(&Token::LParen) {
            self.parse_tuple_pattern()?
        } else {
            let ident = self.parse_identifier()?;
            // Issue #8208: an optional type annotation on a single loop variable,
            // `for i::T in itr`, types (converts) each iterate value to `T`,
            // matching upstream Julia. Only a bare identifier may be annotated —
            // `for (a, b)::T in itr` is a syntax error upstream too — so this is on
            // the non-tuple path. Lowering turns the resulting `TypedExpression`
            // binding into a `convert(T, i)` at the top of the loop body.
            if self.check(&Token::DoubleColon) {
                self.parse_type_declaration(ident)?
            } else {
                ident
            }
        };

        // Expect 'in' or '=' or '∈'
        if !self.check_any(&[Token::KwIn, Token::Eq, Token::ElementOf]) {
            return Err(ParseError::unexpected_token(
                self.current
                    .as_ref()
                    .map(|t| t.text.to_string())
                    .unwrap_or_default(),
                "'in' or '='",
                self.current_span(),
            ));
        }
        self.advance(); // consume in/=/∈

        let iter = self.parse_expression()?;
        let end = iter.span.end;

        let span = self.source_map.span(start, end);
        let mut children = vec![var, iter];

        // If outer, add a marker node at the beginning
        if has_outer {
            let outer_marker = CstNode::leaf(
                NodeKind::Identifier,
                self.source_map.span(start, start + 5), // "outer" is 5 chars
                "outer".to_string(),
            );
            children.insert(0, outer_marker);
        }

        Ok(CstNode::with_children(NodeKind::ForBinding, span, children))
    }

    /// Parse tuple pattern for destructuring: (a, b, ...)
    /// Returns a TupleExpression containing Identifiers
    pub(crate) fn parse_tuple_pattern(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LParen)?;
        let start = start_token.span.start;
        let mut elements = Vec::new();

        // Parse first identifier
        elements.push(self.parse_identifier()?);

        // Parse remaining comma-separated identifiers
        while self.check(&Token::Comma) {
            self.advance(); // consume comma

            // Allow trailing comma
            if self.check(&Token::RParen) {
                break;
            }

            elements.push(self.parse_identifier()?);
        }

        let end_token = self.expect(Token::RParen)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TupleExpression,
            span,
            elements,
        ))
    }

    /// Parse rest of matrix [first row; ...]
    pub(crate) fn parse_matrix_rest(
        &mut self,
        start: usize,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        // First element is part of first row
        let mut first_row = vec![first];

        // Parse rest of first row (space-separated). Inside a matrix row a
        // space-separated `+`/`-` with no trailing space starts a new
        // (unary-signed) element, so signal the whitespace-sensitive context
        // (Issue #7196). The flag is cleared on entering any grouping, so
        // nested `(...)`/`[...]`/calls keep ordinary binary `+`/`-`.
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        while !self.check_any(&[Token::Semicolon, Token::Newline, Token::RBracket])
            && !self.is_at_end()
        {
            first_row.push(self.parse_expression()?);
        }
        self.in_matrix_row = saved_in_matrix_row;

        let mut rows = vec![CstNode::with_children(
            NodeKind::MatrixRow,
            self.source_map
                .span(first_row[0].span.start, first_row.last().unwrap().span.end),
            first_row,
        )];

        // Parse remaining rows
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            self.advance(); // consume ; or newline

            // Skip extra newlines
            while self.check(&Token::Newline) {
                self.advance();
            }

            if self.check(&Token::RBracket) {
                break;
            }

            rows.push(self.parse_matrix_row()?);
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::MatrixExpression,
            span,
            rows,
        ))
    }

    /// Parse matrix row (single row starting with first element already parsed)
    pub(crate) fn parse_matrix_row_rest(
        &mut self,
        start: usize,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let mut elements = vec![first];

        // Parse space-separated elements in the whitespace-sensitive matrix-row
        // context so `[1 -2]` yields two elements (Issue #7196).
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        while !self.check_any(&[Token::Semicolon, Token::Newline, Token::RBracket])
            && !self.is_at_end()
        {
            elements.push(self.parse_expression()?);
        }
        self.in_matrix_row = saved_in_matrix_row;

        // If just one row ending with ], it's a row vector
        if self.check(&Token::RBracket) {
            let end_token = self.advance().unwrap();
            let span = self.source_map.span(start, end_token.span.end);
            let row = CstNode::with_children(
                NodeKind::MatrixRow,
                self.source_map
                    .span(elements[0].span.start, elements.last().unwrap().span.end),
                elements,
            );
            return Ok(CstNode::with_children(
                NodeKind::MatrixExpression,
                span,
                vec![row],
            ));
        }

        // Create first row from all collected elements
        let first_row = CstNode::with_children(
            NodeKind::MatrixRow,
            self.source_map
                .span(elements[0].span.start, elements.last().unwrap().span.end),
            elements,
        );

        let mut rows = vec![first_row];

        // Parse remaining rows
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            self.advance(); // consume ; or newline

            // Skip extra newlines
            while self.check(&Token::Newline) {
                self.advance();
            }

            if self.check(&Token::RBracket) {
                break;
            }

            rows.push(self.parse_matrix_row()?);
        }

        let end_token = self.expect(Token::RBracket)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::MatrixExpression,
            span,
            rows,
        ))
    }

    /// Parse a single matrix row
    pub(crate) fn parse_matrix_row(&mut self) -> ParseResult<CstNode> {
        // The whole row — including its first element — is whitespace-sensitive
        // so `[1 1; 2 -3]` parses `2 -3` as two elements (Issue #7196).
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        let first = self.parse_expression()?;
        let start = first.span.start;
        let mut elements = vec![first];

        // Parse space-separated elements
        while !self.check_any(&[Token::Semicolon, Token::Newline, Token::RBracket])
            && !self.is_at_end()
        {
            elements.push(self.parse_expression()?);
        }
        self.in_matrix_row = saved_in_matrix_row;

        let end = elements.last().unwrap().span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::MatrixRow, span, elements))
    }
}
