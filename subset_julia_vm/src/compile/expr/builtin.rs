//! Builtin function compilation.
//!
//! Handles compilation of Julia builtin functions and operators.
//! This module is organized by function category:
//! - I/O functions (println, print, error)
//! - Math functions (sqrt, sin, cos, exp, log, etc.)
//! - Array functions (zeros, ones, length, sum, etc.)
//! - String functions (uppercase, lowercase, etc.)
//! - Type functions (typeof, isa, convert, etc.)

use crate::builtins::BuiltinId;
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::types::JuliaType;
use crate::vm::value::ArrayElementType;
use crate::vm::{Instr, ValueType};

use super::super::{err, is_builtin_type_name, CResult, CompileError, CoreCompiler};
use crate::compile::inference::promote_numeric_value_types;

impl CoreCompiler<'_> {
    /// Extract type name from an expression if it's a type identifier for zeros/ones
    fn extract_array_type_name<'a>(&self, expr: &'a Expr) -> Option<&'a str> {
        match expr {
            Expr::Var(name, _) => {
                if is_builtin_type_name(name) || self.abstract_type_names.contains(name) {
                    Some(name)
                } else {
                    None
                }
            }
            // Handle Complex{Float64} as Call expression (parametric type instantiation)
            Expr::Call {
                function,
                args: type_args,
                ..
            } => {
                if function == "Complex" && type_args.len() == 1 {
                    if let Expr::Var(inner_name, _) = &type_args[0] {
                        if inner_name == "Float64" {
                            return Some("Complex{Float64}");
                        }
                    }
                }
                None
            }
            // Handle parametric types lowered as TypeOf builtin (e.g., Complex{Float64})
            Expr::Builtin {
                name: BuiltinOp::TypeOf,
                args: type_args,
                ..
            } => {
                if type_args.len() == 1 {
                    if let Expr::Literal(Literal::Str(type_name), _) = &type_args[0] {
                        if type_name == "Complex{Float64}" {
                            return Some("Complex{Float64}");
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub(in super::super) fn compile_builtin_call(
        &mut self,
        name: &str,
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Try delegated modules first
        if let Some(result) = self.compile_builtin_io(name, args)? {
            return Ok(result);
        }
        if let Some(result) = self.compile_builtin_math(name, args)? {
            return Ok(result);
        }
        if let Some(result) = self.compile_builtin_string(name, args)? {
            return Ok(result);
        }
        if let Some(result) = self.compile_builtin_types(name, args)? {
            return Ok(result);
        }
        if let Some(result) = self.compile_builtin_array(name, args)? {
            return Ok(result);
        }
        if let Some(result) = self.compile_builtin_hof(name, args)? {
            return Ok(result);
        }
        if let Some(result) = self.compile_builtin_set(name, args)? {
            return Ok(result);
        }

        match name {
            // I/O functions delegated to builtin_io.rs
            "println" | "print" | "error" | "throw" | "IOBuffer" | "take!" | "takestring!"
            | "write" => err(format!(
                "I/O function {} should be handled by builtin_io",
                name
            )),
            // Math functions delegated to builtin_math.rs
            "rand" | "sqrt" | "sdiv_int" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
            | "exp" | "log" | "floor" | "ceil" | "round" | "trunc" | "nextfloat" | "prevfloat"
            | "sleep" | "count_ones" | "count_zeros" | "leading_zeros" | "leading_ones"
            | "trailing_zeros" | "trailing_ones" | "bitreverse" | "bitrotate" | "bswap"
            | "exponent" | "significand" | "frexp" | "issubnormal" | "maxintfloat" | "fma"
            | "muladd" => err(format!(
                "Math function {} should be handled by builtin_math",
                name
            )),
            // String functions delegated to builtin_string.rs
            // Note: startswith, endswith are now Pure Julia (base/strings.jl)
            "uppercase" | "lowercase" | "titlecase" | "ncodeunits" | "codeunit" | "codeunits"
            | "repeat" | "split" | "join" | "string" | "repr" | "sprintf" => err(format!(
                "String function {} should be handled by builtin_string",
                name
            )),
            // Type constructors delegated to builtin_types.rs
            "Char" | "Int" | "BigInt" | "BigFloat" | "Int8" | "Int16" | "Int32" | "Int64"
            | "Int128" | "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128" | "Float16"
            | "Float32" | "Float64" => err(format!(
                "Type constructor {} should be handled by builtin_types",
                name
            )),
            // Array functions delegated to builtin_array.rs
            "zeros" | "ones" | "length" | "getindex" | "setindex!" => err(format!(
                "Array function {} should be handled by builtin_array",
                name
            )),
            // Higher-order functions delegated to builtin_hof.rs
            // Note: broadcast/broadcast! are now Pure Julia (Issue #2548, #2549)
            "foreach" | "ntuple" => err(format!("HOF {} should be handled by builtin_hof", name)),
            // haskey, get: now Pure Julia (Issue #2572)
            // Phase 7-1 (Issue #2549): Broadcast operators removed from compiler.
            // Dot-syntax (.+, .-, etc.) is now handled by lowering (Phase 6) which generates
            // materialize(Broadcasted(op, (args...))) IR. These compiler patterns are dead code.
            // If somehow reached, fall through to the unknown function error below.
            // Note: sum is now Pure Julia (base/array.jl)
            // Note: mean is now Pure Julia (stdlib/Statistics/src/Statistics.jl)
            "isequal" => {
                // isequal(x, y) - NaN-aware equality
                if args.len() != 2 {
                    return err("isequal requires exactly 2 arguments");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isequal, 2));
                Ok(ValueType::Bool)
            }
            "hash" => {
                // hash(x) - 1-arg: direct Rust builtin for performance
                // hash(x, h) - 2-arg: fall through to Pure Julia dispatch (hashing.jl)
                if args.len() == 1 {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Hash, 1));
                    return Ok(ValueType::I64);
                }
                err("hash with 2+ arguments should use Pure Julia dispatch")
            }
            "_meta_parse" => {
                // _meta_parse(str) - internal builtin for Meta.parse
                // Returns Any because it can be Int64, Float64, String, Symbol, Expr, etc.
                if args.len() != 1 {
                    return err("_meta_parse requires exactly 1 argument: _meta_parse(str)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MetaParse, 1));
                Ok(ValueType::Any)
            }
            "_meta_parse_at" => {
                // _meta_parse_at(str, pos) - internal builtin for Meta.parse with position
                if args.len() != 2 {
                    return err(
                        "_meta_parse_at requires exactly 2 arguments: _meta_parse_at(str, pos)",
                    );
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MetaParseAt, 2));
                Ok(ValueType::Any) // Returns a tuple (expr, next_pos)
            }
            "_meta_lower" => {
                // _meta_lower(expr) - internal builtin for Meta.lower
                // Takes an expression and returns the lowered Core IR representation
                if args.len() != 1 {
                    return err("_meta_lower requires exactly 1 argument: _meta_lower(expr)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MetaLower, 1));
                Ok(ValueType::Any) // Returns lowered IR as Expr
            }
            // Regex internal builtins
            "_regex_replace" => {
                // _regex_replace(string, regex, replacement, count) - internal builtin for regex replace (Issue #2112)
                if args.len() != 4 {
                    return err("_regex_replace requires exactly 4 arguments: _regex_replace(string, regex, replacement, count)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.compile_expr(&args[2])?;
                self.compile_expr(&args[3])?;
                self.emit(Instr::CallBuiltin(BuiltinId::RegexReplace, 4));
                Ok(ValueType::Str)
            }            _ => {
                // Phase 7-1 (Issue #2549): User-defined function broadcast (f.(arr)) is now
                // handled by lowering (Phase 6) which generates materialize(Broadcasted(f, (args...)))
                // IR. The ".f" compiler pattern is dead code.
                err(format!("Unknown function: {}", name))
            }
        }
    }

    /// Resolve a FunctionRef expression to a function index.
    /// For HOF usage, prefers single-argument methods over multi-argument ones.
    pub(in super::super) fn resolve_function_ref(&self, expr: &Expr) -> CResult<usize> {
        self.resolve_function_ref_with_arity(expr, 1)
    }

    /// Resolve a function reference preferring methods with the given arity.
    /// For map functions (arity=1): prefers single-argument methods.
    /// For reduce operators (arity=2): prefers two-argument methods.
    /// This distinction is critical for operators like `+` and `-` that have
    /// both unary and binary forms (Issue #2004).
    pub(in super::super) fn resolve_function_ref_with_arity(
        &self,
        expr: &Expr,
        preferred_arity: usize,
    ) -> CResult<usize> {
        let name = match expr {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => name,
            _ => return err("Expected function reference"),
        };

        if let Some(table) = self.method_tables.get(name) {
            // Prefer methods matching the requested arity
            if let Some(method) = table
                .methods
                .iter()
                .find(|m| m.params.len() == preferred_arity)
            {
                return Ok(method.global_index);
            }
            // Fallback to first method if no method with preferred arity exists
            if let Some(method) = table.methods.first() {
                return Ok(method.global_index);
            }
        }

        match expr {
            Expr::FunctionRef { .. } => err(format!("Unknown function reference: {}", name)),
            _ => err(format!("Unknown function: {}", name)),
        }
    }

    /// Resolve a function reference for use in `sprint(f, args...)`.
    ///
    /// Sprint calls `f(io, args...)`, so the effective arity is `1 + extra_args.len()`.
    /// This helper infers the compile-time types of `extra_args`, prepends `JuliaType::IO`,
    /// and uses full method-table dispatch to select the most specific overload.
    ///
    /// Example: `sprint(show, 42)` → dispatch on `(IO, Int64)` → selects `show(io::IO, x::Int64)`.
    ///
    /// Falls back to arity-based selection and then first-method selection when dispatch fails
    /// (e.g., when extra arg type is unknown `Any`).
    pub(in super::super) fn resolve_sprint_function_ref(
        &mut self,
        func_expr: &Expr,
        extra_args: &[Expr],
    ) -> CResult<usize> {
        let name = match func_expr {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => name.clone(),
            _ => return err("Expected function reference"),
        };

        // Clone the table to avoid borrow conflict with self.infer_expr_type (which needs &mut self).
        let table_opt = self.method_tables.get(&name).cloned();

        if let Some(table) = table_opt {
            // Build arg type list: IO (sprint's buffer) followed by the extra arg types.
            let mut arg_julia_types = vec![JuliaType::IO];
            for arg in extra_args {
                let vt = self.infer_expr_type(arg);
                let jt = self.value_type_to_julia_type(&vt);
                arg_julia_types.push(jt);
            }

            // Type-directed dispatch: selects the most specific overload.
            match table.dispatch(&arg_julia_types) {
                Ok(sig) => return Ok(sig.global_index),
                Err(_) => {
                    // Dispatch failed (e.g. arg type Any, no match) — fall back to arity.
                    let preferred_arity = arg_julia_types.len();
                    if let Some(method) = table
                        .methods
                        .iter()
                        .find(|m| m.params.len() == preferred_arity)
                    {
                        return Ok(method.global_index);
                    }
                    // Final fallback: first registered method.
                    if let Some(method) = table.methods.first() {
                        return Ok(method.global_index);
                    }
                }
            }
        }

        match func_expr {
            Expr::FunctionRef { .. } => err(format!("Unknown function reference: {}", name)),
            _ => err(format!("Unknown function: {}", name)),
        }
    }

    /// Resolve a FunctionRef expression to its return type for HOF type inference.
    pub(in super::super) fn get_function_return_type(&self, expr: &Expr) -> Option<ValueType> {
        match expr {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => {
                if let Some(table) = self.method_tables.get(name) {
                    if let Some(method) = table.methods.first() {
                        return Some(method.return_type.clone());
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub(in super::super) fn compile_builtin(
        &mut self,
        name: &BuiltinOp,
        args: &[Expr],
    ) -> CResult<ValueType> {
        match name {
            BuiltinOp::Rand => {
                if args.is_empty() {
                    self.emit(Instr::RandF64);
                    Ok(ValueType::F64)
                } else {
                    // Check if first argument is a type identifier (Int, Int64, Float64)
                    let (dims, is_int_array) = if let Some(first) = args.first() {
                        match first {
                            Expr::Var(name, _) if name == "Int" || name == "Int64" => {
                                // rand(Int, dims...) or rand(Int64, dims...)
                                (&args[1..], true)
                            }
                            Expr::Var(name, _) if name == "Float64" => {
                                // rand(Float64, dims...) - same as rand(dims...)
                                (&args[1..], false)
                            }
                            _ => (args, false),
                        }
                    } else {
                        (args, false)
                    };

                    for dim in dims {
                        self.compile_expr_as(dim, ValueType::I64)?;
                    }

                    if is_int_array {
                        self.emit(Instr::RandIntArray(dims.len()));
                    } else {
                        self.emit(Instr::RandArray(dims.len()));
                    }
                    Ok(ValueType::Array)
                }
            }
            BuiltinOp::Sqrt => {
                let arg_ty = self.infer_expr_type(&args[0]);
                if self.is_struct_type_of(arg_ty, "Complex") {
                    // sqrt of complex number - use Pure Julia dispatch
                    if let Some(table) = self.method_tables.get("sqrt") {
                        let arg_julia_ty = self.infer_julia_type(&args[0]);
                        let arg_types = vec![arg_julia_ty];
                        if let Ok(method) = table.dispatch(&arg_types) {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::Call(method.global_index, 1));
                            return Ok(method.return_type.clone());
                        }
                    }
                    // Pure Julia dispatch failed - return error
                    err("Complex sqrt should use Pure Julia dispatch - sqrt(z::Complex) not found")
                } else {
                    // sqrt of real number
                    self.compile_expr_as(&args[0], ValueType::F64)?;
                    self.emit(Instr::SqrtF64);
                    Ok(ValueType::F64)
                }
            }
            BuiltinOp::Zeros => {
                // Check if first arg is a type: zeros(Type, dims...)
                if !args.is_empty() {
                    if let Some(type_name) = self.extract_array_type_name(&args[0]) {
                        // Compile remaining args as dimensions
                        for dim in &args[1..] {
                            self.compile_expr_as(dim, ValueType::I64)?;
                        }
                        let builtin = match type_name {
                            "Float64" => BuiltinId::ZerosF64,
                            "Int64" | "Int" => BuiltinId::ZerosI64,
                            "Complex{Float64}" => BuiltinId::ZerosComplexF64,
                            _ => {
                                return err(format!(
                                    "zeros with type {} not yet supported",
                                    type_name
                                ))
                            }
                        };
                        self.emit(Instr::CallBuiltin(builtin, args.len() - 1));
                        return Ok(ValueType::Array);
                    }
                }
                // zeros(dims...) - create array of zeros (via Builtin, default Float64)
                for dim in args {
                    self.compile_expr_as(dim, ValueType::I64)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Zeros, args.len()));
                Ok(ValueType::Array)
            }
            BuiltinOp::Ones => {
                // Check if first arg is a type: ones(Type, dims...)
                if !args.is_empty() {
                    if let Some(type_name) = self.extract_array_type_name(&args[0]) {
                        // Compile remaining args as dimensions
                        for dim in &args[1..] {
                            self.compile_expr_as(dim, ValueType::I64)?;
                        }
                        let builtin = match type_name {
                            "Float64" => BuiltinId::OnesF64,
                            "Int64" | "Int" => BuiltinId::OnesI64,
                            _ => {
                                return err(format!(
                                    "ones with type {} not yet supported",
                                    type_name
                                ))
                            }
                        };
                        self.emit(Instr::CallBuiltin(builtin, args.len() - 1));
                        return Ok(ValueType::Array);
                    }
                }
                // ones(dims...) - create array of ones (via Builtin, default Float64)
                for dim in args {
                    self.compile_expr_as(dim, ValueType::I64)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Ones, args.len()));
                Ok(ValueType::Array)
            }
            // Note: Trues, Falses, Fill are now Pure Julia (base/array.jl) — Issue #2640
            BuiltinOp::Reshape => {
                // reshape(arr, dims...) - change array dimensions (via Builtin)
                if args.is_empty() {
                    return err("reshape requires at least 1 argument: reshape(arr, dims...)");
                }
                // First argument is the array
                self.compile_expr(&args[0])?;
                // Remaining arguments are new dimensions
                for dim in &args[1..] {
                    self.compile_expr_as(dim, ValueType::I64)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::Reshape, args.len()));
                Ok(ValueType::Array)
            }
            BuiltinOp::Zero => {
                // zero(x) - return zero of same type as x
                if args.len() != 1 {
                    return Err(CompileError::Msg(
                        "zero() expects exactly 1 argument".to_string(),
                    ));
                }
                let input_type = self.compile_expr(&args[0])?;
                self.emit(Instr::Zero);
                // Return type matches input type
                Ok(input_type)
            }
            // Note: Complex operations (complex, conj, abs, abs2) are now Pure Julia with runtime dispatch
            BuiltinOp::Length => {
                // Check if argument is a Dict
                if let Expr::Var(name, _) = &args[0] {
                    if self.locals.get(name) == Some(&ValueType::Dict) {
                        self.emit(Instr::LoadDict(name.clone()));
                        self.emit(Instr::DictLen);
                        return Ok(ValueType::I64);
                    }
                }
                // Universal length - handles Array, Tuple, Dict, Range, String via CallBuiltin
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Length, 1));
                Ok(ValueType::I64)
            }
            // Note: BuiltinOp::Sum removed — sum is now Pure Julia (base/array.jl)
            BuiltinOp::Size => {
                // size(arr) or size(arr, dim) - via Builtin
                if args.is_empty() || args.len() > 2 {
                    return err("size requires 1 or 2 arguments: size(arr) or size(arr, dim)");
                }

                // Compile array
                self.compile_expr(&args[0])?;

                if args.len() == 2 {
                    // Compile dimension index
                    self.compile_expr_as(&args[1], ValueType::I64)?;
                }

                self.emit(Instr::CallBuiltin(BuiltinId::Size, args.len()));

                if args.len() == 1 {
                    Ok(ValueType::Tuple)
                } else {
                    Ok(ValueType::I64)
                }
            }
            BuiltinOp::Ndims => {
                // ndims(arr) - return number of dimensions
                if args.len() != 1 {
                    return err("ndims requires exactly 1 argument: ndims(arr)");
                }

                // Compile array
                self.compile_expr(&args[0])?;

                self.emit(Instr::CallBuiltin(BuiltinId::Ndims, 1));

                Ok(ValueType::I64)
            }
            BuiltinOp::Push => {
                // push!(arr_or_set, val)
                if args.len() != 2 {
                    return err("push! requires exactly 2 arguments: push!(arr, val)");
                }
                // Get the variable name for in-place modification
                if let Expr::Var(name, _) = &args[0] {
                    // Check if it's a Set or Array
                    let is_set = matches!(self.locals.get(name), Some(ValueType::Set));
                    if is_set {
                        // Set: load set, compile value, add to set, store back
                        self.emit(Instr::LoadSet(name.clone()));
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::SetAdd);
                        self.emit(Instr::StoreSet(name.clone()));
                        self.emit(Instr::LoadSet(name.clone()));
                        Ok(ValueType::Set)
                    } else {
                        // Array: load array, push value, store back
                        self.emit(Instr::LoadArray(name.clone()));
                        // Compile value without type coercion to support tuples and other types
                        let val_ty = self.compile_expr(&args[1])?;
                        // Only coerce to F64 if it's a numeric type (not Tuple, Struct, etc.)
                        match val_ty {
                            ValueType::I64 | ValueType::I32 | ValueType::F32 => {
                                self.emit(Instr::ToF64);
                            }
                            _ => {}
                        }
                        self.emit(Instr::ArrayPush);
                        // StoreArray is suppressed for globals (Issue #3121, #3127)
                        self.compile_store_and_reload_array(name);
                        Ok(ValueType::Array)
                    }
                } else {
                    err("push! first argument must be a variable")
                }
            }
            BuiltinOp::Pop => {
                // pop!(arr) for arrays - 1 argument
                // pop!(dict, key) or pop!(dict, key, default) for dicts - 2 or 3 arguments
                match args.len() {
                    1 => {
                        // Array pop: pop!(arr)
                        if let Expr::Var(name, _) = &args[0] {
                            // Load the array, pop the value, store back
                            self.emit(Instr::LoadArray(name.clone()));
                            self.emit(Instr::ArrayPop);
                            // ArrayPop leaves: [modified_array, popped_value] on stack
                            // Swap so we have: [popped_value, modified_array]
                            self.emit(Instr::Swap);
                            // StoreArray is suppressed for globals (Issue #3121, #3127)
                            self.compile_store_or_pop_global_array(name);
                            // Now popped_value is on top of stack
                            Ok(ValueType::F64)
                        } else {
                            err("pop! first argument must be a variable")
                        }
                    }
                    2 | 3 => {
                        // Dict pop: pop!(dict, key) or pop!(dict, key, default)
                        // For in-place semantics, first arg must be a variable
                        if let Expr::Var(name, _) = &args[0] {
                            // Load the dict variable
                            self.emit(Instr::LoadDict(name.clone()));
                            // Compile key
                            self.compile_expr(&args[1])?;
                            // Compile default if provided
                            if args.len() == 3 {
                                self.compile_expr(&args[2])?;
                            }
                            // Call the builtin - this leaves [modified_dict, result] on stack
                            self.emit(Instr::CallBuiltin(BuiltinId::DictPop, args.len()));
                            // Stack: [modified_dict, result]
                            // Swap to get [result, modified_dict]
                            self.emit(Instr::Swap);
                            // Store modified dict back to variable
                            self.emit(Instr::StoreDict(name.clone()));
                            // Result is now on top of stack
                            Ok(ValueType::Any)
                        } else {
                            err("pop! first argument must be a variable for dict")
                        }
                    }
                    _ => {
                        err("pop! requires 1 argument for arrays (pop!(arr)) or 2-3 arguments for dicts (pop!(dict, key) or pop!(dict, key, default))")
                    }
                }
            }
            BuiltinOp::PushFirst => {
                // pushfirst!(arr, val)
                if args.len() != 2 {
                    return err("pushfirst! requires exactly 2 arguments: pushfirst!(arr, val)");
                }
                if let Expr::Var(name, _) = &args[0] {
                    self.emit(Instr::LoadArray(name.clone()));
                    // Compile value without type coercion to support tuples and other types
                    let val_ty = self.compile_expr(&args[1])?;
                    // Only coerce to F64 if it's a numeric type (not Tuple, Struct, etc.)
                    match val_ty {
                        ValueType::I64 | ValueType::I32 | ValueType::F32 => {
                            self.emit(Instr::ToF64);
                        }
                        _ => {}
                    }
                    self.emit(Instr::ArrayPushFirst);
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_and_reload_array(name);
                    Ok(ValueType::Array)
                } else {
                    err("pushfirst! first argument must be a variable")
                }
            }
            BuiltinOp::PopFirst => {
                // popfirst!(arr)
                if args.len() != 1 {
                    return err("popfirst! requires exactly 1 argument: popfirst!(arr)");
                }
                if let Expr::Var(name, _) = &args[0] {
                    self.emit(Instr::LoadArray(name.clone()));
                    self.emit(Instr::ArrayPopFirst);
                    // ArrayPopFirst leaves: [modified_array, popped_value] on stack
                    self.emit(Instr::Swap);
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_or_pop_global_array(name);
                    Ok(ValueType::F64)
                } else {
                    err("popfirst! first argument must be a variable")
                }
            }
            BuiltinOp::Insert => {
                // insert!(arr, i, val)
                if args.len() != 3 {
                    return err("insert! requires exactly 3 arguments: insert!(arr, i, val)");
                }
                if let Expr::Var(name, _) = &args[0] {
                    self.emit(Instr::LoadArray(name.clone()));
                    self.compile_expr_as(&args[1], ValueType::I64)?; // index
                    self.compile_expr_as(&args[2], ValueType::F64)?; // value
                    self.emit(Instr::ArrayInsert);
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_and_reload_array(name);
                    Ok(ValueType::Array)
                } else {
                    err("insert! first argument must be a variable")
                }
            }
            BuiltinOp::DeleteAt => {
                // deleteat!(arr, i)
                if args.len() != 2 {
                    return err("deleteat! requires exactly 2 arguments: deleteat!(arr, i)");
                }
                if let Expr::Var(name, _) = &args[0] {
                    self.emit(Instr::LoadArray(name.clone()));
                    self.compile_expr_as(&args[1], ValueType::I64)?; // index
                    self.emit(Instr::ArrayDeleteAt);
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_and_reload_array(name);
                    Ok(ValueType::Array)
                } else {
                    err("deleteat! first argument must be a variable")
                }
            }
            // Note: BuiltinOp::Adjoint and BuiltinOp::Transpose have been removed
            // They are now implemented in Pure Julia
            BuiltinOp::Lu => {
                // lu(A) - LU decomposition with partial pivoting
                // Returns (L, U, p) tuple
                if args.len() != 1 {
                    return err("lu requires exactly 1 argument: lu(A)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Lu, 1));
                Ok(ValueType::Tuple)
            }
            BuiltinOp::Det => {
                // det(A) - matrix determinant
                if args.len() != 1 {
                    return err("det requires exactly 1 argument: det(A)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Det, 1));
                Ok(ValueType::F64)
            }
            // Note: BuiltinOp::Inv removed — dead code (Issue #2643)
            BuiltinOp::StableRNG => {
                // StableRNG(seed) - create StableRNG instance
                if args.len() != 1 {
                    return err("StableRNG requires exactly one argument (seed)");
                }
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::NewStableRng);
                Ok(ValueType::Rng)
            }
            BuiltinOp::XoshiroRNG => {
                // Xoshiro(seed) - create Xoshiro256++ RNG instance
                if args.len() != 1 {
                    return err("Xoshiro requires exactly one argument (seed)");
                }
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::NewXoshiro);
                Ok(ValueType::Rng)
            }
            BuiltinOp::Randn => {
                // randn() or randn(rng) - standard normal distribution
                if args.is_empty() {
                    // randn() - use global RNG
                    self.emit(Instr::RandnF64);
                    Ok(ValueType::F64)
                } else {
                    // randn(rng) - use provided RNG
                    // First check if first arg is an RNG
                    let first_ty = self.infer_expr_type(&args[0]);
                    if first_ty == ValueType::Rng {
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::RngRandnF64);
                        Ok(ValueType::F64)
                    } else {
                        // randn(dims...) - create array with global RNG
                        for dim in args {
                            self.compile_expr_as(dim, ValueType::I64)?;
                        }
                        self.emit(Instr::RandnArray(args.len()));
                        Ok(ValueType::Array)
                    }
                }
            }
            BuiltinOp::DictGet => {
                // get(dict, key, default)
                if args.len() != 3 {
                    return err("get requires exactly 3 arguments: get(dict, key, default)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.compile_expr(&args[2])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictGet, 3));
                Ok(ValueType::I64)
            }
            BuiltinOp::DictGetkey => {
                // getkey(dict, key, default) - return the key if it exists, else default
                if args.len() != 3 {
                    return err("getkey requires exactly 3 arguments: getkey(dict, key, default)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.compile_expr(&args[2])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictGetkey, 3));
                Ok(ValueType::Any)
            }
            BuiltinOp::HasKey => {
                // haskey(dict, key)
                if args.len() != 2 {
                    return err("haskey requires exactly 2 arguments: haskey(dict, key)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictHasKey, 2));
                Ok(ValueType::Bool)
            }
            BuiltinOp::IfElse => {
                // ifelse(cond, then_val, else_val) - ternary operator
                if args.len() != 3 {
                    return err("ifelse requires exactly 3 arguments: ifelse(cond, then, else)");
                }
                // Compile condition
                self.compile_expr_as(&args[0], ValueType::Bool)?;

                // Jump to else if condition is false (0)
                let jump_to_else = self.here();
                self.emit(Instr::JumpIfZero(usize::MAX));

                // Then branch
                let then_ty = self.compile_expr(&args[1])?;
                let jump_to_end = self.here();
                self.emit(Instr::Jump(usize::MAX));

                // Else branch
                let else_start = self.here();
                let else_ty = self.compile_expr(&args[2])?;

                // Patch jumps
                let end_label = self.here();
                self.code[jump_to_else] = Instr::JumpIfZero(else_start);
                self.code[jump_to_end] = Instr::Jump(end_label);

                // Return promoted type using Julia's numeric promotion rules
                if then_ty == else_ty {
                    Ok(then_ty)
                } else if let Some(promoted) = promote_numeric_value_types(&then_ty, &else_ty) {
                    Ok(promoted)
                } else {
                    Ok(then_ty)
                }
            }
            BuiltinOp::TimeNs => {
                if !args.is_empty() {
                    return err("time_ns expects no arguments");
                }
                self.emit(Instr::TimeNs);
                Ok(ValueType::I64)
            }
            BuiltinOp::Ref => {
                // Ref(x) - wrap value to protect from broadcasting (treated as scalar)
                if args.len() != 1 {
                    return err("Ref requires exactly 1 argument: Ref(value)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::MakeRef);
                Ok(ValueType::Any) // Ref can wrap any type
            }
            BuiltinOp::TupleFirst => {
                if args.len() == 1 {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::TupleFirst, 1));
                    // Tuple element type is unknown at compile time
                    Ok(ValueType::Any)
                } else if args.len() == 2 {
                    // first(collection, n) - delegate to Pure Julia
                    self.compile_call("first", args, &[], &[], &[])
                } else {
                    err("first requires 1 or 2 arguments: first(x) or first(x, n)")
                }
            }
            BuiltinOp::TupleLast => {
                if args.len() == 1 {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::TupleLast, 1));
                    // Tuple element type is unknown at compile time
                    Ok(ValueType::Any)
                } else if args.len() == 2 {
                    // last(collection, n) - delegate to Pure Julia
                    self.compile_call("last", args, &[], &[], &[])
                } else {
                    err("last requires 1 or 2 arguments: last(x) or last(x, n)")
                }
            }
            // Note: BuiltinOp::TupleLength removed — dead code (Issue #2643)
            BuiltinOp::DictDelete => {
                // delete!(dict_or_set, key)
                if args.len() != 2 {
                    return err("delete! requires exactly 2 arguments: delete!(dict, key)");
                }
                // Get the variable name for in-place modification
                if let Expr::Var(name, _) = &args[0] {
                    let var_type = self.locals.get(name).cloned();
                    if matches!(var_type, Some(ValueType::Set)) {
                        // Set: load set, compile key, delete key, store back
                        self.emit(Instr::LoadSet(name.clone()));
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::DictDelete, 2));
                        self.emit(Instr::StoreSet(name.clone()));
                        self.emit(Instr::LoadSet(name.clone()));
                        Ok(ValueType::Set)
                    } else if matches!(var_type, Some(ValueType::Dict)) || var_type.is_none() {
                        // Dict (statically typed) or global variable: load dict, compile key,
                        // delete key, store back. Global variables (var_type.is_none()) also
                        // use this path for backwards compatibility.
                        self.emit(Instr::LoadDict(name.clone()));
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::DictDelete, 2));
                        self.emit(Instr::StoreDict(name.clone()));
                        self.emit(Instr::LoadDict(name.clone()));
                        Ok(ValueType::Dict)
                    } else {
                        // Any-typed or Struct-typed local: load actual value so runtime can
                        // dispatch to user-defined delete! methods on non-Dict StructRefs.
                        // (Issue #3169)
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::DictDelete, 2));
                        Ok(ValueType::Any)
                    }
                } else {
                    // Fallback: non-variable first argument (field access, function call, etc.)
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::DictDelete, 2));
                    Ok(ValueType::Dict)
                }
            }
            BuiltinOp::DictKeys => {
                // keys(dict)
                if args.len() != 1 {
                    return err("keys requires exactly 1 argument: keys(dict)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictKeys, 1));
                Ok(ValueType::Tuple)
            }
            BuiltinOp::DictValues => {
                // values(dict)
                if args.len() != 1 {
                    return err("values requires exactly 1 argument: values(dict)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictValues, 1));
                Ok(ValueType::Tuple)
            }
            BuiltinOp::DictPairs => {
                // pairs(dict)
                if args.len() != 1 {
                    return err("pairs requires exactly 1 argument: pairs(dict)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictPairs, 1));
                Ok(ValueType::Tuple)
            }
            BuiltinOp::DictMerge => {
                // merge is now Pure Julia (Issue #2573)
                // This arm is kept for exhaustive matching but should not be reached
                // since merge is no longer routed through BuiltinOp.
                err("internal: DictMerge should be handled by Pure Julia merge()")
            }
            BuiltinOp::DictGetBang => {
                // get!(dict, key, default) - get value or insert default
                if args.len() != 3 {
                    return err("get! requires exactly 3 arguments: get!(dict, key, default)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.compile_expr(&args[2])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictGetBang, 3));
                Ok(ValueType::Any)
            }
            BuiltinOp::DictMergeBang => {
                // merge!(dict1, dict2) - merge in-place (Issue #2134)
                if args.len() != 2 {
                    return err("merge! requires exactly 2 arguments: merge!(dict1, dict2)");
                }
                if let Expr::Var(name, _) = &args[0] {
                    self.emit(Instr::LoadDict(name.clone()));
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::DictMergeBang, 2));
                    self.emit(Instr::StoreDict(name.clone()));
                    self.emit(Instr::LoadDict(name.clone()));
                    Ok(ValueType::Dict)
                } else {
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::DictMergeBang, 2));
                    Ok(ValueType::Dict)
                }
            }
            BuiltinOp::DictEmpty => {
                // empty!(dict) - remove all entries (Issue #2134)
                if args.len() != 1 {
                    return err("empty! requires exactly 1 argument: empty!(dict)");
                }
                if let Expr::Var(name, _) = &args[0] {
                    let is_set = matches!(self.locals.get(name), Some(ValueType::Set));
                    if is_set {
                        self.emit(Instr::LoadSet(name.clone()));
                        self.emit(Instr::CallBuiltin(BuiltinId::DictEmpty, 1));
                        self.emit(Instr::StoreSet(name.clone()));
                        self.emit(Instr::LoadSet(name.clone()));
                        Ok(ValueType::Set)
                    } else {
                        self.emit(Instr::LoadDict(name.clone()));
                        self.emit(Instr::CallBuiltin(BuiltinId::DictEmpty, 1));
                        self.emit(Instr::StoreDict(name.clone()));
                        self.emit(Instr::LoadDict(name.clone()));
                        Ok(ValueType::Dict)
                    }
                } else {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::DictEmpty, 1));
                    Ok(ValueType::Dict)
                }
            }
            BuiltinOp::TypeOf => {
                // typeof(x) - get DataType (the type of the value)
                if args.len() != 1 {
                    return err("typeof requires exactly 1 argument: typeof(value)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::TypeOf, 1));
                Ok(ValueType::DataType)
            }
            BuiltinOp::Isa => {
                // isa(x, T) - check if x is of type T
                if args.len() != 2 {
                    return err("isa requires exactly 2 arguments: isa(value, Type)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isa, 2));
                Ok(ValueType::Bool)
            }
            BuiltinOp::Eltype => {
                // eltype(x) - get element type of collection
                if args.len() != 1 {
                    return err("eltype requires exactly 1 argument: eltype(collection)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Eltype, 1));
                Ok(ValueType::DataType)
            }
            BuiltinOp::Keytype => {
                // keytype(x) - get key type of collection
                if args.len() != 1 {
                    return err("keytype requires exactly 1 argument: keytype(collection)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Keytype, 1));
                Ok(ValueType::DataType)
            }
            BuiltinOp::Valtype => {
                // valtype(x) - get value type of collection
                if args.len() != 1 {
                    return err("valtype requires exactly 1 argument: valtype(collection)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Valtype, 1));
                Ok(ValueType::DataType)
            }
            BuiltinOp::Sizeof => {
                // sizeof(x) - get size of value in bytes
                if args.len() != 1 {
                    return err("sizeof requires exactly 1 argument: sizeof(value)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Sizeof, 1));
                Ok(ValueType::I64)
            }
            BuiltinOp::Isbits => {
                // isbits(x) - check if x is an instance of a bits type
                if args.len() != 1 {
                    return err("isbits requires exactly 1 argument: isbits(value)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isbits, 1));
                Ok(ValueType::Bool)
            }
            BuiltinOp::Isbitstype => {
                // isbitstype(T) - check if T is a bits type
                if args.len() != 1 {
                    return err("isbitstype requires exactly 1 argument: isbitstype(Type)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Isbitstype, 1));
                Ok(ValueType::Bool)
            }
            BuiltinOp::Supertype => {
                // supertype(T) - get parent type
                if args.len() != 1 {
                    return err("supertype requires exactly 1 argument: supertype(Type)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Supertype, 1));
                Ok(ValueType::DataType)
            }
            BuiltinOp::Supertypes => {
                // supertypes(T) - tuple of all supertypes
                if args.len() != 1 {
                    return err("supertypes requires exactly 1 argument: supertypes(Type)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Supertypes, 1));
                Ok(ValueType::Tuple)
            }
            BuiltinOp::Subtypes => {
                // subtypes(T) - vector of direct subtypes
                if args.len() != 1 {
                    return err("subtypes requires exactly 1 argument: subtypes(Type)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Subtypes, 1));
                Ok(ValueType::Array)
            }
            BuiltinOp::Typeintersect => {
                // typeintersect(A, B) - compute type intersection
                if args.len() != 2 {
                    return err(
                        "typeintersect requires exactly 2 arguments: typeintersect(Type1, Type2)",
                    );
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Typeintersect, 2));
                Ok(ValueType::DataType)
            }
            // BuiltinOp::Typejoin removed - now Pure Julia (base/reflection.jl)
            // BuiltinOp::Fieldcount removed - now Pure Julia (base/reflection.jl)
            BuiltinOp::Hasfield => {
                // hasfield(T, name) - check if field exists
                if args.len() != 2 {
                    return err("hasfield requires exactly 2 arguments: hasfield(Type, :name)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Hasfield, 2));
                Ok(ValueType::Bool)
            }
            // BuiltinOp::Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
            // removed - now Pure Julia (base/reflection.jl)
            BuiltinOp::Ismutable => {
                // ismutable(x) - is x mutable
                if args.len() != 1 {
                    return err("ismutable requires exactly 1 argument: ismutable(x)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Ismutable, 1));
                Ok(ValueType::Bool)
            }
            // BuiltinOp::Ismutabletype removed - now Pure Julia (base/reflection.jl)
            // BuiltinOp::NameOf removed - now Pure Julia (base/reflection.jl)
            BuiltinOp::Objectid => {
                // objectid(x) - unique object identifier
                if args.len() != 1 {
                    return err("objectid requires exactly 1 argument: objectid(x)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Objectid, 1));
                Ok(ValueType::U64)
            }
            BuiltinOp::Isunordered => {
                err("internal: Isunordered should be handled by Pure Julia isunordered()")
            }
            BuiltinOp::In => {
                // in(x, collection) - check if element is in collection
                if args.len() != 2 {
                    return err("in requires exactly 2 arguments: in(x, collection)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::In, 2));
                Ok(ValueType::Bool)
            }
            BuiltinOp::Methods => {
                // methods(f) or methods(f, types) - get methods for function, optionally filtered
                if args.len() == 1 {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Methods, 1));
                } else if args.len() == 2 {
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Methods, 2));
                } else {
                    return err("methods requires 1 or 2 arguments: methods(f) or methods(f, types)");
                }
                Ok(ValueType::Array) // Returns Vector{Method}
            }
            BuiltinOp::HasMethod => {
                // hasmethod(f, types) - check if method exists
                if args.len() != 2 {
                    return err("hasmethod requires exactly 2 arguments: hasmethod(f, types)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::HasMethod, 2));
                Ok(ValueType::Bool)
            }
            BuiltinOp::Which => {
                // which(f, types) - get specific method
                if args.len() != 2 {
                    return err("which requires exactly 2 arguments: which(f, types)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Which, 2));
                Ok(ValueType::Any) // Returns Method struct
            }
            BuiltinOp::Seed => {
                // seed!(n) - reseed global RNG (only via Random.seed!())
                if args.len() != 1 {
                    return err("seed! requires exactly 1 argument: seed!(seed_value)");
                }
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::SeedGlobalRng);
                Ok(ValueType::Nothing)
            }
            BuiltinOp::Iterate => {
                // iterate(collection) or iterate(collection, state)
                // Returns (element, state) or nothing
                //
                // First check for user-defined iterate methods (for custom iterators)
                // Then fall back to builtin for basic types
                if !args.is_empty() {
                    let arg_ty = self.infer_julia_type(&args[0]);

                    // Check method tables for iterate - enables custom iterators
                    if let Some(table) = self.method_tables.get("iterate") {
                        let arg_types: Vec<JuliaType> =
                            args.iter().map(|a| self.infer_julia_type(a)).collect();

                        match table.dispatch(&arg_types) {
                            Ok(method) => {
                                // User-defined iterate method found - use it
                                for arg in args {
                                    self.compile_expr(arg)?;
                                }
                                self.emit(Instr::Call(method.global_index, args.len()));
                                // Return Any since iterate can return Tuple or Nothing
                                // IndexLoad handles tuple indexing at runtime
                                return Ok(ValueType::Any);
                            }
                            Err(_) => {
                                // No matching method, fall through to builtin handling
                            }
                        }
                    }

                    // For struct types with no matching iterate method, try runtime dispatch
                    // But only for actual Struct types, not for Any (which could be an Array/Range/etc.)
                    if matches!(arg_ty, JuliaType::Struct(ref type_name) if type_name == "EachCol" || type_name == "EachRow" || type_name == "EachSlice")
                    {
                        // EachCol and EachRow need to use IterateDynamic for Julia method delegation
                        if let Some(table) = self.method_tables.get("iterate") {
                            // Build candidates list from iterate methods with matching arg count
                            let candidates: Vec<(usize, String)> = table
                                .methods
                                .iter()
                                .filter(|m| m.params.len() == args.len())
                                .filter_map(|m| {
                                    // Extract type name from first parameter
                                    if let (_, JuliaType::Struct(type_name)) = &m.params[0] {
                                        Some((m.global_index, type_name.clone()))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !candidates.is_empty() {
                                // Compile arguments and emit IterateDynamic
                                for arg in args {
                                    self.compile_expr(arg)?;
                                }
                                self.emit(Instr::IterateDynamic(args.len(), candidates));
                                return Ok(ValueType::Any);
                            }
                        }
                    } else if let JuliaType::Struct(ref struct_name) = arg_ty {
                        // Other struct types: try dispatch with type matching
                        if let Some(table) = self.method_tables.get("iterate") {
                            // Extract base name for parametric struct matching
                            // e.g., "Zip3{Any, Any, Any}" -> "Zip3"
                            let struct_base = if let Some(idx) = struct_name.find('{') {
                                &struct_name[..idx]
                            } else {
                                struct_name.as_str()
                            };
                            // Find a method matching both arg count and struct type
                            let matching_method = table.methods.iter().find(|m| {
                                if m.params.len() != args.len() {
                                    return false;
                                }
                                // Check first parameter is the correct struct type
                                if let (_, JuliaType::Struct(ref param_type)) = m.params[0] {
                                    let param_base = if let Some(idx) = param_type.find('{') {
                                        &param_type[..idx]
                                    } else {
                                        param_type.as_str()
                                    };
                                    param_base == struct_base
                                } else {
                                    false
                                }
                            });
                            if let Some(method) = matching_method {
                                for arg in args {
                                    self.compile_expr(arg)?;
                                }
                                self.emit(Instr::Call(method.global_index, args.len()));
                                // Return Any since iterate can return Tuple or Nothing
                                return Ok(ValueType::Any);
                            }
                        }
                    }

                    // For Any type, use IterateDynamic for runtime dispatch
                    // This handles the case where parametric struct fields (e.g., it.xs in Drop{I})
                    // could be either builtin types or custom iterators at runtime
                    if matches!(arg_ty, JuliaType::Any) {
                        if let Some(table) = self.method_tables.get("iterate") {
                            // Build candidates list from iterate methods with matching arg count
                            let candidates: Vec<(usize, String)> = table
                                .methods
                                .iter()
                                .filter(|m| m.params.len() == args.len())
                                .filter_map(|m| {
                                    // Extract type name from first parameter
                                    if let (_, JuliaType::Struct(type_name)) = &m.params[0] {
                                        Some((m.global_index, type_name.clone()))
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            if !candidates.is_empty() {
                                // Compile arguments and emit IterateDynamic
                                for arg in args {
                                    self.compile_expr(arg)?;
                                }
                                self.emit(Instr::IterateDynamic(args.len(), candidates));
                                return Ok(ValueType::Any);
                            }
                        }
                    }
                }

                // Fall back to builtin for basic types (Array, Range, Tuple, String)
                match args.len() {
                    1 => {
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::Iterate, 1));
                    }
                    2 => {
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::Iterate, 2));
                    }
                    _ => return err("iterate requires 1 or 2 arguments: iterate(collection) or iterate(collection, state)"),
                }
                // Returns Any (Tuple or Nothing depending on collection)
                Ok(ValueType::Any)
            }
            BuiltinOp::Collect => {
                // collect(iterable) -> Array
                if args.len() != 1 {
                    return err("collect requires exactly 1 argument: collect(iterable)");
                }

                // For struct types or Any type, prefer Pure Julia collect
                // which uses the iterate protocol
                let arg_ty = self.infer_julia_type(&args[0]);
                if matches!(arg_ty, JuliaType::Struct(_)) || matches!(arg_ty, JuliaType::Any) {
                    if let Some(table) = self.method_tables.get("collect") {
                        // Find the generic collect(::Any) method
                        if let Some(method) = table.methods.iter().find(|m| {
                            m.params.len() == 1 && matches!(m.params[0].1, JuliaType::Any)
                        }) {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::Call(method.global_index, 1));
                            return Ok(ValueType::Array);
                        }
                    }
                }

                // Fall back to builtin for basic types (Array, Range, Tuple)
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::RangeCollect, 1));

                // Infer element type from argument for proper Vector{T} dispatch
                // Ranges produce Int64 elements, other types produce generic Array
                let result_type = match arg_ty {
                    JuliaType::UnitRange | JuliaType::StepRange => {
                        ValueType::ArrayOf(ArrayElementType::I64)
                    }
                    JuliaType::VectorOf(ref elem) => {
                        // Preserve element type for collect on vectors
                        match elem.as_ref() {
                            JuliaType::Int64 => ValueType::ArrayOf(ArrayElementType::I64),
                            JuliaType::Float64 => ValueType::ArrayOf(ArrayElementType::F64),
                            JuliaType::Bool => ValueType::ArrayOf(ArrayElementType::Bool),
                            JuliaType::String => ValueType::ArrayOf(ArrayElementType::String),
                            JuliaType::Char => ValueType::ArrayOf(ArrayElementType::Char),
                            _ => ValueType::Array,
                        }
                    }
                    _ => ValueType::Array,
                };
                Ok(result_type)
            }
            BuiltinOp::Generator => {
                // Generator(f, iter) - create lazy generator
                // First arg is function reference, second is iterator
                if args.len() != 2 {
                    return err("Generator requires 2 arguments: Generator(f, iter)");
                }
                let func_index = self.resolve_function_ref(&args[0])?;
                self.compile_expr(&args[1])?;
                self.emit(Instr::MakeGenerator(func_index));
                Ok(ValueType::Generator)
            }
            BuiltinOp::SymbolNew => {
                // Symbol("name") - create a symbol from a string
                if args.len() != 1 {
                    return err("Symbol requires exactly 1 argument: Symbol(name)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::SymbolNew, 1));
                Ok(ValueType::Symbol)
            }
            BuiltinOp::ExprNew => {
                // Expr(head, args...) - create an expression
                // head is a Symbol, args are the expression arguments
                if args.is_empty() {
                    return err("Expr requires at least 1 argument: Expr(head, args...)");
                }

                // Check if any argument is a SplatInterpolation marker
                let mut splat_mask: u64 = 0;
                let mut has_splat = false;
                for (i, arg) in args.iter().enumerate() {
                    if let Expr::Builtin {
                        name: BuiltinOp::SplatInterpolation,
                        ..
                    } = arg
                    {
                        if i < 64 {
                            splat_mask |= 1u64 << i;
                            has_splat = true;
                        }
                    }
                }

                if has_splat {
                    // Compile args: for SplatInterpolation, compile the inner variable
                    for arg in args.iter() {
                        if let Expr::Builtin {
                            name: BuiltinOp::SplatInterpolation,
                            args: splat_args,
                            ..
                        } = arg
                        {
                            // Compile the variable being splatted
                            if let Some(inner) = splat_args.first() {
                                self.compile_expr(inner)?;
                            } else {
                                return err("SplatInterpolation requires an argument");
                            }
                        } else {
                            self.compile_expr(arg)?;
                        }
                    }
                    // Push splat_mask as the last argument
                    self.emit(Instr::PushI64(splat_mask as i64));
                    // Call ExprNewWithSplat with argc + 1 (for the mask)
                    self.emit(Instr::CallBuiltin(
                        BuiltinId::ExprNewWithSplat,
                        args.len() + 1,
                    ));
                } else {
                    // No splat, use regular ExprNew
                    for arg in args.iter() {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::CallBuiltin(BuiltinId::ExprNew, args.len()));
                }
                Ok(ValueType::Expr)
            }
            BuiltinOp::LineNumberNodeNew => {
                // LineNumberNode(line) or LineNumberNode(line, file)
                // line is an integer, file is a Symbol (optional)
                match args.len() {
                    1 => {
                        // LineNumberNode(line) - file is None
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::LineNumberNodeNew, 1));
                    }
                    2 => {
                        // LineNumberNode(line, file)
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::LineNumberNodeNew, 2));
                    }
                    _ => {
                        return err("LineNumberNode requires 1 or 2 arguments: LineNumberNode(line) or LineNumberNode(line, file)");
                    }
                }
                Ok(ValueType::LineNumberNode)
            }
            BuiltinOp::QuoteNodeNew => {
                // QuoteNode(value) - wrap value in QuoteNode
                if args.len() != 1 {
                    return err("QuoteNode requires exactly 1 argument: QuoteNode(value)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::QuoteNodeNew, 1));
                Ok(ValueType::QuoteNode)
            }
            BuiltinOp::GlobalRefNew => {
                // GlobalRef(mod, name) - create a global reference
                // mod can be a Module or a Symbol (module name)
                // name is a Symbol
                if args.len() != 2 {
                    return err("GlobalRef requires exactly 2 arguments: GlobalRef(mod, name)");
                }
                // Compile mod argument
                self.compile_expr(&args[0])?;
                // Compile name argument
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::GlobalRefNew, 2));
                Ok(ValueType::GlobalRef)
            }
            BuiltinOp::Gensym => {
                // gensym() or gensym("base") - generate unique symbol
                if args.is_empty() {
                    // gensym() - no arguments
                    self.emit(Instr::CallBuiltin(BuiltinId::Gensym, 0));
                } else if args.len() == 1 {
                    // gensym("base") - with base name
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Gensym, 1));
                } else {
                    return err("gensym takes 0 or 1 argument: gensym() or gensym(base)");
                }
                Ok(ValueType::Symbol)
            }
            BuiltinOp::Esc => {
                // esc(expr) - escape expression for macro hygiene
                if args.len() != 1 {
                    return err("esc requires exactly 1 argument: esc(expr)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Esc, 1));
                Ok(ValueType::Expr)
            }
            BuiltinOp::Eval => {
                // eval(expr) - evaluate an Expr at runtime
                if args.len() != 1 {
                    return err("eval requires exactly 1 argument: eval(expr)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Eval, 1));
                Ok(ValueType::Any) // Result type is dynamic
            }
            BuiltinOp::MacroExpand => {
                // macroexpand(m, x) - return expanded form of macro call
                // In SubsetJuliaVM, macro expansion happens at compile time, so at runtime
                // we just return the expression as-is (already expanded during lowering).
                // The module parameter is ignored since we don't have runtime module support.
                if args.len() != 2 {
                    return err("macroexpand requires exactly 2 arguments: macroexpand(m, x)");
                }
                // Compile the module (ignored at runtime)
                self.compile_expr(&args[0])?;
                // Compile the expression
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MacroExpand, 2));
                Ok(ValueType::Any) // Can return any type (Expr, literal, Symbol, etc.)
            }
            BuiltinOp::MacroExpandBang => {
                // macroexpand!(m, x) - destructively expand macro call
                // Same behavior as macroexpand in SubsetJuliaVM (no mutation distinction)
                if args.len() != 2 {
                    return err("macroexpand! requires exactly 2 arguments: macroexpand!(m, x)");
                }
                // Compile the module (ignored at runtime)
                self.compile_expr(&args[0])?;
                // Compile the expression
                self.compile_expr(&args[1])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MacroExpandBang, 2));
                Ok(ValueType::Any) // Can return any type (Expr, literal, Symbol, etc.)
            }
            BuiltinOp::IncludeString => {
                // include_string(m, code) or include_string(m, code, filename)
                // Parse and evaluate all expressions in the code string.
                if args.len() < 2 || args.len() > 3 {
                    return err("include_string requires 2 or 3 arguments: include_string(m, code) or include_string(m, code, filename)");
                }
                // Compile all arguments
                for arg in args.iter() {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::IncludeString, args.len()));
                Ok(ValueType::Any) // Result type is dynamic
            }
            BuiltinOp::EvalFile => {
                // evalfile(path) or evalfile(path, args)
                // Read file and evaluate all expressions.
                if args.is_empty() || args.len() > 2 {
                    return err(
                        "evalfile requires 1 or 2 arguments: evalfile(path) or evalfile(path, args)",
                    );
                }
                // Compile all arguments
                for arg in args.iter() {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::EvalFile, args.len()));
                Ok(ValueType::Any) // Result type is dynamic
            }
            BuiltinOp::SplatInterpolation => {
                // This marker is handled during ExprNew compilation above.
                // If it appears standalone, it's an error.
                err("SplatInterpolation should be inside ExprNew, not as a standalone builtin")
            }
            // Note: RuntimeSplatInterpolation, ExprNewWithSplat removed — dead code (Issue #2643)
            BuiltinOp::TestRecord => {
                // _test_record!(passed, msg) - record test result
                if args.len() != 2 {
                    return err(
                        "_test_record! requires exactly 2 arguments: _test_record!(passed, msg)",
                    );
                }
                self.compile_expr(&args[0])?; // passed: Bool
                self.compile_expr(&args[1])?; // msg: String
                self.emit(Instr::CallBuiltin(BuiltinId::TestRecord, 2));
                Ok(ValueType::Nothing)
            }
            BuiltinOp::TestRecordBroken => {
                // _test_record_broken!(passed, msg) - record broken test result
                if args.len() != 2 {
                    return err(
                        "_test_record_broken! requires exactly 2 arguments: _test_record_broken!(passed, msg)",
                    );
                }
                self.compile_expr(&args[0])?; // passed: Bool
                self.compile_expr(&args[1])?; // msg: String
                self.emit(Instr::CallBuiltin(BuiltinId::TestRecordBroken, 2));
                Ok(ValueType::Nothing)
            }
            BuiltinOp::TestSetBegin => {
                // _testset_begin!(name) - begin test set
                if args.len() != 1 {
                    return err(
                        "_testset_begin! requires exactly 1 argument: _testset_begin!(name)",
                    );
                }
                self.compile_expr(&args[0])?; // name: String
                self.emit(Instr::CallBuiltin(BuiltinId::TestSetBegin, 1));
                Ok(ValueType::Nothing)
            }
            BuiltinOp::TestSetEnd => {
                // _testset_end!() - end test set and print summary
                if !args.is_empty() {
                    return err("_testset_end! takes no arguments");
                }
                self.emit(Instr::CallBuiltin(BuiltinId::TestSetEnd, 0));
                Ok(ValueType::Nothing)
            }
            BuiltinOp::IsDefined => {
                // @isdefined(x) - check if variable x is defined
                // The argument is a string literal containing the variable name
                if args.len() != 1 {
                    return err("@isdefined requires exactly 1 argument: @isdefined(var)");
                }
                // Extract the variable name from the string literal argument
                let var_name = match &args[0] {
                    crate::ir::core::Expr::Literal(crate::ir::core::Literal::Str(name), _) => {
                        name.clone()
                    }
                    _ => {
                        return err("@isdefined internal error: expected string literal argument");
                    }
                };
                self.emit(Instr::IsDefined(var_name));
                Ok(ValueType::Bool)
            }
        }
    }

    /// Check whether `name` refers to a global array variable (not in locals).
    ///
    /// **Invariant**: When a `StoreArray(name)` instruction is emitted inside a function
    /// body, the slotization pass (`vm/slot.rs`) allocates a local slot for `name` and
    /// rewrites every `LoadArray(name)` → `LoadSlot(slot)`. For global arrays this is
    /// wrong: the slot starts uninitialized and the first `LoadSlot` raises `UndefVarError`.
    ///
    /// Therefore, **never emit `StoreArray(name)` for global arrays**. Use the helpers
    /// [`compile_store_and_reload_array`] / [`compile_store_or_pop_global_array`] which
    /// automatically suppress `StoreArray` for globals. (Issue #3121 / #3127)
    pub(crate) fn is_global_array(&self, name: &str) -> bool {
        !self.locals.contains_key(name) && self.shared_ctx.global_types.contains_key(name)
    }

    /// Emit `StoreArray(name) + LoadArray(name)` for local arrays; do nothing for globals.
    ///
    /// Use after *push-type* mutation instructions (`ArrayPush`, `ArrayPushFirst`,
    /// `ArrayInsert`, `ArrayDeleteAt`) where the mutated array is on top of the stack and
    /// the caller expects the modified array to remain on the stack as the expression value.
    ///
    /// Stack before: `[..., modified_arr]`
    /// Stack after:  `[..., modified_arr]`  (locals: stored; globals: unchanged)
    pub(crate) fn compile_store_and_reload_array(&mut self, name: &str) {
        if !self.is_global_array(name) {
            self.emit(Instr::StoreArray(name.to_string()));
            self.emit(Instr::LoadArray(name.to_string()));
        }
        // For globals: arr is Arc-ref-counted and already mutated in place; it stays on stack.
    }

    /// Emit `Pop` for global arrays or `StoreArray(name)` for local arrays.
    ///
    /// Use after *pop-type* mutation instructions (`ArrayPop`, `ArrayPopFirst`) **after**
    /// the `Swap` that puts `[value, modified_arr]` → `[modified_arr, value]` on the stack.
    /// Wait — actually after `Swap` the stack is `[..., value, modified_arr]`, so we need
    /// to dispose of `modified_arr`:
    ///
    /// - Global: `Pop` discards `modified_arr` (in-place mutation already done).
    /// - Local:  `StoreArray(name)` saves it back and leaves `value` on top.
    ///
    /// Stack before: `[..., value, modified_arr]`  (after Swap)
    /// Stack after:  `[..., value]`
    pub(crate) fn compile_store_or_pop_global_array(&mut self, name: &str) {
        if self.is_global_array(name) {
            self.emit(Instr::Pop);
        } else {
            self.emit(Instr::StoreArray(name.to_string()));
        }
    }
}
