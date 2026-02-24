//! String parsing for the macro system.
//!
//! Handles Meta.parse and include_string: parse Julia source strings to AST Values.

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::value::{ExprValue, SymbolValue, Value};
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    // =========================================================================
    // Meta.parse implementation - convert string to AST Value
    // =========================================================================

    /// Parse a string and return the AST as a Value
    pub(super) fn parse_string_to_value(&self, source: &str) -> Result<Value, VmError> {
        use subset_julia_vm_parser::Parser as RustParser;

        let parser = RustParser::new(source);
        let (cst, errors) = parser.parse();

        if !errors.is_empty() {
            let error_msg = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(VmError::TypeError(format!("ParseError: {}", error_msg)));
        }

        // Get the first expression from source file
        if cst.children.is_empty() {
            return Ok(Value::Nothing);
        }

        // Convert the first child to Value
        self.cst_to_value(&cst.children[0], source)
    }

    /// Parse a string at a given position and return (expr, next_pos)
    pub(super) fn parse_string_at_to_value(
        &self,
        source: &str,
        start: usize,
    ) -> Result<(Value, usize), VmError> {
        // Julia's parse uses 1-based indexing, but we receive it as-is
        // For now, assume 1-based and convert to 0-based
        let start_0based = if start > 0 { start - 1 } else { 0 };

        if start_0based >= source.len() {
            // Past end of string - return (nothing, start)
            return Ok((Value::Nothing, start));
        }

        // Parse from the substring
        let substring = &source[start_0based..];
        use subset_julia_vm_parser::Parser as RustParser;

        let parser = RustParser::new(substring);
        let (cst, errors) = parser.parse();

        if !errors.is_empty() {
            let error_msg = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(VmError::TypeError(format!("ParseError: {}", error_msg)));
        }

        if cst.children.is_empty() {
            return Ok((Value::Nothing, start));
        }

        // Get the first expression
        let first_child = &cst.children[0];
        let value = self.cst_to_value(first_child, substring)?;

        // Calculate next position (1-based)
        let next_pos = start_0based + first_child.span.end + 1;

        Ok((value, next_pos))
    }

    /// Convert a CST node to a Value (for Meta.parse)
    fn cst_to_value(
        &self,
        node: &subset_julia_vm_parser::CstNode,
        source: &str,
    ) -> Result<Value, VmError> {
        use subset_julia_vm_parser::NodeKind;

        let text = &source[node.span.start..node.span.end];

        match node.kind {
            // Literals
            NodeKind::IntegerLiteral => {
                // Parse integer literal
                let clean = text.replace("_", "");
                if let Ok(n) = clean.parse::<i64>() {
                    Ok(Value::I64(n))
                } else {
                    // Try parsing as hex/oct/bin
                    if clean.starts_with("0x") || clean.starts_with("0X") {
                        i64::from_str_radix(&clean[2..], 16)
                            .map(Value::I64)
                            .map_err(|_| VmError::TypeError(format!("Invalid integer: {}", text)))
                    } else if clean.starts_with("0o") || clean.starts_with("0O") {
                        i64::from_str_radix(&clean[2..], 8)
                            .map(Value::I64)
                            .map_err(|_| VmError::TypeError(format!("Invalid integer: {}", text)))
                    } else if clean.starts_with("0b") || clean.starts_with("0B") {
                        i64::from_str_radix(&clean[2..], 2)
                            .map(Value::I64)
                            .map_err(|_| VmError::TypeError(format!("Invalid integer: {}", text)))
                    } else {
                        Err(VmError::TypeError(format!("Invalid integer: {}", text)))
                    }
                }
            }

            NodeKind::FloatLiteral => {
                let clean = text.replace("_", "");
                clean
                    .parse::<f64>()
                    .map(Value::F64)
                    .map_err(|_| VmError::TypeError(format!("Invalid float: {}", text)))
            }

            NodeKind::BooleanLiteral => Ok(Value::Bool(text == "true")),

            NodeKind::StringLiteral => {
                // Remove quotes and handle escape sequences
                let inner = if text.starts_with("\"\"\"") {
                    // Triple-quoted string
                    &text[3..text.len() - 3]
                } else if text.starts_with('"') {
                    &text[1..text.len() - 1]
                } else {
                    text
                };
                // Basic escape processing
                let unescaped = inner
                    .replace("\\n", "\n")
                    .replace("\\t", "\t")
                    .replace("\\r", "\r")
                    .replace("\\\\", "\\")
                    .replace("\\\"", "\"");
                Ok(Value::Str(unescaped))
            }

            NodeKind::CharacterLiteral => {
                // Remove quotes
                let inner = &text[1..text.len() - 1];
                if inner.starts_with('\\') {
                    // Escape sequence
                    let c = match inner.chars().nth(1) {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('\\') => '\\',
                        Some('\'') => '\'',
                        Some(c) => c,
                        None => return Err(VmError::TypeError("Invalid char literal".to_string())),
                    };
                    Ok(Value::Char(c))
                } else {
                    inner
                        .chars()
                        .next()
                        .map(Value::Char)
                        .ok_or_else(|| VmError::TypeError("Empty char literal".to_string()))
                }
            }

            NodeKind::Identifier => Ok(Value::Symbol(SymbolValue::new(text))),

            NodeKind::Operator => Ok(Value::Symbol(SymbolValue::new(text))),

            // Binary expression: a op b -> Expr(:call, op, a, b)
            NodeKind::BinaryExpression => {
                // Children: [left, op, right]
                if node.children.len() < 3 {
                    return Err(VmError::TypeError("Invalid binary expression".to_string()));
                }
                let left = self.cst_to_value(&node.children[0], source)?;
                let op_text = &source[node.children[1].span.start..node.children[1].span.end];
                let right = self.cst_to_value(&node.children[2], source)?;

                Ok(Value::Expr(ExprValue::from_head(
                    "call",
                    vec![Value::Symbol(SymbolValue::new(op_text)), left, right],
                )))
            }

            // Unary expression: op a -> Expr(:call, op, a)
            NodeKind::UnaryExpression => {
                if node.children.len() < 2 {
                    return Err(VmError::TypeError("Invalid unary expression".to_string()));
                }
                let op_text = &source[node.children[0].span.start..node.children[0].span.end];
                let arg = self.cst_to_value(&node.children[1], source)?;

                Ok(Value::Expr(ExprValue::from_head(
                    "call",
                    vec![Value::Symbol(SymbolValue::new(op_text)), arg],
                )))
            }

            // Call expression: f(a, b, ...) -> Expr(:call, f, a, b, ...)
            NodeKind::CallExpression => {
                let mut args = Vec::new();

                // First child is the function
                if let Some(func_node) = node.children.first() {
                    args.push(self.cst_to_value(func_node, source)?);
                }

                // Rest are arguments (may be in argument_list)
                for child in node.children.iter().skip(1) {
                    if child.kind == NodeKind::ArgumentList {
                        for arg in &child.children {
                            if arg.is_named {
                                args.push(self.cst_to_value(arg, source)?);
                            }
                        }
                    } else if child.is_named {
                        args.push(self.cst_to_value(child, source)?);
                    }
                }

                Ok(Value::Expr(ExprValue::from_head("call", args)))
            }

            // Tuple expression: (a, b, ...) -> Expr(:tuple, a, b, ...)
            NodeKind::TupleExpression => {
                let mut args = Vec::new();
                for child in &node.children {
                    if child.is_named {
                        args.push(self.cst_to_value(child, source)?);
                    }
                }
                Ok(Value::Expr(ExprValue::from_head("tuple", args)))
            }

            // Vector expression: [a, b, ...] -> Expr(:vect, a, b, ...)
            NodeKind::VectorExpression => {
                let mut args = Vec::new();
                for child in &node.children {
                    if child.is_named {
                        args.push(self.cst_to_value(child, source)?);
                    }
                }
                Ok(Value::Expr(ExprValue::from_head("vect", args)))
            }

            // Assignment: a = b -> Expr(:(=), a, b)
            NodeKind::Assignment => {
                if node.children.len() < 2 {
                    return Err(VmError::TypeError("Invalid assignment".to_string()));
                }
                let lhs = self.cst_to_value(&node.children[0], source)?;
                let last_child = node.children.last().ok_or_else(|| {
                    VmError::TypeError("Assignment has no right-hand side".to_string())
                })?;
                let rhs = self.cst_to_value(last_child, source)?;

                Ok(Value::Expr(ExprValue::from_head("=", vec![lhs, rhs])))
            }

            // Block: begin ... end -> Expr(:block, ...)
            NodeKind::Block | NodeKind::BeginBlock => {
                let mut args = Vec::new();
                for child in &node.children {
                    if child.is_named {
                        args.push(self.cst_to_value(child, source)?);
                    }
                }
                Ok(Value::Expr(ExprValue::from_head("block", args)))
            }

            // If statement: if cond ... end -> Expr(:if, cond, then_block, else_block?)
            NodeKind::IfStatement => {
                let mut args = Vec::new();
                for child in &node.children {
                    if child.is_named {
                        args.push(self.cst_to_value(child, source)?);
                    }
                }
                Ok(Value::Expr(ExprValue::from_head("if", args)))
            }

            // Function definition -> Expr(:function, signature, body)
            NodeKind::FunctionDefinition => {
                let mut args = Vec::new();
                for child in &node.children {
                    if child.is_named {
                        args.push(self.cst_to_value(child, source)?);
                    }
                }
                Ok(Value::Expr(ExprValue::from_head("function", args)))
            }

            // Short function: f(x) = expr -> Expr(:(=), call, expr)
            NodeKind::ShortFunctionDefinition => {
                if node.children.len() < 2 {
                    return Err(VmError::TypeError("Invalid short function".to_string()));
                }
                let sig = self.cst_to_value(&node.children[0], source)?;
                let last_child = node
                    .children
                    .last()
                    .ok_or_else(|| VmError::TypeError("Short function has no body".to_string()))?;
                let body = self.cst_to_value(last_child, source)?;
                Ok(Value::Expr(ExprValue::from_head("=", vec![sig, body])))
            }

            // Macro call: @macro args... -> Expr(:macrocall, macro_sym, linenumber, args...)
            NodeKind::MacrocallExpression => {
                let mut args = Vec::new();
                for child in &node.children {
                    if child.is_named {
                        let val = self.cst_to_value(child, source)?;
                        args.push(val);
                    }
                }
                Ok(Value::Expr(ExprValue::from_head("macrocall", args)))
            }

            // Quote: :(expr) -> Expr(:quote, expr)
            NodeKind::QuoteExpression => {
                if let Some(child) = node.children.iter().find(|c| c.is_named) {
                    let inner = self.cst_to_value(child, source)?;
                    Ok(Value::Expr(ExprValue::from_head("quote", vec![inner])))
                } else {
                    // :symbol -> just the Symbol (NOT QuoteNode)
                    // In Julia, Meta.parse(":x") returns Symbol, not QuoteNode
                    let sym_text = text.trim_start_matches(':');
                    Ok(Value::Symbol(SymbolValue::new(sym_text)))
                }
            }

            // Parenthesized expression - unwrap
            NodeKind::ParenthesizedExpression => {
                if let Some(child) = node.children.iter().find(|c| c.is_named) {
                    self.cst_to_value(child, source)
                } else {
                    Ok(Value::Nothing)
                }
            }

            // Source file - return first statement
            NodeKind::SourceFile => {
                if let Some(child) = node.children.iter().find(|c| c.is_named) {
                    self.cst_to_value(child, source)
                } else {
                    Ok(Value::Nothing)
                }
            }

            // Default: wrap as generic Expr with kind as head
            _ => {
                let head = format!("{:?}", node.kind).to_lowercase();
                let mut args = Vec::new();
                for child in &node.children {
                    if child.is_named {
                        args.push(self.cst_to_value(child, source)?);
                    }
                }
                if args.is_empty() {
                    // Leaf node - return as symbol
                    Ok(Value::Symbol(SymbolValue::new(text)))
                } else {
                    Ok(Value::Expr(ExprValue::from_head(head, args)))
                }
            }
        }
    }

    // =========================================================================
    // Meta.lower implementation - convert Value (AST) to lowered Core IR
    // =========================================================================

    // Lower an expression value to Core IR representation.
    //
    // This implements Meta.lower(m, x) which takes an expression and returns
    // the lowered form. In SubsetJuliaVM, we parse and lower the expression
    // through our lowering pipeline and return a representation of the IR.
    //
    // For simple values (literals, symbols), we return them as-is since they
    // don't need lowering. For Expr values, we convert them back to source code,
    // lower them, and return an IR representation.
    // =========================================================================
    // include_string implementation - parse and evaluate code string
    // =========================================================================

    /// Parse and evaluate all expressions in a code string.
    /// Returns the value of the last expression.
    pub(super) fn include_string_impl(&mut self, code: &str) -> Result<Value, VmError> {
        let mut result = Value::Nothing;
        let mut pos: usize = 1; // Julia uses 1-based indexing
        let code_length = code.len();

        while pos <= code_length {
            // Parse one expression starting at pos
            let (expr, next_pos) = self.parse_string_at_to_value(code, pos)?;

            // Check if we got nothing (end of string or whitespace-only)
            if matches!(expr, Value::Nothing) {
                break;
            }

            // Evaluate the expression
            result = self.eval_expr_value(&expr)?;

            // Check for progress to avoid infinite loop
            if next_pos <= pos {
                break;
            }

            pos = next_pos;
        }

        Ok(result)
    }
}
