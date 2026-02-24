//! Function and macro definition parsers

use crate::cst::CstNode;
use crate::error::{ParseError, ParseResult};
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
        // If next token is '(' directly, check if it's a callable struct pattern
        let name = if self.check(&Token::LParen) {
            // Disambiguate: (::Type) is a callable struct, anything else is anonymous function
            if self.peek_next() == Some(Token::DoubleColon) {
                // Callable struct definition: function (::Type)(args) body end
                // Parse (::Type) as a parenthesized unary typed expression
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
        let where_clause = if self.check(&Token::KwWhere) {
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

    /// Parse function name (identifier, operator, or qualified name like Base.:-)
    pub(crate) fn parse_function_name(&mut self) -> ParseResult<CstNode> {
        let mut left = if self.check(&Token::Identifier) {
            self.parse_identifier()?
        } else if self
            .current
            .as_ref()
            .map(|t| t.token.is_operator())
            .unwrap_or(false)
        {
            // Operator as function name
            let token = self.advance().unwrap();
            CstNode::leaf(NodeKind::Operator, token.span, token.text)
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
                    let semi_token = self.advance().unwrap();
                    params.push(CstNode::leaf(NodeKind::Semicolon, semi_token.span, ";"));
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
        // Check for anonymous typed parameter: ::Type{T}
        if self.check(&Token::DoubleColon) {
            let start = self.current.as_ref().map(|t| t.span.start).unwrap_or(0);
            self.advance();
            let type_expr = self.parse_type_expression()?;
            let end = type_expr.span.end;
            let span = self.source_map.span(start, end);
            return Ok(CstNode::with_children(
                NodeKind::TypedParameter,
                span,
                vec![type_expr],
            ));
        }

        // Check for tuple destructuring: (x, y) or (x, y)::Type
        if self.check(&Token::LParen) {
            let start = self.current.as_ref().map(|t| t.span.start).unwrap_or(0);
            let tuple = self.parse_parenthesized_or_tuple()?;
            let mut children = vec![tuple];

            // Optional type annotation for tuple
            if self.check(&Token::DoubleColon) {
                self.advance();
                children.push(self.parse_type_expression()?);
            }

            let end = children.last().unwrap().span.end;
            let span = self.source_map.span(start, end);
            return Ok(CstNode::with_children(NodeKind::Parameter, span, children));
        }

        let name = self.parse_identifier()?;
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

        // Optional default value (only if not varargs)
        if !is_splat && self.check(&Token::Eq) {
            self.advance();
            children.push(self.parse_expression()?);
        }

        let end = children.last().unwrap().span.end;
        let span = self.source_map.span(start, end);

        let kind = if is_splat {
            NodeKind::SplatParameter
        } else {
            NodeKind::Parameter
        };
        Ok(CstNode::with_children(kind, span, children))
    }

    /// Parse a keyword parameter (after semicolon): name=default, name::Type=default, or kwargs...
    /// Uses KwParameter NodeKind to distinguish from positional parameters.
    /// Supports kwargs splat: kwargs... to collect all remaining keyword arguments
    pub(crate) fn parse_kw_parameter(&mut self) -> ParseResult<CstNode> {
        let name = self.parse_identifier()?;
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
            children.push(self.parse_expression()?);
        }

        let end = children.last().unwrap().span.end;
        let span = self.source_map.span(start, end);

        // Use SplatParameter for kwargs... style, KwParameter for regular
        let kind = if is_splat {
            NodeKind::SplatParameter
        } else {
            NodeKind::KwParameter
        };
        Ok(CstNode::with_children(kind, span, children))
    }

    /// Parse where clause: where T <: SomeType or where {T, S}
    pub(crate) fn parse_where_clause(&mut self) -> ParseResult<CstNode> {
        let start_token = self.expect(Token::KwWhere)?;
        let start = start_token.span.start;

        // Check for braced type parameter list: where {T, S}
        if self.check(&Token::LBrace) {
            self.advance(); // consume '{'
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
            let span = self.source_map.span(start, end_token.span.end);

            // Wrap the type parameters in a TypeParameters node (reusing existing kind)
            let params_span = self
                .source_map
                .span(start_token.span.end, end_token.span.end);
            let params_node =
                CstNode::with_children(NodeKind::TypeParameters, params_span, type_params);
            return Ok(CstNode::with_children(
                NodeKind::WhereClause,
                span,
                vec![params_node],
            ));
        }

        // Single constraint: where T or where T <: SomeType
        let constraints = self.parse_expression()?;
        let span = self.source_map.span(start, constraints.span.end);
        Ok(CstNode::with_children(
            NodeKind::WhereClause,
            span,
            vec![constraints],
        ))
    }

    /// Parse operator method definition: *(x, y) = expr or <(x, y) = expr
    /// This is a short function definition where the function name is an operator.
    pub(crate) fn parse_operator_method_definition(&mut self) -> ParseResult<CstNode> {
        let op_token = self.advance().unwrap(); // consume operator
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
        let where_clause = if self.check(&Token::KwWhere) {
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
        children.push(CstNode::leaf(
            NodeKind::Operator,
            op_token.span,
            op_token.text,
        ));
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

        let name = self.parse_identifier()?;

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
