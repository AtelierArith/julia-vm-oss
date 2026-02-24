//! Transfer functions for string operations.
//!
//! This module implements type inference for Julia's string operations.

use crate::compile::lattice::types::{ConcreteType, LatticeType};

/// Transfer function for `string` (string conversion/concatenation).
///
/// Type rules:
/// - string(T...) → String
///
/// # Examples
/// ```text
/// string(Int64) → String
/// string(String, Int64) → String
/// ```
pub fn tfunc_string(_args: &[LatticeType]) -> LatticeType {
    // string() always returns String regardless of input types
    LatticeType::Concrete(ConcreteType::String)
}

/// Transfer function for `*` (string concatenation).
///
/// Type rules:
/// - String * String → String
///
/// # Examples
/// ```text
/// *(String, String) → String
/// ```
pub fn tfunc_string_concat(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    match (&args[0], &args[1]) {
        (
            LatticeType::Concrete(ConcreteType::String),
            LatticeType::Concrete(ConcreteType::String),
        ) => LatticeType::Concrete(ConcreteType::String),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `length` applied to strings.
///
/// This is the same as the generic length in array_ops, but included here
/// for completeness.
pub fn tfunc_string_length(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Int64),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `uppercase` (convert to uppercase).
///
/// Type rules:
/// - uppercase(String) → String
/// - uppercase(Char) → Char (Issue #2064)
pub fn tfunc_uppercase(args: &[LatticeType]) -> LatticeType {
    if args.len() != 1 {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::String),
        LatticeType::Concrete(ConcreteType::Char) => LatticeType::Concrete(ConcreteType::Char),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `lowercase` (convert to lowercase).
/// - lowercase(String) → String
/// - lowercase(Char) → Char (Issue #2064)
pub fn tfunc_lowercase(args: &[LatticeType]) -> LatticeType {
    tfunc_uppercase(args) // Same type rules
}

/// Transfer function for `replace` (string replacement).
///
/// Type rules:
/// - replace(String, ...) → String
pub fn tfunc_replace(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::String),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `split` (string splitting).
///
/// Type rules:
/// - split(String, ...) → Array{String}
pub fn tfunc_split(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::String) => LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::String),
        }),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `join` (array joining into string).
///
/// Type rules:
/// - join(Array{T}, String) → String
pub fn tfunc_join(args: &[LatticeType]) -> LatticeType {
    if args.is_empty() {
        return LatticeType::Top;
    }

    match &args[0] {
        LatticeType::Concrete(ConcreteType::Array { .. }) => {
            LatticeType::Concrete(ConcreteType::String)
        }
        _ => LatticeType::Top,
    }
}

/// Transfer function for `startswith` (check string prefix).
///
/// Type rules:
/// - startswith(String, String) → Bool
pub fn tfunc_startswith(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    match (&args[0], &args[1]) {
        (
            LatticeType::Concrete(ConcreteType::String),
            LatticeType::Concrete(ConcreteType::String),
        ) => LatticeType::Concrete(ConcreteType::Bool),
        _ => LatticeType::Top,
    }
}

/// Transfer function for `endswith` (check string suffix).
pub fn tfunc_endswith(args: &[LatticeType]) -> LatticeType {
    tfunc_startswith(args) // Same type rules
}

/// Transfer function for `contains` (check substring).
pub fn tfunc_contains(args: &[LatticeType]) -> LatticeType {
    tfunc_startswith(args) // Same type rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_conversion() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Float64),
        ];
        let result = tfunc_string(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::String));
    }

    #[test]
    fn test_string_concat() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::String),
            LatticeType::Concrete(ConcreteType::String),
        ];
        let result = tfunc_string_concat(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::String));
    }

    #[test]
    fn test_string_length() {
        let args = vec![LatticeType::Concrete(ConcreteType::String)];
        let result = tfunc_string_length(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_uppercase() {
        let args = vec![LatticeType::Concrete(ConcreteType::String)];
        let result = tfunc_uppercase(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::String));
    }

    #[test]
    fn test_split() {
        let args = vec![LatticeType::Concrete(ConcreteType::String)];
        let result = tfunc_split(&args);
        assert_eq!(
            result,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::String)
            })
        );
    }

    #[test]
    fn test_join() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::String),
            }),
            LatticeType::Concrete(ConcreteType::String),
        ];
        let result = tfunc_join(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::String));
    }

    #[test]
    fn test_startswith() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::String),
            LatticeType::Concrete(ConcreteType::String),
        ];
        let result = tfunc_startswith(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_contains() {
        let args = vec![
            LatticeType::Concrete(ConcreteType::String),
            LatticeType::Concrete(ConcreteType::String),
        ];
        let result = tfunc_contains(&args);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Bool));
    }
}
