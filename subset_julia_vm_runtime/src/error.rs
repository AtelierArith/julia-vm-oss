//! Runtime error types for AoT compiled code
//!
//! This module provides error types that can occur during execution
//! of AoT compiled code.

use thiserror::Error;

/// Runtime error type
///
/// Represents errors that can occur during execution of AoT compiled code.
#[derive(Debug, Error)]
pub enum RuntimeError {
    /// Type mismatch error
    #[error("TypeError: {0}")]
    TypeError(String),

    /// Method not found error (multiple dispatch failure)
    #[error("MethodError: no method matching {0}")]
    MethodError(String),

    /// Index out of bounds error
    #[error("BoundsError: attempt to access index {index} of array with length {length}")]
    BoundsError {
        /// Attempted index
        index: i64,
        /// Array length
        length: usize,
    },

    /// Division by zero error
    #[error("DivideError: integer division error")]
    DivisionByZero,

    /// Invalid argument error
    #[error("ArgumentError: {0}")]
    ArgumentError(String),

    /// Key not found in dictionary
    #[error("KeyError: key {0} not found")]
    KeyError(String),

    /// Field not found in struct
    #[error("FieldError: field {0} not found in type {1}")]
    FieldError(String, String),

    /// Stack overflow error
    #[error("StackOverflowError: stack overflow")]
    StackOverflow,

    /// Out of memory error
    #[error("OutOfMemoryError: out of memory")]
    OutOfMemory,

    /// Assertion failure
    #[error("AssertionError: {0}")]
    AssertionError(String),

    /// Domain error (e.g., sqrt of negative number)
    #[error("DomainError: {0}")]
    DomainError(String),

    /// Inexact error (e.g., converting 1.5 to Int)
    #[error("InexactError: {0}")]
    InexactError(String),

    /// Overflow error
    #[error("OverflowError: {0}")]
    OverflowError(String),

    /// Unimplemented feature
    #[error("UnimplementedError: {0}")]
    Unimplemented(String),

    /// Generic error with custom message
    #[error("{0}")]
    Custom(String),
}

impl RuntimeError {
    /// Create a type error
    pub fn type_error<S: Into<String>>(msg: S) -> Self {
        RuntimeError::TypeError(msg.into())
    }

    /// Create a method error
    pub fn method_error<S: Into<String>>(method: S) -> Self {
        RuntimeError::MethodError(method.into())
    }

    /// Create a bounds error
    pub fn bounds_error(index: i64, length: usize) -> Self {
        RuntimeError::BoundsError { index, length }
    }

    /// Create an argument error
    pub fn argument_error<S: Into<String>>(msg: S) -> Self {
        RuntimeError::ArgumentError(msg.into())
    }

    /// Create a key error
    pub fn key_error<S: Into<String>>(key: S) -> Self {
        RuntimeError::KeyError(key.into())
    }

    /// Create a field error
    pub fn field_error<S1: Into<String>, S2: Into<String>>(field: S1, type_name: S2) -> Self {
        RuntimeError::FieldError(field.into(), type_name.into())
    }

    /// Create a domain error
    pub fn domain_error<S: Into<String>>(msg: S) -> Self {
        RuntimeError::DomainError(msg.into())
    }

    /// Create an inexact error
    pub fn inexact_error<S: Into<String>>(msg: S) -> Self {
        RuntimeError::InexactError(msg.into())
    }

    /// Create an overflow error
    pub fn overflow_error<S: Into<String>>(msg: S) -> Self {
        RuntimeError::OverflowError(msg.into())
    }

    /// Create an assertion error
    pub fn assertion_error<S: Into<String>>(msg: S) -> Self {
        RuntimeError::AssertionError(msg.into())
    }

    /// Create an unimplemented error
    pub fn unimplemented<S: Into<String>>(feature: S) -> Self {
        RuntimeError::Unimplemented(feature.into())
    }

    /// Create a custom error
    pub fn custom<S: Into<String>>(msg: S) -> Self {
        RuntimeError::Custom(msg.into())
    }
}

/// Result type alias for AoT runtime operations
pub type RuntimeResult<T> = Result<T, RuntimeError>;

/// Diverging `throw` used by AoT-compiled code: maps Julia's `throw(e)` to an
/// aborting panic. Kept in the runtime crate (rather than inlined as
/// `panic!(...)` in every generated file) so the generated code itself stays
/// free of raw `panic!` / `.unwrap()` (Issue #5658).
pub fn aot_throw<T: std::fmt::Display>(e: T) -> ! {
    panic!("{}", e);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = RuntimeError::type_error("expected Int64, got Float64");
        assert_eq!(format!("{}", err), "TypeError: expected Int64, got Float64");

        let err = RuntimeError::bounds_error(10, 5);
        assert_eq!(
            format!("{}", err),
            "BoundsError: attempt to access index 10 of array with length 5"
        );

        let err = RuntimeError::DivisionByZero;
        assert_eq!(format!("{}", err), "DivideError: integer division error");
    }

    #[test]
    fn test_error_constructors() {
        let _ = RuntimeError::method_error("add(Int64, String)");
        let _ = RuntimeError::key_error("missing_key");
        let _ = RuntimeError::field_error("x", "Point");
    }

    #[test]
    #[should_panic(expected = "DivideError: integer division error")]
    fn aot_throw_uses_display_text_issue_7018() {
        aot_throw(RuntimeError::DivisionByZero);
    }
}
