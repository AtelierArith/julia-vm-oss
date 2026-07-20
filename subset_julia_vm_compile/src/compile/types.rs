//! Type parsing utilities for parametric types and compile errors.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub use crate::types::{parse_parametric_call, parse_type_args_recursive};
#[cfg(test)]
use crate::types::{parse_single_type_expr, JuliaType};
use crate::types::{DispatchError, TypeExpr};

/// Key for parametric type instantiation.
/// Each unique combination of (base_name, type_args) gets its own type_id.
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct InstantiationKey {
    pub base_name: String,
    pub type_args: Vec<TypeExpr>,
}

/// Re-export shim: `ParametricStructDef` is owned by the shared runtime type
/// layer (`crate::runtime_types::struct_info`) since Issue #8557; the
/// `compile::types::ParametricStructDef` / `compile::ParametricStructDef`
/// paths stay valid for existing users.
pub use crate::runtime_types::struct_info::ParametricStructDef;

#[derive(Debug)]
pub enum CompileError {
    Msg(String),
    Dispatch(DispatchError),
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::Msg(msg) => write!(f, "{}", msg),
            CompileError::Dispatch(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for CompileError {}

impl From<DispatchError> for CompileError {
    fn from(err: DispatchError) -> Self {
        CompileError::Dispatch(err)
    }
}

pub type CResult<T> = Result<T, CompileError>;

pub fn err<T>(m: impl Into<String>) -> CResult<T> {
    Err(CompileError::Msg(m.into()))
}

/// Build an internal-error [`CompileError`] for a proof-backed compile-time
/// invariant that should be unreachable given the immediately preceding
/// control flow. Mirrors the parser crate's `internal_parser_error` helper
/// (Issue #10904) and this module's own pre-existing `"internal: ..."`
/// message convention (e.g. `compile/stmt.rs`'s `Try` statement guard): a
/// compiler-side invariant break must surface as a typed error instead of an
/// uncaught host crash if a future refactor ever invalidates the
/// precondition (Issue #10905, Phase 1b of #10869).
pub fn internal_compile_error(context: impl Into<String>) -> CompileError {
    CompileError::Msg(format!("internal: {}", context.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_single_type_expr ───────────────────────────────────────────────

    #[test]
    fn test_parse_single_type_expr_empty_returns_none() {
        assert!(parse_single_type_expr("").is_none());
        assert!(parse_single_type_expr("   ").is_none());
    }

    #[test]
    fn test_parse_single_type_expr_concrete_float64() {
        let result = parse_single_type_expr("Float64");
        assert_eq!(result, Some(TypeExpr::Concrete(JuliaType::Float64)));
    }

    #[test]
    fn test_parse_single_type_expr_concrete_int64() {
        let result = parse_single_type_expr("Int64");
        assert_eq!(result, Some(TypeExpr::Concrete(JuliaType::Int64)));
    }

    #[test]
    fn test_parse_single_type_expr_concrete_string() {
        let result = parse_single_type_expr("String");
        assert_eq!(result, Some(TypeExpr::Concrete(JuliaType::String)));
    }

    #[test]
    fn test_parse_single_type_expr_type_var_unknown() {
        // Unknown names become TypeVar (type parameter references)
        let result = parse_single_type_expr("T");
        assert_eq!(result, Some(TypeExpr::TypeVar("T".to_string())));
    }

    #[test]
    fn test_parse_single_type_expr_parameterized() {
        // Array{Float64} → Parameterized
        let result = parse_single_type_expr("Array{Float64}");
        assert_eq!(
            result,
            Some(TypeExpr::Parameterized {
                base: "Array".to_string(),
                params: vec![TypeExpr::Concrete(JuliaType::Float64)],
            })
        );
    }

    #[test]
    fn test_parse_single_type_expr_nested_parameterized() {
        // Container{Point{Float64}} — nested braces
        let result = parse_single_type_expr("Container{Point{Float64}}");
        assert_eq!(
            result,
            Some(TypeExpr::Parameterized {
                base: "Container".to_string(),
                params: vec![TypeExpr::Parameterized {
                    base: "Point".to_string(),
                    params: vec![TypeExpr::Concrete(JuliaType::Float64)],
                }],
            })
        );
    }

    #[test]
    fn test_parse_single_type_expr_runtime_expr() {
        // Expressions with parentheses before braces → RuntimeExpr
        let result = parse_single_type_expr("Symbol(s)");
        assert_eq!(result, Some(TypeExpr::RuntimeExpr("Symbol(s)".to_string())));
    }

    #[test]
    fn test_parse_single_type_expr_value_param_arithmetic_runtime_expr() {
        let result = parse_single_type_expr("N-1");
        assert_eq!(result, Some(TypeExpr::RuntimeExpr("N-1".to_string())));
    }

    #[test]
    fn test_parse_single_type_expr_unclosed_brace_returns_none() {
        // "Point{" has no closing brace → None
        let result = parse_single_type_expr("Point{");
        assert!(result.is_none(), "unclosed brace should return None");
    }

    #[test]
    fn test_parse_single_type_expr_whitespace_trimmed() {
        // Leading/trailing whitespace is trimmed
        let result = parse_single_type_expr("  Float64  ");
        assert_eq!(result, Some(TypeExpr::Concrete(JuliaType::Float64)));
    }

    // ── parse_type_args_recursive ────────────────────────────────────────────

    #[test]
    fn test_parse_type_args_recursive_empty_returns_empty_vec() {
        let result = parse_type_args_recursive("");
        assert_eq!(result, Some(vec![]));
    }

    #[test]
    fn test_parse_type_args_recursive_empty_union_is_bottom() {
        let result = parse_type_args_recursive("Union{}");
        assert_eq!(result, Some(vec![TypeExpr::Concrete(JuliaType::Bottom)]));
    }

    #[test]
    fn test_parse_type_args_recursive_single_type() {
        let result = parse_type_args_recursive("Float64");
        assert_eq!(result, Some(vec![TypeExpr::Concrete(JuliaType::Float64)]));
    }

    #[test]
    fn test_parse_type_args_recursive_two_types() {
        let result = parse_type_args_recursive("Float64, Int64");
        assert_eq!(
            result,
            Some(vec![
                TypeExpr::Concrete(JuliaType::Float64),
                TypeExpr::Concrete(JuliaType::Int64),
            ])
        );
    }

    #[test]
    fn test_parse_type_args_recursive_nested_does_not_split_inner_comma() {
        // "Point{Int64, Float64}, T" — the comma inside braces must NOT split the first arg
        let result = parse_type_args_recursive("Point{Int64, Float64}, T");
        assert_eq!(
            result,
            Some(vec![
                TypeExpr::Parameterized {
                    base: "Point".to_string(),
                    params: vec![
                        TypeExpr::Concrete(JuliaType::Int64),
                        TypeExpr::Concrete(JuliaType::Float64),
                    ],
                },
                TypeExpr::TypeVar("T".to_string()),
            ])
        );
    }

    #[test]
    fn test_parse_type_args_recursive_tuple_value_param_does_not_split_inner_comma() {
        let result = parse_type_args_recursive("(1, 2), Int64");
        assert_eq!(
            result,
            Some(vec![
                TypeExpr::RuntimeExpr("(1, 2)".to_string()),
                TypeExpr::Concrete(JuliaType::Int64),
            ])
        );
    }

    // ── parse_parametric_call ────────────────────────────────────────────────

    #[test]
    fn test_parse_parametric_call_simple() {
        let result = parse_parametric_call("Point{Float64}");
        assert_eq!(
            result,
            Some((
                "Point".to_string(),
                vec![TypeExpr::Concrete(JuliaType::Float64)]
            ))
        );
    }

    #[test]
    fn test_parse_parametric_call_two_params() {
        let result = parse_parametric_call("Pair{Int64, String}");
        assert_eq!(
            result,
            Some((
                "Pair".to_string(),
                vec![
                    TypeExpr::Concrete(JuliaType::Int64),
                    TypeExpr::Concrete(JuliaType::String),
                ]
            ))
        );
    }

    #[test]
    fn test_parse_parametric_call_no_braces_returns_none() {
        // Simple name without braces → None
        assert!(parse_parametric_call("Int64").is_none());
        assert!(parse_parametric_call("Float64").is_none());
    }

    #[test]
    fn test_parse_parametric_call_nested() {
        // Container{Point{Float64}} — nested parameterized type
        let result = parse_parametric_call("Container{Point{Float64}}");
        assert_eq!(
            result,
            Some((
                "Container".to_string(),
                vec![TypeExpr::Parameterized {
                    base: "Point".to_string(),
                    params: vec![TypeExpr::Concrete(JuliaType::Float64)],
                }]
            ))
        );
    }

    #[test]
    fn test_parse_parametric_call_type_var_param() {
        // Generic{T} — T is an unknown name, becomes TypeVar
        let result = parse_parametric_call("Generic{T}");
        assert_eq!(
            result,
            Some((
                "Generic".to_string(),
                vec![TypeExpr::TypeVar("T".to_string())]
            ))
        );
    }

    #[test]
    fn test_parse_parametric_call_qualified_type_arg() {
        let result = parse_parametric_call("DataStructures.BinaryMaxHeap{M.B}");
        assert_eq!(
            result,
            Some((
                "DataStructures.BinaryMaxHeap".to_string(),
                vec![TypeExpr::TypeVar("M.B".to_string())]
            ))
        );
    }
}
