//! Primary expression parsers

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
                        let token = self.advance().unwrap();
                        Ok(CstNode::leaf(NodeKind::Identifier, token.span, token.text))
                    }
                    // Otherwise, parse as a begin...end block expression
                    _ => self.parse_begin_block(),
                }
            }

            // 'in' can be used as a function call: in(x, itr)
            Token::KwIn => {
                let token = self.advance().unwrap();
                Ok(CstNode::leaf(NodeKind::Identifier, token.span, token.text))
            }

            // 'end' keyword can be used in indexing expressions: a[end]
            // 'isa' can be used as a function call: isa(x, T)
            // ('outer' is lexed as a plain Identifier — see Token enum / Issue #8099)
            Token::KwEnd | Token::KwIsa => {
                let token = self.advance().unwrap();
                Ok(CstNode::leaf(NodeKind::Identifier, token.span, token.text))
            }

            // 'if' as expression: y = if cond a else b end
            Token::KwIf => self.parse_if_statement(),

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
                let is_op = token.token.is_operator();
                let span = token.span;
                let text = token.text.to_string();
                // token borrow ends here (NLL: last use of `token`)

                // Allow operator tokens as primary expressions when immediately followed by '('
                // This enables partial application syntax: ==(x), >(3), <=(5), etc. (Issue #3119)
                if is_op {
                    if let Some(Token::LParen) = self.peek_next() {
                        let op_token = self.advance().unwrap();
                        return Ok(CstNode::leaf(
                            NodeKind::Operator,
                            op_token.span,
                            op_token.text,
                        ));
                    }
                }
                Err(ParseError::unexpected_token(text, "expression", span))
            }
        }
    }

    /// Parse colon prefix: :symbol, :(expr), :keyword, or standalone :
    pub(crate) fn parse_colon_prefix(&mut self) -> ParseResult<CstNode> {
        let colon_token = self.advance().unwrap(); // consume :
        let start = colon_token.span.start;

        // Check what follows the colon
        match self.current.as_ref().map(|t| &t.token) {
            // :identifier - symbol literal
            Some(Token::Identifier) => {
                let ident = self.advance().unwrap();
                let span = self.source_map.span(start, ident.span.end);
                Ok(CstNode::leaf(
                    NodeKind::QuoteExpression,
                    span,
                    &self.source[start..ident.span.end],
                ))
            }

            // :(expr) - quote expression (including operators and statements)
            Some(Token::LParen) => {
                self.advance(); // consume (

                // Check if it's an operator, statement, or expression inside parens
                let mut inner = if let Some(token) = &self.current {
                    if token.token.is_operator() || token.token.is_assignment() {
                        // Check if this is an operator symbol like :(+) or a prefix expression like :(!true)
                        // If the next token after the operator is ), it's an operator symbol
                        // Otherwise, it's a prefix expression and we should parse it as an expression
                        let is_operator_symbol =
                            self.peek_next().is_none_or(|t| t == Token::RParen);
                        if is_operator_symbol {
                            // Operator as value: :(+), :(==), etc.
                            let op_token = self.advance().unwrap();
                            CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text)
                        } else {
                            // Prefix operator expression: :(!true), :(-x), etc.
                            self.parse_expression()?
                        }
                    } else if matches!(
                        token.token,
                        Token::KwIf
                            | Token::KwFor
                            | Token::KwWhile
                            | Token::KwTry
                            | Token::KwBegin
                            | Token::KwLet
                            | Token::KwFunction
                            | Token::KwMacro
                            | Token::KwStruct
                            | Token::KwMutable
                            | Token::KwAbstract
                            | Token::KwModule
                            | Token::KwBaremodule
                            | Token::KwReturn
                            | Token::KwBreak
                            | Token::KwContinue
                    ) {
                        // Statement inside quote: :(while true break end)
                        self.parse_top_level_item()?
                    } else {
                        // Regular expression
                        self.parse_expression()?
                    }
                } else {
                    return Err(ParseError::unexpected_eof(
                        "expression",
                        self.current_span(),
                    ));
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
                        block_children.push(self.parse_expression()?);
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

                let end_token = self.expect(Token::RParen)?;
                let span = self.source_map.span(start, end_token.span.end);
                Ok(CstNode::with_children(
                    NodeKind::QuoteExpression,
                    span,
                    vec![inner],
                ))
            }

            // :operator - quoted operator symbol (e.g., :+, :-, :*, etc.)
            Some(token) if token.is_operator() => {
                let op_token = self.advance().unwrap();
                let span = self.source_map.span(start, op_token.span.end);
                Ok(CstNode::leaf(
                    NodeKind::QuoteExpression,
                    span,
                    &self.source[start..op_token.span.end],
                ))
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
            Some(Token::Dot) | Some(Token::Ellipsis) | Some(Token::Dollar) => {
                let op_token = self.advance().unwrap();
                let span = self.source_map.span(start, op_token.span.end);
                Ok(CstNode::leaf(
                    NodeKind::QuoteExpression,
                    span,
                    &self.source[start..op_token.span.end],
                ))
            }

            // :keyword - keyword symbol (e.g., :if, :for, :quote, :end, etc.)
            Some(token) if token.keyword_as_symbol_text().is_some() => {
                let kw_token = self.advance().unwrap();
                let span = self.source_map.span(start, kw_token.span.end);
                Ok(CstNode::leaf(
                    NodeKind::QuoteExpression,
                    span,
                    &self.source[start..kw_token.span.end],
                ))
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
            _ => Ok(CstNode::leaf(
                NodeKind::Operator,
                colon_token.span,
                colon_token.text,
            )),
        }
    }
}
