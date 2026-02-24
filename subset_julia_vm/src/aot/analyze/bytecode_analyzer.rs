//! Bytecode analysis for AoT compilation.
//!
//! The `BytecodeAnalyzer` discovers functions, builds call graphs,
//! detects recursion, and identifies entry points in bytecode programs.

use super::super::call_graph::CallGraph;
use super::super::AotResult;
use super::loader::load_bytecode_bytes;
use crate::ir::core::{Block, Expr, Program, Stmt};
use crate::types::JuliaType;
use std::collections::{HashMap, HashSet};

/// Information about a function discovered during analysis
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name
    pub name: String,
    /// Bytecode offset where function starts
    pub offset: usize,
    /// Parameter types (if known)
    pub param_types: Vec<JuliaType>,
    /// Return type (if known)
    pub return_type: Option<JuliaType>,
    /// Set of called functions
    pub calls: HashSet<String>,
    /// Whether this function is recursive
    pub is_recursive: bool,
}

impl FunctionInfo {
    /// Create new function info
    pub fn new(name: String, offset: usize) -> Self {
        Self {
            name,
            offset,
            param_types: Vec::new(),
            return_type: None,
            calls: HashSet::new(),
            is_recursive: false,
        }
    }
}

/// Result of bytecode analysis
#[derive(Debug)]
pub struct AnalysisResult {
    /// Discovered functions
    pub functions: HashMap<String, FunctionInfo>,
    /// Global variables
    pub globals: HashMap<String, JuliaType>,
    /// Constants
    pub constants: Vec<ConstantInfo>,
    /// Entry point function name
    pub entry_point: Option<String>,
}

impl AnalysisResult {
    /// Create a new empty analysis result
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            globals: HashMap::new(),
            constants: Vec::new(),
            entry_point: None,
        }
    }

    /// Add a function
    pub fn add_function(&mut self, info: FunctionInfo) {
        self.functions.insert(info.name.clone(), info);
    }

    /// Get function info by name
    pub fn get_function(&self, name: &str) -> Option<&FunctionInfo> {
        self.functions.get(name)
    }
}

impl Default for AnalysisResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a constant value
#[derive(Debug, Clone)]
pub struct ConstantInfo {
    /// Constant index
    pub index: usize,
    /// Constant type
    pub ty: JuliaType,
    /// String representation of the value
    pub value: String,
}

/// Bytecode analyzer
///
/// This analyzer provides functionality to analyze bytecode files (.sjbc) for:
/// - Function discovery and enumeration
/// - Call graph construction
/// - Recursion detection (direct and mutual)
/// - Entry point identification
///
/// The analyzer works with the Core IR representation stored in bytecode files.
#[derive(Debug)]
pub struct BytecodeAnalyzer {
    /// Current analysis result
    result: AnalysisResult,
    /// Call graph edges (function -> set of called functions)
    call_graph: HashMap<String, HashSet<String>>,
}

impl BytecodeAnalyzer {
    /// Create a new analyzer
    pub fn new() -> Self {
        Self {
            result: AnalysisResult::new(),
            call_graph: HashMap::new(),
        }
    }

    /// Analyze bytecode
    ///
    /// This is the main entry point for bytecode analysis. It loads the bytecode,
    /// discovers functions, builds a call graph, and detects recursion.
    ///
    /// # Arguments
    ///
    /// * `bytecode` - The raw bytecode bytes to analyze
    ///
    /// # Returns
    ///
    /// Returns the analysis result containing function info, call relationships,
    /// and recursion detection.
    pub fn analyze(&mut self, bytecode: &[u8]) -> AotResult<AnalysisResult> {
        // Step 1: Load the program from bytecode
        let program = load_bytecode_bytes(bytecode)?;

        // Step 2: Discover all functions
        self.find_functions_from_program(&program);

        // Step 3: Build the call graph
        self.build_call_graph_from_program(&program);

        // Step 4: Detect recursive functions
        self.detect_recursion();

        // Step 5: Identify entry point (first function called from main)
        self.find_entry_point(&program);

        Ok(std::mem::take(&mut self.result))
    }

    /// Analyze a Core IR Program directly (for use when program is already loaded)
    ///
    /// This is useful when you already have a Program loaded and want to analyze it
    /// without going through the bytecode loading step.
    pub fn analyze_program(&mut self, program: &Program) -> AnalysisResult {
        // Step 1: Discover all functions
        self.find_functions_from_program(program);

        // Step 2: Build the call graph
        self.build_call_graph_from_program(program);

        // Step 3: Detect recursive functions
        self.detect_recursion();

        // Step 4: Identify entry point
        self.find_entry_point(program);

        std::mem::take(&mut self.result)
    }

    /// Find all function definitions in the program
    fn find_functions_from_program(&mut self, program: &Program) {
        for (idx, func) in program.functions.iter().enumerate() {
            let mut info = FunctionInfo::new(func.name.clone(), idx);

            // Extract parameter types
            for param in &func.params {
                info.param_types.push(param.effective_type());
            }

            // Extract return type
            info.return_type = func.return_type.clone();

            self.result.add_function(info);
        }

        // Also discover functions in modules
        for module in &program.modules {
            self.find_functions_in_module(module, &module.name);
        }
    }

    /// Find functions in a module recursively
    fn find_functions_in_module(&mut self, module: &crate::ir::core::Module, prefix: &str) {
        for (idx, func) in module.functions.iter().enumerate() {
            let full_name = format!("{}.{}", prefix, func.name);
            let mut info = FunctionInfo::new(full_name.clone(), idx);

            for param in &func.params {
                info.param_types.push(param.effective_type());
            }

            info.return_type = func.return_type.clone();

            self.result.add_function(info);
        }

        // Handle submodules recursively
        for submodule in &module.submodules {
            let sub_prefix = format!("{}.{}", prefix, submodule.name);
            self.find_functions_in_module(submodule, &sub_prefix);
        }
    }

    /// Build call graph from the program
    fn build_call_graph_from_program(&mut self, program: &Program) {
        // Use the existing CallGraph infrastructure
        let _cg = CallGraph::from_program(program);

        // For each function, find what it calls
        for func in &program.functions {
            let calls = self.collect_calls_in_block(&func.body);
            self.call_graph.insert(func.name.clone(), calls.clone());

            // Update the function info with calls
            if let Some(info) = self.result.functions.get_mut(&func.name) {
                info.calls = calls;
            }
        }

        // Handle module functions
        for module in &program.modules {
            self.build_call_graph_in_module(module, &module.name);
        }

        // Find entry point calls from main
        let main_calls = self.collect_calls_in_block(&program.main);
        self.call_graph.insert("__main__".to_string(), main_calls);
    }

    /// Build call graph for functions in a module
    fn build_call_graph_in_module(&mut self, module: &crate::ir::core::Module, prefix: &str) {
        for func in &module.functions {
            let full_name = format!("{}.{}", prefix, func.name);
            let calls = self.collect_calls_in_block(&func.body);
            self.call_graph.insert(full_name.clone(), calls.clone());

            if let Some(info) = self.result.functions.get_mut(&full_name) {
                info.calls = calls;
            }
        }

        for submodule in &module.submodules {
            let sub_prefix = format!("{}.{}", prefix, submodule.name);
            self.build_call_graph_in_module(submodule, &sub_prefix);
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
            Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
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
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.collect_calls_in_expr(expr, calls);
                }
            }
            _ => {}
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
                for arg in args {
                    self.collect_calls_in_expr(arg, calls);
                }
                for (_, v) in kwargs {
                    self.collect_calls_in_expr(v, calls);
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
            Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
                for elem in elements {
                    self.collect_calls_in_expr(elem, calls);
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
            Expr::FunctionRef { name, .. } => {
                calls.insert(name.clone());
            }
            Expr::Builtin { args, .. } => {
                for arg in args {
                    self.collect_calls_in_expr(arg, calls);
                }
            }
            _ => {}
        }
    }

    /// Detect recursive functions using DFS-based cycle detection
    ///
    /// This detects both direct recursion (function calls itself) and
    /// mutual recursion (A calls B, B calls A).
    fn detect_recursion(&mut self) {
        let func_names: Vec<String> = self.result.functions.keys().cloned().collect();

        for name in func_names {
            // Check for direct recursion first (fast path)
            if let Some(calls) = self.call_graph.get(&name) {
                if calls.contains(&name) {
                    if let Some(info) = self.result.functions.get_mut(&name) {
                        info.is_recursive = true;
                    }
                    continue;
                }
            }

            // Check for mutual recursion using DFS
            if self.has_cycle_to(&name, &name, &mut HashSet::new()) {
                if let Some(info) = self.result.functions.get_mut(&name) {
                    info.is_recursive = true;
                }
            }
        }
    }

    /// Check if there's a cycle from `start` back to `target` in the call graph
    fn has_cycle_to(&self, current: &str, target: &str, visited: &mut HashSet<String>) -> bool {
        if visited.contains(current) {
            return false;
        }
        visited.insert(current.to_string());

        if let Some(calls) = self.call_graph.get(current) {
            for callee in calls {
                if callee == target {
                    return true;
                }
                if self.result.functions.contains_key(callee)
                    && self.has_cycle_to(callee, target, visited)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Find the entry point function (first function called from main)
    fn find_entry_point(&mut self, program: &Program) {
        // Look for the first function call in main block
        for stmt in &program.main.stmts {
            if let Some(entry) = self.find_first_call_in_stmt(stmt) {
                self.result.entry_point = Some(entry);
                return;
            }
        }
    }

    /// Find the first function call in a statement
    fn find_first_call_in_stmt(&self, stmt: &Stmt) -> Option<String> {
        match stmt {
            Stmt::Expr { expr, .. } => self.find_first_call_in_expr(expr),
            Stmt::Assign { value, .. } => self.find_first_call_in_expr(value),
            _ => None,
        }
    }

    /// Find the first function call in an expression
    fn find_first_call_in_expr(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Call { function, .. } => Some(function.clone()),
            Expr::ModuleCall {
                module, function, ..
            } => Some(format!("{}.{}", module, function)),
            Expr::BinaryOp { left, right, .. } => self
                .find_first_call_in_expr(left)
                .or_else(|| self.find_first_call_in_expr(right)),
            _ => None,
        }
    }

    /// Get the computed call graph
    pub fn get_call_graph(&self) -> &HashMap<String, HashSet<String>> {
        &self.call_graph
    }
}

impl Default for BytecodeAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
