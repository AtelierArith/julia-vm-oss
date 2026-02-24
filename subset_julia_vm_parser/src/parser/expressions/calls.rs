//! Call expression parsers

use crate::cst::CstNode;
use crate::error::ParseResult;
use crate::node_kind::NodeKind;
use crate::token::Token;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse a function call expression
    pub(crate) fn parse_call_expression(&mut self, callee: CstNode) -> ParseResult<CstNode> {
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
                let semi_token = self.advance().unwrap();
                arg_children.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span, ";"));
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

                // Check for operator as argument: f(+, a, b)
                // or anonymous typed parameter: f(::Type{T})
                let arg = if let Some(token) = &self.current {
                    if token.token == Token::DoubleColon {
                        // Anonymous typed parameter: ::Type{T} for short function definitions
                        // This is needed for patterns like: keytype(::Type{Dict{K,V}}) where {K,V} = K
                        let start = token.span.start;
                        self.advance(); // consume ::
                        let type_expr = self.parse_type_expression()?;
                        let end = type_expr.span.end;
                        let span = self.source_map.span(start, end);
                        CstNode::with_children(NodeKind::TypedParameter, span, vec![type_expr])
                    } else if token.token.is_operator() {
                        // Peek at next token to see if it's , or )
                        if let Some(next) = self.peek_next() {
                            if next == Token::Comma || next == Token::RParen {
                                // It's an operator as argument
                                let op_token = self.advance().unwrap();
                                CstNode::leaf(NodeKind::Operator, op_token.span, op_token.text)
                            } else {
                                // Not just an operator, parse as expression
                                self.parse_expression()?
                            }
                        } else {
                            self.parse_expression()?
                        }
                    } else if self.is_keyword_argument() {
                        self.parse_keyword_argument()?
                    } else if after_semicolon && self.is_keyword_argument_shorthand() {
                        // Keyword argument shorthand: f(;x) is equivalent to f(;x=x)
                        self.parse_keyword_argument_shorthand()?
                    } else {
                        self.parse_expression()?
                    }
                } else {
                    // Check for keyword argument (name = value)
                    if self.is_keyword_argument() {
                        self.parse_keyword_argument()?
                    } else if after_semicolon && self.is_keyword_argument_shorthand() {
                        // Keyword argument shorthand: f(;x) is equivalent to f(;x=x)
                        self.parse_keyword_argument_shorthand()?
                    } else {
                        self.parse_expression()?
                    }
                };

                // Check for generator inside call: sum(x for x in iter)
                if self.check(&Token::KwFor) {
                    let gen_start = lparen_token.span.start;
                    let generator = self.parse_generator_rest(gen_start, arg)?;
                    // Generator consumed the closing paren, so adjust span
                    let span = self.source_map.span(start, generator.span.end);
                    // Wrap generator in ArgumentList
                    let args_span = self.source_map.span(args_start, generator.span.end);
                    let arg_list =
                        CstNode::with_children(NodeKind::ArgumentList, args_span, vec![generator]);
                    return Ok(CstNode::with_children(
                        NodeKind::CallExpression,
                        span,
                        vec![callee, arg_list],
                    ));
                }

                arg_children.push(arg);

                // Check for comma or semicolon separator
                if self.check(&Token::Comma) {
                    self.advance(); // consume comma
                } else if self.check(&Token::Semicolon) {
                    // Add semicolon to children as a marker (for lowering to detect kwargs context)
                    let semi_token = self.advance().unwrap();
                    arg_children.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span, ";"));
                    // After semicolon, only keyword arguments are allowed
                    // Continue parsing - keyword arguments will be detected by is_keyword_argument
                    // Also enable shorthand syntax: f(a; x) where x becomes x=x
                    after_semicolon = true;
                } else {
                    break;
                }
            }
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

    /// Parse a do clause: do args; body end
    pub(crate) fn parse_do_clause(&mut self) -> ParseResult<CstNode> {
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

    /// Parse do clause parameters: x, y
    pub(crate) fn parse_do_params(&mut self) -> ParseResult<CstNode> {
        let first = self.parse_identifier()?;
        let start = first.span.start;
        let mut params = vec![first];

        while self.check(&Token::Comma) {
            self.advance();
            params.push(self.parse_identifier()?);
        }

        let end = params.last().unwrap().span.end;
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
        // Create a copy of the name node for the value (using the same identifier)
        // name.text is Option<String>, so we need to unwrap it
        let text = name.text.clone().unwrap_or_default();
        let value = CstNode::leaf(NodeKind::Identifier, span, text);
        Ok(CstNode::with_children(
            NodeKind::KeywordArgument,
            span,
            vec![name, value],
        ))
    }
}
