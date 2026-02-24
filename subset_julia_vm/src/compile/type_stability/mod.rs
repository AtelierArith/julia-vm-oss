//! Type stability analysis for SubsetJuliaVM.
//!
//! This module provides tools for analyzing the type stability of Julia functions.
//! A function is type-stable if its return type can be uniquely determined from
//! the types of its input arguments.
//!
//! # Type Stability Definition
//!
//! A function `f(x::T1, y::T2, ...)` is **type-stable** if:
//! - Its return type is `LatticeType::Concrete` or `LatticeType::Const`
//! - Its return type is NOT `Top` (Any) or `Union`
//!
//! Type stability is important for performance in Julia because it enables
//! the compiler to generate efficient specialized code.
//!
//! # Usage
//!
//! The type stability analyzer works with compiled programs. Here's a conceptual
//! example of the typical workflow:
//!
//! ```no_run
//! use subset_julia_vm::compile::type_stability::{
//!     TypeStabilityAnalyzer, OutputFormat, format_report
//! };
//!
//! // The analyzer requires a compiled Program (from the lowering stage)
//! // let program = compile_julia_source("function f(x::Int64) x + 1 end");
//!
//! // Create analyzer and analyze the program
//! // let mut analyzer = TypeStabilityAnalyzer::new();
//! // let report = analyzer.analyze_program(&program);
//!
//! // Format and print results
//! // println!("{}", format_report(&report, OutputFormat::Text).unwrap());
//! ```
//!
//! For a complete working example, see the unit tests in this module or use
//! the public API function `analyze_type_stability()` from the crate root.
//!
//! # Module Structure
//!
//! - `reason`: Reasons why a function might be type-unstable
//! - `report`: Individual function stability reports
//! - `analysis_report`: Overall analysis report for a program
//! - `analyzer`: The core analysis logic
//! - `output`: Formatters for text and JSON output

pub mod analysis_report;
pub mod analyzer;
pub mod output;
pub mod reason;
pub mod report;

// Re-export main types for convenience
pub use analysis_report::{AnalysisSummary, TypeStabilityAnalysisReport};
pub use analyzer::{AnalysisConfig, TypeStabilityAnalyzer};
pub use output::{format_json_report, format_report, format_text_report, OutputFormat};
pub use reason::TypeStabilityReason;
pub use report::{FunctionStabilityReport, StabilityStatus};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::{ConcreteType, LatticeType};
    use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Stmt, TypedParam};
    use crate::span::Span;
    use crate::types::JuliaType;

    fn create_span() -> Span {
        Span::new(0, 10, 1, 1, 0, 10)
    }

    fn create_function(name: &str, params: Vec<TypedParam>, return_expr: Expr) -> Function {
        Function {
            name: name.to_string(),
            params,
            kwparams: vec![],
            type_params: vec![],
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(return_expr),
                    span: create_span(),
                }],
                span: create_span(),
            },
            return_type: None,
            is_base_extension: false,
            span: create_span(),
        }
    }

    #[test]
    fn test_stable_function_with_typed_params() {
        // f(x::Int64)::Int64 = x * 2
        let func = create_function(
            "double",
            vec![TypedParam::new(
                "x".to_string(),
                Some(JuliaType::Int64),
                create_span(),
            )],
            Expr::BinaryOp {
                op: BinaryOp::Mul,
                left: Box::new(Expr::Var("x".to_string(), create_span())),
                right: Box::new(Expr::Literal(Literal::Int(2), create_span())),
                span: create_span(),
            },
        );

        let mut analyzer = TypeStabilityAnalyzer::new();
        let report = analyzer.analyze_function(&func);

        assert!(
            report.is_stable(),
            "Expected stable but got {:?}",
            report.status
        );
        assert_eq!(report.function_name, "double");
        assert!(report.reasons.is_empty());
    }

    #[test]
    fn test_unstable_function_returns_any() {
        // f(x) = x (untyped parameter, returns Any)
        let func = create_function(
            "identity",
            vec![TypedParam::new(
                "x".to_string(),
                None, // Untyped
                create_span(),
            )],
            Expr::Var("x".to_string(), create_span()),
        );

        let mut analyzer = TypeStabilityAnalyzer::new();
        let report = analyzer.analyze_function(&func);

        assert!(
            report.is_unstable(),
            "Expected unstable but got {:?}",
            report.status
        );
        assert!(!report.reasons.is_empty());
    }

    #[test]
    fn test_format_text_output() {
        // Create an unstable function (returns Any due to untyped parameter)
        let func = create_function(
            "test_func",
            vec![TypedParam::new(
                "x".to_string(),
                None, // Untyped parameter makes it unstable
                create_span(),
            )],
            Expr::Var("x".to_string(), create_span()),
        );

        let mut analyzer = TypeStabilityAnalyzer::new();
        let func_report = analyzer.analyze_function(&func);

        let mut analysis_report = TypeStabilityAnalysisReport::new();
        analysis_report.add_function(func_report);

        let text = format_text_report(&analysis_report);
        assert!(text.contains("Type Stability Analysis Report"));
        // Unstable functions are listed in the output
        assert!(text.contains("test_func"));
    }

    #[test]
    fn test_format_json_output() {
        let analysis_report = TypeStabilityAnalysisReport::new();
        let json = format_json_report(&analysis_report).unwrap();

        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"functions\""));
    }

    #[test]
    fn test_analysis_summary() {
        let mut report = TypeStabilityAnalysisReport::new();

        // Add a stable function
        report.add_function(FunctionStabilityReport::new(
            "stable_func".to_string(),
            1,
            vec![],
            LatticeType::Concrete(ConcreteType::Int64),
        ));

        // Add an unstable function
        report.add_function(FunctionStabilityReport::new(
            "unstable_func".to_string(),
            10,
            vec![],
            LatticeType::Top,
        ));

        assert_eq!(report.summary.total_functions, 2);
        assert_eq!(report.summary.stable_count, 1);
        assert_eq!(report.summary.unstable_count, 1);
        assert_eq!(report.summary.stable_percentage(), 50.0);
    }
}
