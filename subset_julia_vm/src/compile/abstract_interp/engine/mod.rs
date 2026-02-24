//! Abstract interpretation engine for type inference.
//!
//! This module implements the core fixpoint loop for abstract interpretation,
//! inferring types through control flow analysis.

// SAFETY: i64→usize cast is for a 1-based index guarded by `if idx_0based < elements.len()`.
#![allow(clippy::cast_sign_loss)]

use crate::compile::abstract_interp::{StructTypeInfo, TypeEnv};
use crate::compile::const_prop::{try_eval_binary, try_eval_unary};
use crate::compile::diagnostics::{
    emit_recursive_cycle, emit_unknown_array_element, emit_unknown_field, DiagnosticReason,
    DiagnosticsCollector, TypeInferenceDiagnostic,
};
use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
use crate::compile::lattice::widening::MAX_INFERENCE_ITERATIONS;
use crate::compile::tfuncs::{TFuncContext, TransferFunctions};
use crate::ir::core::{BinaryOp, Block, BuiltinOp, Expr, Function, Literal, Stmt};
use crate::types::JuliaType;
use std::collections::HashMap;

const MAX_LOOP_FIXPOINT_ITERATIONS: usize = 10;
const MAX_INTERPROCEDURAL_ANALYSIS_DEPTH: usize = 10;

/// Result of statement inference.
#[derive(Debug)]
enum StmtResult {
    /// Statement completed normally
    Continue,
    /// Statement returned a value (explicit `return` statement)
    Return(LatticeType),
}

/// Cache key for interprocedural type inference.
///
/// The cache key includes both the function name and the argument types,
/// enabling polymorphic functions to be analyzed with different argument
/// type combinations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CallCacheKey {
    /// Function name
    name: String,
    /// Argument types (hashed directly via LatticeType's Hash impl)
    arg_types: Vec<LatticeType>,
}

impl CallCacheKey {
    /// Creates a new cache key from function name and argument types.
    fn new(name: &str, arg_types: &[LatticeType]) -> Self {
        Self {
            name: name.to_string(),
            arg_types: arg_types.to_vec(),
        }
    }
}

/// Abstract interpretation engine for type inference.
///
/// The engine performs fixpoint iteration to infer types for variables
/// and function return values using abstract interpretation.
pub struct InferenceEngine {
    /// Transfer functions for inferring call return types
    tfuncs: TransferFunctions,
    /// Cache of inferred function return types by (function name, arg types) key.
    /// This allows polymorphic functions to return different types based on argument types.
    return_type_cache: HashMap<CallCacheKey, LatticeType>,
    /// Struct type information table (struct name -> StructTypeInfo)
    struct_table: HashMap<String, StructTypeInfo>,
    /// Function table for interprocedural analysis (function name -> Function)
    function_table: HashMap<String, Function>,
    /// Set of (function, arg_types) currently being analyzed (for cycle detection)
    analyzing_functions: std::collections::HashSet<CallCacheKey>,
    /// Current recursion depth for interprocedural analysis
    analysis_depth: usize,
}

impl InferenceEngine {
    /// Creates a new inference engine with registered transfer functions.
    pub fn new() -> Self {
        Self::with_struct_table(HashMap::new())
    }

    /// Creates a new inference engine with a given struct table.
    pub fn with_struct_table(struct_table: HashMap<String, StructTypeInfo>) -> Self {
        Self::with_tables(struct_table, HashMap::new())
    }

    /// Creates a new inference engine with struct table and function table.
    ///
    /// The function table enables interprocedural analysis by allowing
    /// the engine to analyze called functions to determine their return types.
    pub fn with_tables(
        struct_table: HashMap<String, StructTypeInfo>,
        function_table: HashMap<String, Function>,
    ) -> Self {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);

        Self {
            tfuncs,
            return_type_cache: HashMap::new(),
            struct_table,
            function_table,
            analyzing_functions: std::collections::HashSet::new(),
            analysis_depth: 0,
        }
    }

    /// Adds a function to the function table for interprocedural analysis.
    pub fn add_function(&mut self, func: Function) {
        self.function_table.insert(func.name.clone(), func);
    }

    /// Adds multiple functions to the function table.
    pub fn add_functions(&mut self, funcs: impl IntoIterator<Item = Function>) {
        for func in funcs {
            self.add_function(func);
        }
    }

    /// Infers the return type of a function.
    ///
    /// Uses fixpoint iteration to handle recursive calls and loops.
    /// Returns the inferred return type or Top if inference fails.
    pub fn infer_function(&mut self, func: &Function) -> LatticeType {
        // Build argument types from parameter annotations (for cache key)
        let arg_types: Vec<LatticeType> = func
            .params
            .iter()
            .map(|param| {
                if param.is_varargs {
                    // For varargs, use an empty Tuple as the default type
                    // since we don't know how many arguments will be passed
                    LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
                } else if let Some(ty) = &param.type_annotation {
                    self.julia_type_to_lattice(ty)
                } else {
                    LatticeType::Top
                }
            })
            .collect();

        // Check cache first (using function name + arg types as key)
        let cache_key = CallCacheKey::new(&func.name, &arg_types);
        if let Some(cached) = self.return_type_cache.get(&cache_key) {
            return cached.clone();
        }

        // Initialize environment with parameter types
        let mut env = TypeEnv::new();
        for (param, param_type) in func.params.iter().zip(arg_types.iter()) {
            env.set(&param.name, param_type.clone());
        }

        // Run fixpoint iteration
        let return_type = self.infer_block_with_fixpoint(&func.body, &mut env);

        // Cache the result
        self.return_type_cache
            .insert(cache_key, return_type.clone());

        return_type
    }

    /// Infers the return type of a function using explicit argument types.
    ///
    /// This enables call-site specialization without requiring parameter annotations.
    pub fn infer_function_with_arg_types(
        &mut self,
        func: &Function,
        arg_types: &[LatticeType],
    ) -> LatticeType {
        let cache_key = CallCacheKey::new(&func.name, arg_types);
        if let Some(cached) = self.return_type_cache.get(&cache_key) {
            return cached.clone();
        }

        let mut env = TypeEnv::new();
        let num_args = arg_types.len();

        for (idx, param) in func.params.iter().enumerate() {
            let param_type = if param.is_varargs {
                // Varargs parameter: collect all remaining arguments into a Tuple
                // If no remaining arguments, this is an empty Tuple
                if idx < num_args {
                    // Collect remaining args (from idx to end) into a Tuple
                    let remaining: Vec<_> = arg_types[idx..].to_vec();
                    if remaining.is_empty() {
                        // Empty varargs: create empty Tuple
                        // Type annotation is for the element type, not the tuple itself
                        LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
                    } else if remaining.len() == 1 {
                        // Single vararg: treat as Tuple with one element
                        // But for iteration purposes, we want the Tuple type itself
                        // so that `for y in ys` works correctly
                        if let LatticeType::Concrete(ct) = &remaining[0] {
                            LatticeType::Concrete(ConcreteType::Tuple {
                                elements: vec![ct.clone()],
                            })
                        } else {
                            LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
                        }
                    } else {
                        // Multiple varargs: create Tuple with all elements
                        let elements: Vec<_> = remaining
                            .iter()
                            .filter_map(|t| {
                                if let LatticeType::Concrete(ct) = t {
                                    Some(ct.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        LatticeType::Concrete(ConcreteType::Tuple { elements })
                    }
                } else {
                    // No arguments for varargs: empty Tuple
                    LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
                }
            } else if let Some(arg_ty) = arg_types.get(idx) {
                arg_ty.clone()
            } else if let Some(ann) = &param.type_annotation {
                self.julia_type_to_lattice(ann)
            } else {
                LatticeType::Top
            };
            env.set(&param.name, param_type);
        }

        let return_type = self.infer_block_with_fixpoint(&func.body, &mut env);
        self.return_type_cache
            .insert(cache_key, return_type.clone());
        return_type
    }

    /// Infers types for a block using fixpoint iteration.
    ///
    /// Iterates until types stabilize or max iterations reached.
    fn infer_block_with_fixpoint(&mut self, block: &Block, env: &mut TypeEnv) -> LatticeType {
        let mut iteration = 0;
        let mut prev_return_type = LatticeType::Bottom;

        while iteration < MAX_INFERENCE_ITERATIONS {
            iteration += 1;

            let current_return_type = self.infer_block(block, env);

            // Check if we've reached a fixpoint
            if current_return_type == prev_return_type {
                return current_return_type;
            }

            prev_return_type = current_return_type.clone();
        }

        // Max iterations reached: return current best guess
        prev_return_type
    }

    /// Infers types for a block of statements.
    ///
    /// Returns the inferred return type of the block.
    /// In Julia, the value of a block is the value of its last expression/statement.
    fn infer_block(&mut self, block: &Block, env: &mut TypeEnv) -> LatticeType {
        let mut return_type: Option<LatticeType> = None;
        let mut last_stmt_type: Option<LatticeType> = None;

        for stmt in &block.stmts {
            // Track the type of the last statement's value
            // In Julia, most statements have a value:
            // - Expression statements: the expression's value
            // - Assignment statements: the assigned value
            // - Control flow (if/while/for): typically Nothing
            // - Return statements are handled separately via StmtResult::Return
            match stmt {
                Stmt::Expr { expr, .. } => {
                    last_stmt_type = Some(self.infer_expr(expr, env));
                }
                Stmt::Assign { value, .. } => {
                    // Assignment evaluates to the assigned value
                    last_stmt_type = Some(self.infer_expr(value, env));
                }
                Stmt::Break { .. } | Stmt::Continue { .. } => {
                    // Flow control doesn't change the implicit return
                }
                _ => {
                    // Other statements (if, while, for, etc.) typically evaluate to Nothing
                    last_stmt_type = Some(LatticeType::Concrete(ConcreteType::Nothing));
                }
            }

            match self.infer_stmt(stmt, env) {
                StmtResult::Continue => {}
                StmtResult::Return(ty) => {
                    return_type = Some(if let Some(existing) = return_type {
                        existing.join(&ty)
                    } else {
                        ty
                    });
                }
            }
        }

        // If we have explicit returns, use that type
        // Otherwise, use the last statement's type as the implicit return
        // If neither, return Nothing
        if let Some(rt) = return_type {
            // Only use explicit return types
            rt
        } else {
            last_stmt_type.unwrap_or(LatticeType::Concrete(ConcreteType::Nothing))
        }
    }

    /// Infers types for a block but only returns non-Nothing if there is an explicit
    /// `return` statement inside the block. The implicit block value (last statement's
    /// value) is ignored for the return type.
    ///
    /// This is used for loop bodies (while, for, foreach) where the body's implicit value
    /// does not contribute to the enclosing function's return type. Only explicit `return`
    /// statements inside the loop body should propagate as function returns.
    fn infer_block_explicit_return_only(
        &mut self,
        block: &Block,
        env: &mut TypeEnv,
    ) -> LatticeType {
        let mut return_type: Option<LatticeType> = None;

        for stmt in &block.stmts {
            match self.infer_stmt(stmt, env) {
                StmtResult::Continue => {}
                StmtResult::Return(ty) => {
                    return_type = Some(if let Some(existing) = return_type {
                        existing.join(&ty)
                    } else {
                        ty
                    });
                }
            }
        }

        return_type.unwrap_or(LatticeType::Concrete(ConcreteType::Nothing))
    }

    /// Infers types for a statement.
    fn infer_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> StmtResult {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                let value_type = self.infer_expr(value, env);
                env.set(var, value_type);
                StmtResult::Continue
            }

            Stmt::Return { value, .. } => {
                let return_type = if let Some(expr) = value {
                    self.infer_expr(expr, env)
                } else {
                    LatticeType::Concrete(ConcreteType::Nothing)
                };
                StmtResult::Return(return_type)
            }

            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                // Infer condition type
                let _ = self.infer_expr(condition, env);

                // Apply conditional narrowing
                let split = crate::compile::abstract_interp::conditional::split_env_by_condition(
                    env, condition,
                );

                // Infer then branch with narrowed environment
                let mut then_env = split.then_env;
                let then_return = self.infer_block(then_branch, &mut then_env);

                // Infer else branch with narrowed environment
                let mut else_env = split.else_env;
                let else_return = if let Some(else_blk) = else_branch {
                    self.infer_block(else_blk, &mut else_env)
                } else {
                    LatticeType::Concrete(ConcreteType::Nothing)
                };

                // Merge environments from both branches
                *env = then_env;
                env.merge(&else_env);

                // Join return types
                let combined_return = then_return.join(&else_return);

                if combined_return != LatticeType::Concrete(ConcreteType::Nothing) {
                    StmtResult::Return(combined_return)
                } else {
                    StmtResult::Continue
                }
            }

            Stmt::For {
                var,
                start,
                end,
                step,
                body,
                ..
            } => {
                // Range-based for loop: for var in start:end or for var in start:step:end
                // Infer the types of start, end, step
                let _start_ty = self.infer_expr(start, env);
                let _end_ty = self.infer_expr(end, env);
                if let Some(step_expr) = step {
                    let _ = self.infer_expr(step_expr, env);
                }

                // Loop variable is I64 for range-based loops
                env.set(var, LatticeType::Concrete(ConcreteType::Int64));

                // Snapshot environment before loop
                let pre_loop_env = env.snapshot();

                // Infer loop body with fixpoint iteration (Issue #3360: reuse body_env)
                let mut changed = true;
                let mut iterations = 0;
                let mut body_env = env.clone();

                while changed && iterations < MAX_LOOP_FIXPOINT_ITERATIONS {
                    iterations += 1;

                    body_env.clone_from(env);
                    let body_return = self.infer_block_explicit_return_only(body, &mut body_env);

                    // Merge updated types from loop body
                    changed = env.merge_changed(&body_env);

                    // If body has an explicit return, propagate it
                    if !matches!(body_return, LatticeType::Concrete(ConcreteType::Nothing)) {
                        env.merge(&pre_loop_env);
                        return StmtResult::Return(body_return);
                    }
                }

                // After loop: merge pre-loop types directly into post-loop env (Issue #3360)
                env.merge(&pre_loop_env);

                StmtResult::Continue
            }

            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // Infer the type of the iterable
                let iterable_ty = self.infer_expr(iterable, env);

                // Extract element type from the iterable
                let elem_ty =
                    crate::compile::abstract_interp::loop_analysis::element_type(&iterable_ty);

                // Set initial loop variable type
                env.set(var, elem_ty.clone());

                // Snapshot environment before loop
                let pre_loop_env = env.snapshot();

                // Infer loop body with fixpoint iteration (Issue #3360: reuse body_env)
                let mut changed = true;
                let mut iterations = 0;
                let mut body_env = env.clone();

                while changed && iterations < MAX_LOOP_FIXPOINT_ITERATIONS {
                    iterations += 1;

                    body_env.clone_from(env);
                    let body_return = self.infer_block_explicit_return_only(body, &mut body_env);

                    // Merge updated types from loop body
                    changed = env.merge_changed(&body_env);

                    // If body has an explicit return, propagate it
                    if !matches!(body_return, LatticeType::Concrete(ConcreteType::Nothing)) {
                        env.merge(&pre_loop_env);
                        return StmtResult::Return(body_return);
                    }
                }

                // After loop: merge pre-loop types directly into post-loop env (Issue #3360)
                env.merge(&pre_loop_env);

                StmtResult::Continue
            }

            Stmt::While {
                condition, body, ..
            } => {
                // Snapshot environment before loop
                let pre_loop_env = env.snapshot();

                // Infer loop body with fixpoint iteration (Issue #3360: reuse body_env)
                let mut changed = true;
                let mut iterations = 0;
                let mut body_env = env.clone();

                while changed && iterations < MAX_LOOP_FIXPOINT_ITERATIONS {
                    iterations += 1;

                    // Infer condition type
                    let _ = self.infer_expr(condition, env);

                    // Apply type narrowing based on condition (Issue #2303)
                    // The loop body executes when condition is true, so use then_env
                    let split =
                        crate::compile::abstract_interp::conditional::split_env_by_condition(
                            env, condition,
                        );

                    // Reuse body_env allocation across iterations (Issue #3360)
                    body_env.clone_from(&split.then_env);

                    // Infer the body with narrowed environment - only propagate explicit
                    // return statements. The loop body's implicit value (last expression)
                    // does NOT contribute to the enclosing function's return type (Issue #2241)
                    let body_return = self.infer_block_explicit_return_only(body, &mut body_env);

                    // Merge updated types from loop body back into main env
                    changed = env.merge_changed(&body_env);

                    // If body has an explicit return, propagate it
                    if !matches!(body_return, LatticeType::Concrete(ConcreteType::Nothing)) {
                        env.merge(&pre_loop_env);
                        return StmtResult::Return(body_return);
                    }
                }

                // After loop: merge pre-loop types directly into post-loop env (Issue #3360)
                env.merge(&pre_loop_env);

                StmtResult::Continue
            }

            Stmt::Expr { expr, .. } => {
                let _ = self.infer_expr(expr, env);
                StmtResult::Continue
            }

            Stmt::Break { .. } | Stmt::Continue { .. } => StmtResult::Continue,

            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                // Analyze try block
                let try_return = self.infer_block(try_block, env);

                // Analyze catch block if present
                let catch_return = if let Some(catch_blk) = catch_block {
                    self.infer_block(catch_blk, env)
                } else {
                    LatticeType::Concrete(ConcreteType::Nothing)
                };

                // Analyze else block if present (runs if no exception)
                let else_return = if let Some(else_blk) = else_block {
                    self.infer_block(else_blk, env)
                } else {
                    LatticeType::Concrete(ConcreteType::Nothing)
                };

                // Analyze finally block if present (always runs, but doesn't affect return type)
                if let Some(finally_blk) = finally_block {
                    let _ = self.infer_block(finally_blk, env);
                }

                // Join return types from try and catch blocks
                // The try block's return type is the join of:
                // 1. Returns from the try block (if no exception)
                // 2. Returns from the catch block (if exception caught)
                // 3. Returns from the else block (if present and no exception)
                let combined = try_return.join(&catch_return).join(&else_return);

                if combined != LatticeType::Concrete(ConcreteType::Nothing) {
                    StmtResult::Return(combined)
                } else {
                    StmtResult::Continue
                }
            }

            _ => StmtResult::Continue,
        }
    }

    /// Infers the type of an expression.
    fn infer_expr(&mut self, expr: &Expr, env: &TypeEnv) -> LatticeType {
        match expr {
            Expr::Literal(lit, _) => self.infer_literal(lit),

            Expr::Var(name, _) => env.get(name).cloned().unwrap_or(LatticeType::Top),

            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let left_ty = self.infer_expr(left, env);
                let right_ty = self.infer_expr(right, env);
                let op_name = binary_op_to_function(op);
                // Try constant folding first: if both operands are constants,
                // evaluate the operation at compile time
                if let Some(const_result) = try_eval_binary(&op_name, &left_ty, &right_ty) {
                    return const_result;
                }
                // Fall back to transfer function
                self.tfuncs
                    .infer_return_type(&op_name, &[left_ty, right_ty])
            }

            Expr::UnaryOp { op, operand, .. } => {
                let operand_ty = self.infer_expr(operand, env);
                let op_name = unary_op_to_function(op);
                // Try constant folding first: if operand is a constant,
                // evaluate the operation at compile time
                if let Some(const_result) = try_eval_unary(&op_name, &operand_ty) {
                    return const_result;
                }
                // Fall back to transfer function
                self.tfuncs.infer_return_type(&op_name, &[operand_ty])
            }

            Expr::Call { function, args, .. } => {
                let arg_types: Vec<_> = args.iter().map(|arg| self.infer_expr(arg, env)).collect();

                // Special handling for getfield with struct table lookup
                if function == "getfield" && args.len() >= 2 {
                    if let LatticeType::Concrete(ConcreteType::Struct { name, .. }) = &arg_types[0]
                    {
                        // Try to extract field name from the second argument (literal Symbol)
                        let field_name = match &args[1] {
                            Expr::Literal(Literal::Symbol(s), _) => Some(s.clone()),
                            Expr::Literal(Literal::Str(s), _) => Some(s.clone()),
                            _ => None,
                        };

                        if let Some(field) = field_name {
                            if let Some(struct_info) = self.struct_table.get(name) {
                                if let Some(field_ty) = struct_info.get_field_type(&field) {
                                    return field_ty.clone();
                                }
                            }
                        }
                    }
                }

                // Special handling for map: infer return type based on function argument
                if function == "map" && args.len() == 2 {
                    if let Some(return_type) =
                        self.infer_map_return_type(&args[0], &arg_types[1], env)
                    {
                        return return_type;
                    }
                }

                // Create cache key with function name AND argument types
                // This enables polymorphic functions to be analyzed with different arg types
                let cache_key = CallCacheKey::new(function, &arg_types);

                // Check if we have a cached return type for this (function, arg_types) combination
                if let Some(cached) = self.return_type_cache.get(&cache_key) {
                    return cached.clone();
                }

                // Try interprocedural analysis if the function is in our function table
                // Limit recursion depth to prevent stack overflow
                if let Some(func) = self.function_table.get(function).cloned() {
                    // Check for recursive call cycle (using cache_key for cycle detection)
                    // Also check depth limit
                    if !self.analyzing_functions.contains(&cache_key)
                        && self.analysis_depth < MAX_INTERPROCEDURAL_ANALYSIS_DEPTH
                    {
                        self.analyzing_functions.insert(cache_key.clone());
                        self.analysis_depth += 1;

                        // Create a fresh environment with argument types bound to parameters
                        let mut call_env = TypeEnv::new();
                        for (param, arg_ty) in func.params.iter().zip(arg_types.iter()) {
                            call_env.set(&param.name, arg_ty.clone());
                        }

                        // Recursively infer the function's return type
                        let return_type = self.infer_block_with_fixpoint(&func.body, &mut call_env);

                        self.analysis_depth -= 1;
                        self.analyzing_functions.remove(&cache_key);
                        self.return_type_cache
                            .insert(cache_key, return_type.clone());
                        return return_type;
                    }
                    // Recursive call or depth limit reached - return Top
                    if self.analyzing_functions.contains(&cache_key) {
                        emit_recursive_cycle(vec![function.to_string()]);
                    }
                    return LatticeType::Top;
                }

                // Use transfer function for built-in functions
                // Create context with struct table for contextual transfer functions
                let ctx = TFuncContext::with_struct_table(&self.struct_table);
                self.tfuncs
                    .infer_return_type_with_context(function, &arg_types, &ctx)
            }

            Expr::Builtin { name, args, .. } => {
                let arg_types: Vec<_> = args.iter().map(|arg| self.infer_expr(arg, env)).collect();
                let builtin_name = builtin_op_to_function(name);
                let ctx = TFuncContext::with_struct_table(&self.struct_table);
                self.tfuncs
                    .infer_return_type_with_context(&builtin_name, &arg_types, &ctx)
            }

            Expr::ArrayLiteral { elements, .. } => {
                if elements.is_empty() {
                    // Empty array [] in Julia defaults to Vector{Any}
                    LatticeType::Concrete(ConcreteType::Array {
                        element: Box::new(ConcreteType::Any),
                    })
                } else {
                    // Infer element type as join of all elements
                    let mut element_type = self.infer_expr(&elements[0], env);
                    for elem in &elements[1..] {
                        let elem_ty = self.infer_expr(elem, env);
                        element_type = element_type.join(&elem_ty);
                    }

                    // Return Array{element_type}
                    match element_type {
                        LatticeType::Concrete(ct) => LatticeType::Concrete(ConcreteType::Array {
                            element: Box::new(ct),
                        }),
                        LatticeType::Const(cv) => LatticeType::Concrete(ConcreteType::Array {
                            element: Box::new(cv.to_concrete_type()),
                        }),
                        _ => {
                            emit_unknown_array_element();
                            LatticeType::Top
                        }
                    }
                }
            }

            Expr::Index { array, indices, .. } => {
                let array_ty = self.infer_expr(array, env);

                // For single-index access on Tuple, use constant index to get precise element type
                if indices.len() == 1 {
                    let index_ty = self.infer_expr(&indices[0], env);

                    // Check if we have a Tuple with a constant integer index
                    if let LatticeType::Concrete(ConcreteType::Tuple { elements }) = &array_ty {
                        if let LatticeType::Const(ConstValue::Int64(idx)) = &index_ty {
                            // Julia uses 1-based indexing
                            let idx_0based = (*idx - 1) as usize;
                            if idx_0based < elements.len() {
                                return LatticeType::Concrete(elements[idx_0based].clone());
                            }
                        }
                    }

                    // Use getindex transfer function with actual index type
                    self.tfuncs
                        .infer_return_type("getindex", &[array_ty, index_ty])
                } else {
                    // Multi-dimensional indexing: use getindex transfer function
                    self.tfuncs.infer_return_type(
                        "getindex",
                        &[array_ty, LatticeType::Concrete(ConcreteType::Int64)],
                    )
                }
            }

            Expr::TupleLiteral { elements, .. } => {
                let element_types: Vec<_> = elements
                    .iter()
                    .filter_map(|e| match self.infer_expr(e, env) {
                        LatticeType::Concrete(ct) => Some(ct),
                        LatticeType::Const(cv) => Some(cv.to_concrete_type()),
                        _ => None,
                    })
                    .collect();

                if element_types.len() == elements.len() {
                    LatticeType::Concrete(ConcreteType::Tuple {
                        elements: element_types,
                    })
                } else {
                    LatticeType::Top
                }
            }

            Expr::FieldAccess { object, field, .. } => {
                // Infer the type of the object
                let object_ty = self.infer_expr(object, env);

                // If the object is a struct, look up the field type
                if let LatticeType::Concrete(ConcreteType::Struct { name, .. }) = &object_ty {
                    if let Some(struct_info) = self.struct_table.get(name) {
                        if let Some(field_ty) = struct_info.get_field_type(field) {
                            return field_ty.clone();
                        }
                        // Known struct but unknown field
                        emit_unknown_field(name, field);
                    } else {
                        // Emit diagnostic for unknown struct
                        DiagnosticsCollector::emit(
                            TypeInferenceDiagnostic::new(DiagnosticReason::UnknownStruct(Some(
                                name.clone(),
                            )))
                            .with_context(format!("field access on {}.{}", name, field)),
                        );
                    }
                }

                // Unknown struct or field: fall back to Top
                LatticeType::Top
            }

            Expr::Range {
                start, step, stop, ..
            } => {
                // Infer the element type from start, step, stop
                let start_ty = self.infer_expr(start, env);
                let stop_ty = self.infer_expr(stop, env);

                // Join start and stop types
                let mut element_ty = start_ty.join(&stop_ty);

                // If step is present, join with step type
                if let Some(step_expr) = step {
                    let step_ty = self.infer_expr(step_expr, env);
                    element_ty = element_ty.join(&step_ty);
                }

                // Return Range{element_type}
                match element_ty {
                    LatticeType::Concrete(ct) => LatticeType::Concrete(ConcreteType::Range {
                        element: Box::new(ct),
                    }),
                    // If element type is not concrete (e.g., Union), default to Int64
                    _ => LatticeType::Concrete(ConcreteType::Range {
                        element: Box::new(ConcreteType::Int64),
                    }),
                }
            }

            _ => LatticeType::Top,
        }
    }

    /// Infers the type of a literal.
    ///
    /// Returns `LatticeType::Const` for basic literals (Int, Float, Bool, String, Nothing)
    /// to enable constant propagation and folding during type inference.
    /// Falls back to `LatticeType::Concrete` for types not supported by ConstValue.
    fn infer_literal(&self, lit: &Literal) -> LatticeType {
        match lit {
            // Return Const for basic types to enable constant propagation
            Literal::Int(v) => LatticeType::Const(ConstValue::Int64(*v)),
            Literal::Float(v) => LatticeType::Const(ConstValue::Float64(*v)),
            Literal::Bool(v) => LatticeType::Const(ConstValue::Bool(*v)),
            Literal::Str(v) => LatticeType::Const(ConstValue::String(v.clone())),
            Literal::Nothing => LatticeType::Const(ConstValue::Nothing),
            // Fall back to Concrete for types not supported by ConstValue
            // Note: Int128/BigInt -> Int64 to maintain arithmetic dispatch compatibility
            Literal::Int128(_) | Literal::BigInt(_) => LatticeType::Concrete(ConcreteType::Int64),
            // Float32/Float16 preserve their types
            Literal::Float32(_) => LatticeType::Concrete(ConcreteType::Float32),
            Literal::Float16(_) => LatticeType::Concrete(ConcreteType::Float16),
            // BigFloat -> Float64 for arithmetic dispatch compatibility
            Literal::BigFloat(_) => LatticeType::Concrete(ConcreteType::Float64),
            Literal::Char(_) => LatticeType::Concrete(ConcreteType::Char),
            Literal::Symbol(s) => LatticeType::Const(ConstValue::Symbol(s.clone())),
            Literal::Missing => LatticeType::Concrete(ConcreteType::Missing),
            _ => LatticeType::Top,
        }
    }

    /// Converts a Julia type annotation to a LatticeType.
    /// Preserves numeric bit widths where arithmetic support exists.
    fn julia_type_to_lattice(&self, ty: &JuliaType) -> LatticeType {
        match ty {
            // Signed integers - preserve bit width (all have proper arithmetic support)
            JuliaType::Int8 => LatticeType::Concrete(ConcreteType::Int8),
            JuliaType::Int16 => LatticeType::Concrete(ConcreteType::Int16),
            JuliaType::Int32 => LatticeType::Concrete(ConcreteType::Int32),
            JuliaType::Int64 => LatticeType::Concrete(ConcreteType::Int64),
            JuliaType::Int128 => LatticeType::Concrete(ConcreteType::Int128),
            // Unsigned integers - preserve bit width
            JuliaType::UInt8 => LatticeType::Concrete(ConcreteType::UInt8),
            JuliaType::UInt16 => LatticeType::Concrete(ConcreteType::UInt16),
            JuliaType::UInt32 => LatticeType::Concrete(ConcreteType::UInt32),
            JuliaType::UInt64 => LatticeType::Concrete(ConcreteType::UInt64),
            JuliaType::UInt128 => LatticeType::Concrete(ConcreteType::UInt128),
            // Floating point - preserve precision
            JuliaType::Float16 => LatticeType::Concrete(ConcreteType::Float16),
            JuliaType::Float32 => LatticeType::Concrete(ConcreteType::Float32),
            JuliaType::Float64 => LatticeType::Concrete(ConcreteType::Float64),
            // Other types
            JuliaType::Bool => LatticeType::Concrete(ConcreteType::Bool),
            JuliaType::String => LatticeType::Concrete(ConcreteType::String),
            JuliaType::Char => LatticeType::Concrete(ConcreteType::Char),
            // BigInt, BigFloat, and other types fall through to Top (Any)
            // to maintain dynamic dispatch compatibility
            _ => LatticeType::Top,
        }
    }

    /// Gets the cached return type for a function with specific argument types, if available.
    ///
    /// This method allows querying the cache with specific argument types for polymorphic
    /// function support.
    pub fn get_cached_return_type(
        &self,
        function_name: &str,
        arg_types: &[LatticeType],
    ) -> Option<&LatticeType> {
        let cache_key = CallCacheKey::new(function_name, arg_types);
        self.return_type_cache.get(&cache_key)
    }

    /// Gets the cached return type for a function by name only (legacy compatibility).
    ///
    /// This method looks for any cached entry with the given function name.
    /// For precise lookups with argument types, use `get_cached_return_type` instead.
    pub fn get_cached_return_type_by_name(&self, function_name: &str) -> Option<&LatticeType> {
        // Find any entry matching the function name
        for (key, value) in &self.return_type_cache {
            if key.name == function_name {
                return Some(value);
            }
        }
        None
    }

    /// Infer the return type of a `map(f, arr)` call by analyzing the function argument.
    ///
    /// This enables type inference like:
    /// - `map(x -> x + 1, [1, 2, 3])` returns `Array{Int64}`
    /// - `map(x -> Float64(x), [1, 2, 3])` returns `Array{Float64}`
    /// - `map(abs, [-1, -2])` returns `Array{Int64}`
    fn infer_map_return_type(
        &mut self,
        func_arg: &Expr,
        array_type: &LatticeType,
        _env: &TypeEnv,
    ) -> Option<LatticeType> {
        // Extract the element type from the array argument
        let element_type = match array_type {
            LatticeType::Concrete(ConcreteType::Array { element }) => element.as_ref().clone(),
            _ => return None, // Can't infer if array type is unknown
        };

        // Extract the function name from the function argument
        let func_name = match func_arg {
            Expr::FunctionRef { name, .. } => name.clone(),
            Expr::Var(name, _) => name.clone(),
            _ => return None, // Can't handle other expression types
        };

        // Look up the function in the function table
        // Limit recursion depth to prevent stack overflow
        if let Some(func) = self.function_table.get(&func_name).cloned() {
            // Create cache key for cycle detection
            let element_lattice = LatticeType::Concrete(element_type);
            let cache_key = CallCacheKey::new(&func_name, std::slice::from_ref(&element_lattice));

            // Check for recursive call cycle and depth limit
            if self.analyzing_functions.contains(&cache_key)
                || self.analysis_depth >= MAX_INTERPROCEDURAL_ANALYSIS_DEPTH
            {
                return None; // Recursive or too deep - fall back to transfer function
            }

            // Check cache first
            if let Some(cached) = self.return_type_cache.get(&cache_key) {
                return Some(LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(match cached {
                        LatticeType::Concrete(ct) => ct.clone(),
                        _ => return None,
                    }),
                }));
            }

            // Mark as being analyzed
            self.analyzing_functions.insert(cache_key.clone());
            self.analysis_depth += 1;

            // Create environment with parameter bound to element type
            let mut call_env = TypeEnv::new();
            if let Some(param) = func.params.first() {
                call_env.set(&param.name, element_lattice);
            }

            // Infer the function's return type
            let return_type = self.infer_block_with_fixpoint(&func.body, &mut call_env);

            // Remove from analyzing set
            self.analysis_depth -= 1;
            self.analyzing_functions.remove(&cache_key);

            // Cache the result
            self.return_type_cache
                .insert(cache_key, return_type.clone());

            // Return Array{ReturnType}
            return match return_type {
                LatticeType::Concrete(ct) => Some(LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(ct),
                })),
                _ => None, // Fall back to transfer function if return type is not concrete
            };
        }

        None // Function not found, fall back to transfer function
    }
}

impl Default for InferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Converts a binary operator to its function name.
fn binary_op_to_function(op: &BinaryOp) -> String {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Eq => "==",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        _ => "unknown_binop",
    }
    .to_string()
}

/// Converts a unary operator to its function name.
fn unary_op_to_function(op: &crate::ir::core::UnaryOp) -> String {
    match op {
        crate::ir::core::UnaryOp::Neg => "-",
        crate::ir::core::UnaryOp::Not => "!",
        _ => "unknown_unop",
    }
    .to_string()
}

/// Converts a builtin operation to its function name.
fn builtin_op_to_function(op: &BuiltinOp) -> String {
    match op {
        BuiltinOp::Isa => "isa",
        BuiltinOp::TypeOf => "typeof",
        // Note: Adjoint and Transpose are now Pure Julia
        _ => "unknown_builtin",
    }
    .to_string()
}

#[cfg(test)]
mod tests;
