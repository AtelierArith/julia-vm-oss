//! Recursive descent parser for Julia subset
//!
//! Converts token stream from lexer into CST nodes.

mod collections;
mod expressions;
mod literals;
mod statements;

use crate::cst::CstNode;
use crate::error::{ParseError, ParseErrors, ParseResult};
use crate::lexer::{Lexer, SpannedToken};
use crate::node_kind::NodeKind;
use crate::span::{SourceMap, Span};
use crate::token::{Precedence, Token};

/// Julia parser
///
/// Parses Julia source code into a Concrete Syntax Tree (CST).
pub struct Parser<'a> {
    /// Source code
    pub(crate) source: &'a str,
    /// Lexer
    pub(crate) lexer: Lexer<'a>,
    /// Source map for line/column calculation
    pub(crate) source_map: SourceMap,
    /// Current token (peeked)
    pub(crate) current: Option<SpannedToken<'a>>,
    /// Collected errors (for error recovery)
    pub(crate) errors: ParseErrors,
    /// When set, a space before `(` or `[` does NOT fuse into a call/index
    /// at the top level of the current term. This mirrors upstream Julia's
    /// whitespace sensitivity for space-separated macro arguments: `@m foo (bar)`
    /// is two arguments (`foo` and `bar`), whereas `@m foo(bar)` is one call
    /// argument (Issue #5494). The flag is scoped to the macro argument's own
    /// postfix/operator chain and is cleared as soon as a grouping construct
    /// (`(...)`, `[...]`, `{...}`) is entered, so that sjulia's lenient
    /// space-before-paren call parsing inside groupings is preserved.
    pub(crate) macro_arg_space_sensitive: bool,
    /// When set, we are parsing the whitespace-separated elements of a
    /// matrix/`hcat` row (`[a b c]`, `[a b; c d]`). In this context a `+`/`-`
    /// with a space BEFORE it but NO space AFTER it begins a new
    /// (unary-negated/-signed) element instead of being a binary operator:
    /// `[1 -2]` is two elements, while `[1 - 2]` is binary subtraction
    /// (Issue #7196). Like `macro_arg_space_sensitive`, the flag is scoped to
    /// the row's top-level term and is cleared on entering any grouping
    /// construct (`(...)`, `[...]`, `{...}`, call/index argument lists), so an
    /// inner `-` such as in `[1 (2 - 3)]` or `[f(1 - 2)]` stays binary.
    pub(crate) in_matrix_row: bool,
    /// When set, we are parsing the then-branch of a ternary `cond ? then : else`.
    /// In this context a whitespace-preceded `:` terminates the then-branch (it is
    /// the ternary separator) instead of being consumed as a range operator, at
    /// any operator-recursion depth — without this, `cond ? a > b : c` parses the
    /// `:` as a range inside the comparison's right operand and the ternary then
    /// finds no separator (Issue #8314). Like `in_matrix_row`, the flag is cleared
    /// on entering any grouping construct (`(...)`, `[...]`, `{...}`, call/index
    /// argument lists), so a genuine range such as `cond ? (1 : 2) : c` still works.
    pub(crate) in_ternary_then: bool,
}

impl<'a> Parser<'a> {
    /// Create a new parser for the given source code
    pub fn new(source: &'a str) -> Self {
        let source_map = SourceMap::new(source);
        let lexer = Lexer::new(source);
        Self {
            source,
            lexer,
            source_map,
            current: None,
            errors: ParseErrors::new(),
            macro_arg_space_sensitive: false,
            in_matrix_row: false,
            in_ternary_then: false,
        }
    }

    /// Parse the source and return a SourceFile CST node
    pub fn parse(mut self) -> (CstNode, ParseErrors) {
        let start = 0;
        let mut children = Vec::new();

        // Prime the parser with first token
        self.advance();

        // Parse top-level items
        while !self.is_at_end() {
            // Skip newlines and semicolons between statements
            while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
                self.advance();
            }

            if self.is_at_end() {
                break;
            }

            match self.parse_top_level_item() {
                Ok(node) => children.push(node),
                Err(e) => {
                    self.errors.push(e);
                    // Error recovery: skip to next newline or end
                    self.synchronize();
                }
            }
        }

        let end = self.source.len();
        let span = self.source_map.span(start, end);
        let root = CstNode::with_children(NodeKind::SourceFile, span, children);

        (root, self.errors)
    }

    // ==================== Token Management ====================

    /// Advance to the next token
    pub(crate) fn advance(&mut self) -> Option<SpannedToken<'a>> {
        let prev = self.current.take();
        loop {
            match self.lexer.next_token() {
                Some(Ok(token)) => {
                    // Skip comments
                    if matches!(token.token, Token::LineComment) {
                        continue;
                    }
                    self.current = Some(token);
                    break;
                }
                Some(Err(e)) => {
                    self.errors.push(e);
                    continue;
                }
                None => {
                    self.current = None;
                    break;
                }
            }
        }
        prev
    }

    /// Skip any consecutive newline tokens at the current position.
    ///
    /// Used in contexts where newlines are insignificant (e.g. inside
    /// `[...]`/`(...)` brackets) to advance past line breaks before checking
    /// for the next meaningful token.
    pub(crate) fn skip_newlines(&mut self) {
        while self.check(&Token::Newline) {
            self.advance();
        }
    }

    /// Check if current token matches
    pub(crate) fn check(&self, expected: &Token) -> bool {
        self.current
            .as_ref()
            .map(|t| &t.token == expected)
            .unwrap_or(false)
    }

    /// Check if current token is any of the given tokens
    pub(crate) fn check_any(&self, expected: &[Token]) -> bool {
        self.current
            .as_ref()
            .map(|t| expected.contains(&t.token))
            .unwrap_or(false)
    }

    /// Check whether the current token is a plain identifier with the given text.
    ///
    /// Used for Julia's *contextual* keywords — `outer`, `type`, `as` — which are
    /// lexed as ordinary `Identifier`s (Issue #8099 / #8108) but are syntactically
    /// significant in one specific position each (`for outer x`, `abstract`/
    /// `primitive type`, import/using aliasing). Callers gate on this only in that
    /// position, leaving the words usable as ordinary identifiers everywhere else.
    pub(crate) fn check_contextual_keyword(&self, text: &str) -> bool {
        self.current
            .as_ref()
            .map(|t| t.token == Token::Identifier && t.text == text)
            .unwrap_or(false)
    }

    /// Consume a contextual keyword (an identifier with the given text), erroring
    /// if the current token is not that identifier. Mirrors [`Self::expect`] for
    /// the contextual-keyword case (see [`Self::check_contextual_keyword`]).
    pub(crate) fn expect_contextual_keyword(
        &mut self,
        text: &str,
    ) -> ParseResult<SpannedToken<'a>> {
        if self.check_contextual_keyword(text) {
            Ok(self.advance().unwrap())
        } else {
            let found = self
                .current
                .as_ref()
                .map(|t| t.text)
                .unwrap_or("end of input");
            let span = self.current_span();
            Err(ParseError::unexpected_token(found, text, span))
        }
    }

    /// Peek at the next token without consuming it
    pub(crate) fn peek_next(&mut self) -> Option<Token> {
        // Use lexer's peek to look ahead
        loop {
            match self.lexer.peek() {
                Some(Ok(token)) => {
                    // Skip comments
                    if matches!(token.token, Token::LineComment) {
                        let _ = self.lexer.next_token();
                        continue;
                    }
                    return Some(token.token.clone());
                }
                Some(Err(_)) => {
                    let _ = self.lexer.next_token();
                    continue;
                }
                None => return None,
            }
        }
    }

    /// Peek at the start byte offset of the next token without consuming it.
    /// Used for whitespace-sensitive disambiguation (e.g. matrix-row `-`/`+`,
    /// Issue #7196): a gap between the current token's end and this start means
    /// whitespace separates them.
    pub(crate) fn peek_next_start(&mut self) -> Option<usize> {
        loop {
            match self.lexer.peek() {
                Some(Ok(token)) => {
                    // Skip comments
                    if matches!(token.token, Token::LineComment) {
                        let _ = self.lexer.next_token();
                        continue;
                    }
                    return Some(token.span.start);
                }
                Some(Err(_)) => {
                    let _ = self.lexer.next_token();
                    continue;
                }
                None => return None,
            }
        }
    }

    /// Return the current token, or the next non-newline token if the current
    /// position is on one or more newlines, without consuming parser state.
    pub(crate) fn peek_non_newline_token(&self) -> Option<Token> {
        let start = self.current.as_ref()?.span.start;
        let mut lexer = Lexer::new(self.source);
        lexer.restart_from(start);
        loop {
            match lexer.next_token() {
                Some(Ok(token)) if matches!(token.token, Token::LineComment | Token::Newline) => {
                    continue;
                }
                Some(Ok(token)) => return Some(token.token),
                Some(Err(_)) => continue,
                None => return None,
            }
        }
    }

    /// Peek at the first non-newline token strictly *after* the current token,
    /// without consuming parser state. Used to look past a separator (e.g. the
    /// `,` between 2D comprehension bindings) and any insignificant newlines to
    /// the next meaningful token (Issue #8008).
    pub(crate) fn peek_non_newline_token_after_current(&self) -> Option<Token> {
        let after = self.current.as_ref()?.span.end;
        let mut lexer = Lexer::new(self.source);
        lexer.restart_from(after);
        loop {
            match lexer.next_token() {
                Some(Ok(token)) if matches!(token.token, Token::LineComment | Token::Newline) => {
                    continue;
                }
                Some(Ok(token)) => return Some(token.token),
                Some(Err(_)) => continue,
                None => return None,
            }
        }
    }

    /// Consume current token if it matches, return error otherwise
    pub(crate) fn expect(&mut self, expected: Token) -> ParseResult<SpannedToken<'a>> {
        if self.check(&expected) {
            Ok(self.advance().unwrap())
        } else {
            let found = self
                .current
                .as_ref()
                .map(|t| t.text)
                .unwrap_or("end of input");
            let span = self.current_span();
            Err(ParseError::unexpected_token(
                found,
                format!("{:?}", expected),
                span,
            ))
        }
    }

    /// Get the span of the current token
    pub(crate) fn current_span(&self) -> Span {
        self.current
            .as_ref()
            .map(|t| t.span)
            .unwrap_or_else(|| self.source_map.span(self.source.len(), self.source.len()))
    }

    /// Check if we're at end of input
    pub(crate) fn is_at_end(&self) -> bool {
        self.current.is_none()
    }

    /// Error recovery: skip tokens until we find a synchronization point
    pub(crate) fn synchronize(&mut self) {
        // Always advance at least once to avoid infinite loops
        self.advance();

        while !self.is_at_end() {
            // Stop at newline
            if self.check(&Token::Newline) {
                self.advance();
                return;
            }
            // Stop at keywords that start new statements
            if self.check_any(&[
                Token::KwFunction,
                Token::KwStruct,
                Token::KwModule,
                Token::KwIf,
                Token::KwFor,
                Token::KwWhile,
                Token::KwEnd,
                Token::KwLet,
                Token::KwTry,
                Token::KwBegin,
                Token::KwReturn,
                Token::KwConst,
                Token::KwAbstract,
            ]) {
                return;
            }
            self.advance();
        }
    }

    // ==================== Top-level Parsing ====================

    /// Parse a top-level item (statement, expression, or definition).
    ///
    /// This function is the central dispatch point for parsing Julia source code.
    /// It examines the current token and routes to the appropriate parsing function.
    ///
    /// ## Dispatch Decision Table
    ///
    /// See `docs/vm/PARSER.md` for the complete dispatch decision table.
    ///
    /// Key dispatch rules:
    /// - Keyword tokens (`function`, `struct`, `if`, etc.) dispatch to their specific parsers
    /// - `Identifier` followed by `,` dispatches to `parse_bare_tuple_assignment()`
    /// - Regular operators followed by `(` dispatch to `parse_operator_method_definition()`
    /// - Dotted operators (`.+`, `.-`, etc.) followed by `(` dispatch to `parse_expression()`
    ///   (these are broadcast calls, not operator method definitions - see Issue #1574)
    /// - Everything else dispatches to `parse_expression()`
    ///
    /// ## Related Tests
    ///
    /// See `subset_julia_vm_parser/tests/parser_dispatch_tests.rs` for invariant tests
    /// that verify this dispatch logic.
    pub(crate) fn parse_top_level_item(&mut self) -> ParseResult<CstNode> {
        // Clone the current token to avoid borrow issues with peek_next
        let token = self
            .current
            .as_ref()
            .ok_or_else(|| ParseError::unexpected_eof("statement", self.current_span()))?
            .token
            .clone();

        match &token {
            // Definitions (see docs/vm/PARSER.md: "Keyword Dispatch" section)
            Token::KwFunction => {
                let function_def = self.parse_function_definition()?;
                self.parse_pair_tail_after_definition(function_def)
            }
            Token::KwMacro => self.parse_macro_definition(),
            Token::KwStruct | Token::KwMutable => self.parse_struct_definition(),
            Token::KwAbstract => self.parse_abstract_definition(),
            Token::KwPrimitive => self.parse_primitive_definition(),
            Token::KwModule | Token::KwBaremodule => self.parse_module_definition(),

            // Control flow
            Token::KwIf => self.parse_if_statement(),
            Token::KwFor => self.parse_for_statement(),
            Token::KwWhile => self.parse_while_statement(),
            Token::KwTry => self.parse_try_statement(),
            Token::KwBegin => self.parse_begin_block(),
            Token::KwLet => self.parse_let_expression(),
            Token::KwQuote => self.parse_quote_expression(),

            // Jump statements
            Token::KwReturn => self.parse_return_statement(),
            Token::KwBreak => self.parse_break_statement(),
            Token::KwContinue => self.parse_continue_statement(),

            // Import/Export/Public
            Token::KwUsing => self.parse_using_statement(),
            Token::KwImport => self.parse_import_statement(),
            Token::KwExport => self.parse_export_statement(),
            Token::KwPublic => self.parse_public_statement(),

            // Variable declarations
            Token::KwConst => self.parse_const_declaration(),
            Token::KwGlobal => self.parse_global_declaration(),
            Token::KwLocal => self.parse_local_declaration(),

            // Identifier dispatch (see docs/vm/PARSER.md: "Bare Tuple Assignment" section)
            // For bare tuple assignment (a, b = expr), detect identifier followed by comma
            Token::Identifier => {
                if self.peek_next() == Some(Token::Comma) {
                    self.parse_bare_tuple_assignment()
                } else {
                    self.parse_expression()
                }
            }

            // Default dispatch (see docs/vm/PARSER.md: "Operator Dispatch" section)
            _ => {
                // Operator method definitions: *(x, y) = expr, <(x, y) = expr, etc.
                // IMPORTANT: Dotted operators like .+, .- are NOT operator method definitions.
                // They are broadcast function calls and must be parsed as expressions.
                // See Issue #1574 for context on why this distinction matters.
                if token.is_operator()
                    && !token.is_dotted_operator()
                    && !matches!(
                        token,
                        Token::ElementOf
                            | Token::NotElementOf
                            | Token::Contains
                            | Token::NotContains
                    )
                    && self.peek_next() == Some(Token::LParen)
                    && self.operator_call_is_definition()
                {
                    self.parse_operator_method_definition()
                } else {
                    // Issue #5337: a line-leading operator call with no trailing
                    // `=` (e.g. `+(t...)`, `*(2, 3, 4)`) is an ordinary expression
                    // statement, not an operator method definition. Let the Pratt
                    // parser handle it as a prefix operator-function call.
                    self.parse_expression()
                }
            }
        }
    }

    fn parse_pair_tail_after_definition(&mut self, left: CstNode) -> ParseResult<CstNode> {
        if !self.check(&Token::FatArrow) {
            return Ok(left);
        }

        let op_token = self.advance().unwrap();
        while self.check(&Token::Newline) {
            self.advance();
        }
        let right = self.parse_expression_with_precedence(Precedence::Pair)?;
        let span = self.source_map.span(left.span.start, right.span.end);
        let op_node = CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text);
        Ok(CstNode::with_children(
            NodeKind::BinaryExpression,
            span,
            vec![left, op_node, right],
        ))
    }

    /// Issue #5337: Decide whether a line-leading operator call (the current
    /// token is an operator followed by `(`) is an operator *method definition*
    /// (`op(params) = body`) rather than an ordinary expression statement
    /// (`op(args...)`). Only a genuine definition has a top-level `=` following
    /// the balanced parameter list; an expression statement is terminated by a
    /// newline/semicolon/EOF first.
    ///
    /// This scans a throwaway lexer over the remaining source (cost bounded by
    /// the current statement, and only runs for the rare operator-paren start),
    /// tracking bracket depth so that `=` inside the argument list (default
    /// args / keyword args) and inside `where {T}` / `::ReturnType{...}`
    /// annotations are not mistaken for the assignment.
    fn operator_call_is_definition(&self) -> bool {
        let Some(tok) = self.current.as_ref() else {
            return false;
        };
        let start = tok.span.start;
        let mut lexer = Lexer::new(&self.source[start..]);
        let mut depth: i32 = 0;
        // `closed` becomes true once the operator's own parameter list `)` is
        // matched; only then can a depth-0 `=` mean a definition.
        let mut closed = false;
        while let Some(result) = lexer.next_token() {
            let Ok(spanned) = result else { continue };
            match spanned.token {
                Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
                Token::RParen | Token::RBracket | Token::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        closed = true;
                    }
                }
                Token::Eq if closed && depth == 0 => return true,
                Token::Newline | Token::Semicolon if closed && depth == 0 => return false,
                _ => {}
            }
        }
        false
    }

    /// Parse a block of statements until we see 'end'
    pub(crate) fn parse_block_until_end(&mut self) -> ParseResult<CstNode> {
        self.parse_block_until(&[Token::KwEnd])
    }

    /// Parse a block of statements until we see one of the given tokens
    pub(crate) fn parse_block_until(&mut self, terminators: &[Token]) -> ParseResult<CstNode> {
        let start = self.current_span().start;
        let mut children = Vec::new();

        while !self.is_at_end() && !self.check_any(terminators) {
            // Skip newlines and semicolons (statement separators)
            while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
                self.advance();
            }

            if self.is_at_end() || self.check_any(terminators) {
                break;
            }

            match self.parse_top_level_item() {
                Ok(node) => children.push(node),
                Err(e) => {
                    self.errors.push(e);
                    self.synchronize();
                }
            }
        }

        let end = self.current_span().start;
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(NodeKind::Block, span, children))
    }
}

/// Parse Julia source code into a CST
pub fn parse(source: &str) -> (CstNode, ParseErrors) {
    Parser::new(source).parse()
}
