//! String parsing for the macro system.
//!
//! Handles Meta.parse and include_string: parse Julia source strings to AST Values.

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::value::{ExprValue, StructInstance, SymbolValue, Value};
use super::super::Vm;
use crate::lowering::expr::{parse_float, ParsedFloat};

impl<R: RngLike> Vm<R> {
    // =========================================================================
    // Meta.parse implementation - convert string to AST Value
    // =========================================================================

    /// Parse a string and return the AST as a Value
    fn immutable_struct_value(&self, name: &str, fields: Vec<Value>) -> Result<Value, VmError> {
        let type_id = self
            .struct_defs
            .iter()
            .position(|definition| definition.name == name)
            .ok_or_else(|| {
                VmError::InternalError(format!(
                    "Base did not register required parser detail type {name}"
                ))
            })?;
        Ok(Value::Struct(StructInstance::with_name(
            type_id,
            name.to_string(),
            fields,
        )))
    }

    fn parser_error_detail(
        &mut self,
        source: &str,
        byte_offset: usize,
        errors: &subset_julia_vm_parser::ParseErrors,
    ) -> Result<Value, VmError> {
        let byte_offset = i64::try_from(byte_offset).map_err(|_| {
            VmError::InternalError("parser detail byte offset exceeds Int64".to_string())
        })?;

        let mut line_starts = vec![Value::I64(1)];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                let next = i64::try_from(index + 2).map_err(|_| {
                    VmError::InternalError("parser line start exceeds Int64".to_string())
                })?;
                if !matches!(line_starts.last(), Some(Value::I64(last)) if *last == next) {
                    line_starts.push(Value::I64(next));
                }
            }
        }
        let sentinel = i64::try_from(source.len() + 1).map_err(|_| {
            VmError::InternalError("parser source length exceeds Int64".to_string())
        })?;
        if !matches!(line_starts.last(), Some(Value::I64(last)) if *last == sentinel) {
            line_starts.push(Value::I64(sentinel));
        }
        let line_count = line_starts.len();
        let line_starts = self.create_typed_array_from_values(line_starts, vec![line_count])?;

        let source_file = self.immutable_struct_value(
            "JuliaSyntax.SourceFile",
            vec![
                Value::str_new(source.to_string()),
                Value::I64(byte_offset),
                Value::str_new("none"),
                Value::I64(1),
                line_starts,
            ],
        )?;

        let mut diagnostics = Vec::with_capacity(errors.len());
        for error in errors {
            let span = error.span().ok_or_else(|| {
                VmError::InternalError("parser diagnostic has no source span".to_string())
            })?;
            // Parser spans are zero-based half-open byte extents. JuliaSyntax
            // diagnostics are one-based inclusive, so start advances by one
            // while end is already the inclusive last byte. This also preserves
            // EOF diagnostics where first > last (`"function"` => 9:8).
            // Extra-token spans begin at the completed expression's byte end,
            // including intervening whitespace (`"é )"` => 3:4, Issue #11634).
            let local_first = span.start.checked_add(1).ok_or_else(|| {
                VmError::InternalError("parser diagnostic start overflows usize".to_string())
            })?;
            let local_last = span.end;
            let first_byte = byte_offset
                .checked_add(i64::try_from(local_first).map_err(|_| {
                    VmError::InternalError("parser diagnostic start exceeds Int64".to_string())
                })?)
                .ok_or_else(|| {
                    VmError::InternalError("parser diagnostic start overflows Int64".to_string())
                })?;
            let last_byte = byte_offset
                .checked_add(i64::try_from(local_last).map_err(|_| {
                    VmError::InternalError("parser diagnostic end exceeds Int64".to_string())
                })?)
                .ok_or_else(|| {
                    VmError::InternalError("parser diagnostic end overflows Int64".to_string())
                })?;
            diagnostics.push(self.immutable_struct_value(
                "JuliaSyntax.Diagnostic",
                vec![
                    Value::I64(first_byte),
                    Value::I64(last_byte),
                    Value::Symbol(SymbolValue::new("error")),
                    Value::str_new(error.diagnostic_message()),
                ],
            )?);
        }
        let diagnostic_count = diagnostics.len();
        let diagnostics =
            self.create_typed_array_from_values(diagnostics, vec![diagnostic_count])?;

        self.immutable_struct_value(
            "JuliaSyntax.ParseError",
            vec![
                source_file,
                diagnostics,
                Value::Symbol(SymbolValue::new(errors.incomplete_tag())),
            ],
        )
    }

    pub(super) fn parse_string_to_value(&mut self, source: &str) -> Result<Value, VmError> {
        use subset_julia_vm_parser::Parser as RustParser;

        let (node, errors, consumed) = RustParser::new(source).parse_one();

        if !errors.is_empty() {
            let segment = &source[..consumed.min(source.len())];
            let error_msg = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            // Upstream raises `Base.Meta.ParseError`; sjulia's Base defines the
            // matching `ParseError` struct. This used to raise the `TypeError`
            // variant with a `"ParseError: "` text prefix (Issue #11146).
            let detail = self.parser_error_detail(segment, 0, &errors)?;
            if errors.is_incomplete_input() {
                let error = self.immutable_struct_value(
                    "ParseError",
                    vec![Value::str_new(error_msg), detail],
                )?;
                return Ok(Self::expr_value("incomplete", vec![error]));
            }
            return Err(self.parse_error_with_detail(error_msg, detail));
        }

        if consumed < source.len() {
            return Err(VmError::ParseError(
                "extra token after end of expression".to_string(),
            ));
        }

        match node {
            Some(node) => self.cst_to_value(&node, source),
            None => Ok(Value::Nothing),
        }
    }

    /// Parse a string at a given position and return (expr, next_pos)
    pub(super) fn parse_string_at_to_value(
        &mut self,
        source: &str,
        start: i64,
    ) -> Result<(Value, i64), VmError> {
        let eof_position = i64::try_from(source.len())
            .map_err(|_| VmError::InternalError("source length exceeds Int64".to_string()))?
            .checked_add(1)
            .ok_or_else(|| VmError::InternalError("source length overflows Int64".to_string()))?;
        if start < 1 || start > eof_position {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![start],
                shape: vec![source.len() + 1],
            });
        }
        if start == eof_position {
            return Ok((Value::Nothing, start));
        }
        let start_0based = usize::try_from(start - 1).map_err(|_| {
            VmError::InternalError("Meta.parse start does not fit usize".to_string())
        })?;
        if !source.is_char_boundary(start_0based) {
            // Rust `str` slicing would panic at a UTF-8 continuation byte.
            // Upstream falls back to flisp and raises ParseError(msg, nothing)
            // for this caller error (Issue #11639).
            return Err(VmError::ParseError("invalid UTF-8 sequence".to_string()));
        }

        // Parse from the substring
        let substring = &source[start_0based..];
        use subset_julia_vm_parser::Parser as RustParser;

        let (node, errors, consumed) = RustParser::new(substring).parse_one();

        if !errors.is_empty() {
            let segment = &substring[..consumed.min(substring.len())];
            let error_msg = errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ");
            // Upstream raises `Base.Meta.ParseError`; sjulia's Base defines the
            // matching `ParseError` struct. This used to raise the `TypeError`
            // variant with a `"ParseError: "` text prefix (Issue #11146).
            let detail = self.parser_error_detail(segment, start_0based, &errors)?;
            if errors.is_incomplete_input() {
                let error = self.immutable_struct_value(
                    "ParseError",
                    vec![Value::str_new(error_msg), detail],
                )?;
                let next_position = i64::try_from(start_0based + consumed.min(substring.len()) + 1)
                    .map_err(|_| {
                        VmError::InternalError("Meta.parse next position exceeds Int64".to_string())
                    })?;
                return Ok((Self::expr_value("incomplete", vec![error]), next_position));
            }
            return Err(self.parse_error_with_detail(error_msg, detail));
        }

        let Some(first_child) = node else {
            return Ok((Value::Nothing, start));
        };

        // Get the first expression
        let value = self.cst_to_value(&first_child, substring)?;

        // Calculate next position (1-based)
        let next_pos =
            i64::try_from(start_0based + consumed.min(substring.len()) + 1).map_err(|_| {
                VmError::InternalError("Meta.parse next position exceeds Int64".to_string())
            })?;

        Ok((value, next_pos))
    }

    fn expr_value(head: &str, args: Vec<Value>) -> Value {
        Value::Expr(ExprValue::from_head(head, args))
    }

    fn block_value(args: Vec<Value>) -> Value {
        Self::expr_value("block", args)
    }

    fn named_children(
        node: &subset_julia_vm_parser::CstNode,
    ) -> Vec<&subset_julia_vm_parser::CstNode> {
        node.children
            .iter()
            .filter(|child| child.is_named)
            .collect()
    }

    fn cst_to_block_value(
        &self,
        node: &subset_julia_vm_parser::CstNode,
        source: &str,
    ) -> Result<Value, VmError> {
        use subset_julia_vm_parser::NodeKind;

        if matches!(node.kind, NodeKind::Block | NodeKind::BeginBlock) {
            self.cst_to_value(node, source)
        } else {
            Ok(Self::block_value(vec![self.cst_to_value(node, source)?]))
        }
    }

    fn let_bindings_to_value(
        &self,
        node: &subset_julia_vm_parser::CstNode,
        source: &str,
    ) -> Result<Value, VmError> {
        let mut bindings = Self::named_children(node)
            .into_iter()
            .map(|child| self.cst_to_value(child, source))
            .collect::<Result<Vec<_>, _>>()?;

        if bindings.len() == 1 {
            Ok(bindings.remove(0))
        } else {
            Ok(Self::block_value(bindings))
        }
    }

    fn let_expr_to_value(
        &self,
        node: &subset_julia_vm_parser::CstNode,
        source: &str,
    ) -> Result<Value, VmError> {
        use subset_julia_vm_parser::NodeKind;

        let children = Self::named_children(node);
        let mut child_index = 0;
        let bindings = if children
            .first()
            .map(|child| child.kind == NodeKind::LetBindings)
            .unwrap_or(false)
        {
            child_index = 1;
            self.let_bindings_to_value(children[0], source)?
        } else {
            Self::block_value(Vec::new())
        };
        let body = match children.get(child_index) {
            Some(body_node) => self.cst_to_block_value(body_node, source)?,
            None => Self::block_value(Vec::new()),
        };

        Ok(Self::expr_value("let", vec![bindings, body]))
    }

    fn else_clause_to_value(
        &self,
        node: &subset_julia_vm_parser::CstNode,
        source: &str,
    ) -> Result<Value, VmError> {
        match Self::named_children(node).first() {
            Some(body) => self.cst_to_block_value(body, source),
            None => Ok(Self::block_value(Vec::new())),
        }
    }

    fn elseif_clause_to_value(
        &self,
        node: &subset_julia_vm_parser::CstNode,
        tail: Option<Value>,
        source: &str,
    ) -> Result<Value, VmError> {
        let children = Self::named_children(node);
        if children.len() < 2 {
            return Err(VmError::TypeError(
                "Invalid elseif clause: missing condition or body".to_string(),
            ));
        }

        let condition = self.cst_to_value(children[0], source)?;
        let then_branch = self.cst_to_block_value(children[1], source)?;
        let mut args = vec![Self::block_value(vec![condition]), then_branch];
        if let Some(tail) = tail {
            args.push(tail);
        }

        Ok(Self::expr_value("elseif", args))
    }

    fn if_statement_to_value(
        &self,
        node: &subset_julia_vm_parser::CstNode,
        source: &str,
    ) -> Result<Value, VmError> {
        use subset_julia_vm_parser::NodeKind;

        let children = Self::named_children(node);
        if children.len() < 2 {
            return Err(VmError::TypeError(
                "Invalid if statement: missing condition or body".to_string(),
            ));
        }

        let condition = self.cst_to_value(children[0], source)?;
        let then_branch = self.cst_to_block_value(children[1], source)?;
        let mut elseif_clauses = Vec::new();
        let mut tail = None;

        for child in children.iter().skip(2) {
            match child.kind {
                NodeKind::ElseifClause => elseif_clauses.push(*child),
                NodeKind::ElseClause => tail = Some(self.else_clause_to_value(child, source)?),
                _ => {
                    return Err(VmError::TypeError(format!(
                        "Invalid if statement child: {:?}",
                        child.kind
                    )));
                }
            }
        }

        for elseif_clause in elseif_clauses.into_iter().rev() {
            tail = Some(self.elseif_clause_to_value(elseif_clause, tail, source)?);
        }

        let mut args = vec![condition, then_branch];
        if let Some(tail) = tail {
            args.push(tail);
        }

        Ok(Self::expr_value("if", args))
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
                // Parse integer literal. Issue #4753: overflowing literals
                // (e.g. `Meta.parse(repr(typemin(Int64)))` produces the
                // magnitude `9223372036854775808` which doesn't fit in
                // i64) used to fail with `Invalid integer`. Match upstream
                // Julia by promoting to Int128 and then BigInt as the
                // literal magnitude grows — `-9223372036854775808` then
                // parses as `Expr(:call, :-, Int128(9223372036854775808))`,
                // and the unary-minus eval lands at `Int64::MIN` exactly.
                let clean = text.replace("_", "");
                if let Ok(n) = clean.parse::<i64>() {
                    return Ok(Value::I64(n));
                }
                let (radix, body) = if clean.starts_with("0x") || clean.starts_with("0X") {
                    (16, &clean[2..])
                } else if clean.starts_with("0o") || clean.starts_with("0O") {
                    (8, &clean[2..])
                } else if clean.starts_with("0b") || clean.starts_with("0B") {
                    (2, &clean[2..])
                } else {
                    (10, clean.as_str())
                };
                if let Ok(n) = i64::from_str_radix(body, radix) {
                    return Ok(Value::I64(n));
                }
                if let Ok(n) = i128::from_str_radix(body, radix) {
                    return Ok(Value::I128(n));
                }
                if radix == 10 {
                    if let Ok(big) = body.parse::<crate::vm::value::RustBigInt>() {
                        return Ok(Value::BigInt(big));
                    }
                }
                Err(VmError::TypeError(format!("Invalid integer: {}", text)))
            }

            NodeKind::FloatLiteral => parse_float(text)
                .map(|parsed| match parsed {
                    ParsedFloat::F64(value) => Value::F64(value),
                    ParsedFloat::F32(value) => Value::F32(value),
                })
                .ok_or_else(|| VmError::TypeError(format!("Invalid float: {}", text))),

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
                Ok(Value::str_new(unescaped))
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

            // `var"..."` non-standard identifiers keep the full source span;
            // the Symbol name is the quoted content (Issue #8754).
            NodeKind::Identifier => Ok(Value::Symbol(SymbolValue::new(
                subset_julia_vm_parser::strip_var_quotes(text),
            ))),

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
                Ok(Self::block_value(args))
            }

            // If statement: if cond ... end -> Expr(:if, cond, then_block, else_block?)
            //
            // The Rust parser has parser-internal `ElseifClause` / `ElseClause`
            // nodes, but upstream `Meta.parse` returns a normalized `Expr(:if, ...)`
            // with branch bodies as `Expr(:block, ...)` and nested `Expr(:elseif, ...)`
            // tails.
            NodeKind::IfStatement => self.if_statement_to_value(node, source),

            NodeKind::ElseifClause => self.elseif_clause_to_value(node, None, source),

            NodeKind::ElseClause => self.else_clause_to_value(node, source),

            NodeKind::LetBindings => self.let_bindings_to_value(node, source),

            // Let expression: let bindings body end -> Expr(:let, bindings, body)
            //
            // Normalize the parser-internal `LetExpression` / `LetBindings` nodes to
            // upstream-shaped `Expr(:let, ...)` so `Meta.parse` values can flow back
            // through eval and macro-return lowering (Issue #7754).
            NodeKind::LetExpression | NodeKind::LetStatement => {
                self.let_expr_to_value(node, source)
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

            // Prefixed string literal: `var"@q"`, `r"abc"`, `big"123"`, ... (Issue #7753)
            //
            // Upstream `Meta.parse`:
            //   - `var"name"` is the *non-standard identifier* syntax and parses to
            //     `Symbol("name")` (NOT a string / a `:prefixedstringliteral` Expr),
            //     which `string`/`show` print back as `var"name"` when the name is not
            //     a plain identifier (see `format_symbol_name`, Issue #7676).
            //   - every other prefix `x"content"` is the string-macro sugar and parses
            //     to `Expr(:macrocall, Symbol("@x_str"), LineNumberNode(...), "content")`.
            NodeKind::PrefixedStringLiteral => {
                let children: Vec<&subset_julia_vm_parser::CstNode> =
                    node.children.iter().filter(|c| c.is_named).collect();
                if children.len() < 2 {
                    return Err(VmError::TypeError(
                        "Invalid prefixed string literal".to_string(),
                    ));
                }
                let prefix_text = &source[children[0].span.start..children[0].span.end];
                let string_text = &source[children[1].span.start..children[1].span.end];
                // Mirror the lowering path (Issue #7676): the var/string content is the
                // text with its surrounding `"` quotes stripped.
                let content = string_text.trim_matches('"').to_string();

                if prefix_text == "var" {
                    Ok(Value::Symbol(SymbolValue::new(&content)))
                } else {
                    let macro_sym = format!("@{}_str", prefix_text);
                    Ok(Value::Expr(ExprValue::from_head(
                        "macrocall",
                        vec![
                            Value::Symbol(SymbolValue::new(&macro_sym)),
                            Value::LineNumberNode(crate::vm::value::LineNumberNodeValue::new(
                                1, None,
                            )),
                            Value::str_new(content),
                        ],
                    )))
                }
            }

            // Keyword argument: `name = value` in a call -> Expr(:kw, name, value)
            // (Issue #7753). Upstream parses `f(a=2)` as
            // `Expr(:call, :f, Expr(:kw, :a, 2))`, NOT a parser-internal
            // `:keywordargument` head.
            NodeKind::KeywordArgument => {
                let named: Vec<&subset_julia_vm_parser::CstNode> =
                    node.children.iter().filter(|c| c.is_named).collect();
                if named.len() < 2 {
                    return Err(VmError::TypeError("Invalid keyword argument".to_string()));
                }
                let name = self.cst_to_value(named[0], source)?;
                let value = self.cst_to_value(named[named.len() - 1], source)?;
                Ok(Value::Expr(ExprValue::from_head("kw", vec![name, value])))
            }

            // Quote: :(expr) -> Expr(:quote, expr); :symbol -> QuoteNode(:symbol)
            NodeKind::QuoteExpression => {
                if let Some(child) = node.children.iter().find(|c| c.is_named) {
                    let inner = self.cst_to_value(child, source)?;
                    Ok(Value::Expr(ExprValue::from_head("quote", vec![inner])))
                } else {
                    // `:symbol` -> QuoteNode(Symbol). Upstream `Meta.parse(":x")`
                    // returns `QuoteNode(:x)` (and `Dict(:a => 1)` keeps the `:a`
                    // QuoteNode so it prints `:a`, not `a` — Issue #7753).
                    let sym_text = text.trim_start_matches(':');
                    Ok(Value::QuoteNode(Box::new(Value::Symbol(SymbolValue::new(
                        sym_text,
                    )))))
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

            // `parse_one` uses SourceFile as the carrier for semicolon-joined
            // expressions, which Julia exposes as Expr(:toplevel, ...).
            NodeKind::SourceFile => {
                let mut args = Vec::new();
                for child in node.children.iter().filter(|child| child.is_named) {
                    args.push(self.cst_to_value(child, source)?);
                }
                Ok(Self::expr_value("toplevel", args))
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
        let mut pos: i64 = 1; // Julia uses 1-based indexing
        let code_length = i64::try_from(code.len())
            .map_err(|_| VmError::InternalError("source length exceeds Int64".to_string()))?;

        while pos <= code_length {
            // Parse one expression starting at pos
            let (expr, next_pos) = self.parse_string_at_to_value(code, pos)?;

            // Check if we got nothing (end of string or whitespace-only)
            if matches!(expr, Value::Nothing) {
                break;
            }

            // Evaluate the expression
            result = self.eval_module_expr_value(&expr, None)?;

            // Check for progress to avoid infinite loop
            if next_pos <= pos {
                break;
            }

            pos = next_pos;
        }

        Ok(result)
    }
}
