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
    /// 16-bit signed integer (Issue #10131)
    I16(i16),
    /// 8-bit signed integer (Issue #10131)
    I8(i8),
    /// 128-bit signed integer (Issue #10131)
    I128(i128),
    /// 64-bit unsigned integer (Issue #10131)
    U64(u64),
    /// 32-bit unsigned integer (Issue #10131)
    U32(u32),
    /// 16-bit unsigned integer (Issue #10131)
    U16(u16),
    /// 8-bit unsigned integer (Issue #10131)
    U8(u8),
    /// 128-bit unsigned integer (Issue #10131)
    U128(u128),
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
    /// Julia DataType/type object represented by its display name
    DataType(String),

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
            Value::I16(_) => "Int16",
            Value::I8(_) => "Int8",
            Value::I128(_) => "Int128",
            Value::U64(_) => "UInt64",
            Value::U32(_) => "UInt32",
            Value::U16(_) => "UInt16",
            Value::U8(_) => "UInt8",
            Value::U128(_) => "UInt128",
            Value::F64(_) => "Float64",
            Value::F32(_) => "Float32",
            Value::Bool(_) => "Bool",
            Value::Char(_) => "Char",
            Value::Nothing => "Nothing",
            Value::Missing => "Missing",
            Value::DataType(_) => "DataType",
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
            Value::I16(v) => Some(f64::from(*v)),
            Value::I8(v) => Some(f64::from(*v)),
            Value::I128(v) => Some(*v as f64),
            Value::U64(v) => Some(*v as f64),
            Value::U32(v) => Some(f64::from(*v)),
            Value::U16(v) => Some(f64::from(*v)),
            Value::U8(v) => Some(f64::from(*v)),
            Value::U128(v) => Some(*v as f64),
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
        self.is_integer() || self.is_float()
    }

    /// Check if this is an integer type
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Value::I64(_)
                | Value::I32(_)
                | Value::I16(_)
                | Value::I8(_)
                | Value::I128(_)
                | Value::U64(_)
                | Value::U32(_)
                | Value::U16(_)
                | Value::U8(_)
                | Value::U128(_)
        )
    }

    /// Check if this is a float type
    pub fn is_float(&self) -> bool {
        matches!(self, Value::F64(_) | Value::F32(_))
    }

    /// Return the 1-based element used by flat destructuring of a dynamic value.
    ///
    /// AoT code keeps runtime container representation behind this boundary so
    /// the compiler does not need to match runtime `Value` variants directly.
    pub fn destructure_index(&self, index: i64) -> Value {
        if index < 1 {
            crate::error::aot_throw(format!("BoundsError({self:?}, ({index},))"));
        }
        let offset = (index - 1) as usize;
        let element = match self {
            Value::Tuple(values) => values.get(offset).cloned(),
            Value::Array(values) => values.borrow().get(offset).cloned(),
            _ => crate::error::aot_throw(format!(
                "MethodError: no method matching iterate(::{})",
                self.type_name()
            )),
        };
        element.unwrap_or_else(|| {
            crate::error::aot_throw(format!("BoundsError({self:?}, ({index},))"))
        })
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

impl From<i16> for Value {
    fn from(v: i16) -> Self {
        Value::I16(v)
    }
}

impl From<i8> for Value {
    fn from(v: i8) -> Self {
        Value::I8(v)
    }
}

impl From<i128> for Value {
    fn from(v: i128) -> Self {
        Value::I128(v)
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::U64(v)
    }
}

impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::U32(v)
    }
}

impl From<u16> for Value {
    fn from(v: u16) -> Self {
        Value::U16(v)
    }
}

impl From<u8> for Value {
    fn from(v: u8) -> Self {
        Value::U8(v)
    }
}

impl From<u128> for Value {
    fn from(v: u128) -> Self {
        Value::U128(v)
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
            Value::I16(v) => write!(f, "{}", v),
            Value::I8(v) => write!(f, "{}", v),
            Value::I128(v) => write!(f, "{}", v),
            Value::U64(v) => write!(f, "{}", v),
            Value::U32(v) => write!(f, "{}", v),
            Value::U16(v) => write!(f, "{}", v),
            Value::U8(v) => write!(f, "{}", v),
            Value::U128(v) => write!(f, "{}", v),
            Value::F64(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    write!(f, "{}.0", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            // Print-form like upstream `print(::Float32)`: "2.5", "1.0" —
            // no `f0` suffix (Display feeds the generated `println!` calls,
            // Issue #10131).
            Value::F32(v) => {
                if v.fract() == 0.0 && v.abs() < 1e15 {
                    write!(f, "{}.0", v)
                } else {
                    write!(f, "{}", v)
                }
            }
            Value::Bool(v) => write!(f, "{}", v),
            Value::Char(v) => write!(f, "'{}'", v),
            Value::Nothing => write!(f, "nothing"),
            Value::Missing => write!(f, "missing"),
            Value::DataType(name) => write!(f, "{}", name),
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
            (Value::I16(a), Value::I16(b)) => a == b,
            (Value::I8(a), Value::I8(b)) => a == b,
            (Value::I128(a), Value::I128(b)) => a == b,
            (Value::U64(a), Value::U64(b)) => a == b,
            (Value::U32(a), Value::U32(b)) => a == b,
            (Value::U16(a), Value::U16(b)) => a == b,
            (Value::U8(a), Value::U8(b)) => a == b,
            (Value::U128(a), Value::U128(b)) => a == b,
            (Value::F64(a), Value::F64(b)) => a == b,
            (Value::F32(a), Value::F32(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Nothing, Value::Nothing) => true,
            (Value::Missing, Value::Missing) => true,
            (Value::DataType(a), Value::DataType(b)) => a == b,
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
        assert_eq!(Value::DataType("Int64".to_string()).type_name(), "DataType");
    }

    #[test]
    fn dynamic_value_destructuring_indexes_tuple_and_array_10464() {
        let tuple = Value::Tuple(vec![Value::I64(1), Value::I64(2)]);
        let array = Value::from(vec![Value::I64(3), Value::I64(4)]);

        assert_eq!(tuple.destructure_index(2), Value::I64(2));
        assert_eq!(array.destructure_index(1), Value::I64(3));
    }

    #[test]
    #[should_panic(expected = "BoundsError")]
    fn dynamic_value_destructuring_checks_bounds_10464() {
        Value::Tuple(vec![Value::I64(1)]).destructure_index(2);
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
        assert_eq!(format!("{}", Value::DataType("Int64".to_string())), "Int64");
    }
}
