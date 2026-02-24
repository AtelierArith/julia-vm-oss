//! Effect propagation through function calls and call graphs.
//!
//! This module implements propagation of effects across function boundaries,
//! using a worklist-based fixpoint algorithm.

use super::Effects;
use crate::ir::core::{Block, Expr, Function, Program, Stmt};
use std::collections::{HashMap, HashSet, VecDeque};

/// Function identifier for call graph tracking
pub type FuncId = String;

/// Call graph node representing a function and its callees
#[derive(Debug, Clone)]
pub struct CallGraphNode {
    pub func_id: FuncId,
    pub callees: HashSet<FuncId>,
}

/// Call graph for effect propagation
#[derive(Debug, Clone)]
pub struct CallGraph {
    pub nodes: HashMap<FuncId, CallGraphNode>,
}

impl CallGraph {
    /// Create a new empty call graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Add a function to the call graph
    pub fn add_function(&mut self, func_id: FuncId) {
        self.nodes.entry(func_id.clone()).or_insert(CallGraphNode {
            func_id,
            callees: HashSet::new(),
        });
    }

    /// Add a call edge from caller to callee
    pub fn add_call(&mut self, caller: &FuncId, callee: &FuncId) {
        self.add_function(caller.clone());
        self.add_function(callee.clone());
        if let Some(node) = self.nodes.get_mut(caller) {
            node.callees.insert(callee.clone());
        }
    }

    /// Get callees of a function
    pub fn get_callees(&self, func_id: &FuncId) -> Option<&HashSet<FuncId>> {
        self.nodes.get(func_id).map(|node| &node.callees)
    }

    /// Build call graph from a program
    pub fn from_program(program: &Program) -> Self {
        let mut graph = CallGraph::new();

        // Add all functions to the graph
        for func in &program.functions {
            graph.add_function(func.name.clone());
        }

        // Analyze function bodies for calls
        for func in &program.functions {
            let callees = extract_callees_from_block(&func.body);
            for callee in callees {
                graph.add_call(&func.name, &callee);
            }
        }

        graph
    }
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract function calls from a block
fn extract_callees_from_block(block: &Block) -> HashSet<FuncId> {
    let mut callees = HashSet::new();
    for stmt in &block.stmts {
        callees.extend(extract_callees_from_stmt(stmt));
    }
    callees
}

/// Extract function calls from a statement
fn extract_callees_from_stmt(stmt: &Stmt) -> HashSet<FuncId> {
    match stmt {
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Expr { expr: value, .. } => extract_callees_from_expr(value),
        Stmt::For {
            body,
            start,
            end,
            step,
            ..
        } => {
            let mut callees = extract_callees_from_expr(start);
            callees.extend(extract_callees_from_expr(end));
            if let Some(step_expr) = step {
                callees.extend(extract_callees_from_expr(step_expr));
            }
            callees.extend(extract_callees_from_block(body));
            callees
        }
        Stmt::ForEach { body, iterable, .. } | Stmt::ForEachTuple { body, iterable, .. } => {
            let mut callees = extract_callees_from_expr(iterable);
            callees.extend(extract_callees_from_block(body));
            callees
        }
        Stmt::While {
            condition, body, ..
        } => {
            let mut callees = extract_callees_from_expr(condition);
            callees.extend(extract_callees_from_block(body));
            callees
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let mut callees = extract_callees_from_expr(condition);
            callees.extend(extract_callees_from_block(then_branch));
            if let Some(else_b) = else_branch {
                callees.extend(extract_callees_from_block(else_b));
            }
            callees
        }
        Stmt::Return { value, .. } => {
            if let Some(val) = value {
                extract_callees_from_expr(val)
            } else {
                HashSet::new()
            }
        }
        Stmt::Block(block) => extract_callees_from_block(block),
        Stmt::Try {
            try_block,
            catch_block,
            finally_block,
            ..
        } => {
            let mut callees = extract_callees_from_block(try_block);
            if let Some(catch_b) = catch_block {
                callees.extend(extract_callees_from_block(catch_b));
            }
            if let Some(finally_b) = finally_block {
                callees.extend(extract_callees_from_block(finally_b));
            }
            callees
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => extract_callees_from_block(body),
        _ => HashSet::new(),
    }
}

/// Extract function calls from an expression
fn extract_callees_from_expr(expr: &Expr) -> HashSet<FuncId> {
    match expr {
        Expr::Call { function, args, .. } => {
            let mut callees = HashSet::new();
            callees.insert(function.clone());
            for arg in args {
                callees.extend(extract_callees_from_expr(arg));
            }
            callees
        }
        Expr::Builtin { args, .. } => {
            let mut callees = HashSet::new();
            for arg in args {
                callees.extend(extract_callees_from_expr(arg));
            }
            callees
        }
        Expr::BinaryOp { left, right, .. } => {
            let mut callees = extract_callees_from_expr(left);
            callees.extend(extract_callees_from_expr(right));
            callees
        }
        Expr::UnaryOp { operand, .. } => extract_callees_from_expr(operand),
        Expr::ArrayLiteral { elements, .. } => {
            let mut callees = HashSet::new();
            for elem in elements {
                callees.extend(extract_callees_from_expr(elem));
            }
            callees
        }
        Expr::TupleLiteral { elements, .. } => {
            let mut callees = HashSet::new();
            for elem in elements {
                callees.extend(extract_callees_from_expr(elem));
            }
            callees
        }
        Expr::NamedTupleLiteral { fields, .. } => {
            let mut callees = HashSet::new();
            for (_, field_expr) in fields {
                callees.extend(extract_callees_from_expr(field_expr));
            }
            callees
        }
        Expr::Range {
            start, stop, step, ..
        } => {
            let mut callees = extract_callees_from_expr(start);
            callees.extend(extract_callees_from_expr(stop));
            if let Some(step_expr) = step {
                callees.extend(extract_callees_from_expr(step_expr));
            }
            callees
        }
        Expr::Index { array, indices, .. } => {
            let mut callees = extract_callees_from_expr(array);
            for idx in indices {
                callees.extend(extract_callees_from_expr(idx));
            }
            callees
        }
        Expr::FieldAccess { object, .. } => extract_callees_from_expr(object),
        Expr::Comprehension { body, .. } | Expr::MultiComprehension { body, .. } => {
            extract_callees_from_expr(body)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            let mut callees = extract_callees_from_expr(condition);
            callees.extend(extract_callees_from_expr(then_expr));
            callees.extend(extract_callees_from_expr(else_expr));
            callees
        }
        Expr::StringConcat { parts, .. } => {
            let mut callees = HashSet::new();
            for part in parts {
                callees.extend(extract_callees_from_expr(part));
            }
            callees
        }
        Expr::AssignExpr { value, .. } => extract_callees_from_expr(value),
        _ => HashSet::new(),
    }
}

/// Propagate effects through the call graph using fixpoint iteration.
///
/// This implements a worklist-based algorithm:
/// 1. Initialize all functions with conservative (arbitrary) effects
/// 2. Compute effects for each function based on its body and callee effects
/// 3. If a function's effects change, add its callers to the worklist
/// 4. Repeat until no changes (fixpoint reached)
pub fn propagate_effects(
    call_graph: &CallGraph,
    functions: &[Function],
) -> HashMap<FuncId, Effects> {
    let mut effects_map: HashMap<FuncId, Effects> = HashMap::new();
    let mut worklist: VecDeque<FuncId> = VecDeque::new();

    // Initialize all functions with arbitrary effects and add to worklist
    for func in functions {
        effects_map.insert(func.name.clone(), Effects::arbitrary());
        worklist.push_back(func.name.clone());
    }

    // Build reverse call graph (callers map)
    let mut callers: HashMap<FuncId, HashSet<FuncId>> = HashMap::new();
    for (caller, node) in &call_graph.nodes {
        for callee in &node.callees {
            callers
                .entry(callee.clone())
                .or_default()
                .insert(caller.clone());
        }
    }

    // Fixpoint iteration
    let mut iteration = 0;
    const MAX_ITERATIONS: usize = 100; // Prevent infinite loops

    while let Some(func_id) = worklist.pop_front() {
        iteration += 1;
        if iteration > MAX_ITERATIONS {
            // Safety: prevent infinite loops in case of bugs
            break;
        }

        // Find the function
        let func = match functions.iter().find(|f| f.name == func_id) {
            Some(f) => f,
            None => continue,
        };

        // Compute new effects based on function body
        let new_effects = compute_function_effects(func, &effects_map);

        // Check if effects changed
        let old_effects = effects_map.get(&func_id).copied().unwrap_or_default();
        if new_effects != old_effects {
            effects_map.insert(func_id.clone(), new_effects);

            // Add callers to worklist (they need recomputation)
            if let Some(caller_set) = callers.get(&func_id) {
                for caller in caller_set {
                    if !worklist.contains(caller) {
                        worklist.push_back(caller.clone());
                    }
                }
            }
        }
    }

    effects_map
}

/// Compute effects for a single function based on its body and callee effects.
fn compute_function_effects(func: &Function, effects_map: &HashMap<FuncId, Effects>) -> Effects {
    compute_block_effects(&func.body, effects_map)
}

/// Compute effects for a block of statements.
fn compute_block_effects(block: &Block, effects_map: &HashMap<FuncId, Effects>) -> Effects {
    let mut result = Effects::total();
    for stmt in &block.stmts {
        result = result.merge(&compute_stmt_effects(stmt, effects_map));
    }
    result
}

/// Compute effects for a statement.
fn compute_stmt_effects(stmt: &Stmt, effects_map: &HashMap<FuncId, Effects>) -> Effects {
    match stmt {
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Expr { expr: value, .. } => compute_expr_effects(value, effects_map),
        Stmt::For {
            body,
            start,
            end,
            step,
            ..
        } => {
            let mut eff = compute_expr_effects(start, effects_map);
            eff = eff.merge(&compute_expr_effects(end, effects_map));
            if let Some(step_expr) = step {
                eff = eff.merge(&compute_expr_effects(step_expr, effects_map));
            }
            eff = eff.merge(&compute_block_effects(body, effects_map));
            // Loops may not terminate
            Effects {
                terminates: false,
                ..eff
            }
        }
        Stmt::ForEach { body, iterable, .. } | Stmt::ForEachTuple { body, iterable, .. } => {
            let mut eff = compute_expr_effects(iterable, effects_map);
            eff = eff.merge(&compute_block_effects(body, effects_map));
            // Loops may not terminate
            Effects {
                terminates: false,
                ..eff
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            let mut eff = compute_expr_effects(condition, effects_map);
            eff = eff.merge(&compute_block_effects(body, effects_map));
            // Loops may not terminate
            Effects {
                terminates: false,
                ..eff
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let mut eff = compute_expr_effects(condition, effects_map);
            eff = eff.merge(&compute_block_effects(then_branch, effects_map));
            if let Some(else_b) = else_branch {
                eff = eff.merge(&compute_block_effects(else_b, effects_map));
            }
            eff
        }
        Stmt::Return { value, .. } => {
            if let Some(val) = value {
                compute_expr_effects(val, effects_map)
            } else {
                Effects::total()
            }
        }
        Stmt::Block(block) => compute_block_effects(block, effects_map),
        Stmt::Try {
            try_block,
            catch_block,
            finally_block,
            ..
        } => {
            let mut eff = compute_block_effects(try_block, effects_map);
            if let Some(catch_b) = catch_block {
                eff = eff.merge(&compute_block_effects(catch_b, effects_map));
            }
            if let Some(finally_b) = finally_block {
                eff = eff.merge(&compute_block_effects(finally_b, effects_map));
            }
            eff
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => {
            compute_block_effects(body, effects_map)
        }
        _ => Effects::total(),
    }
}

/// Compute effects for an expression, looking up callee effects.
fn compute_expr_effects(expr: &Expr, effects_map: &HashMap<FuncId, Effects>) -> Effects {
    match expr {
        Expr::Call { function, args, .. } => {
            // Get callee effects from map
            let callee_effects = effects_map
                .get(function)
                .copied()
                .unwrap_or_else(Effects::arbitrary);

            // Merge with argument effects
            let mut result = callee_effects;
            for arg in args {
                result = result.merge(&compute_expr_effects(arg, effects_map));
            }
            result
        }
        _ => {
            // Use inference for non-call expressions
            super::inference::infer_expr_effects(expr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Expr, Function, Literal, Program, Stmt};
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn test_call_graph_construction() {
        let mut graph = CallGraph::new();
        graph.add_function("main".to_string());
        graph.add_function("helper".to_string());
        graph.add_call(&"main".to_string(), &"helper".to_string());

        assert!(graph.nodes.contains_key("main"));
        assert!(graph.nodes.contains_key("helper"));
        assert!(graph
            .get_callees(&"main".to_string())
            .unwrap()
            .contains("helper"));
    }

    #[test]
    fn test_extract_callees_from_expr() {
        let expr = Expr::Call {
            function: "foo".to_string(),
            args: vec![Expr::Call {
                function: "bar".to_string(),
                args: vec![],
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span: dummy_span(),
            }],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        };

        let callees = extract_callees_from_expr(&expr);
        assert_eq!(callees.len(), 2);
        assert!(callees.contains("foo"));
        assert!(callees.contains("bar"));
    }

    #[test]
    fn test_propagate_effects_simple() {
        // Create a simple program with two functions
        let pure_func = Function {
            name: "pure".to_string(),
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
        };

        let caller_func = Function {
            name: "caller".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "pure".to_string(),
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
        };

        let program = Program {
            abstract_types: vec![],
            type_aliases: vec![],
            functions: vec![pure_func, caller_func],
            base_function_count: 0,
            structs: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: Block {
                stmts: vec![],
                span: dummy_span(),
            },
        };

        let call_graph = CallGraph::from_program(&program);
        let effects_map = propagate_effects(&call_graph, &program.functions);

        // Both functions should have computed effects
        assert!(effects_map.contains_key("pure"));
        assert!(effects_map.contains_key("caller"));

        // Pure function should be foldable
        let pure_effects = effects_map.get("pure").unwrap();
        assert!(pure_effects.is_foldable());
    }
}
