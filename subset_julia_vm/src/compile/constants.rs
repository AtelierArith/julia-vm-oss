//! Math constants and helper functions for the compiler.
//!
//! This module provides utilities for handling Julia math constants
//! like π, ℯ, and other special values.

use crate::ir::core::Function;

/// Check if name is π (pi)
pub(super) fn is_pi_name(name: &str) -> bool {
    matches!(name, "pi" | "\u{03C0}")
}

/// Check if name is Euler's number ℯ
pub(super) fn is_euler_name(name: &str) -> bool {
    // ℯ is U+212F (SCRIPT SMALL E), Julia's Euler constant
    matches!(name, "\u{212F}")
}

/// Check if a name is a MathConstants constant and return its value
pub(super) fn get_math_constant_value(name: &str) -> Option<f64> {
    match name {
        // π and pi (π is U+03C0)
        "π" | "pi" => Some(std::f64::consts::PI),
        // ℯ and e (Euler's number, ℯ is U+212F)
        "ℯ" | "e" => Some(std::f64::consts::E),
        // φ and golden (golden ratio, φ is U+03C6)
        "φ" | "golden" => Some((1.0 + 5.0_f64.sqrt()) / 2.0),
        // γ and eulergamma (Euler-Mascheroni constant, γ is U+03B3)
        "γ" | "eulergamma" => Some(0.5772156649015329),
        // Catalan's constant
        "catalan" => Some(0.915_965_594_177_219),
        // IEEE 754 special values
        "NaN" => Some(f64::NAN),
        "Inf" => Some(f64::INFINITY),
        _ => None,
    }
}

/// Check if a name is a MathConstants export
pub(super) fn is_math_constant(name: &str) -> bool {
    get_math_constant_value(name).is_some()
}

/// Get the value of a constant exported from Base module.
/// Only a subset of MathConstants are exported from Base:
/// - pi, π (pi)
/// - ℯ (Euler's number, U+212F - but NOT ascii 'e')
/// - Inf, NaN
pub(super) fn get_base_exported_constant_value(name: &str) -> Option<f64> {
    match name {
        "π" | "pi" => Some(std::f64::consts::PI),
        "ℯ" => Some(std::f64::consts::E), // Only Unicode ℯ, NOT ascii 'e'
        "NaN" => Some(f64::NAN),
        "Inf" => Some(f64::INFINITY),
        _ => None,
    }
}

/// Check if a function needs Lazy AoT specialization.
/// A function needs specialization if:
/// 1. It has parameters without type annotations, AND
/// 2. It's not just an intrinsic wrapper (Core.Intrinsics.xxx call)
pub fn needs_specialization(func: &Function) -> bool {
    // Must have untyped parameters to benefit from specialization
    let has_untyped_params = func.params.iter().any(|p| p.type_annotation.is_none());
    if !has_untyped_params {
        return false;
    }

    // Exclude functions that are just Core.Intrinsics wrappers
    // These are already optimal and don't benefit from specialization
    if is_intrinsic_wrapper(func) {
        return false;
    }

    true
}

/// Check if a function is just a thin wrapper around Core.Intrinsics.
/// These functions don't benefit from specialization.
fn is_intrinsic_wrapper(func: &Function) -> bool {
    use crate::ir::core::{Expr, Stmt};

    // A function is an intrinsic wrapper if:
    // 1. Body has exactly one statement that is a return of a call
    // 2. The call is to Core.Intrinsics.xxx (ModuleCall to Core)

    if func.body.stmts.len() != 1 {
        return false;
    }

    // Check if the statement is a ModuleCall to Core.Intrinsics
    match &func.body.stmts[0] {
        Stmt::Expr { expr, .. }
        | Stmt::Return {
            value: Some(expr), ..
        } => {
            matches!(expr, Expr::ModuleCall { module, .. } if module == "Core" || module.starts_with("Core."))
        }
        _ => false,
    }
}

/// Check if a module name is part of Julia's standard library
pub(super) fn is_stdlib_module(name: &str) -> bool {
    matches!(
        name,
        "Base"
            | "Core"
            | "Main"
            | "LinearAlgebra"
            | "Statistics"
            | "Random"
            | "Dates"
            | "Printf"
            | "Test"
            | "SparseArrays"
            | "Distributed"
            | "SharedArrays"
            | "Serialization"
            | "REPL"
            | "InteractiveUtils"
            | "Pkg"
            | "Markdown"
            | "UUIDs"
            | "Sockets"
            | "DelimitedFiles"
            | "FileWatching"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Expr, Function, Stmt, TypedParam};
    use crate::span::Span;

    fn span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    // === is_pi_name ===

    #[test]
    fn test_is_pi_name_ascii() {
        assert!(is_pi_name("pi"));
    }

    #[test]
    fn test_is_pi_name_unicode() {
        assert!(is_pi_name("\u{03C0}")); // π
    }

    #[test]
    fn test_is_pi_name_rejects_other() {
        assert!(!is_pi_name("PI"));
        assert!(!is_pi_name("Pi"));
        assert!(!is_pi_name("e"));
    }

    // === is_euler_name ===

    #[test]
    fn test_is_euler_name_unicode() {
        assert!(is_euler_name("\u{212F}")); // ℯ
    }

    #[test]
    fn test_is_euler_name_rejects_ascii_e() {
        assert!(!is_euler_name("e"));
        assert!(!is_euler_name("E"));
    }

    // === get_math_constant_value ===

    #[test]
    fn test_math_constant_pi() {
        let val = get_math_constant_value("pi");
        assert_eq!(val, Some(std::f64::consts::PI));
        assert_eq!(get_math_constant_value("\u{03C0}"), Some(std::f64::consts::PI));
    }

    #[test]
    fn test_math_constant_e() {
        assert_eq!(get_math_constant_value("e"), Some(std::f64::consts::E));
        assert_eq!(get_math_constant_value("\u{212F}"), Some(std::f64::consts::E));
    }

    #[test]
    fn test_math_constant_golden_ratio() {
        let expected = (1.0 + 5.0_f64.sqrt()) / 2.0;
        assert_eq!(get_math_constant_value("\u{03C6}"), Some(expected));
        assert_eq!(get_math_constant_value("golden"), Some(expected));
    }

    #[test]
    fn test_math_constant_nan() {
        let val = get_math_constant_value("NaN");
        assert!(val.is_some());
        assert!(val.unwrap().is_nan()); // NaN != NaN, so use is_nan()
    }

    #[test]
    fn test_math_constant_inf() {
        assert_eq!(get_math_constant_value("Inf"), Some(f64::INFINITY));
    }

    #[test]
    fn test_math_constant_unknown() {
        assert_eq!(get_math_constant_value("tau"), None);
        assert_eq!(get_math_constant_value(""), None);
    }

    // === is_math_constant ===

    #[test]
    fn test_is_math_constant_known() {
        assert!(is_math_constant("pi"));
        assert!(is_math_constant("NaN"));
        assert!(is_math_constant("catalan"));
    }

    #[test]
    fn test_is_math_constant_unknown() {
        assert!(!is_math_constant("tau"));
        assert!(!is_math_constant("x"));
    }

    // === get_base_exported_constant_value ===

    #[test]
    fn test_base_exported_pi() {
        assert_eq!(
            get_base_exported_constant_value("pi"),
            Some(std::f64::consts::PI)
        );
    }

    #[test]
    fn test_base_exported_euler_unicode_only() {
        // Unicode ℯ IS exported from Base
        assert_eq!(
            get_base_exported_constant_value("\u{212F}"),
            Some(std::f64::consts::E)
        );
        // ASCII 'e' is NOT exported from Base
        assert_eq!(get_base_exported_constant_value("e"), None);
    }

    #[test]
    fn test_base_exported_golden_not_exported() {
        // golden/φ are in MathConstants but NOT exported from Base
        assert_eq!(get_base_exported_constant_value("golden"), None);
        assert_eq!(get_base_exported_constant_value("\u{03C6}"), None);
    }

    // === is_stdlib_module ===

    #[test]
    fn test_is_stdlib_module_core_modules() {
        assert!(is_stdlib_module("Base"));
        assert!(is_stdlib_module("Core"));
        assert!(is_stdlib_module("Main"));
    }

    #[test]
    fn test_is_stdlib_module_standard_libraries() {
        assert!(is_stdlib_module("LinearAlgebra"));
        assert!(is_stdlib_module("Statistics"));
        assert!(is_stdlib_module("Random"));
        assert!(is_stdlib_module("Printf"));
        assert!(is_stdlib_module("Test"));
    }

    #[test]
    fn test_is_stdlib_module_rejects_unknown() {
        assert!(!is_stdlib_module("MyModule"));
        assert!(!is_stdlib_module("base")); // case-sensitive
        assert!(!is_stdlib_module(""));
    }

    // === needs_specialization ===

    fn make_func(name: &str, params: Vec<TypedParam>, body: Vec<Stmt>) -> Function {
        Function {
            name: name.to_string(),
            params,
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: body,
                span: span(),
            },
            is_base_extension: false,
            span: span(),
        }
    }

    #[test]
    fn test_needs_specialization_untyped_params() {
        // f(x) = x + 1 → needs specialization (untyped param, not intrinsic)
        let func = make_func(
            "f",
            vec![TypedParam::untyped("x".to_string(), span())],
            vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string(), span())),
                span: span(),
            }],
        );
        assert!(needs_specialization(&func));
    }

    #[test]
    fn test_needs_specialization_all_typed_params() {
        // f(x::Int64) → does NOT need specialization (all params typed)
        let func = make_func(
            "f",
            vec![TypedParam {
                name: "x".to_string(),
                type_annotation: Some(crate::types::JuliaType::Int64),
                is_varargs: false,
                vararg_count: None,
                span: span(),
            }],
            vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string(), span())),
                span: span(),
            }],
        );
        assert!(!needs_specialization(&func));
    }

    #[test]
    fn test_needs_specialization_no_params() {
        // f() → does NOT need specialization (no untyped params)
        let func = make_func("f", vec![], vec![]);
        assert!(!needs_specialization(&func));
    }

    #[test]
    fn test_needs_specialization_intrinsic_wrapper() {
        // f(x) = Core.Intrinsics.neg_int(x) → does NOT need specialization
        let func = make_func(
            "neg_int",
            vec![TypedParam::untyped("x".to_string(), span())],
            vec![Stmt::Return {
                value: Some(Expr::ModuleCall {
                    module: "Core".to_string(),
                    function: "neg_int".to_string(),
                    args: vec![Expr::Var("x".to_string(), span())],
                    kwargs: vec![],
                    span: span(),
                }),
                span: span(),
            }],
        );
        assert!(!needs_specialization(&func));
    }
}
