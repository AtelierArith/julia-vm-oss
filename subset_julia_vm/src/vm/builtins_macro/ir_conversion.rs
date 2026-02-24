//! IR conversion for the macro system.
//!
//! Converts between Value/Expr AST and Core IR representations.
//! Implements Meta.lower and source-string round-tripping.

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::value::{ExprValue, SymbolValue, Value};
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    pub(super) fn lower_value_to_ir(&self, val: &Value) -> Result<Value, VmError> {
        match val {
            // Literals and simple values are already in their lowered form
            Value::I64(_)
            | Value::I32(_)
            | Value::I16(_)
            | Value::I8(_)
            | Value::U64(_)
            | Value::U32(_)
            | Value::U16(_)
            | Value::U8(_)
            | Value::F64(_)
            | Value::F32(_)
            | Value::Bool(_)
            | Value::Char(_)
            | Value::Str(_)
            | Value::Nothing
            | Value::Symbol(_) => {
                // These are already "lowered" - return as-is
                Ok(val.clone())
            }

            Value::Expr(_expr) => {
                // For expressions, we need to lower them through our pipeline
                // Convert the Expr back to source code representation
                let source = self.value_to_source_string(val);

                // Parse the source
                use crate::lowering::Lowering;
                use crate::parser::Parser;

                let mut parser = Parser::new().map_err(|e| {
                    VmError::TypeError(format!("Parser initialization failed: {}", e))
                })?;

                let parse_outcome = parser
                    .parse(&source)
                    .map_err(|e| VmError::TypeError(format!("Parse error in Meta.lower: {}", e)))?;

                // Lower the parsed AST
                let mut lowering = Lowering::new(&source);
                let program = lowering.lower(parse_outcome).map_err(|e| {
                    VmError::TypeError(format!("Lowering error in Meta.lower: {}", e))
                })?;

                // Convert the lowered IR to an Expr representation
                // The main block contains the lowered statements
                self.ir_block_to_expr(&program.main)
            }

            Value::QuoteNode(qn) => {
                // QuoteNode wraps a value - lower the inner value
                self.lower_value_to_ir(qn.as_ref())
            }

            // For other values (arrays, tuples, structs, etc.), return as-is
            // since they represent runtime values, not AST to be lowered
            _ => Ok(val.clone()),
        }
    }

    /// Convert a Value (AST representation) to a source string for parsing.
    fn value_to_source_string(&self, val: &Value) -> String {
        match val {
            Value::I64(n) => format!("{}", n),
            Value::I32(n) => format!("{}", n),
            Value::F64(n) => {
                if n.is_nan() {
                    "NaN".to_string()
                } else if n.is_infinite() {
                    if *n > 0.0 {
                        "Inf".to_string()
                    } else {
                        "-Inf".to_string()
                    }
                } else {
                    format!("{}", n)
                }
            }
            Value::F32(n) => {
                if n.is_nan() {
                    "NaN32".to_string()
                } else if n.is_infinite() {
                    if *n > 0.0 {
                        "Inf32".to_string()
                    } else {
                        "-Inf32".to_string()
                    }
                } else {
                    format!("{}f0", n)
                }
            }
            Value::Bool(b) => {
                if *b {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            Value::Str(s) => format!("{:?}", s),
            Value::Char(c) => format!("'{}'", c),
            Value::Symbol(sym) => sym.as_str().to_string(),
            Value::Nothing => "nothing".to_string(),
            Value::Expr(expr) => self.expr_to_source_string(expr),
            Value::QuoteNode(qn) => {
                format!("$(QuoteNode({}))", self.value_to_source_string(qn.as_ref()))
            }
            _ => format!("{:?}", val),
        }
    }

    /// Convert an ExprValue to source string representation.
    fn expr_to_source_string(&self, expr: &ExprValue) -> String {
        let head = expr.head.as_str();
        let args = &expr.args;

        match head {
            "call" => {
                // Function call: Expr(:call, func, args...)
                if args.is_empty() {
                    return "()".to_string();
                }
                let func = self.value_to_source_string(&args[0]);
                let call_args: Vec<String> = args[1..]
                    .iter()
                    .map(|a| self.value_to_source_string(a))
                    .collect();
                format!("{}({})", func, call_args.join(", "))
            }
            "=" => {
                // Assignment: Expr(:(=), lhs, rhs)
                if args.len() >= 2 {
                    let lhs = self.value_to_source_string(&args[0]);
                    let rhs = self.value_to_source_string(&args[1]);
                    format!("{} = {}", lhs, rhs)
                } else {
                    format!("(= {:?})", args)
                }
            }
            "block" => {
                // Block: Expr(:block, stmts...)
                let stmts: Vec<String> = args
                    .iter()
                    .map(|a| self.value_to_source_string(a))
                    .collect();
                format!("begin\n{}\nend", stmts.join("\n"))
            }
            "if" => {
                // If: Expr(:if, cond, then_branch, else_branch?)
                if args.len() >= 2 {
                    let cond = self.value_to_source_string(&args[0]);
                    let then_branch = self.value_to_source_string(&args[1]);
                    if args.len() >= 3 {
                        let else_branch = self.value_to_source_string(&args[2]);
                        format!("if {}\n{}\nelse\n{}\nend", cond, then_branch, else_branch)
                    } else {
                        format!("if {}\n{}\nend", cond, then_branch)
                    }
                } else {
                    format!("(if {:?})", args)
                }
            }
            "quote" => {
                // Quote: Expr(:quote, expr)
                if !args.is_empty() {
                    format!(":({}))", self.value_to_source_string(&args[0]))
                } else {
                    ":()".to_string()
                }
            }
            "ref" => {
                // Indexing: Expr(:ref, array, indices...)
                if args.is_empty() {
                    return "[]".to_string();
                }
                let arr = self.value_to_source_string(&args[0]);
                let indices: Vec<String> = args[1..]
                    .iter()
                    .map(|a| self.value_to_source_string(a))
                    .collect();
                format!("{}[{}]", arr, indices.join(", "))
            }
            "tuple" | "vect" => {
                // Tuple or vector literal
                let elems: Vec<String> = args
                    .iter()
                    .map(|a| self.value_to_source_string(a))
                    .collect();
                if head == "vect" {
                    format!("[{}]", elems.join(", "))
                } else {
                    format!("({})", elems.join(", "))
                }
            }
            _ => {
                // Generic expression
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| self.value_to_source_string(a))
                    .collect();
                if args_str.is_empty() {
                    head.to_string()
                } else {
                    format!("Expr(:{}, {})", head, args_str.join(", "))
                }
            }
        }
    }

    /// Convert a lowered IR Block to an Expr representation.
    fn ir_block_to_expr(&self, block: &crate::ir::core::Block) -> Result<Value, VmError> {
        let mut args = Vec::new();

        for stmt in &block.stmts {
            let stmt_val = self.ir_stmt_to_value(stmt)?;
            args.push(stmt_val);
        }

        // If only one statement, return it directly
        if args.len() == 1 {
            Ok(args.pop().unwrap_or(Value::Nothing))
        } else {
            // Wrap in a :block Expr
            Ok(Value::Expr(ExprValue::from_head("block", args)))
        }
    }

    /// Convert a lowered IR statement to a Value representation.
    fn ir_stmt_to_value(&self, stmt: &crate::ir::core::Stmt) -> Result<Value, VmError> {
        use crate::ir::core::Stmt;

        match stmt {
            Stmt::Block(block) => self.ir_block_to_expr(block),
            Stmt::Expr { expr, .. } => self.ir_expr_to_value(expr),
            Stmt::Assign { var, value, .. } => {
                let name_val = Value::Symbol(SymbolValue::new(var));
                let val_val = self.ir_expr_to_value(value)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "=",
                    vec![name_val, val_val],
                )))
            }
            Stmt::AddAssign { var, value, .. } => {
                let name_val = Value::Symbol(SymbolValue::new(var));
                let val_val = self.ir_expr_to_value(value)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "+=",
                    vec![name_val, val_val],
                )))
            }
            Stmt::Return { value, .. } => {
                let val = if let Some(v) = value {
                    self.ir_expr_to_value(v)?
                } else {
                    Value::Nothing
                };
                Ok(Value::Expr(ExprValue::from_head("return", vec![val])))
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let cond_val = self.ir_expr_to_value(condition)?;
                let then_val = self.ir_block_to_expr(then_branch)?;
                let mut args = vec![cond_val, then_val];
                if let Some(else_b) = else_branch {
                    args.push(self.ir_block_to_expr(else_b)?);
                }
                Ok(Value::Expr(ExprValue::from_head("if", args)))
            }
            Stmt::While {
                condition, body, ..
            } => {
                let cond_val = self.ir_expr_to_value(condition)?;
                let body_val = self.ir_block_to_expr(body)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "while",
                    vec![cond_val, body_val],
                )))
            }
            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                ..
            } => {
                let var_val = Value::Symbol(SymbolValue::new(var));
                let start_val = self.ir_expr_to_value(start)?;
                let end_val = self.ir_expr_to_value(end)?;
                let range_expr = if let Some(step_expr) = step {
                    let step_val = self.ir_expr_to_value(step_expr)?;
                    Value::Expr(ExprValue::from_head(
                        "call",
                        vec![
                            Value::Symbol(SymbolValue::new(":")),
                            start_val,
                            step_val,
                            end_val,
                        ],
                    ))
                } else {
                    Value::Expr(ExprValue::from_head(
                        "call",
                        vec![Value::Symbol(SymbolValue::new(":")), start_val, end_val],
                    ))
                };
                let body_val = self.ir_block_to_expr(body)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "for",
                    vec![
                        Value::Expr(ExprValue::from_head("=", vec![var_val, range_expr])),
                        body_val,
                    ],
                )))
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                let var_val = Value::Symbol(SymbolValue::new(var));
                let iter_val = self.ir_expr_to_value(iterable)?;
                let body_val = self.ir_block_to_expr(body)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "for",
                    vec![
                        Value::Expr(ExprValue::from_head("=", vec![var_val, iter_val])),
                        body_val,
                    ],
                )))
            }
            Stmt::ForEachTuple {
                vars,
                iterable,
                body,
                ..
            } => {
                let var_vals: Vec<Value> = vars
                    .iter()
                    .map(|v| Value::Symbol(SymbolValue::new(v)))
                    .collect();
                let tuple_var = Value::Expr(ExprValue::from_head("tuple", var_vals));
                let iter_val = self.ir_expr_to_value(iterable)?;
                let body_val = self.ir_block_to_expr(body)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "for",
                    vec![
                        Value::Expr(ExprValue::from_head("=", vec![tuple_var, iter_val])),
                        body_val,
                    ],
                )))
            }
            Stmt::Break { .. } => Ok(Value::Expr(ExprValue::from_head("break", vec![]))),
            Stmt::Continue { .. } => Ok(Value::Expr(ExprValue::from_head("continue", vec![]))),
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block: _,
                finally_block,
                ..
            } => {
                let try_val = self.ir_block_to_expr(try_block)?;
                let mut args = vec![try_val];

                if let Some(catch_b) = catch_block {
                    let catch_val = self.ir_block_to_expr(catch_b)?;
                    if let Some(var) = catch_var {
                        args.push(Value::Symbol(SymbolValue::new(var)));
                    }
                    args.push(catch_val);
                }

                if let Some(finally_b) = finally_block {
                    args.push(self.ir_block_to_expr(finally_b)?);
                }

                Ok(Value::Expr(ExprValue::from_head("try", args)))
            }
            Stmt::Test {
                condition, message, ..
            } => {
                let cond_val = self.ir_expr_to_value(condition)?;
                let mut args = vec![cond_val];
                if let Some(msg) = message {
                    args.push(Value::Str(msg.clone()));
                }
                Ok(Value::Expr(ExprValue::from_head(
                    "macrocall",
                    vec![
                        Value::Symbol(SymbolValue::new("@test")),
                        Value::Expr(ExprValue::from_head("tuple", args)),
                    ],
                )))
            }
            Stmt::TestSet { name, body, .. } => {
                let body_val = self.ir_block_to_expr(body)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "macrocall",
                    vec![
                        Value::Symbol(SymbolValue::new("@testset")),
                        Value::Str(name.clone()),
                        body_val,
                    ],
                )))
            }
            Stmt::TestThrows {
                exception_type,
                expr,
                ..
            } => {
                let expr_val = self.ir_expr_to_value(expr)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "macrocall",
                    vec![
                        Value::Symbol(SymbolValue::new("@test_throws")),
                        Value::Symbol(SymbolValue::new(exception_type)),
                        expr_val,
                    ],
                )))
            }
            Stmt::Timed { body, .. } => {
                let body_val = self.ir_block_to_expr(body)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "macrocall",
                    vec![Value::Symbol(SymbolValue::new("@time")), body_val],
                )))
            }
            Stmt::IndexAssign {
                array,
                indices,
                value,
                ..
            } => {
                let array_val = Value::Symbol(SymbolValue::new(array));
                let mut idx_vals: Vec<Value> = indices
                    .iter()
                    .map(|i| self.ir_expr_to_value(i))
                    .collect::<Result<_, _>>()?;
                let val_val = self.ir_expr_to_value(value)?;
                let ref_expr = {
                    let mut args = vec![array_val];
                    args.append(&mut idx_vals);
                    Value::Expr(ExprValue::from_head("ref", args))
                };
                Ok(Value::Expr(ExprValue::from_head(
                    "=",
                    vec![ref_expr, val_val],
                )))
            }
            Stmt::FieldAssign {
                object,
                field,
                value,
                ..
            } => {
                let obj_val = Value::Symbol(SymbolValue::new(object));
                let val_val = self.ir_expr_to_value(value)?;
                let field_access = Value::Expr(ExprValue::from_head(
                    ".",
                    vec![
                        obj_val,
                        Value::QuoteNode(Box::new(Value::Symbol(SymbolValue::new(field)))),
                    ],
                ));
                Ok(Value::Expr(ExprValue::from_head(
                    "=",
                    vec![field_access, val_val],
                )))
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                let target_vals: Vec<Value> = targets
                    .iter()
                    .map(|t| Value::Symbol(SymbolValue::new(t)))
                    .collect();
                let tuple_target = Value::Expr(ExprValue::from_head("tuple", target_vals));
                let val_val = self.ir_expr_to_value(value)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "=",
                    vec![tuple_target, val_val],
                )))
            }
            Stmt::DictAssign {
                dict, key, value, ..
            } => {
                let dict_val = Value::Symbol(SymbolValue::new(dict));
                let key_val = self.ir_expr_to_value(key)?;
                let val_val = self.ir_expr_to_value(value)?;
                let ref_expr = Value::Expr(ExprValue::from_head("ref", vec![dict_val, key_val]));
                Ok(Value::Expr(ExprValue::from_head(
                    "=",
                    vec![ref_expr, val_val],
                )))
            }
            Stmt::Using { module, .. } => Ok(Value::Expr(ExprValue::from_head(
                "using",
                vec![Value::Symbol(SymbolValue::new(module))],
            ))),
            Stmt::Export { names, .. } => {
                let name_vals: Vec<Value> = names
                    .iter()
                    .map(|n| Value::Symbol(SymbolValue::new(n)))
                    .collect();
                Ok(Value::Expr(ExprValue::from_head("export", name_vals)))
            }
            Stmt::FunctionDef { func, .. } => {
                // Simplified function definition representation
                let func_name = Value::Symbol(SymbolValue::new(&func.name));
                Ok(Value::Expr(ExprValue::from_head(
                    "function",
                    vec![func_name],
                )))
            }
            Stmt::Label { name, .. } => {
                // Convert @label to Expr(:symboliclabel, name)
                Ok(Value::Expr(ExprValue::from_head(
                    "symboliclabel",
                    vec![Value::Symbol(SymbolValue::new(name))],
                )))
            }
            Stmt::Goto { name, .. } => {
                // Convert @goto to Expr(:symbolicgoto, name)
                Ok(Value::Expr(ExprValue::from_head(
                    "symbolicgoto",
                    vec![Value::Symbol(SymbolValue::new(name))],
                )))
            }
            Stmt::EnumDef { enum_def, .. } => {
                // Convert @enum to Expr(:macrocall, Symbol("@enum"), ...)
                let mut args = vec![Value::Symbol(SymbolValue::new("@enum"))];
                args.push(Value::Symbol(SymbolValue::new(&enum_def.name)));
                for member in &enum_def.members {
                    args.push(Value::Symbol(SymbolValue::new(&member.name)));
                }
                Ok(Value::Expr(ExprValue::from_head("macrocall", args)))
            }
        }
    }

    /// Convert a lowered IR expression to a Value representation.
    fn ir_expr_to_value(&self, expr: &crate::ir::core::Expr) -> Result<Value, VmError> {
        use crate::ir::core::{BinaryOp, Expr as IrExpr, UnaryOp};

        match expr {
            IrExpr::Literal(lit, _) => self.ir_literal_to_value(lit),
            IrExpr::Var(name, _) => Ok(Value::Symbol(SymbolValue::new(name))),
            IrExpr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                let mut call_args = vec![Value::Symbol(SymbolValue::new(function))];
                for arg in args {
                    call_args.push(self.ir_expr_to_value(arg)?);
                }
                // Handle kwargs
                for (kw_name, kw_value) in kwargs {
                    let kw_expr = Value::Expr(ExprValue::from_head(
                        "kw",
                        vec![
                            Value::Symbol(SymbolValue::new(kw_name)),
                            self.ir_expr_to_value(kw_value)?,
                        ],
                    ));
                    call_args.push(kw_expr);
                }
                Ok(Value::Expr(ExprValue::from_head("call", call_args)))
            }
            IrExpr::BinaryOp {
                op, left, right, ..
            } => {
                let left_val = self.ir_expr_to_value(left)?;
                let right_val = self.ir_expr_to_value(right)?;
                let op_str = match op {
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    BinaryOp::Div => "/",
                    BinaryOp::IntDiv => "÷",
                    BinaryOp::Mod => "%",
                    BinaryOp::Pow => "^",
                    BinaryOp::Lt => "<",
                    BinaryOp::Gt => ">",
                    BinaryOp::Le => "<=",
                    BinaryOp::Ge => ">=",
                    BinaryOp::Eq => "==",
                    BinaryOp::Ne => "!=",
                    BinaryOp::Egal => "===",
                    BinaryOp::NotEgal => "!==",
                    BinaryOp::Subtype => "<:",
                    BinaryOp::And => "&&",
                    BinaryOp::Or => "||",
                };
                Ok(Value::Expr(ExprValue::from_head(
                    "call",
                    vec![Value::Symbol(SymbolValue::new(op_str)), left_val, right_val],
                )))
            }
            IrExpr::UnaryOp { op, operand, .. } => {
                let operand_val = self.ir_expr_to_value(operand)?;
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                    UnaryOp::Pos => "+",
                };
                Ok(Value::Expr(ExprValue::from_head(
                    "call",
                    vec![Value::Symbol(SymbolValue::new(op_str)), operand_val],
                )))
            }
            IrExpr::Index { array, indices, .. } => {
                let array_val = self.ir_expr_to_value(array)?;
                let mut ref_args = vec![array_val];
                for idx in indices {
                    ref_args.push(self.ir_expr_to_value(idx)?);
                }
                Ok(Value::Expr(ExprValue::from_head("ref", ref_args)))
            }
            IrExpr::TupleLiteral { elements, .. } => {
                let elems: Vec<Value> = elements
                    .iter()
                    .map(|e| self.ir_expr_to_value(e))
                    .collect::<Result<_, _>>()?;
                Ok(Value::Expr(ExprValue::from_head("tuple", elems)))
            }
            IrExpr::ArrayLiteral { elements, .. } => {
                let elems: Vec<Value> = elements
                    .iter()
                    .map(|e| self.ir_expr_to_value(e))
                    .collect::<Result<_, _>>()?;
                Ok(Value::Expr(ExprValue::from_head("vect", elems)))
            }
            IrExpr::Range {
                start, stop, step, ..
            } => {
                let start_val = self.ir_expr_to_value(start)?;
                let stop_val = self.ir_expr_to_value(stop)?;
                if let Some(step_expr) = step {
                    let step_val = self.ir_expr_to_value(step_expr)?;
                    Ok(Value::Expr(ExprValue::from_head(
                        "call",
                        vec![
                            Value::Symbol(SymbolValue::new(":")),
                            start_val,
                            step_val,
                            stop_val,
                        ],
                    )))
                } else {
                    Ok(Value::Expr(ExprValue::from_head(
                        "call",
                        vec![Value::Symbol(SymbolValue::new(":")), start_val, stop_val],
                    )))
                }
            }
            IrExpr::FieldAccess { object, field, .. } => {
                let obj_val = self.ir_expr_to_value(object)?;
                Ok(Value::Expr(ExprValue::from_head(
                    ".",
                    vec![
                        obj_val,
                        Value::QuoteNode(Box::new(Value::Symbol(SymbolValue::new(field)))),
                    ],
                )))
            }
            IrExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                let cond_val = self.ir_expr_to_value(condition)?;
                let then_val = self.ir_expr_to_value(then_expr)?;
                let else_val = self.ir_expr_to_value(else_expr)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "if",
                    vec![cond_val, then_val, else_val],
                )))
            }
            IrExpr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            }
            | IrExpr::Generator {
                body,
                var,
                iter,
                filter,
                ..
            } => {
                let body_val = self.ir_expr_to_value(body)?;
                let var_val = Value::Symbol(SymbolValue::new(var));
                let iter_val = self.ir_expr_to_value(iter)?;
                let mut gen_args = vec![
                    body_val,
                    Value::Expr(ExprValue::from_head("=", vec![var_val, iter_val])),
                ];
                if let Some(f) = filter {
                    gen_args.push(self.ir_expr_to_value(f)?);
                }
                Ok(Value::Expr(ExprValue::from_head("generator", gen_args)))
            }
            IrExpr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                let body_val = self.ir_expr_to_value(body)?;
                let mut gen_args = vec![body_val];
                for (var, iter_expr) in iterations {
                    let var_val = Value::Symbol(SymbolValue::new(var));
                    let iter_val = self.ir_expr_to_value(iter_expr)?;
                    gen_args.push(Value::Expr(ExprValue::from_head(
                        "=",
                        vec![var_val, iter_val],
                    )));
                }
                if let Some(f) = filter {
                    gen_args.push(self.ir_expr_to_value(f)?);
                }
                Ok(Value::Expr(ExprValue::from_head("generator", gen_args)))
            }
            IrExpr::Pair { key, value, .. } => {
                let left_val = self.ir_expr_to_value(key)?;
                let right_val = self.ir_expr_to_value(value)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "call",
                    vec![Value::Symbol(SymbolValue::new("=>")), left_val, right_val],
                )))
            }
            IrExpr::DictLiteral { pairs, .. } => {
                let mut dict_args = vec![Value::Symbol(SymbolValue::new("Dict"))];
                for (k, v) in pairs {
                    dict_args.push(Value::Expr(ExprValue::from_head(
                        "call",
                        vec![
                            Value::Symbol(SymbolValue::new("=>")),
                            self.ir_expr_to_value(k)?,
                            self.ir_expr_to_value(v)?,
                        ],
                    )));
                }
                Ok(Value::Expr(ExprValue::from_head("call", dict_args)))
            }
            IrExpr::LetBlock { bindings, body, .. } => {
                let mut let_args = vec![];
                for (name, val) in bindings {
                    let_args.push(Value::Expr(ExprValue::from_head(
                        "=",
                        vec![
                            Value::Symbol(SymbolValue::new(name)),
                            self.ir_expr_to_value(val)?,
                        ],
                    )));
                }
                let body_val = self.ir_block_to_expr(body)?;
                let_args.push(body_val);
                Ok(Value::Expr(ExprValue::from_head("let", let_args)))
            }
            IrExpr::StringConcat { parts, .. } => {
                let part_vals: Vec<Value> = parts
                    .iter()
                    .map(|p| self.ir_expr_to_value(p))
                    .collect::<Result<_, _>>()?;
                let mut args = vec![Value::Symbol(SymbolValue::new("string"))];
                args.extend(part_vals);
                Ok(Value::Expr(ExprValue::from_head("call", args)))
            }
            IrExpr::ModuleCall {
                module,
                function,
                args,
                kwargs,
                ..
            } => {
                let func_access = Value::Expr(ExprValue::from_head(
                    ".",
                    vec![
                        Value::Symbol(SymbolValue::new(module)),
                        Value::QuoteNode(Box::new(Value::Symbol(SymbolValue::new(function)))),
                    ],
                ));
                let mut call_args = vec![func_access];
                for arg in args {
                    call_args.push(self.ir_expr_to_value(arg)?);
                }
                for (kw_name, kw_value) in kwargs {
                    call_args.push(Value::Expr(ExprValue::from_head(
                        "kw",
                        vec![
                            Value::Symbol(SymbolValue::new(kw_name)),
                            self.ir_expr_to_value(kw_value)?,
                        ],
                    )));
                }
                Ok(Value::Expr(ExprValue::from_head("call", call_args)))
            }
            IrExpr::FunctionRef { name, .. } => Ok(Value::Symbol(SymbolValue::new(name))),
            IrExpr::NamedTupleLiteral { fields, .. } => {
                let mut args = vec![];
                for (name, val) in fields {
                    args.push(Value::Expr(ExprValue::from_head(
                        "=",
                        vec![
                            Value::Symbol(SymbolValue::new(name)),
                            self.ir_expr_to_value(val)?,
                        ],
                    )));
                }
                Ok(Value::Expr(ExprValue::from_head("tuple", args)))
            }
            IrExpr::SliceAll { .. } => Ok(Value::Symbol(SymbolValue::new(":"))),
            IrExpr::TypedEmptyArray { element_type, .. } => Ok(Value::Expr(ExprValue::from_head(
                "call",
                vec![
                    Value::Symbol(SymbolValue::new(element_type)),
                    Value::Expr(ExprValue::from_head("vect", vec![])),
                ],
            ))),
            IrExpr::New { args, .. } => {
                let mut call_args = vec![Value::Symbol(SymbolValue::new("new"))];
                for arg in args {
                    call_args.push(self.ir_expr_to_value(arg)?);
                }
                Ok(Value::Expr(ExprValue::from_head("call", call_args)))
            }
            IrExpr::QuoteLiteral { constructor, .. } => {
                let inner_val = self.ir_expr_to_value(constructor)?;
                Ok(Value::Expr(ExprValue::from_head("quote", vec![inner_val])))
            }
            IrExpr::AssignExpr { var, value, .. } => {
                let var_val = Value::Symbol(SymbolValue::new(var));
                let val_val = self.ir_expr_to_value(value)?;
                Ok(Value::Expr(ExprValue::from_head(
                    "=",
                    vec![var_val, val_val],
                )))
            }
            IrExpr::ReturnExpr { value, .. } => {
                let val = if let Some(v) = value {
                    self.ir_expr_to_value(v)?
                } else {
                    Value::Nothing
                };
                Ok(Value::Expr(ExprValue::from_head("return", vec![val])))
            }
            IrExpr::BreakExpr { .. } => Ok(Value::Expr(ExprValue::from_head("break", vec![]))),
            IrExpr::ContinueExpr { .. } => {
                Ok(Value::Expr(ExprValue::from_head("continue", vec![])))
            }
            IrExpr::Builtin { name, args, .. } => {
                let mut call_args = vec![Value::Symbol(SymbolValue::new(format!("{:?}", name)))];
                for arg in args {
                    call_args.push(self.ir_expr_to_value(arg)?);
                }
                Ok(Value::Expr(ExprValue::from_head("call", call_args)))
            }
            IrExpr::DynamicTypeConstruct {
                base, type_args, ..
            } => {
                // Convert to a curly expression: Base{type_arg1, type_arg2, ...}
                let mut curly_args = vec![Value::Symbol(SymbolValue::new(base))];
                for arg in type_args {
                    curly_args.push(self.ir_expr_to_value(arg)?);
                }
                Ok(Value::Expr(ExprValue::from_head("curly", curly_args)))
            }
        }
    }

    /// Convert an IR Literal to a Value.
    fn ir_literal_to_value(&self, lit: &crate::ir::core::Literal) -> Result<Value, VmError> {
        use crate::ir::core::Literal;

        match lit {
            Literal::Int(n) => Ok(Value::I64(*n)),
            Literal::Int128(n) => Ok(Value::I128(*n)),
            Literal::BigInt(s) => Ok(Value::Str(s.clone())), // Return as string
            Literal::BigFloat(s) => Ok(Value::Str(s.clone())), // Return as string
            Literal::Float(f) => Ok(Value::F64(*f)),
            Literal::Float32(f) => Ok(Value::F32(*f)),
            Literal::Float16(f) => Ok(Value::F16(*f)),
            Literal::Bool(b) => Ok(Value::Bool(*b)),
            Literal::Str(s) => Ok(Value::Str(s.clone())),
            Literal::Char(c) => Ok(Value::Char(*c)),
            Literal::Nothing => Ok(Value::Nothing),
            Literal::Missing => Ok(Value::Missing),
            Literal::Undef => {
                // Return a special Expr for undef
                Ok(Value::Expr(ExprValue::from_head("undef", vec![])))
            }
            Literal::Module(name) => Ok(Value::Symbol(SymbolValue::new(name))),
            Literal::Array(data, _shape) => {
                // Convert array literal data to a vect expression
                let elems: Vec<Value> = data.iter().map(|f| Value::F64(*f)).collect();
                Ok(Value::Expr(ExprValue::from_head("vect", elems)))
            }
            Literal::ArrayI64(data, _shape) => {
                // Convert I64 array literal data to a vect expression
                let elems: Vec<Value> = data.iter().map(|i| Value::I64(*i)).collect();
                Ok(Value::Expr(ExprValue::from_head("vect", elems)))
            }
            Literal::ArrayBool(data, _shape) => {
                // Convert Bool array literal data to a vect expression
                let elems: Vec<Value> = data.iter().map(|b| Value::Bool(*b)).collect();
                Ok(Value::Expr(ExprValue::from_head("vect", elems)))
            }
            Literal::Struct(name, _fields) => Ok(Value::Symbol(SymbolValue::new(name))),
            Literal::Symbol(s) => Ok(Value::Symbol(SymbolValue::new(s))),
            Literal::Expr { head, args } => {
                let args_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.ir_literal_to_value(a))
                    .collect::<Result<_, _>>()?;
                Ok(Value::Expr(ExprValue::from_head(head, args_vals)))
            }
            Literal::QuoteNode(inner) => {
                let inner_val = self.ir_literal_to_value(inner)?;
                Ok(Value::QuoteNode(Box::new(inner_val)))
            }
            Literal::LineNumberNode { line, file } => {
                use crate::vm::LineNumberNodeValue;
                Ok(Value::LineNumberNode(LineNumberNodeValue {
                    line: *line,
                    file: file.clone(),
                }))
            }
            Literal::Regex { pattern, flags } => {
                use crate::vm::value::RegexValue;
                match RegexValue::new(pattern, flags) {
                    Ok(regex) => Ok(Value::Regex(regex)),
                    Err(e) => Err(VmError::TypeError(format!("Invalid regex: {}", e))),
                }
            }
            Literal::Enum { type_name, value } => Ok(Value::Enum {
                type_name: type_name.clone(),
                value: *value,
            }),
        }
    }
}
