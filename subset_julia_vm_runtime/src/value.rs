//! Dynamic Value type for AoT runtime
//!
//! This module provides the `Value` enum used for dynamic typing
//! in cases where static type information is not available.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Dynamic value type for AoT compiled code
///
/// Used when type information cannot be statically determined,
/// requiring runtime dispatch.
#[derive(Debug, Clone)]
pub enum Value {
    // ========== Primitive Types ==========
    /// 64-bit signed integer
    I64(i64),
    /// 32-bit signed integer
    I32(i32),
    /// 64-bit floating point
    F64(f64),
    /// 32-bit floating point
    F32(f32),
    /// Boolean
    Bool(bool),
    /// Character
    Char(char),
    /// Nothing (unit type, like void)
    Nothing,
    /// Missing value
    Missing,

    // ========== Heap-Allocated Types ==========
    /// String
    Str(String),
    /// Array (dynamically typed elements)
    Array(Rc<RefCell<Vec<Value>>>),
    /// Tuple
    Tuple(Vec<Value>),
    /// Dictionary
    Dict(Rc<RefCell<HashMap<String, Value>>>),

    // ========== Struct ==========
    /// User-defined struct
    Struct {
        /// Type name
        type_name: String,
        /// Field values
        fields: Vec<Value>,
    },

    // ========== Range ==========
    /// Integer range (start, stop, step)
    RangeI64 { start: i64, stop: i64, step: i64 },
}

impl Value {
    /// Get the Julia type name of this value
    pub fn type_name(&self) -> &str {
        match self {
            Value::I64(_) => "Int64",
            Value::I32(_) => "Int32",
            Value::F64(_) => "Float64",
            Value::F32(_) => "Float32",
            Value::Bool(_) => "Bool",
            Value::Char(_) => "Char",
            Value::Nothing => "Nothing",
            Value::Missing => "Missing",
            Value::Str(_) => "String",
            Value::Array(_) => "Array",
            Value::Tuple(_) => "Tuple",
            Value::Dict(_) => "Dict",
            Value::Struct { type_name, .. } => type_name,
            Value::RangeI64 { .. } => "UnitRange{Int64}",
        }
    }

    /// Check if this value is nothing
    pub fn is_nothing(&self) -> bool {
        matches!(self, Value::Nothing)
    }

    /// Check if this value is missing
    pub fn is_missing(&self) -> bool {
        matches!(self, Value::Missing)
    }

    /// Try to extract as i64
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I64(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract as i32
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Value::I32(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract as f64
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F64(v) => Some(*v),
            Value::I64(v) => Some(*v as f64),
            Value::I32(v) => Some(*v as f64),
            Value::F32(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Try to extract as f32
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract as char
    pub fn as_char(&self) -> Option<char> {
        match self {
            Value::Char(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract as string reference
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Check if this is a numeric type
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Value::I64(_) | Value::I32(_) | Value::F64(_) | Value::F32(_)
        )
    }

    /// Check if this is an integer type
    pub fn is_integer(&self) -> bool {
        matches!(self, Value::I64(_) | Value::I32(_))
    }

    /// Check if this is a float type
    pub fn is_float(&self) -> bool {
        matches!(self, Value::F64(_) | Value::F32(_))
    }
}

// ========== From implementations ==========

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::I64(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::I32(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::F64(v)
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::F32(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<char> for Value {
    fn from(v: char) -> Self {
        Value::Char(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        let values: Vec<Value> = v.into_iter().map(|x| x.into()).collect();
        Value::Array(Rc::new(RefCell::new(values)))
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Nothing
    }
}

// ========== Display implementation ==========

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I64(v) => write!(f, "{}", v),
            Value::I32(v) => write!(f, "{}", v),
            Value::F64(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    write!(f, "{}.0", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            Value::F32(v) => write!(f, "{}f0", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::Char(v) => write!(f, "'{}'", v),
            Value::Nothing => write!(f, "nothing"),
            Value::Missing => write!(f, "missing"),
            Value::Str(s) => write!(f, "\"{}\"", s),
            Value::Array(arr) => {
                let arr = arr.borrow();
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            Value::Tuple(elements) => {
                write!(f, "(")?;
                for (i, v) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                if elements.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Value::Dict(dict) => {
                let dict = dict.borrow();
                write!(f, "Dict(")?;
                for (i, (k, v)) in dict.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\" => {}", k, v)?;
                }
                write!(f, ")")
            }
            Value::Struct { type_name, fields } => {
                write!(f, "{}(", type_name)?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", field)?;
                }
                write!(f, ")")
            }
            Value::RangeI64 { start, stop, step } => {
                if *step == 1 {
                    write!(f, "{}:{}", start, stop)
                } else {
                    write!(f, "{}:{}:{}", start, step, stop)
                }
            }
        }
    }
}

// ========== PartialEq implementation ==========

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::I32(a), Value::I32(b)) => a == b,
            (Value::F64(a), Value::F64(b)) => a == b,
            (Value::F32(a), Value::F32(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Nothing, Value::Nothing) => true,
            (Value::Missing, Value::Missing) => true,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (
                Value::RangeI64 {
                    start: s1,
                    stop: e1,
                    step: st1,
                },
                Value::RangeI64 {
                    start: s2,
                    stop: e2,
                    step: st2,
                },
            ) => s1 == s2 && e1 == e2 && st1 == st2,
            // Arrays and Dicts are compared by reference
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_from_primitives() {
        assert!(matches!(Value::from(42i64), Value::I64(42)));
        assert!(matches!(Value::from(3.125f64), Value::F64(_)));
        assert!(matches!(Value::from(true), Value::Bool(true)));
        assert!(matches!(Value::from("hello"), Value::Str(_)));
    }

    #[test]
    fn test_value_type_name() {
        assert_eq!(Value::I64(42).type_name(), "Int64");
        assert_eq!(Value::F64(3.125).type_name(), "Float64");
        assert_eq!(Value::Bool(true).type_name(), "Bool");
        assert_eq!(Value::Nothing.type_name(), "Nothing");
    }

    #[test]
    fn test_value_as_methods() {
        assert_eq!(Value::I64(42).as_i64(), Some(42));
        assert_eq!(Value::F64(3.125).as_f64(), Some(3.125));
        assert_eq!(Value::I64(42).as_f64(), Some(42.0));
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
    }

    #[test]
    fn test_value_display() {
        assert_eq!(format!("{}", Value::I64(42)), "42");
        assert_eq!(format!("{}", Value::F64(3.0)), "3.0");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Nothing), "nothing");
    }
}
