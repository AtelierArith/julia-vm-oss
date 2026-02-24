//! Strongly Connected Components (SCC) detection for recursive function handling.
//!
//! This module implements Tarjan's algorithm for detecting strongly connected
//! components in the call graph, which correspond to mutually recursive function groups.

use super::call_graph::CallGraph;
use std::collections::HashSet;

/// Detect strongly connected components (SCCs) in the call graph.
///
/// Each SCC represents a group of mutually recursive functions that must be
/// analyzed together using fixed-point iteration.
///
/// # Arguments
///
/// * `call_graph` - The call graph to analyze
///
/// # Returns
///
/// A vector of SCCs, where each SCC is a vector of function IDs.
/// SCCs are returned in reverse topological order (dependencies first).
///
/// # Example
///
/// The `detect_sccs` function takes a `CallGraph` built from IR functions:
///
/// ```no_run
/// use subset_julia_vm::compile::ipo::CallGraph;
/// use subset_julia_vm::compile::ipo::detect_sccs;
///
/// // Build a call graph from IR functions (requires Function objects)
/// // let call_graph = CallGraph::build_from_ir(&functions);
///
/// // Detect strongly connected components
/// // let sccs = detect_sccs(&call_graph);
///
/// // Process SCCs in reverse topological order
/// // for scc in sccs {
/// //     if scc.len() > 1 {
/// //         // Mutually recursive group - needs fixed-point iteration
/// //         println!("Recursive group: {:?}", scc);
/// //     } else {
/// //         // Single function (may still be self-recursive)
/// //         println!("Function: {}", scc[0]);
/// //     }
/// // }
/// ```
///
/// See unit tests in the `ipo` module for complete working examples.
pub fn detect_sccs(call_graph: &CallGraph) -> Vec<Vec<usize>> {
    let mut detector = TarjanSCCDetector::new(call_graph.nodes.len());

    for func_id in 0..call_graph.nodes.len() {
        if detector.index[func_id].is_none() {
            detector.strongconnect(func_id, call_graph);
        }
    }

    detector.sccs
}

/// Tarjan's SCC detection algorithm state.
struct TarjanSCCDetector {
    /// Current index counter
    current_index: usize,
    /// Index assigned to each node (None if not visited)
    index: Vec<Option<usize>>,
    /// Lowlink value for each node
    lowlink: Vec<usize>,
    /// Stack of nodes being processed
    stack: Vec<usize>,
    /// Set of nodes currently on the stack
    on_stack: HashSet<usize>,
    /// Detected SCCs
    sccs: Vec<Vec<usize>>,
}

impl TarjanSCCDetector {
    fn new(size: usize) -> Self {
        Self {
            current_index: 0,
            index: vec![None; size],
            lowlink: vec![0; size],
            stack: Vec::new(),
            on_stack: HashSet::new(),
            sccs: Vec::new(),
        }
    }

    fn strongconnect(&mut self, v: usize, call_graph: &CallGraph) {
        // Set the depth index for v to the smallest unused index
        self.index[v] = Some(self.current_index);
        self.lowlink[v] = self.current_index;
        self.current_index += 1;
        self.stack.push(v);
        self.on_stack.insert(v);

        // Consider successors of v (callees)
        if let Some(node) = call_graph.get_node(v) {
            for &w in &node.callees {
                if self.index[w].is_none() {
                    // Successor w has not yet been visited; recurse on it
                    self.strongconnect(w, call_graph);
                    self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
                } else if self.on_stack.contains(&w) {
                    // Successor w is in stack and hence in the current SCC
                    self.lowlink[v] = self.lowlink[v].min(self.index[w].unwrap());
                }
            }
        }

        // If v is a root node, pop the stack and create an SCC
        if self.lowlink[v] == self.index[v].unwrap() {
            let mut scc = Vec::new();
            loop {
                let w = self.stack.pop().unwrap();
                self.on_stack.remove(&w);
                scc.push(w);
                if w == v {
                    break;
                }
            }
            // Reverse to maintain original order within SCC
            scc.reverse();
            self.sccs.push(scc);
        }
    }
}

/// Check if an SCC represents a recursive group.
///
/// An SCC is recursive if:
/// - It contains more than one function (mutually recursive), or
/// - It contains a single function that calls itself (self-recursive)
pub fn is_recursive_scc(scc: &[usize], call_graph: &CallGraph) -> bool {
    if scc.len() > 1 {
        return true; // Mutually recursive
    }

    if scc.len() == 1 {
        // Check for self-recursion
        let func_id = scc[0];
        return call_graph.is_recursive(func_id);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Expr, Function, Literal, Stmt};
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

    fn make_function_with_call(name: &str, called_name: &str) -> Function {
        Function {
            name: name.to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: called_name.to_string(),
                        args: vec![],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        }
    }

    #[test]
    fn test_detect_no_recursion() {
        // f() = 42
        // g() = f()
        let functions = vec![make_simple_function("f"), make_function_with_call("g", "f")];

        let graph = CallGraph::build_from_ir(&functions);
        let sccs = detect_sccs(&graph);

        // Should have 2 SCCs, each with 1 function
        assert_eq!(sccs.len(), 2);
        assert!(sccs.iter().all(|scc| scc.len() == 1));
    }

    #[test]
    fn test_detect_self_recursion() {
        // f() = f()
        let functions = vec![make_function_with_call("f", "f")];

        let graph = CallGraph::build_from_ir(&functions);
        let sccs = detect_sccs(&graph);

        // Should have 1 SCC with 1 function
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 1);
        assert!(is_recursive_scc(&sccs[0], &graph));
    }

    #[test]
    fn test_detect_mutual_recursion() {
        // f() = g()
        // g() = f()
        let functions = vec![
            make_function_with_call("f", "g"),
            make_function_with_call("g", "f"),
        ];

        let graph = CallGraph::build_from_ir(&functions);
        let sccs = detect_sccs(&graph);

        // Should have 1 SCC with 2 functions
        assert_eq!(sccs.len(), 1);
        assert_eq!(sccs[0].len(), 2);
        assert!(is_recursive_scc(&sccs[0], &graph));
    }

    #[test]
    fn test_complex_graph() {
        // a() = 42
        // b() = a()
        // c() = b()
        // d() = c()
        // e() = d() and e()  (self-recursive)
        let functions = vec![
            make_simple_function("a"),
            make_function_with_call("b", "a"),
            make_function_with_call("c", "b"),
            make_function_with_call("d", "c"),
            make_function_with_call("e", "e"),
        ];

        let graph = CallGraph::build_from_ir(&functions);
        let sccs = detect_sccs(&graph);

        // Should have 5 SCCs (one for each function)
        assert_eq!(sccs.len(), 5);
    }
}
