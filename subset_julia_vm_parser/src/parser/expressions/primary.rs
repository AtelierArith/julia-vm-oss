//! Primary expression parsers

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse a primary expression (literals, identifiers, parenthesized expressions)
    pub(crate) fn parse_primary(&mut self) -> ParseResult<CstNode> {
        let token = self
            .current
            .as_ref()
            .ok_or_else(|| ParseError::unexpected_eof("expression", self.current_span()))?;

        match &token.token {
            // Literals
            Token::DecimalLiteral
            | Token::BinaryLiteral
            | Token::OctalLiteral
            | Token::HexLiteral => self.parse_integer_literal(),

            Token::FloatLiteral
            | Token::FloatLeadingDot
            | Token::FloatExponent
            | Token::HexFloat => self.parse_float_literal(),

            Token::True | Token::False => self.parse_boolean_literal(),

            Token::CharLiteral => self.parse_character_literal(),

            // A prime token is valid only as a postfix adjoint. In primary
            // position it begins a character literal that failed the closed
            // CharLiteral lexer rule. Distinguish an actually closed but
            // invalid literal (`''`, `'ab'`) from an appendable EOF (`'a`) so
            // the REPL can request another line without treating ordinary
            // syntax errors as incomplete (Issues #10262/#10862).
            Token::Prime => {
                let span = token.span;
                let text = token.text.to_string();
                let suffix = &self.source[span.end..];
                let mut escaped = false;
                let has_closing_quote = suffix.chars().any(|ch| {
                    if escaped {
                        escaped = false;
                        false
                    } else if ch == '\\' {
                        escaped = true;
                        false
                    } else {
                        ch == '\''
                    }
                });
                if has_closing_quote {
                    Err(ParseError::unexpected_token(text, "expression", span))
                } else {
                    Err(ParseError::UnterminatedCharacter {
                        span: self.source_map.span(span.start, self.source.len()),
                    })
                }
            }

            Token::DoubleQuote | Token::TripleDoubleQuote => self.parse_string_literal(),

            Token::Backtick | Token::TripleBacktick => self.parse_command_literal(),

            Token::Identifier => self.parse_identifier_or_symbol(),

            // Parenthesized expression or tuple
            Token::LParen => self.parse_parenthesized_or_tuple(),

            // Array/Vector
            Token::LBracket => self.parse_array_or_comprehension(),

            // Macro call: @macro args
            Token::At => self.parse_macro_call(),

            // Colon: quote expression or range start
            Token::Colon => self.parse_colon_prefix(),

            // 'begin' can be either a block expression or an indexing keyword:
            //   z = begin ... end   → block expression (Issue #1794)
            //   a[begin:end]        → indexing identifier
            //   a[begin+1]          → indexing identifier (Issue #2310)
            // Disambiguate by peeking at the next token.
            Token::KwBegin => {
                let next = self.peek_next();
                match next {
                    // In indexing context, begin is followed by operators, delimiters, or end-of-input.
                    // A begin...end block would be followed by an expression start (identifier,
                    // literal, keyword, etc.), not by a binary operator or closing bracket.
                    Some(Token::Colon) | Some(Token::Comma)
                    | Some(Token::RBracket) | Some(Token::RParen)
                    // Arithmetic operators: a[begin+1], a[begin-1], a[begin*2], etc. (Issue #2310)
                    | Some(Token::Plus) | Some(Token::Minus)
                    | Some(Token::Star) | Some(Token::Slash) | Some(Token::SlashSlash)
                    | Some(Token::Percent) | Some(Token::Caret)
                    // Comparison operators: a[begin == end], etc.
                    | Some(Token::EqEq) | Some(Token::NotEq)
                    | Some(Token::Lt) | Some(Token::Gt)
                    | Some(Token::LtEq) | Some(Token::GtEq)
                    | None => {
                        let token = self.advance_checked(
                            "KwBegin token already matched by the outer match on parse_primary's `token.token` above",
                        )?;
                        Ok(CstNode::leaf(NodeKind::Identifier, token.span))
                    }
                    // Otherwise, parse as a begin...end block expression
                    _ => self.parse_begin_block(),
                }
            }

            // 'in' can be used as a function call: in(x, itr)
            Token::KwIn => {
                let token = self.advance_checked(
                    "KwIn token already matched by the outer match on parse_primary's `token.token` above",
                )?;
                Ok(CstNode::leaf(NodeKind::Identifier, token.span))
            }

            // Upstream dynamically treats `end` as an ordinary symbol only
            // while parsing the contents of a bracket ref expression. The
            // scope extends through nested calls/groupings (`a[f(end)]`) but
            // does not include arbitrary grouping contexts (`f(end)`).
            Token::KwEnd if self.end_symbol_depth > 0 => {
                let token = self.advance().ok_or_else(|| {
                    ParseError::unexpected_eof("`end` in a ref expression", self.current_span())
                })?;
                Ok(CstNode::leaf(NodeKind::Identifier, token.span))
            }

            Token::KwEnd => Err(ParseError::unexpected_token(
                token.text.to_string(),
                "expression",
                token.span,
            )),

            // 'isa' can be used as a function call: isa(x, T)
            // ('outer' is lexed as a plain Identifier — see Token enum / Issue #8099)
            Token::KwIsa => {
                let token = self.advance_checked(
                    "KwIsa token already matched by the outer match on parse_primary's `token.token` above",
                )?;
                Ok(CstNode::leaf(NodeKind::Identifier, token.span))
            }

            // 'if' as expression: y = if cond a else b end
            Token::KwIf => self.parse_if_statement(),

            // Loop forms are expressions in Julia and can appear as RHS bodies:
            // `f() = for x in xs ... end`, `c -> for x in xs ... end`.
            Token::KwFor => self.parse_for_statement(),
            Token::KwWhile => self.parse_while_statement(),

            // 'let' as expression: y = let a = 1; a + 1 end
            Token::KwLet => self.parse_let_expression(),

            // 'try' as expression: y = try ... catch ... end (Issue #4784).
            // Upstream Julia treats try/catch/finally as a first-class
            // expression whose value is the last expression in whichever
            // branch ran (try body if no exception, catch body if any).
            Token::KwTry => self.parse_try_statement(),

            // 'quote' as expression: esc(quote ... end)
            Token::KwQuote => self.parse_quote_expression(),

            // Anonymous function expression: f = function (x) x + 1 end
            Token::KwFunction => self.parse_function_definition(),

            // Jump expressions: these can appear as the right-hand side of && or ||
            // e.g., x > 0 && return nothing
            Token::KwReturn => self.parse_return_statement(),
            Token::KwBreak => self.parse_break_statement(),
            Token::KwContinue => self.parse_continue_statement(),

            // Declaration statements can appear in expression bodies, especially
            // generated callbacks such as `() -> global loaded = true`.
            Token::KwConst => self.parse_const_declaration(),
            Token::KwGlobal => self.parse_global_declaration(),
            Token::KwLocal => self.parse_local_declaration(),

            // Unary typed expression: ::Type or ::Type{T}
            // Used in callable struct definitions: (::MyType)(args) = body
            // and anonymous typed parameters: f(::Type{T}) = ...
            Token::DoubleColon => {
                let start = token.span.start;
                self.advance(); // consume ::
                let type_expr = self.parse_type_expression()?;
                let end = type_expr.span.end;
                let span = self.source_map.span(start, end);
                Ok(CstNode::with_children(
                    NodeKind::UnaryTypedExpression,
                    span,
                    vec![type_expr],
                ))
            }

            _ => {
                // Extract token data before any &mut self calls (borrow checker)
                let is_op = token.token.is_operator_identifier();
                let span = token.span;
                let text = token.text.to_string();
                // token borrow ends here (NLL: last use of `token`)

                // Allow operator tokens as primary expressions when immediately followed by '('
                // This enables partial application syntax: ==(x), >(3), <=(5), etc. (Issue #3119)
                if is_op {
                    if let Some(Token::LParen) = self.peek_next() {
                        let op_token = self.advance_checked(
                            "operator token already established as `self.current` at the top of parse_primary",
                        )?;
                        return Ok(CstNode::leaf(NodeKind::Operator, op_token.span));
                    }
                }
                Err(ParseError::unexpected_token(text, "expression", span))
            }
        }
    }

    /// Parse colon prefix: :symbol, :(expr), :keyword, or standalone :
    pub(crate) fn parse_colon_prefix(&mut self) -> ParseResult<CstNode> {
        // A quote disables the surrounding ref expression's special `end`
        // binding. The wrapper restores the dynamic state on every error path.
        self.with_end_symbol_depth(0, |parser| parser.parse_colon_prefix_inner())
    }

    fn parse_colon_prefix_inner(&mut self) -> ParseResult<CstNode> {
        let colon_token = self.advance_checked(
            "Colon token already matched by parse_primary's dispatch on Token::Colon",
        )?; // consume :
        let start = colon_token.span.start;

        // Check what follows the colon
        match self.current.as_ref().map(|t| &t.token) {
            // :identifier - symbol literal
            Some(Token::Identifier) => {
                if self.check_adjacent_prefixed_string("var") {
                    let prefix = self.parse_identifier()?;
                    let prefixed = self.parse_prefixed_string_literal(prefix)?;
                    let prefixed = self.merge_var_quoted_identifier(prefixed);
                    let span = self.source_map.span(start, prefixed.span.end);
                    return Ok(CstNode::with_children(
                        NodeKind::QuoteExpression,
                        span,
                        vec![prefixed],
                    ));
                }
                let ident = self.advance_checked(
                    "Identifier token already matched by the match arm on self.current above",
                )?;
                let mut end = ident.span.end;
                while self
                    .current
                    .as_ref()
                    .is_some_and(|next| next.token == Token::Identifier && next.span.start == end)
                {
                    let suffix = self.advance_checked(
                        "Identifier token already matched by the while condition above",
                    )?;
                    end = suffix.span.end;
                }
                let span = self.source_map.span(start, end);
                Ok(CstNode::leaf(NodeKind::QuoteExpression, span))
            }

            // :(expr) - quote expression (including operators and statements)
            Some(Token::LParen) => {
                self.advance(); // consume (

                // Inside :(...) newlines are insignificant — the quoted
                // expression may span multiple lines (Issue #8753). Increment
                // grouping_depth so binary-operator continuation and ternary ':'
                // continuation work inside the quoted expression.
                let saved_in_ternary_then = std::mem::replace(&mut self.in_ternary_then, false);
                self.grouping_depth += 1;

                // A newline immediately after the opening `:(` continues onto
                // the next line — skip it before parsing the inner expression.
                while self.check(&Token::Newline) {
                    self.advance();
                }

                // Check if it's an operator, statement, or expression inside parens.
                // Clone token data before any &mut self calls (borrow-checker).
                let mut inner = {
                    if self.check(&Token::RParen) {
                        // :() — empty-paren in a quote context (e.g. repr(:()) == ":(())")
                        let empty = self.current_span().start;
                        CstNode::new(
                            NodeKind::TupleExpression,
                            self.source_map.span(empty, empty),
                        )
                    } else if self.check(&Token::Semicolon) {
                        // :(;) — empty block expression (Expr(:block)) in quote
                        let semi_token = self
                            .advance_checked("Semicolon token already matched by check() above")?;
                        let semi = CstNode::leaf(NodeKind::Semicolon, semi_token.span);
                        CstNode::with_children(NodeKind::ParameterList, semi_token.span, vec![semi])
                    } else {
                        // Extract flags from current token BEFORE any &mut self calls (borrow-checker).
                        let (is_op_or_assign, is_dotted_op, token_text) =
                            if let Some(token) = &self.current {
                                (
                                    token.token.is_quoted_operator_symbol()
                                        || token.token.is_assignment(),
                                    token.token.is_operator() && token.text.starts_with('.'),
                                    token.text.to_string(),
                                )
                            } else {
                                return Err(ParseError::unexpected_eof(
                                    "expression",
                                    self.current_span(),
                                ));
                            };

                        // Julia accepts ∓/± as spaced prefix calls inside quoted expressions:
                        // `:(± 1)` / `:(∓ 1)` (Issue #8759). But `:(∓)` alone is a symbol.
                        // Peek to decide: if the next token is `)`, treat as a plain identifier
                        // symbol; otherwise parse as a prefix unary call.
                        let is_spaced_prefix_op = matches!(token_text.as_str(), "∓" | "±")
                            && !matches!(self.peek_next(), Some(Token::RParen) | None);
                        if is_spaced_prefix_op {
                            let op_token = self.advance_checked(
                                "spaced-prefix operator token already captured from self.current above",
                            )?;
                            let operand = self.parse_prefix_with_postfix()?;
                            let operand = self.absorb_power_into_unary_operand(operand)?;
                            let span = self.source_map.span(op_token.span.start, operand.span.end);
                            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span);
                            CstNode::with_children(
                                NodeKind::UnaryExpression,
                                span,
                                vec![op_node, operand],
                            )
                        } else if is_op_or_assign {
                            // Check if this is an operator symbol like :(+) or a prefix expression like :(!true)
                            // If the next token after the operator is ), it's an operator symbol
                            // Otherwise, it's a prefix expression and we should parse it as an expression
                            let next = self.peek_next();
                            let is_operator_symbol =
                                next.as_ref().is_none_or(|t| *t == Token::RParen);
                            // Issue #8759: Compound dotted-assignment symbols like :(.\=), :(.<<=),
                            // :(.÷=). The lexer doesn't produce a single token for these, so we see
                            // DotOp + Eq inside :(…). Detect this: dotted operator text starts with '.'
                            // and peek_next is Eq. We consume both tokens and produce a compound symbol.
                            let is_dotted_compound_assign =
                                !is_operator_symbol && is_dotted_op && next == Some(Token::Eq);
                            if is_operator_symbol {
                                // Operator as value: :(+), :(==), etc.
                                let op_token = self.advance_checked(
                                    "operator token already captured from self.current above",
                                )?;
                                CstNode::leaf(NodeKind::Operator, op_token.span)
                            } else if is_dotted_compound_assign {
                                // Compound dotted assignment operator symbol: :(.\=), :(.<<=), :(.÷=).
                                let op_tok = self.advance_checked(
                                    "dotted operator token already captured from self.current above",
                                )?; // consume the dotted operator
                                let eq_tok = self.advance_checked(
                                    "Eq token already confirmed by peek_next() == Some(Token::Eq) above",
                                )?; // consume =
                                let compound_end = eq_tok.span.end;
                                CstNode::leaf(
                                    NodeKind::Operator,
                                    self.source_map.span(op_tok.span.start, compound_end),
                                )
                            } else {
                                // Prefix operator expression: :(!true), :(-x), etc.
                                self.parse_expression()?
                            }
                        } else {
                            // Regular expression or statement inside quote:
                            // `:(while true break end)`, `:(const x = y)`, etc.
                            self.parse_group_item_or_expression()?
                        }
                    } // close else (non-empty paren)
                };

                if self.check(&Token::Comma) {
                    let tuple_start = inner.span.start;
                    let mut elements = vec![inner];
                    while self.check(&Token::Comma) {
                        self.advance();
                        while self.check(&Token::Newline) {
                            self.advance();
                        }
                        if self.check(&Token::RParen) {
                            break;
                        }
                        if self.check(&Token::Semicolon) {
                            let semi_token = self.advance_checked(
                                "Semicolon token already matched by check() above",
                            )?;
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
                    let tuple_end = elements
                        .last()
                        .map(|child| child.span.end)
                        .unwrap_or(tuple_start);
                    inner = CstNode::with_children(
                        NodeKind::TupleExpression,
                        self.source_map.span(tuple_start, tuple_end),
                        elements,
                    );
                }

                if self.check(&Token::KwFor) {
                    let generator_start = inner.span.start;
                    inner = self.parse_generator_rest_opts(generator_start, inner, false)?;
                }

                if self.check(&Token::Semicolon) {
                    let block_start = inner.span.start;
                    let mut block_children = vec![inner];
                    while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
                        self.advance();
                        while self.check(&Token::Semicolon) || self.check(&Token::Newline) {
                            self.advance();
                        }
                        if self.check(&Token::RParen) {
                            break;
                        }
                        block_children.push(self.parse_group_item_or_expression()?);
                    }
                    let block_end = block_children
                        .last()
                        .map(|child| child.span.end)
                        .unwrap_or(block_start);
                    inner = CstNode::with_children(
                        NodeKind::CompoundStatement,
                        self.source_map.span(block_start, block_end),
                        block_children,
                    );
                }

                // Skip newlines before `)` so multi-line quoted expressions
                // like `:(   \n   module M ... end\n)` parse correctly
                // (Issue #8753).
                while self.check(&Token::Newline) {
                    self.advance();
                }

                self.grouping_depth -= 1;
                self.in_ternary_then = saved_in_ternary_then;

                let end_token = self.expect(Token::RParen)?;
                let span = self.source_map.span(start, end_token.span.end);
                Ok(CstNode::with_children(
                    NodeKind::QuoteExpression,
                    span,
                    vec![inner],
                ))
            }

            // :operator - quoted operator symbol (e.g., :+, :-, :*, etc.)
            Some(token) if token.is_operator() || token.is_assignment() => {
                let op_token = self.advance_checked(
                    "operator/assignment token already matched by the match arm above",
                )?;
                let span = self.source_map.span(start, op_token.span.end);
                Ok(CstNode::leaf(NodeKind::QuoteExpression, span))
            }

            // Issue #4908: `:.` and `:...` — Symbols whose names are the
            // dot and ellipsis operators. These tokens are not classified
            // as operators by `Token::is_operator()` (they're treated as
            // syntactic markers for field access and splat in the rest of
            // the grammar), so they fall through to the standalone-colon
            // arm below and produce `ParseFailed`. Upstream Julia accepts
            // `:.` as `Symbol(".")` (the canonical Expr head for field
            // access) and `:...` as `Symbol("...")` (the splat head); add
            // explicit arms so the colon-prefix sugar mirrors the
            // user-visible `Symbol(name)` constructor.
            //
            // Issue #8759: `:?` — `?` is the ternary conditional marker; without
            // an explicit arm it falls through to standalone-colon and the `?` is
            // later parsed as a ternary opener, crashing the surrounding expression.
            // Upstream Julia treats `:?` as `Symbol("?")`.
            // Note: `DotDot` (`.`) is now covered by `is_operator()` so it no
            // longer needs an explicit arm here.
            Some(Token::Dot)
            | Some(Token::HorizontalEllipsis)
            | Some(Token::Ellipsis)
            | Some(Token::Dollar)
            | Some(Token::Question) => {
                let op_token = self.advance_checked(
                    "Dot/Ellipsis/Dollar/Question token already matched by the match arm above",
                )?;
                let span = self.source_map.span(start, op_token.span.end);
                Ok(CstNode::leaf(NodeKind::QuoteExpression, span))
            }

            // :keyword - keyword symbol (e.g., :if, :for, :quote, :end, etc.)
            Some(token) if token.keyword_as_symbol_text().is_some() => {
                let kw_token =
                    self.advance_checked("keyword token already matched by the match arm above")?;
                let span = self.source_map.span(start, kw_token.span.end);
                Ok(CstNode::leaf(NodeKind::QuoteExpression, span))
            }

            // Issue #4923: `:42`, `:3.14`, `:"hello"`, `:'x'` —
            // colon-prefix on a literal — must produce `QuoteNode(value)`
            // per upstream Julia. Unlike `:identifier` (which produces a
            // Symbol), `:literal` produces a literal value wrapped in
            // QuoteNode. The colon-prefix is tightly-bound here:
            // `:1 + 2` is `QuoteNode(1) + 2`, not `QuoteNode(1 + 2)`,
            // so we parse exactly the single literal via `parse_primary`
            // and wrap it as the child of a `QuoteExpression` with-
            // children node. The quote-lowering arm (`cst_to_constructor.rs`,
            // `NodeKind::QuoteExpression` with-children branch) then
            // recurses on the literal child and produces `QuoteNode(value)`
            // because the inner kind is an atom (IntegerLiteral / FloatLiteral
            // / CharacterLiteral / StringLiteral / BooleanLiteral).
            Some(Token::DecimalLiteral)
            | Some(Token::BinaryLiteral)
            | Some(Token::OctalLiteral)
            | Some(Token::HexLiteral)
            | Some(Token::FloatLiteral)
            | Some(Token::FloatLeadingDot)
            | Some(Token::FloatExponent)
            | Some(Token::HexFloat)
            | Some(Token::CharLiteral)
            | Some(Token::Backtick)
            | Some(Token::TripleBacktick)
            | Some(Token::DoubleQuote)
            | Some(Token::TripleDoubleQuote) => {
                let literal = self.parse_primary()?;
                let span = self.source_map.span(start, literal.span.end);
                Ok(CstNode::with_children(
                    NodeKind::QuoteExpression,
                    span,
                    vec![literal],
                ))
            }

            // Standalone colon (for range start like :end or 1:end)
            _ => Ok(CstNode::leaf(NodeKind::Operator, colon_token.span)),
        }
    }
}
