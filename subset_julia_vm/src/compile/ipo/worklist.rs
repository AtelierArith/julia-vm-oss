//! Worklist-based interprocedural type inference.
//!
//! This module implements the core IPO inference algorithm using a worklist
//! approach. Functions are analyzed in dependency order, and callers are
//! re-analyzed when callee return types change.

use super::cache::InferenceCache;
use super::call_graph::CallGraph;
use super::recursion::{detect_sccs, is_recursive_scc};
use crate::compile::diagnostics::emit_fixed_point_divergence;
use crate::compile::lattice::types::LatticeType;
use crate::ir::core::Function;
use std::collections::{HashMap, HashSet};

/// IPO inference engine for analyzing functions.
///
/// The engine uses a worklist-based algorithm to propagate type information
/// across function boundaries, handling both non-recursive and recursive
/// function groups.
pub struct IPOInferenceEngine<'a> {
    /// The functions being analyzed
    functions: &'a [Function],
    /// Call graph for dependency tracking
    call_graph: CallGraph,
    /// Inferred return types for each function
    return_types: HashMap<usize, LatticeType>,
    /// Inference result cache
    cache: InferenceCache,
    /// Functions currently being processed (for cycle detection)
    in_progress: HashSet<usize>,
}

impl<'a> IPOInferenceEngine<'a> {
    /// Create a new IPO inference engine.
    ///
    /// # Arguments
    ///
    /// * `functions` - The functions to analyze
    pub fn new(functions: &'a [Function]) -> Self {
        let call_graph = CallGraph::build_from_ir(functions);

        Self {
            functions,
            call_graph,
            return_types: HashMap::new(),
            cache: InferenceCache::new(),
            in_progress: HashSet::new(),
        }
    }

    /// Run the full IPO inference algorithm.
    ///
    /// This analyzes all functions and propagates type information across
    /// function boundaries until a fixed point is reached.
    pub fn infer_all(&mut self) {
        // Detect SCCs for handling recursive functions
        let sccs = detect_sccs(&self.call_graph);

        // Process SCCs in reverse topological order (leaves first)
        for scc in sccs {
            if is_recursive_scc(&scc, &self.call_graph) {
                self.infer_recursive_group(&scc);
            } else {
                // Non-recursive: process each function once
                for &func_id in &scc {
                    self.infer_function(func_id);
                }
            }
        }
    }

    /// Infer the return type of a single function.
    ///
    /// # Arguments
    ///
    /// * `func_id` - The function ID to analyze
    ///
    /// # Returns
    ///
    /// true if the return type changed, false otherwise.
    fn infer_function(&mut self, func_id: usize) -> bool {
        // Check if already in progress (cycle detection)
        if self.in_progress.contains(&func_id) {
            // Return Top for recursive calls during analysis
            self.return_types.entry(func_id).or_insert(LatticeType::Top);
            return false;
        }

        // Check cache
        if let Some(cached_type) = self.cache.get(func_id, &[]) {
            let old_type = self.return_types.get(&func_id);
            if old_type != Some(cached_type) {
                self.return_types.insert(func_id, cached_type.clone());
                return true;
            }
            return false;
        }

        self.in_progress.insert(func_id);

        // Infer return type from function body
        let inferred_type = if func_id < self.functions.len() {
            self.infer_function_body(func_id)
        } else {
            LatticeType::Top
        };

        self.in_progress.remove(&func_id);

        // Check if return type changed
        let changed = match self.return_types.get(&func_id) {
            Some(old_type) if old_type != &inferred_type => true,
            None => true,
            _ => false,
        };

        // Update return type and cache
        self.return_types.insert(func_id, inferred_type.clone());
        self.cache.insert(func_id, vec![], inferred_type);

        changed
    }

    /// Infer return type from a function's body.
    ///
    /// This is a simplified implementation that returns Top.
    /// A full implementation would analyze the function body using
    /// abstract interpretation.
    fn infer_function_body(&self, func_id: usize) -> LatticeType {
        let func = &self.functions[func_id];

        // Check if function has a declared return type
        if func.return_type.is_some() {
            // For now, we just return Top
            // A full implementation would convert JuliaType to LatticeType
            return LatticeType::Top;
        }

        // Analyze function body
        // For now, return Top as a conservative approximation
        // A full implementation would use abstract interpretation
        LatticeType::Top
    }

    /// Infer return types for a recursive function group using fixed-point iteration.
    ///
    /// # Arguments
    ///
    /// * `scc` - The strongly connected component (recursive group)
    fn infer_recursive_group(&mut self, scc: &[usize]) {
        // Initialize all functions in the SCC with Bottom
        for &func_id in scc {
            self.return_types.insert(func_id, LatticeType::Bottom);
        }

        // Fixed-point iteration
        let max_iterations = 10; // Widening threshold
        for iteration in 0..max_iterations {
            let mut changed = false;

            for &func_id in scc {
                if self.infer_function(func_id) {
                    changed = true;
                }
            }

            if !changed {
                break; // Reached fixed point
            }

            // Widening: after N iterations, widen to Top to ensure termination
            if iteration >= max_iterations - 2 {
                // Emit diagnostic for fixed-point divergence
                let func_names: Vec<String> = scc
                    .iter()
                    .filter_map(|&id| self.functions.get(id).map(|f| f.name.clone()))
                    .collect();
                emit_fixed_point_divergence(iteration, &func_names);
                for &func_id in scc {
                    self.return_types.insert(func_id, LatticeType::Top);
                }
                break;
            }
        }
    }

    /// Get the inferred return types for all functions.
    pub fn get_return_types(&self) -> HashMap<usize, LatticeType> {
        self.return_types.clone()
    }

    /// Get the inferred return type for a specific function.
    pub fn get_return_type(&self, func_id: usize) -> Option<&LatticeType> {
        self.return_types.get(&func_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Expr, Literal, Stmt};
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn make_simple_function(name: &str) -> Function {
        Function {
            name: name.to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Literal(Literal::Int(42), dummy_span())),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        }
    }

    #[test]
    fn test_infer_simple_function() {
        // f() = 42
        let functions = vec![make_simple_function("f")];

        let mut engine = IPOInferenceEngine::new(&functions);
        engine.infer_all();

        let return_types = engine.get_return_types();
        assert_eq!(return_types.len(), 1);
        // Return type should be inferred (Top for simplified implementation)
        assert!(return_types.contains_key(&0));
    }

    #[test]
    fn test_infer_multiple_functions() {
        // f() = 42
        // g() = 3.14
        let functions = vec![make_simple_function("f"), make_simple_function("g")];

        let mut engine = IPOInferenceEngine::new(&functions);
        engine.infer_all();

        let return_types = engine.get_return_types();
        assert_eq!(return_types.len(), 2);
    }

    #[test]
    fn test_get_return_type() {
        let functions = vec![make_simple_function("f")];

        let mut engine = IPOInferenceEngine::new(&functions);
        engine.infer_all();

        let return_type = engine.get_return_type(0);
        assert!(return_type.is_some());
    }
}
