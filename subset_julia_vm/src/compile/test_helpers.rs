//! Shared test constructors for `compile/` unit tests (Issue #3183).
//!
//! This module provides helper functions for constructing IR nodes in tests,
//! avoiding common pitfalls:
//! - `Span` has no `Default` impl — use `zero_span()` instead
//! - `Instr::LoadI64(String)` loads a variable by name, not a literal — use `Instr::PushI64(i64)`
//! - `Expr::Call` requires `splat_mask` and `kwargs_splat_mask` — use `call_expr()` helper
//! - Use `Literal::Int(i64)` for integer literals, NOT `Literal::Int64` (Issue #3194)

use crate::ir::core::{Expr, Literal};
use crate::span::Span;

/// Create a zero-span for synthetic IR nodes in tests.
///
/// `Span` has no `Default` impl — `Default::default()` type-infers
/// to the wrong type and causes confusing errors.
pub(crate) fn zero_span() -> Span {
    Span::new(0, 0, 0, 0, 0, 0)
}

/// Create an integer literal expression.
///
/// Uses `Literal::Int(i64)` — NOT `Literal::Int64` which does not exist.
pub(crate) fn int_lit(v: i64) -> Expr {
    Expr::Literal(Literal::Int(v), zero_span())
}

/// Create a variable expression.
pub(crate) fn var_expr(name: &str) -> Expr {
    Expr::Var(name.to_string(), zero_span())
}

/// Create a function call expression with no kwargs or splats.
///
/// `Expr::Call` requires `splat_mask: vec![]` and `kwargs_splat_mask: vec![]`
/// even for simple calls — this helper fills them in.
pub(crate) fn call_expr(function: &str, args: Vec<Expr>) -> Expr {
    Expr::Call {
        function: function.to_string(),
        args,
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span: zero_span(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_span_all_zeros() {
        let s = zero_span();
        assert_eq!(s.start, 0);
        assert_eq!(s.end, 0);
        assert_eq!(s.start_line, 0);
        assert_eq!(s.end_line, 0);
        assert_eq!(s.start_column, 0);
        assert_eq!(s.end_column, 0);
    }

    #[test]
    fn test_int_lit_constructs_literal_int() {
        let expr = int_lit(42);
        assert!(matches!(expr, Expr::Literal(Literal::Int(42), _)));
    }

    #[test]
    fn test_var_expr_constructs_var() {
        let expr = var_expr("x");
        assert!(matches!(expr, Expr::Var(ref name, _) if name == "x"));
    }

    #[test]
    fn test_call_expr_has_empty_splat_masks() {
        let expr = call_expr("f", vec![int_lit(1)]);
        assert!(matches!(
            expr,
            Expr::Call {
                ref function,
                ref splat_mask,
                ref kwargs_splat_mask,
                ..
            } if function == "f" && splat_mask.is_empty() && kwargs_splat_mask.is_empty()
        ));
    }
}
