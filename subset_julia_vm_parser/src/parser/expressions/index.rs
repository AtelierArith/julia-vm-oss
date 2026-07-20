//! Index expression parsers

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse an index expression or typed array/matrix
    /// Handles: obj[i], obj[i, j], Type[1, 2, 3] (typed vector), Type[1 2; 3 4] (typed matrix)
    pub(crate) fn parse_index_expression(&mut self, object: CstNode) -> ParseResult<CstNode> {
        // Inside the brackets we leave macro-argument whitespace sensitivity
        // behind; it is restored before returning (Issue #5494). Any outer
        // matrix-row context is also reset; the typed-matrix-row helpers below
        // re-establish it per element so `Float64[1 -2]` parses two elements
        // (Issue #7196), while `a[i, j]` indexing is unaffected.
        let saved_space_sensitive = std::mem::replace(&mut self.macro_arg_space_sensitive, false);
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, false);
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        self.grouping_depth += 1;
        let saved_end_symbol_depth = self.end_symbol_depth;
        let result = self.with_end_symbol_depth(saved_end_symbol_depth + 1, |parser| {
            parser.parse_index_expression_inner(object, saved_end_symbol_depth)
        });
        self.grouping_depth -= 1;
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
        self.in_ternary_then = saved_in_ternary_then;
        result
    }

    fn parse_index_expression_inner(
        &mut self,
        object: CstNode,
        saved_end_symbol_depth: usize,
    ) -> ParseResult<CstNode> {
        let start = object.span.start;
        let bracket_token = self.expect(Token::LBracket)?;
        let bracket_start = bracket_token.span.start;

        // Skip newlines right after `[` so multi-line typed literals like
        // `Bool[\n true,\n false,\n]` parse like their untyped `[...]` counterpart
        // (Issue #8188). A newline immediately after `[` is cosmetic — it has no
        // preceding element, so it can never be a matrix-row separator.
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Empty brackets are syntactically zero-argument indexing (`a[]`).
        // Lowering decides whether the left side is a type name and should become
        // a typed empty array (`T[]`).
        if self.check(&Token::RBracket) {
            let end_token =
                self.advance_checked("RBracket token already matched by check() above")?;
            let span = self.source_map.span(start, end_token.span.end);
            return Ok(CstNode::with_children(
                NodeKind::IndexExpression,
                span,
                vec![object],
            ));
        }

        if self.check(&Token::Semicolon) {
            // Once the bracket is known to be a cat expression, `end` falls
            // back to the surrounding dynamic context. At top level that
            // rejects `a[; end]`; nested inside another ref it remains valid.
            return self.with_end_symbol_depth(saved_end_symbol_depth, |parser| {
                let ncat = parser.parse_empty_ncat_rest(bracket_start, Token::RBracket)?;
                let span = parser.source_map.span(start, ncat.span.end);
                Ok(CstNode::with_children(
                    NodeKind::TypedExpression,
                    span,
                    vec![object, ncat],
                ))
            });
        }

        // Parse first element. As with `[...]` literals, a `T[...]`/`a[...]`
        // bracket may be a typed matrix/`hcat` row, so the first element parses
        // in the whitespace-sensitive matrix-row context: `Float64[0.20 -0.26]`
        // is two elements, and `a[i +j]` is typed-hcat (matching upstream),
        // while `a[i + j]` and `a[i, j]` are ordinary indexing (Issue #7196).
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, true);
        let first = self.parse_expression()?;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;

        if self.check(&Token::Newline) && self.peek_next() == Some(Token::KwFor) {
            self.advance();
        }

        // Check what follows to determine the type
        if self.check(&Token::KwFor)
            || (self.check(&Token::Newline) && self.peek_non_newline_token() == Some(Token::KwFor))
        {
            while self.check(&Token::Newline) {
                self.advance();
            }
            // Typed comprehension: Type[expr for x in iter]
            // The element expression was parsed with ref-style `end`, but
            // iteration specifications are outside that scope in upstream.
            let comprehension = self.with_end_symbol_depth(0, |parser| {
                parser.parse_comprehension_rest(bracket_start, first)
            })?;
            let span = self.source_map.span(start, comprehension.span.end);
            Ok(CstNode::with_children(
                NodeKind::TypedExpression,
                span,
                vec![object, comprehension],
            ))
        } else if self.check(&Token::Comma) {
            // Comma-separated: either index or typed vector
            let mut elements = vec![first];
            while self.check(&Token::Comma) {
                self.advance(); // consume comma

                // Skip newlines
                while self.check(&Token::Newline) {
                    self.advance();
                }

                // Allow trailing comma
                if self.check(&Token::RBracket) {
                    break;
                }

                elements.push(self.parse_expression()?);
            }

            // Skip a cosmetic trailing newline before `]` when the last element
            // had no trailing comma, e.g. `obj[\n i,\n j\n]` (Issue #8188).
            while self.check(&Token::Newline) {
                self.advance();
            }

            let end_token = self.expect(Token::RBracket)?;
            let span = self.source_map.span(start, end_token.span.end);

            // For now, treat as index expression (Type[i, j] for indexing)
            // Typed vectors like Int[1, 2, 3] are also valid as indexing syntax
            let mut children = vec![object];
            children.extend(elements);
            Ok(CstNode::with_children(
                NodeKind::IndexExpression,
                span,
                children,
            ))
        } else if self.check(&Token::RBracket) {
            // Single element: obj[i] or Type[expr]
            let end_token =
                self.advance_checked("RBracket token already matched by check() above")?;
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::IndexExpression,
                span,
                vec![object, first],
            ))
        } else if self.check(&Token::Newline)
            && self.peek_non_newline_token() == Some(Token::RBracket)
        {
            // A newline (or blank lines) before `]` with no further element is a
            // cosmetic trailing newline, so this is single-element indexing
            // `obj[\n i\n]`, not a vcat (Issue #8188). A newline *followed by*
            // another element falls through to the matrix arm below.
            while self.check(&Token::Newline) {
                self.advance();
            }
            let end_token = self.expect(Token::RBracket)?;
            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::IndexExpression,
                span,
                vec![object, first],
            ))
        } else if self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            // Matrix-like: Type[a b; c d] or Type[a\n b]
            // First element is already parsed, now parse rest as matrix
            self.with_end_symbol_depth(saved_end_symbol_depth, |parser| {
                parser.parse_typed_matrix_rest(start, object, first)
            })
        } else {
            // Could be matrix row: Type[a b c]
            self.with_end_symbol_depth(saved_end_symbol_depth, |parser| {
                parser.parse_typed_matrix_row_rest(start, object, first)
            })
        }
    }

    /// Parse rest of typed matrix: Type[first ...; ...]
    pub(crate) fn parse_typed_matrix_rest(
        &mut self,
        start: usize,
        type_node: CstNode,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let matrix_start = first.span.start;

        // Build first row from 'first' element
        let mut first_row_elements = vec![first];

        // Parse rest of first row (space-separated elements until ; or newline)
        // in the whitespace-sensitive matrix-row context (Issue #7196).
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        while !self.check(&Token::Semicolon)
            && !self.check(&Token::Newline)
            && !self.check(&Token::RBracket)
        {
            first_row_elements.push(self.parse_expression()?);
        }
        self.in_matrix_row = saved_in_matrix_row;

        let (first_row_start, first_row_end) = self.span_bounds(
            &first_row_elements,
            "typed matrix's first row always pushes the leading element above",
        )?;
        let first_row_span = self.source_map.span(first_row_start, first_row_end);
        let first_row =
            CstNode::with_children(NodeKind::MatrixRow, first_row_span, first_row_elements);

        let mut rows = vec![first_row];

        // Parse additional rows. Each separator run's semicolon count (its
        // dimension level) is preserved as `Semicolon` leaves interleaved
        // with the `MatrixRow` children, exactly like the untyped `[...]`
        // literal, so `Type[1 2; 3 4;;; 5 6; 7 8]` lowers to a genuine
        // `Array{Type,3}` instead of collapsing to a `Matrix` (Issue #10190).
        // Untyped matrix parsing already accepts trailing higher-dimensional
        // separators such as `[1; 2;;]`; typed literals need the same
        // syntax-level leniency for `T[1; 2;;]` (Issue #8759) — a trailing
        // run with no following row is simply dropped by the `RBracket`
        // check below.
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            let continuation = self.consume_row_separator_run(&mut rows)?;

            if self.check(&Token::RBracket) {
                break;
            }

            self.push_matrix_row(&mut rows, continuation)?;
        }

        let end_token = self.expect(Token::RBracket)?;
        let matrix_span = self.source_map.span(matrix_start, end_token.span.end);
        let matrix = CstNode::with_children(NodeKind::MatrixExpression, matrix_span, rows);

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TypedExpression,
            span,
            vec![type_node, matrix],
        ))
    }

    /// Parse rest of typed matrix row: Type[first elem2 elem3]
    pub(crate) fn parse_typed_matrix_row_rest(
        &mut self,
        start: usize,
        type_node: CstNode,
        first: CstNode,
    ) -> ParseResult<CstNode> {
        let matrix_start = first.span.start;
        let mut elements = vec![first];

        // Parse remaining space-separated elements in the row in the
        // whitespace-sensitive matrix-row context (Issue #7196).
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        while !self.check(&Token::RBracket)
            && !self.check(&Token::Semicolon)
            && !self.check(&Token::Newline)
        {
            elements.push(self.parse_expression()?);
        }
        self.in_matrix_row = saved_in_matrix_row;

        // If we hit semicolon or newline, there are more rows
        if self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            let (elements_start, elements_end) = self.span_bounds(
                &elements,
                "typed matrix row-rest always pushes the leading element above",
            )?;
            let first_row_span = self.source_map.span(elements_start, elements_end);
            let first_row = CstNode::with_children(NodeKind::MatrixRow, first_row_span, elements);

            let mut rows = vec![first_row];

            // See `parse_typed_matrix_rest` above: preserve each separator
            // run's semicolon count so N-dimensional typed literals lower
            // correctly (Issue #10190).
            while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
                let continuation = self.consume_row_separator_run(&mut rows)?;

                if self.check(&Token::RBracket) {
                    break;
                }

                self.push_matrix_row(&mut rows, continuation)?;
            }

            let end_token = self.expect(Token::RBracket)?;
            let matrix_span = self.source_map.span(matrix_start, end_token.span.end);
            let matrix = CstNode::with_children(NodeKind::MatrixExpression, matrix_span, rows);

            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::TypedExpression,
                span,
                vec![type_node, matrix],
            ))
        } else {
            // Single row matrix: Type[a b c]
            let end_token = self.expect(Token::RBracket)?;
            let (elements_start, elements_end) = self.span_bounds(
                &elements,
                "typed matrix row-rest always pushes the leading element above",
            )?;
            let row_span = self.source_map.span(elements_start, elements_end);
            let row = CstNode::with_children(NodeKind::MatrixRow, row_span, elements);
            let matrix_span = self.source_map.span(matrix_start, end_token.span.end);
            let matrix = CstNode::with_children(NodeKind::MatrixExpression, matrix_span, vec![row]);

            let span = self.source_map.span(start, end_token.span.end);
            Ok(CstNode::with_children(
                NodeKind::TypedExpression,
                span,
                vec![type_node, matrix],
            ))
        }
    }
}
