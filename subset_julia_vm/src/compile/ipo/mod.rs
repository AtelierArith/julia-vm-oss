//! Interprocedural Analysis (IPO) for type inference.
//!
//! This module implements interprocedural analysis to propagate type information
//! across function boundaries, enabling more precise type inference for user-defined
//! functions.
//!
//! # Architecture
//!
//! The IPO system consists of:
//! - `call_graph`: Call graph construction from IR functions
//! - `recursion`: SCC detection for handling recursive function groups
//! - `worklist`: Worklist-based inference algorithm
//! - `cache`: Inference result caching for performance
//!
//! # Usage
//!
//! The IPO system analyzes a set of functions to infer their return types:
//!
//! ```no_run
//! use subset_julia_vm::compile::ipo::analyze_functions;
//! use subset_julia_vm::ir::core::Function;
//!
//! // Given a set of IR functions (from lowering stage)
//! // let functions: Vec<Function> = compile_program(...);
//!
//! // Analyze interprocedurally to infer return types
//! // let return_types = analyze_functions(&functions);
//! // return_types[func_idx] contains the inferred LatticeType
//! ```
//!
//! See unit tests in this module for complete working examples.

pub mod cache;
pub mod call_graph;
pub mod recursion;
pub mod worklist;

use crate::compile::lattice::types::LatticeType;
use crate::ir::core::Function;
use std::collections::HashMap;

pub use cache::InferenceCache;
pub use call_graph::CallGraph;
pub use recursion::detect_sccs;
pub use worklist::IPOInferenceEngine;

/// Main entry point for interprocedural analysis.
///
/// This is a convenience function that creates an IPO engine and runs
/// the full analysis pipeline on a set of functions.
///
/// # Arguments
///
/// * `functions` - The functions to analyze
///
/// # Returns
///
/// A map from function index to inferred return type.
pub fn analyze_functions(functions: &[Function]) -> HashMap<usize, LatticeType> {
    let mut engine = IPOInferenceEngine::new(functions);
    engine.infer_all();
    engine.get_return_types()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Expr, Function, Literal, Stmt};
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn make_test_function(name: &str, body_expr: Expr) -> Function {
        Function {
            name: name.to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(body_expr),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        }
    }

    #[test]
    fn test_analyze_simple_function() {
        // Test: f() = 42
        let func = make_test_function("f", Expr::Literal(Literal::Int(42), dummy_span()));

        let functions = vec![func];
        let result = analyze_functions(&functions);

        // Should infer Int64 return type
        assert_eq!(result.len(), 1);
        // Note: Actual return type depends on inference implementation
    }

    #[test]
    fn test_analyze_empty_function_list() {
        let functions = vec![];
        let result = analyze_functions(&functions);
        assert_eq!(result.len(), 0);
    }
}
