//! Call expression parsers

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse a function call expression
    pub(crate) fn parse_call_expression(&mut self, callee: CstNode) -> ParseResult<CstNode> {
        // Inside the parentheses we leave macro-argument whitespace sensitivity
        // behind: interior expressions parse normally (Issue #5494). The flag is
        // restored before returning so a following space-before-paren in the same
        // macro argument is still treated as an argument separator. The
        // matrix-row context likewise does not extend into a call's argument
        // list, so `[f(1 - 2)]` keeps `-` binary (Issue #7196).
        let saved_space_sensitive = std::mem::replace(&mut self.macro_arg_space_sensitive, false);
        let saved_in_matrix_row = std::mem::replace(&mut self.in_matrix_row, false);
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, false);
        let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
        self.grouping_depth += 1;
        let result = self.parse_call_expression_inner(callee);
        self.grouping_depth -= 1;
        self.macro_arg_space_sensitive = saved_space_sensitive;
        self.in_matrix_row = saved_in_matrix_row;
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
        self.in_ternary_then = saved_in_ternary_then;
        result
    }

    fn parse_call_expression_inner(&mut self, callee: CstNode) -> ParseResult<CstNode> {
        let start = callee.span.start;
        let lparen_token = self.expect(Token::LParen)?;

        // Collect arguments separately, then wrap in ArgumentList
        let mut arg_children = Vec::new();
        let args_start = lparen_token.span.end;

        // Track if we're after a semicolon (keyword-only arguments section)
        let mut after_semicolon = false;

        // Check for empty call
        if !self.check(&Token::RParen) {
            // Check for semicolon at start: f(; x=1)
            if self.check(&Token::Semicolon) {
                // Add semicolon to children as a marker (for lowering to detect kwargs context)
                let semi_token =
                    self.advance_checked("Semicolon token already matched by check() above")?;
                arg_children.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span));
                after_semicolon = true;
            }

            // Parse arguments
            loop {
                // Skip newlines
                while self.check(&Token::Newline) {
                    self.advance();
                }

                if self.check(&Token::RParen) {
                    break;
                }

                if self.check(&Token::Semicolon) {
                    let semi_token =
                        self.advance_checked("Semicolon token already matched by check() above")?;
                    arg_children.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span));
                    after_semicolon = true;
                    continue;
                }

                // Check for operator as argument: f(+, a, b)
                // or anonymous typed parameter: f(::Type{T})
                let arg = if let Some(token) = &self.current {
                    if token.token == Token::DoubleColon {
                        // Anonymous typed parameter: ::Type{T} for short function definitions.
                        // This is needed for patterns like:
                        // keytype(::Type{Dict{K,V}}) where {K,V} = K.
                        self.parse_anonymous_typed_parameter()?
                    } else if token.token.is_operator_identifier() && token.token != Token::Dollar {
                        // Peek at next token to see if it's , or )
                        if let Some(next) = self.peek_next() {
                            if next == Token::Comma || next == Token::RParen {
                                // It's an operator as argument
                                let op_token = self.advance_checked(
                                    "operator token already confirmed by peek_next() == Comma/RParen above",
                                )?;
                                CstNode::leaf(NodeKind::Operator, op_token.span)
                            } else {
                                // Not just an operator, parse as expression
                                self.parse_call_argument_expression()?
                            }
                        } else {
                            self.parse_call_argument_expression()?
                        }
                    } else if self.is_keyword_argument() {
                        self.parse_keyword_argument()?
                    } else if after_semicolon && self.is_keyword_argument_shorthand() {
                        // Keyword argument shorthand: f(;x) is equivalent to f(;x=x)
                        self.parse_keyword_argument_shorthand()?
                    } else {
                        self.parse_call_argument_expression()?
                    }
                } else {
                    // Check for keyword argument (name = value)
                    if self.is_keyword_argument() {
                        self.parse_keyword_argument()?
                    } else if after_semicolon && self.is_keyword_argument_shorthand() {
                        // Keyword argument shorthand: f(;x) is equivalent to f(;x=x)
                        self.parse_keyword_argument_shorthand()?
                    } else {
                        self.parse_call_argument_expression()?
                    }
                };

                // Check for generator inside call: sum(x for x in iter).
                // The generator is a single positional argument; do NOT consume
                // the closing paren here so the shared separator handling below
                // can accept trailing `; kw=...` keyword arguments
                // (`f(x for x in it; kw=v)`, Issue #5763).
                if self.check(&Token::Newline)
                    && self.peek_non_newline_token() == Some(Token::KwFor)
                {
                    self.skip_newlines();
                }
                let arg = if self.check(&Token::KwFor) {
                    let gen_start = lparen_token.span.start;
                    self.parse_generator_rest_opts(gen_start, arg, false)?
                } else {
                    arg
                };

                arg_children.push(arg);

                // Check for comma or semicolon separator
                if self.check(&Token::Comma) {
                    self.advance(); // consume comma
                } else if self.check(&Token::Semicolon) {
                    // Add semicolon to children as a marker (for lowering to detect kwargs context)
                    let semi_token =
                        self.advance_checked("Semicolon token already matched by check() above")?;
                    arg_children.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span));
                    // After semicolon, only keyword arguments are allowed
                    // Continue parsing - keyword arguments will be detected by is_keyword_argument
                    // Also enable shorthand syntax: f(a; x) where x becomes x=x
                    after_semicolon = true;
                } else {
                    break;
                }
            }
        }

        // Skip newlines before the closing paren so the no-trailing-
        // comma multi-line call shape `f(\n a,\n b,\n c\n)` also
        // parses (Issue #4776). The trailing-comma form is already
        // handled by the in-loop newline skip above.
        while self.check(&Token::Newline) {
            self.advance();
        }

        let end_token = self.expect(Token::RParen)?;

        // Check for do clause: func(args) do x; ... end
        if self.check(&Token::KwDo) {
            let do_clause = self.parse_do_clause()?;
            let span = self.source_map.span(start, do_clause.span.end);
            // Create ArgumentList with do_clause
            let args_span = self.source_map.span(args_start, do_clause.span.end);
            let mut all_args = arg_children;
            all_args.push(do_clause);
            let arg_list = CstNode::with_children(NodeKind::ArgumentList, args_span, all_args);
            return Ok(CstNode::with_children(
                NodeKind::CallExpression,
                span,
                vec![callee, arg_list],
            ));
        }

        let span = self.source_map.span(start, end_token.span.end);
        // Create ArgumentList node wrapping all arguments (for tree-sitter compatibility)
        let args_span = self.source_map.span(args_start, end_token.span.start);
        let arg_list = CstNode::with_children(NodeKind::ArgumentList, args_span, arg_children);
        Ok(CstNode::with_children(
            NodeKind::CallExpression,
            span,
            vec![callee, arg_list],
        ))
    }

    fn parse_call_argument_expression(&mut self) -> ParseResult<CstNode> {
        let saved_macro_for_stop =
            std::mem::replace(&mut self.macro_arg_stops_before_comprehension_for, true);
        let result = self.parse_expression();
        self.macro_arg_stops_before_comprehension_for = saved_macro_for_stop;
        result
    }

    /// Parse a do clause: do args; body end
    pub(crate) fn parse_do_clause(&mut self) -> ParseResult<CstNode> {
        // A `do ... end` block body is a fresh statement block: newlines separate
        // its statements. But the do-clause is parsed *inside* the enclosing call's
        // argument parsing (`f(args) do ... end` — see `parse_call_expression_inner`),
        // where `grouping_depth` is still elevated from the call's `(`, which makes
        // interior newlines insignificant. Reset grouping for the duration of the
        // do-block so its body's newlines stay significant; otherwise a statement
        // ending in an expression followed by a line starting with `:` merges into a
        // range (e.g. `b = :Int` ⏎ `:($a::$b)` → `(b = :Int):(…)`), which broke
        // MacroTools' `combinestructdef` and thus `using MacroTools` (Issue #9176).
        let saved_grouping_depth = std::mem::replace(&mut self.grouping_depth, 0);
        let result = self.parse_do_clause_inner();
        self.grouping_depth = saved_grouping_depth;
        result
    }

    fn parse_do_clause_inner(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwDo)?;
        let start = start_token.span.start;

        let mut children = Vec::new();

        // Parse optional parameters: do x, y
        if !self.check(&Token::Newline)
            && !self.check(&Token::Semicolon)
            && !self.check(&Token::KwEnd)
        {
            let params = self.parse_do_params()?;
            children.push(params);
        }

        // Skip newline/semicolon before body
        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        // Parse body until 'end'
        let body = self.parse_block_until(&[Token::KwEnd])?;
        children.push(body);

        let end_token = self.expect(Token::KwEnd)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(NodeKind::DoClause, span, children))
    }

    fn parse_do_param(&mut self) -> ParseResult<CstNode> {
        if self.check(&Token::LParen) {
            self.parse_tuple_pattern()
        } else {
            let ident = self.parse_identifier()?;
            let mut param = if self.check(&Token::DoubleColon) {
                self.parse_type_declaration(ident)
            } else {
                Ok(ident)
            }?;
            if self.check(&Token::Ellipsis) {
                let ellipsis =
                    self.advance_checked("Ellipsis token already matched by check() above")?;
                let span = self.source_map.span(param.span.start, ellipsis.span.end);
                param = CstNode::with_children(NodeKind::SplatParameter, span, vec![param]);
            }
            Ok(param)
        }
    }

    /// Parse do clause parameters: x, y or (x, y), z
    pub(crate) fn parse_do_params(&mut self) -> ParseResult<CstNode> {
        let first = self.parse_do_param()?;
        let start = first.span.start;
        let mut params = vec![first];

        while self.check(&Token::Comma) {
            self.advance();
            params.push(self.parse_do_param()?);
        }

        let end = self.last_span_end(&params, "do-clause params always push `first` above")?;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::ParameterList,
            span,
            params,
        ))
    }

    /// Check if current position is a keyword argument (identifier followed by =)
    pub(crate) fn is_keyword_argument(&mut self) -> bool {
        if !self.check(&Token::Identifier) {
            return false;
        }
        // Check if next token is = (not == or ===)
        if let Some(next) = self.peek_next() {
            matches!(next, Token::Eq)
        } else {
            false
        }
    }

    /// Parse a keyword argument (name = value)
    pub(crate) fn parse_keyword_argument(&mut self) -> ParseResult<CstNode> {
        let name = self.parse_identifier()?;
        let start = name.span.start;
        self.expect(Token::Eq)?;
        self.skip_newlines();
        let value = self.parse_expression()?;
        let span = self.source_map.span(start, value.span.end);
        Ok(CstNode::with_children(
            NodeKind::KeywordArgument,
            span,
            vec![name, value],
        ))
    }

    /// Check if current position is a keyword argument shorthand (identifier after semicolon, not followed by =)
    /// This is for syntax like f(;x) which is equivalent to f(;x=x)
    pub(crate) fn is_keyword_argument_shorthand(&mut self) -> bool {
        if !self.check(&Token::Identifier) {
            return false;
        }
        // It's a shorthand if the identifier is NOT followed by =
        // (if followed by =, it will be handled by is_keyword_argument instead)
        if let Some(next) = self.peek_next() {
            // Check it's followed by comma, semicolon, or closing paren
            matches!(next, Token::Comma | Token::Semicolon | Token::RParen)
        } else {
            // End of input - could be a shorthand
            true
        }
    }

    /// Parse a keyword argument shorthand (name after semicolon, becomes name=name)
    /// f(;x) is equivalent to f(;x=x)
    pub(crate) fn parse_keyword_argument_shorthand(&mut self) -> ParseResult<CstNode> {
        let name = self.parse_identifier()?;
        let span = name.span;
        // Create a copy of the name node for the value (using the same
        // identifier text, recovered from source at `span` by both nodes —
        // Issue #10126).
        let value = CstNode::leaf(NodeKind::Identifier, span);
        Ok(CstNode::with_children(
            NodeKind::KeywordArgument,
            span,
            vec![name, value],
        ))
    }
}
