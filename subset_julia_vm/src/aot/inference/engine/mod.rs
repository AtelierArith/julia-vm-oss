//! Type inference engine for AoT compilation.
//!
//! The `TypeInferenceEngine` performs static type analysis for Julia programs,
//! inferring types for expressions, functions, and whole programs.

mod type_ops;

use super::super::ir::AotBinOp;
use super::super::types::StaticType;
use super::super::AotResult;
use super::types::{FunctionSignature, StructTypeInfo, TypeEnv, TypedFunction, TypedProgram};
use crate::ir::core::{
    BinaryOp, Block, Expr, Function, Literal, Program, Stmt, StructDef, UnaryOp,
};
use std::collections::{HashMap, HashSet};

pub struct TypeInferenceEngine {
    /// Built-in function signatures (name -> return type for common arities)
    pub(crate) builtins: HashMap<String, Vec<(Vec<StaticType>, StaticType)>>,
    /// Current type environment
    pub env: TypeEnv,
    /// Struct definitions (public for setting from IrConverter)
    pub structs: HashMap<String, StructTypeInfo>,
    /// Collected call sites for function specialization
    /// Maps function name to list of observed call sites
    pub(crate) call_sites: HashMap<String, Vec<Vec<StaticType>>>,
}

impl TypeInferenceEngine {
    /// Create a new type inference engine
    pub fn new() -> Self {
        let mut engine = Self {
            builtins: HashMap::new(),
            env: HashMap::new(),
            structs: HashMap::new(),
            call_sites: HashMap::new(),
        };
        engine.register_builtins();
        engine
    }

    /// Register built-in function return types
    pub fn register_builtins(&mut self) {
        // Arithmetic operations - return type depends on arguments
        // For simplicity, register common patterns

        // Math functions
        self.register_builtin("abs", vec![StaticType::I64], StaticType::I64);
        self.register_builtin("abs", vec![StaticType::F64], StaticType::F64);

        self.register_builtin("sqrt", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("sqrt", vec![StaticType::I64], StaticType::F64);

        self.register_builtin("sin", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("cos", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("tan", vec![StaticType::F64], StaticType::F64);

        self.register_builtin("exp", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("log", vec![StaticType::F64], StaticType::F64);

        self.register_builtin("floor", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("ceil", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("round", vec![StaticType::F64], StaticType::F64);

        self.register_builtin(
            "min",
            vec![StaticType::I64, StaticType::I64],
            StaticType::I64,
        );
        self.register_builtin(
            "min",
            vec![StaticType::F64, StaticType::F64],
            StaticType::F64,
        );
        self.register_builtin(
            "max",
            vec![StaticType::I64, StaticType::I64],
            StaticType::I64,
        );
        self.register_builtin(
            "max",
            vec![StaticType::F64, StaticType::F64],
            StaticType::F64,
        );

        // Type conversion
        self.register_builtin("Int64", vec![StaticType::Any], StaticType::I64);
        self.register_builtin("Int32", vec![StaticType::Any], StaticType::I32);
        self.register_builtin("Float64", vec![StaticType::Any], StaticType::F64);
        self.register_builtin("Float32", vec![StaticType::Any], StaticType::F32);
        self.register_builtin("Bool", vec![StaticType::Any], StaticType::Bool);
        self.register_builtin("String", vec![StaticType::Any], StaticType::Str);

        // String functions
        self.register_builtin("length", vec![StaticType::Str], StaticType::I64);
        self.register_builtin("string", vec![StaticType::Any], StaticType::Str);

        // Array functions
        let arr_any = StaticType::Array {
            element: Box::new(StaticType::Any),
            ndims: None,
        };
        self.register_builtin("length", vec![arr_any.clone()], StaticType::I64);
        self.register_builtin(
            "size",
            vec![arr_any.clone()],
            StaticType::Tuple(vec![StaticType::I64]),
        );
        self.register_builtin(
            "push!",
            vec![arr_any.clone(), StaticType::Any],
            arr_any.clone(),
        );
        self.register_builtin("pop!", vec![arr_any.clone()], StaticType::Any);

        // Minimal prelude helpers used by AoT examples/tests.
        self.register_builtin(
            "range",
            vec![StaticType::F64, StaticType::F64, StaticType::I64],
            StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(1),
            },
        );
        self.register_builtin(
            "adjoint",
            vec![StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(1),
            }],
            StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(2),
            },
        );
        self.register_builtin(
            "abs2",
            vec![StaticType::Struct {
                type_id: 0,
                name: "Complex".to_string(),
            }],
            StaticType::F64,
        );

        // Comparison (return Bool)
        self.register_builtin(
            "==",
            vec![StaticType::Any, StaticType::Any],
            StaticType::Bool,
        );
        self.register_builtin(
            "!=",
            vec![StaticType::Any, StaticType::Any],
            StaticType::Bool,
        );
        self.register_builtin(
            "<",
            vec![StaticType::Any, StaticType::Any],
            StaticType::Bool,
        );
        self.register_builtin(
            "<=",
            vec![StaticType::Any, StaticType::Any],
            StaticType::Bool,
        );
        self.register_builtin(
            ">",
            vec![StaticType::Any, StaticType::Any],
            StaticType::Bool,
        );
        self.register_builtin(
            ">=",
            vec![StaticType::Any, StaticType::Any],
            StaticType::Bool,
        );

        // IO
        self.register_builtin("println", vec![StaticType::Any], StaticType::Nothing);
        self.register_builtin("print", vec![StaticType::Any], StaticType::Nothing);
    }

    /// Register a built-in function signature
    fn register_builtin(&mut self, name: &str, params: Vec<StaticType>, ret: StaticType) {
        self.builtins
            .entry(name.to_string())
            .or_default()
            .push((params, ret));
    }

    /// Analyze a complete program
    pub fn analyze_program(&mut self, program: &Program) -> AotResult<TypedProgram> {
        let mut typed = TypedProgram::new();

        // First pass: collect struct definitions
        for struct_def in &program.structs {
            let info = self.analyze_struct(struct_def)?;
            typed.add_struct(info);
        }

        // Store struct info in engine for function analysis
        self.structs = typed.structs.clone();

        // Collect user-defined function names for call-site specialization
        let user_functions: HashSet<String> =
            program.functions.iter().map(|f| f.name.clone()).collect();

        // Build a map from function name to function for quick lookup
        let _func_map: HashMap<String, &Function> = program
            .functions
            .iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        // Iterative type inference:
        // 1. First, collect call sites from main block (where we have concrete types)
        // 2. Infer function signatures based on collected call sites
        // 3. Re-collect call sites from function bodies with inferred types
        // 4. Repeat until no new call sites are discovered

        const MAX_ITERATIONS: usize = 10;
        for _iteration in 0..MAX_ITERATIONS {
            let old_call_sites = self.call_sites.clone();

            // Collect from main block (always has concrete types from literals)
            self.collect_call_sites_from_block(&program.main, &user_functions);

            // Collect from function bodies with current inferred parameter types
            for func in &program.functions {
                // Set up environment with current inferred parameter types
                self.env.clear();
                let sig = self.infer_function_signature(func);
                for (name, ty) in sig.param_names.iter().zip(sig.param_types.iter()) {
                    self.env.insert(name.clone(), ty.clone());
                }
                // Also add local variables from for-loops and assignments
                self.setup_local_env_from_block(&func.body);

                self.collect_call_sites_from_block(&func.body, &user_functions);
            }

            // Check if call sites have stabilized
            if self.call_sites == old_call_sites {
                break;
            }
        }

        // Clear env for function analysis
        self.env.clear();

        // Final pass: analyze functions with stabilized call-site information
        for func in &program.functions {
            let typed_func = self.analyze_function(func)?;
            // Make already-inferred signatures available while analyzing subsequent
            // functions in this pass (e.g., g() return type while inferring main()).
            self.register_builtin(
                &typed_func.signature.name,
                typed_func.signature.param_types.clone(),
                typed_func.signature.return_type.clone(),
            );
            typed.add_function(typed_func);
        }

        // Collect globals from main block
        let globals = self.collect_globals(&program.main)?;
        typed.globals = globals;

        Ok(typed)
    }

    /// Set up local environment from a block (for-loop variables, assignments, etc.)
    fn setup_local_env_from_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.setup_local_env_from_stmt(stmt);
        }
    }

    /// Set up local environment from a statement
    fn setup_local_env_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                // Assignment expressions (`x = (y = value)`) assign both y and x.
                if let Expr::AssignExpr {
                    var: inner_var,
                    value: inner_value,
                    ..
                } = value
                {
                    let inner_ty = self.infer_expr_type(inner_value);
                    self.env.insert(inner_var.clone(), inner_ty.clone());
                    self.env.insert(var.clone(), inner_ty);
                } else {
                    // Infer type of the assigned value
                    let ty = self.infer_expr_type(value);
                    self.env.insert(var.clone(), ty);
                }
            }
            Stmt::For {
                var,
                start,
                end,
                body,
                ..
            } => {
                // For loop variable has the type of the range elements
                let start_ty = self.infer_expr_type(start);
                let end_ty = self.infer_expr_type(end);
                // For integer ranges, the loop variable is the promoted integer type
                let elem_ty = if start_ty.is_integer() && end_ty.is_integer() {
                    self.numeric_promote(&start_ty, &end_ty)
                } else {
                    StaticType::I64 // Default for 1:N style ranges
                };
                self.env.insert(var.clone(), elem_ty);
                self.setup_local_env_from_block(body);
            }
            Stmt::ForEach {
                var,
                iterable,
                body,
                ..
            } => {
                // Infer element type from iterable
                let iter_ty = self.infer_expr_type(iterable);
                let elem_ty = self.element_type(&iter_ty);
                self.env.insert(var.clone(), elem_ty);
                self.setup_local_env_from_block(body);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                self.setup_local_env_from_block(then_branch);
                if let Some(else_block) = else_branch {
                    self.setup_local_env_from_block(else_block);
                }
            }
            Stmt::While { body, .. } => {
                self.setup_local_env_from_block(body);
            }
            Stmt::Block(inner) => {
                self.setup_local_env_from_block(inner);
            }
            Stmt::Expr { expr, .. } => {
                if let Expr::LetBlock { bindings, body, .. } = expr {
                    for (name, value) in bindings {
                        let ty = self.infer_expr_type(value);
                        self.env.insert(name.clone(), ty);
                    }
                    self.setup_local_env_from_block(body);
                }
            }
            _ => {}
        }
    }

    /// Collect call sites from a block for function specialization
    fn collect_call_sites_from_block(&mut self, block: &Block, user_functions: &HashSet<String>) {
        for stmt in &block.stmts {
            self.collect_call_sites_from_stmt(stmt, user_functions);
        }
    }

    /// Collect call sites from a statement
    fn collect_call_sites_from_stmt(&mut self, stmt: &Stmt, user_functions: &HashSet<String>) {
        match stmt {
            Stmt::Assign { value, .. } => {
                self.collect_call_sites_from_expr(value, user_functions);
            }
            Stmt::Expr { expr, .. } => {
                self.collect_call_sites_from_expr(expr, user_functions);
            }
            Stmt::Return {
                value: Some(expr), ..
            } => {
                self.collect_call_sites_from_expr(expr, user_functions);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_call_sites_from_expr(condition, user_functions);
                self.collect_call_sites_from_block(then_branch, user_functions);
                if let Some(else_block) = else_branch {
                    self.collect_call_sites_from_block(else_block, user_functions);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.collect_call_sites_from_expr(condition, user_functions);
                self.collect_call_sites_from_block(body, user_functions);
            }
            Stmt::For {
                start,
                end,
                step,
                body,
                ..
            } => {
                self.collect_call_sites_from_expr(start, user_functions);
                self.collect_call_sites_from_expr(end, user_functions);
                if let Some(s) = step {
                    self.collect_call_sites_from_expr(s, user_functions);
                }
                self.collect_call_sites_from_block(body, user_functions);
            }
            Stmt::ForEach { iterable, body, .. } => {
                self.collect_call_sites_from_expr(iterable, user_functions);
                self.collect_call_sites_from_block(body, user_functions);
            }
            Stmt::Block(inner) => {
                self.collect_call_sites_from_block(inner, user_functions);
            }
            _ => {}
        }
    }

    /// Collect call sites from an expression
    fn collect_call_sites_from_expr(&mut self, expr: &Expr, user_functions: &HashSet<String>) {
        match expr {
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                // Recursively collect from arguments first
                for arg in args {
                    self.collect_call_sites_from_expr(arg, user_functions);
                }
                for (_, arg) in kwargs {
                    self.collect_call_sites_from_expr(arg, user_functions);
                }

                // Broadcasted(function_ref, (args...)) carries call-sites in function-ref form.
                if function == "Broadcasted" && args.len() == 2 {
                    if let Expr::FunctionRef { name, .. } = &args[0] {
                        if user_functions.contains(name) {
                            let bc_args: Vec<&Expr> = match &args[1] {
                                Expr::TupleLiteral { elements, .. } => elements.iter().collect(),
                                other => vec![other],
                            };

                            let arg_types: Vec<StaticType> = bc_args
                                .iter()
                                .map(|arg| {
                                    // Ref(x) is scalar-protection in broadcast; treat as x.
                                    let ty = if let Expr::Builtin {
                                        name: crate::ir::core::BuiltinOp::Ref,
                                        args,
                                        ..
                                    } = arg
                                    {
                                        if args.len() == 1 {
                                            self.infer_expr_type(&args[0])
                                        } else {
                                            self.infer_expr_type(arg)
                                        }
                                    } else {
                                        self.infer_expr_type(arg)
                                    };

                                    // Broadcasted functions are applied element-wise.
                                    match ty {
                                        StaticType::Array { .. } | StaticType::Range { .. } => {
                                            self.element_type(&ty)
                                        }
                                        _ => ty,
                                    }
                                })
                                .collect();

                            let has_concrete =
                                arg_types.iter().any(|t| !matches!(t, StaticType::Any));
                            if has_concrete {
                                self.call_sites
                                    .entry(name.clone())
                                    .or_default()
                                    .push(arg_types);
                            }
                        }
                    }
                }

                // If this is a call to a user-defined function, record the argument types
                if user_functions.contains(function) {
                    let arg_types: Vec<StaticType> = args
                        .iter()
                        .map(|a| self.infer_expr_type(a))
                        .chain(kwargs.iter().map(|(_, a)| self.infer_expr_type(a)))
                        .collect();

                    // Only record if we have concrete types (not all Any)
                    let has_concrete = arg_types.iter().any(|t| !matches!(t, StaticType::Any));
                    if has_concrete {
                        self.call_sites
                            .entry(function.clone())
                            .or_default()
                            .push(arg_types);
                    }
                }
            }
            Expr::BinaryOp { left, right, .. } => {
                self.collect_call_sites_from_expr(left, user_functions);
                self.collect_call_sites_from_expr(right, user_functions);
            }
            Expr::UnaryOp { operand, .. } => {
                self.collect_call_sites_from_expr(operand, user_functions);
            }
            Expr::Index { array, indices, .. } => {
                self.collect_call_sites_from_expr(array, user_functions);
                for idx in indices {
                    self.collect_call_sites_from_expr(idx, user_functions);
                }
            }
            Expr::ArrayLiteral { elements, .. } => {
                for elem in elements {
                    self.collect_call_sites_from_expr(elem, user_functions);
                }
            }
            Expr::TupleLiteral { elements, .. } => {
                for elem in elements {
                    self.collect_call_sites_from_expr(elem, user_functions);
                }
            }
            Expr::Range {
                start, stop, step, ..
            } => {
                self.collect_call_sites_from_expr(start, user_functions);
                self.collect_call_sites_from_expr(stop, user_functions);
                if let Some(s) = step {
                    self.collect_call_sites_from_expr(s, user_functions);
                }
            }
            Expr::Ternary {
                condition,
                then_expr,
                else_expr,
                ..
            } => {
                self.collect_call_sites_from_expr(condition, user_functions);
                self.collect_call_sites_from_expr(then_expr, user_functions);
                self.collect_call_sites_from_expr(else_expr, user_functions);
            }
            Expr::FieldAccess { object, .. } => {
                self.collect_call_sites_from_expr(object, user_functions);
            }
            Expr::Builtin { args, .. } => {
                for arg in args {
                    self.collect_call_sites_from_expr(arg, user_functions);
                }
            }
            Expr::AssignExpr { value, .. } => {
                self.collect_call_sites_from_expr(value, user_functions);
            }
            Expr::LetBlock { bindings, body, .. } => {
                for (_, value) in bindings {
                    self.collect_call_sites_from_expr(value, user_functions);
                }
                self.collect_call_sites_from_block(body, user_functions);
            }
            // TypedEmptyArray doesn't contain subexpressions
            Expr::TypedEmptyArray { .. } => {}
            _ => {}
        }
    }

    /// Analyze a struct definition
    pub fn analyze_struct(&self, struct_def: &StructDef) -> AotResult<StructTypeInfo> {
        let mut info = StructTypeInfo::new(struct_def.name.clone(), struct_def.is_mutable);

        info.parent = struct_def.parent_type.clone();
        info.type_params = struct_def
            .type_params
            .iter()
            .map(|tp| tp.name.clone())
            .collect();

        for field in &struct_def.fields {
            let field_type = if let Some(type_expr) = &field.type_expr {
                self.type_expr_to_static(type_expr)
            } else {
                StaticType::Any
            };
            info.add_field(field.name.clone(), field_type);
        }

        Ok(info)
    }

    /// Convert TypeExpr to StaticType
    fn type_expr_to_static(&self, type_expr: &crate::types::TypeExpr) -> StaticType {
        use crate::types::TypeExpr;

        match type_expr {
            TypeExpr::Concrete(jt) => StaticType::from(jt),
            TypeExpr::TypeVar(name) => StaticType::Struct {
                type_id: 0,
                name: name.clone(),
            },
            TypeExpr::Parameterized { base, params } => {
                // For now, treat parameterized types as structs
                let param_strs: Vec<_> = params.iter().map(|p| format!("{}", p)).collect();
                StaticType::Struct {
                    type_id: 0,
                    name: format!("{}{{{}}}", base, param_strs.join(", ")),
                }
            }
            TypeExpr::RuntimeExpr(expr_str) => StaticType::Struct {
                type_id: 0,
                name: expr_str.clone(),
            },
        }
    }

    /// Infer function signature with call-site specialization
    ///
    /// If a parameter has no type annotation but we have observed call sites
    /// with concrete types, we use the most general type that covers all call sites.
    pub fn infer_function_signature(&self, func: &Function) -> FunctionSignature {
        let param_names: Vec<_> = func.params.iter().map(|p| p.name.clone()).collect();

        // Start with declared types (or Any if not declared)
        let mut param_types: Vec<_> = func
            .params
            .iter()
            .map(|p| {
                if let Some(ref ann) = p.type_annotation {
                    StaticType::from(ann)
                } else {
                    StaticType::Any
                }
            })
            .collect();

        // Apply call-site specialization for untyped parameters
        if let Some(call_sites) = self.call_sites.get(&func.name) {
            for (i, param_ty) in param_types.iter_mut().enumerate() {
                if matches!(param_ty, StaticType::Any) {
                    // Collect all types used at this position across call sites
                    let mut observed_types: Vec<StaticType> = call_sites
                        .iter()
                        .filter_map(|args| args.get(i).cloned())
                        .filter(|t| !matches!(t, StaticType::Any))
                        .collect();

                    if !observed_types.is_empty() {
                        // Deduplicate types
                        observed_types.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
                        observed_types.dedup();

                        if observed_types.len() == 1 {
                            // Single concrete type observed - specialize to that type
                            *param_ty = observed_types.pop().unwrap();
                        } else {
                            // Multiple types observed - find common supertype
                            // For numeric types, use promotion; otherwise keep Any
                            let all_numeric = observed_types.iter().all(|t| t.is_numeric());
                            if all_numeric && observed_types.len() >= 2 {
                                // Promote all numeric types to the widest one
                                let promoted = observed_types
                                    .into_iter()
                                    .reduce(|a, b| self.numeric_promote(&a, &b))
                                    .unwrap_or(StaticType::Any);
                                *param_ty = promoted;
                            } else {
                                // Check if all observed types are arrays with compatible element types
                                let all_arrays = observed_types
                                    .iter()
                                    .all(|t| matches!(t, StaticType::Array { .. }));
                                if all_arrays && observed_types.len() >= 2 {
                                    // Find common array element type
                                    let elem_types: Vec<StaticType> = observed_types
                                        .iter()
                                        .filter_map(|t| {
                                            if let StaticType::Array { element, .. } = t {
                                                Some((**element).clone())
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();

                                    // If all element types are numeric, promote them
                                    let all_elem_numeric =
                                        elem_types.iter().all(|t| t.is_numeric());
                                    if all_elem_numeric && !elem_types.is_empty() {
                                        let promoted_elem = elem_types
                                            .into_iter()
                                            .reduce(|a, b| self.numeric_promote(&a, &b))
                                            .unwrap_or(StaticType::Any);
                                        // Use the first array's ndims
                                        let ndims =
                                            if let Some(StaticType::Array { ndims, .. }) =
                                                observed_types.first()
                                            {
                                                *ndims
                                            } else {
                                                Some(1)
                                            };
                                        *param_ty = StaticType::Array {
                                            element: Box::new(promoted_elem),
                                            ndims,
                                        };
                                    }
                                }
                            }
                            // If not all numeric or array, keep Any
                        }
                    }
                }
            }
        }

        // Infer return type with the specialized parameter types
        let return_type = if let Some(ref ret) = func.return_type {
            StaticType::from(ret)
        } else {
            // Try to infer return type from function body with specialized params
            self.infer_return_type(&func.body, &param_names, &param_types)
        };

        FunctionSignature::new(func.name.clone(), param_names, param_types, return_type)
    }

    /// Analyze a function
    pub fn analyze_function(&mut self, func: &Function) -> AotResult<TypedFunction> {
        let signature = self.infer_function_signature(func);
        let mut typed_func = TypedFunction::new(signature);

        // Set up environment with parameter types
        self.env.clear();
        for (name, ty) in typed_func
            .signature
            .param_names
            .iter()
            .zip(typed_func.signature.param_types.iter())
        {
            self.env.insert(name.clone(), ty.clone());
        }

        // Collect local variable types
        let locals = self.collect_local_types(&func.body)?;
        for (name, ty) in locals {
            typed_func.add_local(name, ty);
        }

        Ok(typed_func)
    }

    /// Collect local variable types from a block
    pub fn collect_local_types(&mut self, block: &Block) -> AotResult<TypeEnv> {
        let mut locals = TypeEnv::new();

        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { var, value, .. } => {
                    if let Expr::AssignExpr {
                        var: inner_var,
                        value: inner_value,
                        ..
                    } = value
                    {
                        let inner_ty = self.infer_expr_type(inner_value);
                        locals.insert(inner_var.clone(), inner_ty.clone());
                        self.env.insert(inner_var.clone(), inner_ty.clone());
                        locals.insert(var.clone(), inner_ty.clone());
                        self.env.insert(var.clone(), inner_ty);
                    } else {
                        let ty = self.infer_expr_type(value);
                        locals.insert(var.clone(), ty.clone());
                        self.env.insert(var.clone(), ty);
                    }
                }
                Stmt::For {
                    var,
                    start,
                    end,
                    body,
                    ..
                } => {
                    // Loop variable is integer
                    let start_ty = self.infer_expr_type(start);
                    let end_ty = self.infer_expr_type(end);
                    let var_ty = self.join_types(&start_ty, &end_ty);
                    locals.insert(var.clone(), var_ty.clone());
                    self.env.insert(var.clone(), var_ty);

                    // Recurse into body
                    let body_locals = self.collect_local_types(body)?;
                    locals.extend(body_locals);
                }
                Stmt::ForEach {
                    var,
                    iterable,
                    body,
                    ..
                } => {
                    let iter_ty = self.infer_expr_type(iterable);
                    let elem_ty = self.element_type(&iter_ty);
                    locals.insert(var.clone(), elem_ty.clone());
                    self.env.insert(var.clone(), elem_ty);

                    let body_locals = self.collect_local_types(body)?;
                    locals.extend(body_locals);
                }
                Stmt::While { body, .. } => {
                    let body_locals = self.collect_local_types(body)?;
                    locals.extend(body_locals);
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    let then_locals = self.collect_local_types(then_branch)?;
                    locals.extend(then_locals);

                    if let Some(else_block) = else_branch {
                        let else_locals = self.collect_local_types(else_block)?;
                        locals.extend(else_locals);
                    }
                }
                Stmt::Block(inner_block) => {
                    let inner_locals = self.collect_local_types(inner_block)?;
                    locals.extend(inner_locals);
                }
                Stmt::Expr { expr, .. } => {
                    if let Expr::LetBlock { bindings, body, .. } = expr {
                        for (name, value) in bindings {
                            let ty = self.infer_expr_type(value);
                            locals.insert(name.clone(), ty.clone());
                            self.env.insert(name.clone(), ty);
                        }
                        let body_locals = self.collect_local_types(body)?;
                        locals.extend(body_locals);
                    }
                }
                _ => {}
            }
        }

        Ok(locals)
    }

    /// Collect global variable types from a block
    pub fn collect_globals(&mut self, block: &Block) -> AotResult<TypeEnv> {
        // For now, globals are collected the same way as locals
        // In a real implementation, we'd distinguish based on scope
        self.collect_local_types(block)
    }

    /// Infer the return type of a function body
    pub(crate) fn infer_return_type(
        &self,
        block: &Block,
        param_names: &[String],
        param_types: &[StaticType],
    ) -> StaticType {
        // Create a temporary environment with parameters
        let mut env = TypeEnv::new();
        for (name, ty) in param_names.iter().zip(param_types.iter()) {
            env.insert(name.clone(), ty.clone());
        }

        // Collect local variable types from the block to properly infer return type
        self.collect_local_types_for_env(block, &mut env);

        // Find return statements and infer their types
        let mut return_types = Vec::new();
        self.collect_return_types(block, &env, &mut return_types);

        if return_types.is_empty() {
            // No explicit return, check last expression
            if let Some(Stmt::Expr { expr, .. }) = block.stmts.last() {
                self.infer_expr_type_with_env(expr, &env)
            } else {
                // Check if last statement is something else that could return a value
                // For example, an if-else where both branches have values
                match block.stmts.last() {
                    Some(Stmt::If {
                        then_branch,
                        else_branch: Some(else_block),
                        ..
                    }) => {
                        // Get return types from both branches
                        let then_type = self.infer_block_value_type(then_branch, &env);
                        let else_type = self.infer_block_value_type(else_block, &env);
                        self.join_types(&then_type, &else_type)
                    }
                    _ => StaticType::Nothing,
                }
            }
        } else if return_types.len() == 1 {
            return_types.pop().unwrap()
        } else {
            // Multiple return types - create union
            StaticType::Union {
                variants: return_types,
            }
        }
    }

    /// Infer the value type of a block (last expression)
    fn infer_block_value_type(&self, block: &Block, env: &TypeEnv) -> StaticType {
        if let Some(Stmt::Expr { expr, .. }) = block.stmts.last() {
            self.infer_expr_type_with_env(expr, env)
        } else {
            StaticType::Nothing
        }
    }

    /// Collect local variable types into an environment (for return type inference)
    fn collect_local_types_for_env(&self, block: &Block, env: &mut TypeEnv) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Assign { var, value, .. } => {
                    if let Expr::AssignExpr {
                        var: inner_var,
                        value: inner_value,
                        ..
                    } = value
                    {
                        let inner_ty = self.infer_expr_type_with_env(inner_value, env);
                        env.insert(inner_var.clone(), inner_ty.clone());
                        env.insert(var.clone(), inner_ty);
                    } else {
                        let ty = self.infer_expr_type_with_env(value, env);
                        env.insert(var.clone(), ty);
                    }
                }
                Stmt::For {
                    var,
                    start,
                    end,
                    body,
                    ..
                } => {
                    // Loop variable type from range
                    let start_ty = self.infer_expr_type_with_env(start, env);
                    let end_ty = self.infer_expr_type_with_env(end, env);
                    let var_ty = self.join_types(&start_ty, &end_ty);
                    env.insert(var.clone(), var_ty);
                    // Recurse into body
                    self.collect_local_types_for_env(body, env);
                }
                Stmt::ForEach {
                    var,
                    iterable,
                    body,
                    ..
                } => {
                    let iter_ty = self.infer_expr_type_with_env(iterable, env);
                    let elem_ty = self.element_type(&iter_ty);
                    env.insert(var.clone(), elem_ty);
                    self.collect_local_types_for_env(body, env);
                }
                Stmt::While { body, .. } => {
                    self.collect_local_types_for_env(body, env);
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    self.collect_local_types_for_env(then_branch, env);
                    if let Some(else_block) = else_branch {
                        self.collect_local_types_for_env(else_block, env);
                    }
                }
                Stmt::Block(inner) => {
                    self.collect_local_types_for_env(inner, env);
                }
                Stmt::Expr { expr, .. } => {
                    if let Expr::LetBlock { bindings, body, .. } = expr {
                        let mut local_env = env.clone();
                        for (name, value) in bindings {
                            let ty = self.infer_expr_type_with_env(value, &local_env);
                            local_env.insert(name.clone(), ty);
                        }
                        self.collect_local_types_for_env(body, &mut local_env);
                    }
                }
                _ => {}
            }
        }
    }

    /// Collect return types from a block
    fn collect_return_types(&self, block: &Block, env: &TypeEnv, types: &mut Vec<StaticType>) {
        for stmt in &block.stmts {
            match stmt {
                Stmt::Return {
                    value: Some(expr), ..
                } => {
                    let ty = self.infer_expr_type_with_env(expr, env);
                    if !types.contains(&ty) {
                        types.push(ty);
                    }
                }
                Stmt::Return { value: None, .. } => {
                    if !types.contains(&StaticType::Nothing) {
                        types.push(StaticType::Nothing);
                    }
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    self.collect_return_types(then_branch, env, types);
                    if let Some(else_block) = else_branch {
                        self.collect_return_types(else_block, env, types);
                    }
                }
                Stmt::For { body, .. } | Stmt::ForEach { body, .. } | Stmt::While { body, .. } => {
                    self.collect_return_types(body, env, types);
                }
                Stmt::Block(inner) => {
                    self.collect_return_types(inner, env, types);
                }
                _ => {}
            }
        }
    }

    /// Infer expression type using current environment
    pub fn infer_expr_type(&self, expr: &Expr) -> StaticType {
        self.infer_expr_type_with_env(expr, &self.env)
    }

    /// Infer expression type with explicit environment
    fn infer_expr_type_with_env(&self, expr: &Expr, env: &TypeEnv) -> StaticType {
        match expr {
            Expr::Literal(lit, _) => self.literal_type(lit),
            Expr::Var(name, _) => env
                .get(name)
                .cloned()
                .unwrap_or_else(|| self.lookup_global_or_const(name)),
            Expr::AssignExpr { value, .. } => self.infer_expr_type_with_env(value, env),
            Expr::LetBlock { bindings, body, .. } => {
                let mut local_env = env.clone();
                for (name, value) in bindings {
                    let ty = self.infer_expr_type_with_env(value, &local_env);
                    local_env.insert(name.clone(), ty);
                }
                self.collect_local_types_for_env(body, &mut local_env);
                self.infer_block_value_type(body, &local_env)
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let left_ty = self.infer_expr_type_with_env(left, env);
                let right_ty = self.infer_expr_type_with_env(right, env);
                self.binop_result_type(op, &left_ty, &right_ty)
            }
            Expr::UnaryOp { op, operand, .. } => {
                let operand_ty = self.infer_expr_type_with_env(operand, env);
                self.unaryop_result_type(op, &operand_ty)
            }
            Expr::Call {
                function,
                args,
                kwargs,
                ..
            } => {
                if function == "Broadcasted" {
                    return self.infer_broadcasted_result_type(args, env);
                }

                // Infer result type from lowered broadcast forms:
                // materialize(Broadcasted(fn, (args...)))
                if function == "materialize" && args.len() == 1 {
                    if let Expr::Call {
                        function: inner_fn,
                        args: inner_args,
                        ..
                    } = &args[0]
                    {
                        if inner_fn == "Broadcasted" {
                            return self.infer_broadcasted_result_type(inner_args, env);
                        }
                    }
                }

                // Special handling for convert(Type, value) - return type is based on the value
                // This is important because lowering wraps return values in convert(Any, value)
                let call_args: Vec<&Expr> =
                    args.iter().chain(kwargs.iter().map(|(_, v)| v)).collect();

                if function == "convert" && call_args.len() == 2 {
                    // For convert(Any, value), return the type of value, not Any
                    // This preserves the inferred type through the convert wrapper
                    if let Expr::Var(type_name, _) = call_args[0] {
                        if type_name == "Any" {
                            // convert(Any, value) - return the type of value
                            return self.infer_expr_type_with_env(call_args[1], env);
                        }
                    }
                    // For convert(T, value) where T is a concrete type, return T
                    let target_type = self.infer_expr_type_with_env(call_args[0], env);
                    if !matches!(target_type, StaticType::Any) {
                        return target_type;
                    }
                    // Otherwise, infer from the value
                    return self.infer_expr_type_with_env(call_args[1], env);
                }

                let arg_types: Vec<_> = call_args
                    .iter()
                    .map(|a| self.infer_expr_type_with_env(a, env))
                    .collect();
                // Check if it's a struct constructor
                if let Some(struct_info) = self.structs.get(function) {
                    return StaticType::Struct {
                        type_id: 0,
                        name: struct_info.name.clone(),
                    };
                }
                self.call_result_type(function, &arg_types)
            }
            Expr::Index { array, indices, .. } => {
                let arr_ty = self.infer_expr_type_with_env(array, env);
                // For tuple with constant index, get specific element type
                if matches!(arr_ty, StaticType::Tuple(_)) && indices.len() == 1 {
                    if let Expr::Literal(Literal::Int(idx), _) = &indices[0] {
                        return self.tuple_element_type_at(&arr_ty, *idx as usize);
                    }
                }
                self.element_type(&arr_ty)
            }
            Expr::ArrayLiteral {
                elements, shape, ..
            } => {
                // Use shape.len() for ndims to support multidimensional arrays
                let ndims = shape.len();
                if elements.is_empty() {
                    StaticType::Array {
                        element: Box::new(StaticType::Any),
                        ndims: Some(ndims),
                    }
                } else {
                    let elem_types: Vec<_> = elements
                        .iter()
                        .map(|e| self.infer_expr_type_with_env(e, env))
                        .collect();
                    let elem_type = elem_types
                        .into_iter()
                        .reduce(|a, b| self.join_types(&a, &b))
                        .unwrap_or(StaticType::Any);
                    StaticType::Array {
                        element: Box::new(elem_type),
                        ndims: Some(ndims),
                    }
                }
            }
            Expr::TupleLiteral { elements, .. } => {
                let elem_types: Vec<_> = elements
                    .iter()
                    .map(|e| self.infer_expr_type_with_env(e, env))
                    .collect();
                StaticType::Tuple(elem_types)
            }
            // Typed empty array literal: Int64[], Float64[], etc.
            Expr::TypedEmptyArray { element_type, .. } => {
                let elem_ty = self.type_name_to_static(element_type);
                StaticType::Array {
                    element: Box::new(elem_ty),
                    ndims: Some(1),
                }
            }
            Expr::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                let then_ty = self.infer_expr_type_with_env(then_expr, env);
                let else_ty = self.infer_expr_type_with_env(else_expr, env);
                self.join_types(&then_ty, &else_ty)
            }
            Expr::FieldAccess { object, field, .. } => {
                let obj_ty = self.infer_expr_type_with_env(object, env);
                self.field_type(&obj_ty, field)
            }
            Expr::Range { start, stop, .. } => {
                let start_ty = self.infer_expr_type_with_env(start, env);
                let stop_ty = self.infer_expr_type_with_env(stop, env);
                let elem_ty = self.join_types(&start_ty, &stop_ty);
                StaticType::Range {
                    element: Box::new(elem_ty),
                }
            }
            Expr::Builtin { name, args, .. } => {
                use crate::ir::core::BuiltinOp;
                let arg_types: Vec<_> = args
                    .iter()
                    .map(|a| self.infer_expr_type_with_env(a, env))
                    .collect();
                match name {
                    // Math functions that return F64
                    BuiltinOp::Sqrt | BuiltinOp::Rand | BuiltinOp::Randn => StaticType::F64,
                    // Time returns I64 (nanoseconds)
                    BuiltinOp::TimeNs => StaticType::I64,
                    // Array constructors return arrays
                    BuiltinOp::Zeros
                    | BuiltinOp::Ones => {
                    // Note: Fill, Trues, Falses are now Pure Julia — Issue #2640
                        // These create f64 arrays by default
                        let ndims = if args.is_empty() { 1 } else { args.len() };
                        StaticType::Array {
                            element: Box::new(StaticType::F64),
                            ndims: Some(ndims),
                        }
                    }
                    BuiltinOp::Reshape => {
                        // Returns array with same element type as input
                        if let Some(arr_ty) = arg_types.first() {
                            arr_ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // Length/Ndims return I64
                    BuiltinOp::Length | BuiltinOp::Ndims => StaticType::I64,
                    // Note: BuiltinOp::Sum removed — sum is now Pure Julia
                    // Size returns tuple or I64
                    BuiltinOp::Size => {
                        if args.len() == 2 {
                            StaticType::I64 // size(arr, dim)
                        } else {
                            StaticType::Tuple(vec![StaticType::I64]) // size(arr)
                        }
                    }
                    // Mutating operations return the array
                    BuiltinOp::Push | BuiltinOp::PushFirst | BuiltinOp::Insert => {
                        if let Some(arr_ty) = arg_types.first() {
                            arr_ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // Pop operations return element type
                    BuiltinOp::Pop | BuiltinOp::PopFirst | BuiltinOp::DeleteAt => {
                        if let Some(arr_ty) = arg_types.first() {
                            self.element_type(arr_ty)
                        } else {
                            StaticType::Any
                        }
                    }
                    // Zero returns same type as input
                    BuiltinOp::Zero => {
                        if let Some(ty) = arg_types.first() {
                            ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // Linear algebra
                    BuiltinOp::Det => StaticType::F64,
                    BuiltinOp::Lu => {
                        // Returns same array type
                        if let Some(arr_ty) = arg_types.first() {
                            arr_ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // RNG constructors - return opaque type
                    BuiltinOp::StableRNG | BuiltinOp::XoshiroRNG => StaticType::Any,
                    // Tuple operations
                    BuiltinOp::TupleFirst | BuiltinOp::TupleLast => {
                        if let Some(StaticType::Tuple(elems)) = arg_types.first() {
                            if !elems.is_empty() {
                                return elems[0].clone();
                            }
                        }
                        StaticType::Any
                    }
                    // Note: TupleLength removed — dead code (Issue #2643)
                    // Dict operations
                    BuiltinOp::HasKey => StaticType::Bool,
                    BuiltinOp::DictGet | BuiltinOp::DictGetBang => StaticType::Any, // Value type unknown
                    BuiltinOp::DictDelete | BuiltinOp::DictMerge | BuiltinOp::DictMergeBang => {
                        // Returns dict
                        if let Some(dict_ty) = arg_types.first() {
                            dict_ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    BuiltinOp::DictKeys | BuiltinOp::DictValues | BuiltinOp::DictPairs => {
                        // These return iterators/collections
                        StaticType::Any
                    }
                    // Ternary if-else
                    BuiltinOp::IfElse => {
                        if arg_types.len() >= 3 {
                            self.join_types(&arg_types[1], &arg_types[2])
                        } else {
                            StaticType::Any
                        }
                    }
                    // Type operations
                    BuiltinOp::TypeOf => StaticType::Str,
                    BuiltinOp::Isa => StaticType::Bool,
                    // Broadcasting control
                    BuiltinOp::Ref => {
                        if let Some(ty) = arg_types.first() {
                            ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // Dict operations fallback
                    BuiltinOp::DictEmpty | BuiltinOp::DictGetkey => {
                        if let Some(dict_ty) = arg_types.first() {
                            dict_ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // Size-related operations
                    BuiltinOp::Sizeof => StaticType::I64,
                    // Boolean predicates
                    BuiltinOp::Isbits
                    | BuiltinOp::Isbitstype
                    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
                    // removed - now Pure Julia (base/reflection.jl)
                    | BuiltinOp::Ismutable
                    | BuiltinOp::Hasfield => StaticType::Bool,
                    // Type operations returning types (represented as Any)
                    BuiltinOp::Eltype
                    | BuiltinOp::Keytype
                    | BuiltinOp::Valtype
                    | BuiltinOp::Supertype
                    | BuiltinOp::Supertypes
                    | BuiltinOp::Subtypes
                    | BuiltinOp::Typeintersect
                    // BuiltinOp::Typejoin removed - now Pure Julia (base/reflection.jl)
                    => StaticType::Any,
                    // Identity/symbol operations
                    // BuiltinOp::NameOf removed - now Pure Julia (base/reflection.jl)
                    BuiltinOp::Objectid => StaticType::U64,
                    // Fallback for any unknown builtins
                    _ => StaticType::Any,
                }
            }
            _ => StaticType::Any,
        }
    }

    /// Infer the result type of lowered `Broadcasted(fn, (args...))`.
    fn infer_broadcasted_result_type(&self, args: &[Expr], env: &TypeEnv) -> StaticType {
        if args.len() != 2 {
            return StaticType::Any;
        }

        let fn_name = match &args[0] {
            Expr::FunctionRef { name, .. } => name.as_str(),
            Expr::Var(name, _) => name.as_str(),
            _ => return StaticType::Any,
        };

        let bc_args: Vec<&Expr> = match &args[1] {
            Expr::TupleLiteral { elements, .. } => elements.iter().collect(),
            other => vec![other],
        };

        if bc_args.len() != 2 {
            return StaticType::Any;
        }

        fn unwrap_ref_expr(expr: &Expr) -> &Expr {
            if let Expr::Builtin {
                name: crate::ir::core::BuiltinOp::Ref,
                args,
                ..
            } = expr
            {
                if args.len() == 1 {
                    return &args[0];
                }
            }
            expr
        }

        let lhs_expr = unwrap_ref_expr(bc_args[0]);
        let rhs_expr = unwrap_ref_expr(bc_args[1]);

        let lhs_ty = self.infer_expr_type_with_env(lhs_expr, env);
        let rhs_ty = self.infer_expr_type_with_env(rhs_expr, env);

        let shape = |ty: &StaticType| -> usize {
            match ty {
                StaticType::Array { ndims: Some(n), .. } => *n,
                StaticType::Array { ndims: None, .. } => 1,
                _ => 0,
            }
        };

        // scalar .* vector => vector
        if fn_name == "*" && shape(&lhs_ty) == 0 && shape(&rhs_ty) == 1 {
            let rhs_elem = self.element_type(&rhs_ty);
            let result_elem = self.binop_result_type_static(&AotBinOp::Mul, &lhs_ty, &rhs_elem);
            return StaticType::Array {
                element: Box::new(result_elem),
                ndims: Some(1),
            };
        }

        // row .+ vector => matrix
        if fn_name == "+" && shape(&lhs_ty) == 2 && shape(&rhs_ty) == 1 {
            let lhs_elem = self.element_type(&lhs_ty);
            let rhs_elem = self.element_type(&rhs_ty);
            let result_elem = self.binop_result_type_static(&AotBinOp::Add, &lhs_elem, &rhs_elem);
            return StaticType::Array {
                element: Box::new(result_elem),
                ndims: Some(2),
            };
        }

        // matrix .(f) scalar => matrix
        if shape(&lhs_ty) == 2 && shape(&rhs_ty) == 0 {
            let lhs_elem = self.element_type(&lhs_ty);
            let result_elem = self.call_result_type(fn_name, &[lhs_elem, rhs_ty]);
            return StaticType::Array {
                element: Box::new(result_elem),
                ndims: Some(2),
            };
        }

        StaticType::Any
    }

    /// Get type of a literal
    pub(crate) fn literal_type(&self, lit: &Literal) -> StaticType {
        match lit {
            Literal::Int(_) => StaticType::I64,
            Literal::Int128(_) => StaticType::Any, // No direct i128 support in StaticType
            Literal::BigInt(_) => StaticType::Any,
            Literal::BigFloat(_) => StaticType::Any,
            Literal::Float(_) => StaticType::F64,
            Literal::Float32(_) => StaticType::F32,
            Literal::Float16(_) => StaticType::Any, // Float16 — AoT static type (StaticType has no F16)
            Literal::Bool(_) => StaticType::Bool,
            Literal::Str(_) => StaticType::Str,
            Literal::Char(_) => StaticType::Char,
            Literal::Nothing => StaticType::Nothing,
            Literal::Missing => StaticType::Missing,
            Literal::Undef => StaticType::Any,
            Literal::Module(_) => StaticType::Any, // Module type
            Literal::Array(_, shape) => StaticType::Array {
                element: Box::new(StaticType::F64),
                ndims: Some(shape.len()),
            },
            Literal::ArrayI64(_, shape) => StaticType::Array {
                element: Box::new(StaticType::I64),
                ndims: Some(shape.len()),
            },
            Literal::ArrayBool(_, shape) => StaticType::Array {
                element: Box::new(StaticType::Bool),
                ndims: Some(shape.len()),
            },
            Literal::Struct(name, _) => {
                let normalized = if name.starts_with("Complex{") {
                    "Complex".to_string()
                } else {
                    name.clone()
                };
                StaticType::Struct {
                    type_id: 0,
                    name: normalized,
                }
            }
            Literal::Symbol(_) => StaticType::Any, // Symbol type
            Literal::Expr { .. } => StaticType::Any, // Expr type
            Literal::QuoteNode(_) => StaticType::Any, // QuoteNode type
            Literal::LineNumberNode { .. } => StaticType::Any, // LineNumberNode type
            Literal::Regex { .. } => StaticType::Any, // Regex type
            Literal::Enum { .. } => StaticType::Any, // Enum type
        }
    }

    /// Get result type of binary operation
    pub fn binop_result_type(
        &self,
        op: &BinaryOp,
        left: &StaticType,
        right: &StaticType,
    ) -> StaticType {
        match op {
            // Comparison operators always return Bool
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Egal
            | BinaryOp::NotEgal
            | BinaryOp::Subtype => StaticType::Bool,
            // Logical operators
            BinaryOp::And | BinaryOp::Or => StaticType::Bool,
            // Arithmetic - promote types
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                // String concatenation in Julia uses *
                if matches!(left, StaticType::Str) && matches!(op, BinaryOp::Mul) {
                    StaticType::Str
                } else {
                    self.numeric_promote(left, right)
                }
            }
            BinaryOp::Div => StaticType::F64, // Division always returns float
            BinaryOp::IntDiv => self.integer_type(left, right),
            BinaryOp::Mod => self.integer_type(left, right),
            BinaryOp::Pow => {
                // Power: if exponent is integer and base is integer, result is integer
                // Otherwise float
                if left.is_integer() && right.is_integer() {
                    left.clone()
                } else if matches!(left, StaticType::Struct { .. }) && right.is_integer() {
                    left.clone()
                } else {
                    StaticType::F64
                }
            }
        }
    }

    /// Get result type of binary operation for AotBinOp
    /// Used when unfolding multi-argument operator calls
    pub fn binop_result_type_static(
        &self,
        op: &AotBinOp,
        left: &StaticType,
        right: &StaticType,
    ) -> StaticType {
        match op {
            // Comparison operators always return Bool
            AotBinOp::Lt
            | AotBinOp::Gt
            | AotBinOp::Le
            | AotBinOp::Ge
            | AotBinOp::Eq
            | AotBinOp::Ne
            | AotBinOp::Egal
            | AotBinOp::NotEgal => StaticType::Bool,
            // Logical operators
            AotBinOp::And | AotBinOp::Or => StaticType::Bool,
            // Arithmetic - promote types
            AotBinOp::Add | AotBinOp::Sub | AotBinOp::Mul => {
                // String concatenation in Julia uses *
                if matches!(left, StaticType::Str) && matches!(op, AotBinOp::Mul) {
                    StaticType::Str
                } else {
                    self.numeric_promote(left, right)
                }
            }
            AotBinOp::Div => StaticType::F64, // Division always returns float
            AotBinOp::IntDiv | AotBinOp::Mod => self.integer_type(left, right),
            AotBinOp::Pow => {
                if left.is_integer() && right.is_integer() {
                    left.clone()
                } else if matches!(left, StaticType::Struct { .. }) && right.is_integer() {
                    left.clone()
                } else {
                    StaticType::F64
                }
            }
            // Bitwise operators - preserve integer type
            AotBinOp::BitAnd
            | AotBinOp::BitOr
            | AotBinOp::BitXor
            | AotBinOp::Shl
            | AotBinOp::Shr => self.integer_type(left, right),
        }
    }

    /// Get result type of unary operation
    pub fn unaryop_result_type(&self, op: &UnaryOp, operand: &StaticType) -> StaticType {
        match op {
            UnaryOp::Neg => operand.clone(),
            UnaryOp::Not => StaticType::Bool,
            UnaryOp::Pos => operand.clone(),
        }
    }

    /// Get result type of function call
    pub fn call_result_type(&self, name: &str, arg_types: &[StaticType]) -> StaticType {
        // Check builtin signatures
        if let Some(signatures) = self.builtins.get(name) {
            for (params, ret) in signatures {
                if params.len() == arg_types.len() {
                    // Check if argument types match (simplified matching)
                    let matches = params
                        .iter()
                        .zip(arg_types.iter())
                        .all(|(p, a)| p == a || matches!(p, StaticType::Any));
                    if matches {
                        return ret.clone();
                    }
                }
            }
            // Fall back to first signature with matching arity
            for (params, ret) in signatures {
                if params.len() == arg_types.len() {
                    return ret.clone();
                }
            }
        }

        // Check if it's a type constructor
        match name {
            "Int64" | "Int" => StaticType::I64,
            "Int32" => StaticType::I32,
            "Float64" => StaticType::F64,
            "Float32" => StaticType::F32,
            "Bool" => StaticType::Bool,
            "String" => StaticType::Str,
            _ => StaticType::Any,
        }
    }

    /// Convert a type name string to StaticType
    /// Used for typed empty arrays like Int64[], Float64[], etc.
    pub(crate) fn type_name_to_static(&self, name: &str) -> StaticType {
        match name {
            "Int" | "Int64" => StaticType::I64,
            "Int32" => StaticType::I32,
            "Float64" => StaticType::F64,
            "Float32" => StaticType::F32,
            "Bool" => StaticType::Bool,
            "String" => StaticType::Str,
            "Char" => StaticType::Char,
            "Any" => StaticType::Any,
            "Nothing" => StaticType::Nothing,
            // Check if it's a known struct
            _ if self.structs.contains_key(name) => StaticType::Struct {
                type_id: 0,
                name: name.to_string(),
            },
            _ => StaticType::Any,
        }
    }

    /// Get element type of a container
    pub fn element_type(&self, container: &StaticType) -> StaticType {
        match container {
            StaticType::Array { element, .. } => (**element).clone(),
            StaticType::Tuple(elements) => {
                if elements.len() == 1 {
                    elements[0].clone()
                } else {
                    // Union of all element types
                    StaticType::Union {
                        variants: elements.clone(),
                    }
                }
            }
            StaticType::Range { element } => (**element).clone(),
            StaticType::Str => StaticType::Char,
            _ => StaticType::Any,
        }
    }

    /// Get element type of a tuple at a specific constant index (1-based Julia indexing)
    pub fn tuple_element_type_at(&self, container: &StaticType, index: usize) -> StaticType {
        match container {
            StaticType::Tuple(elements) => {
                // Julia uses 1-based indexing
                if index >= 1 && index <= elements.len() {
                    elements[index - 1].clone()
                } else {
                    // Out of bounds - return Any
                    StaticType::Any
                }
            }
            _ => self.element_type(container),
        }
    }

    /// Get field type of a struct
    pub fn field_type(&self, obj: &StaticType, field: &str) -> StaticType {
        if let StaticType::Struct { name, .. } = obj {
            if let Some(info) = self.structs.get(name) {
                if let Some(ty) = info.get_field_type(field) {
                    return ty.clone();
                }
            }
        }
        StaticType::Any
    }

    /// Convert literal to static type (alias for literal_type)
    pub fn literal_to_static(&self, lit: &Literal) -> StaticType {
        self.literal_type(lit)
    }
}

impl Default for TypeInferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}
