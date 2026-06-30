//! Expression parsing (Pratt parser)
//!
//! Handles:
//! - Binary and unary expressions with precedence climbing
//! - Postfix operations (call, index, field access)
//! - Ternary expressions
//! - Type declarations and parametric types

mod calls;
mod field;
mod index;
mod postfix;
mod primary;
mod types;

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::node_kind::NodeKind;
use crate::token::{Associativity, Precedence, Token};

use super::Parser;

impl<'a> Parser<'a> {
    // ==================== Expression Parsing (Pratt Parser) ====================

    /// Parse an expression (top-level entry point)
    pub(crate) fn parse_expression(&mut self) -> ParseResult<CstNode> {
        self.parse_expression_with_precedence(Precedence::MacroArg)
    }

    /// Parse an expression with minimum precedence (Pratt parser core)
    pub(crate) fn parse_expression_with_precedence(
        &mut self,
        min_prec: Precedence,
    ) -> ParseResult<CstNode> {
        // Parse prefix expression (unary or primary)
        let mut left = self.parse_prefix()?;

        // Parse infix and postfix expressions
        while !self.is_at_end() {
            // Check for postfix operations (call, index, field access)
            if let Some(postfix) = self.try_parse_postfix(&left)? {
                left = postfix;
                continue;
            }

            // Check for ternary operator
            if self.check(&Token::Question) && min_prec <= Precedence::Conditional {
                left = self.parse_ternary(left)?;
                continue;
            }

            // `->` (anonymous function) binds its parameter at the current operand
            // level, BEFORE the precedence gate below. `->` has the lowest binary
            // precedence (`Afunc`), so when it appears as the right operand of a
            // higher-precedence operator the gate would stop and leave the `->` for
            // the enclosing level — parsing `a |> x -> b` as `(a |> x) -> b` (the
            // lambda parameter becomes `a |> x`, and the body's `x` is unbound).
            // Forming the lambda here makes it `a |> (x -> b)`, matching Julia
            // (Issue #5673). The body is parsed right-associatively at `Afunc`.
            if self.check(&Token::Arrow) {
                self.advance();
                let body = self.parse_expression_with_precedence(Precedence::Afunc)?;
                let span = self.source_map.span(left.span.start, body.span.end);
                left = CstNode::with_children(
                    NodeKind::ArrowFunctionExpression,
                    span,
                    vec![left, body],
                );
                continue;
            }

            // Check for binary operator. Copy the operator's kind and span into
            // owned locals so the `self.current` borrow is released before any
            // `&mut self` lookahead (e.g. `peek_next_start` below).
            let (op_kind, op_span) = {
                let Some(token) = self.current.as_ref() else {
                    break;
                };
                (token.token.clone(), token.span)
            };

            let Some((prec, assoc)) = op_kind.binary_precedence() else {
                break;
            };

            // In a whitespace-separated matrix/`hcat` row, a `+`/`-` with a
            // space BEFORE it but NO space AFTER it begins a new
            // (unary-signed) element rather than acting as a binary operator:
            // `[1 -2]` is two elements, `[1 - 2]` is binary subtraction
            // (Issue #7196). Only `+`/`-` participate — they are the operators
            // with both a unary and a binary form; `[1 *2]` stays `1*2`.
            if self.in_matrix_row
                && matches!(op_kind, Token::Plus | Token::Minus)
                && op_span.start > left.span.end
                && self.peek_next_start() == Some(op_span.end)
            {
                break;
            }

            // Special case: Don't consume `:` as a range operator when it is the
            // separator of a ternary `cond ? then : else`. This only applies while
            // parsing the *then*-branch (`in_ternary_then`): at the top of the
            // then-branch (min_prec == Conditional) any `:` marks the end, and
            // deeper down — inside a higher-precedence operator's right operand,
            // e.g. the comparison in `cond ? a > b : c` — the separator is the
            // whitespace-preceded `:` (the ternary `:` is always space-delimited, a
            // range `1:2` is not), so genuine ranges like `cond ? (1 : 2) : c` still
            // parse (Issue #8314). The else-branch and other Conditional-level parses
            // are NOT gated, so a range in the else-branch (`cond ? a : b:c`) keeps
            // its `:` as a range operator (Issue #8318).
            if op_kind == Token::Colon
                && self.in_ternary_then
                && (min_prec == Precedence::Conditional || op_span.start > left.span.end)
            {
                break;
            }
            // In whitespace-separated matrix rows, `[:x :y]` means two Symbol
            // elements. Without this guard, the second token is consumed as a
            // range operator and parsed as `(:x):y`.
            if op_kind == Token::Colon
                && left.kind == NodeKind::QuoteExpression
                && op_span.start > left.span.end
            {
                break;
            }

            // Check precedence
            if prec < min_prec {
                break;
            }

            // Consume the operator
            let op_token = self.advance().unwrap();

            // Line continuation: skip newlines after a binary operator at end
            // of line (Issue #3660). Per Julia, an infix operator at the end
            // of a line continues the expression onto the next line — e.g.
            //     (x == 1) ||
            //     (x == 2)
            // Julia explicitly disallows this for `:` in range expressions
            // (`1:\n10` is rejected); the same restriction applies to `..`
            // and the `…` ellipsis range operator. All other binary operators
            // (including `||`, `&&`, `+`, `-`, `*`, `where`, `->`, `?`, `=>`,
            // and assignments) accept the continuation.
            if !matches!(
                op_token.token,
                Token::Colon | Token::DotDot | Token::HorizontalEllipsis
            ) {
                while self.check(&Token::Newline) {
                    self.advance();
                }
            }

            // Special case for 'where' with braced type params: expr where {T, S}
            if op_token.token == Token::KwWhere && self.check(&Token::LBrace) {
                let right = self.parse_braced_type_params()?;
                let span = self.source_map.span(left.span.start, right.span.end);
                left = CstNode::with_children(NodeKind::WhereExpression, span, vec![left, right]);
                continue;
            }

            // Calculate next precedence based on associativity
            let next_prec = match assoc {
                Associativity::Left => Precedence::try_from((prec as i8) + 1).unwrap_or(prec),
                Associativity::Right | Associativity::None => prec,
            };

            // Parse right-hand side. A non-braced value-position `where`
            // constraint may carry the `>:` lower-bound direction or the
            // double-bounded form `Lower<:T<:Upper`; parse it as a type
            // constraint so the bound direction is preserved (emitting
            // Subtype/SupertypeConstraint nodes), not flattened into a generic
            // binary expression that loses `>:` (Issue #5650).
            let right = if op_token.token == Token::KwWhere {
                self.parse_type_constraint()?
            } else {
                self.parse_expression_with_precedence(next_prec)?
            };

            // Create expression node based on operator
            let span = self.source_map.span(left.span.start, right.span.end);

            // Special case for 'where' - create WhereExpression
            if op_token.token == Token::KwWhere {
                left = CstNode::with_children(NodeKind::WhereExpression, span, vec![left, right]);
            } else if op_token.token == Token::Eq {
                // Simple assignment: lhs = rhs
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
                left =
                    CstNode::with_children(NodeKind::Assignment, span, vec![left, op_node, right]);
            } else if op_token.token.is_compound_assignment() {
                // Compound assignment: lhs += rhs, etc.
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
                left = CstNode::with_children(
                    NodeKind::CompoundAssignmentExpression,
                    span,
                    vec![left, op_node, right],
                );
            } else if op_token.token == Token::Colon {
                // Range expression: start:end or start:step:end
                left = CstNode::with_children(NodeKind::RangeExpression, span, vec![left, right]);
            } else if op_token.token == Token::Arrow {
                // Arrow function: x -> expr or (x, y) -> expr
                // left is parameter(s), right is body
                left = CstNode::with_children(
                    NodeKind::ArrowFunctionExpression,
                    span,
                    vec![left, right],
                );
            } else {
                let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
                left = CstNode::with_children(
                    NodeKind::BinaryExpression,
                    span,
                    vec![left, op_node, right],
                );
            }
        }

        Ok(left)
    }

    /// Parse a prefix expression (unary operator or primary)
    pub(crate) fn parse_prefix(&mut self) -> ParseResult<CstNode> {
        if let Some(node) = self.try_parse_dotted_unary_broadcast_prefix()? {
            return Ok(node);
        }

        let token = self
            .current
            .as_ref()
            .ok_or_else(|| ParseError::unexpected_eof("expression", self.current_span()))?;
        let token_kind = token.token.clone();

        // Check for dotted operators as expression start: .+([1,2,3]), .-(x, y)
        // This is the broadcast function call syntax where the operator is used as a function
        if token_kind.is_dotted_operator() {
            let op_token = self.advance().unwrap();

            // Check if followed by parenthesis - this means it's a broadcast function call
            if self.check(&Token::LParen) {
                let start = op_token.span.start;
                self.advance(); // consume '('

                let mut args = vec![];

                // Parse arguments
                if !self.check(&Token::RParen) {
                    loop {
                        while self.check(&Token::Newline) {
                            self.advance();
                        }
                        if self.check(&Token::RParen) {
                            break;
                        }
                        args.push(self.parse_expression()?);
                        if !self.check(&Token::Comma) {
                            break;
                        }
                        self.advance();
                    }
                }

                let end_token = self.expect(Token::RParen)?;
                let span = self.source_map.span(start, end_token.span.end);

                // Create the callee node as an Operator node (the base operator)
                let callee = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);

                // Insert callee at the front of args
                let mut all_children = vec![callee];
                all_children.extend(args);

                return Ok(CstNode::with_children(
                    NodeKind::BroadcastCallExpression,
                    span,
                    all_children,
                ));
            } else {
                // Dotted operator not followed by paren is an error
                return Err(ParseError::unexpected_token(
                    op_token.text,
                    "expression (dotted operators require parentheses when used as functions)",
                    op_token.span,
                ));
            }
        }

        // Operators at a statement/delimiter boundary are first-class function
        // values (`f = +; f(1, 2)`), not unary operators missing an operand.
        if token_kind.is_operator() && self.operator_value_is_at_boundary() {
            let op_token = self.advance().unwrap();
            return Ok(CstNode::leaf(
                NodeKind::Operator,
                op_token.span,
                op_token.text,
            ));
        }

        // Check for unary operators
        if let Some(_prec) = token_kind.unary_precedence() {
            let op_token = self.advance().unwrap();
            // Parse operand: unary binds tighter than binary, but postfix binds tightest
            // So -abs(x) should be -(abs(x)), not (-abs)(x)
            let operand = self.parse_prefix_with_postfix()?;
            // `^`/`.^` bind TIGHTER than the unary operator: `-x^2` == `-(x^2)`
            // (Issue #7232). Fold a trailing power expression into the operand
            // before the unary wraps it.
            let operand = self.absorb_power_into_unary_operand(operand)?;

            let span = self.source_map.span(op_token.span.start, operand.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
            return Ok(CstNode::with_children(
                NodeKind::UnaryExpression,
                span,
                vec![op_node, operand],
            ));
        }

        // Parse primary expression with postfix operations
        self.parse_primary_with_postfix()
    }

    /// Parse a primary expression followed by any postfix operations (call, index, field)
    fn parse_primary_with_postfix(&mut self) -> ParseResult<CstNode> {
        let mut left = self.parse_primary()?;

        // Apply postfix operations
        while !self.is_at_end() {
            if let Some(postfix) = self.try_parse_postfix(&left)? {
                left = postfix;
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse a prefix expression (possibly with nested unary ops) followed by postfix operations
    fn parse_prefix_with_postfix(&mut self) -> ParseResult<CstNode> {
        if let Some(node) = self.try_parse_dotted_unary_broadcast_prefix()? {
            return Ok(node);
        }

        let token = self
            .current
            .as_ref()
            .ok_or_else(|| ParseError::unexpected_eof("expression", self.current_span()))?;
        let token_kind = token.token.clone();

        // Check for unary operators (handles chained unary: --x, !!x)
        if token_kind.is_operator() && self.operator_value_is_at_boundary() {
            let op_token = self.advance().unwrap();
            return Ok(CstNode::leaf(
                NodeKind::Operator,
                op_token.span,
                op_token.text,
            ));
        }
        if let Some(_prec) = token_kind.unary_precedence() {
            let op_token = self.advance().unwrap();
            let operand = self.parse_prefix_with_postfix()?;
            // `^`/`.^` bind tighter than the unary operator (Issue #7232).
            let operand = self.absorb_power_into_unary_operand(operand)?;

            let span = self.source_map.span(op_token.span.start, operand.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
            return Ok(CstNode::with_children(
                NodeKind::UnaryExpression,
                span,
                vec![op_node, operand],
            ));
        }

        // Parse primary with postfix
        self.parse_primary_with_postfix()
    }

    /// `^`/`.^` (and the other Power-precedence operators `↑ ↓`) bind TIGHTER
    /// than a prefix unary operator. Julia parses `-x^2` as `-(x^2)` and `-2^3`
    /// as `-(2^3)` (julia/src/julia-parser.scm `parse-unary`/`parse-factor`:
    /// "-2^3 is parsed as -(2^3)"). The Pratt loop would otherwise wrap the
    /// unary first and apply `^` to `(-x)`, flipping the sign of the result.
    ///
    /// Given a just-parsed unary operand, if a Power-precedence operator
    /// follows, fold the full (right-associative) power expression into the
    /// operand so the unary wraps `x^2`, not just `x`. The RHS is parsed at
    /// `Power` precedence, which keeps a signed exponent (`x^-2`) and the
    /// right-associative chain (`x^2^3` == `x^(2^3)`) correct. The RHS of a
    /// power is a plain operand and is unaffected, so `2^-3` stays `2^(-3)`.
    fn absorb_power_into_unary_operand(&mut self, left: CstNode) -> ParseResult<CstNode> {
        let is_power = self
            .current
            .as_ref()
            .and_then(|t| t.token.binary_precedence())
            .map(|(prec, _)| prec == Precedence::Power)
            .unwrap_or(false);
        if !is_power {
            return Ok(left);
        }

        let op_token = self.advance().unwrap();
        let right = self.parse_expression_with_precedence(Precedence::Power)?;
        let span = self.source_map.span(left.span.start, right.span.end);
        let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
        Ok(CstNode::with_children(
            NodeKind::BinaryExpression,
            span,
            vec![left, op_node, right],
        ))
    }

    fn operator_value_is_at_boundary(&mut self) -> bool {
        matches!(
            self.peek_next(),
            None | Some(
                Token::Newline
                    | Token::Semicolon
                    | Token::Comma
                    | Token::RParen
                    | Token::RBracket
                    | Token::RBrace
                    | Token::KwEnd
                    | Token::KwElse
                    | Token::KwElseif
                    | Token::KwCatch
                    | Token::KwFinally
            )
        )
    }

    /// Parse a prefix dotted unary broadcast operator into a
    /// `BroadcastCallExpression`, mirroring upstream where `parse-unary` lowers
    /// a dotted unary like `.-x` to `broadcast(-, x)` (Issue #7234). Returns
    /// `Ok(None)` when the current position is not a prefix dotted unary so the
    /// caller falls through to its normal prefix handling.
    ///
    /// Two token shapes are recognized:
    /// - `.` followed by `!` or `~`: the two-token dotted unary operators
    ///   `.!x` / `.~x` (`.!` already supported, `.~` added here).
    /// - a single `.+` / `.-` token NOT followed by `(`: the unary-capable
    ///   broadcast operators in prefix position (`.+v`, `.-v`). When followed by
    ///   `(` it is the broadcast-function-call form (`.+(x, y)`) handled by the
    ///   caller, so we return `Ok(None)` in that case. Non-unary dotted
    ///   operators (`.*`, `./`, ...) are likewise left to the caller, matching
    ///   upstream which rejects e.g. `.*v` in prefix position.
    fn try_parse_dotted_unary_broadcast_prefix(&mut self) -> ParseResult<Option<CstNode>> {
        // `.!x` / `.~x`: a Dot token followed by the unary operator token.
        if self.check(&Token::Dot) {
            let base_op = match self.peek_next() {
                Some(Token::Not) => "!",
                Some(Token::Tilde) => "~",
                _ => return Ok(None),
            };
            return Ok(Some(self.parse_dotted_unary_broadcast_two_token(base_op)?));
        }

        // `.+v` / `.-v`: a single dotted-operator token whose base operator has
        // a prefix-unary form, not used as a broadcast function call (`.+(...)`).
        let is_unary_dotted = matches!(
            self.current.as_ref().map(|t| &t.token),
            Some(Token::DotPlus) | Some(Token::DotMinus)
        );
        if is_unary_dotted {
            if matches!(self.peek_next(), Some(Token::LParen)) {
                // `.+(...)`/`.-(...)` is a broadcast function call, not unary.
                return Ok(None);
            }
            let op_token = self.advance().unwrap();
            let operand = self.parse_prefix_with_postfix()?;
            // `^`/`.^` bind tighter than the broadcast unary (Issue #7232).
            let operand = self.absorb_power_into_unary_operand(operand)?;

            let span = self.source_map.span(op_token.span.start, operand.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
            return Ok(Some(CstNode::with_children(
                NodeKind::BroadcastCallExpression,
                span,
                vec![op_node, operand],
            )));
        }

        Ok(None)
    }

    /// Parse a two-token prefix dotted unary broadcast operator (`.!x`, `.~x`)
    /// into a `BroadcastCallExpression`. `base_op` is the underlying unary
    /// operator (`"!"` or `"~"`); the emitted operator node keeps the dotted
    /// spelling (`.!` / `.~`) so lowering can strip the dot.
    fn parse_dotted_unary_broadcast_two_token(&mut self, base_op: &str) -> ParseResult<CstNode> {
        let dot_token = self.expect(Token::Dot)?;
        let op_inner = self
            .advance()
            .ok_or_else(|| ParseError::unexpected_eof("expression", self.current_span()))?;
        let operand = self.parse_prefix_with_postfix()?;
        // `^`/`.^` bind tighter than the broadcast unary too (Issue #7232).
        let operand = self.absorb_power_into_unary_operand(operand)?;

        let op_span = self
            .source_map
            .span(dot_token.span.start, op_inner.span.end);
        let span = self.source_map.span(dot_token.span.start, operand.span.end);
        let op_node = CstNode::leaf(NodeKind::Operator, op_span, format!(".{base_op}"));

        Ok(CstNode::with_children(
            NodeKind::BroadcastCallExpression,
            span,
            vec![op_node, operand],
        ))
    }
}
