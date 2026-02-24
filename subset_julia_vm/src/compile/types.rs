//! Type parsing utilities for parametric types and compile errors.

use crate::ir::core::StructDef;
use crate::types::{DispatchError, JuliaType, TypeExpr};

/// Key for parametric type instantiation.
/// Each unique combination of (base_name, type_args) gets its own type_id.
#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct InstantiationKey {
    pub base_name: String,
    pub type_args: Vec<TypeExpr>,
}

/// Parametric struct definition (stores the original definition before instantiation).
#[derive(Debug, Clone)]
pub struct ParametricStructDef {
    pub def: StructDef,
}

/// Parse a single type expression from a string.
/// Handles simple types (Float64), type variables (T), nested parameterized types (Point{Float64}),
/// and runtime expressions like Symbol(s).
pub fn parse_single_type_expr(s: &str) -> Option<TypeExpr> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Check for runtime expressions (function calls like Symbol(s))
    // These contain parentheses but not curly braces at the top level
    if let Some(open_paren) = s.find('(') {
        let open_brace = s.find('{');
        // If parentheses come before any curly brace (or no curly brace exists),
        // this is likely a function call that needs runtime evaluation
        if open_brace.is_none() || open_paren < open_brace.unwrap() {
            // This is a runtime expression like Symbol(s)
            return Some(TypeExpr::RuntimeExpr(s.to_string()));
        }
    }

    if let Some(open) = s.find('{') {
        // Parameterized type: Point{Float64} or Array{Point{Float64}}
        let close = s.rfind('}')?;
        if close <= open {
            return None;
        }

        let base = s[..open].trim().to_string();
        let args_str = &s[open + 1..close];

        // Recursively parse type arguments
        let args = parse_type_args_recursive(args_str)?;

        Some(TypeExpr::Parameterized { base, params: args })
    } else {
        // Simple type: Float64, Int64, T, etc.
        match JuliaType::from_name(s) {
            Some(jt) => Some(TypeExpr::Concrete(jt)),
            None => Some(TypeExpr::TypeVar(s.to_string())),
        }
    }
}

/// Parse comma-separated type arguments, respecting nested braces.
pub fn parse_type_args_recursive(s: &str) -> Option<Vec<TypeExpr>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for c in s.chars() {
        match c {
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    args.push(parse_single_type_expr(trimmed)?);
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        args.push(parse_single_type_expr(trimmed)?);
    }

    Some(args)
}

/// Parse a parametric function call like "Point{Float64}" or "Pair{Int64, Point{Float64}}"
/// Returns (base_name, type_args) if it's a parametric call, None otherwise.
/// Supports nested parameterized types like "Container{Point{Float64}}".
pub fn parse_parametric_call(name: &str) -> Option<(String, Vec<TypeExpr>)> {
    // Look for pattern: Name{Type1, Type2, ...}
    let open_brace = name.find('{')?;
    let close_brace = name.rfind('}')?;

    if close_brace <= open_brace {
        return None;
    }

    let base_name = name[..open_brace].to_string();
    let type_args_str = &name[open_brace + 1..close_brace];

    // Parse type arguments with recursive brace handling
    let type_args = parse_type_args_recursive(type_args_str)?;

    Some((base_name, type_args))
}

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

    // ── parse_parametric_call ────────────────────────────────────────────────

    #[test]
    fn test_parse_parametric_call_simple() {
        let result = parse_parametric_call("Point{Float64}");
        assert_eq!(
            result,
            Some(("Point".to_string(), vec![TypeExpr::Concrete(JuliaType::Float64)]))
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
            Some(("Generic".to_string(), vec![TypeExpr::TypeVar("T".to_string())]))
        );
    }
}
