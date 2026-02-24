//! Type conversion utilities for AoT runtime
//!
//! This module provides type conversion functions between Julia types.

// SAFETY: i64→u32 cast is guarded by `if *v < 0 || *v > 0x10FFFF` check above.
#![allow(clippy::cast_sign_loss)]

use crate::error::{RuntimeError, RuntimeResult};
use crate::value::Value;

/// Convert a Value to i64
pub fn to_i64(value: &Value) -> RuntimeResult<i64> {
    match value {
        Value::I64(v) => Ok(*v),
        Value::I32(v) => Ok(*v as i64),
        Value::F64(v) => {
            if v.fract() != 0.0 {
                Err(RuntimeError::inexact_error(format!(
                    "cannot convert {} to Int64",
                    v
                )))
            } else {
                Ok(*v as i64)
            }
        }
        Value::F32(v) => {
            if v.fract() != 0.0 {
                Err(RuntimeError::inexact_error(format!(
                    "cannot convert {} to Int64",
                    v
                )))
            } else {
                Ok(*v as i64)
            }
        }
        Value::Bool(v) => Ok(if *v { 1 } else { 0 }),
        Value::Char(c) => Ok(*c as i64),
        _ => Err(RuntimeError::type_error(format!(
            "cannot convert {} to Int64",
            value.type_name()
        ))),
    }
}

/// Convert a Value to i32
pub fn to_i32(value: &Value) -> RuntimeResult<i32> {
    match value {
        Value::I32(v) => Ok(*v),
        Value::I64(v) => {
            if *v < i32::MIN as i64 || *v > i32::MAX as i64 {
                Err(RuntimeError::overflow_error(format!(
                    "{} is out of range for Int32",
                    v
                )))
            } else {
                Ok(*v as i32)
            }
        }
        Value::F64(v) => {
            if v.fract() != 0.0 {
                Err(RuntimeError::inexact_error(format!(
                    "cannot convert {} to Int32",
                    v
                )))
            } else {
                Ok(*v as i32)
            }
        }
        Value::Bool(v) => Ok(if *v { 1 } else { 0 }),
        _ => Err(RuntimeError::type_error(format!(
            "cannot convert {} to Int32",
            value.type_name()
        ))),
    }
}

/// Convert a Value to f64
pub fn to_f64(value: &Value) -> RuntimeResult<f64> {
    match value {
        Value::F64(v) => Ok(*v),
        Value::F32(v) => Ok(*v as f64),
        Value::I64(v) => Ok(*v as f64),
        Value::I32(v) => Ok(*v as f64),
        Value::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        _ => Err(RuntimeError::type_error(format!(
            "cannot convert {} to Float64",
            value.type_name()
        ))),
    }
}

/// Convert a Value to f32
pub fn to_f32(value: &Value) -> RuntimeResult<f32> {
    match value {
        Value::F32(v) => Ok(*v),
        Value::F64(v) => Ok(*v as f32),
        Value::I64(v) => Ok(*v as f32),
        Value::I32(v) => Ok(*v as f32),
        Value::Bool(v) => Ok(if *v { 1.0 } else { 0.0 }),
        _ => Err(RuntimeError::type_error(format!(
            "cannot convert {} to Float32",
            value.type_name()
        ))),
    }
}

/// Convert a Value to bool
pub fn to_bool(value: &Value) -> RuntimeResult<bool> {
    match value {
        Value::Bool(v) => Ok(*v),
        Value::I64(v) => {
            if *v == 0 {
                Ok(false)
            } else if *v == 1 {
                Ok(true)
            } else {
                Err(RuntimeError::type_error(format!(
                    "cannot convert {} to Bool",
                    v
                )))
            }
        }
        _ => Err(RuntimeError::type_error(format!(
            "cannot convert {} to Bool",
            value.type_name()
        ))),
    }
}

/// Convert a Value to String
pub fn to_string(value: &Value) -> String {
    format!("{}", value)
}

/// Convert a Value to char
pub fn to_char(value: &Value) -> RuntimeResult<char> {
    match value {
        Value::Char(c) => Ok(*c),
        Value::I64(v) => {
            if *v < 0 || *v > 0x10FFFF {
                Err(RuntimeError::argument_error(format!(
                    "{} is not a valid Unicode code point",
                    v
                )))
            } else {
                char::from_u32(*v as u32).ok_or_else(|| {
                    RuntimeError::argument_error(format!("{} is not a valid Unicode code point", v))
                })
            }
        }
        Value::Str(s) => {
            if s.len() == 1 {
                Ok(s.chars().next().unwrap())
            } else {
                Err(RuntimeError::argument_error(
                    "string must have exactly one character",
                ))
            }
        }
        _ => Err(RuntimeError::type_error(format!(
            "cannot convert {} to Char",
            value.type_name()
        ))),
    }
}

/// Promote two numeric types to a common type
pub fn promote_numeric(a: &Value, b: &Value) -> RuntimeResult<(Value, Value)> {
    match (a, b) {
        // Same types - no promotion needed
        (Value::I64(_), Value::I64(_)) => Ok((a.clone(), b.clone())),
        (Value::I32(_), Value::I32(_)) => Ok((a.clone(), b.clone())),
        (Value::F64(_), Value::F64(_)) => Ok((a.clone(), b.clone())),
        (Value::F32(_), Value::F32(_)) => Ok((a.clone(), b.clone())),

        // Promote to f64
        (Value::I64(x), Value::F64(y)) => Ok((Value::F64(*x as f64), Value::F64(*y))),
        (Value::F64(x), Value::I64(y)) => Ok((Value::F64(*x), Value::F64(*y as f64))),
        (Value::I32(x), Value::F64(y)) => Ok((Value::F64(*x as f64), Value::F64(*y))),
        (Value::F64(x), Value::I32(y)) => Ok((Value::F64(*x), Value::F64(*y as f64))),
        (Value::F32(x), Value::F64(y)) => Ok((Value::F64(*x as f64), Value::F64(*y))),
        (Value::F64(x), Value::F32(y)) => Ok((Value::F64(*x), Value::F64(*y as f64))),

        // Promote i32 to i64
        (Value::I32(x), Value::I64(y)) => Ok((Value::I64(*x as i64), Value::I64(*y))),
        (Value::I64(x), Value::I32(y)) => Ok((Value::I64(*x), Value::I64(*y as i64))),

        // Promote to f32
        (Value::I64(x), Value::F32(y)) => Ok((Value::F32(*x as f32), Value::F32(*y))),
        (Value::F32(x), Value::I64(y)) => Ok((Value::F32(*x), Value::F32(*y as f32))),
        (Value::I32(x), Value::F32(y)) => Ok((Value::F32(*x as f32), Value::F32(*y))),
        (Value::F32(x), Value::I32(y)) => Ok((Value::F32(*x), Value::F32(*y as f32))),

        _ => Err(RuntimeError::type_error(format!(
            "cannot promote {} and {} to common type",
            a.type_name(),
            b.type_name()
        ))),
    }
}

/// Widen a numeric type
pub fn widen(value: &Value) -> Value {
    match value {
        Value::I32(v) => Value::I64(*v as i64),
        Value::F32(v) => Value::F64(*v as f64),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_i64() {
        assert_eq!(to_i64(&Value::I64(42)).unwrap(), 42);
        assert_eq!(to_i64(&Value::F64(3.0)).unwrap(), 3);
        assert!(to_i64(&Value::F64(3.5)).is_err());
        assert_eq!(to_i64(&Value::Bool(true)).unwrap(), 1);
    }

    #[test]
    fn test_to_f64() {
        assert_eq!(to_f64(&Value::F64(3.125)).unwrap(), 3.125);
        assert_eq!(to_f64(&Value::I64(42)).unwrap(), 42.0);
        assert_eq!(to_f64(&Value::Bool(false)).unwrap(), 0.0);
    }

    #[test]
    fn test_promote_numeric() {
        let (a, b) = promote_numeric(&Value::I64(1), &Value::F64(2.0)).unwrap();
        assert!(matches!(a, Value::F64(_)));
        assert!(matches!(b, Value::F64(_)));
    }

    #[test]
    fn test_widen() {
        let v = widen(&Value::I32(42));
        assert!(matches!(v, Value::I64(42)));

        let v = widen(&Value::F32(3.125));
        assert!(matches!(v, Value::F64(_)));
    }
}
