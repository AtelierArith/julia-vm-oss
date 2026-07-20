//! Function and macro definition parsers

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
use crate::lexer::Lexer;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    // ==================== Function & Macro Definitions ====================

    /// Parse a function definition: function name(args) body end
    /// Also handles anonymous functions: function (args) body end
    /// Also handles callable struct definitions: function (::Type)(args) body end
    pub(crate) fn parse_function_definition(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwFunction)?;
        let start = start_token.span.start;

        // Check for anonymous function: function (x) ... end
        // Or callable struct definition: function (::Type)(args) ... end
        //                            or: function (self::Type)(args) ... end
        // If next token is '(' directly, check if it's a callable struct pattern
        let name = if self.check(&Token::LParen) {
            // Disambiguate the parenthesized head:
            //   `(::Type)`        anonymous callable struct  -> head is the name
            //   `(self::Type)(x)` bound callable struct      -> head is the name
            //   `(x)` / `(x, y)`  anonymous function         -> head is the parameter list
            //
            // The anonymous-self form is detected by a leading `::`. The bound
            // form `(self::Type)(args)` is detected by a `(` immediately
            // following the closing `)` of the parenthesized head — that second
            // paren group is the callable's argument list (Issue #5126).
            if self.peek_next() == Some(Token::DoubleColon)
                || self.paren_head_is_callable_object()
                || self.paren_head_is_operator_signature()
            {
                // Callable struct definition: function (::Type)(args) body end
                //                         or: function (self::Type)(args) body end
                // Parse the parenthesized head as the function "name"; the
                // following parameter list is parsed below.
                Some(self.parse_parenthesized_or_tuple()?)
            } else {
                None // Anonymous function
            }
        } else {
            // Parse function name (identifier or operator)
            Some(self.parse_function_name()?)
        };

        // Parse old-style type parameters: function foo{T}(x::T) ... end
        // This syntax is deprecated but still valid
        let old_type_params = if self.check(&Token::LBrace) {
            Some(self.parse_type_parameters()?)
        } else {
            None
        };

        // Parse parameters
        let params = if self.check(&Token::LParen) {
            Some(self.parse_parameter_list()?)
        } else {
            None
        };

        // Parse optional return type annotation: function foo(x)::Int
        let return_type = if self.check(&Token::DoubleColon) {
            self.advance(); // consume ::
                            // Parse type expression (identifier or parametric type)
            Some(self.parse_type_expression()?)
        } else {
            None
        };

        // Parse optional where clause
        let where_clause = if self.check_where_keyword() {
            Some(self.parse_where_clause()?)
        } else {
            None
        };

        // Parse body
        let body = self.parse_block_until_end()?;

        let end_token = self.expect(Token::KwEnd)?;
        let span = self.source_map.span(start, end_token.span.end);

        let mut children = Vec::new();
        if let Some(n) = name {
            children.push(n);
        }
        if let Some(tp) = old_type_params {
            children.push(tp);
        }
        if let Some(p) = params {
            children.push(p);
        }
        if let Some(rt) = return_type {
            children.push(rt);
        }
        if let Some(w) = where_clause {
            children.push(w);
        }
        children.push(body);

        Ok(CstNode::with_children(
            NodeKind::FunctionDefinition,
            span,
            children,
        ))
    }

    fn paren_head_is_operator_signature(&self) -> bool {
        let Some(tok) = self.current.as_ref() else {
            return false;
        };
        if tok.token != Token::LParen {
            return false;
        }

        // Span-agnostic constructor: only `spanned.token` is read below, so
        // skip the O(remaining source length) `SourceMap` scan (Issue #10128).
        let mut lexer = Lexer::new_for_token_peek(&self.source[tok.span.start..]);
        let mut depth: i32 = 0;
        while let Some(result) = lexer.next_token() {
            let Ok(spanned) = result else { continue };
            match spanned.token {
                Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
                Token::RParen | Token::RBracket | Token::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                token if depth == 1 && token.is_operator() => return true,
                _ => {}
            }
        }
        false
    }

    /// Decide whether the parenthesized head at the current `(` is the name of
    /// a callable-object method definition rather than an anonymous function's
    /// parameter list.
    ///
    /// A bound callable struct `function (self::Type)(args) ... end` is
    /// distinguished from an anonymous function `function (x) ... end` by what
    /// follows the closing `)` of the parenthesized head: a callable-object
    /// definition has a *second* parenthesized group (the argument list)
    /// immediately after, whereas an anonymous function has its body there.
    ///
    /// The current token is the opening `(`. We scan the source from that byte
    /// offset, tracking nesting of `()`, `[]`, and `{}` (so parametric type
    /// annotations like `Foo{T,S}` and tuple heads do not confuse the scan),
    /// and skipping string literals. When the head paren group closes, we check
    /// the next non-whitespace byte for `(` (Issue #5126).
    fn paren_head_is_callable_object(&self) -> bool {
        let Some(tok) = self.current.as_ref() else {
            return false;
        };
        let bytes = self.source.as_bytes();
        let mut i = tok.span.start;
        // Defensive: the current token must actually be the opening paren.
        if i >= bytes.len() || bytes[i] != b'(' {
            return false;
        }
        let mut depth: i32 = 0;
        let mut in_string = false;
        let mut string_delim = b'"';
        while i < bytes.len() {
            let c = bytes[i];
            if in_string {
                match c {
                    b'\\' => {
                        // Skip the escaped byte.
                        i += 2;
                        continue;
                    }
                    _ if c == string_delim => in_string = false,
                    _ => {}
                }
                i += 1;
                continue;
            }
            match c {
                b'"' | b'\'' => {
                    in_string = true;
                    string_delim = c;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        // Found the close of the head paren group. Look at the
                        // next non-whitespace byte.
                        let mut j = i + 1;
                        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                            j += 1;
                        }
                        return j < bytes.len() && bytes[j] == b'(';
                    }
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Parse function name (identifier, operator, or qualified name like Base.:-)
    pub(crate) fn parse_function_name(&mut self) -> ParseResult<CstNode> {
        self.reject_invalid_operator_identifier()?;

        let mut left = if self.check(&Token::Dollar) {
            let op_token = self.advance_checked("Dollar token already matched by check() above")?;
            let name = if self.check(&Token::Identifier) {
                self.parse_identifier()?
            } else if self.check(&Token::LParen) {
                // Parenthesized interpolated function name, e.g.
                // `function $(esc(:f))(x) ... end` (Issue #8066). Parse only the
                // `(...)` group as the `$` operand (a primary atom, no postfix),
                // so the following `(args)` is parsed as the parameter list. The
                // quote constructor interpolates this `$(...)` payload (handling
                // `esc`) when building the signature.
                self.parse_primary()?
            } else {
                return Err(ParseError::unexpected_token(
                    self.current
                        .as_ref()
                        .map(|t| t.text.to_string())
                        .unwrap_or_default(),
                    "interpolated function name",
                    self.current_span(),
                ));
            };
            let span = self.source_map.span(op_token.span.start, name.span.end);
            let op_node = CstNode::leaf(NodeKind::Operator, op_token.span);
            CstNode::with_children(NodeKind::UnaryExpression, span, vec![op_node, name])
        } else if self.check(&Token::At) {
            let at_token = self.advance_checked("At token already matched by check() above")?;
            let start = at_token.span.start;
            let ident = self.parse_identifier()?;
            let end = ident.span.end;
            // Text ("@name") is recovered from `span` (contiguous `@`+identifier
            // in source), so it no longer needs to be built here (Issue #10126).
            let span = self.source_map.span(start, end);
            CstNode::leaf(NodeKind::Identifier, span)
        } else if self.check(&Token::Identifier) {
            self.parse_identifier_like_name()?
        } else if matches!(
            self.current.as_ref().map(|t| &t.token),
            Some(Token::KwIn | Token::KwIsa)
        ) {
            let token = self.advance_checked(
                "KwIn/KwIsa token already matched by the matches!() guard above",
            )?;
            CstNode::leaf(NodeKind::Identifier, token.span)
        } else if self
            .current
            .as_ref()
            .map(|t| t.token.is_operator())
            .unwrap_or(false)
        {
            // Operator as function name
            let token =
                self.advance_checked("operator token already confirmed by is_operator() above")?;
            CstNode::leaf(NodeKind::Operator, token.span)
        } else {
            return Err(ParseError::unexpected_token(
                self.current
                    .as_ref()
                    .map(|t| t.text.to_string())
                    .unwrap_or_default(),
                "function name",
                self.current_span(),
            ));
        };

        // Handle qualified names: Base.foo or Base.:+
        while self.check(&Token::Dot) {
            left = self.parse_field_expression(left)?;
        }

        Ok(left)
    }

    /// Parse parameter list: (param1, param2, ...; kwarg1=val1, ...)
    /// Supports:
    /// - Positional parameters: (x, y)
    /// - Keyword parameters after semicolon: (x; y=1, z=2)
    /// - Mixed: (x, y; z=1)
    pub(crate) fn parse_parameter_list(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::LParen)?;
        let start = start_token.span.start;
        let mut params = Vec::new();

        if !self.check(&Token::RParen) {
            loop {
                while self.check(&Token::Newline) {
                    self.advance();
                }
                if self.check(&Token::RParen) {
                    break;
                }

                // Check for semicolon (keyword arguments separator)
                if self.check(&Token::Semicolon) {
                    // Add semicolon to params as a marker (for lowering to detect kwargs context)
                    let semi_token =
                        self.advance_checked("Semicolon token already matched by check() above")?;
                    params.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span));
                    // Parse keyword arguments after semicolon
                    while self.check(&Token::Newline) {
                        self.advance();
                    }
                    if self.check(&Token::RParen) {
                        break;
                    }
                    // Continue parsing as keyword parameters (use KwParameter kind)
                    params.push(self.parse_kw_parameter()?);
                    while self.check(&Token::Comma) {
                        self.advance();
                        while self.check(&Token::Newline) {
                            self.advance();
                        }
                        if self.check(&Token::RParen) {
                            break;
                        }
                        params.push(self.parse_kw_parameter()?);
                    }
                    break;
                }

                params.push(self.parse_parameter()?);

                if self.check(&Token::Semicolon) {
                    // Don't consume semicolon here, let the loop handle it
                    continue;
                }

                if !self.check(&Token::Comma) {
                    break;
                }
                self.advance();
            }
        }
        self.skip_newlines();

        let end_token = self.expect(Token::RParen)?;
        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::ParameterList,
            span,
            params,
        ))
    }

    /// Parse a single parameter: name, name::Type, name=default, ::Type, or name...
    /// Supports:
    /// - Simple: x
    /// - Typed: x::Int
    /// - Default value: x=1
    /// - Typed with default: x::Int=1
    /// - Varargs: args...
    /// - Typed varargs: args::T...
    /// - Anonymous typed: ::Type (e.g., ::Type{T} in promote_rule)
    pub(crate) fn parse_parameter(&mut self) -> ParseResult<CstNode> {
        // Compiler metadata annotations in parameter position:
        // `function f(@nospecialize x) ... end` and
        // `function f(@specialize(x)) ... end`, including qualified spellings
        // such as `Core.@nospecialize x`. These annotations affect
        // specialization metadata in upstream Julia; the current parser slice
        // accepts them by returning the wrapped parameter unchanged.
        if let Some(param) = self.parse_parameter_metadata_annotation(false)? {
            return Ok(param);
        }

        // Check for anonymous typed parameter: ::Type{T}
        if self.check(&Token::DoubleColon) {
            return self.parse_anonymous_typed_parameter();
        }

        if self.check(&Token::Dollar) {
            let expr = self.parse_prefix()?;
            let span = expr.span;
            return Ok(CstNode::with_children(
                NodeKind::Parameter,
                span,
                vec![expr],
            ));
        }

        // Check for tuple destructuring: (x, y), (x, y)::Type, or
        // (x, y)=default.
        if self.check(&Token::LParen) {
            let start = self.current.as_ref().map(|t| t.span.start).unwrap_or(0);
            let tuple = self.parse_parenthesized_or_tuple()?;
            let mut children = vec![tuple];

            // Optional type annotation for tuple
            if self.check(&Token::DoubleColon) {
                self.advance();
                children.push(self.parse_type_expression()?);
            }

            if self.check(&Token::Eq) {
                self.advance();
                self.skip_newlines();
                children.push(self.parse_expression()?);
            }

            let end = self.last_span_end(
                &children,
                "tuple-destructuring parameter always pushes `tuple` above",
            )?;
            let span = self.source_map.span(start, end);
            return Ok(CstNode::with_children(NodeKind::Parameter, span, children));
        }

        let name = self.parse_identifier_like_name()?;
        let start = name.span.start;
        let mut children = vec![name];
        let mut is_splat = false;

        // Optional type annotation
        if self.check(&Token::DoubleColon) {
            self.advance();
            children.push(self.parse_type_expression()?);
        }

        // Check for varargs: name... or name::Type...
        if self.check(&Token::Ellipsis) {
            self.advance();
            is_splat = true;
        }

        // Optional default value, including upstream's accepted slurp-default
        // syntax (`args...=default`) used in parser corpus invalid-syntax tests.
        if self.check(&Token::Eq) {
            self.advance();
            self.skip_newlines();
            children.push(self.parse_expression()?);
        }

        let end = self.last_span_end(&children, "parameter always pushes `name` above")?;
        let span = self.source_map.span(start, end);

        let kind = if is_splat {
            NodeKind::SplatParameter
        } else {
            NodeKind::Parameter
        };
        Ok(CstNode::with_children(kind, span, children))
    }

    /// Parse an anonymous typed parameter in signature position:
    /// `::Type`, `::Type{T}`, or `::Type{T}=Default`.
    pub(crate) fn parse_anonymous_typed_parameter(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::DoubleColon)?;
        let start = start_token.span.start;
        let type_expr = self.parse_type_expression()?;
        let mut children = vec![type_expr];
        let mut kind = NodeKind::TypedParameter;

        if self.check(&Token::Eq) {
            self.advance();
            self.skip_newlines();
            children.push(self.parse_expression()?);
            kind = NodeKind::Parameter;
        }

        let end = children.last().map(|child| child.span.end).unwrap_or(start);
        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(kind, span, children))
    }

    /// Parse a keyword parameter (after semicolon): name=default, name::Type=default, or kwargs...
    /// Uses KwParameter NodeKind to distinguish from positional parameters.
    /// Supports kwargs splat: kwargs... to collect all remaining keyword arguments
    pub(crate) fn parse_kw_parameter(&mut self) -> ParseResult<CstNode> {
        if let Some(param) = self.parse_parameter_metadata_annotation(true)? {
            return Ok(param);
        }

        if self.check(&Token::Dollar) {
            let expr = self.parse_prefix()?;
            let span = expr.span;
            return Ok(CstNode::with_children(
                NodeKind::KwParameter,
                span,
                vec![expr],
            ));
        }

        let name = self.parse_identifier_like_name()?;
        let start = name.span.start;
        let mut children = vec![name];
        let mut is_splat = false;

        // Optional type annotation
        if self.check(&Token::DoubleColon) {
            self.advance();
            children.push(self.parse_type_expression()?);
        }

        // Check for kwargs splat: kwargs...
        if self.check(&Token::Ellipsis) {
            self.advance();
            is_splat = true;
        }

        // Optional default value (keyword parameters usually have defaults, but not splat)
        if !is_splat && self.check(&Token::Eq) {
            self.advance();
            self.skip_newlines();
            children.push(self.parse_expression()?);
        }

        let end = self.last_span_end(&children, "keyword parameter always pushes `name` above")?;
        let span = self.source_map.span(start, end);

        // Use SplatParameter for kwargs... style, KwParameter for regular
        let kind = if is_splat {
            NodeKind::SplatParameter
        } else {
            NodeKind::KwParameter
        };
        Ok(CstNode::with_children(kind, span, children))
    }

    fn parse_parameter_metadata_annotation(
        &mut self,
        keyword: bool,
    ) -> ParseResult<Option<CstNode>> {
        if self.check(&Token::Identifier) && self.peek_next() == Some(Token::Dot) {
            self.advance();
            self.expect(Token::Dot)?;
            if !self.check(&Token::At) {
                return Err(ParseError::UnexpectedToken {
                    expected: "parameter metadata annotation".to_string(),
                    found: self
                        .current
                        .as_ref()
                        .map(|t| t.text.to_string())
                        .unwrap_or_else(|| "end of input".to_string()),
                    span: self.current_span(),
                });
            }
        } else if !self.check(&Token::At) {
            return Ok(None);
        }

        self.advance();
        let macro_name = self.parse_identifier()?;
        let macro_text = macro_name.text_from_source(self.source);
        if !matches!(macro_text, "nospecialize" | "specialize") {
            return Err(ParseError::UnexpectedToken {
                expected: if keyword {
                    "keyword parameter metadata annotation".to_string()
                } else {
                    "parameter metadata annotation".to_string()
                },
                found: format!("@{}", macro_text),
                span: macro_name.span,
            });
        }

        if self.check(&Token::LParen) {
            self.advance();
            let param = if keyword {
                self.parse_kw_parameter()?
            } else {
                self.parse_parameter()?
            };
            self.expect(Token::RParen)?;
            return Ok(Some(param));
        }

        if keyword {
            self.parse_kw_parameter().map(Some)
        } else {
            self.parse_parameter().map(Some)
        }
    }

    /// Parse where clause: where T <: SomeType or where {T, S}
    ///
    /// Unbraced constraints are parsed with `parse_type_constraint` (not the
    /// general expression parser): a general expression parse would swallow a
    /// following `= body` as an Assignment, so the assignment-form operator
    /// definition `*(a, b) where T<:Real = expr` failed with `expected Eq`
    /// (Issue #6537). `parse_type_constraint` stops cleanly before `=` / the
    /// body and preserves the `>:` bound direction and the double-bounded
    /// `Lower<:T<:Upper` shape (Issue #5650).
    ///
    /// Chained clauses (`where T where S`, possibly mixing braced lists) are
    /// folded into a single `WhereClause` node whose children are the
    /// constraints / `TypeParameters` lists in source order.
    pub(crate) fn parse_where_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect_contextual_keyword("where")?;
        let start = start_token.span.start;
        let mut children = Vec::new();
        // Always assigned on the first loop iteration (both branches set it).
        let mut end;

        loop {
            if self.check(&Token::LBrace) {
                // Braced type parameter list: where {T, S}
                let lbrace_token =
                    self.advance_checked("LBrace token already matched by check() above")?; // consume '{'
                let mut type_params = Vec::new();

                loop {
                    // Skip newlines
                    while self.check(&Token::Newline) {
                        self.advance();
                    }
                    if self.check(&Token::RBrace) {
                        break;
                    }

                    // Parse type parameter (could be T or T <: Bound)
                    let param = self.parse_expression()?;
                    type_params.push(param);

                    // Skip newlines after param
                    while self.check(&Token::Newline) {
                        self.advance();
                    }

                    if !self.check(&Token::Comma) {
                        break;
                    }
                    self.advance(); // consume comma
                }

                let end_token = self.expect(Token::RBrace)?;

                // Wrap the type parameters in a TypeParameters node (reusing existing kind)
                let params_span = self
                    .source_map
                    .span(lbrace_token.span.start, end_token.span.end);
                children.push(CstNode::with_children(
                    NodeKind::TypeParameters,
                    params_span,
                    type_params,
                ));
                end = end_token.span.end;
            } else {
                // Single constraint: where T or where T <: SomeType
                let constraint = self.parse_type_constraint_before_chained_where()?;
                end = constraint.span.end;
                children.push(constraint);
            }

            // Chained where clause: where T where S
            if self.check_where_keyword() {
                self.advance();
            } else {
                break;
            }
        }

        let span = self.source_map.span(start, end);
        Ok(CstNode::with_children(
            NodeKind::WhereClause,
            span,
            children,
        ))
    }

    /// Parse operator method definition: *(x, y) = expr or <(x, y) = expr
    /// This is a short function definition where the function name is an operator.
    pub(crate) fn parse_operator_method_definition(&mut self) -> ParseResult<CstNode> {
        self.reject_invalid_operator_identifier()?;

        // Callers (`Parser::parse_top_level_item`) only dispatch here after
        // confirming `self.current` is an operator token (and the reject
        // check above did not consume it), so this is guaranteed present.
        let op_token = self.advance_checked(
            "operator token already confirmed present by the caller before dispatching here",
        )?; // consume operator
        let start = op_token.span.start;

        // Parse parameter list
        let params = self.parse_parameter_list()?;

        // Optional return type annotation: *(x, y)::ReturnType = expr
        let return_type = if self.check(&Token::DoubleColon) {
            self.advance(); // consume ::
            Some(self.parse_type_expression()?)
        } else {
            None
        };

        // Optional where clause: *(x::T, y::T) where T = expr
        let where_clause = if self.check_where_keyword() {
            Some(self.parse_where_clause()?)
        } else {
            None
        };

        // Expect assignment operator
        self.expect(Token::Eq)?;

        // Skip newlines after =
        while self.check(&Token::Newline) {
            self.advance();
        }

        // Parse body expression
        let body = self.parse_expression()?;

        let span = self.source_map.span(start, body.span.end);

        // Build children: [name, params, (return_type), (where_clause), body]
        let mut children = Vec::new();
        children.push(CstNode::leaf(NodeKind::Operator, op_token.span));
        children.push(params);
        if let Some(rt) = return_type {
            children.push(rt);
        }
        if let Some(w) = where_clause {
            children.push(w);
        }
        children.push(body);

        Ok(CstNode::with_children(
            NodeKind::ShortFunctionDefinition,
            span,
            children,
        ))
    }

    /// Parse macro definition: macro name(args) body end
    pub(crate) fn parse_macro_definition(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwMacro)?;
        let start = start_token.span.start;

        let name = self.parse_function_name()?;

        let params = if self.check(&Token::LParen) {
            Some(self.parse_parameter_list()?)
        } else {
            None
        };

        while self.check(&Token::Newline) || self.check(&Token::Semicolon) {
            self.advance();
        }

        let body = self.parse_block_until_end()?;
        let end_token = self.expect(Token::KwEnd)?;

        let mut children = vec![name];
        if let Some(p) = params {
            children.push(p);
        }
        children.push(body);

        let span = self.source_map.span(start, end_token.span.end);
        Ok(CstNode::with_children(
            NodeKind::MacroDefinition,
            span,
            children,
        ))
    }
}
