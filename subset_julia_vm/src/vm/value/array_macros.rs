//! Macros for dispatching operations across ArrayData variants.
//!
//! This module provides macros to reduce code duplication when implementing
//! methods on ArrayData that need to handle all 15 variants.

/// Dispatch a simple method call to all ArrayData variants.
///
/// This macro handles the common pattern of calling a method on the underlying
/// Vec for each ArrayData variant.
///
/// # Example
/// ```
/// use subset_julia_vm::vm::value::ArrayData;
/// use subset_julia_vm::array_data_dispatch;
///
/// let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
/// let len = array_data_dispatch!(&data, len);
/// assert_eq!(len, 3);
///
/// let is_empty = array_data_dispatch!(&data, is_empty);
/// assert!(!is_empty);
/// ```
#[macro_export]
macro_rules! array_data_dispatch {
    ($data:expr, $method:ident) => {
        match $data {
            $crate::vm::value::ArrayData::F32(v) => v.$method(),
            $crate::vm::value::ArrayData::F64(v) => v.$method(),
            $crate::vm::value::ArrayData::I8(v) => v.$method(),
            $crate::vm::value::ArrayData::I16(v) => v.$method(),
            $crate::vm::value::ArrayData::I32(v) => v.$method(),
            $crate::vm::value::ArrayData::I64(v) => v.$method(),
            $crate::vm::value::ArrayData::U8(v) => v.$method(),
            $crate::vm::value::ArrayData::U16(v) => v.$method(),
            $crate::vm::value::ArrayData::U32(v) => v.$method(),
            $crate::vm::value::ArrayData::U64(v) => v.$method(),
            $crate::vm::value::ArrayData::Bool(v) => v.$method(),
            $crate::vm::value::ArrayData::String(v) => v.$method(),
            $crate::vm::value::ArrayData::Char(v) => v.$method(),
            $crate::vm::value::ArrayData::StructRefs(v) => v.$method(),
            $crate::vm::value::ArrayData::Any(v) => v.$method(),
        }
    };
    // With arguments
    ($data:expr, $method:ident, $($arg:expr),+) => {
        match $data {
            $crate::vm::value::ArrayData::F32(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::F64(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::I8(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::I16(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::I32(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::I64(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::U8(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::U16(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::U32(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::U64(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::Bool(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::String(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::Char(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::StructRefs(v) => v.$method($($arg),+),
            $crate::vm::value::ArrayData::Any(v) => v.$method($($arg),+),
        }
    };
}

/// Dispatch a closure to all ArrayData variants, returning Option<Value>.
///
/// This macro handles the pattern of getting a value at an index and converting
/// it to the Value enum type.
///
/// # Example
/// ```
/// use subset_julia_vm::vm::value::{ArrayData, Value};
/// use subset_julia_vm::array_data_get_value;
///
/// let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
/// let value = array_data_get_value!(&data, 1);
/// assert!(matches!(value, Some(Value::F64(x)) if x == 2.0));
/// ```
#[macro_export]
macro_rules! array_data_get_value {
    ($data:expr, $index:expr) => {
        match $data {
            $crate::vm::value::ArrayData::F32(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::F32(x))
            }
            $crate::vm::value::ArrayData::F64(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::F64(x))
            }
            $crate::vm::value::ArrayData::I8(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::I8(x))
            }
            $crate::vm::value::ArrayData::I16(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::I16(x))
            }
            $crate::vm::value::ArrayData::I32(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::I32(x))
            }
            $crate::vm::value::ArrayData::I64(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::I64(x))
            }
            $crate::vm::value::ArrayData::U8(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::U8(x))
            }
            $crate::vm::value::ArrayData::U16(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::U16(x))
            }
            $crate::vm::value::ArrayData::U32(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::U32(x))
            }
            $crate::vm::value::ArrayData::U64(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::U64(x))
            }
            $crate::vm::value::ArrayData::Bool(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::Bool(x))
            }
            $crate::vm::value::ArrayData::String(v) => v
                .get($index)
                .map(|x| $crate::vm::value::Value::Str(x.clone())),
            $crate::vm::value::ArrayData::Char(v) => {
                v.get($index).map(|&x| $crate::vm::value::Value::Char(x))
            }
            $crate::vm::value::ArrayData::StructRefs(v) => v
                .get($index)
                .map(|&idx| $crate::vm::value::Value::StructRef(idx)),
            $crate::vm::value::ArrayData::Any(v) => v.get($index).cloned(),
        }
    };
}

/// Dispatch a pop operation to all ArrayData variants, returning Result<Value, VmError>.
///
/// # Example
/// ```
/// use subset_julia_vm::vm::value::{ArrayData, Value};
/// use subset_julia_vm::vm::VmError;
/// use subset_julia_vm::array_data_pop_value;
///
/// let mut data = ArrayData::I64(vec![10, 20, 30]);
/// let value = array_data_pop_value!(&mut data);
/// assert!(matches!(value, Ok(Value::I64(30))));
///
/// let mut empty_data = ArrayData::I64(vec![]);
/// let result = array_data_pop_value!(&mut empty_data);
/// assert!(matches!(result, Err(VmError::EmptyArrayPop)));
/// ```
#[macro_export]
macro_rules! array_data_pop_value {
    ($data:expr) => {
        match $data {
            $crate::vm::value::ArrayData::F32(v) => v
                .pop()
                .map($crate::vm::value::Value::F32)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::F64(v) => v
                .pop()
                .map($crate::vm::value::Value::F64)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::I8(v) => v
                .pop()
                .map($crate::vm::value::Value::I8)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::I16(v) => v
                .pop()
                .map($crate::vm::value::Value::I16)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::I32(v) => v
                .pop()
                .map($crate::vm::value::Value::I32)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::I64(v) => v
                .pop()
                .map($crate::vm::value::Value::I64)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::U8(v) => v
                .pop()
                .map($crate::vm::value::Value::U8)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::U16(v) => v
                .pop()
                .map($crate::vm::value::Value::U16)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::U32(v) => v
                .pop()
                .map($crate::vm::value::Value::U32)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::U64(v) => v
                .pop()
                .map($crate::vm::value::Value::U64)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::Bool(v) => v
                .pop()
                .map($crate::vm::value::Value::Bool)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::String(v) => v
                .pop()
                .map($crate::vm::value::Value::Str)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::Char(v) => v
                .pop()
                .map($crate::vm::value::Value::Char)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::StructRefs(v) => v
                .pop()
                .map($crate::vm::value::Value::StructRef)
                .ok_or($crate::vm::VmError::EmptyArrayPop),
            $crate::vm::value::ArrayData::Any(v) => {
                v.pop().ok_or($crate::vm::VmError::EmptyArrayPop)
            }
        }
    };
}

/// Map a numeric operation over all numeric ArrayData variants.
///
/// This macro handles the common pattern of applying a numeric function
/// (like sum, max, min) to all numeric variants.
///
/// Non-numeric variants (String, Char, StructRefs, Any) return a default value.
///
/// The `$op` expression has access to `v`, which is the inner Vec of the matched variant.
///
/// # Example
///
/// Due to Rust macro hygiene, this macro is designed for internal use where the
/// `v` identifier is accessible. See the unit tests in this module for usage examples.
///
/// ```no_run
/// use subset_julia_vm::vm::value::ArrayData;
/// use subset_julia_vm::array_data_numeric_reduce;
///
/// // The macro binds `v` internally for use in the operation expression:
/// // let sum: f64 = array_data_numeric_reduce!(&data, v.iter().map(|&x| x as f64).sum::<f64>(), 0.0);
/// ```
#[macro_export]
macro_rules! array_data_numeric_reduce {
    ($data:expr, $op:expr, $default:expr) => {
        match $data {
            $crate::vm::value::ArrayData::F32(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::F64(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::I8(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::I16(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::I32(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::I64(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::U8(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::U16(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::U32(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::U64(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::Bool(v) => {
                let v = v;
                $op
            }
            $crate::vm::value::ArrayData::String(_)
            | $crate::vm::value::ArrayData::Char(_)
            | $crate::vm::value::ArrayData::StructRefs(_)
            | $crate::vm::value::ArrayData::Any(_) => $default,
        }
    };
}

/// Get the element type name for each ArrayData variant.
///
/// # Example
/// ```
/// use subset_julia_vm::vm::value::ArrayData;
/// use subset_julia_vm::array_data_type_name;
///
/// let data = ArrayData::F64(vec![1.0, 2.0]);
/// let type_name = array_data_type_name!(&data);
/// assert_eq!(type_name, "F64");
///
/// let int_data = ArrayData::I64(vec![1, 2, 3]);
/// assert_eq!(array_data_type_name!(&int_data), "I64");
/// ```
#[macro_export]
macro_rules! array_data_type_name {
    ($data:expr) => {
        match $data {
            $crate::vm::value::ArrayData::F32(_) => "F32",
            $crate::vm::value::ArrayData::F64(_) => "F64",
            $crate::vm::value::ArrayData::I8(_) => "I8",
            $crate::vm::value::ArrayData::I16(_) => "I16",
            $crate::vm::value::ArrayData::I32(_) => "I32",
            $crate::vm::value::ArrayData::I64(_) => "I64",
            $crate::vm::value::ArrayData::U8(_) => "U8",
            $crate::vm::value::ArrayData::U16(_) => "U16",
            $crate::vm::value::ArrayData::U32(_) => "U32",
            $crate::vm::value::ArrayData::U64(_) => "U64",
            $crate::vm::value::ArrayData::Bool(_) => "Bool",
            $crate::vm::value::ArrayData::String(_) => "String",
            $crate::vm::value::ArrayData::Char(_) => "Char",
            $crate::vm::value::ArrayData::StructRefs(_) => "StructRefs",
            $crate::vm::value::ArrayData::Any(_) => "Any",
        }
    };
}

/// Execute a block for each ArrayData variant with the inner Vec bound to `v`.
///
/// This is the most flexible macro - it allows arbitrary code to be executed
/// with access to the underlying Vec.
///
/// # Example
/// ```
/// use subset_julia_vm::vm::value::ArrayData;
/// use subset_julia_vm::array_data_with_vec;
///
/// let data = ArrayData::I64(vec![10, 20, 30]);
/// let length: usize = array_data_with_vec!(&data, v, { v.len() });
/// assert_eq!(length, 3);
/// ```
#[macro_export]
macro_rules! array_data_with_vec {
    ($data:expr, $vec_name:ident, $block:expr) => {
        match $data {
            $crate::vm::value::ArrayData::F32($vec_name) => $block,
            $crate::vm::value::ArrayData::F64($vec_name) => $block,
            $crate::vm::value::ArrayData::I8($vec_name) => $block,
            $crate::vm::value::ArrayData::I16($vec_name) => $block,
            $crate::vm::value::ArrayData::I32($vec_name) => $block,
            $crate::vm::value::ArrayData::I64($vec_name) => $block,
            $crate::vm::value::ArrayData::U8($vec_name) => $block,
            $crate::vm::value::ArrayData::U16($vec_name) => $block,
            $crate::vm::value::ArrayData::U32($vec_name) => $block,
            $crate::vm::value::ArrayData::U64($vec_name) => $block,
            $crate::vm::value::ArrayData::Bool($vec_name) => $block,
            $crate::vm::value::ArrayData::String($vec_name) => $block,
            $crate::vm::value::ArrayData::Char($vec_name) => $block,
            $crate::vm::value::ArrayData::StructRefs($vec_name) => $block,
            $crate::vm::value::ArrayData::Any($vec_name) => $block,
        }
    };
}

/// Execute a block for each ArrayData variant with type-specific handling.
///
/// This macro allows you to specify different code for numeric and non-numeric variants.
/// Note: Bool is excluded from numeric types because `bool as f64` is invalid in Rust.
///
/// # Example
/// ```
/// use subset_julia_vm::vm::value::ArrayData;
/// use subset_julia_vm::array_data_numeric_or_else;
///
/// let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
/// let sum: f64 = array_data_numeric_or_else!(&data, v, {
///     v.iter().map(|&x| x as f64).sum::<f64>()
/// }, 0.0);
/// assert_eq!(sum, 6.0);
///
/// // Non-numeric path returns the default
/// let string_data = ArrayData::String(vec!["hello".to_string()]);
/// let sum: f64 = array_data_numeric_or_else!(&string_data, _v, { 42.0 }, 0.0);
/// assert_eq!(sum, 0.0);
/// ```
#[macro_export]
macro_rules! array_data_numeric_or_else {
    ($data:expr, $vec_name:ident, $numeric_block:expr, $default:expr) => {
        match $data {
            $crate::vm::value::ArrayData::F32($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::F64($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::I8($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::I16($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::I32($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::I64($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::U8($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::U16($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::U32($vec_name) => $numeric_block,
            $crate::vm::value::ArrayData::U64($vec_name) => $numeric_block,
            // Note: Bool is NOT included here because `bool as f64` is invalid in Rust
            $crate::vm::value::ArrayData::Bool(_)
            | $crate::vm::value::ArrayData::String(_)
            | $crate::vm::value::ArrayData::Char(_)
            | $crate::vm::value::ArrayData::StructRefs(_)
            | $crate::vm::value::ArrayData::Any(_) => $default,
        }
    };
}

/// Execute a block for each ArrayData variant that can be cast to f64.
/// This includes all numeric types AND Bool (via conversion).
///
/// # Example
/// ```
/// use subset_julia_vm::vm::value::ArrayData;
/// use subset_julia_vm::array_data_as_f64;
///
/// let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
/// let sum: f64 = array_data_as_f64!(
///     &data,
///     v,
///     { v.iter().map(|&x| x as f64).sum::<f64>() },  // numeric block
///     { 0.0 },  // bool block (not executed here)
///     0.0       // default for non-castable types
/// );
/// assert_eq!(sum, 6.0);
///
/// // Bool variant uses the bool_block
/// let bool_data = ArrayData::Bool(vec![true, false, true]);
/// let count: f64 = array_data_as_f64!(
///     &bool_data,
///     v,
///     { 0.0 },  // numeric block (not executed)
///     { v.iter().filter(|&&b| b).count() as f64 },  // bool block
///     0.0
/// );
/// assert_eq!(count, 2.0);
/// ```
#[macro_export]
macro_rules! array_data_as_f64 {
    ($data:expr, $vec_name:ident, $block:expr, $bool_block:expr, $default:expr) => {
        match $data {
            $crate::vm::value::ArrayData::F32($vec_name) => $block,
            $crate::vm::value::ArrayData::F64($vec_name) => $block,
            $crate::vm::value::ArrayData::I8($vec_name) => $block,
            $crate::vm::value::ArrayData::I16($vec_name) => $block,
            $crate::vm::value::ArrayData::I32($vec_name) => $block,
            $crate::vm::value::ArrayData::I64($vec_name) => $block,
            $crate::vm::value::ArrayData::U8($vec_name) => $block,
            $crate::vm::value::ArrayData::U16($vec_name) => $block,
            $crate::vm::value::ArrayData::U32($vec_name) => $block,
            $crate::vm::value::ArrayData::U64($vec_name) => $block,
            $crate::vm::value::ArrayData::Bool($vec_name) => $bool_block,
            $crate::vm::value::ArrayData::String(_)
            | $crate::vm::value::ArrayData::Char(_)
            | $crate::vm::value::ArrayData::StructRefs(_)
            | $crate::vm::value::ArrayData::Any(_) => $default,
        }
    };
}

// Re-export macros at module level for convenience
pub use array_data_as_f64;
pub use array_data_dispatch;
pub use array_data_get_value;
pub use array_data_numeric_or_else;
pub use array_data_numeric_reduce;
pub use array_data_pop_value;
pub use array_data_type_name;
pub use array_data_with_vec;

#[cfg(test)]
mod tests {
    use super::super::*;
    use num_traits::ToPrimitive;

    #[test]
    fn test_array_data_dispatch_len() {
        let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
        let len = array_data_dispatch!(&data, len);
        assert_eq!(len, 3);

        let data = ArrayData::I64(vec![1, 2, 3, 4, 5]);
        let len = array_data_dispatch!(&data, len);
        assert_eq!(len, 5);
    }

    #[test]
    fn test_array_data_dispatch_is_empty() {
        let data = ArrayData::F64(vec![]);
        let is_empty = array_data_dispatch!(&data, is_empty);
        assert!(is_empty);

        let data = ArrayData::I64(vec![1]);
        let is_empty = array_data_dispatch!(&data, is_empty);
        assert!(!is_empty);
    }

    #[test]
    fn test_array_data_get_value() {
        let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
        let val = array_data_get_value!(&data, 1);
        assert!(matches!(val, Some(Value::F64(x)) if x == 2.0));

        let data = ArrayData::I64(vec![10, 20, 30]);
        let val = array_data_get_value!(&data, 0);
        assert!(matches!(val, Some(Value::I64(10))));
    }

    #[test]
    fn test_array_data_type_name() {
        let data = ArrayData::F64(vec![]);
        assert_eq!(array_data_type_name!(&data), "F64");

        let data = ArrayData::I64(vec![]);
        assert_eq!(array_data_type_name!(&data), "I64");

        let data = ArrayData::String(vec![]);
        assert_eq!(array_data_type_name!(&data), "String");
    }

    #[test]
    fn test_array_data_numeric_or_else() {
        let data = ArrayData::F64(vec![1.0, 2.0, 3.0]);
        let sum: f64 = array_data_numeric_or_else!(
            &data,
            v,
            { v.iter().filter_map(|x| x.to_f64()).sum() },
            0.0
        );
        assert_eq!(sum, 6.0);

        let data = ArrayData::I64(vec![10, 20, 30]);
        let sum: f64 = array_data_numeric_or_else!(
            &data,
            v,
            { v.iter().filter_map(|x| x.to_f64()).sum() },
            0.0
        );
        assert_eq!(sum, 60.0);

        // Non-numeric variant returns default
        let data = ArrayData::String(vec!["a".to_string()]);
        let sum: f64 = array_data_numeric_or_else!(
            &data,
            _v,
            {
                42.0 // This block won't execute
            },
            0.0
        );
        assert_eq!(sum, 0.0);
    }
}
