//! Expression evaluation for the macro system.
//!
//! Handles eval() builtin: evaluates Expr AST nodes at runtime.

// SAFETY: i64→u32 cast in integer exponentiation is guarded by `if *exp >= 0` check.
#![allow(clippy::cast_sign_loss)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::value::{ExprValue, Value};
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Evaluate an Expr value at runtime (for eval() builtin)
    pub(super) fn eval_expr_value(&mut self, val: &Value) -> Result<Value, VmError> {
        match val {
            // Literals evaluate to themselves
            Value::I64(n) => Ok(Value::I64(*n)),
            Value::I32(n) => Ok(Value::I32(*n)),
            Value::I16(n) => Ok(Value::I16(*n)),
            Value::I8(n) => Ok(Value::I8(*n)),
            Value::I128(n) => Ok(Value::I128(*n)),
            Value::U64(n) => Ok(Value::U64(*n)),
            Value::U32(n) => Ok(Value::U32(*n)),
            Value::U16(n) => Ok(Value::U16(*n)),
            Value::U8(n) => Ok(Value::U8(*n)),
            Value::U128(n) => Ok(Value::U128(*n)),
            Value::F64(n) => Ok(Value::F64(*n)),
            Value::F32(n) => Ok(Value::F32(*n)),
            Value::Bool(b) => Ok(Value::Bool(*b)),
            Value::Str(s) => Ok(Value::Str(s.clone())),
            Value::Char(c) => Ok(Value::Char(*c)),
            Value::Nothing => Ok(Value::Nothing),

            // QuoteNode: unwrap and return the inner value
            Value::QuoteNode(inner) => Ok((**inner).clone()),

            // Symbol: look up variable value if it exists, otherwise return as-is
            Value::Symbol(s) => {
                // Try to resolve the symbol to a variable value
                if let Some(val) = self.get_variable_value(s.as_str()) {
                    Ok(val)
                } else {
                    // Return as-is if not a known variable (might be a function name)
                    Ok(Value::Symbol(s.clone()))
                }
            }

            // Expr: evaluate based on head
            Value::Expr(expr) => self.eval_expr_ast(expr),

            // Fallback: non-Expr/Symbol values returned as-is in eval
            other => Ok(other.clone()),
        }
    }

    /// Evaluate an Expr AST node
    fn eval_expr_ast(&mut self, expr: &ExprValue) -> Result<Value, VmError> {
        let head = expr.head.as_str();

        match head {
            "call" => {
                // Expr(:call, :func, args...)
                if expr.args.is_empty() {
                    return Err(VmError::TypeError(
                        "call expression requires function name".to_string(),
                    ));
                }

                // First arg is the function (as Symbol)
                let func_sym = match &expr.args[0] {
                    Value::Symbol(s) => s.as_str().to_string(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "call expression function must be Symbol, got {:?}",
                            other.value_type()
                        )))
                    }
                };

                // Remaining args are the function arguments
                let mut eval_args = Vec::new();
                for arg in &expr.args[1..] {
                    eval_args.push(self.eval_expr_value(arg)?);
                }

                // Apply the function
                self.eval_call(&func_sym, eval_args)
            }

            // Block: evaluate statements in sequence, return last
            "block" => {
                let mut result = Value::Nothing;
                for arg in &expr.args {
                    // Skip LineNumberNode
                    if matches!(arg, Value::LineNumberNode(_)) {
                        continue;
                    }
                    result = self.eval_expr_value(arg)?;
                }
                Ok(result)
            }

            // Comparison: ==, !=, <, >, <=, >=
            "comparison" => {
                if expr.args.len() < 3 {
                    return Err(VmError::TypeError(
                        "comparison requires at least 3 args".to_string(),
                    ));
                }
                let left = self.eval_expr_value(&expr.args[0])?;
                let op = match &expr.args[1] {
                    Value::Symbol(s) => s.as_str().to_string(),
                    _ => {
                        return Err(VmError::TypeError(
                            "comparison operator must be Symbol".to_string(),
                        ))
                    }
                };
                let right = self.eval_expr_value(&expr.args[2])?;
                self.eval_comparison(&op, left, right)
            }

            // && and ||
            "&&" => {
                if expr.args.len() != 2 {
                    return Err(VmError::TypeError("&& requires 2 args".to_string()));
                }
                let left = self.eval_expr_value(&expr.args[0])?;
                if let Value::Bool(false) = left {
                    return Ok(Value::Bool(false));
                }
                self.eval_expr_value(&expr.args[1])
            }

            "||" => {
                if expr.args.len() != 2 {
                    return Err(VmError::TypeError("|| requires 2 args".to_string()));
                }
                let left = self.eval_expr_value(&expr.args[0])?;
                if let Value::Bool(true) = left {
                    return Ok(Value::Bool(true));
                }
                self.eval_expr_value(&expr.args[1])
            }

            // Assignment: x = expr
            "=" => {
                if expr.args.len() != 2 {
                    return Err(VmError::TypeError(
                        "assignment requires exactly 2 args".to_string(),
                    ));
                }

                // First arg is the variable name (as Symbol)
                let var_name = match &expr.args[0] {
                    Value::Symbol(s) => s.as_str().to_string(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "assignment target must be Symbol, got {:?}",
                            other.value_type()
                        )))
                    }
                };

                // Second arg is the value to assign
                let value = self.eval_expr_value(&expr.args[1])?;

                // Store the value in the current frame
                self.set_variable_value(&var_name, value.clone());

                // Return the assigned value (Julia semantics)
                Ok(value)
            }

            _ => Err(VmError::NotImplemented(format!(
                "eval: unsupported Expr head '{}'",
                head
            ))),
        }
    }

    /// Evaluate a function call from eval
    fn eval_call(&mut self, func: &str, args: Vec<Value>) -> Result<Value, VmError> {
        match func {
            // Arithmetic
            "+" => self.eval_binary_arith(&args, |a, b| a + b, |a, b| a + b),
            "-" => {
                if args.len() == 1 {
                    // Unary minus
                    match &args[0] {
                        Value::I64(n) => Ok(Value::I64(-n)),
                        Value::F64(n) => Ok(Value::F64(-n)),
                        _ => Err(VmError::TypeError(
                            "unary - requires numeric argument".to_string(),
                        )),
                    }
                } else {
                    self.eval_binary_arith(&args, |a, b| a - b, |a, b| a - b)
                }
            }
            "*" => self.eval_binary_arith(&args, |a, b| a * b, |a, b| a * b),
            "/" => {
                // Division always returns Float64 in Julia
                if args.len() != 2 {
                    return Err(VmError::TypeError("/ requires 2 arguments".to_string()));
                }
                let a = self.to_f64(&args[0])?;
                let b = self.to_f64(&args[1])?;
                Ok(Value::F64(a / b))
            }
            "÷" | "div" => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("div requires 2 arguments".to_string()));
                }
                match (&args[0], &args[1]) {
                    (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a / b)),
                    _ => Err(VmError::TypeError(
                        "div requires integer arguments".to_string(),
                    )),
                }
            }
            "%" | "mod" => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("mod requires 2 arguments".to_string()));
                }
                match (&args[0], &args[1]) {
                    (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a % b)),
                    (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a % b)),
                    _ => Err(VmError::TypeError(
                        "mod requires numeric arguments".to_string(),
                    )),
                }
            }
            "^" => {
                if args.len() != 2 {
                    return Err(VmError::TypeError("^ requires 2 arguments".to_string()));
                }
                match (&args[0], &args[1]) {
                    (Value::I64(base), Value::I64(exp)) => {
                        if *exp >= 0 {
                            Ok(Value::I64(base.pow(*exp as u32)))
                        } else {
                            Ok(Value::F64((*base as f64).powi(*exp as i32)))
                        }
                    }
                    (Value::F64(base), Value::I64(exp)) => Ok(Value::F64(base.powi(*exp as i32))),
                    (Value::F64(base), Value::F64(exp)) => Ok(Value::F64(base.powf(*exp))),
                    (Value::I64(base), Value::F64(exp)) => {
                        Ok(Value::F64((*base as f64).powf(*exp)))
                    }
                    _ => Err(VmError::TypeError(
                        "^ requires numeric arguments".to_string(),
                    )),
                }
            }

            // Comparison
            "==" => self.eval_comparison("==", args[0].clone(), args[1].clone()),
            "!=" => self.eval_comparison("!=", args[0].clone(), args[1].clone()),
            "<" => self.eval_comparison("<", args[0].clone(), args[1].clone()),
            ">" => self.eval_comparison(">", args[0].clone(), args[1].clone()),
            "<=" => self.eval_comparison("<=", args[0].clone(), args[1].clone()),
            ">=" => self.eval_comparison(">=", args[0].clone(), args[1].clone()),

            // Math functions
            "sqrt" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("sqrt requires 1 argument".to_string()));
                }
                let x = self.to_f64(&args[0])?;
                Ok(Value::F64(x.sqrt()))
            }
            "abs" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("abs requires 1 argument".to_string()));
                }
                match &args[0] {
                    Value::I64(n) => Ok(Value::I64(n.abs())),
                    Value::F64(n) => Ok(Value::F64(n.abs())),
                    _ => Err(VmError::TypeError(
                        "abs requires numeric argument".to_string(),
                    )),
                }
            }
            "sin" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("sin requires 1 argument".to_string()));
                }
                let x = self.to_f64(&args[0])?;
                Ok(Value::F64(x.sin()))
            }
            "cos" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("cos requires 1 argument".to_string()));
                }
                let x = self.to_f64(&args[0])?;
                Ok(Value::F64(x.cos()))
            }

            // Boolean (handle both "!" and escaped "\!")
            "!" | "\\!" => {
                if args.len() != 1 {
                    return Err(VmError::TypeError("! requires 1 argument".to_string()));
                }
                match &args[0] {
                    Value::Bool(b) => Ok(Value::Bool(!b)),
                    _ => Err(VmError::TypeError(
                        "! requires boolean argument".to_string(),
                    )),
                }
            }

            _ => Err(VmError::NotImplemented(format!(
                "eval: unsupported function '{}'",
                func
            ))),
        }
    }

    /// Helper for binary arithmetic operations
    fn eval_binary_arith(
        &self,
        args: &[Value],
        int_op: fn(i64, i64) -> i64,
        float_op: fn(f64, f64) -> f64,
    ) -> Result<Value, VmError> {
        if args.len() != 2 {
            return Err(VmError::TypeError(
                "binary operation requires 2 arguments".to_string(),
            ));
        }
        match (&args[0], &args[1]) {
            (Value::I64(a), Value::I64(b)) => Ok(Value::I64(int_op(*a, *b))),
            (Value::F64(a), Value::F64(b)) => Ok(Value::F64(float_op(*a, *b))),
            (Value::I64(a), Value::F64(b)) => Ok(Value::F64(float_op(*a as f64, *b))),
            (Value::F64(a), Value::I64(b)) => Ok(Value::F64(float_op(*a, *b as f64))),
            _ => Err(VmError::TypeError(
                "arithmetic requires numeric arguments".to_string(),
            )),
        }
    }

    /// Helper for comparison operations
    pub(super) fn eval_comparison(
        &self,
        op: &str,
        left: Value,
        right: Value,
    ) -> Result<Value, VmError> {
        let result = match (left, right) {
            (Value::I64(a), Value::I64(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::F64(a), Value::F64(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::I64(a), Value::F64(b)) => {
                let a = a as f64;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::F64(a), Value::I64(b)) => {
                let b = b as f64;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            // Int128 comparisons
            (Value::I128(a), Value::I128(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::I128(a), Value::I64(b)) => {
                let b = b as i128;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::I64(a), Value::I128(b)) => {
                let a = a as i128;
                match op {
                    "==" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    ">=" => a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            // BigInt comparisons
            (Value::BigInt(ref a), Value::BigInt(ref b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            (Value::BigInt(ref a), Value::I64(b)) => {
                let b = num_bigint::BigInt::from(b);
                match op {
                    "==" => *a == b,
                    "!=" => *a != b,
                    "<" => *a < b,
                    ">" => *a > b,
                    "<=" => *a <= b,
                    ">=" => *a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::I64(a), Value::BigInt(ref b)) => {
                let a = num_bigint::BigInt::from(a);
                match op {
                    "==" => a == *b,
                    "!=" => a != *b,
                    "<" => a < *b,
                    ">" => a > *b,
                    "<=" => a <= *b,
                    ">=" => a >= *b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::BigInt(ref a), Value::I128(b)) => {
                let b = num_bigint::BigInt::from(b);
                match op {
                    "==" => *a == b,
                    "!=" => *a != b,
                    "<" => *a < b,
                    ">" => *a > b,
                    "<=" => *a <= b,
                    ">=" => *a >= b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::I128(a), Value::BigInt(ref b)) => {
                let a = num_bigint::BigInt::from(a);
                match op {
                    "==" => a == *b,
                    "!=" => a != *b,
                    "<" => a < *b,
                    ">" => a > *b,
                    "<=" => a <= *b,
                    ">=" => a >= *b,
                    _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
                }
            }
            (Value::Bool(a), Value::Bool(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                _ => {
                    return Err(VmError::TypeError(
                        "comparison not supported for Bool".to_string(),
                    ))
                }
            },
            (Value::Str(a), Value::Str(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                "<" => a < b,
                ">" => a > b,
                "<=" => a <= b,
                ">=" => a >= b,
                _ => return Err(VmError::TypeError(format!("unknown comparison op: {}", op))),
            },
            // DataType (Type) comparison - uses identity semantics like Julia
            (Value::DataType(a), Value::DataType(b)) => match op {
                "==" => a == b,
                "!=" => a != b,
                _ => {
                    return Err(VmError::TypeError(format!(
                        "comparison op {} not supported for DataType",
                        op
                    )))
                }
            },
            _ => return Err(VmError::TypeError("comparison type mismatch".to_string())),
        };
        Ok(Value::Bool(result))
    }

    /// Convert Value to f64 for math operations
    pub(super) fn to_f64(&self, val: &Value) -> Result<f64, VmError> {
        match val {
            Value::I64(n) => Ok(*n as f64),
            Value::F64(n) => Ok(*n),
            Value::I32(n) => Ok(*n as f64),
            Value::F32(n) => Ok(*n as f64),
            _ => Err(VmError::TypeError("expected numeric value".to_string())),
        }
    }
}
