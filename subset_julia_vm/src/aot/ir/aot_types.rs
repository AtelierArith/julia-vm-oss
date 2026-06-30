//! High-level AoT IR types for code generation.
//!
//! Contains AoT program, function, global, struct, statement, and expression types.

use super::super::types::StaticType;
use super::ops::{AotBinOp, AotBuiltinOp, AotUnaryOp, CompoundAssignOp};
use crate::aot::abi::AotCallAbi;
use std::collections::{HashMap, HashSet};
use std::fmt;

// Higher-Level AoT IR (for code generation)
// ============================================================================

/// AoT program representation
///
/// Contains all functions, globals, structs, enums, and the main execution block.
#[derive(Debug, Clone)]
pub struct AotProgram {
    /// Function definitions
    pub functions: Vec<AotFunction>,
    /// Global variable declarations
    pub globals: Vec<AotGlobal>,
    /// Struct definitions
    pub structs: Vec<AotStruct>,
    /// Enum definitions
    pub enums: Vec<AotEnum>,
    /// Main block statements
    pub main: Vec<AotStmt>,
}

impl AotProgram {
    /// Create a new empty AoT program
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
            globals: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            main: Vec::new(),
        }
    }

    /// Add a function to the program
    pub fn add_function(&mut self, func: AotFunction) {
        self.functions.push(func);
    }

    /// Add a global variable to the program
    pub fn add_global(&mut self, global: AotGlobal) {
        self.globals.push(global);
    }

    /// Add a struct definition to the program
    pub fn add_struct(&mut self, s: AotStruct) {
        self.structs.push(s);
    }

    /// Add an enum definition to the program
    pub fn add_enum(&mut self, e: AotEnum) {
        self.enums.push(e);
    }

    /// Build a method table mapping function names to their methods
    ///
    /// Returns a map from function name to a list of all functions with that name
    /// (different type specializations). Used for implementing multiple dispatch.
    ///
    /// # Example
    /// ```ignore
    /// let table = program.build_method_table();
    /// // If program has add(Int64, Int64) and add(Float64, Float64):
    /// // table["add"] = vec![add_i64_i64_func, add_f64_f64_func]
    /// ```
    pub fn build_method_table(&self) -> HashMap<String, Vec<&AotFunction>> {
        let mut table: HashMap<String, Vec<&AotFunction>> = HashMap::new();
        for func in &self.functions {
            table.entry(func.name.clone()).or_default().push(func);
        }
        table
    }

    /// Get all functions with multiple dispatch (same name, different signatures)
    ///
    /// Returns a list of function names that have multiple methods defined.
    pub fn get_multidispatch_functions(&self) -> Vec<String> {
        let table = self.build_method_table();
        table
            .into_iter()
            .filter(|(_, methods)| methods.len() > 1)
            .map(|(name, _)| name)
            .collect()
    }

    /// Count total number of instructions (statements) in the program
    pub fn instruction_count(&self) -> usize {
        let func_stmts: usize = self
            .functions
            .iter()
            .map(|f| Self::count_stmts(&f.body))
            .sum();
        let main_stmts = Self::count_stmts(&self.main);
        func_stmts + main_stmts
    }

    /// Count statements recursively
    fn count_stmts(stmts: &[AotStmt]) -> usize {
        let mut count = stmts.len();
        for stmt in stmts {
            count += match stmt {
                AotStmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    Self::count_stmts(then_branch)
                        + else_branch.as_ref().map_or(0, |e| Self::count_stmts(e))
                }
                AotStmt::While { body, .. } => Self::count_stmts(body),
                AotStmt::ForRange { body, .. } => Self::count_stmts(body),
                AotStmt::ForEach { body, .. } => Self::count_stmts(body),
                _ => 0,
            };
        }
        count
    }

    /// Count dynamic (non-statically-typed) function calls
    pub fn count_dynamic_calls(&self) -> usize {
        let func_dynamic: usize = self
            .functions
            .iter()
            .map(|f| Self::count_dynamic_in_stmts(&f.body))
            .sum();
        let main_dynamic = Self::count_dynamic_in_stmts(&self.main);
        func_dynamic + main_dynamic
    }

    /// Remove functions that are unreachable from the program entry point.
    ///
    /// After AoT broadcast specialization and inlining, the program can retain
    /// Base helper functions whose only callers were the `broadcast` / `collect`
    /// calls that specialization replaced with concrete `__aot_broadcast_*`
    /// helpers. Code generation emits every function in `functions`, so those now
    /// dead helpers survive — frequently with type-erased `-> Value` signatures
    /// because they are generic Base machinery (Issue #6629).
    ///
    /// Pruning runs only when the reachable closure contains no dynamic
    /// (multiple-dispatch) call: a `CallDynamic` / `BinOpDynamic` resolves to a
    /// method by runtime type, so when one is reachable we conservatively keep
    /// every function (no regression for dynamic-dispatch programs). Fully static
    /// programs — the case that regressed — prune cleanly. A bare reference to a
    /// function name (a function used as a value, e.g. passed to a higher-order
    /// helper) keeps that function reachable.
    pub fn prune_unreachable_functions(&mut self) {
        self.prune_unreachable_functions_with_roots(&[]);
    }

    /// Like [`Self::prune_unreachable_functions`], but also treats the supplied
    /// function names as roots. This is used for C ABI exports: export candidates
    /// may be uncalled from `main`, but still need to survive codegen.
    pub fn prune_unreachable_functions_with_roots(&mut self, extra_roots: &[String]) {
        let all_names: HashSet<String> = self.functions.iter().map(|f| f.name.clone()).collect();

        let mut worklist: Vec<String> = Vec::new();
        // Seed roots from the entry (main) block. A reachable dynamic call means
        // we cannot know the full target set statically, so keep everything.
        if Self::collect_reachable_refs_in_stmts(&self.main, &all_names, &mut worklist) {
            return;
        }
        worklist.extend(
            extra_roots
                .iter()
                .filter(|name| all_names.contains(*name))
                .cloned(),
        );

        // name -> indices of its methods (a name may have several specializations).
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, f) in self.functions.iter().enumerate() {
            by_name.entry(f.name.clone()).or_default().push(i);
        }

        let mut reachable: HashSet<String> = HashSet::new();
        while let Some(name) = worklist.pop() {
            if !reachable.insert(name.clone()) {
                continue;
            }
            if let Some(indices) = by_name.get(&name) {
                for &i in indices {
                    if Self::collect_reachable_refs_in_stmts(
                        &self.functions[i].body,
                        &all_names,
                        &mut worklist,
                    ) {
                        return; // dynamic dispatch reachable: keep everything.
                    }
                }
            }
        }

        self.functions.retain(|f| reachable.contains(&f.name));
    }

    /// Walk `stmts`, pushing every referenced function name onto `worklist`.
    /// Returns `true` if a dynamic (runtime-dispatched) call or binary op was
    /// encountered anywhere in the walk (Issue #6629).
    fn collect_reachable_refs_in_stmts(
        stmts: &[AotStmt],
        all_names: &HashSet<String>,
        worklist: &mut Vec<String>,
    ) -> bool {
        let mut dynamic = false;
        for stmt in stmts {
            // Use non-short-circuiting `|` so every branch is walked for refs.
            dynamic |= match stmt {
                AotStmt::Let { value, .. } => {
                    Self::collect_reachable_refs_in_expr(value, all_names, worklist)
                }
                AotStmt::Assign { target, value } => {
                    Self::collect_reachable_refs_in_expr(target, all_names, worklist)
                        | Self::collect_reachable_refs_in_expr(value, all_names, worklist)
                }
                AotStmt::CompoundAssign { target, value, .. } => {
                    Self::collect_reachable_refs_in_expr(target, all_names, worklist)
                        | Self::collect_reachable_refs_in_expr(value, all_names, worklist)
                }
                AotStmt::Expr(expr) => {
                    Self::collect_reachable_refs_in_expr(expr, all_names, worklist)
                }
                AotStmt::Return(Some(expr)) => {
                    Self::collect_reachable_refs_in_expr(expr, all_names, worklist)
                }
                AotStmt::Return(None) | AotStmt::Break | AotStmt::Continue => false,
                AotStmt::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    Self::collect_reachable_refs_in_expr(condition, all_names, worklist)
                        | Self::collect_reachable_refs_in_stmts(then_branch, all_names, worklist)
                        | else_branch.as_ref().is_some_and(|e| {
                            Self::collect_reachable_refs_in_stmts(e, all_names, worklist)
                        })
                }
                AotStmt::While { condition, body } => {
                    Self::collect_reachable_refs_in_expr(condition, all_names, worklist)
                        | Self::collect_reachable_refs_in_stmts(body, all_names, worklist)
                }
                AotStmt::ForRange {
                    start,
                    stop,
                    step,
                    body,
                    ..
                } => {
                    Self::collect_reachable_refs_in_expr(start, all_names, worklist)
                        | Self::collect_reachable_refs_in_expr(stop, all_names, worklist)
                        | step.as_ref().is_some_and(|s| {
                            Self::collect_reachable_refs_in_expr(s, all_names, worklist)
                        })
                        | Self::collect_reachable_refs_in_stmts(body, all_names, worklist)
                }
                AotStmt::ForEach { iter, body, .. } => {
                    Self::collect_reachable_refs_in_expr(iter, all_names, worklist)
                        | Self::collect_reachable_refs_in_stmts(body, all_names, worklist)
                }
            };
        }
        dynamic
    }

    /// Walk an expression, pushing referenced function names onto `worklist`.
    /// Returns `true` if a dynamic call / binary op was encountered.
    fn collect_reachable_refs_in_expr(
        expr: &AotExpr,
        all_names: &HashSet<String>,
        worklist: &mut Vec<String>,
    ) -> bool {
        let walk_all = |exprs: &[AotExpr], worklist: &mut Vec<String>| -> bool {
            exprs.iter().fold(false, |d, e| {
                d | Self::collect_reachable_refs_in_expr(e, all_names, worklist)
            })
        };
        match expr {
            // A bare reference to a function name (a function used as a value,
            // e.g. passed to a higher-order helper) keeps that function alive.
            AotExpr::Var { name, .. } => {
                if all_names.contains(name) {
                    worklist.push(name.clone());
                }
                false
            }
            AotExpr::CallStatic { function, args, .. } => {
                if all_names.contains(function) {
                    worklist.push(function.clone());
                }
                walk_all(args, worklist)
            }
            AotExpr::CallDynamic { function, args, .. } => {
                if all_names.contains(function) {
                    worklist.push(function.clone());
                }
                walk_all(args, worklist);
                true
            }
            AotExpr::CallBuiltin { args, .. } => walk_all(args, worklist),
            AotExpr::BinOpStatic { left, right, .. } => {
                Self::collect_reachable_refs_in_expr(left, all_names, worklist)
                    | Self::collect_reachable_refs_in_expr(right, all_names, worklist)
            }
            AotExpr::BinOpDynamic { left, right, .. } => {
                Self::collect_reachable_refs_in_expr(left, all_names, worklist);
                Self::collect_reachable_refs_in_expr(right, all_names, worklist);
                true
            }
            AotExpr::UnaryOp { operand, .. } => {
                Self::collect_reachable_refs_in_expr(operand, all_names, worklist)
            }
            AotExpr::ArrayLit { elements, .. }
            | AotExpr::TupleLit { elements }
            | AotExpr::StructNew {
                fields: elements, ..
            } => walk_all(elements, worklist),
            AotExpr::SetFromIter { iter, .. } => {
                Self::collect_reachable_refs_in_expr(iter, all_names, worklist)
            }
            AotExpr::NamedTupleLit { fields } => fields
                .iter()
                .any(|(_, expr)| Self::collect_reachable_refs_in_expr(expr, all_names, worklist)),
            AotExpr::Comprehension {
                body, iter, filter, ..
            }
            | AotExpr::Generator {
                body, iter, filter, ..
            } => {
                Self::collect_reachable_refs_in_expr(iter, all_names, worklist)
                    | filter.as_ref().is_some_and(|filter| {
                        Self::collect_reachable_refs_in_expr(filter, all_names, worklist)
                    })
                    | Self::collect_reachable_refs_in_expr(body, all_names, worklist)
            }
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                iterations.iter().any(|(_, iter)| {
                    Self::collect_reachable_refs_in_expr(iter, all_names, worklist)
                }) | filter.as_ref().is_some_and(|filter| {
                    Self::collect_reachable_refs_in_expr(filter, all_names, worklist)
                }) | Self::collect_reachable_refs_in_expr(body, all_names, worklist)
            }
            AotExpr::Index { array, indices, .. } => {
                Self::collect_reachable_refs_in_expr(array, all_names, worklist)
                    | walk_all(indices, worklist)
            }
            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::collect_reachable_refs_in_expr(start, all_names, worklist)
                    | Self::collect_reachable_refs_in_expr(stop, all_names, worklist)
                    | step.as_ref().is_some_and(|s| {
                        Self::collect_reachable_refs_in_expr(s, all_names, worklist)
                    })
            }
            AotExpr::FieldAccess { object, .. } => {
                Self::collect_reachable_refs_in_expr(object, all_names, worklist)
            }
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::collect_reachable_refs_in_expr(condition, all_names, worklist)
                    | Self::collect_reachable_refs_in_expr(then_expr, all_names, worklist)
                    | Self::collect_reachable_refs_in_expr(else_expr, all_names, worklist)
            }
            AotExpr::Box(inner) => Self::collect_reachable_refs_in_expr(inner, all_names, worklist),
            AotExpr::Unbox { value, .. } | AotExpr::Convert { value, .. } => {
                Self::collect_reachable_refs_in_expr(value, all_names, worklist)
            }
            AotExpr::Lambda { body, .. } => {
                Self::collect_reachable_refs_in_expr(body, all_names, worklist)
            }
            // Literals carry no references.
            _ => false,
        }
    }

    /// Count dynamic calls in statements
    fn count_dynamic_in_stmts(stmts: &[AotStmt]) -> usize {
        let mut count = 0;
        for stmt in stmts {
            count += match stmt {
                AotStmt::Let { value, .. } => Self::count_dynamic_in_expr(value),
                AotStmt::Assign { value, .. } => Self::count_dynamic_in_expr(value),
                AotStmt::Expr(expr) => Self::count_dynamic_in_expr(expr),
                AotStmt::Return(Some(expr)) => Self::count_dynamic_in_expr(expr),
                AotStmt::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    Self::count_dynamic_in_expr(condition)
                        + Self::count_dynamic_in_stmts(then_branch)
                        + else_branch
                            .as_ref()
                            .map_or(0, |e| Self::count_dynamic_in_stmts(e))
                }
                AotStmt::While {
                    condition, body, ..
                } => Self::count_dynamic_in_expr(condition) + Self::count_dynamic_in_stmts(body),
                AotStmt::ForRange {
                    start,
                    stop,
                    step,
                    body,
                    ..
                } => {
                    Self::count_dynamic_in_expr(start)
                        + Self::count_dynamic_in_expr(stop)
                        + step.as_ref().map_or(0, Self::count_dynamic_in_expr)
                        + Self::count_dynamic_in_stmts(body)
                }
                AotStmt::ForEach { iter, body, .. } => {
                    Self::count_dynamic_in_expr(iter) + Self::count_dynamic_in_stmts(body)
                }
                _ => 0,
            };
        }
        count
    }

    /// Count dynamic calls in an expression
    fn count_dynamic_in_expr(expr: &AotExpr) -> usize {
        match expr {
            AotExpr::CallDynamic { args, .. } => {
                1 + args.iter().map(Self::count_dynamic_in_expr).sum::<usize>()
            }
            AotExpr::BinOpDynamic { left, right, .. } => {
                1 + Self::count_dynamic_in_expr(left) + Self::count_dynamic_in_expr(right)
            }
            AotExpr::CallStatic { args, .. } | AotExpr::CallBuiltin { args, .. } => {
                args.iter().map(Self::count_dynamic_in_expr).sum()
            }
            AotExpr::BinOpStatic { left, right, .. } => {
                Self::count_dynamic_in_expr(left) + Self::count_dynamic_in_expr(right)
            }
            AotExpr::UnaryOp { operand, .. } => Self::count_dynamic_in_expr(operand),
            AotExpr::ArrayLit { elements, .. }
            | AotExpr::TupleLit { elements }
            | AotExpr::StructNew {
                fields: elements, ..
            } => elements.iter().map(Self::count_dynamic_in_expr).sum(),
            AotExpr::SetFromIter { iter, .. } => Self::count_dynamic_in_expr(iter),
            AotExpr::NamedTupleLit { fields } => fields
                .iter()
                .map(|(_, expr)| Self::count_dynamic_in_expr(expr))
                .sum(),
            AotExpr::Comprehension {
                body, iter, filter, ..
            }
            | AotExpr::Generator {
                body, iter, filter, ..
            } => {
                Self::count_dynamic_in_expr(iter)
                    + filter
                        .as_ref()
                        .map_or(0, |filter| Self::count_dynamic_in_expr(filter))
                    + Self::count_dynamic_in_expr(body)
            }
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                iterations
                    .iter()
                    .map(|(_, iter)| Self::count_dynamic_in_expr(iter))
                    .sum::<usize>()
                    + filter
                        .as_ref()
                        .map_or(0, |filter| Self::count_dynamic_in_expr(filter))
                    + Self::count_dynamic_in_expr(body)
            }
            AotExpr::Index { array, indices, .. } => {
                Self::count_dynamic_in_expr(array)
                    + indices
                        .iter()
                        .map(Self::count_dynamic_in_expr)
                        .sum::<usize>()
            }
            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::count_dynamic_in_expr(start)
                    + Self::count_dynamic_in_expr(stop)
                    + step.as_ref().map_or(0, |s| Self::count_dynamic_in_expr(s))
            }
            AotExpr::FieldAccess { object, .. } => Self::count_dynamic_in_expr(object),
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::count_dynamic_in_expr(condition)
                    + Self::count_dynamic_in_expr(then_expr)
                    + Self::count_dynamic_in_expr(else_expr)
            }
            AotExpr::Box(inner) => Self::count_dynamic_in_expr(inner),
            AotExpr::Unbox { value, .. } => Self::count_dynamic_in_expr(value),
            _ => 0,
        }
    }

    /// Collect detailed diagnostics about dynamic operations
    ///
    /// Returns a list of (location, description) tuples explaining why
    /// dynamic dispatch is being used. Useful for error messages.
    pub fn diagnose_dynamic_operations(&self) -> Vec<DynamicOpDiagnostic> {
        let mut diagnostics = Vec::new();

        // Check functions
        for func in &self.functions {
            Self::diagnose_dynamic_in_stmts(
                &func.body,
                &format!("function `{}`", func.name),
                &mut diagnostics,
            );
        }

        // Check main block
        Self::diagnose_dynamic_in_stmts(&self.main, "main block", &mut diagnostics);

        diagnostics
    }

    fn diagnose_dynamic_in_stmts(
        stmts: &[AotStmt],
        location: &str,
        diagnostics: &mut Vec<DynamicOpDiagnostic>,
    ) {
        for stmt in stmts {
            match stmt {
                AotStmt::Let { value, .. } => {
                    Self::diagnose_dynamic_in_expr(value, location, diagnostics);
                }
                AotStmt::Assign { value, .. } => {
                    Self::diagnose_dynamic_in_expr(value, location, diagnostics);
                }
                AotStmt::Expr(expr) => {
                    Self::diagnose_dynamic_in_expr(expr, location, diagnostics);
                }
                AotStmt::Return(Some(expr)) => {
                    Self::diagnose_dynamic_in_expr(expr, location, diagnostics);
                }
                AotStmt::If {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    Self::diagnose_dynamic_in_expr(condition, location, diagnostics);
                    Self::diagnose_dynamic_in_stmts(then_branch, location, diagnostics);
                    if let Some(else_stmts) = else_branch {
                        Self::diagnose_dynamic_in_stmts(else_stmts, location, diagnostics);
                    }
                }
                AotStmt::While {
                    condition, body, ..
                } => {
                    Self::diagnose_dynamic_in_expr(condition, location, diagnostics);
                    Self::diagnose_dynamic_in_stmts(body, location, diagnostics);
                }
                AotStmt::ForRange {
                    start,
                    stop,
                    step,
                    body,
                    ..
                } => {
                    Self::diagnose_dynamic_in_expr(start, location, diagnostics);
                    Self::diagnose_dynamic_in_expr(stop, location, diagnostics);
                    if let Some(s) = step {
                        Self::diagnose_dynamic_in_expr(s, location, diagnostics);
                    }
                    Self::diagnose_dynamic_in_stmts(body, location, diagnostics);
                }
                AotStmt::ForEach { iter, body, .. } => {
                    Self::diagnose_dynamic_in_expr(iter, location, diagnostics);
                    Self::diagnose_dynamic_in_stmts(body, location, diagnostics);
                }
                _ => {}
            }
        }
    }

    fn diagnose_dynamic_in_expr(
        expr: &AotExpr,
        location: &str,
        diagnostics: &mut Vec<DynamicOpDiagnostic>,
    ) {
        match expr {
            AotExpr::CallDynamic { function, args, .. } => {
                diagnostics.push(DynamicOpDiagnostic {
                    location: location.to_string(),
                    operation: format!("call `{}`", function),
                    reason: "argument types could not be statically determined".to_string(),
                    suggestion: format!(
                        "Add explicit type annotations to the arguments of `{}`",
                        function
                    ),
                });
                for arg in args {
                    Self::diagnose_dynamic_in_expr(arg, location, diagnostics);
                }
            }
            AotExpr::BinOpDynamic {
                op, left, right, ..
            } => {
                diagnostics.push(DynamicOpDiagnostic {
                    location: location.to_string(),
                    operation: format!("binary operation `{}`", op),
                    reason: "operand types could not be statically determined".to_string(),
                    suggestion: "Ensure operand types are known at compile time".to_string(),
                });
                Self::diagnose_dynamic_in_expr(left, location, diagnostics);
                Self::diagnose_dynamic_in_expr(right, location, diagnostics);
            }
            AotExpr::CallStatic { args, .. } | AotExpr::CallBuiltin { args, .. } => {
                for arg in args {
                    Self::diagnose_dynamic_in_expr(arg, location, diagnostics);
                }
            }
            AotExpr::BinOpStatic { left, right, .. } => {
                Self::diagnose_dynamic_in_expr(left, location, diagnostics);
                Self::diagnose_dynamic_in_expr(right, location, diagnostics);
            }
            AotExpr::UnaryOp { operand, .. } => {
                Self::diagnose_dynamic_in_expr(operand, location, diagnostics);
            }
            AotExpr::ArrayLit { elements, .. }
            | AotExpr::TupleLit { elements }
            | AotExpr::StructNew {
                fields: elements, ..
            } => {
                for elem in elements {
                    Self::diagnose_dynamic_in_expr(elem, location, diagnostics);
                }
            }
            AotExpr::SetFromIter { iter, .. } => {
                Self::diagnose_dynamic_in_expr(iter, location, diagnostics);
            }
            AotExpr::NamedTupleLit { fields } => {
                for (_, elem) in fields {
                    Self::diagnose_dynamic_in_expr(elem, location, diagnostics);
                }
            }
            AotExpr::Comprehension {
                body, iter, filter, ..
            }
            | AotExpr::Generator {
                body, iter, filter, ..
            } => {
                Self::diagnose_dynamic_in_expr(iter, location, diagnostics);
                if let Some(filter) = filter {
                    Self::diagnose_dynamic_in_expr(filter, location, diagnostics);
                }
                Self::diagnose_dynamic_in_expr(body, location, diagnostics);
            }
            AotExpr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                for (_, iter) in iterations {
                    Self::diagnose_dynamic_in_expr(iter, location, diagnostics);
                }
                if let Some(filter) = filter {
                    Self::diagnose_dynamic_in_expr(filter, location, diagnostics);
                }
                Self::diagnose_dynamic_in_expr(body, location, diagnostics);
            }
            AotExpr::Index { array, indices, .. } => {
                Self::diagnose_dynamic_in_expr(array, location, diagnostics);
                for idx in indices {
                    Self::diagnose_dynamic_in_expr(idx, location, diagnostics);
                }
            }
            AotExpr::Range {
                start, stop, step, ..
            } => {
                Self::diagnose_dynamic_in_expr(start, location, diagnostics);
                Self::diagnose_dynamic_in_expr(stop, location, diagnostics);
                if let Some(s) = step {
                    Self::diagnose_dynamic_in_expr(s, location, diagnostics);
                }
            }
            AotExpr::FieldAccess { object, .. } => {
                Self::diagnose_dynamic_in_expr(object, location, diagnostics);
            }
            AotExpr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                Self::diagnose_dynamic_in_expr(condition, location, diagnostics);
                Self::diagnose_dynamic_in_expr(then_expr, location, diagnostics);
                Self::diagnose_dynamic_in_expr(else_expr, location, diagnostics);
            }
            AotExpr::Box(inner) => {
                Self::diagnose_dynamic_in_expr(inner, location, diagnostics);
            }
            AotExpr::Unbox { value, .. } => {
                Self::diagnose_dynamic_in_expr(value, location, diagnostics);
            }
            _ => {}
        }
    }
}

/// Diagnostic information about a dynamic operation
#[derive(Debug, Clone)]
pub struct DynamicOpDiagnostic {
    /// Where the operation occurs (function name or "main block")
    pub location: String,
    /// Description of the operation (e.g., "call `foo`", "binary operation `+`")
    pub operation: String,
    /// Why it requires dynamic dispatch
    pub reason: String,
    /// Suggested fix
    pub suggestion: String,
}

impl fmt::Display for DynamicOpDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "In {}: {} requires dynamic dispatch\n  Reason: {}\n  Suggestion: {}",
            self.location, self.operation, self.reason, self.suggestion
        )
    }
}

impl Default for AotProgram {
    fn default() -> Self {
        Self::new()
    }
}

/// AoT function definition
#[derive(Debug, Clone)]
pub struct AotFunction {
    /// Function name
    pub name: String,
    /// Parameters (name, type)
    pub params: Vec<(String, StaticType)>,
    /// Return type
    pub return_type: StaticType,
    /// Function body statements
    pub body: Vec<AotStmt>,
    /// Whether this is a generic function (has Any-typed params)
    pub is_generic: bool,
    /// Inline/noinline policy retained from Julia metadata annotations.
    pub inline_policy: AotInlinePolicy,
}

/// AoT inlining policy derived from Julia metadata annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AotInlinePolicy {
    Auto,
    Always,
    Never,
}

impl AotFunction {
    /// Create a new AoT function
    pub fn new(name: String, params: Vec<(String, StaticType)>, return_type: StaticType) -> Self {
        let is_generic = params.iter().any(|(_, ty)| matches!(ty, StaticType::Any));
        Self {
            name,
            params,
            return_type,
            body: Vec::new(),
            is_generic,
            inline_policy: AotInlinePolicy::Auto,
        }
    }

    /// Check if this function is fully statically typed
    pub fn is_fully_static(&self) -> bool {
        self.params.iter().all(|(_, ty)| ty.is_fully_static()) && self.return_type.is_fully_static()
    }

    /// Build the backend-neutral call ABI for this inferred specialization.
    pub fn call_abi(&self) -> AotCallAbi {
        let params: Vec<_> = self.params.iter().map(|(_, ty)| ty.clone()).collect();
        AotCallAbi::from_signature(&params, &self.return_type)
    }

    /// Get the mangled name for this function based on parameter types
    ///
    /// This creates a unique name for each type specialization of a function.
    /// For example, `add(x::Int64, y::Int64)` becomes `add_i64_i64`.
    ///
    /// # Examples
    /// ```ignore
    /// use subset_julia_vm::aot::ir::AotFunction;
    /// use subset_julia_vm::aot::types::StaticType;
    ///
    /// let func = AotFunction::new(
    ///     "add".to_string(),
    ///     vec![("x".to_string(), StaticType::I64), ("y".to_string(), StaticType::I64)],
    ///     StaticType::I64,
    /// );
    /// assert_eq!(func.mangled_name(), "add_i64_i64");
    /// ```
    pub fn mangled_name(&self) -> String {
        // Sanitize the function name for Rust (convert operators to valid names)
        let sanitized_name = Self::sanitize_function_name(&self.name);

        if self.params.is_empty() {
            sanitized_name
        } else {
            let type_suffix: Vec<_> = self
                .params
                .iter()
                .map(|(_, ty)| ty.mangle_suffix())
                .collect();
            format!("{}_{}", sanitized_name, type_suffix.join("_"))
        }
    }

    /// Sanitize a Julia function name to a valid Rust identifier
    pub(crate) fn sanitize_function_name(name: &str) -> String {
        match name {
            "+" => "op_add".to_string(),
            "-" => "op_sub".to_string(),
            "*" => "op_mul".to_string(),
            "/" => "op_div".to_string(),
            "÷" => "op_intdiv".to_string(),
            "%" => "op_mod".to_string(),
            "^" => "op_pow".to_string(),
            "==" => "op_eq".to_string(),
            "!=" => "op_ne".to_string(),
            "<" => "op_lt".to_string(),
            "<=" => "op_le".to_string(),
            ">" => "op_gt".to_string(),
            ">=" => "op_ge".to_string(),
            "===" => "op_egal".to_string(),
            "!==" => "op_notegal".to_string(),
            "!" => "op_not".to_string(),
            "&" => "op_band".to_string(),
            "|" => "op_bor".to_string(),
            "⊻" | "xor" => "op_xor".to_string(),
            "<<" => "op_lshift".to_string(),
            ">>" => "op_rshift".to_string(),
            ">>>" => "op_urshift".to_string(),
            "~" => "op_bnot".to_string(),
            "&&" => "op_and".to_string(),
            "||" => "op_or".to_string(),
            _ => {
                // Replace invalid characters with underscores
                name.chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect()
            }
        }
    }

    /// Get the type signature as a string for display
    pub fn type_signature(&self) -> String {
        let param_types: Vec<_> = self
            .params
            .iter()
            .map(|(_, ty)| ty.julia_type_name())
            .collect();
        format!("{}({})", self.name, param_types.join(", "))
    }
}

/// AoT global variable
#[derive(Debug, Clone)]
pub struct AotGlobal {
    /// Variable name
    pub name: String,
    /// Variable type
    pub ty: StaticType,
    /// Initial value (if any)
    pub init: Option<AotExpr>,
}

impl AotGlobal {
    /// Create a new global variable
    pub fn new(name: String, ty: StaticType) -> Self {
        Self {
            name,
            ty,
            init: None,
        }
    }

    /// Create a new global variable with initial value
    pub fn with_init(name: String, ty: StaticType, init: AotExpr) -> Self {
        Self {
            name,
            ty,
            init: Some(init),
        }
    }
}

/// AoT struct definition
#[derive(Debug, Clone)]
pub struct AotStruct {
    /// Struct name
    pub name: String,
    /// Type parameter names for generic structs, e.g. `T` in `Box{T}`.
    pub type_params: Vec<String>,
    /// Fields (name, type)
    pub fields: Vec<(String, StaticType)>,
    /// Whether this struct is mutable
    pub is_mutable: bool,
}

impl AotStruct {
    /// Create a new struct definition
    pub fn new(name: String, is_mutable: bool) -> Self {
        Self {
            name,
            type_params: Vec::new(),
            fields: Vec::new(),
            is_mutable,
        }
    }

    /// Set generic type parameter names for this struct.
    pub fn with_type_params(mut self, type_params: Vec<String>) -> Self {
        self.type_params = type_params;
        self
    }

    /// Add a field to the struct
    pub fn add_field(&mut self, name: String, ty: StaticType) {
        self.fields.push((name, ty));
    }
}

/// AoT enum definition
///
/// Julia enums are integer-backed symbolic types (`@enum Color red green blue`).
/// They are represented as i32 constants in the generated Rust code.
#[derive(Debug, Clone)]
pub struct AotEnum {
    /// Enum type name (e.g., "Color")
    pub name: String,
    /// Members: (name, integer value)
    pub members: Vec<(String, i32)>,
}

impl AotEnum {
    /// Create a new enum definition
    pub fn new(name: String) -> Self {
        Self {
            name,
            members: Vec::new(),
        }
    }

    /// Add a member to the enum
    pub fn add_member(&mut self, name: String, value: i32) {
        self.members.push((name, value));
    }
}

/// AoT statement
#[derive(Debug, Clone)]
pub enum AotStmt {
    /// Variable declaration and assignment: let x: T = value
    Let {
        name: String,
        ty: StaticType,
        value: AotExpr,
        is_mutable: bool,
    },
    /// Assignment: x = value
    Assign { target: AotExpr, value: AotExpr },
    /// Compound assignment: x += value, x -= value, etc.
    CompoundAssign {
        target: AotExpr,
        op: CompoundAssignOp,
        value: AotExpr,
    },
    /// Expression statement
    Expr(AotExpr),
    /// Return statement
    Return(Option<AotExpr>),
    /// If statement
    If {
        condition: AotExpr,
        then_branch: Vec<AotStmt>,
        else_branch: Option<Vec<AotStmt>>,
    },
    /// While loop
    While {
        condition: AotExpr,
        body: Vec<AotStmt>,
    },
    /// For loop with range: for var in start:stop
    ForRange {
        var: String,
        start: AotExpr,
        stop: AotExpr,
        step: Option<AotExpr>,
        body: Vec<AotStmt>,
    },
    /// For-each loop: for var in iter
    ForEach {
        var: String,
        iter: AotExpr,
        body: Vec<AotStmt>,
    },
    /// Break statement
    Break,
    /// Continue statement
    Continue,
}

/// AoT expression
#[derive(Debug, Clone)]
pub enum AotExpr {
    // ========== Literals ==========
    /// 64-bit integer literal
    LitI64(i64),
    /// 32-bit integer literal
    LitI32(i32),
    /// 64-bit float literal
    LitF64(f64),
    /// 32-bit float literal
    LitF32(f32),
    /// Boolean literal
    LitBool(bool),
    /// String literal
    LitStr(String),
    /// Character literal
    LitChar(char),
    /// Nothing literal
    LitNothing,
    /// Missing literal
    LitMissing,

    // ========== Variables ==========
    /// Variable reference
    Var { name: String, ty: StaticType },

    // ========== Operations ==========
    /// Static binary operation (types are known)
    BinOpStatic {
        op: AotBinOp,
        left: Box<AotExpr>,
        right: Box<AotExpr>,
        result_ty: StaticType,
    },
    /// Dynamic binary operation (requires runtime dispatch)
    BinOpDynamic {
        op: AotBinOp,
        left: Box<AotExpr>,
        right: Box<AotExpr>,
    },
    /// Unary operation
    UnaryOp {
        op: AotUnaryOp,
        operand: Box<AotExpr>,
        result_ty: StaticType,
    },

    // ========== Function Calls ==========
    /// Static function call (fully typed)
    CallStatic {
        function: String,
        args: Vec<AotExpr>,
        return_ty: StaticType,
        inline_policy: AotInlinePolicy,
    },
    /// Dynamic function call (requires multiple dispatch)
    CallDynamic {
        function: String,
        args: Vec<AotExpr>,
    },
    /// Builtin function call
    CallBuiltin {
        builtin: AotBuiltinOp,
        args: Vec<AotExpr>,
        return_ty: StaticType,
    },

    // ========== Containers ==========
    /// Array literal (supports 1D and multidimensional arrays)
    ///
    /// # Shape
    /// - `[1, 2, 3]` → shape = [3] (1D array with 3 elements)
    /// - `[1 2; 3 4]` → shape = [2, 2] (2x2 matrix, column-major)
    /// - `[1 2 3; 4 5 6]` → shape = [2, 3] (2x3 matrix)
    ///
    /// For multidimensional arrays, elements are stored in column-major order
    /// (Julia convention).
    ArrayLit {
        elements: Vec<AotExpr>,
        elem_ty: StaticType,
        /// Shape of the array (e.g., [3] for 1D, [2, 3] for 2x3 matrix)
        shape: Vec<usize>,
    },
    /// Set construction lowered to a native HashSet build from a static iterator.
    SetFromIter {
        iter: Box<AotExpr>,
        elem_ty: StaticType,
    },
    /// Tuple literal
    TupleLit { elements: Vec<AotExpr> },
    /// NamedTuple literal, carried as a Rust tuple in field order.
    NamedTupleLit { fields: Vec<(String, AotExpr)> },
    /// Array comprehension lowered to a native Vec build.
    Comprehension {
        body: Box<AotExpr>,
        var: String,
        iter: Box<AotExpr>,
        filter: Option<Box<AotExpr>>,
        elem_ty: StaticType,
    },
    /// Multi-clause array comprehension lowered to nested native Vec loops.
    MultiComprehension {
        body: Box<AotExpr>,
        iterations: Vec<(String, AotExpr)>,
        filter: Option<Box<AotExpr>>,
        elem_ty: StaticType,
    },
    /// Lazy generator expression lowered to a boxed Rust iterator.
    Generator {
        body: Box<AotExpr>,
        var: String,
        iter: Box<AotExpr>,
        filter: Option<Box<AotExpr>>,
        elem_ty: StaticType,
    },
    /// Array/tuple indexing (supports 1D and multidimensional)
    ///
    /// # Examples
    /// - `arr[i]` → indices = [i]
    /// - `mat[i, j]` → indices = [i, j]
    /// - `tensor[i, j, k]` → indices = [i, j, k]
    /// - `t[1]` → tuple index (uses `.0` syntax in Rust)
    Index {
        array: Box<AotExpr>,
        /// Index expressions (one per dimension)
        indices: Vec<AotExpr>,
        elem_ty: StaticType,
        /// Whether this is a tuple index (uses `.0`, `.1` syntax in Rust)
        is_tuple: bool,
    },
    /// Range expression
    Range {
        start: Box<AotExpr>,
        stop: Box<AotExpr>,
        step: Option<Box<AotExpr>>,
        elem_ty: StaticType,
    },

    // ========== Structs ==========
    /// Struct construction
    StructNew { name: String, fields: Vec<AotExpr> },
    /// Field access
    FieldAccess {
        object: Box<AotExpr>,
        field: String,
        field_ty: StaticType,
    },

    // ========== Control ==========
    /// Ternary conditional expression: cond ? then : else
    Ternary {
        condition: Box<AotExpr>,
        then_expr: Box<AotExpr>,
        else_expr: Box<AotExpr>,
        result_ty: StaticType,
    },

    // ========== Dynamic Types ==========
    /// Box a value into Value type
    Box(Box<AotExpr>),
    /// Unbox from Value type to specific type
    Unbox {
        value: Box<AotExpr>,
        target_ty: StaticType,
    },

    // ========== Type Conversions ==========
    /// Type conversion/coercion expression
    ///
    /// Used for explicit type conversions, especially for return value coercion
    /// when a function has a declared return type annotation.
    ///
    /// # Example
    /// ```ignore
    /// // Julia: function f()::Float64; return 3; end
    /// // Return type is Float64, but return value is Int64
    /// // This generates: Convert { value: 3_i64, target_ty: F64 }
    /// // Which produces: 3_i64 as f64
    /// ```
    Convert {
        value: Box<AotExpr>,
        target_ty: StaticType,
    },

    // ========== Closures ==========
    /// Lambda/closure expression: (x, y) -> body
    ///
    /// Represents an anonymous function that can capture variables from
    /// the surrounding scope.
    ///
    /// # Examples
    /// ```ignore
    /// // Julia: x -> x + 1
    /// Lambda { params: [("x", I64)], body: x + 1, captures: [], return_ty: I64 }
    ///
    /// // Julia: (x, y) -> x + y
    /// Lambda { params: [("x", I64), ("y", I64)], body: x + y, captures: [], return_ty: I64 }
    ///
    /// // Julia closure capturing outer variable:
    /// // let a = 10
    /// // f = x -> x + a
    /// Lambda { params: [("x", I64)], body: x + a, captures: [("a", I64)], return_ty: I64 }
    /// ```
    Lambda {
        /// Parameters (name, type)
        params: Vec<(String, StaticType)>,
        /// Body expression
        body: Box<AotExpr>,
        /// Captured variables from outer scope (name, type)
        captures: Vec<(String, StaticType)>,
        /// Return type of the lambda
        return_ty: StaticType,
    },
}

impl AotExpr {
    /// Get the type of this expression
    pub fn get_type(&self) -> StaticType {
        match self {
            AotExpr::LitI64(_) => StaticType::I64,
            AotExpr::LitI32(_) => StaticType::I32,
            AotExpr::LitF64(_) => StaticType::F64,
            AotExpr::LitF32(_) => StaticType::F32,
            AotExpr::LitBool(_) => StaticType::Bool,
            AotExpr::LitStr(_) => StaticType::Str,
            AotExpr::LitChar(_) => StaticType::Char,
            AotExpr::LitNothing => StaticType::Nothing,
            AotExpr::LitMissing => StaticType::Missing,
            AotExpr::Var { ty, .. } => ty.clone(),
            AotExpr::BinOpStatic { result_ty, .. } => result_ty.clone(),
            AotExpr::BinOpDynamic { .. } => StaticType::Any,
            AotExpr::UnaryOp { result_ty, .. } => result_ty.clone(),
            AotExpr::CallStatic { return_ty, .. } => return_ty.clone(),
            AotExpr::CallDynamic { .. } => StaticType::Any,
            AotExpr::CallBuiltin { return_ty, .. } => return_ty.clone(),
            AotExpr::ArrayLit { elem_ty, shape, .. } => StaticType::Array {
                element: Box::new(elem_ty.clone()),
                ndims: Some(shape.len()),
            },
            AotExpr::SetFromIter { elem_ty, .. } => StaticType::Set {
                element: Box::new(elem_ty.clone()),
            },
            AotExpr::TupleLit { elements } => {
                StaticType::Tuple(elements.iter().map(|e| e.get_type()).collect())
            }
            AotExpr::NamedTupleLit { fields } => StaticType::NamedTuple(
                fields
                    .iter()
                    .map(|(name, expr)| (name.clone(), expr.get_type()))
                    .collect(),
            ),
            AotExpr::Comprehension { elem_ty, .. }
            | AotExpr::MultiComprehension { elem_ty, .. } => StaticType::Array {
                element: Box::new(elem_ty.clone()),
                ndims: Some(1),
            },
            AotExpr::Generator { elem_ty, .. } => StaticType::Generator {
                element: Box::new(elem_ty.clone()),
            },
            AotExpr::Index { elem_ty, .. } => elem_ty.clone(),
            AotExpr::Range { elem_ty, .. } => StaticType::Range {
                element: Box::new(elem_ty.clone()),
            },
            AotExpr::StructNew { name, .. } => StaticType::Struct {
                type_id: 0,
                name: name.clone(),
            },
            AotExpr::FieldAccess { field_ty, .. } => field_ty.clone(),
            AotExpr::Ternary { result_ty, .. } => result_ty.clone(),
            AotExpr::Box(_) => StaticType::Any,
            AotExpr::Unbox { target_ty, .. } => target_ty.clone(),
            AotExpr::Convert { target_ty, .. } => target_ty.clone(),
            AotExpr::Lambda {
                params, return_ty, ..
            } => StaticType::Function {
                params: params.iter().map(|(_, ty)| ty.clone()).collect(),
                ret: Box::new(return_ty.clone()),
            },
        }
    }

    /// Check if this expression has a fully static type
    pub fn is_fully_static(&self) -> bool {
        self.get_type().is_fully_static()
    }
}

#[cfg(test)]
mod prune_tests {
    use super::*;

    fn func_with_body(name: &str, body: Vec<AotStmt>) -> AotFunction {
        let mut f = AotFunction::new(name.to_string(), vec![], StaticType::Nothing);
        f.body = body;
        f
    }

    fn call_static(function: &str, args: Vec<AotExpr>) -> AotExpr {
        AotExpr::CallStatic {
            function: function.to_string(),
            args,
            return_ty: StaticType::Nothing,
            inline_policy: AotInlinePolicy::Auto,
        }
    }

    fn names(program: &AotProgram) -> Vec<&str> {
        program.functions.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn prune_removes_functions_unreachable_from_entry_issue_6629() {
        // main -> foo -> bar; `dead` is referenced by nothing reachable.
        let mut program = AotProgram::new();
        program.functions.push(func_with_body(
            "foo",
            vec![AotStmt::Expr(call_static("bar", vec![]))],
        ));
        program.functions.push(func_with_body("bar", vec![]));
        program.functions.push(func_with_body("dead", vec![]));
        program.main = vec![AotStmt::Expr(call_static("foo", vec![]))];

        program.prune_unreachable_functions();

        let kept = names(&program);
        assert!(kept.contains(&"foo"));
        assert!(kept.contains(&"bar"));
        assert!(
            !kept.contains(&"dead"),
            "unreachable `dead` should be pruned"
        );
    }

    #[test]
    fn prune_keeps_function_used_as_a_value_issue_6629() {
        // main: foo(g) — `g` is passed as a value (a function reference), so it
        // must stay reachable even though it is never directly called.
        let mut program = AotProgram::new();
        program.functions.push(func_with_body("foo", vec![]));
        program.functions.push(func_with_body("g", vec![]));
        program.functions.push(func_with_body("dead", vec![]));
        program.main = vec![AotStmt::Expr(call_static(
            "foo",
            vec![AotExpr::Var {
                name: "g".to_string(),
                ty: StaticType::Any,
            }],
        ))];

        program.prune_unreachable_functions();

        let kept = names(&program);
        assert!(kept.contains(&"foo"));
        assert!(
            kept.contains(&"g"),
            "function passed as a value must be kept"
        );
        assert!(!kept.contains(&"dead"));
    }

    #[test]
    fn prune_keeps_everything_when_dynamic_dispatch_is_reachable_issue_6629() {
        // A reachable dynamic call can target any method by runtime type, so
        // pruning is skipped entirely (no regression for dispatch programs).
        let mut program = AotProgram::new();
        program.functions.push(func_with_body("foo", vec![]));
        program.functions.push(func_with_body("dead", vec![]));
        program.main = vec![AotStmt::Expr(AotExpr::CallDynamic {
            function: "foo".to_string(),
            args: vec![],
        })];

        program.prune_unreachable_functions();

        let kept = names(&program);
        assert!(kept.contains(&"foo"));
        assert!(
            kept.contains(&"dead"),
            "dynamic dispatch present: all functions must be kept"
        );
    }
}
