//! Call graph construction and reachability analysis for Dead Code Elimination.
//!
//! This module provides functionality to build a call graph from a Core IR Program
//! and perform reachability analysis to determine which functions are actually used.
//!
//! # Overview
//!
//! The call graph analysis works in three phases:
//! 1. **Build call graph**: Scan all functions and main block to find call edges
//! 2. **Find roots**: Identify entry points (main block calls, exported functions)
//! 3. **Compute reachability**: Mark all functions reachable from roots
//!
//! # Usage
//!
//! ```ignore
//! let call_graph = CallGraph::from_program(&program);
//! let reachable = call_graph.reachable_functions();
//! let filtered_program = call_graph.filter_program(&program);
//! ```

use crate::ir::core::{Block, Expr, Function, Module, Program, Stmt};
use std::collections::{HashMap, HashSet, VecDeque};

/// Call graph for a program
#[derive(Debug)]
pub struct CallGraph {
    /// Map from function name to set of called functions
    edges: HashMap<String, HashSet<String>>,
    /// Set of root functions (called from main or exported)
    roots: HashSet<String>,
    /// All function names in the program
    all_functions: HashSet<String>,
    /// Struct names referenced in the program
    referenced_structs: HashSet<String>,
    /// Module names referenced via ModuleCall expressions
    referenced_modules: HashSet<String>,
}

impl CallGraph {
    /// Build a call graph from a Core IR Program
    pub fn from_program(program: &Program) -> Self {
        let mut graph = Self {
            edges: HashMap::new(),
            roots: HashSet::new(),
            all_functions: HashSet::new(),
            referenced_structs: HashSet::new(),
            referenced_modules: HashSet::new(),
        };

        // Collect all function names
        for func in &program.functions {
            graph.all_functions.insert(func.name.clone());
            graph.edges.insert(func.name.clone(), HashSet::new());
        }

        // Collect module functions
        for module in &program.modules {
            graph.collect_module_functions(module);
        }

        // Build edges for each function
        for func in &program.functions {
            let calls = graph.collect_calls_in_block(&func.body);
            graph.edges.insert(func.name.clone(), calls);
        }

        // Build edges for module functions
        for module in &program.modules {
            graph.collect_module_edges(module);
        }

        // Find root functions from main block
        let main_calls = graph.collect_calls_in_block(&program.main);
        graph.roots.extend(main_calls);

        // Add struct references from main
        graph.collect_struct_refs_in_block(&program.main);

        // Collect module references from main and all function bodies
        graph.collect_module_refs_in_block(&program.main);
        for func in &program.functions {
            graph.collect_module_refs_in_block(&func.body);
        }
        for module in &program.modules {
            graph.collect_module_refs_in_module(module);
        }

        graph
    }

    /// Collect function names from a module
    fn collect_module_functions(&mut self, module: &Module) {
        for func in &module.functions {
            let full_name = format!("{}.{}", module.name, func.name);
            self.all_functions.insert(full_name.clone());
            self.edges.insert(full_name, HashSet::new());
            // Also add short name for resolution
            self.all_functions.insert(func.name.clone());
        }

        for submodule in &module.submodules {
            self.collect_module_functions(submodule);
        }
    }

    /// Collect edges from module functions
    fn collect_module_edges(&mut self, module: &Module) {
        for func in &module.functions {
            let full_name = format!("{}.{}", module.name, func.name);
            let calls = self.collect_calls_in_block(&func.body);
            self.edges.insert(full_name, calls.clone());
            // Also add with short name
            self.edges.insert(func.name.clone(), calls);
        }

        for submodule in &module.submodules {
            self.collect_module_edges(submodule);
        }
    }

    /// Collect all function calls in a block
    fn collect_calls_in_block(&self, block: &Block) -> HashSet<String> {
        let mut calls = HashSet::new();
        for stmt in &block.stmts {
            self.collect_calls_in_stmt(stmt, &mut calls);
        }
        calls
    }

    /// Collect function calls in a statement
    fn collect_calls_in_stmt(&self, stmt: &Stmt, calls: &mut HashSet<String>) {
        match stmt {
            Stmt::Block(block) => {
                calls.extend(self.collect_calls_in_block(block));
            }
            Stmt::Expr { expr, .. } => {
                self.collect_calls_in_expr(expr, calls);
            }
            Stmt::Assign { value, .. } => {
                self.collect_calls_in_expr(value, calls);
            }
            Stmt::AddAssign { value, .. } => {
                self.collect_calls_in_expr(value, calls);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_calls_in_expr(condition, calls);
                calls.extend(self.collect_calls_in_block(then_branch));
                if let Some(else_b) = else_branch {
                    calls.extend(self.collect_calls_in_block(else_b));
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.collect_calls_in_expr(condition, calls);
                calls.extend(self.collect_calls_in_block(body));
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                self.collect_calls_in_expr(start, calls);
                self.collect_calls_in_expr(end, calls);
                if let Some(s) = step {
                    self.collect_calls_in_expr(s, calls);
                }
                calls.extend(self.collect_calls_in_block(body));
            }
            Stmt::ForEach { iterable, body, .. } => {
                self.collect_calls_in_expr(iterable, calls);
                calls.extend(self.collect_calls_in_block(body));
            }
            Stmt::ForEachTuple { iterable, body, .. } => {
                self.collect_calls_in_expr(iterable, calls);
                calls.extend(self.collect_calls_in_block(body));
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.collect_calls_in_expr(expr, calls);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                calls.extend(self.collect_calls_in_block(try_block));
                if let Some(catch_b) = catch_block {
                    calls.extend(self.collect_calls_in_block(catch_b));
                }
                if let Some(else_b) = else_block {
                    calls.extend(self.collect_calls_in_block(else_b));
                }
                if let Some(finally_b) = finally_block {
                    calls.extend(self.collect_calls_in_block(finally_b));
                }
            }
            Stmt::Timed { body, .. } => {
                calls.extend(self.collect_calls_in_block(body));
            }
            Stmt::Test { condition, .. } => {
                self.collect_calls_in_expr(condition, calls);
            }
            Stmt::TestSet { body, .. } => {
                calls.extend(self.collect_calls_in_block(body));
            }
            Stmt::IndexAssign { indices, value, .. } => {
                for idx in indices {
                    self.collect_calls_in_expr(idx, calls);
                }
                self.collect_calls_in_expr(value, calls);
            }
            Stmt::FieldAssign { value, .. } => {
                self.collect_calls_in_expr(value, calls);
            }
            Stmt::DestructuringAssign { value, .. } => {
                self.collect_calls_in_expr(value, calls);
            }
            Stmt::DictAssign { key, value, .. } => {
                self.collect_calls_in_expr(key, calls);
                self.collect_calls_in_expr(value, calls);
            }
            Stmt::TestThrows { expr, .. } => {
                self.collect_calls_in_expr(expr, calls);
            }
            Stmt::Using { .. } | Stmt::Export { .. } => {}
            Stmt::FunctionDef { func, .. } => {
                // Add edges from this function definition
                calls.extend(self.collect_calls_in_block(&func.body));
            }
            // Label, Goto, and EnumDef are low-level control flow / declarations that don't contain function calls
            Stmt::Label { .. } | Stmt::Goto { .. } | Stmt::EnumDef { .. } => {}
        }
    }

    /// Collect function calls in an expression
    fn collect_calls_in_expr(&self, expr: &Expr, calls: &mut HashSet<String>) {
        match expr {
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                calls.insert(function.clone());
                for arg in args {
                    self.collect_calls_in_expr(arg, calls);
                }
                for (_, v) in kwargs {
                    self.collect_calls_in_expr(v, calls);
                }
            }
            Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
                ..
            } => {
                calls.insert(format!("{}.{}", module, function));
                calls.insert(function.clone()); // Also add short name
                for arg in args {
                    self.collect_calls_in_expr(arg, calls);
                }
                for (_, v) in kwargs {
                    self.collect_calls_in_expr(v, calls);
                }
            }
            Expr::Builtin { args, .. } => {
                for arg in args {
                    self.collect_calls_in_expr(arg, calls);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_calls_in_expr(left, calls);
                self.collect_calls_in_expr(right, calls);
            }
            Expr::UnaryOp { operand, .. } => {
                self.collect_calls_in_expr(operand, calls);
            }
            Expr::Index { array, indices, .. } => {
                self.collect_calls_in_expr(array, calls);
                for idx in indices {
                    self.collect_calls_in_expr(idx, calls);
                }
            }
            Expr::FieldAccess { object, .. } => {
                self.collect_calls_in_expr(object, calls);
            }
            Expr::ArrayLiteral { elements, .. } => {
                for elem in elements {
                    self.collect_calls_in_expr(elem, calls);
                }
            }
            Expr::TupleLiteral { elements, .. } => {
                for elem in elements {
                    self.collect_calls_in_expr(elem, calls);
                }
            }
            Expr::NamedTupleLiteral { fields, .. } => {
                for (_, v) in fields {
                    self.collect_calls_in_expr(v, calls);
                }
            }
            Expr::DictLiteral { pairs, .. } => {
                for (k, v) in pairs {
                    self.collect_calls_in_expr(k, calls);
                    self.collect_calls_in_expr(v, calls);
                }
            }
            Expr::Range {
                start, step, stop, ..
            } => {
                self.collect_calls_in_expr(start, calls);
                if let Some(s) = step {
                    self.collect_calls_in_expr(s, calls);
                }
                self.collect_calls_in_expr(stop, calls);
            }
            Expr::Comprehension {
                body, iter, filter, ..
            } => {
                self.collect_calls_in_expr(body, calls);
                self.collect_calls_in_expr(iter, calls);
                if let Some(f) = filter {
                    self.collect_calls_in_expr(f, calls);
                }
            }
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                self.collect_calls_in_expr(body, calls);
                for (_, iter_expr) in iterations {
                    self.collect_calls_in_expr(iter_expr, calls);
                }
                if let Some(f) = filter {
                    self.collect_calls_in_expr(f, calls);
                }
            }
            Expr::Generator {
                body, iter, filter, ..
            } => {
                self.collect_calls_in_expr(body, calls);
                self.collect_calls_in_expr(iter, calls);
                if let Some(f) = filter {
                    self.collect_calls_in_expr(f, calls);
                }
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                self.collect_calls_in_expr(condition, calls);
                self.collect_calls_in_expr(then_expr, calls);
                self.collect_calls_in_expr(else_expr, calls);
            }
            Expr::LetBlock { bindings, body, .. } => {
                for (_, v) in bindings {
                    self.collect_calls_in_expr(v, calls);
                }
                calls.extend(self.collect_calls_in_block(body));
            }
            Expr::AssignExpr { value, .. } => {
                self.collect_calls_in_expr(value, calls);
            }
            Expr::ReturnExpr { value, .. } => {
                if let Some(v) = value {
                    self.collect_calls_in_expr(v, calls);
                }
            }
            Expr::StringConcat { parts, .. } => {
                for p in parts {
                    self.collect_calls_in_expr(p, calls);
                }
            }
            Expr::Pair { key, value, .. } => {
                self.collect_calls_in_expr(key, calls);
                self.collect_calls_in_expr(value, calls);
            }
            Expr::FunctionRef { name, .. } => {
                calls.insert(name.clone());
            }
            Expr::New { args, .. } => {
                for arg in args {
                    self.collect_calls_in_expr(arg, calls);
                }
            }
            Expr::Literal(_, _)
            | Expr::Var(_, _)
            | Expr::SliceAll { .. }
            | Expr::TypedEmptyArray { .. } => {}
            // Handle other expression types
            _ => {
                // For any unhandled cases, recursively check if they contain calls
            }
        }
    }

    /// Collect struct references in a block (for filtering structs)
    fn collect_struct_refs_in_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.collect_struct_refs_in_stmt(stmt);
        }
    }

    /// Collect struct references in a statement
    fn collect_struct_refs_in_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Block(block) => self.collect_struct_refs_in_block(block),
            Stmt::Expr { expr, .. } => self.collect_struct_refs_in_expr(expr),
            Stmt::Assign { value, .. } => self.collect_struct_refs_in_expr(value),
            Stmt::AddAssign { value, .. } => self.collect_struct_refs_in_expr(value),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_struct_refs_in_expr(condition);
                self.collect_struct_refs_in_block(then_branch);
                if let Some(b) = else_branch {
                    self.collect_struct_refs_in_block(b);
                }
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                self.collect_struct_refs_in_expr(start);
                self.collect_struct_refs_in_expr(end);
                if let Some(s) = step {
                    self.collect_struct_refs_in_expr(s);
                }
                self.collect_struct_refs_in_block(body);
            }
            Stmt::ForEach { iterable, body, .. } => {
                self.collect_struct_refs_in_expr(iterable);
                self.collect_struct_refs_in_block(body);
            }
            Stmt::ForEachTuple { iterable, body, .. } => {
                self.collect_struct_refs_in_expr(iterable);
                self.collect_struct_refs_in_block(body);
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.collect_struct_refs_in_expr(condition);
                self.collect_struct_refs_in_block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.collect_struct_refs_in_expr(expr);
                }
            }
            _ => {}
        }
    }

    /// Collect struct references in an expression
    fn collect_struct_refs_in_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { function, args, .. } => {
                // Constructor calls look like struct names
                if function.chars().next().map_or(false, |c| c.is_uppercase()) {
                    self.referenced_structs.insert(function.clone());
                }
                for arg in args {
                    self.collect_struct_refs_in_expr(arg);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_struct_refs_in_expr(left);
                self.collect_struct_refs_in_expr(right);
            }
            Expr::UnaryOp { operand, .. } => {
                self.collect_struct_refs_in_expr(operand);
            }
            _ => {}
        }
    }

    /// Collect module names referenced via ModuleCall in a block
    fn collect_module_refs_in_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.collect_module_refs_in_stmt(stmt);
        }
    }

    /// Collect module names referenced via ModuleCall in a statement
    fn collect_module_refs_in_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Block(block) => self.collect_module_refs_in_block(block),
            Stmt::Expr { expr, .. } => self.collect_module_refs_in_expr(expr),
            Stmt::Assign { value, .. } => self.collect_module_refs_in_expr(value),
            Stmt::AddAssign { value, .. } => self.collect_module_refs_in_expr(value),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_module_refs_in_expr(condition);
                self.collect_module_refs_in_block(then_branch);
                if let Some(b) = else_branch {
                    self.collect_module_refs_in_block(b);
                }
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                self.collect_module_refs_in_expr(start);
                self.collect_module_refs_in_expr(end);
                if let Some(s) = step {
                    self.collect_module_refs_in_expr(s);
                }
                self.collect_module_refs_in_block(body);
            }
            Stmt::ForEach { iterable, body, .. } => {
                self.collect_module_refs_in_expr(iterable);
                self.collect_module_refs_in_block(body);
            }
            Stmt::ForEachTuple { iterable, body, .. } => {
                self.collect_module_refs_in_expr(iterable);
                self.collect_module_refs_in_block(body);
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.collect_module_refs_in_expr(condition);
                self.collect_module_refs_in_block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.collect_module_refs_in_expr(expr);
                }
            }
            _ => {}
        }
    }

    /// Collect module names referenced via ModuleCall in an expression
    fn collect_module_refs_in_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::ModuleCall {
                module,
                args,
                kwargs,
                ..
            } => {
                self.referenced_modules.insert(module.clone());
                for arg in args {
                    self.collect_module_refs_in_expr(arg);
                }
                for (_, v) in kwargs {
                    self.collect_module_refs_in_expr(v);
                }
            }
            Expr::Call { args, kwargs, .. } => {
                for arg in args {
                    self.collect_module_refs_in_expr(arg);
                }
                for (_, v) in kwargs {
                    self.collect_module_refs_in_expr(v);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_module_refs_in_expr(left);
                self.collect_module_refs_in_expr(right);
            }
            Expr::UnaryOp { operand, .. } => {
                self.collect_module_refs_in_expr(operand);
            }
            _ => {}
        }
    }

    /// Collect module references from module function bodies (recursive).
    /// Scans function bodies for ModuleCall expressions that reference other modules.
    fn collect_module_refs_in_module(&mut self, module: &Module) {
        for func in &module.functions {
            self.collect_module_refs_in_block(&func.body);
        }
        for submodule in &module.submodules {
            self.collect_module_refs_in_module(submodule);
        }
    }

    /// Compute the set of reachable functions from roots
    pub fn reachable_functions(&self) -> HashSet<String> {
        let mut reachable = HashSet::new();
        let mut worklist: VecDeque<String> = self.roots.iter().cloned().collect();

        while let Some(func) = worklist.pop_front() {
            if reachable.contains(&func) {
                continue;
            }

            // Only add if it's a known function
            if self.all_functions.contains(&func) {
                reachable.insert(func.clone());

                // Add all functions called by this function
                if let Some(callees) = self.edges.get(&func) {
                    for callee in callees {
                        if !reachable.contains(callee) {
                            worklist.push_back(callee.clone());
                        }
                    }
                }
            }
        }

        reachable
    }

    /// Filter a program to only include reachable functions
    pub fn filter_program(&self, program: &Program) -> Program {
        let reachable = self.reachable_functions();

        // Filter functions
        let filtered_functions: Vec<Function> = program
            .functions
            .iter()
            .filter(|f| reachable.contains(&f.name))
            .cloned()
            .collect();

        // Build set of referenced struct names from reachable functions
        let mut struct_refs: HashSet<String> = self.referenced_structs.clone();
        for func in &filtered_functions {
            self.collect_struct_names_in_function(func, &mut struct_refs);
        }

        // Filter structs - keep those that are referenced
        let filtered_structs = program
            .structs
            .iter()
            .filter(|s| struct_refs.contains(&s.name))
            .cloned()
            .collect();

        // Filter abstract types - keep those referenced by kept structs
        let filtered_abstract_types = program
            .abstract_types
            .iter()
            .filter(|a| {
                // Keep if any struct has this as parent
                program.structs.iter().any(|s| {
                    struct_refs.contains(&s.name) && s.parent_type.as_ref() == Some(&a.name)
                })
            })
            .cloned()
            .collect();

        // Filter modules - keep only those referenced via ModuleCall expressions
        let filtered_modules = program
            .modules
            .iter()
            .filter(|m| self.referenced_modules.contains(&m.name))
            .cloned()
            .collect();

        Program {
            functions: filtered_functions,
            structs: filtered_structs,
            abstract_types: filtered_abstract_types,
            type_aliases: program.type_aliases.clone(),
            modules: filtered_modules,
            usings: program.usings.clone(),
            macros: program.macros.clone(),
            enums: program.enums.clone(),
            base_function_count: 0, // Reset since we're filtering
            main: program.main.clone(),
        }
    }

    /// Collect struct names referenced in a function
    fn collect_struct_names_in_function(&self, func: &Function, refs: &mut HashSet<String>) {
        // Check parameter types
        for param in &func.params {
            if let Some(ref ty) = param.type_annotation {
                self.extract_struct_names_from_julia_type(ty, refs);
            }
        }
        // Check return type
        if let Some(ref ty) = func.return_type {
            self.extract_struct_names_from_julia_type(ty, refs);
        }
        // Check body for constructor calls
        self.collect_struct_refs_in_block_to_set(&func.body, refs);
    }

    /// Extract struct names from a JuliaType
    fn extract_struct_names_from_julia_type(
        &self,
        ty: &crate::types::JuliaType,
        refs: &mut HashSet<String>,
    ) {
        use crate::types::JuliaType;
        match ty {
            JuliaType::Struct(name) => {
                refs.insert(name.clone());
            }
            JuliaType::VectorOf(elem_type) | JuliaType::MatrixOf(elem_type) => {
                self.extract_struct_names_from_julia_type(elem_type, refs);
            }
            JuliaType::TupleOf(elements) => {
                for elem in elements {
                    self.extract_struct_names_from_julia_type(elem, refs);
                }
            }
            JuliaType::Union(types) => {
                for t in types {
                    self.extract_struct_names_from_julia_type(t, refs);
                }
            }
            _ => {}
        }
    }

    /// Collect struct references to a set
    fn collect_struct_refs_in_block_to_set(&self, block: &Block, refs: &mut HashSet<String>) {
        for stmt in &block.stmts {
            self.collect_struct_refs_in_stmt_to_set(stmt, refs);
        }
    }

    fn collect_struct_refs_in_stmt_to_set(&self, stmt: &Stmt, refs: &mut HashSet<String>) {
        match stmt {
            Stmt::Block(block) => self.collect_struct_refs_in_block_to_set(block, refs),
            Stmt::Expr { expr, .. } => self.collect_struct_refs_in_expr_to_set(expr, refs),
            Stmt::Assign { value, .. } => self.collect_struct_refs_in_expr_to_set(value, refs),
            Stmt::AddAssign { value, .. } => self.collect_struct_refs_in_expr_to_set(value, refs),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_struct_refs_in_expr_to_set(condition, refs);
                self.collect_struct_refs_in_block_to_set(then_branch, refs);
                if let Some(b) = else_branch {
                    self.collect_struct_refs_in_block_to_set(b, refs);
                }
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                self.collect_struct_refs_in_expr_to_set(start, refs);
                self.collect_struct_refs_in_expr_to_set(end, refs);
                if let Some(s) = step {
                    self.collect_struct_refs_in_expr_to_set(s, refs);
                }
                self.collect_struct_refs_in_block_to_set(body, refs);
            }
            Stmt::ForEach { iterable, body, .. } => {
                self.collect_struct_refs_in_expr_to_set(iterable, refs);
                self.collect_struct_refs_in_block_to_set(body, refs);
            }
            Stmt::ForEachTuple { iterable, body, .. } => {
                self.collect_struct_refs_in_expr_to_set(iterable, refs);
                self.collect_struct_refs_in_block_to_set(body, refs);
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.collect_struct_refs_in_expr_to_set(condition, refs);
                self.collect_struct_refs_in_block_to_set(body, refs);
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.collect_struct_refs_in_expr_to_set(expr, refs);
                }
            }
            _ => {}
        }
    }

    fn collect_struct_refs_in_expr_to_set(&self, expr: &Expr, refs: &mut HashSet<String>) {
        match expr {
            Expr::Call { function, args, .. } => {
                if function.chars().next().map_or(false, |c| c.is_uppercase()) {
                    refs.insert(function.clone());
                }
                for arg in args {
                    self.collect_struct_refs_in_expr_to_set(arg, refs);
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_struct_refs_in_expr_to_set(left, refs);
                self.collect_struct_refs_in_expr_to_set(right, refs);
            }
            Expr::UnaryOp { operand, .. } => {
                self.collect_struct_refs_in_expr_to_set(operand, refs);
            }
            Expr::ArrayLiteral { elements, .. } => {
                for e in elements {
                    self.collect_struct_refs_in_expr_to_set(e, refs);
                }
            }
            _ => {}
        }
    }

    /// Get statistics about the call graph
    pub fn stats(&self) -> CallGraphStats {
        let reachable = self.reachable_functions();
        CallGraphStats {
            total_functions: self.all_functions.len(),
            reachable_functions: reachable.len(),
            root_functions: self.roots.len(),
            eliminated_functions: self.all_functions.len() - reachable.len(),
        }
    }
}

/// Statistics about call graph analysis
#[derive(Debug, Clone)]
pub struct CallGraphStats {
    /// Total number of functions in the program
    pub total_functions: usize,
    /// Number of functions reachable from entry points
    pub reachable_functions: usize,
    /// Number of root functions (entry points)
    pub root_functions: usize,
    /// Number of functions eliminated by DCE
    pub eliminated_functions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 1, 1, 0, 0)
    }

    fn make_call_expr(name: &str) -> Expr {
        Expr::Call {
            function: name.to_string(),
            args: vec![],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span: dummy_span(),
        }
    }

    fn make_function(name: &str, calls: Vec<&str>) -> Function {
        let stmts: Vec<Stmt> = calls
            .into_iter()
            .map(|c| Stmt::Expr {
                expr: make_call_expr(c),
                span: dummy_span(),
            })
            .collect();

        Function {
            name: name.to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts,
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        }
    }

    #[test]
    fn test_simple_call_graph() {
        let program = Program {
            functions: vec![
                make_function("foo", vec!["bar"]),
                make_function("bar", vec![]),
                make_function("unused", vec![]),
            ],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            base_function_count: 0,
            main: Block {
                stmts: vec![Stmt::Expr {
                    expr: make_call_expr("foo"),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
        };

        let graph = CallGraph::from_program(&program);
        let reachable = graph.reachable_functions();

        assert!(reachable.contains("foo"));
        assert!(reachable.contains("bar"));
        assert!(!reachable.contains("unused"));
    }

    #[test]
    fn test_recursive_function() {
        let factorial = make_function("factorial", vec!["factorial"]);

        let program = Program {
            functions: vec![factorial],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            base_function_count: 0,
            main: Block {
                stmts: vec![Stmt::Expr {
                    expr: make_call_expr("factorial"),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
        };

        let graph = CallGraph::from_program(&program);
        let reachable = graph.reachable_functions();

        assert!(reachable.contains("factorial"));
        assert_eq!(reachable.len(), 1);
    }

    #[test]
    fn test_stats() {
        let program = Program {
            functions: vec![
                make_function("used1", vec!["used2"]),
                make_function("used2", vec![]),
                make_function("unused1", vec![]),
                make_function("unused2", vec!["unused1"]),
            ],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            base_function_count: 0,
            main: Block {
                stmts: vec![Stmt::Expr {
                    expr: make_call_expr("used1"),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
        };

        let graph = CallGraph::from_program(&program);
        let stats = graph.stats();

        assert_eq!(stats.total_functions, 4);
        assert_eq!(stats.reachable_functions, 2);
        assert_eq!(stats.eliminated_functions, 2);
    }

    #[test]
    fn test_assign_expr_inside_letblock_marks_call_reachable() {
        let foo = make_function("foo", vec![]);

        let timed_like_expr = Expr::LetBlock {
            bindings: vec![],
            body: Block {
                stmts: vec![Stmt::Assign {
                    var: "#result#1".to_string(),
                    value: Expr::AssignExpr {
                        var: "grid".to_string(),
                        value: Box::new(make_call_expr("foo")),
                        span: dummy_span(),
                    },
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            span: dummy_span(),
        };

        let program = Program {
            functions: vec![foo],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            base_function_count: 0,
            main: Block {
                stmts: vec![Stmt::Expr {
                    expr: timed_like_expr,
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
        };

        let graph = CallGraph::from_program(&program);
        let reachable = graph.reachable_functions();
        assert!(reachable.contains("foo"));
    }

    fn make_module_call_expr(module: &str, function: &str) -> Expr {
        Expr::ModuleCall {
            module: module.to_string(),
            function: function.to_string(),
            args: vec![],
            kwargs: vec![],
            span: dummy_span(),
        }
    }

    fn make_empty_module(name: &str) -> Module {
        Module {
            name: name.to_string(),
            is_bare: false,
            functions: vec![],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            submodules: vec![],
            usings: vec![],
            macros: vec![],
            exports: vec![],
            publics: vec![],
            body: Block {
                stmts: vec![],
                span: dummy_span(),
            },
            span: dummy_span(),
        }
    }

    #[test]
    fn test_filter_program_only_keeps_referenced_modules() {
        // UsedModule is referenced from main via ModuleCall; UnusedModule is not.
        let used_module = make_empty_module("UsedModule");
        let unused_module = make_empty_module("UnusedModule");

        let program = Program {
            functions: vec![],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            modules: vec![used_module, unused_module],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            base_function_count: 0,
            main: Block {
                stmts: vec![Stmt::Expr {
                    expr: make_module_call_expr("UsedModule", "foo"),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
        };

        let graph = CallGraph::from_program(&program);
        let filtered = graph.filter_program(&program);

        assert_eq!(filtered.modules.len(), 1);
        assert!(filtered.modules.iter().any(|m| m.name == "UsedModule"));
        assert!(!filtered.modules.iter().any(|m| m.name == "UnusedModule"));
    }

    #[test]
    fn test_filter_program_keeps_all_modules_when_all_used() {
        let module_a = make_empty_module("ModuleA");
        let module_b = make_empty_module("ModuleB");

        let program = Program {
            functions: vec![],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            modules: vec![module_a, module_b],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            base_function_count: 0,
            main: Block {
                stmts: vec![
                    Stmt::Expr {
                        expr: make_module_call_expr("ModuleA", "foo"),
                        span: dummy_span(),
                    },
                    Stmt::Expr {
                        expr: make_module_call_expr("ModuleB", "bar"),
                        span: dummy_span(),
                    },
                ],
                span: dummy_span(),
            },
        };

        let graph = CallGraph::from_program(&program);
        let filtered = graph.filter_program(&program);

        assert_eq!(filtered.modules.len(), 2);
    }

    #[test]
    fn test_filter_program_removes_all_modules_when_none_used() {
        let module_a = make_empty_module("ModuleA");
        let module_b = make_empty_module("ModuleB");

        let program = Program {
            functions: vec![make_function("foo", vec![])],
            structs: vec![],
            abstract_types: vec![],
            type_aliases: vec![],
            modules: vec![module_a, module_b],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            base_function_count: 0,
            main: Block {
                stmts: vec![Stmt::Expr {
                    expr: make_call_expr("foo"),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
        };

        let graph = CallGraph::from_program(&program);
        let filtered = graph.filter_program(&program);

        // No module calls in main or foo() — no modules kept
        assert_eq!(filtered.modules.len(), 0);
    }
}
