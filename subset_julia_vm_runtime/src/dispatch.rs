//! Dynamic dispatch for AoT runtime
//!
//! This module provides dynamic dispatch support for cases where
//! type information is not available at compile time.

// SAFETY: i64→usize cast in string repeat is always non-negative from caller context;
// i64→u32 cast in pow is guarded by `if *b >= 0` check.
#![allow(clippy::cast_sign_loss)]

use crate::error::{RuntimeError, RuntimeResult};
use crate::value::Value;

/// Binary operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    IntDiv,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl BinOp {
    /// Get the Julia operator string
    pub fn as_str(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Pow => "^",
            BinOp::IntDiv => "÷",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "⊻",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }
}

/// Perform dynamic binary operation
///
/// Dispatches based on the runtime types of the operands.
pub fn dynamic_binop(op: BinOp, lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match op {
        BinOp::Add => dynamic_add(lhs, rhs),
        BinOp::Sub => dynamic_sub(lhs, rhs),
        BinOp::Mul => dynamic_mul(lhs, rhs),
        BinOp::Div => dynamic_div(lhs, rhs),
        BinOp::Mod => dynamic_mod(lhs, rhs),
        BinOp::Pow => dynamic_pow(lhs, rhs),
        BinOp::IntDiv => dynamic_intdiv(lhs, rhs),
        BinOp::Lt => dynamic_lt(lhs, rhs),
        BinOp::Gt => dynamic_gt(lhs, rhs),
        BinOp::Le => dynamic_le(lhs, rhs),
        BinOp::Ge => dynamic_ge(lhs, rhs),
        BinOp::Eq => dynamic_eq(lhs, rhs),
        BinOp::Ne => dynamic_ne(lhs, rhs),
        BinOp::And => dynamic_and(lhs, rhs),
        BinOp::Or => dynamic_or(lhs, rhs),
        BinOp::BitAnd => dynamic_bitand(lhs, rhs),
        BinOp::BitOr => dynamic_bitor(lhs, rhs),
        BinOp::BitXor => dynamic_bitxor(lhs, rhs),
        BinOp::Shl => dynamic_shl(lhs, rhs),
        BinOp::Shr => dynamic_shr(lhs, rhs),
    }
}

/// Dynamic addition
fn dynamic_add(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a + b)),
        (Value::I32(a), Value::I32(b)) => Ok(Value::I32(a + b)),
        (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a + b)),
        (Value::F32(a), Value::F32(b)) => Ok(Value::F32(a + b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::F64(*a as f64 + b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::F64(a + *b as f64)),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
        _ => Err(RuntimeError::method_error(format!(
            "+({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

/// Dynamic subtraction
fn dynamic_sub(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a - b)),
        (Value::I32(a), Value::I32(b)) => Ok(Value::I32(a - b)),
        (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a - b)),
        (Value::F32(a), Value::F32(b)) => Ok(Value::F32(a - b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::F64(*a as f64 - b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::F64(a - *b as f64)),
        _ => Err(RuntimeError::method_error(format!(
            "-({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

/// Dynamic multiplication
fn dynamic_mul(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a * b)),
        (Value::I32(a), Value::I32(b)) => Ok(Value::I32(a * b)),
        (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a * b)),
        (Value::F32(a), Value::F32(b)) => Ok(Value::F32(a * b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::F64(*a as f64 * b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::F64(a * *b as f64)),
        (Value::Str(s), Value::I64(n)) | (Value::I64(n), Value::Str(s)) => {
            Ok(Value::Str(s.repeat(*n as usize)))
        }
        _ => Err(RuntimeError::method_error(format!(
            "*({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

/// Dynamic division
fn dynamic_div(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero)
            } else {
                Ok(Value::F64(*a as f64 / *b as f64))
            }
        }
        (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a / b)),
        (Value::F32(a), Value::F32(b)) => Ok(Value::F32(a / b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::F64(*a as f64 / b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::F64(a / *b as f64)),
        _ => Err(RuntimeError::method_error(format!(
            "/({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

/// Dynamic modulo
fn dynamic_mod(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero)
            } else {
                Ok(Value::I64(a % b))
            }
        }
        (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a % b)),
        _ => Err(RuntimeError::method_error(format!(
            "%({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

/// Dynamic power
fn dynamic_pow(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => {
            if *b >= 0 {
                Ok(Value::I64(a.pow(*b as u32)))
            } else {
                Ok(Value::F64((*a as f64).powi(*b as i32)))
            }
        }
        (Value::F64(a), Value::F64(b)) => Ok(Value::F64(a.powf(*b))),
        (Value::F64(a), Value::I64(b)) => Ok(Value::F64(a.powi(*b as i32))),
        (Value::I64(a), Value::F64(b)) => Ok(Value::F64((*a as f64).powf(*b))),
        _ => Err(RuntimeError::method_error(format!(
            "^({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

/// Dynamic integer division
fn dynamic_intdiv(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => {
            if *b == 0 {
                Err(RuntimeError::DivisionByZero)
            } else {
                Ok(Value::I64(a / b))
            }
        }
        _ => Err(RuntimeError::method_error(format!(
            "÷({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

// ========== Comparison operations ==========

fn dynamic_lt(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::Bool(a < b)),
        (Value::F64(a), Value::F64(b)) => Ok(Value::Bool(a < b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::Bool((*a as f64) < *b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::Bool(*a < (*b as f64))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a < b)),
        _ => Err(RuntimeError::method_error(format!(
            "<({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_gt(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::Bool(a > b)),
        (Value::F64(a), Value::F64(b)) => Ok(Value::Bool(a > b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::Bool((*a as f64) > *b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::Bool(*a > (*b as f64))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a > b)),
        _ => Err(RuntimeError::method_error(format!(
            ">({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_le(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::Bool(a <= b)),
        (Value::F64(a), Value::F64(b)) => Ok(Value::Bool(a <= b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::Bool((*a as f64) <= *b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::Bool(*a <= (*b as f64))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a <= b)),
        _ => Err(RuntimeError::method_error(format!(
            "<=({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_ge(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::Bool(a >= b)),
        (Value::F64(a), Value::F64(b)) => Ok(Value::Bool(a >= b)),
        (Value::I64(a), Value::F64(b)) => Ok(Value::Bool((*a as f64) >= *b)),
        (Value::F64(a), Value::I64(b)) => Ok(Value::Bool(*a >= (*b as f64))),
        (Value::Str(a), Value::Str(b)) => Ok(Value::Bool(a >= b)),
        _ => Err(RuntimeError::method_error(format!(
            ">=({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_eq(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    Ok(Value::Bool(lhs == rhs))
}

fn dynamic_ne(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    Ok(Value::Bool(lhs != rhs))
}

// ========== Logical operations ==========

fn dynamic_and(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a && *b)),
        _ => Err(RuntimeError::method_error(format!(
            "&&({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_or(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a || *b)),
        _ => Err(RuntimeError::method_error(format!(
            "||({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

// ========== Bitwise operations ==========

fn dynamic_bitand(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a & b)),
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a & *b)),
        _ => Err(RuntimeError::method_error(format!(
            "&({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_bitor(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a | b)),
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a | *b)),
        _ => Err(RuntimeError::method_error(format!(
            "|({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_bitxor(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a ^ b)),
        (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(*a ^ *b)),
        _ => Err(RuntimeError::method_error(format!(
            "⊻({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_shl(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a << b)),
        _ => Err(RuntimeError::method_error(format!(
            "<<({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

fn dynamic_shr(lhs: &Value, rhs: &Value) -> RuntimeResult<Value> {
    match (lhs, rhs) {
        (Value::I64(a), Value::I64(b)) => Ok(Value::I64(a >> b)),
        _ => Err(RuntimeError::method_error(format!(
            ">>({}:::{}, {}:::{})",
            lhs,
            lhs.type_name(),
            rhs,
            rhs.type_name()
        ))),
    }
}

// ========== Dynamic function call ==========

/// Perform a dynamic function call
///
/// This is a placeholder for the full dispatch table implementation.
pub fn dynamic_call(name: &str, args: &[Value]) -> RuntimeResult<Value> {
    // This will be expanded in later phases to support full multiple dispatch
    Err(RuntimeError::method_error(format!(
        "{}({})",
        name,
        args.iter()
            .map(|a| a.type_name())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dynamic_add() {
        let result = dynamic_binop(BinOp::Add, &Value::I64(10), &Value::I64(20)).unwrap();
        assert_eq!(result, Value::I64(30));

        let result = dynamic_binop(BinOp::Add, &Value::F64(1.5), &Value::F64(2.5)).unwrap();
        assert_eq!(result, Value::F64(4.0));

        let result = dynamic_binop(BinOp::Add, &Value::I64(1), &Value::F64(2.5)).unwrap();
        assert_eq!(result, Value::F64(3.5));
    }

    #[test]
    fn test_dynamic_comparison() {
        let result = dynamic_binop(BinOp::Lt, &Value::I64(5), &Value::I64(10)).unwrap();
        assert_eq!(result, Value::Bool(true));

        let result = dynamic_binop(BinOp::Eq, &Value::I64(5), &Value::I64(5)).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_dynamic_string_ops() {
        let result = dynamic_binop(
            BinOp::Add,
            &Value::Str("Hello".to_string()),
            &Value::Str(" World".to_string()),
        )
        .unwrap();
        assert_eq!(result, Value::Str("Hello World".to_string()));

        let result =
            dynamic_binop(BinOp::Mul, &Value::Str("ab".to_string()), &Value::I64(3)).unwrap();
        assert_eq!(result, Value::Str("ababab".to_string()));
    }
}
