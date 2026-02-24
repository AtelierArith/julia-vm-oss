//! Collection compilation (arrays, dicts, Memory, comprehensions).
//!
//! Handles compilation of:
//! - Array comprehensions
//! - Dict constructors
//! - Memory{T} constructors

// SAFETY: i64→usize casts are from integer literals in the source AST, which are
// non-negative by construction (Memory{T} constructor sizes).
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::ir::core::Expr;
use crate::span::Span;
use crate::vm::{ArrayElementType, Instr, ValueType};

use super::super::{err, CResult, CoreCompiler, TypeExpr};

/// Convert a `TypeExpr` to a Julia type name string, if it resolves to a concrete name.
fn type_expr_to_type_name(te: &TypeExpr) -> Option<String> {
    match te {
        TypeExpr::Concrete(jt) => Some(jt.to_string()),
        TypeExpr::TypeVar(name) => Some(name.clone()),
        _ => None,
    }
}

impl CoreCompiler<'_> {
    pub(in super::super) fn compile_comprehension(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
    ) -> CResult<ValueType> {
        let result_var = self.new_temp("comp_result");
        let iter_var = self.new_temp("comp_iter");
        let idx_var = self.new_temp("comp_idx");
        let len_var = self.new_temp("comp_len");

        // Step 1: Infer iterator element type and register loop variable (Issue #2125)
        // For ranges like 1:5, the element type is I64. For arrays, use the element type.
        let iter_elem_type = match iter {
            Expr::Range { start, .. } => {
                // Infer element type from the range start expression
                self.infer_expr_type(start)
            }
            _ => {
                let iter_ty = self.infer_expr_type(iter);
                match iter_ty {
                    ValueType::ArrayOf(ref elem) => match elem {
                        ArrayElementType::I64 => ValueType::I64,
                        ArrayElementType::F64 => ValueType::F64,
                        ArrayElementType::F32 => ValueType::F32,
                        ArrayElementType::Bool => ValueType::Bool,
                        ArrayElementType::String => ValueType::Str,
                        ArrayElementType::Char => ValueType::Char,
                        _ => ValueType::Any,
                    },
                    _ => ValueType::Any,
                }
            }
        };
        self.locals.insert(var.to_owned(), iter_elem_type);

        // Step 2: Infer body type (now uses properly typed loop variable)
        let body_type = self.infer_expr_type(body);

        // Step 3: Create empty result array with appropriate type (Issue #2125)
        let array_elem_type = match body_type {
            ValueType::Tuple => ArrayElementType::Any,
            ValueType::I64 => ArrayElementType::I64,
            ValueType::F32 => ArrayElementType::F32,
            ValueType::Bool => ArrayElementType::Bool,
            ValueType::Str => ArrayElementType::String,
            ValueType::Char => ArrayElementType::Char,
            _ => ArrayElementType::F64,
        };
        match array_elem_type {
            ArrayElementType::F64 => {
                self.emit(Instr::NewArray(0));
            }
            _ => {
                self.emit(Instr::NewArrayTyped(array_elem_type.clone(), 0));
            }
        }
        self.emit(Instr::FinalizeArray(vec![0]));
        self.locals.insert(result_var.clone(), ValueType::Array);
        self.emit(Instr::StoreArray(result_var.clone()));

        // Compile iterator (can be Array or Range)
        let iter_type = self.compile_expr(iter)?;
        self.locals.insert(iter_var.clone(), iter_type);
        // Use StoreAny/LoadAny to handle both Array and Range iterators
        self.emit(Instr::StoreAny(iter_var.clone()));

        // Get length (via CallBuiltin) - works for both Array and Range
        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
        self.emit(Instr::StoreI64(len_var.clone()));

        // Initialize index
        self.emit(Instr::PushI64(1));
        self.emit(Instr::StoreI64(idx_var.clone()));

        let loop_start = self.here();

        // Check if done
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::LoadI64(len_var.clone()));
        self.emit(Instr::GtI64);
        let j_continue = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        let j_exit = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let continue_label = self.here();
        self.patch_jump(j_continue, continue_label);

        // Get current element (use Any to handle Array, Range, and other types)
        self.emit(Instr::LoadAny(iter_var.clone()));
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::IndexLoad(1));
        self.locals.insert(var.to_owned(), ValueType::Any);
        self.emit(Instr::StoreAny(var.to_owned()));

        // Apply filter if present
        let j_skip = if let Some(filter_expr) = filter {
            self.compile_expr_as(filter_expr, ValueType::Bool)?;
            let j = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            Some(j)
        } else {
            None
        };

        // Compute body and push to result (type-aware, Issue #2125)
        let temp_val = self.new_temp("comp_val");
        match body_type {
            ValueType::Tuple => {
                // Tuple: compile as-is and store as Any
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            ValueType::I64 => {
                self.compile_expr_as(body, ValueType::I64)?;
                self.emit(Instr::StoreI64(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadI64(temp_val.clone()));
            }
            ValueType::Bool => {
                self.compile_expr_as(body, ValueType::Bool)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            ValueType::Str => {
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            ValueType::Char => {
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            _ => {
                // Default: F64
                self.compile_expr_as(body, ValueType::F64)?;
                self.emit(Instr::StoreF64(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadF64(temp_val.clone()));
            }
        }
        self.emit(Instr::ArrayPush);
        self.emit(Instr::StoreArray(result_var.clone()));

        // Skip label
        if let Some(j) = j_skip {
            let skip_label = self.here();
            self.patch_jump(j, skip_label);
        }

        // Increment index
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::PushI64(1));
        self.emit(Instr::AddI64);
        self.emit(Instr::StoreI64(idx_var.clone()));

        self.emit(Instr::Jump(loop_start));

        let exit_label = self.here();
        self.patch_jump(j_exit, exit_label);

        // Load result and return appropriate type (Issue #2125)
        self.emit(Instr::LoadArray(result_var));
        Ok(ValueType::ArrayOf(array_elem_type))
    }

    /// Compile a multi-variable comprehension: [expr for var1 in iter1, var2 in iter2, ...]
    /// Produces a flat array via nested loops (cartesian product). Issue #2143.
    pub(in super::super) fn compile_multi_comprehension(
        &mut self,
        body: &Expr,
        iterations: &[(String, Expr)],
        filter: Option<&Expr>,
    ) -> CResult<ValueType> {
        let result_var = self.new_temp("mcomp_result");

        // Register all loop variables for type inference
        for (var, iter) in iterations {
            let iter_elem_type = match iter {
                Expr::Range { start, .. } => self.infer_expr_type(start),
                _ => {
                    let iter_ty = self.infer_expr_type(iter);
                    match iter_ty {
                        ValueType::ArrayOf(ref elem) => match elem {
                            ArrayElementType::I64 => ValueType::I64,
                            ArrayElementType::F64 => ValueType::F64,
                            ArrayElementType::F32 => ValueType::F32,
                            ArrayElementType::Bool => ValueType::Bool,
                            ArrayElementType::String => ValueType::Str,
                            ArrayElementType::Char => ValueType::Char,
                            _ => ValueType::Any,
                        },
                        _ => ValueType::Any,
                    }
                }
            };
            self.locals.insert(var.clone(), iter_elem_type);
        }

        // Infer body type with all loop variables registered
        let body_type = self.infer_expr_type(body);

        // Create empty result array with appropriate type
        let array_elem_type = match body_type {
            ValueType::Tuple => ArrayElementType::Any,
            ValueType::I64 => ArrayElementType::I64,
            ValueType::F32 => ArrayElementType::F32,
            ValueType::Bool => ArrayElementType::Bool,
            ValueType::Str => ArrayElementType::String,
            ValueType::Char => ArrayElementType::Char,
            _ => ArrayElementType::F64,
        };
        match array_elem_type {
            ArrayElementType::F64 => {
                self.emit(Instr::NewArray(0));
            }
            _ => {
                self.emit(Instr::NewArrayTyped(array_elem_type.clone(), 0));
            }
        }
        self.emit(Instr::FinalizeArray(vec![0]));
        self.locals.insert(result_var.clone(), ValueType::Array);
        self.emit(Instr::StoreArray(result_var.clone()));

        // For each iteration clause, compile the iterator and prepare loop vars
        let n = iterations.len();
        let mut iter_vars = Vec::with_capacity(n);
        let mut idx_vars = Vec::with_capacity(n);
        let mut len_vars = Vec::with_capacity(n);

        for (_, iter_expr) in iterations {
            let iter_var = self.new_temp("mcomp_iter");
            let idx_var = self.new_temp("mcomp_idx");
            let len_var = self.new_temp("mcomp_len");

            // Compile and store iterator
            self.compile_expr(iter_expr)?;
            self.locals.insert(iter_var.clone(), ValueType::Any);
            self.emit(Instr::StoreAny(iter_var.clone()));

            // Get length
            self.emit(Instr::LoadAny(iter_var.clone()));
            self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
            self.emit(Instr::StoreI64(len_var.clone()));

            // Initialize index to 1
            self.emit(Instr::PushI64(1));
            self.emit(Instr::StoreI64(idx_var.clone()));

            iter_vars.push(iter_var);
            idx_vars.push(idx_var);
            len_vars.push(len_var);
        }

        // Generate nested loops: outermost = LAST iteration (column-major order)
        // Julia: [f(i,j) for i in 1:3, j in 1:3] iterates as (1,1),(2,1),(3,1),(1,2),...
        let mut loop_starts = Vec::with_capacity(n);
        let mut j_exits = Vec::with_capacity(n);

        for ri in (0..n).rev() {
            let loop_start = self.here();
            loop_starts.push(loop_start);

            // Check if done: idx > len
            self.emit(Instr::LoadI64(idx_vars[ri].clone()));
            self.emit(Instr::LoadI64(len_vars[ri].clone()));
            self.emit(Instr::GtI64);
            let j_continue = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            let j_exit = self.here();
            self.emit(Instr::Jump(usize::MAX));
            j_exits.push(j_exit);

            let continue_label = self.here();
            self.patch_jump(j_continue, continue_label);
        }

        // At the innermost level: bind all loop variables to current elements
        for (i, (var, _)) in iterations.iter().enumerate() {
            self.emit(Instr::LoadAny(iter_vars[i].clone()));
            self.emit(Instr::LoadI64(idx_vars[i].clone()));
            self.emit(Instr::IndexLoad(1));
            self.locals.insert(var.clone(), ValueType::Any);
            self.emit(Instr::StoreAny(var.clone()));
        }

        // Apply filter if present
        let j_skip = if let Some(filter_expr) = filter {
            self.compile_expr_as(filter_expr, ValueType::Bool)?;
            let j = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            Some(j)
        } else {
            None
        };

        // Compute body and push to result
        let temp_val = self.new_temp("mcomp_val");
        match body_type {
            ValueType::Tuple => {
                self.compile_expr(body)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            ValueType::I64 => {
                self.compile_expr_as(body, ValueType::I64)?;
                self.emit(Instr::StoreI64(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadI64(temp_val.clone()));
            }
            ValueType::Bool => {
                self.compile_expr_as(body, ValueType::Bool)?;
                self.emit(Instr::StoreAny(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadAny(temp_val.clone()));
            }
            _ => {
                self.compile_expr_as(body, ValueType::F64)?;
                self.emit(Instr::StoreF64(temp_val.clone()));
                self.emit(Instr::LoadArray(result_var.clone()));
                self.emit(Instr::LoadF64(temp_val.clone()));
            }
        }
        self.emit(Instr::ArrayPush);
        self.emit(Instr::StoreArray(result_var.clone()));

        // Skip label for filter
        if let Some(j) = j_skip {
            let skip_label = self.here();
            self.patch_jump(j, skip_label);
        }

        // Close nested loops: innermost first (loop_vars[0]), outermost last
        // loop_starts/j_exits were pushed in reverse: index 0 = outermost, n-1 = innermost
        for close_i in 0..n {
            let lv_idx = close_i;
            let ls_idx = n - 1 - close_i;

            // Increment index for this loop level
            self.emit(Instr::LoadI64(idx_vars[lv_idx].clone()));
            self.emit(Instr::PushI64(1));
            self.emit(Instr::AddI64);
            self.emit(Instr::StoreI64(idx_vars[lv_idx].clone()));

            // Jump back to this loop's start
            self.emit(Instr::Jump(loop_starts[ls_idx]));

            // Patch exit jump
            let exit_label = self.here();
            self.patch_jump(j_exits[ls_idx], exit_label);

            // Reset inner loop indices when outer loop iterates
            if close_i < n - 1 {
                self.emit(Instr::PushI64(1));
                self.emit(Instr::StoreI64(idx_vars[lv_idx].clone()));
            }
        }

        // Load result
        self.emit(Instr::LoadArray(result_var));
        Ok(ValueType::ArrayOf(array_elem_type))
    }

    /// Compile a generator expression: (expr for var in iter) or (expr for var in iter if cond)
    /// Creates a Value::Generator that wraps the underlying iterator and function.
    ///
    /// For generators where the body is a simple function call like `f(x)`,
    /// we try to resolve the function and create a true lazy generator.
    /// Otherwise, we fall back to eager evaluation wrapped in a Generator type.
    pub(in super::super) fn compile_generator_expr(
        &mut self,
        body: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
        _span: Span,
    ) -> CResult<ValueType> {
        // Check if body is a simple function call on the loop variable
        // e.g., `(f(x) for x in 1:3)` or `(square(x) for x in arr)`
        if filter.is_none() {
            if let Some(func_name) = self.extract_simple_function_call(body, var) {
                // Try to resolve the function
                if let Some(table) = self.method_tables.get(&func_name) {
                    if let Some(method) = table.methods.first() {
                        let func_index = method.global_index;
                        // Compile iterator
                        self.compile_expr(iter)?;
                        // Create lazy generator with MakeGenerator
                        self.emit(Instr::MakeGenerator(func_index));
                        return Ok(ValueType::Generator);
                    }
                }
            }
        }

        // Fall back: compile as comprehension but wrap result in Generator
        // This provides correct typeof() behavior while being eager
        let result_var = self.new_temp("gen_result");

        // Compile as comprehension to get the array result
        let arr_type = self.compile_comprehension(body, var, iter, filter)?;
        let _ = arr_type; // Ignore the array type, we're wrapping it

        // Store the array temporarily
        self.emit(Instr::StoreArray(result_var.clone()));

        // Load and wrap in Generator
        self.emit(Instr::LoadArray(result_var));
        self.emit(Instr::WrapInGenerator);

        Ok(ValueType::Generator)
    }

    /// Extract the function name if body is a simple call like `f(var)` where var is the loop variable.
    fn extract_simple_function_call(&self, body: &Expr, var: &str) -> Option<String> {
        match body {
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                ..
            } => {
                // Must have exactly one argument, no kwargs, no splats
                if args.len() == 1 && kwargs.is_empty() && splat_mask.iter().all(|&s| !s) {
                    // The argument must be the loop variable
                    if let Expr::Var(arg_name, _) = &args[0] {
                        if arg_name == var {
                            return Some(function.clone());
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Compile a Dict{K,V}() constructor with explicit type parameters.
    /// Emits `NewDictTyped(K, V)` when both type args resolve to concrete type names.
    /// Falls back to untyped `compile_dict_constructor` otherwise.
    pub(in super::super) fn compile_dict_constructor_typed(
        &mut self,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> CResult<ValueType> {
        if type_args.len() == 2 {
            let key_name = type_expr_to_type_name(&type_args[0]);
            let val_name = type_expr_to_type_name(&type_args[1]);
            if let (Some(k), Some(v)) = (key_name, val_name) {
                self.emit(Instr::NewDictTyped(k, v));

                for arg in args {
                    match arg {
                        Expr::Pair { key, value, .. } => {
                            self.compile_expr(key)?;
                            self.compile_expr(value)?;
                            self.emit(Instr::DictSet);
                        }
                        _ => {
                            return err("Dict constructor arguments must be key => value pairs");
                        }
                    }
                }
                return Ok(ValueType::Dict);
            }
        }
        // Fallback to untyped constructor
        self.compile_dict_constructor(args)
    }

    /// Compile a Dict constructor call: Dict(), Dict{K,V}(), Dict("a" => 1, "b" => 2),
    /// or Dict comprehension: Dict(k => v for (k, v) in pairs)
    pub(in super::super) fn compile_dict_constructor(
        &mut self,
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Check for Dict comprehension: Dict(k => v for ...)
        // This is a single Comprehension or Generator argument whose body is a Pair
        if args.len() == 1 {
            if let Expr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            } = &args[0]
            {
                if let Expr::Pair { key, value, .. } = body.as_ref() {
                    return self.compile_dict_comprehension(
                        key,
                        value,
                        var,
                        iter,
                        filter.as_deref(),
                    );
                }
            }
            // Also handle Generator syntax (which is now distinct from Comprehension)
            if let Expr::Generator {
                body,
                var,
                iter,
                filter,
                ..
            } = &args[0]
            {
                if let Expr::Pair { key, value, .. } = body.as_ref() {
                    return self.compile_dict_comprehension(
                        key,
                        value,
                        var,
                        iter,
                        filter.as_deref(),
                    );
                }
            }
        }

        // Create a new empty dict
        self.emit(Instr::NewDict);

        // If there are arguments, they should be Pair expressions (key => value)
        for arg in args {
            match arg {
                Expr::Pair { key, value, .. } => {
                    self.compile_expr(key)?;
                    self.compile_expr(value)?;
                    self.emit(Instr::DictSet);
                }
                _ => {
                    return err("Dict constructor arguments must be key => value pairs");
                }
            }
        }

        Ok(ValueType::Dict)
    }

    /// Compile a Dict comprehension: Dict(key_expr => value_expr for var in iter [if cond])
    pub(in super::super) fn compile_dict_comprehension(
        &mut self,
        key_expr: &Expr,
        value_expr: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
    ) -> CResult<ValueType> {
        let result_var = self.new_temp("dict_result");
        let iter_var = self.new_temp("dict_iter");
        let idx_var = self.new_temp("dict_idx");
        let len_var = self.new_temp("dict_len");

        // Step 1: Register loop variable for type inference
        self.locals.insert(var.to_owned(), ValueType::Any);

        // Step 2: Create empty result Dict
        self.emit(Instr::NewDict);
        self.locals.insert(result_var.clone(), ValueType::Dict);
        self.emit(Instr::StoreDict(result_var.clone()));

        // Compile iterator
        self.compile_expr(iter)?;
        self.locals.insert(iter_var.clone(), ValueType::Array);
        self.emit(Instr::StoreArray(iter_var.clone()));

        // Get length (via CallBuiltin)
        self.emit(Instr::LoadArray(iter_var.clone()));
        self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
        self.emit(Instr::StoreI64(len_var.clone()));

        // Initialize index
        self.emit(Instr::PushI64(1));
        self.emit(Instr::StoreI64(idx_var.clone()));

        let loop_start = self.here();

        // Check if done
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::LoadI64(len_var.clone()));
        self.emit(Instr::GtI64);
        let j_continue = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        let j_exit = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let continue_label = self.here();
        self.patch_jump(j_continue, continue_label);

        // Get current element (use Any to handle structs and other types)
        self.emit(Instr::LoadArray(iter_var.clone()));
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::IndexLoad(1));
        self.locals.insert(var.to_owned(), ValueType::Any);
        self.emit(Instr::StoreAny(var.to_owned()));

        // Apply filter if present
        let j_skip = if let Some(filter_expr) = filter {
            self.compile_expr_as(filter_expr, ValueType::Bool)?;
            let j = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            Some(j)
        } else {
            None
        };

        // Load result dict, compute key and value, set entry
        self.emit(Instr::LoadDict(result_var.clone()));
        self.compile_expr(key_expr)?;
        self.compile_expr(value_expr)?;
        self.emit(Instr::DictSet);
        self.emit(Instr::StoreDict(result_var.clone()));

        // Skip label
        if let Some(j) = j_skip {
            let skip_label = self.here();
            self.patch_jump(j, skip_label);
        }

        // Increment index
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::PushI64(1));
        self.emit(Instr::AddI64);
        self.emit(Instr::StoreI64(idx_var.clone()));

        self.emit(Instr::Jump(loop_start));

        let exit_label = self.here();
        self.patch_jump(j_exit, exit_label);

        // Load result and return
        self.emit(Instr::LoadDict(result_var));
        Ok(ValueType::Dict)
    }

    /// Compile a Set constructor call: Set(), Set{T}(), Set([1, 2, 3]),
    /// or Set comprehension: Set(x for x in arr [if cond])
    pub(in super::super) fn compile_set_constructor(
        &mut self,
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Check for Set comprehension: Set(x for ...)
        // This is a single Comprehension or Generator argument
        if args.len() == 1 {
            if let Expr::Comprehension {
                body,
                var,
                iter,
                filter,
                ..
            } = &args[0]
            {
                return self.compile_set_comprehension(body, var, iter, filter.as_deref());
            }
            // Also handle Generator syntax (which is now distinct from Comprehension)
            if let Expr::Generator {
                body,
                var,
                iter,
                filter,
                ..
            } = &args[0]
            {
                return self.compile_set_comprehension(body, var, iter, filter.as_deref());
            }
        }

        // Create a new empty set
        self.emit(Instr::NewSet);

        // If there is a single array argument, iterate and add elements
        if args.len() == 1 {
            // Compile the argument (should be an array)
            self.compile_expr(&args[0])?;

            // Store the array
            let arr_var = self.new_temp("set_arr");
            self.emit(Instr::StoreArray(arr_var.clone()));

            // Get length
            let len_var = self.new_temp("set_len");
            self.emit(Instr::LoadArray(arr_var.clone()));
            self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
            self.emit(Instr::StoreI64(len_var.clone()));

            // Store the set
            let set_var = self.new_temp("set_result");
            self.emit(Instr::StoreSet(set_var.clone()));

            // Initialize index
            let idx_var = self.new_temp("set_idx");
            self.emit(Instr::PushI64(1));
            self.emit(Instr::StoreI64(idx_var.clone()));

            let loop_start = self.here();

            // Check if done
            self.emit(Instr::LoadI64(idx_var.clone()));
            self.emit(Instr::LoadI64(len_var.clone()));
            self.emit(Instr::GtI64);
            let j_continue = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            let j_exit = self.here();
            self.emit(Instr::Jump(usize::MAX));

            let continue_label = self.here();
            self.patch_jump(j_continue, continue_label);

            // Load set, get element, add to set
            self.emit(Instr::LoadSet(set_var.clone()));
            self.emit(Instr::LoadArray(arr_var.clone()));
            self.emit(Instr::LoadI64(idx_var.clone()));
            self.emit(Instr::IndexLoad(1));
            self.emit(Instr::SetAdd);
            self.emit(Instr::StoreSet(set_var.clone()));

            // Increment index
            self.emit(Instr::LoadI64(idx_var.clone()));
            self.emit(Instr::PushI64(1));
            self.emit(Instr::AddI64);
            self.emit(Instr::StoreI64(idx_var.clone()));

            self.emit(Instr::Jump(loop_start));

            let exit_label = self.here();
            self.patch_jump(j_exit, exit_label);

            // Load result
            self.emit(Instr::LoadSet(set_var));
        }

        Ok(ValueType::Set)
    }

    /// Compile a Set comprehension: Set(expr for var in iter [if cond])
    pub(in super::super) fn compile_set_comprehension(
        &mut self,
        body_expr: &Expr,
        var: &str,
        iter: &Expr,
        filter: Option<&Expr>,
    ) -> CResult<ValueType> {
        let result_var = self.new_temp("set_result");
        let iter_var = self.new_temp("set_iter");
        let idx_var = self.new_temp("set_idx");
        let len_var = self.new_temp("set_len");

        // Step 1: Register loop variable for type inference
        self.locals.insert(var.to_owned(), ValueType::Any);

        // Step 2: Create empty result Set
        self.emit(Instr::NewSet);
        self.locals.insert(result_var.clone(), ValueType::Set);
        self.emit(Instr::StoreSet(result_var.clone()));

        // Compile iterator
        self.compile_expr(iter)?;
        self.locals.insert(iter_var.clone(), ValueType::Array);
        self.emit(Instr::StoreArray(iter_var.clone()));

        // Get length (via CallBuiltin)
        self.emit(Instr::LoadArray(iter_var.clone()));
        self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
        self.emit(Instr::StoreI64(len_var.clone()));

        // Initialize index
        self.emit(Instr::PushI64(1));
        self.emit(Instr::StoreI64(idx_var.clone()));

        let loop_start = self.here();

        // Check if done
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::LoadI64(len_var.clone()));
        self.emit(Instr::GtI64);
        let j_continue = self.here();
        self.emit(Instr::JumpIfZero(usize::MAX));
        let j_exit = self.here();
        self.emit(Instr::Jump(usize::MAX));

        let continue_label = self.here();
        self.patch_jump(j_continue, continue_label);

        // Get current element (use Any to handle structs and other types)
        self.emit(Instr::LoadArray(iter_var.clone()));
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::IndexLoad(1));
        self.locals.insert(var.to_owned(), ValueType::Any);
        self.emit(Instr::StoreAny(var.to_owned()));

        // Apply filter if present
        let j_skip = if let Some(filter_expr) = filter {
            self.compile_expr_as(filter_expr, ValueType::Bool)?;
            let j = self.here();
            self.emit(Instr::JumpIfZero(usize::MAX));
            Some(j)
        } else {
            None
        };

        // Load result set, compute element, add to set
        self.emit(Instr::LoadSet(result_var.clone()));
        self.compile_expr(body_expr)?;
        self.emit(Instr::SetAdd);
        self.emit(Instr::StoreSet(result_var.clone()));

        // Skip label
        if let Some(j) = j_skip {
            let skip_label = self.here();
            self.patch_jump(j, skip_label);
        }

        // Increment index
        self.emit(Instr::LoadI64(idx_var.clone()));
        self.emit(Instr::PushI64(1));
        self.emit(Instr::AddI64);
        self.emit(Instr::StoreI64(idx_var.clone()));

        self.emit(Instr::Jump(loop_start));

        let exit_label = self.here();
        self.patch_jump(j_exit, exit_label);

        // Load result and return
        self.emit(Instr::LoadSet(result_var));
        Ok(ValueType::Set)
    }

    /// Compile an Array/Vector constructor call: Array{Int64}(), Vector{Float64}(), etc.
    /// Supports:
    /// - Empty arrays: Vector{Int64}(), Array{Float64}()
    /// - Array conversion: Vector{Int64}(existing_array)
    /// - Uninitialized arrays: Vector{Float64}(undef, n), Array{Int64}(undef, m, n)
    pub(in super::super) fn compile_array_constructor(
        &mut self,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Determine the element type from type_args
        let elem_type = if type_args.is_empty() {
            ArrayElementType::Any
        } else {
            match &type_args[0] {
                TypeExpr::Concrete(jt) => {
                    use crate::types::JuliaType;
                    match jt {
                        JuliaType::Int64 | JuliaType::Integer => ArrayElementType::I64,
                        JuliaType::Int8 => ArrayElementType::I8,
                        JuliaType::Int16 => ArrayElementType::I16,
                        JuliaType::Int32 => ArrayElementType::I32,
                        JuliaType::UInt8 => ArrayElementType::U8,
                        JuliaType::UInt16 => ArrayElementType::U16,
                        JuliaType::UInt32 => ArrayElementType::U32,
                        JuliaType::UInt64 => ArrayElementType::U64,
                        JuliaType::Float64 | JuliaType::AbstractFloat => ArrayElementType::F64,
                        JuliaType::Float32 => ArrayElementType::F32,
                        JuliaType::Bool => ArrayElementType::Bool,
                        JuliaType::Char => ArrayElementType::Char,
                        JuliaType::String => ArrayElementType::String,
                        _ => ArrayElementType::Any,
                    }
                }
                TypeExpr::TypeVar(name) => {
                    // Try to resolve known type names (Issue #2218: support all numeric types)
                    match name.as_str() {
                        "Int64" | "Int" => ArrayElementType::I64,
                        "Int8" => ArrayElementType::I8,
                        "Int16" => ArrayElementType::I16,
                        "Int32" => ArrayElementType::I32,
                        "UInt8" => ArrayElementType::U8,
                        "UInt16" => ArrayElementType::U16,
                        "UInt32" => ArrayElementType::U32,
                        "UInt64" => ArrayElementType::U64,
                        "Float64" => ArrayElementType::F64,
                        "Float32" => ArrayElementType::F32,
                        "Float16" => ArrayElementType::Any, // No native F16 ArrayData; store as Any
                        "Bool" => ArrayElementType::Bool,
                        "Char" => ArrayElementType::Char,
                        "String" => ArrayElementType::String,
                        "ComplexF64" => ArrayElementType::ComplexF64,
                        _ => ArrayElementType::Any,
                    }
                }
                TypeExpr::Parameterized { base, params } => {
                    // Handle Complex{Float64}, Complex{Int64}, etc.
                    match base.as_str() {
                        "Complex" if !params.is_empty() => match &params[0] {
                            TypeExpr::TypeVar(inner) => match inner.as_str() {
                                "Float64" => ArrayElementType::ComplexF64,
                                "Float32" => ArrayElementType::ComplexF32,
                                _ => ArrayElementType::Any,
                            },
                            TypeExpr::Concrete(jt) => {
                                use crate::types::JuliaType;
                                match jt {
                                    JuliaType::Float64 => ArrayElementType::ComplexF64,
                                    JuliaType::Float32 => ArrayElementType::ComplexF32,
                                    _ => ArrayElementType::Any,
                                }
                            }
                            _ => ArrayElementType::Any,
                        },
                        _ => ArrayElementType::Any,
                    }
                }
                TypeExpr::RuntimeExpr(_) => ArrayElementType::Any, // Runtime expressions can't be resolved at compile time
            }
        };

        if args.is_empty() {
            // Create an empty array with the specified element type
            self.emit(Instr::NewArrayTyped(elem_type.clone(), 0));
            self.emit(Instr::FinalizeArrayTyped(vec![0]));
            Ok(ValueType::ArrayOf(elem_type))
        } else if args.len() == 1 {
            // Array{T}(arr) - copy/convert an existing array
            // For now, compile the argument and return it (shallow copy behavior)
            self.compile_expr(&args[0])?;
            Ok(ValueType::ArrayOf(elem_type))
        } else {
            // Check if first argument is `undef` - this is the Array{T}(undef, dims...) pattern
            let is_undef = matches!(&args[0], Expr::Var(name, _) if name == "undef");

            if is_undef {
                // Array{T}(undef, dims...) - allocate uninitialized array with given dimensions
                // Compile dimension arguments (skip the `undef` first argument)
                let dim_count = args.len() - 1;
                for dim_arg in &args[1..] {
                    self.compile_expr_as(dim_arg, ValueType::I64)?;
                }

                // Use generic AllocUndefTyped instruction for all element types (Issue #2218)
                self.emit(Instr::AllocUndefTyped(elem_type.clone(), dim_count));
                Ok(ValueType::ArrayOf(elem_type))
            } else {
                err("Array/Vector constructor with multiple arguments not yet supported (expected undef as first argument)")
            }
        }
    }

    /// Compile a Memory{T}(n) constructor call.
    /// Supports:
    /// - Empty memory: Memory{Int64}() → zero-length
    /// - Sized memory: Memory{Float64}(n) → n-element undef-initialized
    /// - With undef: Memory{Int64}(undef, n) → n-element undef-initialized
    pub(in super::super) fn compile_memory_constructor(
        &mut self,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Determine the element type from type_args (same logic as Array)
        let elem_type = if type_args.is_empty() {
            ArrayElementType::Any
        } else {
            match &type_args[0] {
                TypeExpr::Concrete(jt) => {
                    use crate::types::JuliaType;
                    match jt {
                        JuliaType::Int64 | JuliaType::Integer => ArrayElementType::I64,
                        JuliaType::Int8 => ArrayElementType::I8,
                        JuliaType::Int16 => ArrayElementType::I16,
                        JuliaType::Int32 => ArrayElementType::I32,
                        JuliaType::UInt8 => ArrayElementType::U8,
                        JuliaType::UInt16 => ArrayElementType::U16,
                        JuliaType::UInt32 => ArrayElementType::U32,
                        JuliaType::UInt64 => ArrayElementType::U64,
                        JuliaType::Float64 | JuliaType::AbstractFloat => ArrayElementType::F64,
                        JuliaType::Float32 => ArrayElementType::F32,
                        JuliaType::Bool => ArrayElementType::Bool,
                        JuliaType::Char => ArrayElementType::Char,
                        JuliaType::String => ArrayElementType::String,
                        _ => ArrayElementType::Any,
                    }
                }
                TypeExpr::TypeVar(name) => match name.as_str() {
                    "Int64" | "Int" => ArrayElementType::I64,
                    "Int8" => ArrayElementType::I8,
                    "Int16" => ArrayElementType::I16,
                    "Int32" => ArrayElementType::I32,
                    "UInt8" => ArrayElementType::U8,
                    "UInt16" => ArrayElementType::U16,
                    "UInt32" => ArrayElementType::U32,
                    "UInt64" => ArrayElementType::U64,
                    "Float64" => ArrayElementType::F64,
                    "Float32" => ArrayElementType::F32,
                    "Bool" => ArrayElementType::Bool,
                    "Char" => ArrayElementType::Char,
                    "String" => ArrayElementType::String,
                    _ => ArrayElementType::Any,
                },
                _ => ArrayElementType::Any,
            }
        };

        let result_type = ValueType::MemoryOf(elem_type.clone());

        if args.is_empty() {
            // Memory{T}() → empty memory with zero length
            self.emit(Instr::NewMemory(elem_type.clone(), 0));
            Ok(result_type)
        } else if args.len() == 1 {
            // Memory{T}(n) → undef-initialized memory with n elements
            // Check if the arg is `undef` (Memory{T}(undef) is a Julia pattern but means 0-length)
            let is_undef = matches!(&args[0], Expr::Var(name, _) if name == "undef");
            if is_undef {
                self.emit(Instr::NewMemory(elem_type.clone(), 0));
            } else {
                // Compile the size argument and use dynamic NewMemory
                // For now, if it's a literal integer, use static size
                if let Expr::Literal(crate::ir::core::Literal::Int(n), _) = &args[0] {
                    self.emit(Instr::NewMemory(elem_type.clone(), *n as usize));
                } else {
                    // Dynamic size: compile size expression, then emit NewMemoryDynamic
                    self.compile_expr_as(&args[0], ValueType::I64)?;
                    self.emit(Instr::NewMemoryDynamic(elem_type.clone()));
                }
            }
            Ok(result_type)
        } else if args.len() == 2 {
            // Memory{T}(undef, n) → undef-initialized memory with n elements
            let is_undef = matches!(&args[0], Expr::Var(name, _) if name == "undef");
            if is_undef {
                if let Expr::Literal(crate::ir::core::Literal::Int(n), _) = &args[1] {
                    self.emit(Instr::NewMemory(elem_type.clone(), *n as usize));
                } else {
                    self.compile_expr_as(&args[1], ValueType::I64)?;
                    self.emit(Instr::NewMemoryDynamic(elem_type.clone()));
                }
                Ok(result_type)
            } else {
                err("Memory{T} constructor with 2 arguments requires `undef` as first argument")
            }
        } else {
            err("Memory{T} constructor takes at most 2 arguments")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JuliaType;

    // ── type_expr_to_type_name ────────────────────────────────────────────────

    #[test]
    fn test_concrete_julia_type_returns_type_string() {
        let te = TypeExpr::Concrete(JuliaType::Int64);
        let result = type_expr_to_type_name(&te);
        assert_eq!(result, Some("Int64".to_string()));
    }

    #[test]
    fn test_concrete_float64_returns_float64_string() {
        let te = TypeExpr::Concrete(JuliaType::Float64);
        let result = type_expr_to_type_name(&te);
        assert_eq!(result, Some("Float64".to_string()));
    }

    #[test]
    fn test_typevar_returns_name() {
        let te = TypeExpr::TypeVar("T".to_string());
        let result = type_expr_to_type_name(&te);
        assert_eq!(result, Some("T".to_string()));
    }

    #[test]
    fn test_parameterized_returns_none() {
        let te = TypeExpr::Parameterized {
            base: "Vector".to_string(),
            params: vec![TypeExpr::Concrete(JuliaType::Int64)],
        };
        let result = type_expr_to_type_name(&te);
        assert!(result.is_none(), "Expected None for Parameterized, got {:?}", result);
    }

    #[test]
    fn test_runtime_expr_returns_none() {
        let te = TypeExpr::RuntimeExpr("some_expr".to_string());
        let result = type_expr_to_type_name(&te);
        assert!(result.is_none(), "Expected None for RuntimeExpr, got {:?}", result);
    }
}
