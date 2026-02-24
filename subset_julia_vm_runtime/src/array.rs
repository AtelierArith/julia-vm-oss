//! Array operations for AoT runtime
//!
//! This module provides typed array support and operations.

// SAFETY: All i64→usize casts in this module are guarded by `index - 1` with
// prior 1-based bounds checks that ensure the value is non-negative.
#![allow(clippy::cast_sign_loss)]

use crate::error::{RuntimeError, RuntimeResult};
use crate::value::Value;

/// Typed array enum for efficient storage
///
/// Stores homogeneous arrays without boxing overhead.
#[derive(Debug, Clone)]
pub enum TypedArray {
    /// Array of 64-bit integers
    I64(Vec<i64>),
    /// Array of 32-bit integers
    I32(Vec<i32>),
    /// Array of 64-bit floats
    F64(Vec<f64>),
    /// Array of 32-bit floats
    F32(Vec<f32>),
    /// Array of booleans
    Bool(Vec<bool>),
    /// Array of characters
    Char(Vec<char>),
    /// Array of strings
    Str(Vec<String>),
    /// Array of dynamic values (fallback)
    Any(Vec<Value>),
}

impl TypedArray {
    /// Create a new empty array of i64
    pub fn new_i64() -> Self {
        TypedArray::I64(Vec::new())
    }

    /// Create a new empty array of f64
    pub fn new_f64() -> Self {
        TypedArray::F64(Vec::new())
    }

    /// Create a new empty array of bool
    pub fn new_bool() -> Self {
        TypedArray::Bool(Vec::new())
    }

    /// Create a new empty array of Any
    pub fn new_any() -> Self {
        TypedArray::Any(Vec::new())
    }

    /// Get the length of the array
    pub fn len(&self) -> usize {
        match self {
            TypedArray::I64(v) => v.len(),
            TypedArray::I32(v) => v.len(),
            TypedArray::F64(v) => v.len(),
            TypedArray::F32(v) => v.len(),
            TypedArray::Bool(v) => v.len(),
            TypedArray::Char(v) => v.len(),
            TypedArray::Str(v) => v.len(),
            TypedArray::Any(v) => v.len(),
        }
    }

    /// Check if the array is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the element type name
    pub fn element_type(&self) -> &'static str {
        match self {
            TypedArray::I64(_) => "Int64",
            TypedArray::I32(_) => "Int32",
            TypedArray::F64(_) => "Float64",
            TypedArray::F32(_) => "Float32",
            TypedArray::Bool(_) => "Bool",
            TypedArray::Char(_) => "Char",
            TypedArray::Str(_) => "String",
            TypedArray::Any(_) => "Any",
        }
    }

    /// Convert to Value array (for dynamic dispatch)
    pub fn to_value_vec(&self) -> Vec<Value> {
        match self {
            TypedArray::I64(v) => v.iter().map(|&x| Value::I64(x)).collect(),
            TypedArray::I32(v) => v.iter().map(|&x| Value::I32(x)).collect(),
            TypedArray::F64(v) => v.iter().map(|&x| Value::F64(x)).collect(),
            TypedArray::F32(v) => v.iter().map(|&x| Value::F32(x)).collect(),
            TypedArray::Bool(v) => v.iter().map(|&x| Value::Bool(x)).collect(),
            TypedArray::Char(v) => v.iter().map(|&x| Value::Char(x)).collect(),
            TypedArray::Str(v) => v.iter().map(|x| Value::Str(x.clone())).collect(),
            TypedArray::Any(v) => v.clone(),
        }
    }
}

// ========== Array creation functions ==========

/// Create an array of zeros
pub fn zeros_i64(n: usize) -> TypedArray {
    TypedArray::I64(vec![0; n])
}

/// Create an array of zeros (f64)
pub fn zeros_f64(n: usize) -> TypedArray {
    TypedArray::F64(vec![0.0; n])
}

/// Create an array of ones
pub fn ones_i64(n: usize) -> TypedArray {
    TypedArray::I64(vec![1; n])
}

/// Create an array of ones (f64)
pub fn ones_f64(n: usize) -> TypedArray {
    TypedArray::F64(vec![1.0; n])
}

/// Fill an array with a value
pub fn fill_i64(value: i64, n: usize) -> TypedArray {
    TypedArray::I64(vec![value; n])
}

/// Fill an array with a value (f64)
pub fn fill_f64(value: f64, n: usize) -> TypedArray {
    TypedArray::F64(vec![value; n])
}

// ========== Array access functions ==========

/// Get element at index (1-based Julia indexing)
pub fn getindex_i64(arr: &[i64], index: i64) -> RuntimeResult<i64> {
    let idx = (index - 1) as usize;
    arr.get(idx)
        .copied()
        .ok_or_else(|| RuntimeError::bounds_error(idx, arr.len()))
}

/// Get element at index (1-based Julia indexing)
pub fn getindex_f64(arr: &[f64], index: i64) -> RuntimeResult<f64> {
    let idx = (index - 1) as usize;
    arr.get(idx)
        .copied()
        .ok_or_else(|| RuntimeError::bounds_error(idx, arr.len()))
}

/// Set element at index (1-based Julia indexing)
pub fn setindex_i64(arr: &mut [i64], value: i64, index: i64) -> RuntimeResult<()> {
    let idx = (index - 1) as usize;
    if idx >= arr.len() {
        return Err(RuntimeError::bounds_error(idx, arr.len()));
    }
    arr[idx] = value;
    Ok(())
}

/// Set element at index (1-based Julia indexing)
pub fn setindex_f64(arr: &mut [f64], value: f64, index: i64) -> RuntimeResult<()> {
    let idx = (index - 1) as usize;
    if idx >= arr.len() {
        return Err(RuntimeError::bounds_error(idx, arr.len()));
    }
    arr[idx] = value;
    Ok(())
}

// ========== Array mutation functions ==========

/// Push element to array
pub fn push_i64(arr: &mut Vec<i64>, value: i64) {
    arr.push(value);
}

/// Push element to array
pub fn push_f64(arr: &mut Vec<f64>, value: f64) {
    arr.push(value);
}

/// Pop element from array
pub fn pop_i64(arr: &mut Vec<i64>) -> RuntimeResult<i64> {
    arr.pop()
        .ok_or_else(|| RuntimeError::argument_error("array is empty"))
}

/// Pop element from array
pub fn pop_f64(arr: &mut Vec<f64>) -> RuntimeResult<f64> {
    arr.pop()
        .ok_or_else(|| RuntimeError::argument_error("array is empty"))
}

// ========== Array aggregation functions ==========

/// Sum of array elements
pub fn sum_i64(arr: &[i64]) -> i64 {
    arr.iter().sum()
}

/// Sum of array elements
pub fn sum_f64(arr: &[f64]) -> f64 {
    arr.iter().sum()
}

/// Product of array elements
pub fn prod_i64(arr: &[i64]) -> i64 {
    arr.iter().product()
}

/// Product of array elements
pub fn prod_f64(arr: &[f64]) -> f64 {
    arr.iter().product()
}

/// Minimum of array elements
pub fn minimum_i64(arr: &[i64]) -> RuntimeResult<i64> {
    arr.iter()
        .copied()
        .min()
        .ok_or_else(|| RuntimeError::argument_error("array is empty"))
}

/// Maximum of array elements
pub fn maximum_i64(arr: &[i64]) -> RuntimeResult<i64> {
    arr.iter()
        .copied()
        .max()
        .ok_or_else(|| RuntimeError::argument_error("array is empty"))
}

/// Minimum of array elements
pub fn minimum_f64(arr: &[f64]) -> RuntimeResult<f64> {
    arr.iter()
        .copied()
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .ok_or_else(|| RuntimeError::argument_error("array is empty"))
}

/// Maximum of array elements
pub fn maximum_f64(arr: &[f64]) -> RuntimeResult<f64> {
    arr.iter()
        .copied()
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .ok_or_else(|| RuntimeError::argument_error("array is empty"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typed_array_creation() {
        let arr = zeros_i64(5);
        assert_eq!(arr.len(), 5);
        assert_eq!(arr.element_type(), "Int64");

        let arr = ones_f64(3);
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.element_type(), "Float64");
    }

    #[test]
    fn test_array_access() {
        let arr = vec![10, 20, 30];
        assert_eq!(getindex_i64(&arr, 1).unwrap(), 10);
        assert_eq!(getindex_i64(&arr, 2).unwrap(), 20);
        assert_eq!(getindex_i64(&arr, 3).unwrap(), 30);
        assert!(getindex_i64(&arr, 4).is_err());
    }

    #[test]
    fn test_array_sum() {
        let arr = vec![1, 2, 3, 4, 5];
        assert_eq!(sum_i64(&arr), 15);

        let arr = vec![1.0, 2.0, 3.0];
        assert_eq!(sum_f64(&arr), 6.0);
    }
}
