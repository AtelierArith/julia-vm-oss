//! Type inference engine for AoT compilation.
//!
//! The `TypeInferenceEngine` performs static type analysis for Julia programs,
//! inferring types for expressions, functions, and whole programs.
#![allow(clippy::cast_sign_loss)] // known-safe index/counter casts (i64->usize)

mod type_ops;

use super::super::ir::AotBinOp;
use super::super::specialization::{
    lattice_type_for_static_type, CodeInstanceKey, SpecializationQueue,
};
use super::super::types::StaticType;
use super::super::AotResult;
use super::types::{FunctionSignature, StructTypeInfo, TypeEnv, TypedFunction, TypedProgram};
use crate::ir::core::{
    BinaryOp, Block, EnumDef, Expr, Function, Literal, Program, Stmt, StructDef, UnaryOp,
};
use std::collections::{HashMap, HashSet};
use subset_julia_vm_types::{
    widen_argtype_for_cache_key, CacheArgType, ConstValue, InferenceCacheKey, LatticeType,
};

/// Recursively collect `@enum` definitions from a block, descending into nested
/// value blocks (`let`/`begin`) and `if` bodies. `@enum` lowers to a
/// `Stmt::EnumDef`; a CLI/test pass may wrap the whole main block in a value
/// block, so a flat top-level scan is not enough (Issue #7050).
pub(crate) fn collect_enum_defs_in_block<'a>(block: &'a Block, out: &mut Vec<&'a EnumDef>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::EnumDef { enum_def, .. } => out.push(enum_def),
            Stmt::Block(inner) => collect_enum_defs_in_block(inner, out),
            Stmt::Expr {
                expr: Expr::LetBlock { body, .. },
                ..
            } => collect_enum_defs_in_block(body, out),
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_enum_defs_in_block(then_branch, out);
                if let Some(else_block) = else_branch {
                    collect_enum_defs_in_block(else_block, out);
                }
            }
            _ => {}
        }
    }
}

pub struct TypeInferenceEngine {
    /// Built-in function signatures (name -> return type for common arities)
    pub(crate) builtins: HashMap<String, Vec<(Vec<StaticType>, StaticType)>>,
    /// Current type environment
    pub env: TypeEnv,
    /// Struct definitions (public for setting from IrConverter)
    pub structs: HashMap<String, StructTypeInfo>,
    /// CodeInstance-like specializations discovered from root call sites and
    /// function-body dependency edges.
    pub(crate) specializations: SpecializationQueue,
    /// Current owner while collecting calls inside a function body.
    active_instance: Option<CodeInstanceKey>,
    /// `@enum` member names → their Int32-backed type. Persistent across the
    /// per-function `env.clear()` so members referenced inside functions still
    /// type as Int32 rather than `Any` (Issue #7050).
    pub(crate) enum_members: HashMap<String, StaticType>,
    /// `@enum` member names → their Int32 backing value.
    pub(crate) enum_member_values: HashMap<String, i32>,
}

impl TypeInferenceEngine {
    /// Create a new type inference engine
    pub fn new() -> Self {
        let mut engine = Self {
            builtins: HashMap::new(),
            env: HashMap::new(),
            structs: HashMap::new(),
            specializations: SpecializationQueue::new(),
            active_instance: None,
            enum_members: HashMap::new(),
            enum_member_values: HashMap::new(),
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
        self.register_builtin("sqrt", vec![StaticType::F32], StaticType::F32);

        self.register_builtin("sin", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("sin", vec![StaticType::F32], StaticType::F32);
        self.register_builtin("cos", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("cos", vec![StaticType::F32], StaticType::F32);
        self.register_builtin("tan", vec![StaticType::F64], StaticType::F64);
        self.register_builtin("tan", vec![StaticType::F32], StaticType::F32);

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
        self.register_builtin(
            "abs2",
            vec![StaticType::Struct {
                type_id: 0,
                name: "Complex{Float64}".to_string(),
            }],
            StaticType::F64,
        );
        self.register_builtin(
            "abs2",
            vec![StaticType::Struct {
                type_id: 0,
                name: "Complex{Float32}".to_string(),
            }],
            StaticType::F32,
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

        // Type reflection
        self.register_builtin("typeof", vec![StaticType::Any], StaticType::DataType);
    }

    /// Register a built-in function signature
    pub(crate) fn register_builtin(
        &mut self,
        name: &str,
        params: Vec<StaticType>,
        ret: StaticType,
    ) {
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

        // Pre-register `@enum` member names as Int32 globals before any function
        // inference, so a member referenced inside a function (`c = green`)
        // types as Int32 instead of `Any` (Issue #7050). `@enum` lowers to a
        // `Stmt::EnumDef`; the scan is recursive because a test/CLI pass may
        // wrap the main block in a `let`/`begin` value block.
        let mut enum_defs = Vec::new();
        collect_enum_defs_in_block(&program.main, &mut enum_defs);
        for enum_def in enum_defs {
            for member in &enum_def.members {
                self.enum_members
                    .insert(member.name.clone(), StaticType::I32);
                self.enum_member_values
                    .insert(member.name.clone(), member.value as i32);
                self.env.insert(member.name.clone(), StaticType::I32);
            }
        }

        // Collect user-defined function names for call-site specialization
        let user_functions: HashSet<String> =
            program.functions.iter().map(|f| f.name.clone()).collect();

        // Build a map from function name to function for quick lookup
        let _func_map: HashMap<String, &Function> = program
            .functions
            .iter()
            .map(|f| (f.name.clone(), f.as_ref()))
            .collect();

        // Iterative type inference:
        // 1. First, collect call sites from main block (where we have concrete types)
        // 2. Infer function signatures based on collected call sites
        // 3. Re-collect call sites from function bodies with inferred types
        // 4. Repeat until no new call sites are discovered

        const MAX_ITERATIONS: usize = 10;
        for _iteration in 0..MAX_ITERATIONS {
            let old_specializations = self.specializations.keys_snapshot();

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

                let previous_instance = self.active_instance.replace(CodeInstanceKey::new(
                    func.name.clone(),
                    sig.param_types.clone(),
                ));
                self.collect_call_sites_from_block(&func.body, &user_functions);
                self.active_instance = previous_instance;
            }

            // Check if call sites have stabilized
            if self.specializations.keys_snapshot() == old_specializations {
                break;
            }
        }

        // Clear env for function analysis
        self.env.clear();

        // Pre-register user function signatures so HOF return inference can see
        // forward and function-value references before each body is analyzed.
        for func in &program.functions {
            let sig = self.infer_function_signature(func);
            self.register_builtin(&sig.name, sig.param_types.clone(), sig.return_type.clone());
        }

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
            self.specializations
                .attach_inference(func, typed_func.signature.clone());
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
                    self.env.insert(inner_var.to_string(), inner_ty.clone());
                    self.env.insert(var.clone(), inner_ty);
                } else {
                    // Infer type of the assigned value
                    let ty = self.infer_expr_type(value);
                    self.env.insert(var.clone(), ty);
                }
            }
            Stmt::DestructuringAssign { targets, value, .. } => {
                let rhs_ty = self.infer_expr_type(value);
                for (index, target) in targets.iter().enumerate() {
                    let ty = self.tuple_element_type_at(&rhs_ty, index + 1);
                    self.env.insert(target.clone(), ty);
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
            Stmt::Expr {
                expr: Expr::LetBlock { bindings, body, .. },
                ..
            } => {
                for (name, value) in bindings {
                    let ty = self.infer_expr_type(value);
                    self.env.insert(name.to_string(), ty);
                }
                self.setup_local_env_from_block(body);
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
            Stmt::Assign { value, .. } | Stmt::DestructuringAssign { value, .. } => {
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

                // Broadcasted(function_ref, (args...)) carries call-sites in
                // function-value form. Lowering may represent user functions
                // as either `FunctionRef` or a bare `Var` here.
                if function == "Broadcasted" && args.len() == 2 {
                    let broadcast_fn = match &args[0] {
                        Expr::FunctionRef { name, .. } | Expr::Var(name, _) => Some(name.as_str()),
                        _ => None,
                    };
                    if let Some(name) = broadcast_fn {
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
                                self.enqueue_call_site(name, arg_types);
                            }
                        }
                    }
                }

                // If this is a call to a user-defined function, record the argument types
                if user_functions.contains(function.as_str()) {
                    let arg_types: Vec<StaticType> = args
                        .iter()
                        .map(|a| self.infer_expr_type(a))
                        .chain(kwargs.iter().map(|(_, a)| self.infer_expr_type(a)))
                        .collect();
                    let arg_key: Vec<CacheArgType> = args
                        .iter()
                        .chain(kwargs.iter().map(|(_, a)| a))
                        .zip(arg_types.iter())
                        .map(|(arg, ty)| self.arg_key_for_expr(arg, ty))
                        .collect();

                    // Only record if we have concrete types (not all Any)
                    let has_concrete = arg_types.iter().any(|t| !matches!(t, StaticType::Any));
                    if has_concrete {
                        self.enqueue_call_site_with_arg_key(function, arg_types, arg_key);
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

    fn enqueue_call_site(&mut self, function: &str, arg_types: Vec<StaticType>) {
        let callee = CodeInstanceKey::new(function.to_string(), arg_types);
        if let Some(owner) = self.active_instance.clone() {
            self.specializations.add_dependency(&owner, callee);
        } else {
            self.specializations.enqueue(callee);
        }
    }

    fn enqueue_call_site_with_arg_key(
        &mut self,
        function: &str,
        arg_types: Vec<StaticType>,
        arg_key: Vec<CacheArgType>,
    ) {
        let inference_key = InferenceCacheKey::from_argtypes(function, arg_key);
        let callee =
            CodeInstanceKey::new_with_inference_key(function.to_string(), arg_types, inference_key);
        if let Some(owner) = self.active_instance.clone() {
            self.specializations.add_dependency(&owner, callee);
        } else {
            self.specializations.enqueue(callee);
        }
    }

    /// Map a call-site argument expression to its AoT specialization key.
    ///
    /// Const literals are lifted to a `ConstValue`, then normalized by the
    /// compile-side cache-key policy itself. Non-const expressions fall back to
    /// the ABI `StaticType` projected into the shared lattice type.
    fn arg_key_for_expr(&self, expr: &Expr, ty: &StaticType) -> CacheArgType {
        match Self::const_value_for_arg_expr(expr) {
            Some(cv) => widen_argtype_for_cache_key(&LatticeType::Const(cv)),
            None => CacheArgType::Type(lattice_type_for_static_type(ty)),
        }
    }

    /// Lift a literal call-site argument to a lattice `ConstValue`, mirroring
    /// the compile-side const lattice. Returns `None` for non-literal or
    /// not-yet-modeled literals so they widen to their ABI `StaticType`.
    fn const_value_for_arg_expr(expr: &Expr) -> Option<ConstValue> {
        match expr {
            Expr::Literal(Literal::Bool(v), _) => Some(ConstValue::Bool(*v)),
            Expr::Literal(Literal::Nothing, _) => Some(ConstValue::Nothing),
            Expr::Literal(Literal::Symbol(s), _) => Some(ConstValue::Symbol(s.clone())),
            Expr::Literal(Literal::Int(n), _) => Some(ConstValue::Int64(*n)),
            _ => None,
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
                StaticType::from_type_expr_lossy(type_expr)
            } else {
                StaticType::Any
            };
            info.add_field(field.name.clone(), field_type);
        }

        Ok(info)
    }

    /// Infer function signature with call-site specialization
    ///
    /// If a parameter has no type annotation but we have observed call sites
    /// with concrete types, we use the most general type that covers all call sites.
    pub fn infer_function_signature(&self, func: &Function) -> FunctionSignature {
        let mut param_names: Vec<_> = func.params.iter().map(|p| p.name.clone()).collect();

        // Start with declared types (or Any if not declared)
        let untyped_params: Vec<_> = func
            .params
            .iter()
            .map(|p| p.type_annotation.is_none())
            .collect();
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
        let call_sites = self.specializations.observed_args_for(&func.name);
        if !call_sites.is_empty() {
            for (i, param_ty) in param_types.iter_mut().enumerate() {
                if untyped_params.get(i).copied().unwrap_or(false)
                    && matches!(param_ty, StaticType::Any)
                {
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
                            *param_ty = observed_types.remove(0);
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

        // Keyword parameters are modeled as trailing positional parameters so
        // they are in scope for body / return-type inference and flow into the
        // generated Rust signature, matching the call-site filling in the IR
        // converter (Issue #7042). `kwargs...`-varargs keyword params stay out.
        for kwp in &func.kwparams {
            if kwp.is_varargs || param_names.iter().any(|n| n == &kwp.name) {
                continue;
            }
            let ty = kwp
                .type_annotation
                .as_ref()
                .map(StaticType::from)
                .unwrap_or_else(|| self.infer_expr_type(&kwp.default));
            param_names.push(kwp.name.clone());
            param_types.push(ty);
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
                        let inner_merged =
                            self.merge_local_type(&mut locals, inner_var, inner_ty.clone());
                        self.env.insert(inner_var.to_string(), inner_merged);
                        let merged = self.merge_local_type(&mut locals, var, inner_ty);
                        self.env.insert(var.clone(), merged);
                    } else {
                        let ty = self.infer_expr_type(value);
                        let merged = self.merge_local_type(&mut locals, var, ty);
                        self.env.insert(var.clone(), merged);
                    }
                }
                Stmt::DestructuringAssign { targets, value, .. } => {
                    let rhs_ty = self.infer_expr_type(value);
                    for (index, target) in targets.iter().enumerate() {
                        let ty = self.tuple_element_type_at(&rhs_ty, index + 1);
                        let merged = self.merge_local_type(&mut locals, target, ty);
                        self.env.insert(target.clone(), merged);
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
                    let merged = self.merge_local_type(&mut locals, var, var_ty);
                    self.env.insert(var.clone(), merged);

                    // Recurse into body
                    let body_locals = self.collect_local_types(body)?;
                    self.merge_local_types(&mut locals, body_locals);
                }
                Stmt::ForEach {
                    var,
                    iterable,
                    body,
                    ..
                } => {
                    let iter_ty = self.infer_expr_type(iterable);
                    let elem_ty = self.element_type(&iter_ty);
                    let merged = self.merge_local_type(&mut locals, var, elem_ty);
                    self.env.insert(var.clone(), merged);

                    let body_locals = self.collect_local_types(body)?;
                    self.merge_local_types(&mut locals, body_locals);
                }
                Stmt::While { body, .. } => {
                    let body_locals = self.collect_local_types(body)?;
                    self.merge_local_types(&mut locals, body_locals);
                }
                Stmt::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    let then_locals = self.collect_local_types(then_branch)?;
                    self.merge_local_types(&mut locals, then_locals);

                    if let Some(else_block) = else_branch {
                        let else_locals = self.collect_local_types(else_block)?;
                        self.merge_local_types(&mut locals, else_locals);
                    }
                }
                Stmt::Block(inner_block) => {
                    let inner_locals = self.collect_local_types(inner_block)?;
                    self.merge_local_types(&mut locals, inner_locals);
                }
                Stmt::Expr {
                    expr: Expr::LetBlock { bindings, body, .. },
                    ..
                } => {
                    for (name, value) in bindings {
                        let ty = self.infer_expr_type(value);
                        let merged = self.merge_local_type(&mut locals, name, ty);
                        self.env.insert(name.to_string(), merged);
                    }
                    let body_locals = self.collect_local_types(body)?;
                    self.merge_local_types(&mut locals, body_locals);
                }
                // `@enum Color red green` binds each member name to an Int32
                // value (the enum is Int32-backed in AoT). Register them so
                // references / `Int(c)` / `c == member` type correctly instead
                // of falling to the dynamic `Any` boundary (Issue #7050).
                Stmt::EnumDef { enum_def, .. } => {
                    for member in &enum_def.members {
                        locals.insert(member.name.clone(), StaticType::I32);
                        self.env.insert(member.name.clone(), StaticType::I32);
                        self.enum_members
                            .insert(member.name.clone(), StaticType::I32);
                        self.enum_member_values
                            .insert(member.name.clone(), member.value as i32);
                    }
                }
                _ => {}
            }
        }

        Ok(locals)
    }

    fn merge_local_type(&self, locals: &mut TypeEnv, name: &str, ty: StaticType) -> StaticType {
        let merged = locals
            .get(name)
            .map_or_else(|| ty.clone(), |existing| self.join_types(existing, &ty));
        locals.insert(name.to_string(), merged.clone());
        merged
    }

    fn merge_local_types(&self, locals: &mut TypeEnv, incoming: TypeEnv) {
        for (name, ty) in incoming {
            self.merge_local_type(locals, &name, ty);
        }
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
            // No explicit return: the function returns the value of the last statement.
            // In Julia, an assignment expression has a value (the assigned value), so a
            // function whose last statement is an assignment returns that value
            // (Issue #3542).
            match block.stmts.last() {
                Some(Stmt::Expr { expr, .. }) => self.infer_expr_type_with_env(expr, &env),
                Some(Stmt::Assign { value, .. }) => self.infer_expr_type_with_env(value, &env),
                Some(Stmt::DestructuringAssign { value, .. }) => {
                    self.infer_expr_type_with_env(value, &env)
                }
                Some(Stmt::If {
                    then_branch,
                    else_branch: Some(else_block),
                    ..
                }) => {
                    // if-else as last expression: join branch values
                    let then_type = self.infer_block_value_type(then_branch, &env);
                    let else_type = self.infer_block_value_type(else_block, &env);
                    self.join_types(&then_type, &else_type)
                }
                Some(Stmt::Block(inner)) => self.infer_block_value_type(inner, &env),
                _ => StaticType::Nothing,
            }
        } else if return_types.len() == 1 {
            return_types.remove(0)
        } else {
            // Multiple return types - create union
            StaticType::Union {
                variants: return_types,
            }
        }
    }

    /// Infer the value type of a block (last statement value).
    ///
    /// Recognises:
    /// - `Stmt::Expr`: the expression's value
    /// - `Stmt::Assign` / `Stmt::DestructuringAssign`: the assigned value
    /// - `Stmt::If`: join of branch values when both branches present
    /// - `Stmt::Block`: nested block's value
    fn infer_block_value_type(&self, block: &Block, env: &TypeEnv) -> StaticType {
        match block.stmts.last() {
            Some(Stmt::Expr { expr, .. }) => self.infer_expr_type_with_env(expr, env),
            Some(Stmt::Assign { value, .. }) => self.infer_expr_type_with_env(value, env),
            Some(Stmt::DestructuringAssign { value, .. }) => {
                self.infer_expr_type_with_env(value, env)
            }
            Some(Stmt::If {
                then_branch,
                else_branch: Some(else_block),
                ..
            }) => {
                let then_type = self.infer_block_value_type(then_branch, env);
                let else_type = self.infer_block_value_type(else_block, env);
                self.join_types(&then_type, &else_type)
            }
            Some(Stmt::Block(inner)) => self.infer_block_value_type(inner, env),
            _ => StaticType::Nothing,
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
                        env.insert(inner_var.to_string(), inner_ty.clone());
                        env.insert(var.clone(), inner_ty);
                    } else {
                        let ty = self.infer_expr_type_with_env(value, env);
                        env.insert(var.clone(), ty);
                    }
                }
                Stmt::DestructuringAssign { targets, value, .. } => {
                    let rhs_ty = self.infer_expr_type_with_env(value, env);
                    for (index, target) in targets.iter().enumerate() {
                        let ty = self.tuple_element_type_at(&rhs_ty, index + 1);
                        env.insert(target.clone(), ty);
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
                Stmt::Expr {
                    expr: Expr::LetBlock { bindings, body, .. },
                    ..
                } => {
                    let mut local_env = env.clone();
                    for (name, value) in bindings {
                        let ty = self.infer_expr_type_with_env(value, &local_env);
                        local_env.insert(name.to_string(), ty);
                    }
                    self.collect_local_types_for_env(body, &mut local_env);
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
                Stmt::Return { value: None, .. } if !types.contains(&StaticType::Nothing) => {
                    types.push(StaticType::Nothing);
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
            // A bare Symbol literal `:foo` carries its interned name as a string
            // in AoT (Issue #7051), so it types as `Str` — keeping it out of the
            // dynamic `Value` boundary so it prints with print (no quotes) rather
            // than show semantics. Quoted expressions stay `Any`.
            Expr::QuoteLiteral { constructor, .. }
                if matches!(
                    constructor.as_ref(),
                    Expr::Builtin { name: crate::ir::core::BuiltinOp::SymbolNew, args, .. }
                        if args.len() == 1
                            && matches!(args.first(), Some(Expr::Literal(Literal::Str(_), _)))
                ) =>
            {
                StaticType::Str
            }
            Expr::Var(name, _) => env
                .get(name.as_str())
                .cloned()
                .or_else(|| self.function_ref_type(name))
                .unwrap_or_else(|| self.lookup_global_or_const(name)),
            Expr::FunctionRef { name, .. } => {
                self.function_ref_type(name).unwrap_or(StaticType::Any)
            }
            Expr::AssignExpr { value, .. } => self.infer_expr_type_with_env(value, env),
            Expr::LetBlock { bindings, body, .. } => {
                let mut local_env = env.clone();
                for (name, value) in bindings {
                    let ty = self.infer_expr_type_with_env(value, &local_env);
                    local_env.insert(name.to_string(), ty);
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
                        let target_type = self.type_name_to_static(type_name);
                        if !matches!(target_type, StaticType::Any) {
                            return target_type;
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

                if let Some(hof_ty) = self.infer_hof_call_type(function, &call_args, env) {
                    return hof_ty;
                }

                if function == "#__sjulia_tuple_tail__" && call_args.len() == 2 {
                    let tuple_ty = self.infer_expr_type_with_env(call_args[0], env);
                    if let Expr::Literal(Literal::Int(start_index), _) = call_args[1] {
                        if let Ok(start_index) = usize::try_from(*start_index) {
                            return self.tuple_tail_type_at(&tuple_ty, start_index);
                        }
                    }
                    return StaticType::Any;
                }

                if matches!(function.as_str(), "zeros" | "ones") {
                    let (element_ty, ndims) = self.array_constructor_element_and_ndims(&call_args);
                    return StaticType::Array {
                        element: Box::new(element_ty),
                        ndims: Some(ndims),
                    };
                }

                let arg_types: Vec<_> = call_args
                    .iter()
                    .map(|a| self.infer_expr_type_with_env(a, env))
                    .collect();
                if let Some(dict_ty) = Self::dict_constructor_type(function, &arg_types) {
                    return dict_ty;
                }
                if let Some(element_ty) = Self::set_constructor_element_type(function, &arg_types) {
                    return StaticType::Set {
                        element: Box::new(element_ty),
                    };
                }
                if let Some((name, _field_types)) =
                    self.parametric_constructor_info(function, &arg_types)
                {
                    return StaticType::Struct { type_id: 0, name };
                }
                // Check if it's a struct constructor
                if let Some(struct_info) = self.structs.get(function.as_str()) {
                    return StaticType::Struct {
                        type_id: 0,
                        name: struct_info.name.clone(),
                    };
                }
                if StaticType::complex_param_type_from_name(function).is_some() {
                    return StaticType::Struct {
                        type_id: 0,
                        name: function.to_string(),
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
                if let StaticType::Dict { value, .. } = &arr_ty {
                    return value.as_ref().clone();
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
            Expr::NamedTupleLiteral { fields, .. } => StaticType::NamedTuple(
                fields
                    .iter()
                    .map(|(name, expr)| {
                        (name.to_string(), self.infer_expr_type_with_env(expr, env))
                    })
                    .collect(),
            ),
            Expr::Pair { key, value, .. } => StaticType::Tuple(vec![
                self.infer_expr_type_with_env(key, env),
                self.infer_expr_type_with_env(value, env),
            ]),
            Expr::DictLiteral { pairs, .. } => {
                let pair_types: Vec<_> = pairs
                    .iter()
                    .map(|(key, value)| {
                        StaticType::Tuple(vec![
                            self.infer_expr_type_with_env(key, env),
                            self.infer_expr_type_with_env(value, env),
                        ])
                    })
                    .collect();
                Self::dict_constructor_type("Dict", &pair_types).unwrap_or(StaticType::Dict {
                    key: Box::new(StaticType::Any),
                    value: Box::new(StaticType::Any),
                })
            }
            Expr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            } => {
                let iter_ty = self.infer_expr_type_with_env(iter, env);
                let elem_ty = self.element_type(&iter_ty);
                let mut local_env = env.clone();
                local_env.insert(var.to_string(), elem_ty);
                if let Some(filter) = filter {
                    let _ = self.infer_expr_type_with_env(filter, &local_env);
                }
                StaticType::Array {
                    element: Box::new(self.infer_expr_type_with_env(body, &local_env)),
                    ndims: Some(1),
                }
            }
            Expr::Generator {
                body,
                var,
                iter,
                filter,
                ..
            } => {
                let iter_ty = self.infer_expr_type_with_env(iter, env);
                let elem_ty = self.element_type(&iter_ty);
                let mut local_env = env.clone();
                local_env.insert(var.to_string(), elem_ty);
                if let Some(filter) = filter {
                    let _ = self.infer_expr_type_with_env(filter, &local_env);
                }
                StaticType::Generator {
                    element: Box::new(self.infer_expr_type_with_env(body, &local_env)),
                }
            }
            Expr::MultiComprehension {
                body,
                iterations,
                filter,
                ..
            } => {
                let mut local_env = env.clone();
                for (var, iter) in iterations {
                    let iter_ty = self.infer_expr_type_with_env(iter, &local_env);
                    local_env.insert(var.to_string(), self.element_type(&iter_ty));
                }
                if let Some(filter) = filter {
                    let _ = self.infer_expr_type_with_env(filter, &local_env);
                }
                StaticType::Array {
                    element: Box::new(self.infer_expr_type_with_env(body, &local_env)),
                    ndims: Some(1),
                }
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
            Expr::Range {
                start, stop, step, ..
            } => {
                let start_ty = self.infer_expr_type_with_env(start, env);
                let stop_ty = self.infer_expr_type_with_env(stop, env);
                let mut elem_ty = self.unify_types(&start_ty, &stop_ty);
                if let Some(step_expr) = step {
                    let step_ty = self.infer_expr_type_with_env(step_expr, env);
                    elem_ty = self.unify_types(&elem_ty, &step_ty);
                }
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
                    // Sqrt preserves Float16/Float32; integers and Float64 return Float64
                    BuiltinOp::Sqrt => match arg_types.first() {
                        Some(StaticType::F16) => StaticType::F16,
                        Some(StaticType::F32) => StaticType::F32,
                        _ => StaticType::F64,
                    },
                    BuiltinOp::Rand | BuiltinOp::Randn => {
                        if args.is_empty() {
                            StaticType::F64
                        } else {
                            StaticType::Array {
                                element: Box::new(StaticType::F64),
                                ndims: Some(args.len()),
                            }
                        }
                    }
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
                    // size(arr, dim) -> I64; size(arr) -> Tuple with arity from ndims
                    BuiltinOp::Size => {
                        if args.len() == 2 {
                            StaticType::I64
                        } else {
                            let n = if let Some(StaticType::Array { ndims: Some(n), .. }) =
                                arg_types.first()
                            {
                                *n
                            } else {
                                1
                            };
                            StaticType::Tuple(vec![StaticType::I64; n])
                        }
                    }
                    // Mutating operations return the array
                    BuiltinOp::Push | BuiltinOp::PushFirst | BuiltinOp::Insert | BuiltinOp::DeleteAt => {
                        if let Some(arr_ty) = arg_types.first() {
                            arr_ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // Pop operations return element type
                    BuiltinOp::Pop | BuiltinOp::PopFirst => {
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
                    BuiltinOp::StableRNG
                    | BuiltinOp::XoshiroRNG
                    | BuiltinOp::MersenneTwisterRNG => StaticType::Any,
                    // Tuple operations
                    BuiltinOp::TupleFirst => {
                        if let Some(StaticType::Tuple(elems)) = arg_types.first() {
                            if !elems.is_empty() {
                                return elems[0].clone();
                            }
                        }
                        StaticType::Any
                    }
                    BuiltinOp::TupleLast => {
                        if let Some(StaticType::Tuple(elems)) = arg_types.first() {
                            if let Some(last) = elems.last() {
                                return last.clone();
                            }
                        }
                        StaticType::Any
                    }
                    BuiltinOp::RangeStep => StaticType::Any,
                    // Note: TupleLength removed — dead code (Issue #2643)
                    // Dict operations
                    BuiltinOp::HasKey | BuiltinOp::In => StaticType::Bool,
                    BuiltinOp::DictGet | BuiltinOp::DictGetBang => {
                        // Returns dict value type when known; otherwise join with default arg
                        match arg_types.first() {
                            Some(StaticType::Dict { value, .. }) => (**value).clone(),
                            _ => StaticType::Any,
                        }
                    }
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
                    // typeof(x) returns a Julia DataType/type object value, not
                    // a Rust carrier name or ordinary String (Issues #6973, #7015).
                    BuiltinOp::TypeOf => StaticType::DataType,
                    BuiltinOp::Isa => StaticType::Bool,
                    // Broadcasting control
                    BuiltinOp::Ref => {
                        if let Some(ty) = arg_types.first() {
                            ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // empty!(dict) clears and returns the dict (Issue #3471)
                    BuiltinOp::DictEmpty => {
                        if let Some(dict_ty) = arg_types.first() {
                            dict_ty.clone()
                        } else {
                            StaticType::Any
                        }
                    }
                    // getkey(d, key, default) returns the stored key or default (Issue #3471)
                    BuiltinOp::DictGetkey => {
                        match arg_types.first() {
                            Some(StaticType::Dict { key, .. }) => (**key).clone(),
                            _ => StaticType::Any,
                        }
                    }
                    // Size-related operations
                    BuiltinOp::Sizeof => StaticType::I64,
                    // Boolean predicates
                    BuiltinOp::Isbitstype
                    // Isbits, Hasfield, Ismutable removed - pure Julia (Issue #6738)
                    // Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
                    // removed - now Pure Julia (base/reflection.jl)
                    => StaticType::Bool,
                    // Type operations returning types (represented as Any)
                    BuiltinOp::Eltype
                    | BuiltinOp::Keytype
                    | BuiltinOp::Valtype
                    | BuiltinOp::Supertype
                    | BuiltinOp::Subtypes
                    // BuiltinOp::Typeintersect/Typejoin removed - now Pure Julia (base/reflection.jl)
                    => StaticType::Any,
                    // Identity/symbol operations. AoT has no Symbol carrier in
                    // StaticType, so keep these dynamically typed.
                    BuiltinOp::Typename | BuiltinOp::FunctionName => StaticType::Any,
                    // BuiltinOp::NameOf removed - now Pure Julia (base/reflection.jl)
                    BuiltinOp::Objectid => StaticType::U64,
                    // Fallback for any unknown builtins
                    _ => StaticType::Any,
                }
            }
            _ => StaticType::Any,
        }
    }

    fn function_name_from_expr<'a>(&self, expr: &'a Expr) -> Option<&'a str> {
        match expr {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => Some(name.as_str()),
            _ => None,
        }
    }

    fn normalize_function_value_name(name: &str) -> &str {
        match name {
            "op_add" => "+",
            "op_mul" => "*",
            "op_sub" => "-",
            "op_div" => "/",
            other => other,
        }
    }

    fn array_ndims_for_type(ty: &StaticType) -> Option<usize> {
        match ty {
            StaticType::Array { ndims, .. } => Some(ndims.unwrap_or(1)),
            StaticType::Range { .. } => Some(1),
            _ => None,
        }
    }

    fn function_ref_type(&self, name: &str) -> Option<StaticType> {
        let normalized = Self::normalize_function_value_name(name);
        let (params, ret) = self.builtins.get(normalized)?.first()?;
        Some(StaticType::Function {
            params: params.clone(),
            ret: Box::new(ret.clone()),
        })
    }

    fn infer_hof_call_type(
        &self,
        function: &str,
        call_args: &[&Expr],
        env: &TypeEnv,
    ) -> Option<StaticType> {
        match function {
            "map" | "Base.map" if call_args.len() == 2 => {
                let fn_name = self.function_name_from_expr(call_args[0])?;
                let arr_ty = self.infer_expr_type_with_env(call_args[1], env);
                let ndims = Self::array_ndims_for_type(&arr_ty)?;
                let elem_ty = self.element_type(&arr_ty);
                let result_elem =
                    self.call_result_type(Self::normalize_function_value_name(fn_name), &[elem_ty]);
                if matches!(result_elem, StaticType::Any) {
                    None
                } else {
                    Some(StaticType::Array {
                        element: Box::new(result_elem),
                        ndims: Some(ndims),
                    })
                }
            }
            "filter" | "Base.filter" if call_args.len() == 2 => {
                let arr_ty = self.infer_expr_type_with_env(call_args[1], env);
                let ndims = Self::array_ndims_for_type(&arr_ty)?;
                Some(StaticType::Array {
                    element: Box::new(self.element_type(&arr_ty)),
                    ndims: Some(ndims),
                })
            }
            "reduce" | "Base.reduce" | "foldl" | "Base.foldl" if call_args.len() >= 2 => {
                let fn_name = self.function_name_from_expr(call_args[0])?;
                let arr_ty = self.infer_expr_type_with_env(call_args[1], env);
                let elem_ty = self.element_type(&arr_ty);
                let result = self.call_result_type(
                    Self::normalize_function_value_name(fn_name),
                    &[elem_ty.clone(), elem_ty],
                );
                if matches!(result, StaticType::Any) {
                    Some(self.element_type(&arr_ty))
                } else {
                    Some(result)
                }
            }
            "sum" | "Base.sum" if call_args.len() == 1 => {
                let arr_ty = self.infer_expr_type_with_env(call_args[0], env);
                Some(self.element_type(&arr_ty))
            }
            "sum" | "Base.sum" if call_args.len() == 2 => {
                let fn_name = self.function_name_from_expr(call_args[0])?;
                let arr_ty = self.infer_expr_type_with_env(call_args[1], env);
                let elem_ty = self.element_type(&arr_ty);
                let mapped =
                    self.call_result_type(Self::normalize_function_value_name(fn_name), &[elem_ty]);
                if matches!(mapped, StaticType::Any) {
                    None
                } else {
                    Some(mapped)
                }
            }
            "mapreduce" | "Base.mapreduce" if call_args.len() >= 3 => {
                let map_name = self.function_name_from_expr(call_args[0])?;
                let op_name = self.function_name_from_expr(call_args[1])?;
                let arr_ty = self.infer_expr_type_with_env(call_args[2], env);
                let elem_ty = self.element_type(&arr_ty);
                let mapped = self
                    .call_result_type(Self::normalize_function_value_name(map_name), &[elem_ty]);
                if matches!(mapped, StaticType::Any) {
                    return None;
                }
                let reduced = self.call_result_type(
                    Self::normalize_function_value_name(op_name),
                    &[mapped.clone(), mapped.clone()],
                );
                if matches!(reduced, StaticType::Any) {
                    Some(mapped)
                } else {
                    Some(reduced)
                }
            }
            _ => None,
        }
    }

    /// Infer the result type of lowered `Broadcasted(fn, (args...))`. (Issue #3464)
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

        fn array_ndims(ty: &StaticType) -> usize {
            match ty {
                StaticType::Array { ndims: Some(n), .. } => *n,
                StaticType::Array { ndims: None, .. } => 1,
                _ => 0,
            }
        }

        fn op_from_name(name: &str) -> Option<AotBinOp> {
            match name {
                "+" => Some(AotBinOp::Add),
                "-" => Some(AotBinOp::Sub),
                "*" => Some(AotBinOp::Mul),
                "/" => Some(AotBinOp::Div),
                "^" => Some(AotBinOp::Pow),
                "==" => Some(AotBinOp::Eq),
                "!=" => Some(AotBinOp::Ne),
                "<" => Some(AotBinOp::Lt),
                ">" => Some(AotBinOp::Gt),
                "<=" => Some(AotBinOp::Le),
                ">=" => Some(AotBinOp::Ge),
                _ => None,
            }
        }

        // Unary broadcast: f.(v) -> Array{result_elem}
        if bc_args.len() == 1 {
            let arg_ty = self.infer_expr_type_with_env(unwrap_ref_expr(bc_args[0]), env);
            let ndims = array_ndims(&arg_ty);
            if ndims > 0 {
                let elem = self.element_type(&arg_ty);
                let result_elem = self.call_result_type(fn_name, &[elem]);
                return StaticType::Array {
                    element: Box::new(result_elem),
                    ndims: Some(ndims),
                };
            }
            return StaticType::Any;
        }

        if bc_args.len() != 2 {
            return StaticType::Any;
        }

        let lhs_ty = self.infer_expr_type_with_env(unwrap_ref_expr(bc_args[0]), env);
        let rhs_ty = self.infer_expr_type_with_env(unwrap_ref_expr(bc_args[1]), env);

        let lhs_ndims = array_ndims(&lhs_ty);
        let rhs_ndims = array_ndims(&rhs_ty);
        let result_ndims = lhs_ndims.max(rhs_ndims);

        if result_ndims == 0 {
            return StaticType::Any;
        }

        // Extract element types (scalar stays as-is)
        let lhs_elem = if lhs_ndims > 0 {
            self.element_type(&lhs_ty)
        } else {
            lhs_ty.clone()
        };
        let rhs_elem = if rhs_ndims > 0 {
            self.element_type(&rhs_ty)
        } else {
            rhs_ty.clone()
        };

        let result_elem = if let Some(op) = op_from_name(fn_name) {
            self.binop_result_type_static(&op, &lhs_elem, &rhs_elem)
        } else {
            self.call_result_type(fn_name, &[lhs_elem, rhs_elem])
        };

        StaticType::Array {
            element: Box::new(result_elem),
            ndims: Some(result_ndims),
        }
    }

    /// Get type of a literal
    pub(crate) fn literal_type(&self, lit: &Literal) -> StaticType {
        match lit {
            Literal::Int(_) => StaticType::I64,
            Literal::Int128(_) => StaticType::I128,
            Literal::BigInt(_) => StaticType::Any,
            Literal::BigFloat(_) => StaticType::Any,
            Literal::Float(_) => StaticType::F64,
            Literal::Float32(_) => StaticType::F32,
            Literal::Float16(_) => StaticType::F16,
            Literal::Bool(_) => StaticType::Bool,
            Literal::Str(_) => StaticType::Str,
            Literal::StrBytes(_) => StaticType::Str,
            Literal::Char(_) => StaticType::Char,
            Literal::CharMalformed(_) => StaticType::Char,
            Literal::Nothing => StaticType::Nothing,
            Literal::Missing => StaticType::Missing,
            Literal::Undef => StaticType::Any,
            Literal::Module(_) => StaticType::Any, // Module type
            Literal::DataType(_) => StaticType::Any, // Type-object literal (Issue #7761)
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
                // Preserve Complex{Float32} and Complex{Float64} for abs2 return type inference (Issue #3466)
                let normalized = match name.as_str() {
                    "Complex{Float32}" | "ComplexF32" => "Complex{Float32}".to_string(),
                    "Complex{Float64}" | "ComplexF64" => "Complex{Float64}".to_string(),
                    n if n.starts_with("Complex{") => "Complex".to_string(),
                    _ => name.clone(),
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
                // String * only concatenates Str with Str or Char (Issue #3465)
                if matches!(op, BinaryOp::Mul)
                    && matches!(left, StaticType::Str | StaticType::Char)
                    && matches!(right, StaticType::Str | StaticType::Char)
                {
                    StaticType::Str
                } else if matches!((left, right), (StaticType::Bool, StaticType::Bool))
                    && matches!(op, BinaryOp::Add | BinaryOp::Sub)
                {
                    // Julia: `true + true === 2::Int64` and `true - true === 0::Int64`
                    // (Bool `+`/`-` widen to Int), whereas `true * true === true`
                    // (`*` is `&`), which `numeric_promote` already yields via the
                    // shared `promote_type` path (Issue #9351).
                    StaticType::I64
                } else {
                    self.numeric_promote(left, right)
                }
            }
            BinaryOp::Div => {
                // Narrow floats are preserved when neither operand is wider.
                if matches!(left, StaticType::F64) || matches!(right, StaticType::F64) {
                    StaticType::F64
                } else if matches!(left, StaticType::F32) || matches!(right, StaticType::F32) {
                    StaticType::F32
                } else if matches!(left, StaticType::F16) || matches!(right, StaticType::F16) {
                    StaticType::F16
                } else {
                    StaticType::F64 // int/int -> Float64
                }
            }
            BinaryOp::IntDiv => self.integer_type(left, right),
            BinaryOp::Mod => self.integer_type(left, right),
            BinaryOp::Pow => {
                if matches!((left, right), (StaticType::Bool, StaticType::Bool)) {
                    return StaticType::Bool;
                }
                if matches!(left, StaticType::Bool) && right.is_signed() {
                    return StaticType::Bool;
                }
                if (left.is_integer() || matches!(left, StaticType::Struct { .. }))
                    && right.is_integer()
                {
                    left.clone()
                } else if matches!(left, StaticType::F32) && !matches!(right, StaticType::F64) {
                    StaticType::F32
                } else if matches!(left, StaticType::F16)
                    && !matches!(right, StaticType::F64 | StaticType::F32)
                {
                    StaticType::F16
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
            | AotBinOp::NotEgal
            | AotBinOp::Subtype => StaticType::Bool,
            // Logical operators
            AotBinOp::And | AotBinOp::Or => StaticType::Bool,
            // Arithmetic - promote types
            AotBinOp::Add | AotBinOp::Sub | AotBinOp::Mul => {
                // String * only concatenates Str with Str or Char (Issue #3465)
                if matches!(op, AotBinOp::Mul)
                    && matches!(left, StaticType::Str | StaticType::Char)
                    && matches!(right, StaticType::Str | StaticType::Char)
                {
                    StaticType::Str
                } else if matches!((left, right), (StaticType::Bool, StaticType::Bool))
                    && matches!(op, AotBinOp::Add | AotBinOp::Sub)
                {
                    // Bool `+`/`-` widen to Int64 (`true + true === 2`); `*` stays
                    // Bool via `numeric_promote`'s shared `promote_type` path (Issue #9351).
                    StaticType::I64
                } else {
                    self.numeric_promote(left, right)
                }
            }
            AotBinOp::Div => {
                if matches!(left, StaticType::F64) || matches!(right, StaticType::F64) {
                    StaticType::F64
                } else if matches!(left, StaticType::F32) || matches!(right, StaticType::F32) {
                    StaticType::F32
                } else if matches!(left, StaticType::F16) || matches!(right, StaticType::F16) {
                    StaticType::F16
                } else {
                    StaticType::F64
                }
            }
            AotBinOp::IntDiv | AotBinOp::Mod => self.integer_type(left, right),
            AotBinOp::Pow => {
                if matches!((left, right), (StaticType::Bool, StaticType::Bool)) {
                    return StaticType::Bool;
                }
                if matches!(left, StaticType::Bool) && right.is_signed() {
                    return StaticType::Any;
                }
                if (left.is_integer() || matches!(left, StaticType::Struct { .. }))
                    && right.is_integer()
                {
                    left.clone()
                } else if matches!(left, StaticType::F32) && !matches!(right, StaticType::F64) {
                    StaticType::F32
                } else if matches!(left, StaticType::F16)
                    && !matches!(right, StaticType::F64 | StaticType::F32)
                {
                    StaticType::F16
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
        // `string(...)` is variadic and always returns a `String`; the builtin
        // table only registers the 1-arg form, so multi-arg calls would
        // otherwise infer as `Any` and print via `show` (with quotes).
        if name == "string" {
            return StaticType::Str;
        }

        if name == "time_ns" && arg_types.is_empty() {
            return StaticType::I64;
        }

        if matches!(name, "rand" | "randn") {
            return if arg_types.is_empty() {
                StaticType::F64
            } else {
                StaticType::Array {
                    element: Box::new(StaticType::F64),
                    ndims: Some(arg_types.len()),
                }
            };
        }

        if matches!(name, "abs2" | "real" | "imag") && arg_types.len() == 1 {
            if let StaticType::Struct { name, .. } = &arg_types[0] {
                if let Some(element_ty) = StaticType::complex_param_type_from_name(name) {
                    return element_ty;
                }
            }
        }

        if name == "size" && arg_types.len() == 1 {
            if let Some(StaticType::Array { ndims: Some(n), .. }) = arg_types.first() {
                return StaticType::Tuple(vec![StaticType::I64; *n]);
            }
        }

        if name == "collect" && arg_types.len() == 1 {
            match &arg_types[0] {
                StaticType::Array { element, .. }
                | StaticType::Range { element }
                | StaticType::Generator { element }
                | StaticType::Set { element } => {
                    return StaticType::Array {
                        element: element.clone(),
                        ndims: Some(1),
                    };
                }
                StaticType::Dict { key, value } => {
                    return StaticType::Array {
                        element: Box::new(StaticType::Tuple(vec![
                            key.as_ref().clone(),
                            value.as_ref().clone(),
                        ])),
                        ndims: Some(1),
                    };
                }
                _ => {}
            }
        }

        if let Some(dict_ty) = Self::dict_constructor_type(name, arg_types) {
            return dict_ty;
        }

        if let Some(element_ty) = Self::set_constructor_element_type(name, arg_types) {
            return StaticType::Set {
                element: Box::new(element_ty),
            };
        }

        if matches!(Self::normalize_function_value_name(name), "in" | "∈") && arg_types.len() == 2
        {
            return StaticType::Bool;
        }

        if arg_types.len() == 2 {
            let op = match Self::normalize_function_value_name(name) {
                "+" => Some(AotBinOp::Add),
                "-" => Some(AotBinOp::Sub),
                "*" => Some(AotBinOp::Mul),
                "/" => Some(AotBinOp::Div),
                "÷" | "div" => Some(AotBinOp::IntDiv),
                "%" | "mod" => Some(AotBinOp::Mod),
                "==" => Some(AotBinOp::Eq),
                "!=" => Some(AotBinOp::Ne),
                "<" => Some(AotBinOp::Lt),
                ">" => Some(AotBinOp::Gt),
                "<=" => Some(AotBinOp::Le),
                ">=" => Some(AotBinOp::Ge),
                _ => None,
            };
            if let Some(op) = op {
                return self.binop_result_type_static(&op, &arg_types[0], &arg_types[1]);
            }
        }

        // Check builtin signatures (Issue #3541: prefer exact matches first,
        // then fall back to Any-wildcard / Array{Any}-wildcard / etc.).
        if let Some(signatures) = self.builtins.get(name) {
            // First pass: prefer signatures that match without using wildcards.
            for (params, ret) in signatures {
                if params.len() == arg_types.len()
                    && params.iter().zip(arg_types.iter()).all(|(p, a)| p == a)
                {
                    return ret.clone();
                }
            }
            // Second pass: allow wildcard-style compatibility (StaticType::Any
            // and Array{Any}/ndims:None acting as supertypes).
            for (params, ret) in signatures {
                if params.len() == arg_types.len()
                    && params
                        .iter()
                        .zip(arg_types.iter())
                        .all(|(p, a)| static_type_compatible(p, a))
                {
                    return ret.clone();
                }
            }
            // No match found; fall back to Any (Issue #3472)
        }

        // Check if it's a type constructor
        match name {
            "Int" if crate::types::native_int_type_name() == "Int32" => StaticType::I32,
            "Int64" | "Int" => StaticType::I64,
            "UInt" if crate::types::native_uint_type_name() == "UInt32" => StaticType::U32,
            "UInt" => StaticType::U64,
            "String" => StaticType::Str,
            _ => {
                if let Some(kind) = crate::inference_core::PrimitiveNumeric::from_julia_name(name) {
                    StaticType::from_primitive_numeric(kind)
                } else {
                    StaticType::Any
                }
            }
        }
    }

    /// Convert a type name string to StaticType
    /// Used for typed empty arrays like Int64[], Float64[], etc.
    pub(crate) fn type_name_to_static(&self, name: &str) -> StaticType {
        if let Some(projected) = StaticType::from_julia_name_lossy(name) {
            projected
        } else if self.structs.contains_key(name) {
            StaticType::Struct {
                type_id: 0,
                name: name.to_string(),
            }
        } else {
            StaticType::Any
        }
    }

    fn array_constructor_element_and_ndims(&self, args: &[&Expr]) -> (StaticType, usize) {
        let mut element_ty = StaticType::F64;
        let mut dim_args = args;
        if let Some(Expr::Var(type_name, _)) = args.first().copied() {
            if let Some(ty) = StaticType::from_julia_name_lossy(type_name) {
                element_ty = ty;
                dim_args = &args[1..];
            }
        }

        let ndims = if dim_args.len() == 1 {
            if let Expr::TupleLiteral { elements, .. } = dim_args[0] {
                elements.len()
            } else {
                1
            }
        } else {
            dim_args.len()
        };
        (element_ty, ndims)
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
            StaticType::NamedTuple(fields) => {
                if fields.len() == 1 {
                    fields[0].1.clone()
                } else {
                    StaticType::Union {
                        variants: fields.iter().map(|(_, ty)| ty.clone()).collect(),
                    }
                }
            }
            StaticType::Range { element }
            | StaticType::Generator { element }
            | StaticType::Set { element } => (**element).clone(),
            StaticType::Dict { key, value } => {
                StaticType::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()])
            }
            StaticType::Str => StaticType::Char,
            _ => StaticType::Any,
        }
    }

    pub(crate) fn dict_constructor_type(
        name: &str,
        arg_types: &[StaticType],
    ) -> Option<StaticType> {
        let explicit = if name == "Dict" {
            None
        } else if let Some((base, params)) = StaticType::parametric_type_parts(name) {
            if base == "Dict" && params.len() == 2 {
                Some((
                    StaticType::parametric_arg_static_type(params[0])?,
                    StaticType::parametric_arg_static_type(params[1])?,
                ))
            } else {
                None
            }
        } else {
            return None;
        };

        let inferred = if arg_types.is_empty() {
            explicit
        } else {
            let mut key_ty = explicit.as_ref().map(|(key, _)| key.clone());
            let mut value_ty = explicit.as_ref().map(|(_, value)| value.clone());
            for arg in arg_types {
                let StaticType::Tuple(elements) = arg else {
                    return None;
                };
                let [key, value] = elements.as_slice() else {
                    return None;
                };
                if key_ty.is_none() {
                    key_ty = Some(key.clone());
                }
                if value_ty.is_none() {
                    value_ty = Some(value.clone());
                }
                if explicit.is_none() {
                    key_ty = Some(match key_ty.take() {
                        Some(existing) => Self::join_static_pair_type(existing, key.clone()),
                        None => key.clone(),
                    });
                    value_ty = Some(match value_ty.take() {
                        Some(existing) => Self::join_static_pair_type(existing, value.clone()),
                        None => value.clone(),
                    });
                }
            }
            Some((key_ty?, value_ty?))
        }?;

        Some(StaticType::Dict {
            key: Box::new(inferred.0),
            value: Box::new(inferred.1),
        })
    }

    fn join_static_pair_type(left: StaticType, right: StaticType) -> StaticType {
        if left == right {
            left
        } else {
            StaticType::Union {
                variants: vec![left, right],
            }
        }
    }

    pub(crate) fn set_constructor_element_type(
        name: &str,
        arg_types: &[StaticType],
    ) -> Option<StaticType> {
        let explicit = if name == "Set" {
            None
        } else if let Some((base, params)) = StaticType::parametric_type_parts(name) {
            if base == "Set" && params.len() == 1 {
                StaticType::parametric_arg_static_type(params[0])
            } else {
                None
            }
        } else {
            return None;
        };

        match arg_types {
            [] => explicit,
            [arg] => Some(explicit.unwrap_or_else(|| match arg {
                StaticType::Array { element, .. }
                | StaticType::Range { element }
                | StaticType::Generator { element }
                | StaticType::Set { element } => element.as_ref().clone(),
                StaticType::Tuple(elements) if elements.is_empty() => StaticType::Any,
                StaticType::Tuple(elements) if elements.iter().all(|ty| ty == &elements[0]) => {
                    elements[0].clone()
                }
                StaticType::Tuple(elements) => StaticType::Union {
                    variants: elements.clone(),
                },
                _ => StaticType::Any,
            })),
            _ => None,
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
            StaticType::NamedTuple(fields) => {
                if index >= 1 && index <= fields.len() {
                    fields[index - 1].1.clone()
                } else {
                    StaticType::Any
                }
            }
            _ => self.element_type(container),
        }
    }

    fn tuple_tail_type_at(&self, container: &StaticType, start_index: usize) -> StaticType {
        match container {
            StaticType::Tuple(elements)
                if start_index >= 1 && start_index <= elements.len() + 1 =>
            {
                StaticType::Tuple(elements[start_index - 1..].to_vec())
            }
            _ => StaticType::Any,
        }
    }

    /// Get field type of a struct
    pub fn field_type(&self, obj: &StaticType, field: &str) -> StaticType {
        if let StaticType::NamedTuple(fields) = obj {
            return fields
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, ty)| ty.clone())
                .unwrap_or(StaticType::Any);
        }
        if let StaticType::Struct { name, .. } = obj {
            if matches!(field, "re" | "im") {
                if let Some(element_ty) = StaticType::complex_param_type_from_name(name) {
                    return element_ty;
                }
            }
            if let Some(info) = self.structs.get(name) {
                if let Some(ty) = info.get_field_type(field) {
                    return ty.clone();
                }
            }
            if let Some((base, params)) = StaticType::parametric_type_parts(name) {
                if let Some(info) = self.structs.get(base) {
                    let subst = self.parametric_substitution(info, &params);
                    if let Some(ty) = info.get_field_type(field) {
                        return Self::substitute_parametric_field_type(ty, &subst);
                    }
                }
            }
        }
        StaticType::Any
    }

    pub(crate) fn parametric_constructor_info(
        &self,
        name: &str,
        arg_types: &[StaticType],
    ) -> Option<(String, Vec<StaticType>)> {
        if let Some((base, params)) = StaticType::parametric_type_parts(name) {
            let info = self.structs.get(base)?;
            if info.type_params.len() != params.len() || info.fields.len() != arg_types.len() {
                return None;
            }
            let subst = self.parametric_substitution(info, &params);
            if subst.len() != info.type_params.len() {
                return None;
            }
            let field_types = info
                .fields
                .iter()
                .map(|(_, ty)| Self::substitute_parametric_field_type(ty, &subst))
                .collect();
            return Some((name.to_string(), field_types));
        }

        let info = self.structs.get(name)?;
        if info.type_params.is_empty() || info.fields.len() != arg_types.len() {
            return None;
        }

        let mut subst = HashMap::new();
        for ((_, field_ty), arg_ty) in info.fields.iter().zip(arg_types.iter()) {
            Self::bind_parametric_field_type(field_ty, arg_ty, &info.type_params, &mut subst);
        }
        if subst.len() != info.type_params.len() {
            return None;
        }

        let params = info
            .type_params
            .iter()
            .map(|param| subst.get(param).map(StaticType::julia_type_name))
            .collect::<Option<Vec<_>>>()?;
        let instantiated_name = format!("{}{{{}}}", info.name, params.join(", "));
        let field_types = info
            .fields
            .iter()
            .map(|(_, ty)| Self::substitute_parametric_field_type(ty, &subst))
            .collect();

        Some((instantiated_name, field_types))
    }

    fn parametric_substitution(
        &self,
        info: &StructTypeInfo,
        params: &[&str],
    ) -> HashMap<String, StaticType> {
        info.type_params
            .iter()
            .zip(params.iter())
            .filter_map(|(param_name, arg_name)| {
                StaticType::parametric_arg_static_type(arg_name)
                    .map(|arg_ty| (param_name.clone(), arg_ty))
            })
            .collect()
    }

    fn bind_parametric_field_type(
        field_ty: &StaticType,
        arg_ty: &StaticType,
        type_params: &[String],
        subst: &mut HashMap<String, StaticType>,
    ) {
        match field_ty {
            StaticType::Struct { name, .. } if type_params.iter().any(|param| param == name) => {
                subst.entry(name.clone()).or_insert_with(|| arg_ty.clone());
            }
            _ => {}
        }
    }

    fn substitute_parametric_field_type(
        ty: &StaticType,
        subst: &HashMap<String, StaticType>,
    ) -> StaticType {
        match ty {
            StaticType::Struct { name, .. } => {
                if let Some(resolved) = subst.get(name) {
                    return resolved.clone();
                }
                if let Some((base, params)) = StaticType::parametric_type_parts(name) {
                    let rendered_params: Vec<_> = params
                        .iter()
                        .map(|param| {
                            subst
                                .get(*param)
                                .map(StaticType::julia_type_name)
                                .unwrap_or_else(|| (*param).to_string())
                        })
                        .collect();
                    return StaticType::Struct {
                        type_id: 0,
                        name: format!("{}{{{}}}", base, rendered_params.join(", ")),
                    };
                }
                ty.clone()
            }
            StaticType::Array { element, ndims } => StaticType::Array {
                element: Box::new(Self::substitute_parametric_field_type(element, subst)),
                ndims: *ndims,
            },
            StaticType::Tuple(elements) => StaticType::Tuple(
                elements
                    .iter()
                    .map(|element| Self::substitute_parametric_field_type(element, subst))
                    .collect(),
            ),
            StaticType::NamedTuple(fields) => StaticType::NamedTuple(
                fields
                    .iter()
                    .map(|(name, ty)| {
                        (
                            name.clone(),
                            Self::substitute_parametric_field_type(ty, subst),
                        )
                    })
                    .collect(),
            ),
            StaticType::Dict { key, value } => StaticType::Dict {
                key: Box::new(Self::substitute_parametric_field_type(key, subst)),
                value: Box::new(Self::substitute_parametric_field_type(value, subst)),
            },
            StaticType::Range { element } => StaticType::Range {
                element: Box::new(Self::substitute_parametric_field_type(element, subst)),
            },
            StaticType::Generator { element } => StaticType::Generator {
                element: Box::new(Self::substitute_parametric_field_type(element, subst)),
            },
            StaticType::Function { params, ret } => StaticType::Function {
                params: params
                    .iter()
                    .map(|param| Self::substitute_parametric_field_type(param, subst))
                    .collect(),
                ret: Box::new(Self::substitute_parametric_field_type(ret, subst)),
            },
            StaticType::Union { variants } => StaticType::Union {
                variants: variants
                    .iter()
                    .map(|variant| Self::substitute_parametric_field_type(variant, subst))
                    .collect(),
            },
            _ => ty.clone(),
        }
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

/// Returns true if a builtin signature parameter `param` is compatible with
/// the inferred argument type `arg`.
///
/// Issue #3541: Treat `StaticType::Any` and `Array{Any, ndims: None}` as
/// wildcards so signatures registered against generic arrays still match
/// `Array{Int64, _}`, `Array{Float64, _}`, etc.
fn static_type_compatible(param: &StaticType, arg: &StaticType) -> bool {
    if param == arg {
        return true;
    }
    match param {
        // `StaticType::Any` matches anything.
        StaticType::Any => true,
        // `Array{Any, ndims: None}` matches any array; `Array{Any, ndims: n}`
        // matches arrays with the same dimensionality (or unknown ndims).
        StaticType::Array {
            element: p_elem,
            ndims: p_dims,
        } => {
            if let StaticType::Array {
                element: a_elem,
                ndims: a_dims,
            } = arg
            {
                let dims_ok = match (p_dims, a_dims) {
                    (None, _) | (_, None) => true,
                    (Some(p), Some(a)) => p == a,
                };
                dims_ok && static_type_compatible(p_elem, a_elem)
            } else {
                false
            }
        }
        // Tuples are compatible if same arity and each element is compatible.
        StaticType::Tuple(p_elems) => {
            if let StaticType::Tuple(a_elems) = arg {
                p_elems.len() == a_elems.len()
                    && p_elems
                        .iter()
                        .zip(a_elems.iter())
                        .all(|(p, a)| static_type_compatible(p, a))
            } else {
                false
            }
        }
        StaticType::NamedTuple(p_fields) => {
            if let StaticType::NamedTuple(a_fields) = arg {
                p_fields.len() == a_fields.len()
                    && p_fields.iter().zip(a_fields.iter()).all(
                        |((p_name, p_ty), (a_name, a_ty))| {
                            p_name == a_name && static_type_compatible(p_ty, a_ty)
                        },
                    )
            } else {
                false
            }
        }
        // Dict{Any, Any} matches any Dict, similarly for narrower keys/values.
        StaticType::Dict {
            key: p_key,
            value: p_value,
        } => {
            if let StaticType::Dict {
                key: a_key,
                value: a_value,
            } = arg
            {
                static_type_compatible(p_key, a_key) && static_type_compatible(p_value, a_value)
            } else {
                false
            }
        }
        StaticType::Range { element: p_elem } => {
            if let StaticType::Range { element: a_elem } = arg {
                static_type_compatible(p_elem, a_elem)
            } else {
                false
            }
        }
        StaticType::Struct { name: p_name, .. } if p_name == "Complex" => {
            matches!(
                arg,
                StaticType::Struct { name: a_name, .. }
                    if StaticType::complex_param_type_from_name(a_name).is_some()
            )
        }
        // Union{...} parameter: arg compatible if it matches any variant.
        StaticType::Union { variants } => variants.iter().any(|v| static_type_compatible(v, arg)),
        _ => false,
    }
}
