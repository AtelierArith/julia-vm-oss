//! Call graph construction and analysis.
//!
//! This module builds a call graph from IR functions, tracking which functions
//! call which other functions. The call graph is used for topological sorting
//! and determining the order of type inference.

use crate::ir::core::{Block, Expr, Function, Stmt};
use std::collections::{HashMap, HashSet, VecDeque};

/// A node in the call graph representing a function.
#[derive(Debug, Clone)]
pub struct FuncNode {
    /// Function index in the program
    pub func_id: usize,
    /// Function name
    pub name: String,
    /// Indices of functions that call this function
    pub callers: Vec<usize>,
    /// Indices of functions that this function calls
    pub callees: Vec<usize>,
}

/// An edge in the call graph representing a function call.
#[derive(Debug, Clone)]
pub struct CallEdge {
    /// Index of the calling function
    pub caller: usize,
    /// Index of the called function
    pub callee: usize,
    /// Call site location (for future use)
    pub call_site: usize,
}

/// Call graph for interprocedural analysis.
///
/// The call graph tracks dependencies between functions,
/// enabling topological sorting and analysis ordering.
#[derive(Debug)]
pub struct CallGraph {
    /// Function nodes indexed by function ID
    pub nodes: Vec<FuncNode>,
    /// All call edges
    pub edges: Vec<CallEdge>,
    /// Map from function name to function ID
    name_to_id: HashMap<String, usize>,
}

impl CallGraph {
    /// Build a call graph from a list of functions.
    ///
    /// # Arguments
    ///
    /// * `functions` - The functions to analyze
    ///
    /// # Returns
    ///
    /// A call graph with nodes and edges.
    pub fn build_from_ir(functions: &[Function]) -> Self {
        let mut nodes = Vec::new();
        let mut name_to_id = HashMap::new();

        // Create nodes for all functions
        for (idx, func) in functions.iter().enumerate() {
            nodes.push(FuncNode {
                func_id: idx,
                name: func.name.clone(),
                callers: Vec::new(),
                callees: Vec::new(),
            });
            name_to_id.insert(func.name.clone(), idx);
        }

        let mut edges = Vec::new();

        // Scan each function body for calls
        for (caller_id, func) in functions.iter().enumerate() {
            let called_functions = extract_called_functions(&func.body);

            for called_name in called_functions {
                if let Some(&callee_id) = name_to_id.get(&called_name) {
                    // Add edge
                    edges.push(CallEdge {
                        caller: caller_id,
                        callee: callee_id,
                        call_site: 0, // Placeholder
                    });

                    // Update callers and callees
                    nodes[caller_id].callees.push(callee_id);
                    nodes[callee_id].callers.push(caller_id);
                }
            }
        }

        Self {
            nodes,
            edges,
            name_to_id,
        }
    }

    /// Get the function ID by name.
    pub fn get_function_id(&self, name: &str) -> Option<usize> {
        self.name_to_id.get(name).copied()
    }

    /// Get a function node by ID.
    pub fn get_node(&self, func_id: usize) -> Option<&FuncNode> {
        self.nodes.get(func_id)
    }

    /// Get all function IDs in topological order (leaves first).
    ///
    /// Functions with no dependencies come first, followed by functions
    /// that depend on them. This is useful for bottom-up analysis.
    ///
    /// Note: For cyclic graphs (recursive functions), this uses
    /// Kahn's algorithm and may not include all nodes in the result.
    /// Use `detect_sccs` for handling recursive function groups.
    pub fn topological_order(&self) -> Vec<usize> {
        let mut result = Vec::new();
        let mut in_degree: Vec<usize> = self.nodes.iter().map(|n| n.callers.len()).collect();
        let mut queue = VecDeque::new();

        // Start with leaf nodes (no callers)
        for (id, degree) in in_degree.iter().enumerate() {
            if *degree == 0 {
                queue.push_back(id);
            }
        }

        while let Some(func_id) = queue.pop_front() {
            result.push(func_id);

            // Process callees
            for &callee_id in &self.nodes[func_id].callees {
                in_degree[callee_id] = in_degree[callee_id].saturating_sub(1);
                if in_degree[callee_id] == 0 {
                    queue.push_back(callee_id);
                }
            }
        }

        result
    }

    /// Get all function IDs in reverse topological order (roots first).
    ///
    /// Functions with dependencies come first, followed by their
    /// dependencies. This is useful for top-down analysis.
    pub fn reverse_topological_order(&self) -> Vec<usize> {
        let mut result = self.topological_order();
        result.reverse();
        result
    }

    /// Check if a function is recursive (calls itself directly or indirectly).
    pub fn is_recursive(&self, func_id: usize) -> bool {
        let mut visited = HashSet::new();
        self.has_cycle_from(func_id, func_id, &mut visited)
    }

    /// Helper for recursion detection using DFS.
    fn has_cycle_from(&self, current: usize, target: usize, visited: &mut HashSet<usize>) -> bool {
        if !visited.insert(current) {
            return false; // Already visited
        }

        if let Some(node) = self.nodes.get(current) {
            for &callee_id in &node.callees {
                if callee_id == target {
                    return true; // Found cycle
                }
                if self.has_cycle_from(callee_id, target, visited) {
                    return true;
                }
            }
        }

        false
    }
}

/// Extract all function names called within a block of code.
fn extract_called_functions(block: &Block) -> HashSet<String> {
    let mut called = HashSet::new();
    for stmt in &block.stmts {
        extract_calls_from_stmt(stmt, &mut called);
    }
    called
}

/// Recursively extract function calls from a statement.
fn extract_calls_from_stmt(stmt: &Stmt, called: &mut HashSet<String>) {
    match stmt {
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            extract_calls_from_expr(value, called);
        }
        Stmt::Return { value, .. } => {
            if let Some(val_expr) = value {
                extract_calls_from_expr(val_expr, called);
            }
        }
        Stmt::Expr { expr, .. } => {
            extract_calls_from_expr(expr, called);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            extract_calls_from_expr(condition, called);
            extract_called_functions(then_branch)
                .into_iter()
                .for_each(|name| {
                    called.insert(name);
                });
            if let Some(else_blk) = else_branch {
                extract_called_functions(else_blk)
                    .into_iter()
                    .for_each(|name| {
                        called.insert(name);
                    });
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            extract_calls_from_expr(condition, called);
            extract_called_functions(body).into_iter().for_each(|name| {
                called.insert(name);
            });
        }
        Stmt::For {
            body,
            start,
            end,
            step,
            ..
        } => {
            extract_calls_from_expr(start, called);
            extract_calls_from_expr(end, called);
            if let Some(step_expr) = step {
                extract_calls_from_expr(step_expr, called);
            }
            extract_called_functions(body).into_iter().for_each(|name| {
                called.insert(name);
            });
        }
        Stmt::ForEach { iterable, body, .. } => {
            extract_calls_from_expr(iterable, called);
            extract_called_functions(body).into_iter().for_each(|name| {
                called.insert(name);
            });
        }
        Stmt::ForEachTuple { iterable, body, .. } => {
            extract_calls_from_expr(iterable, called);
            extract_called_functions(body).into_iter().for_each(|name| {
                called.insert(name);
            });
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {
            // No calls
        }
        Stmt::Block(block) => {
            extract_called_functions(block)
                .into_iter()
                .for_each(|name| {
                    called.insert(name);
                });
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            extract_called_functions(try_block)
                .into_iter()
                .for_each(|name| {
                    called.insert(name);
                });
            if let Some(catch_blk) = catch_block {
                extract_called_functions(catch_blk)
                    .into_iter()
                    .for_each(|name| {
                        called.insert(name);
                    });
            }
            if let Some(else_blk) = else_block {
                extract_called_functions(else_blk)
                    .into_iter()
                    .for_each(|name| {
                        called.insert(name);
                    });
            }
            if let Some(finally_blk) = finally_block {
                extract_called_functions(finally_blk)
                    .into_iter()
                    .for_each(|name| {
                        called.insert(name);
                    });
            }
        }
        Stmt::Timed { body, .. } => {
            extract_called_functions(body).into_iter().for_each(|name| {
                called.insert(name);
            });
        }
        Stmt::Test { condition, .. } => {
            extract_calls_from_expr(condition, called);
        }
        Stmt::TestSet { body, .. } => {
            extract_called_functions(body).into_iter().for_each(|name| {
                called.insert(name);
            });
        }
        Stmt::TestThrows { expr, .. } => {
            extract_calls_from_expr(expr, called);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            // array is a String (variable name), not an Expr
            for idx in indices {
                extract_calls_from_expr(idx, called);
            }
            extract_calls_from_expr(value, called);
        }
        Stmt::FieldAssign { value, .. } => {
            // object and field are Strings (variable/field names), not Exprs
            extract_calls_from_expr(value, called);
        }
        Stmt::DestructuringAssign { value, .. } => {
            // targets are Strings (variable names), not Exprs
            extract_calls_from_expr(value, called);
        }
        Stmt::DictAssign { key, value, .. } => {
            // dict is a String (variable name), not an Expr
            extract_calls_from_expr(key, called);
            extract_calls_from_expr(value, called);
        }
        Stmt::Using { .. } | Stmt::Export { .. } | Stmt::FunctionDef { .. } => {
            // These don't contain expression calls in the same sense
        }
        Stmt::Label { .. } | Stmt::Goto { .. } => {
            // Control flow labels/gotos don't contain function calls
        }
        Stmt::EnumDef { .. } => {
            // Enum definitions don't contain function calls
        }
    }
}

/// Recursively extract function calls from an expression.
fn extract_calls_from_expr(expr: &Expr, called: &mut HashSet<String>) {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            // Record the function call
            called.insert(function.clone());
            // Recursively check arguments
            for arg in args {
                extract_calls_from_expr(arg, called);
            }
            // Check keyword arguments
            for (_, arg_expr) in kwargs {
                extract_calls_from_expr(arg_expr, called);
            }
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            // Record the function call (including module path)
            called.insert(function.clone());
            for arg in args {
                extract_calls_from_expr(arg, called);
            }
            for (_, arg_expr) in kwargs {
                extract_calls_from_expr(arg_expr, called);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            extract_calls_from_expr(left, called);
            extract_calls_from_expr(right, called);
        }
        Expr::UnaryOp { operand, .. } => {
            extract_calls_from_expr(operand, called);
        }
        Expr::FieldAccess { object, .. } => {
            extract_calls_from_expr(object, called);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            extract_calls_from_expr(condition, called);
            extract_calls_from_expr(then_expr, called);
            extract_calls_from_expr(else_expr, called);
        }
        Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                extract_calls_from_expr(elem, called);
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for elem in elements {
                extract_calls_from_expr(elem, called);
            }
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, elem) in fields {
                extract_calls_from_expr(elem, called);
            }
        }
        Expr::Index { array, indices, .. } => {
            extract_calls_from_expr(array, called);
            for idx in indices {
                extract_calls_from_expr(idx, called);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            extract_calls_from_expr(start, called);
            if let Some(step_expr) = step {
                extract_calls_from_expr(step_expr, called);
            }
            extract_calls_from_expr(stop, called);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        } => {
            extract_calls_from_expr(body, called);
            extract_calls_from_expr(iter, called);
            if let Some(filter_expr) = filter {
                extract_calls_from_expr(filter_expr, called);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            extract_calls_from_expr(body, called);
            for (_, iter_expr) in iterations {
                extract_calls_from_expr(iter_expr, called);
            }
            if let Some(filter_expr) = filter {
                extract_calls_from_expr(filter_expr, called);
            }
        }
        Expr::Generator {
            body, iter, filter, ..
        } => {
            extract_calls_from_expr(body, called);
            extract_calls_from_expr(iter, called);
            if let Some(filter_expr) = filter {
                extract_calls_from_expr(filter_expr, called);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, val) in pairs {
                extract_calls_from_expr(key, called);
                extract_calls_from_expr(val, called);
            }
        }
        Expr::Pair { key, value, .. } => {
            extract_calls_from_expr(key, called);
            extract_calls_from_expr(value, called);
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, binding_expr) in bindings {
                extract_calls_from_expr(binding_expr, called);
            }
            extract_called_functions(body).into_iter().for_each(|name| {
                called.insert(name);
            });
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                extract_calls_from_expr(part, called);
            }
        }
        Expr::AssignExpr { value, .. } => {
            extract_calls_from_expr(value, called);
        }
        Expr::ReturnExpr { value, .. } => {
            if let Some(val_expr) = value {
                extract_calls_from_expr(val_expr, called);
            }
        }
        Expr::Builtin { args, .. } => {
            for arg in args {
                extract_calls_from_expr(arg, called);
            }
        }
        Expr::DynamicTypeConstruct { type_args, .. } => {
            // Type args may contain function calls like promote_type(T, S)
            for arg in type_args {
                extract_calls_from_expr(arg, called);
            }
        }
        // Literals, variables, and other simple expressions don't contain calls
        Expr::Literal(_, _)
        | Expr::Var(_, _)
        | Expr::FunctionRef { .. }
        | Expr::SliceAll { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. }
        | Expr::TypedEmptyArray { .. }
        | Expr::New { .. }
        | Expr::QuoteLiteral { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::Literal;
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn make_test_function_with_call(name: &str, called_name: &str) -> Function {
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
    fn test_build_simple_call_graph() {
        // f() = 42
        // g() = f()
        let functions = vec![
            make_simple_function("f"),
            make_test_function_with_call("g", "f"),
        ];

        let graph = CallGraph::build_from_ir(&functions);

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);

        // g calls f
        assert_eq!(graph.nodes[1].callees, vec![0]);
        assert_eq!(graph.nodes[0].callers, vec![1]);
    }

    #[test]
    fn test_topological_order() {
        // f() = 42
        // g() = f()
        // h() = g()
        let functions = vec![
            make_simple_function("f"),
            make_test_function_with_call("g", "f"),
            make_test_function_with_call("h", "g"),
        ];

        let graph = CallGraph::build_from_ir(&functions);
        let order = graph.topological_order();

        // Topological order processes nodes with no incoming edges first
        // In this case, h has no callers, so it comes first, then g, then f
        // (roots/top-level functions first, dependencies last)
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], 2); // h (no callers)
        assert_eq!(order[1], 1); // g (called by h)
        assert_eq!(order[2], 0); // f (called by g)
    }

    #[test]
    fn test_recursive_function_detection() {
        // f() = f() (direct recursion)
        let functions = vec![make_test_function_with_call("f", "f")];

        let graph = CallGraph::build_from_ir(&functions);

        assert!(graph.is_recursive(0));
    }

    #[test]
    fn test_get_function_id() {
        let functions = vec![make_simple_function("foo"), make_simple_function("bar")];

        let graph = CallGraph::build_from_ir(&functions);

        assert_eq!(graph.get_function_id("foo"), Some(0));
        assert_eq!(graph.get_function_id("bar"), Some(1));
        assert_eq!(graph.get_function_id("baz"), None);
    }
}
