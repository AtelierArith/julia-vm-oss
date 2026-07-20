//! Effect propagation through function calls and call graphs.
//!
//! This module implements propagation of effects across function boundaries,
//! using a worklist-based fixpoint algorithm.

use super::Effects;
use crate::compile::abstract_interp::engine::MethodKey;
use crate::ir::core::{Block, Expr, Function, Program, Stmt};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
pub(super) use subset_julia_vm_types::runtime_types::function_effects::compute_function_effects;
pub use subset_julia_vm_types::runtime_types::function_effects::FuncId;

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
        Self::from_program_slice(&program.functions)
    }

    /// Build call graph from an arbitrary function slice (e.g. the non-Base
    /// suffix of `program.functions`). Callees that do not appear in the slice
    /// are silently omitted from the graph; the fixpoint will treat them as
    /// `Effects::arbitrary()` — the conservative fallback (Issue #9150).
    pub fn from_program_slice(functions: &[Arc<Function>]) -> Self {
        let mut graph = CallGraph::new();

        // Add all functions to the graph
        for func in functions {
            graph.add_function(func.name.clone());
        }

        // Analyze function bodies for calls
        for func in functions {
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
        | Stmt::DestructuringAssign { value, .. }
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
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            let mut callees = HashSet::new();
            callees.insert(function.to_string());
            for arg in args {
                callees.extend(extract_callees_from_expr(arg));
            }
            for (_, value) in kwargs {
                callees.extend(extract_callees_from_expr(value));
            }
            callees
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            let mut callees = HashSet::new();
            callees.insert(function.to_string());
            for arg in args {
                callees.extend(extract_callees_from_expr(arg));
            }
            for (_, value) in kwargs {
                callees.extend(extract_callees_from_expr(value));
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
        Expr::LetBlock { bindings, body, .. } => {
            let mut callees = HashSet::new();
            for (_, value) in bindings {
                callees.extend(extract_callees_from_expr(value));
            }
            callees.extend(extract_callees_from_block(body));
            callees
        }
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
    functions: &[Arc<Function>],
) -> HashMap<FuncId, Effects> {
    let mut effects_map: HashMap<FuncId, Effects> = HashMap::new();
    let mut worklist: VecDeque<FuncId> = VecDeque::new();
    // O(1) membership test for the worklist — replaces the O(n) VecDeque::contains
    // calls that made each caller-push scan the whole queue (Issue #9150).
    let mut in_worklist: HashSet<FuncId> = HashSet::new();
    let mut functions_by_name: HashMap<FuncId, Vec<&Function>> = HashMap::new();

    // Initialize all functions with arbitrary effects and add to worklist
    for func in functions {
        functions_by_name
            .entry(func.name.clone())
            .or_default()
            .push(func.as_ref());
        if effects_map
            .insert(func.name.clone(), Effects::arbitrary())
            .is_none()
        {
            in_worklist.insert(func.name.clone());
            worklist.push_back(func.name.clone());
        }
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

    // Fixpoint iteration.
    //
    // Defensive bound only: `Effects::merge` is monotone (a summary only
    // degrades), so each function's summary changes at most a lattice-height
    // number of times and the worklist drains on its own. The bound must
    // scale with the program: a flat cap of 100 silently left every function
    // past the first 100 pops at the `Effects::arbitrary()` seed on real
    // programs (Base alone is ~5k functions), which made the whole-program
    // summaries useless outside unit tests — found when wiring them into the
    // SSA pipeline gate (Issue #8440).
    let mut iteration = 0;
    let max_iterations = functions.len().saturating_mul(64).max(100);

    while let Some(func_id) = worklist.pop_front() {
        in_worklist.remove(&func_id);
        iteration += 1;
        if iteration > max_iterations {
            // Safety valve: prevent runaway iteration in case of bugs.
            debug_assert!(
                false,
                "effect propagation exceeded {max_iterations} worklist pops (Issue #8441)"
            );
            break;
        }

        // Find all methods for this function name. Julia dispatch can select any
        // matching method at runtime, so the name-level summary must be the
        // conservative merge of every method body with that name.
        let funcs = match functions_by_name.get(&func_id) {
            Some(funcs) => funcs,
            None => continue,
        };

        // Compute new effects based on all method bodies for this function name.
        let new_effects = compute_method_set_effects(funcs, &effects_map);

        // Check if effects changed
        let old_effects = effects_map.get(&func_id).copied().unwrap_or_default();
        if new_effects != old_effects {
            effects_map.insert(func_id.clone(), new_effects);

            // Add callers to worklist (they need recomputation).
            // Use the O(1) HashSet sentinel to skip duplicates without scanning
            // the whole queue (the old worklist.contains() was O(n) — Issue #9150).
            if let Some(caller_set) = callers.get(&func_id) {
                for caller in caller_set {
                    if in_worklist.insert(caller.clone()) {
                        worklist.push_back(caller.clone());
                    }
                }
            }
        }
    }

    effects_map
}

/// Infer effect summaries for all functions in a program.
///
/// This is the public entry point for body-derived effect inference: build the
/// call graph from the source IR, then run the fixpoint propagation pass.
///
/// Only non-Base functions are analysed (module/package functions + user
/// functions, i.e. `program.functions[base_function_count..]`). The SSA
/// pipeline only gates user functions, and Base callees that aren't in the
/// slice default to `Effects::arbitrary()` (the conservative fallback that the
/// opt passes already use for unknown callees). Restricting the slice avoids
/// running the fixpoint on the ~5 000-function Base corpus, which was the
/// dominant cost when the worklist-contains check was O(n) (Issue #9150).
pub fn infer_program_effects(program: &Program) -> HashMap<FuncId, Effects> {
    let total = program.functions.len();
    let base_count = program.base_function_count.min(total);
    let non_base = &program.functions[base_count..];
    let call_graph = CallGraph::from_program_slice(non_base);
    propagate_effects(&call_graph, non_base)
}

/// Whole-program effect summaries keyed two ways (Issue #9205).
///
/// * `by_name` is the conservative merge of every method sharing a name —
///   the sound *multi-candidate dispatch fallback*. It is byte-identical to
///   what [`infer_program_effects`] returns, and is what the SSA opt passes
///   (`ssa_ir::opt`) and reflection consume today: a call by name can dispatch
///   to any method with that name at runtime, so the merge is the only sound
///   summary when the target is not statically resolved.
/// * `by_method` keys each summary by the specific method
///   ([`MethodKey`], reused from the inference engine, Issue #8553), so a pure
///   `f(::Int)` is not tainted by an impure `f(::IO)` sibling. This mirrors
///   upstream, which stores `ipo_effects` per `CodeInstance`
///   (`julia/Compiler/src/typeinfer.jl`), not per generic function. A call site
///   that has *statically resolved* dispatch to a unique method can consult
///   `by_method` for that method's precise summary; ambiguous sites keep using
///   `by_name`. Wiring that resolution into the SSA call sites is the tracked
///   follow-up — this type establishes the per-method storage the issue's
///   acceptance criterion 1 requires.
#[derive(Debug)]
pub(crate) struct EffectSummaries {
    pub(crate) by_name: HashMap<FuncId, Effects>,
    pub(crate) by_method: HashMap<MethodKey, Effects>,
}

/// Like [`infer_program_effects`], but additionally returns a per-method map
/// keyed by [`MethodKey`]. The name-level map is produced by the same fixpoint
/// (unchanged), then each method's own summary is computed once against the
/// converged name-level callee information — precise for the method's own body
/// while keeping the (sound) name-level merge for its callees, whose dispatch
/// is likewise unresolved here (Issue #9205).
pub(crate) fn infer_program_effects_per_method(program: &Program) -> EffectSummaries {
    let total = program.functions.len();
    let base_count = program.base_function_count.min(total);
    let non_base = &program.functions[base_count..];
    let call_graph = CallGraph::from_program_slice(non_base);
    let by_name = propagate_effects(&call_graph, non_base);
    let by_method = compute_per_method_summaries(non_base, &by_name);
    EffectSummaries { by_name, by_method }
}

/// Compute each method's own effect summary against the converged name-level
/// callee map. Methods that share a [`MethodKey`] (a redefinition with an
/// identical canonical signature) collapse last-wins, matching how a
/// redefinition replaces a method slot upstream.
fn compute_per_method_summaries(
    functions: &[Arc<Function>],
    name_level: &HashMap<FuncId, Effects>,
) -> HashMap<MethodKey, Effects> {
    let mut by_method = HashMap::with_capacity(functions.len());
    for func in functions {
        let effects = compute_function_effects(func, name_level);
        by_method.insert(MethodKey::from_function(func), effects);
    }
    by_method
}

/// How much per-method precision the name-level merge currently hides
/// (Issue #9205 acceptance criterion 3 / #9129 invariant 4). A method counts as
/// a "recoverable" opportunity when its own summary proves a DCE/CSE-relevant
/// property (`is_foldable` / `is_removable`) that the name-level merge for the
/// same name does not — i.e. a statically-resolved call to that method could be
/// folded/removed but a name-keyed call cannot. `methods` is the total number
/// of per-method summaries considered.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PerMethodPrecisionStats {
    pub(crate) methods: usize,
    pub(crate) foldable_recovered: usize,
    pub(crate) removable_recovered: usize,
}

/// Count methods whose per-method summary is strictly more precise than the
/// name-level merge for their name (see [`PerMethodPrecisionStats`]).
pub(crate) fn per_method_precision_stats(summaries: &EffectSummaries) -> PerMethodPrecisionStats {
    let mut stats = PerMethodPrecisionStats {
        methods: summaries.by_method.len(),
        ..Default::default()
    };
    for (key, method_effects) in &summaries.by_method {
        let Some(name_effects) = summaries.by_name.get(key.function()) else {
            continue;
        };
        if method_effects.is_foldable() && !name_effects.is_foldable() {
            stats.foldable_recovered += 1;
        }
        if method_effects.is_removable() && !name_effects.is_removable() {
            stats.removable_recovered += 1;
        }
    }
    stats
}

/// Whether per-method effect precision measurement is requested for the current
/// compile (Issue #9205 acceptance criterion 3). Off by default so the hot
/// compile path keeps computing only the name-level map; set
/// `SJULIA_EFFECTS_STATS=1` to also build the per-method map and log the
/// DCE/CSE opportunity it exposes.
pub(crate) fn effect_stats_logging_enabled() -> bool {
    std::env::var_os("SJULIA_EFFECTS_STATS").is_some()
}

/// Emit the per-method vs name-level precision delta for one gated program
/// compile to stderr (see [`per_method_precision_stats`]). The crate denies
/// `clippy::print_stderr`; like the SSA pipeline gate log (Issue #8552), this
/// opt-in diagnostic writes through `std::io::stderr()` directly.
pub(crate) fn log_per_method_precision_stats(summaries: &EffectSummaries) {
    use std::io::Write;
    let stats = per_method_precision_stats(summaries);
    let _ = writeln!(
        std::io::stderr(),
        "[effects] per-method summaries: {} methods; \
         foldable recoverable by dispatch resolution: {}; \
         removable recoverable: {} (Issue #9205)",
        stats.methods,
        stats.foldable_recovered,
        stats.removable_recovered
    );
}

/// Compute the conservative effect summary for all methods with one function name.
fn compute_method_set_effects(
    funcs: &[&Function],
    effects_map: &HashMap<FuncId, Effects>,
) -> Effects {
    let mut iter = funcs.iter();
    let Some(first) = iter.next() else {
        return Effects::arbitrary();
    };

    let mut effects = compute_function_effects(first, effects_map);
    for func in iter {
        effects = effects.merge(&compute_function_effects(func, effects_map));
    }
    effects
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::core::{BinaryOp, Block, Expr, Function, Literal, Program, Stmt};
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn function(name: &str, stmt: Stmt) -> Function {
        Function {
            name: name.to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![stmt],
                span: dummy_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        }
    }

    fn program(functions: Vec<Function>) -> Program {
        Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            functions: functions.into_iter().map(Arc::new).collect(),
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
        }
    }

    fn return_expr(expr: Expr) -> Stmt {
        Stmt::Return {
            value: Some(expr),
            span: dummy_span(),
        }
    }

    fn call(function: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            function: function.to_string().into(),
            args,
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        }
    }

    fn module_call(module: &str, function: &str, args: Vec<Expr>) -> Expr {
        Expr::ModuleCall {
            module: module.to_string().into(),
            function: function.to_string().into(),
            args,
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        }
    }

    fn call_with_kwargs(function: &str, kwargs: Vec<(crate::ir::core::InternedStr, Expr)>) -> Expr {
        Expr::Call {
            function: function.to_string().into(),
            args: vec![],
            kwargs,
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        }
    }

    fn let_block(bindings: Vec<(crate::ir::core::InternedStr, Expr)>, body: Vec<Stmt>) -> Expr {
        Expr::LetBlock {
            bindings,
            body: Block {
                stmts: body,
                span: dummy_span(),
            },
            span: dummy_span(),
        }
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
            function: "foo".to_string().into(),
            args: vec![Expr::Call {
                function: "bar".to_string().into(),
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
    fn module_call_edges_recompute_callers_issue_8441() {
        let caller = function(
            "caller",
            return_expr(module_call("Base", "qualified_foldable", vec![])),
        );
        let callee = function(
            "qualified_foldable",
            return_expr(Expr::Literal(Literal::Int(1), dummy_span())),
        );
        let program = program(vec![caller, callee]);
        let effects_map = infer_program_effects(&program);

        assert!(effects_map["qualified_foldable"].is_foldable());
        assert!(effects_map["caller"].is_foldable());
    }

    #[test]
    fn keyword_value_call_edges_recompute_callers_issue_8441() {
        let outer = function(
            "outer",
            return_expr(Expr::Literal(Literal::Int(1), dummy_span())),
        );
        let caller = function(
            "caller",
            return_expr(call_with_kwargs(
                "outer",
                vec![("kw".to_string().into(), call("kw_foldable", vec![]))],
            )),
        );
        let kw_value = function(
            "kw_foldable",
            return_expr(Expr::Literal(Literal::Int(2), dummy_span())),
        );
        let program = program(vec![outer, caller, kw_value]);
        let effects_map = infer_program_effects(&program);

        assert!(effects_map["outer"].is_foldable());
        assert!(effects_map["kw_foldable"].is_foldable());
        assert!(effects_map["caller"].is_foldable());
    }

    #[test]
    fn keyword_value_let_blocks_compose_body_effects_issue_8441() {
        let outer = function(
            "outer",
            return_expr(Expr::Literal(Literal::Int(1), dummy_span())),
        );
        let caller = function(
            "caller",
            return_expr(call_with_kwargs(
                "outer",
                vec![(
                    "kw".to_string().into(),
                    let_block(
                        vec![(
                            "tmp".to_string().into(),
                            Expr::Literal(Literal::Int(2), dummy_span()),
                        )],
                        vec![Stmt::Expr {
                            expr: Expr::Var("tmp".to_string().into(), dummy_span()),
                            span: dummy_span(),
                        }],
                    ),
                )],
            )),
        );
        let program = program(vec![outer, caller]);
        let effects_map = infer_program_effects(&program);

        assert!(effects_map["outer"].is_foldable());
        assert!(effects_map["caller"].is_foldable());
    }

    #[test]
    fn let_block_edges_recompute_callers_issue_8441() {
        let caller = function(
            "caller",
            return_expr(let_block(
                vec![("tmp".to_string().into(), call("let_foldable", vec![]))],
                vec![Stmt::Return {
                    value: Some(call("let_body_foldable", vec![])),
                    span: dummy_span(),
                }],
            )),
        );
        let binding_callee = function(
            "let_foldable",
            return_expr(Expr::Literal(Literal::Int(1), dummy_span())),
        );
        let body_callee = function(
            "let_body_foldable",
            return_expr(Expr::Literal(Literal::Int(2), dummy_span())),
        );
        let program = program(vec![caller, binding_callee, body_callee]);

        let call_graph = CallGraph::from_program(&program);
        let callees = call_graph.get_callees(&"caller".to_string()).unwrap();
        assert!(callees.contains("let_foldable"));
        assert!(callees.contains("let_body_foldable"));

        let effects_map = infer_program_effects(&program);
        assert!(effects_map["let_foldable"].is_foldable());
        assert!(effects_map["let_body_foldable"].is_foldable());
        assert!(effects_map["caller"].is_foldable());
    }

    /// True when `body` proves every effect property that `minimum` proves
    /// (tri-state bits: an AlwaysTrue minimum requires an AlwaysTrue body
    /// bit; bool bits: a true minimum requires a true body bit).
    fn body_at_least_as_precise(minimum: &Effects, body: &Effects) -> bool {
        (!minimum.consistent.is_always_true() || body.consistent.is_always_true())
            && (!minimum.effect_free.is_always_true() || body.effect_free.is_always_true())
            && (!minimum.nothrow || body.nothrow)
            && (!minimum.terminates || body.terminates)
            && (!minimum.notaskstate || body.notaskstate)
            && (!minimum.inaccessiblememonly || body.inaccessiblememonly)
            // Tri-state (Issue #9496): same "if minimum proves AlwaysTrue, body
            // must too" shape as consistent/effect_free above.
            && (!minimum.noub.is_always_true() || body.noub.is_always_true())
            && (!minimum.nonoverlayed || body.nonoverlayed)
            && (!minimum.nortcall || body.nortcall)
    }

    #[test]
    fn retired_effect_hints_are_body_provable_over_base_issue_8441() {
        // Trip-wire for the fixed-name-hint retirement (Issue #8441): every
        // entry removed from the `infer_builtin_effects` table must remain
        // provable from the Base method bodies alone — the whole-program
        // fixpoint summary for the name has to be at least as precise as the
        // record the retired entry used to assert. If a Base body change
        // weakens one of these summaries, re-evaluate that retirement (restore
        // the table entry or fix the body) instead of weakening this test.
        let program = crate::base_loader::get_base_program().expect("base program must lower");
        let effects_map = infer_program_effects(program);
        let retired: &[(&str, Effects)] = &[
            ("!==", Effects::pure_arithmetic()),
            ("ifelse", Effects::pure_arithmetic()),
            ("tuple", Effects::pure_arithmetic()),
        ];
        for (name, minimum) in retired {
            let body = effects_map
                .get(*name)
                .unwrap_or_else(|| panic!("{name}: no Base method summary found"));
            assert!(
                body_at_least_as_precise(minimum, body),
                "{name}: body-derived summary {body:?} no longer proves the retired \
                 table record {minimum:?}"
            );
        }
    }

    /// Measurement gate (Issue #9496 — "gate on a measured DCE/CSE hit-rate
    /// improvement… do not add lattice complexity without a demonstrated
    /// fold/removal win"). Quantifies, over the whole Base corpus (a fixed
    /// fixture corpus), the `is_foldable()` precision delta from wiring the
    /// `noub` tri-state into the formula: the pre-#9496 formula never
    /// consulted `noub` at all, so it is replicated locally here and diffed
    /// against the post-#9496 formula.
    ///
    /// Expected (and asserted) result: **zero regressions**. The only preset
    /// whose `noub` the pre-#9496 formula silently ignored while still
    /// passing `is_foldable()` was `array_getindex` (`consistent`/
    /// `effect_free`/`terminates`/`inaccessiblememonly` all already true); its
    /// `noub` is reclassified from `AlwaysFalse` to `Conditional`
    /// (`NOUB_IF_NOINBOUNDS`-equivalent) in the same change, which the new
    /// gate accepts — so no Base method that used to fold stops folding. This
    /// is an honest zero-measured-improvement result for the *current* Base
    /// corpus (see the PR description for the exact counts this test prints
    /// with `SJULIA_EFFECTS_STATS=1` set); the value delivered is a
    /// correctly-grounded (upstream-cited) representation plus the
    /// discharge/absorption machinery a *future* refinement (e.g. threading
    /// `CoreCompiler::is_proven_inbounds_index` into SSA-level effects) can
    /// build on without re-deriving the tri-state from scratch.
    #[test]
    fn noub_gate_does_not_regress_base_foldability_issue_9496() {
        fn is_foldable_pre_9496(e: &Effects) -> bool {
            e.consistent.is_always_true()
                && e.effect_free.is_always_true()
                && e.terminates
                && e.inaccessiblememonly
        }

        let program = crate::base_loader::get_base_program().expect("base program must lower");
        let effects_map = infer_program_effects(program);

        let mut regressed: Vec<&FuncId> = Vec::new();
        let mut newly_foldable = 0usize;
        let mut conditional_noub = 0usize;
        for (name, effects) in &effects_map {
            let was_foldable = is_foldable_pre_9496(effects);
            let is_foldable_now = effects.is_foldable();
            if was_foldable && !is_foldable_now {
                regressed.push(name);
            }
            if !was_foldable && is_foldable_now {
                newly_foldable += 1;
            }
            if effects.noub.is_conditional() {
                conditional_noub += 1;
            }
        }

        if effect_stats_logging_enabled() {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[effects#9496] Base corpus: {} name-level summaries; \
                 is_foldable() regressions from the noub gate: {} {:?}; \
                 newly-foldable from the noub gate: {}; \
                 summaries with Conditional (NOUB_IF_NOINBOUNDS-equivalent) noub: {}",
                effects_map.len(),
                regressed.len(),
                regressed,
                newly_foldable,
                conditional_noub,
            );
        }

        assert!(
            regressed.is_empty(),
            "noub gate regressed is_foldable() for Base names: {regressed:?} — the \
             Conditional reclassification of array_getindex/array_setindex/Expr::Index \
             must compensate every existing AlwaysFalse noub site (Issue #9496)"
        );
    }

    #[test]
    fn nested_call_operands_compose_body_derived_summaries_issue_8441() {
        // caller() = nested_foldable() + 1 — the callee sits inside a binary
        // operand, not in direct-call position. Its body-derived summary must
        // still reach the caller through the walker (it previously fell back
        // to the name table, which knows nothing about user functions).
        let callee = function(
            "nested_foldable",
            return_expr(Expr::Literal(Literal::Int(1), dummy_span())),
        );
        let caller = function(
            "caller",
            return_expr(Expr::BinaryOp {
                op: BinaryOp::Add,
                left: Box::new(call("nested_foldable", vec![])),
                right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                span: dummy_span(),
            }),
        );
        let program = program(vec![callee, caller]);
        let effects_map = infer_program_effects(&program);

        assert!(effects_map["nested_foldable"].is_foldable());
        assert!(effects_map["caller"].is_foldable());
    }

    #[test]
    fn nested_effectful_call_operands_taint_the_caller_issue_8441() {
        // caller() = nested_effectful() + 1 — a side-effecting callee in
        // operand position must taint the caller summary through the same
        // path (no optimistic fallback).
        let callee = function(
            "nested_effectful",
            return_expr(call(
                "throw",
                vec![Expr::Literal(
                    Literal::Str("boom".to_string()),
                    dummy_span(),
                )],
            )),
        );
        let caller = function(
            "caller",
            return_expr(Expr::BinaryOp {
                op: BinaryOp::Add,
                left: Box::new(call("nested_effectful", vec![])),
                right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                span: dummy_span(),
            }),
        );
        let program = program(vec![callee, caller]);
        let effects_map = infer_program_effects(&program);

        assert!(!effects_map["nested_effectful"].is_foldable());
        assert!(!effects_map["caller"].is_foldable());
        assert!(!effects_map["caller"].nothrow);
    }

    #[test]
    fn nested_call_without_summary_falls_back_to_name_table_issue_8441() {
        // caller(x) = string(x) + 1 — `string` has no body in this program,
        // so the nested-call walk falls back to the curated builtin name
        // table entry (pure), preserving pre-#8441 behavior for names that
        // remain in the table.
        let caller = function(
            "caller",
            return_expr(Expr::BinaryOp {
                op: BinaryOp::Add,
                left: Box::new(call(
                    "string",
                    vec![Expr::Literal(Literal::Int(1), dummy_span())],
                )),
                right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                span: dummy_span(),
            }),
        );
        let program = program(vec![caller]);
        let effects_map = infer_program_effects(&program);

        assert!(effects_map["caller"].is_foldable());
    }

    #[test]
    fn ternary_and_short_circuit_missing_callee_match_if_else_terminates_issue_10368() {
        // `x ? println() : 1`, `x && println()`, and `x || println()` must
        // resolve `println` (a callee absent from this program's
        // `effects_map`, mirroring a Base Rust builtin excluded from the
        // whole-program propagation slice) with the SAME conservative
        // fallback (`Effects::arbitrary()`) as an equivalent if/else branch
        // — not the optimistic curated-name-table classification
        // (`with_side_effects()`, which over-claims `terminates = true`,
        // among other bits; Issue #10368).
        let if_else_fn = function(
            "if_else_form",
            Stmt::If {
                condition: Expr::var("x", dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Expr {
                        expr: call("println", vec![]),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: None,
                span: dummy_span(),
            },
        );

        let ternary_fn = function(
            "ternary_form",
            Stmt::Expr {
                expr: Expr::Ternary {
                    condition: Box::new(Expr::var("x", dummy_span())),
                    then_expr: Box::new(call("println", vec![])),
                    else_expr: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                    span: dummy_span(),
                },
                span: dummy_span(),
            },
        );

        let and_fn = function(
            "and_form",
            Stmt::Expr {
                expr: Expr::BinaryOp {
                    op: BinaryOp::And,
                    left: Box::new(Expr::var("x", dummy_span())),
                    right: Box::new(call("println", vec![])),
                    span: dummy_span(),
                },
                span: dummy_span(),
            },
        );

        let or_fn = function(
            "or_form",
            Stmt::Expr {
                expr: Expr::BinaryOp {
                    op: BinaryOp::Or,
                    left: Box::new(Expr::var("x", dummy_span())),
                    right: Box::new(call("println", vec![])),
                    span: dummy_span(),
                },
                span: dummy_span(),
            },
        );

        let program = program(vec![if_else_fn, ternary_fn, and_fn, or_fn]);
        let effects_map = infer_program_effects(&program);

        // `if_else_form` is the correct-by-construction baseline: `println`
        // has no summary, so it must taint the caller conservatively.
        assert!(!effects_map["if_else_form"].terminates);

        // Ternary/short-circuit must land on the exact same `Effects` as the
        // if/else control — not merely agree on `terminates` in isolation.
        assert_eq!(effects_map["ternary_form"], effects_map["if_else_form"]);
        assert_eq!(effects_map["and_form"], effects_map["if_else_form"]);
        assert_eq!(effects_map["or_form"], effects_map["if_else_form"]);
    }

    #[test]
    fn fixpoint_budget_scales_with_program_size_issue_8441() {
        // A call chain longer than the old fixed 100-pop budget: f0 calls f1,
        // f1 calls f2, ..., f_{N-1} returns a literal. Every function is
        // foldable, but proving f0 requires the fixpoint to keep running well
        // past 100 worklist pops. With the old fixed cap the head of the
        // chain silently stayed at the conservative seed.
        const CHAIN: usize = 300;
        let mut functions = Vec::with_capacity(CHAIN);
        for i in 0..CHAIN {
            let body = if i + 1 == CHAIN {
                return_expr(Expr::Literal(Literal::Int(1), dummy_span()))
            } else {
                return_expr(call(&format!("chain_{}", i + 1), vec![]))
            };
            functions.push(function(&format!("chain_{i}"), body));
        }
        let program = program(functions);
        let effects_map = infer_program_effects(&program);

        assert!(effects_map["chain_0"].is_foldable());
        assert!(effects_map[&format!("chain_{}", CHAIN - 1)].is_foldable());
    }

    #[test]
    fn test_propagate_effects_simple() {
        // Create a simple program with two functions
        let pure_func = function(
            "pure",
            return_expr(Expr::Literal(Literal::Int(42), dummy_span())),
        );
        let caller_func = function("caller", return_expr(call("pure", vec![])));
        let program = program(vec![pure_func, caller_func]);

        let call_graph = CallGraph::from_program(&program);
        let effects_map = propagate_effects(&call_graph, &program.functions);

        // Both functions should have computed effects
        assert!(effects_map.contains_key("pure"));
        assert!(effects_map.contains_key("caller"));

        // Pure function should be foldable
        let pure_effects = effects_map.get("pure").unwrap();
        assert!(pure_effects.is_foldable());
    }

    #[test]
    fn body_derived_effects_mark_unknown_pure_function_foldable_issue_8441() {
        let local_add = function(
            "locally_foldable",
            return_expr(Expr::BinaryOp {
                op: BinaryOp::Add,
                left: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                right: Box::new(Expr::Literal(Literal::Int(2), dummy_span())),
                span: dummy_span(),
            }),
        );
        let caller = function("caller", return_expr(call("locally_foldable", vec![])));
        let program = program(vec![local_add, caller]);
        let effects_map = infer_program_effects(&program);

        assert!(effects_map["locally_foldable"].is_foldable());
        assert!(effects_map["caller"].is_foldable());
    }

    #[test]
    fn overloaded_methods_merge_effects_conservatively_issue_8441() {
        let pure_method = function(
            "overloaded",
            return_expr(Expr::Literal(Literal::Int(1), dummy_span())),
        );
        let throwing_method = function(
            "overloaded",
            return_expr(call(
                "throw",
                vec![Expr::Literal(
                    Literal::Str("boom".to_string()),
                    dummy_span(),
                )],
            )),
        );
        let caller = function("caller", return_expr(call("overloaded", vec![])));
        let program = program(vec![pure_method, throwing_method, caller]);
        let effects_map = propagate_effects(&CallGraph::from_program(&program), &program.functions);

        assert!(!effects_map["overloaded"].is_foldable());
        assert!(!effects_map["caller"].is_foldable());
    }

    #[test]
    fn user_defined_operator_throwing_body_overrides_name_hint_issue_8441() {
        let throwing_plus = function(
            "+",
            return_expr(call(
                "throw",
                vec![Expr::Literal(
                    Literal::Str("boom".to_string()),
                    dummy_span(),
                )],
            )),
        );
        let program = program(vec![throwing_plus]);
        let effects_map = propagate_effects(&CallGraph::from_program(&program), &program.functions);

        assert!(!effects_map["+"].is_foldable());
        assert!(!effects_map["+"].nothrow);
    }

    /// One method of a generic function with a param typed `ty`, whose body is
    /// `return <expr>`.
    fn typed_method(name: &str, param_type: crate::types::JuliaType, body: Expr) -> Function {
        Function {
            name: name.to_string(),
            params: vec![crate::ir::core::TypedParam::new(
                "x".to_string(),
                Some(param_type),
                dummy_span(),
            )],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![return_expr(body)],
                span: dummy_span(),
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span: dummy_span(),
            new_struct_name: None,
        }
    }

    #[test]
    fn per_method_summary_isolates_pure_from_impure_sibling_issue_9205() {
        // Two methods of `f`: a pure `f(x::Int) = x + 1` and an impure
        // `f(x::Float64) = println(x)`. The per-method map must keep the pure
        // method foldable — it must NOT be tainted by its impure sibling — while
        // the name-level merge stays conservative (impure), exactly matching how
        // a by-name call site could still dispatch to either method.
        let pure = typed_method(
            "f",
            crate::types::JuliaType::Int64,
            Expr::BinaryOp {
                op: crate::ir::core::BinaryOp::Add,
                left: Box::new(Expr::Var("x".to_string().into(), dummy_span())),
                right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                span: dummy_span(),
            },
        );
        let impure = typed_method(
            "f",
            crate::types::JuliaType::Float64,
            call(
                "println",
                vec![Expr::Var("x".to_string().into(), dummy_span())],
            ),
        );
        let pure_key = MethodKey::from_function(&pure);
        let impure_key = MethodKey::from_function(&impure);
        assert_ne!(
            pure_key, impure_key,
            "distinct signatures must yield distinct MethodKeys"
        );

        let program = program(vec![pure, impure]);
        let summaries = infer_program_effects_per_method(&program);

        // Per-method: the pure method is foldable; the impure one is not.
        assert!(
            summaries.by_method[&pure_key].is_foldable(),
            "pure f(::Int) per-method summary must stay foldable: {:?}",
            summaries.by_method[&pure_key]
        );
        assert!(
            !summaries.by_method[&impure_key].is_foldable(),
            "impure f(::Float64) per-method summary must not be foldable"
        );

        // Name-level: the conservative merge of both methods is NOT foldable —
        // unchanged from `infer_program_effects`, the sound by-name fallback.
        assert!(!summaries.by_name["f"].is_foldable());
        let name_level_only = infer_program_effects(&program);
        assert_eq!(
            summaries.by_name, name_level_only,
            "per-method variant must derive the identical name-level map"
        );
    }

    #[test]
    fn precision_stats_count_methods_hidden_by_name_merge_issue_9205() {
        // Same pure/impure `f` pair: exactly one method (the pure `f(::Int)`) is
        // foldable while the name-level merge is not, so the recoverable count
        // is 1. A single-method pure `g` is already foldable name-level, so it
        // contributes no recovered opportunity.
        let pure_f = typed_method(
            "f",
            crate::types::JuliaType::Int64,
            Expr::Literal(Literal::Int(1), dummy_span()),
        );
        let impure_f = typed_method(
            "f",
            crate::types::JuliaType::Float64,
            call(
                "println",
                vec![Expr::Var("x".to_string().into(), dummy_span())],
            ),
        );
        let pure_g = typed_method(
            "g",
            crate::types::JuliaType::Int64,
            Expr::Literal(Literal::Int(2), dummy_span()),
        );
        let program = program(vec![pure_f, impure_f, pure_g]);
        let summaries = infer_program_effects_per_method(&program);
        let stats = per_method_precision_stats(&summaries);

        assert_eq!(stats.methods, 3);
        assert_eq!(
            stats.foldable_recovered, 1,
            "only the pure f(::Int) is foldable-but-hidden by the name merge"
        );
    }
}
