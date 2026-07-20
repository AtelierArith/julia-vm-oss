//! Collection parsing for Julia subset
//!
//! Handles parsing of tuples, arrays, comprehensions, and matrices.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

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
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        self.grouping_depth += 1;
        let result = self.parse_parenthesized_or_tuple_inner();
        self.grouping_depth -= 1;
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
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
            let end_token =
                self.advance_checked("RParen token already matched by check() above")?;
            let span = self.source_map.span(start, end_token.span.end);
            return Ok(CstNode::new(NodeKind::TupleExpression, span));
        }

        // Check for operator as value: (+), (-), (*), etc.
        // Look ahead: is it `(operator)`?
        if let Some(token) = &self.current {
            if token.token.is_operator_identifier() {
                // Peek at next token to see if it's )
                if let Some(next) = self.peek_next() {
                    if next == Token::RParen {
                        // It's an operator as value
                        let op_token = self.advance_checked(
                            "operator token already matched by token.token.is_operator() above",
                        )?;
                        let end_token = self.advance_checked(
                            "RParen token already confirmed by peek_next() == Token::RParen above",
                        )?;
                        let span = self.source_map.span(start, end_token.span.end);
                        let op_span = op_token.span;
                        let op_node = CstNode::leaf(NodeKind::Operator, op_span);
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

        // Parse first expression. Julia allows parenthesized block statements
        // such as `(for x in itr; f(x); end; nothing)`, `(while c; b; end)`,
        // and `(global x = y; x)` in expression positions.
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, true);
        let first = self.parse_group_item_or_expression();
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
        let first = first?;
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

                // Allow trailing comma, or trailing comma before semicolon.
                // Issue #8759: `(a=1, ; b=2)` — named tuple with positional args before `;`
                // and keyword args after. After the trailing comma we may see `;` (not `)`).
                if self.check(&Token::RParen) || self.check(&Token::Semicolon) {
                    break;
                }

                if self.check(&Token::Semicolon) {
                    let semi_token =
                        self.advance_checked("Semicolon token already matched by check() above")?;
                    elements.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span));
                    while self.check(&Token::Newline) {
                        self.advance();
                    }
                    if self.check(&Token::RParen) {
                        break;
                    }
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
            params.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span));
            while self.check(&Token::Newline) {
                self.advance();
            }
            if self.check(&Token::RParen) || self.check(&Token::Semicolon) {
                continue;
            }
            loop {
                params.push(self.parse_group_item_or_expression()?);
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
        //
        // ONLY nodes AFTER the `;` are keyword parameters. Issue #10354: this
        // rewrap used to run over the WHOLE parameter list, so an optional
        // POSITIONAL default before the `;` (`(y, x = 2; k = 3) -> (y, x, k)`,
        // whose `x = 2` is the same `Assignment[Identifier, =, value]` shape) was
        // also rewrapped into a `KwParameter` and lowered as a KEYWORD. `f(1, 5)`
        // then raised `NoMethodFound` instead of upstream's `(1, 5, 3)` — the
        // arity was wrong because the positional parameter had silently become a
        // keyword. Splitting at the `Semicolon` marker is what makes the pre-`;`
        // `Assignment` reach the arrow lowering's positional-default arm
        // (Issue #8047) and the post-`;` one reach its keyword arm.
        if self.check(&Token::Arrow) {
            let first_kwarg_index = params
                .iter()
                .position(|node| node.kind == NodeKind::Semicolon)
                .map(|semi| semi + 1)
                .unwrap_or(params.len());
            params = params
                .into_iter()
                .enumerate()
                .map(|(index, node)| {
                    if index < first_kwarg_index {
                        // Positional parameter (before the `;`) — an `Assignment`
                        // here is an optional positional DEFAULT, not a keyword.
                        return node;
                    }
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
                        // An ANNOTATED keyword (`k::Integer = 3`) is an `Assignment`
                        // with a `TypedExpression` LHS, not an `Identifier` — it stays
                        // as-is and is lowered by `signature::parse_kwparam_node`,
                        // which handles that shape (and carries its declared type).
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

        // Parse generator clauses. Julia accepts nested generator tails such as
        // `(x for x in y if p for z in w if q)`, so `for` and `if` clauses may
        // alternate rather than being restricted to all `for` clauses followed
        // by one final `if` (Issue #8759).
        while self.check(&Token::KwFor) || self.check(&Token::KwIf) {
            if self.check(&Token::KwFor) {
                children.push(self.parse_for_clause()?);
            } else {
                children.push(self.parse_if_clause()?);
            }
            self.skip_newlines();
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
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        self.grouping_depth += 1;
        let result = self.parse_array_or_comprehension_inner();
        self.grouping_depth -= 1;
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
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
            let end_token =
                self.advance_checked("RBracket token already matched by check() above")?;
            let span = self.source_map.span(start, end_token.span.end);
            return Ok(CstNode::new(NodeKind::VectorExpression, span));
        }

        if self.check(&Token::Semicolon) {
            return self.parse_empty_ncat_rest(start, Token::RBracket);
        }

        // Parse first element. A `[...]` literal may turn out to be a matrix
        // row (`[a b c]`, `[a b; c d]`), so the FIRST element is also parsed in
        // the whitespace-sensitive matrix-row context: in `[0.20 -0.26; ...]`
        // the `-0.26` is a second element, not `0.20 - 0.26` (Issue #7196).
        // This only affects a space-separated `+`/`-` with no trailing space;
        // `[1, -2]` (comma) and `[1 - 2]` (binary, space after) are unchanged.
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, true);
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, true);
        let first = self.parse_expression()?;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;

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
            // Only the element expression inherits an enclosing ref's special
            // `end` binding. Iterator/filter clauses reset it, while nested
            // refs in those clauses can establish a fresh binding (Issue #10918).
            self.with_end_symbol_depth(0, |parser| parser.parse_comprehension_rest(start, first))
        } else if self.check(&Token::Comma) {
            // Vector: [a, b, c]
            self.parse_vector_rest(start, first)
        } else if self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            // Matrix: [a b; c d] or [a b\n c d]
            self.parse_matrix_rest(start, first)
        } else if self.check(&Token::RBracket) {
            // Single element vector
            let end_token =
                self.advance_checked("RBracket token already matched by check() above")?;
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

            // Allow trailing comma, and Julia's `[1, 2;]` vector form.
            if self.check(&Token::RBracket) || self.check(&Token::Semicolon) {
                break;
            }

            elements.push(self.parse_expression()?);
        }

        while self.check(&Token::Semicolon) {
            self.advance();
            while self.check(&Token::Newline) {
                self.advance();
            }
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

    pub(crate) fn parse_empty_ncat_rest(
        &mut self,
        start: usize,
        end_token: Token,
    ) -> ParseResult<CstNode> {
        let mut children = Vec::new();
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            if self.check(&Token::Semicolon) {
                let semi =
                    self.advance_checked("Semicolon token already matched by check() above")?;
                children.push(CstNode::leaf(NodeKind::Semicolon, semi.span));
            } else {
                self.advance();
            }
        }
        let end = self.expect(end_token)?.span.end;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::VectorExpression,
            span,
            children,
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

        // Parse interleaved `for`/`if` clauses. Julia's flatten/filter
        // comprehension syntax allows any number of `for` and `if` clauses in
        // sequence, e.g. `[x for x in y if aa for z in w if bb]` (Issue #8759).
        loop {
            self.skip_newlines();
            if self.check(&Token::KwFor) {
                children.push(self.parse_for_clause()?);
            } else if self.check(&Token::KwIf) {
                children.push(self.parse_if_clause()?);
            } else {
                break;
            }
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

        self.skip_newlines();

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

        let end = self.last_span_end(
            &bindings,
            "for clause always pushes at least one binding above",
        )?;
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
        let var = if self.check(&Token::Dollar) {
            self.parse_prefix()?
        } else if self.check(&Token::LParen) {
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
        self.skip_newlines();

        let iter = self.parse_expression()?;
        let end = iter.span.end;

        let span = self.source_map.span(start, end);
        let mut children = vec![var, iter];

        // If outer, add a marker node at the beginning
        if has_outer {
            let outer_marker =
                CstNode::leaf(NodeKind::Identifier, self.source_map.span(start, start + 5));
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

        if self.check(&Token::Semicolon) {
            let semi_token =
                self.advance_checked("Semicolon token already matched by check() above")?;
            elements.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span));
        }

        // Parse first identifier
        if !self.check(&Token::RParen) {
            elements.push(self.parse_tuple_pattern_element()?);
        }

        // Parse remaining comma-separated identifiers
        while self.check(&Token::Comma) {
            self.advance(); // consume comma

            // Allow trailing comma
            if self.check(&Token::RParen) {
                break;
            }

            elements.push(self.parse_tuple_pattern_element()?);
        }

        let end_token = self.expect(Token::RParen)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::TupleExpression,
            span,
            elements,
        ))
    }

    fn parse_tuple_pattern_element(&mut self) -> ParseResult<CstNode> {
        if self.check(&Token::LParen) {
            return self.parse_tuple_pattern();
        }

        let ident = self.parse_identifier()?;
        let mut node = ident;
        if self.check(&Token::DoubleColon) {
            node = self.parse_type_declaration(node)?;
        }
        if self.check(&Token::Ellipsis) {
            let ellipsis =
                self.advance_checked("Ellipsis token already matched by check() above")?;
            let span = self.source_map.span(node.span.start, ellipsis.span.end);
            return Ok(CstNode::with_children(
                NodeKind::SplatParameter,
                span,
                vec![node],
            ));
        }
        Ok(node)
    }

    /// Consume one array-literal row separator (the gap between two matrix
    /// rows), emitting one [`NodeKind::Semicolon`] leaf per `;` token
    /// consumed to preserve Julia's `;`/`;;`/`;;;`/... dimension-separator
    /// level (`N` semicolons concatenate along dimension `N`; a run made up
    /// only of newlines is level 1, same as a single `;`) so the lowering
    /// pass can recover the literal's true N-dimensional shape (Issue
    /// #10190).
    ///
    /// Also enforces two of upstream Julia's parse-time restrictions on
    /// separator *placement* that sjulia previously accepted leniently
    /// (Issue #10398):
    ///
    ///   * A `;;` run (exactly two semicolons — upstream only special-cases
    ///     this exact count, not `;;;`/...) that follows a row which already
    ///     used bare space to join >= 2 elements ("row-major") is illegal
    ///     *unless* the run is immediately followed by a newline. That one
    ///     shape is Julia's "wrap a line" idiom (`[a b ;;\n c d]` continues
    ///     the same row/dimension, not a real separator); anything else —
    ///     `[a b;; c d]`, a `;;` split by a bare space (`[a b; ; c d]`, which
    ///     still counts as a 2-run), or a trailing `[a b;;]` — raises
    ///     upstream's own "cannot mix space and ;; separators..."
    ///     diagnostic.
    ///   * A `;`-run never resumes across a newline: `[a b;\n;c d]` stops
    ///     the run at the (insignificant, absorbed) newline rather than
    ///     treating the following `;` as a continuation, unlike the
    ///     legitimate `;;\n` line-wrap above. The dangling `;` is left for
    ///     the caller, which then fails to parse it as the start of the next
    ///     row's first expression — matching upstream's "Expected `]`"
    ///     rejection of a `;`-run split across a line break.
    ///   * More generally, a `;` may not be split from its neighbor(s) in
    ///     the same run by a bare space at all — `[a; ;b]` and `[a;;3 4;; ;
    ///     b]` are both rejected upstream ("whitespace is not allowed
    ///     here"), independent of the exactly-two-semicolons mixing rule
    ///     above (which only fires for a 2-run and only after a row-major
    ///     row — see the Form B doc comment on the mixing check for why
    ///     that one specific case surfaces the "cannot mix" message
    ///     instead).
    pub(crate) fn consume_row_separator_run(
        &mut self,
        children: &mut Vec<CstNode>,
    ) -> ParseResult<bool> {
        // Newlines *before* a semicolon run are never significant.
        while self.check(&Token::Newline) {
            self.advance();
        }

        if !self.check(&Token::Semicolon) {
            // Pure newline separator (or nothing left to consume): level 1,
            // no leaves — unchanged from before.
            return Ok(false);
        }

        let run_span = self.current_span();
        let mut n_semis = 0usize;
        let mut had_interior_space = false;
        loop {
            let semi = self.advance_checked(
                "Semicolon token already matched by check() above (loop entry) or the trailing check() before `continue` (loop repeat)",
            )?;
            let semi_end = semi.span.end;
            children.push(CstNode::leaf(NodeKind::Semicolon, semi.span));
            n_semis += 1;
            if self.check(&Token::Semicolon) {
                // Adjacent (or space-separated — Issue #10398 Form B) `;`:
                // still the same run.
                if self.current_span().start > semi_end {
                    had_interior_space = true;
                }
                continue;
            }
            break;
        }

        let immediately_followed_by_newline = self.check(&Token::Newline);
        if immediately_followed_by_newline {
            // Newlines *after* a semicolon run are never significant, but
            // (unlike the run itself) they never resume it: any further `;`
            // past this point starts the next row's content, not more of
            // this run (Issue #10398 Form C).
            while self.check(&Token::Newline) {
                self.advance();
            }
        }

        if n_semis == 2
            && !immediately_followed_by_newline
            && Self::array_literal_is_row_major(children)
        {
            return Err(ParseError::invalid_syntax(
                "cannot mix space and ;; separators in an array expression, except to wrap a line",
                run_span,
            ));
        }
        if had_interior_space {
            return Err(ParseError::invalid_syntax(
                "whitespace is not allowed between semicolons in an array expression",
                run_span,
            ));
        }

        // In a row-major literal, `;;` followed immediately by a newline is
        // Julia's line-wrap spelling: the next physical row continues the
        // current MatrixRow rather than introducing dimension 2 (Issue #10519).
        // Remove the just-recorded separators and let the caller append the
        // following physical row's elements to the preceding row.
        let continues_current_row = n_semis == 2
            && immediately_followed_by_newline
            && Self::array_literal_is_row_major(children);
        if continues_current_row {
            children.truncate(children.len() - n_semis);
        }

        Ok(continues_current_row)
    }

    /// Has any row parsed so far in this array literal joined >= 2 elements
    /// with a bare space (i.e. established Julia's "row-major"/`hcat`
    /// reading)? Used by [`Self::consume_row_separator_run`] to detect an
    /// illegal mix of space- and `;;`-separators (Issue #10398).
    fn array_literal_is_row_major(children: &[CstNode]) -> bool {
        children
            .iter()
            .any(|child| child.kind == NodeKind::MatrixRow && child.children.len() > 1)
    }

    /// Whether a `;;` (or higher) separator has already established Julia's
    /// column-major ncat reading for this literal (Issue #10518).
    fn array_literal_is_column_major(children: &[CstNode]) -> bool {
        let mut run = 0usize;
        for child in children {
            if child.kind == NodeKind::Semicolon {
                run += 1;
            } else {
                if run == 2 {
                    return true;
                }
                run = 0;
            }
        }
        run == 2
    }

    pub(crate) fn push_matrix_row(
        &mut self,
        rows: &mut Vec<CstNode>,
        continuation: bool,
    ) -> ParseResult<()> {
        let column_major = Self::array_literal_is_column_major(rows);
        let mut row = self.parse_matrix_row(column_major)?;
        if continuation {
            // `continuation` can only be `true` when `consume_row_separator_run`
            // returned it, which requires `array_literal_is_row_major(rows)` to
            // hold — and that in turn requires `rows` to already contain a
            // qualifying `MatrixRow`, so `rows` is guaranteed non-empty here.
            // Proof-backed by caller discipline rather than the type system
            // (Issue #10904); converted from a direct `expect` call to a
            // checked internal error instead of asserting `unreachable!()`.
            let previous = match rows.last_mut() {
                Some(previous) => previous,
                None => {
                    return Err(self
                        .internal_parser_error("row continuation requires a preceding matrix row"))
                }
            };
            previous.children.append(&mut row.children);
            previous.span = self.source_map.span(previous.span.start, row.span.end);
        } else {
            rows.push(row);
        }
        Ok(())
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

        let (first_row_start, first_row_end) = self.span_bounds(
            &first_row,
            "matrix's first row always pushes the leading element above",
        )?;
        let mut rows = vec![CstNode::with_children(
            NodeKind::MatrixRow,
            self.source_map.span(first_row_start, first_row_end),
            first_row,
        )];

        // Parse remaining rows. Each separator run's semicolon count (its
        // dimension level) is preserved as `Semicolon` leaves interleaved
        // with the `MatrixRow` children (Issue #10190); a trailing run with
        // no following row (`[1 2;;;]`) is dropped by the `RBracket` check
        // below, keeping the existing higher-dimension trailing leniency
        // (Issue #8759).
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            let continuation = self.consume_row_separator_run(&mut rows)?;

            if self.check(&Token::RBracket) {
                break;
            }

            self.push_matrix_row(&mut rows, continuation)?;
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
            let end_token =
                self.advance_checked("RBracket token already matched by check() above")?;
            let span = self.source_map.span(start, end_token.span.end);
            let (elements_start, elements_end) = self.span_bounds(
                &elements,
                "matrix row-rest always pushes the leading element above",
            )?;
            let row = CstNode::with_children(
                NodeKind::MatrixRow,
                self.source_map.span(elements_start, elements_end),
                elements,
            );
            return Ok(CstNode::with_children(
                NodeKind::MatrixExpression,
                span,
                vec![row],
            ));
        }

        // Create first row from all collected elements
        let (elements_start, elements_end) = self.span_bounds(
            &elements,
            "matrix row-rest always pushes the leading element above",
        )?;
        let first_row = CstNode::with_children(
            NodeKind::MatrixRow,
            self.source_map.span(elements_start, elements_end),
            elements,
        );

        let mut rows = vec![first_row];

        // Parse remaining rows. As in `parse_matrix_rest`, each separator
        // run's semicolon count is preserved as `Semicolon` leaves so
        // higher-dimensional literals (`;;`, `;;;`, ...) lower correctly
        // instead of collapsing to a 2-D matrix (Issue #10190). The
        // typed-matrix path already accepted trailing higher-dimensional
        // separators such as `T[1 2;;;]`; untyped row vectors need the same
        // syntax-level leniency for Julia hvncat forms like `[1 2;;;]`
        // (Issue #8759) — a trailing run with no following row is simply
        // dropped by the `RBracket` check below.
        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
            let continuation = self.consume_row_separator_run(&mut rows)?;

            if self.check(&Token::RBracket) {
                break;
            }

            self.push_matrix_row(&mut rows, continuation)?;
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
    pub(crate) fn parse_matrix_row(&mut self, column_major: bool) -> ParseResult<CstNode> {
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
            if column_major && elements.len() == 1 {
                self.in_matrix_row = saved_in_matrix_row;
                return Err(ParseError::invalid_syntax(
                    "cannot mix space and ;; separators in an array expression, except to wrap a line",
                    self.current_span(),
                ));
            }
            elements.push(self.parse_expression()?);
        }
        self.in_matrix_row = saved_in_matrix_row;

        let end = self.last_span_end(&elements, "matrix row always pushes `first` above")?;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::MatrixRow, span, elements))
    }
}
