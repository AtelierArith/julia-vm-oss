//! Builtin function compilation.
//!
//! Handles compilation of Julia builtin functions and operators.
//! This module is organized by function category:
//! - I/O functions (println, print, error)
//! - Math functions (sqrt, sin, cos, exp, log, etc.)
//! - Array functions (length, sum, etc.)
//! - String functions (uppercase, lowercase, etc.)
//! - Type functions (typeof, isa, convert, etc.)

use crate::builtins::BuiltinId;
use crate::inference_core::{CoreAbstract, CorePrimitive, CoreType};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Function, Literal, Stmt, UnaryOp};
use crate::types::JuliaType;
use crate::vm::value::{ArrayElementType, GeneratorCallable, Value};
use crate::vm::{DynamicCallCandidate, Instr, NativeIteratorKind, ValueType};

use super::super::{err, CResult, CompileError, CoreCompiler};
use crate::compile::inference::promote_numeric_value_types;

fn unary_float_preserving_result_type(arg: ValueType) -> ValueType {
    match arg {
        ValueType::F16 | ValueType::F32 | ValueType::F64 => arg,
        ValueType::Any => ValueType::Any,
        _ => ValueType::F64,
    }
}

impl CoreCompiler<'_> {
    /// After an explicit-RNG instruction (e.g. `RngRandF64`, `RngRandArrayF64`)
    /// the mutated RNG is left on top of the stack above the result. If the RNG
    /// argument was a plain variable, store the advanced state back so repeated
    /// `rand(rng)` calls keep progressing the stream; otherwise discard it.
    fn store_rng_back(&mut self, rng_arg: &Expr) {
        if let Expr::Var(name, _) = rng_arg {
            self.emit(Instr::StoreRng(name.clone()));
        } else {
            self.emit(Instr::Pop);
        }
    }

    /// For a single-argument `rand(x)` / `randn(x)` whose argument type is
    /// statically `Any`, return `Some(write_back)` describing the runtime-branch
    /// (`RandArg` / `RandnArg`) case: `x` may be a `Value::Rng` at runtime, so we
    /// cannot statically choose between scalar-from-rng and `rand(n)` forms. The
    /// inner `Option<String>` is the local-variable name to write the advanced
    /// RNG state back to (when `x` is a plain variable). Returns `None` when the
    /// argument has a known static type (handled by the typed paths) or when this
    /// is not a single-argument call (Issue #7231).
    fn untyped_rng_arg_write_back(&mut self, args: &[Expr]) -> Option<Option<String>> {
        if args.len() != 1 {
            return None;
        }
        if self.infer_expr_type(&args[0]) != ValueType::Any {
            return None;
        }
        // A literal type identifier (`rand(Int)`) is not a runtime RNG; leave it
        // to the type-identifier path.
        if let Expr::Var(name, _) = &args[0] {
            if matches!(name.as_str(), "Int" | "Int64" | "Float64") {
                return None;
            }
            return Some(Some(name.clone()));
        }
        Some(None)
    }

    /// `rand` / `randn` are compiled as builtins, which normally bypasses the
    /// method tables. When a user has defined a `rand(d::SomeDist)` /
    /// `randn(...)` method whose signature matches the actual argument types
    /// (e.g. a `Distribution` value), route the call to that user method so
    /// `rand(d)`, `rand(rng, d)`, `rand(d, n)` etc. dispatch correctly instead
    /// of misinterpreting the struct argument as an array dimension (Issue
    /// #7178). Returns `None` when no matching user method exists, leaving the
    /// native global-RNG builtin path in place.
    fn try_dispatch_user_rand_method(
        &mut self,
        names: &[&str],
        args: &[Expr],
    ) -> Option<CResult<ValueType>> {
        if args.is_empty() {
            return None;
        }
        let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
        if !self.has_user_dispatch_method_for_arg_types(names, &arg_types) {
            return None;
        }
        // Resolve the concrete method up front (clone out of the borrowed table)
        // so the borrow ends before we mutate `self` to compile the arguments.
        let resolved = names.iter().find_map(|name| {
            self.method_tables
                .get(*name)
                .and_then(|table| table.dispatch(&arg_types).ok())
                .map(|method| (method.global_index, method.return_type.clone()))
        });
        let (global_index, return_type) = resolved?;
        for arg in args {
            if let Err(e) = self.compile_expr(arg) {
                return Some(Err(e));
            }
        }
        self.emit(Instr::Call(global_index, args.len()));
        Some(Ok(return_type))
    }

    fn compile_time_isa_result(&self, args: &[Expr]) -> Option<bool> {
        if args.len() != 2 {
            return None;
        }

        let Expr::Var(var_name, _) = &args[0] else {
            return None;
        };

        // Abstract-numeric params (`x::Number`/`x::Real`/`x::Integer`) are widened
        // to a *representational* ValueType in `self.locals` — `F64` for
        // Number/Real, `I64` for Integer (`type_helpers.rs`,
        // `JuliaType::Number => ValueType::F64`) — that does NOT reflect the
        // concrete runtime type (the value loads via `LoadAny`). Folding `isa`
        // on that representational type is unsound: an Int64 bound to `x::Number`
        // (static `F64`) would wrongly fold `x isa Int64` to `false`, skipping
        // the guarded branch (Issue #5941). Defer all `isa` decisions for these
        // params to the runtime check, which sees the true value type.
        if self.abstract_numeric_params.contains(var_name.as_str()) {
            return None;
        }

        let Expr::Var(type_name, _) = &args[1] else {
            return None;
        };

        let target = super::super::narrowing::value_type_for_type_name(type_name, |name| {
            self.shared_ctx.get_struct_type_id(name)
        })?;
        let current = self.locals.get(var_name)?;

        if current == &target {
            return Some(true);
        }
        exact_isa_codegen_type(current).then_some(false)
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
        match name {
            // I/O functions delegated to builtin_io.rs
            "println" | "print" | "error" | "throw" | "rethrow" | "IOBuffer" | "take!"
            | "takestring!" | "write" => err(format!(
                "I/O function {} should be handled by builtin_io",
                name
            )),
            // Math functions delegated to builtin_math.rs
            // Note: nextfloat/prevfloat/exponent/significand/frexp/issubnormal removed —
            // now Pure Julia (base/float.jl, Issue #6740).
            "rand" | "sqrt" | "sdiv_int" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan"
            | "exp" | "log" | "floor" | "ceil" | "round" | "trunc" | "sleep" | "_ctpop_int"
            | "_ctlz_int" | "_cttz_int" | "_bitreverse_int" | "_bswap_int" | "_fma" => err(
                format!("Math function {} should be handled by builtin_math", name),
            ),
            // String functions delegated to builtin_string.rs
            // Note: startswith, endswith are now Pure Julia (base/strings.jl)
            "uppercase" | "lowercase" | "titlecase" | "ncodeunits" | "codeunit" | "codeunits"
            | "repeat" | "split" | "join" | "string" | "repr" | "sprintf" => err(format!(
                "String function {} should be handled by builtin_string",
                name
            )),
            // Type constructors delegated to builtin_types.rs
            "Bool" | "Char" | "Int" | "BigInt" | "BigFloat" | "Int8" | "Int16" | "Int32"
            | "Int64" | "Int128" | "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128"
            | "Float16" | "Float32" | "Float64" => err(format!(
                "Type constructor {} should be handled by builtin_types",
                name
            )),
            // Array functions delegated to builtin_array.rs
            "length" | "getindex" | "setindex!" => err(format!(
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
            "TypeVar" => {
                // TypeVar(name[, lb, ub]) - fresh runtime TypeVar object.
                // Mirrors upstream Core.TypeVar constructor shape.
                if args.len() != 1 && args.len() != 3 {
                    return err("TypeVar requires 1 or 3 arguments");
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::TypeVar, args.len()));
                Ok(ValueType::DataType)
            }
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
            // Issue #3727: `signed` / `unsigned` are routed through Pure Julia
            // method dispatch first (base/number.jl). When dispatch fails for
            // a type without a Pure Julia method (e.g. Float64), the
            // is_base_function fallback reaches this handler so the legacy
            // Rust BuiltinId::Signed / BuiltinId::Unsigned semantics are
            // preserved for back-compat with existing fixtures.
            "signed" => {
                if args.len() != 1 {
                    return err("signed requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Signed, 1));
                Ok(ValueType::I64)
            }
            "unsigned" => {
                if args.len() != 1 {
                    return err("unsigned requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::Unsigned, 1));
                Ok(ValueType::I64)
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
            "memoryref" | "memoryrefnew" => {
                if !(1..=3).contains(&args.len()) {
                    return err("memoryref requires 1 to 3 arguments");
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::MemoryRefNew, args.len()));
                Ok(ValueType::Any)
            }
            "memoryrefget" => {
                if args.is_empty() {
                    return err("memoryrefget requires at least 1 argument");
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::MemoryRefGet, args.len()));
                Ok(ValueType::Any)
            }
            "memoryrefset!" => {
                if args.len() < 2 {
                    return err("memoryrefset! requires at least 2 arguments");
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::MemoryRefSet, args.len()));
                Ok(ValueType::Any)
            }
            "memoryrefoffset" => {
                if args.len() != 1 {
                    return err("memoryrefoffset requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MemoryRefOffset, 1));
                Ok(ValueType::I64)
            }
            "memoryrefparent" => {
                if args.len() != 1 {
                    return err("memoryrefparent requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::MemoryRefParent, 1));
                Ok(ValueType::Memory)
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
            }
            _ => {
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
                .find(|m| m.param_count() == preferred_arity)
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
                        .find(|m| m.param_count() == preferred_arity)
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
    ///
    /// No longer used after Issue #3731 migrated `mapreduce` / `mapfoldl` /
    /// `mapfoldr` to Pure Julia method dispatch. Kept (with `#[allow(dead_code)]`)
    /// as a small utility for future HOF return-type inference work.
    #[allow(dead_code)]
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
                if let Some(result) =
                    self.try_dispatch_user_rand_method(&["rand", "Base.rand"], args)
                {
                    return result;
                }
                let candidates =
                    self.user_runtime_candidates_for_names(&["rand", "Base.rand"], args.len());
                let needs_runtime_dispatch = args.iter().any(|arg| {
                    matches!(self.infer_expr_type(arg), ValueType::Any)
                        || matches!(self.infer_julia_type(arg), JuliaType::Struct(_))
                });
                if !args.is_empty() && needs_runtime_dispatch && !candidates.is_empty() {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::CallTypedDispatchOrBuiltin(
                        BuiltinId::Rand,
                        "rand".to_string(),
                        args.len(),
                        candidates,
                    ));
                    return Ok(ValueType::Any);
                }
                if args.is_empty() {
                    self.emit(Instr::RandF64);
                    Ok(ValueType::F64)
                } else if self.infer_expr_type(&args[0]) == ValueType::Rng {
                    // rand(rng), rand(rng, dims...), rand(rng, Int, dims...) - explicit RNG
                    let rest = &args[1..];
                    let (dims, is_int_array) = match rest.first() {
                        Some(Expr::Var(name, _)) if name == "Int" || name == "Int64" => {
                            (&rest[1..], true)
                        }
                        Some(Expr::Var(name, _)) if name == "Float64" => (&rest[1..], false),
                        _ => (rest, false),
                    };
                    self.compile_expr(&args[0])?; // push Rng
                    if dims.is_empty() {
                        // scalar rand(rng) -> F64
                        self.emit(Instr::RngRandF64);
                        self.store_rng_back(&args[0]);
                        Ok(ValueType::F64)
                    } else {
                        for dim in dims {
                            self.compile_expr_as(dim, ValueType::I64)?;
                        }
                        if is_int_array {
                            self.emit(Instr::RngRandArrayI64(dims.len()));
                        } else {
                            self.emit(Instr::RngRandArrayF64(dims.len()));
                        }
                        self.store_rng_back(&args[0]);
                        Ok(ValueType::Array)
                    }
                } else if let Some(write_back) = self.untyped_rng_arg_write_back(args) {
                    // rand(x) where x is statically untyped (ValueType::Any): the
                    // value may be a Value::Rng at runtime. Emit RandArg which
                    // branches at runtime between scalar-from-rng and rand(n)
                    // (Issue #7231). Return type is Any (scalar F64 or vector).
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::RandArg(write_back));
                    Ok(ValueType::Any)
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
                    let arg_ty = self.compile_expr(&args[0])?;
                    self.emit(Instr::SqrtF64);
                    Ok(unary_float_preserving_result_type(arg_ty))
                }
            }
            BuiltinOp::Zeros => self.compile_call("zeros", args, &[], &[], &[]),
            BuiltinOp::Ones => self.compile_call("ones", args, &[], &[], &[]),
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
                        // Array: load array, push value, store back.
                        // Issue #3548: only coerce to F64 when the array is a
                        // legacy/F64 storage array. For typed integer/float
                        // arrays (`Int32[]`, `UInt8[]`, …) we must push the
                        // value at its declared type so the storage match in
                        // `ArrayData::push` succeeds.
                        // Issue #5717: an `Any` array stores values verbatim in a
                        // boxed slot, so an integer pushed into `Any[]` must NOT be
                        // widened to Float64 either — only F64-backed (and legacy
                        // untyped) arrays coerce.
                        self.load_local(name)?;
                        let val_ty = self.compile_expr(&args[1])?;
                        let array_stores_verbatim = matches!(
                            self.locals.get(name),
                            Some(ValueType::ArrayOf(elem, _))
                                if !matches!(elem, ArrayElementType::F64)
                        );
                        if !array_stores_verbatim {
                            match val_ty {
                                ValueType::I64 | ValueType::I32 | ValueType::F32 => {
                                    self.emit(Instr::ToF64);
                                }
                                _ => {}
                            }
                        }
                        self.emit(Instr::ArrayPush);
                        // StoreArray is suppressed for globals (Issue #3121, #3127)
                        self.compile_store_and_reload_array(name);
                        Ok(ValueType::Array)
                    }
                } else {
                    // Non-variable receiver (literal, `collect(...)`, a Set/Array
                    // expression): mutate the value on the stack and return it; there
                    // is no binding to store back to (Issue #5674).
                    let recv_ty = self.infer_expr_type(&args[0]);
                    if recv_ty == ValueType::Set {
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::SetAdd);
                        Ok(ValueType::Set)
                    } else {
                        self.compile_expr(&args[0])?;
                        let val_ty = self.compile_expr(&args[1])?;
                        // Mirror the Var path's coercion (Issues #3548, #5717): only
                        // F64-backed / legacy arrays widen integers to Float64.
                        let array_stores_verbatim = matches!(
                            recv_ty,
                            ValueType::ArrayOf(ref elem, _) if !matches!(elem, ArrayElementType::F64)
                        );
                        if !array_stores_verbatim {
                            match val_ty {
                                ValueType::I64 | ValueType::I32 | ValueType::F32 => {
                                    self.emit(Instr::ToF64);
                                }
                                _ => {}
                            }
                        }
                        self.emit(Instr::ArrayPush);
                        Ok(ValueType::Array)
                    }
                }
            }
            BuiltinOp::Pop => {
                // pop!(arr) for arrays - 1 argument
                // pop!(dict, key) or pop!(dict, key, default) for dicts - 2 or 3 arguments
                match args.len() {
                    1 => {
                        // Array pop: pop!(arr)
                        if let Expr::Var(name, _) = &args[0] {
                            let popped_ty = self.array_pop_result_type(&args[0]);
                            // Load the array, pop the value, store back
                            self.load_local(name)?;
                            self.emit(Instr::ArrayPop);
                            // ArrayPop leaves: [modified_array, popped_value] on stack
                            // Swap so we have: [popped_value, modified_array]
                            self.emit(Instr::Swap);
                            // StoreArray is suppressed for globals (Issue #3121, #3127)
                            self.compile_store_or_pop_global_array(name);
                            // Now popped_value is on top of stack
                            Ok(popped_ty)
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
                    self.load_local(name)?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::ArrayPushFirst);
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_and_reload_array(name);
                    Ok(ValueType::Array)
                } else {
                    // Non-variable array expression (Issue #5674): mutate on the stack.
                    self.compile_expr(&args[0])?; // array
                    self.compile_expr(&args[1])?; // value
                    self.emit(Instr::ArrayPushFirst);
                    Ok(ValueType::Array)
                }
            }
            BuiltinOp::PopFirst => {
                // popfirst!(arr)
                if args.len() != 1 {
                    return err("popfirst! requires exactly 1 argument: popfirst!(arr)");
                }
                if let Expr::Var(name, _) = &args[0] {
                    let popped_ty = self.array_pop_result_type(&args[0]);
                    self.load_local(name)?;
                    self.emit(Instr::ArrayPopFirst);
                    // ArrayPopFirst leaves: [modified_array, popped_value] on stack
                    self.emit(Instr::Swap);
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_or_pop_global_array(name);
                    Ok(popped_ty)
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
                    self.load_local(name)?;
                    self.compile_expr_as(&args[1], ValueType::I64)?; // index
                    self.compile_expr(&args[2])?; // value
                    self.emit(Instr::ArrayInsert);
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_and_reload_array(name);
                    Ok(ValueType::Array)
                } else {
                    // Non-variable array expression (literal, `collect(...)`, …):
                    // mutate the array value on the stack and return it. The mutation
                    // instruction leaves the modified array on the stack, and there is
                    // no binding to store back to (Issue #5674).
                    self.compile_expr(&args[0])?; // array
                    self.compile_expr_as(&args[1], ValueType::I64)?; // index
                    self.compile_expr(&args[2])?; // value
                    self.emit(Instr::ArrayInsert);
                    Ok(ValueType::Array)
                }
            }
            BuiltinOp::DeleteAt => {
                // deleteat!(arr, i) or deleteat!(arr, inds) where `inds` is a
                // Vector/Range of indices (Issue #5738).
                if args.len() != 2 {
                    return err("deleteat! requires exactly 2 arguments: deleteat!(arr, i)");
                }
                // A collection index (Vector/Range) deletes multiple positions;
                // a scalar index deletes a single position.
                let index_is_collection = matches!(
                    self.infer_expr_type(&args[1]),
                    ValueType::Array | ValueType::ArrayOf(_, _) | ValueType::Range
                );
                if let Expr::Var(name, _) = &args[0] {
                    self.load_local(name)?;
                    if index_is_collection {
                        self.compile_expr(&args[1])?; // indices collection
                        self.emit(Instr::ArrayDeleteAtIndices);
                    } else {
                        self.compile_expr_as(&args[1], ValueType::I64)?; // index
                        self.emit(Instr::ArrayDeleteAt);
                    }
                    // StoreArray is suppressed for globals (Issue #3121, #3127)
                    self.compile_store_and_reload_array(name);
                    Ok(ValueType::Array)
                } else {
                    // Non-variable array expression (Issue #5674): mutate on the stack.
                    self.compile_expr(&args[0])?; // array
                    if index_is_collection {
                        self.compile_expr(&args[1])?; // indices collection
                        self.emit(Instr::ArrayDeleteAtIndices);
                    } else {
                        self.compile_expr_as(&args[1], ValueType::I64)?; // index
                        self.emit(Instr::ArrayDeleteAt);
                    }
                    Ok(ValueType::Array)
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
            BuiltinOp::MersenneTwisterRNG => {
                // MersenneTwister(seed) - create MT19937-64 RNG instance.
                // Backed by a deterministic MT19937-64 engine; the generated
                // stream is NOT bit-identical to upstream's dSFMT (Issue #7306).
                if args.len() != 1 {
                    return err("MersenneTwister requires exactly one argument (seed)");
                }
                self.compile_expr_as(&args[0], ValueType::I64)?;
                self.emit(Instr::NewMersenne);
                Ok(ValueType::Rng)
            }
            BuiltinOp::Randn => {
                // randn() or randn(rng) - standard normal distribution
                if let Some(result) =
                    self.try_dispatch_user_rand_method(&["randn", "Base.randn"], args)
                {
                    return result;
                }
                if args.is_empty() {
                    // randn() - use global RNG
                    self.emit(Instr::RandnF64);
                    Ok(ValueType::F64)
                } else {
                    // randn(rng) or randn(rng, dims...) - use provided RNG
                    // First check if first arg is an RNG
                    let first_ty = self.infer_expr_type(&args[0]);
                    if first_ty == ValueType::Rng {
                        let rest = &args[1..];
                        self.compile_expr(&args[0])?; // push Rng
                        if rest.is_empty() {
                            self.emit(Instr::RngRandnF64);
                            self.store_rng_back(&args[0]);
                            Ok(ValueType::F64)
                        } else {
                            for dim in rest {
                                self.compile_expr_as(dim, ValueType::I64)?;
                            }
                            self.emit(Instr::RngRandnArrayF64(rest.len()));
                            self.store_rng_back(&args[0]);
                            Ok(ValueType::Array)
                        }
                    } else if let Some(write_back) = self.untyped_rng_arg_write_back(args) {
                        // randn(x) where x is statically untyped (ValueType::Any):
                        // the value may be a Value::Rng at runtime. Emit RandnArg
                        // which branches at runtime between scalar-from-rng and
                        // randn(n) (Issue #7231).
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::RandnArg(write_back));
                        Ok(ValueType::Any)
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
                // get!(dict, key, default) - get value or insert default (Issue #5225)
                if args.len() != 3 {
                    return err("get! requires exactly 3 arguments: get!(dict, key, default)");
                }
                self.compile_expr(&args[0])?;
                self.compile_expr(&args[1])?;
                self.compile_expr(&args[2])?;
                // DictGetBang leaves [modified_dict, result] on the stack. When the dict is a
                // bound variable, store the mutated dict back so the inserted entry persists;
                // the Result variant keeps only the value. Mirrors the call/mod.rs `get!` arm.
                if let Expr::Var(name, _) = &args[0] {
                    self.emit(Instr::CallTypedDispatchOrBuiltinStoreDictResult(Box::new(
                        crate::vm::TypedDispatchStoreDict {
                            builtin: BuiltinId::DictGetBang,
                            function_name: "get!".to_string(),
                            arg_count: 3,
                            candidates: Vec::new(),
                            store_local: name.clone(),
                        },
                    )));
                } else {
                    self.emit(Instr::CallTypedDispatchOrBuiltinResult(
                        BuiltinId::DictGetBang,
                        "get!".to_string(),
                        3,
                        Vec::new(),
                    ));
                }
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
                // `Value::Dict`/`Value::Set` carriers were removed (Issues
                // #6731/#6732); empty! on a pure Dict{K,V}/Set{T} struct dispatches
                // to its method via the DictEmpty trampoline (which mutates the
                // struct in place — no carrier load/store-back needed).
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::DictEmpty, 1));
                Ok(ValueType::Any)
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
                if let Some(result) = self.compile_time_isa_result(args) {
                    self.emit(Instr::PushBool(result));
                    return Ok(ValueType::Bool);
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
            // BuiltinOp::Isbits removed - pure Julia (Issue #6738)
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
                // _supertype(T) - internal intrinsic for Pure Julia supertype()
                if args.len() != 1 {
                    return err("_supertype requires exactly 1 argument: _supertype(Type)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::_Supertype, 1));
                Ok(ValueType::DataType)
            }
            BuiltinOp::Typename => {
                // _typename(T) - internal intrinsic for Pure Julia nameof(::Type)
                // and Base.typename (Issue #5106). Returns the canonical TypeName
                // symbol.
                if args.len() != 1 {
                    return err("_typename requires exactly 1 argument: _typename(Type)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::_Typename, 1));
                Ok(ValueType::Symbol)
            }
            BuiltinOp::FunctionName => {
                // _function_name(f) - internal intrinsic for Pure Julia
                // nameof(::Function). Avoids parsing string(f), whose string
                // slicing path regressed for user-defined functions (Issue #5580).
                if args.len() != 1 {
                    return err("_function_name requires exactly 1 argument: _function_name(f)");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::_FunctionName, 1));
                Ok(ValueType::Symbol)
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
            // BuiltinOp::Typeintersect/Typejoin removed - now Pure Julia (base/reflection.jl)
            // BuiltinOp::Fieldcount removed - now Pure Julia (base/reflection.jl)
            // BuiltinOp::Hasfield removed - pure Julia (Issue #6738)
            // BuiltinOp::Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype, Ismutabletype
            // removed - now Pure Julia (base/reflection.jl)
            // BuiltinOp::Ismutable removed - pure Julia (Issue #6738)
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
                // _methods_by_ftype(f) or _methods_by_ftype(f, types)
                // Internal intrinsic used by Pure Julia methods/which wrappers.
                if args.len() == 1 {
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::_MethodsByFtype, 1));
                } else if args.len() == 2 {
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::_MethodsByFtype, 2));
                } else {
                    return err(
                        "_methods_by_ftype requires 1 or 2 arguments: _methods_by_ftype(f) or _methods_by_ftype(f, types)",
                    );
                }
                Ok(ValueType::Array) // Returns Vector{Method}
            }
            BuiltinOp::HasMethod => {
                // hasmethod(f, types) or hasmethod(f, types, kwnames)
                if !(args.len() == 2 || args.len() == 3) {
                    return err(
                        "hasmethod requires 2 or 3 arguments: hasmethod(f, types[, kwnames])",
                    );
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::HasMethod, args.len()));
                Ok(ValueType::Bool)
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
                            // Build candidates list from iterate methods with matching arg
                            // count. The runtime derives each candidate's per-arity signature
                            // from its FunctionInfo, so 2-arg iterate(collection, state)
                            // dispatch still scores the state type (Issue #3910, #6336).
                            let candidates: Vec<usize> = table
                                .methods
                                .iter()
                                .filter(|m| m.param_count() == args.len())
                                .filter_map(|m| {
                                    Self::method_first_param_matches(
                                        m,
                                        Self::core_is_runtime_iterate_candidate_type,
                                    )
                                    .then_some(m.global_index)
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
                                if m.param_count() != args.len() {
                                    return false;
                                }
                                // Check first parameter is the correct struct
                                // type, read from the core_signature
                                // projection (Issue #6495, stage 6b; the
                                // legacy `params` fallback was retired at
                                // stage 7c-ii — a test-only Bottom
                                // placeholder never matches).
                                let param_base = m
                                    .structured_arg_core_types()
                                    .and_then(|cores| cores.first())
                                    .and_then(Self::core_param_struct_base);
                                param_base == Some(struct_base)
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
                            // Build candidates list from iterate methods with matching arg
                            // count. The runtime derives each candidate's per-arity signature
                            // from its FunctionInfo, so 2-arg iterate(collection, state)
                            // dispatch still scores the state type (Issue #3910, #6336).
                            let candidates: Vec<usize> = table
                                .methods
                                .iter()
                                .filter(|m| m.param_count() == args.len())
                                .filter_map(|m| {
                                    Self::method_first_param_matches(
                                        m,
                                        Self::core_is_runtime_iterate_candidate_type,
                                    )
                                    .then_some(m.global_index)
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

                let has_user_generator_collect_method = self.has_user_generator_collect_method();
                let has_user_range_collect_method =
                    self.has_user_range_collect_method_for_builtin();
                // For struct types or Any type, prefer Pure Julia collect
                // which uses the iterate protocol
                let arg_value_ty = self.infer_expr_type(&args[0]);
                let arg_ty = self.infer_julia_type(&args[0]);
                if matches!(
                    arg_ty,
                    JuliaType::Struct(_)
                        | JuliaType::Array
                        | JuliaType::VectorOf(_)
                        | JuliaType::MatrixOf(_)
                        | JuliaType::AbstractArray
                        | JuliaType::Any
                        | JuliaType::Generator
                        | JuliaType::UnitRange
                        | JuliaType::StepRange
                        | JuliaType::AbstractRange
                ) {
                    let collect_tables = ["collect", "Base.collect"]
                        .iter()
                        .filter_map(|name| self.method_tables.get(*name))
                        .collect::<Vec<_>>();
                    if !collect_tables.is_empty() {
                        // Issue #3648/#4056/#4061/#4068: Route matching
                        // runtime values to safe typed collect overloads. VM-native
                        // `Value::Generator` values use a sentinel candidate so the
                        // dynamic dispatcher can enter the existing RangeCollect /
                        // collect_generator representation boundary, while keeping
                        // the generic `collect(::Any)` fallback for custom iterators.
                        //
                        // We only narrow dispatch to candidates whose bodies are valid
                        // for runtime values here. VM-native `Value::Range` used to make
                        // struct-backed range candidates unsafe because it reports a
                        // Pure-Julia parametric type name even though it is not a struct.
                        // The CallDynamic collect handler now routes `Value::Range`
                        // through the RangeCollect boundary before candidate scoring
                        // (Issues #4075/#4078), so real LinRange/StepRangeLen structs
                        // can use their Pure Julia collect methods.
                        let mut collect_candidates: Vec<DynamicCallCandidate> = collect_tables
                            .iter()
                            .flat_map(|table| table.methods.iter())
                            .filter(|m| {
                                if m.param_count() != 1 {
                                    return false;
                                }
                                let is_user_range_method = self.is_user_range_collect_method(m);
                                if has_user_range_collect_method
                                    && Self::method_first_param_matches(
                                        m,
                                        Self::core_is_range_collect_signature_type,
                                    )
                                    && !is_user_range_method
                                {
                                    return false;
                                }
                                Self::method_first_param_matches(
                                    m,
                                    Self::core_is_runtime_collect_candidate_type,
                                ) || is_user_range_method
                                    || self.is_user_generator_collect_method(m)
                            })
                            .map(|m| DynamicCallCandidate::Method(m.global_index))
                            .collect();
                        collect_candidates.extend(collect_tables.iter().flat_map(|table| {
                            table.methods.iter().filter_map(|m| {
                                (m.param_count() == 1
                                    && Self::method_first_param_matches(m, |core| {
                                        matches!(
                                            core,
                                            CoreType::Struct { name, params }
                                                if name == "Tuple" && params.is_empty()
                                        )
                                    }))
                                .then_some(DynamicCallCandidate::Method(m.global_index))
                            })
                        }));
                        collect_candidates.extend(
                            [
                                NativeIteratorKind::Zip,
                                NativeIteratorKind::Zip3,
                                NativeIteratorKind::Zip4,
                                NativeIteratorKind::Zip5,
                                NativeIteratorKind::Zip6,
                                NativeIteratorKind::Zip7,
                                NativeIteratorKind::Generator,
                            ]
                            .map(DynamicCallCandidate::NativeIterator),
                        );
                        if let Some(fallback) = collect_tables
                            .iter()
                            .flat_map(|table| table.methods.iter())
                            .find(|m| {
                                m.param_count() == 1
                                    && Self::method_first_param_matches(m, |core| {
                                        matches!(core, CoreType::Any)
                                    })
                            })
                        {
                            if !collect_candidates.is_empty() {
                                self.compile_expr(&args[0])?;
                                self.emit(Instr::CallDynamic(
                                    fallback.global_index,
                                    1,
                                    collect_candidates,
                                ));
                                let return_type = if !has_user_generator_collect_method
                                    && (matches!(arg_value_ty, ValueType::Generator)
                                        || matches!(arg_ty, JuliaType::Generator))
                                {
                                    ValueType::Array
                                } else {
                                    ValueType::Any
                                };
                                return Ok(return_type);
                            }
                            // No typed Array overload — preserve historical behavior
                            // by calling the generic `collect(::Any)` directly.
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::Call(fallback.global_index, 1));
                            return Ok(ValueType::Any);
                        }
                    }
                }

                // Fall back to builtin for basic types (Array, Range, Tuple)
                // CollectFallback: builtin-rangecollect-final-boundary
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::RangeCollect, 1));

                // Infer element type from argument for proper Vector{T} dispatch
                // Ranges produce Int64 elements, other types produce generic Array
                let result_type = match arg_ty {
                    JuliaType::UnitRange | JuliaType::StepRange => {
                        ValueType::ArrayOf(ArrayElementType::I64, None)
                    }
                    JuliaType::VectorOf(ref elem) => {
                        // Preserve element type for collect on vectors
                        match elem.as_ref() {
                            JuliaType::Int64 => ValueType::ArrayOf(ArrayElementType::I64, None),
                            JuliaType::Float64 => ValueType::ArrayOf(ArrayElementType::F64, None),
                            JuliaType::Bool => ValueType::ArrayOf(ArrayElementType::Bool, None),
                            JuliaType::String => ValueType::ArrayOf(ArrayElementType::String, None),
                            JuliaType::Char => ValueType::ArrayOf(ArrayElementType::Char, None),
                            _ => ValueType::Array,
                        }
                    }
                    _ => ValueType::Array,
                };
                Ok(result_type)
            }
            BuiltinOp::Generator => {
                if args.len() < 2 {
                    return err("Generator requires at least 2 arguments: Generator(f, iter)");
                }
                let result_element_type = if args.len() == 2 {
                    self.infer_generator_default_eltype(&args[0], &args[1])
                } else {
                    self.infer_generator_tuple_splat_default_eltype(&args[0], &args[1..])
                };
                let callable = if let Some(julia_type) = Self::generator_type_object(&args[0]) {
                    if args.len() > 2 {
                        GeneratorCallable::TupleSplatRuntimeValue(Box::new(Value::DataType(
                            Box::new(julia_type),
                        )))
                    } else {
                        GeneratorCallable::RuntimeValue(Box::new(Value::DataType(Box::new(
                            julia_type,
                        ))))
                    }
                } else if matches!(&args[0], Expr::Var(name, _) if self.locals.contains_key(name)) {
                    self.compile_expr(&args[0])?;
                    if args.len() == 2 {
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::MakeGeneratorRuntime(false, result_element_type));
                    } else {
                        let zip_args = args[1..].to_vec();
                        let zip_arg_count = zip_args.len();
                        let zip_expr = Expr::Call {
                            function: "zip".to_string(),
                            args: zip_args,
                            kwargs: vec![],
                            splat_mask: vec![false; zip_arg_count],
                            kwargs_splat_mask: vec![],
                            span: args[1].span(),
                        };
                        self.compile_expr(&zip_expr)?;
                        self.emit(Instr::MakeGeneratorRuntime(true, result_element_type));
                    }
                    return Ok(ValueType::Generator);
                } else if args.len() > 2 {
                    if let Ok(func_index) =
                        self.resolve_function_ref_with_arity(&args[0], args.len() - 1)
                    {
                        GeneratorCallable::TupleSplatFunctionIndex(func_index)
                    } else {
                        self.compile_expr(&args[0])?;
                        let zip_args = args[1..].to_vec();
                        let zip_arg_count = zip_args.len();
                        let zip_expr = Expr::Call {
                            function: "zip".to_string(),
                            args: zip_args,
                            kwargs: vec![],
                            splat_mask: vec![false; zip_arg_count],
                            kwargs_splat_mask: vec![],
                            span: args[1].span(),
                        };
                        self.compile_expr(&zip_expr)?;
                        self.emit(Instr::MakeGeneratorRuntime(true, result_element_type));
                        return Ok(ValueType::Generator);
                    }
                } else if let Ok(func_index) = self.resolve_function_ref(&args[0]) {
                    GeneratorCallable::FunctionIndex(func_index)
                } else {
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::MakeGeneratorRuntime(false, result_element_type));
                    return Ok(ValueType::Generator);
                };
                if args.len() == 2 {
                    self.compile_expr(&args[1])?;
                } else {
                    let zip_args = args[1..].to_vec();
                    let zip_arg_count = zip_args.len();
                    let zip_expr = Expr::Call {
                        function: "zip".to_string(),
                        args: zip_args,
                        kwargs: vec![],
                        splat_mask: vec![false; zip_arg_count],
                        kwargs_splat_mask: vec![],
                        span: args[1].span(),
                    };
                    self.compile_expr(&zip_expr)?;
                }
                self.emit(Instr::MakeGenerator(Box::new(
                    crate::vm::MakeGeneratorOperands {
                        callable,
                        result_element_type,
                    },
                )));
                Ok(ValueType::Generator)
            }
            BuiltinOp::SymbolNew => {
                // Symbol("name") - create a symbol from a string.
                // Symbol(a, b, c, ...) - concatenate string forms of all
                // arguments and form a single Symbol (Issue #4780).
                // Mirrors upstream Julia's `Base.Symbol(args...) =
                // Symbol(string(args...))`.
                if args.is_empty() {
                    return err("Symbol requires at least 1 argument: Symbol(name, ...)");
                }
                for arg in args.iter() {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallBuiltin(BuiltinId::SymbolNew, args.len()));
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
            BuiltinOp::GeneratedEval => {
                if args.len() != 1 {
                    return err("_generated_eval requires exactly 1 argument");
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(BuiltinId::GeneratedEval, 1));
                Ok(ValueType::Any)
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

    /// Infer the `@default_eltype` equivalent for `Base.Generator(f, iter)`.
    ///
    /// Upstream Julia computes this in `julia/base/array.jl` before reading the
    /// first item, so empty generator collection can still return
    /// `Vector{return_type(f(::eltype(iter)))}`. sjulia keeps the existing
    /// VM-native Generator boundary, but stores this metadata on the generator
    /// value for the empty `collect` branch.
    fn infer_generator_default_eltype(
        &mut self,
        func_arg: &Expr,
        iter_arg: &Expr,
    ) -> Option<ArrayElementType> {
        if let Some(julia_type) = Self::generator_type_object(func_arg) {
            return Self::array_element_type_for_generator_type(&julia_type);
        }
        match self.infer_map_call_return_type(func_arg, iter_arg) {
            Some(ValueType::ArrayOf(element_type, _)) => Some(element_type),
            _ => None,
        }
    }

    fn infer_generator_tuple_splat_default_eltype(
        &mut self,
        func_arg: &Expr,
        iter_args: &[Expr],
    ) -> Option<ArrayElementType> {
        if let Some(julia_type) = Self::generator_type_object(func_arg) {
            return Self::array_element_type_for_tuple_splat_generator_type(&julia_type, iter_args);
        }

        let func_name = match func_arg {
            Expr::FunctionRef { name, .. } | Expr::Var(name, _) => name.clone(),
            _ => return None,
        };

        let mut arg_types = Vec::with_capacity(iter_args.len());
        for iter_arg in iter_args {
            let element_type = self.infer_generator_iter_element_type(iter_arg)?;
            arg_types.push(element_type);
        }

        if let Ok(func_index) = self.resolve_function_ref_with_arity(func_arg, iter_args.len()) {
            if let Some(func_ir) = self.shared_ctx.function_ir_by_global_index.get(&func_index) {
                let inferred =
                    self.infer_shared_function_return_type_with_arg_types(func_ir, &arg_types);
                if !matches!(inferred, ValueType::Any) {
                    return Some(ArrayElementType::from_value_type(&inferred));
                }
                if let Some(inferred) =
                    Self::infer_simple_function_return_type_for_args(func_ir, &arg_types)
                {
                    return Some(ArrayElementType::from_value_type(&inferred));
                }
            }
        }

        let table = self.method_tables.get(func_name.as_str())?;
        let julia_arg_types: Vec<JuliaType> = arg_types
            .iter()
            .map(|arg_type| self.value_type_to_julia_type(arg_type))
            .collect();
        let method = table.dispatch(&julia_arg_types).ok()?;
        let return_type = if matches!(&method.return_type, ValueType::Any) {
            if let Some(func_ir) = self
                .shared_ctx
                .function_ir_by_global_index
                .get(&method.global_index)
            {
                let inferred =
                    self.infer_shared_function_return_type_with_arg_types(func_ir, &arg_types);
                if matches!(inferred, ValueType::Any) {
                    Self::infer_simple_function_return_type_for_args(func_ir, &arg_types)
                        .unwrap_or(ValueType::Any)
                } else {
                    inferred
                }
            } else {
                method.return_type.clone()
            }
        } else {
            method.return_type.clone()
        };

        if matches!(return_type, ValueType::Any) {
            None
        } else {
            Some(ArrayElementType::from_value_type(&return_type))
        }
    }

    fn infer_simple_function_return_type_for_args(
        func: &Function,
        arg_types: &[ValueType],
    ) -> Option<ValueType> {
        if func.params.len() != arg_types.len() {
            return None;
        }
        let bindings: Vec<(&str, ValueType)> = func
            .params
            .iter()
            .zip(arg_types.iter())
            .map(|(param, ty)| (param.name.as_str(), ty.clone()))
            .collect();
        let Stmt::Return {
            value: Some(expr), ..
        } = func.body.stmts.first()?
        else {
            return None;
        };
        Self::infer_simple_bound_expr_type(expr, &bindings)
            .filter(|ty| !matches!(ty, ValueType::Any))
    }

    fn infer_simple_bound_expr_type(
        expr: &Expr,
        bindings: &[(&str, ValueType)],
    ) -> Option<ValueType> {
        match expr {
            Expr::Var(name, _) => bindings
                .iter()
                .find_map(|(binding_name, ty)| (*binding_name == name).then(|| ty.clone())),
            Expr::Literal(literal, _) => Self::literal_value_type(literal),
            Expr::UnaryOp { op, operand, .. } => {
                let operand_type = Self::infer_simple_bound_expr_type(operand, bindings)?;
                match op {
                    UnaryOp::Neg | UnaryOp::Pos
                        if Self::is_known_numeric_value_type(&operand_type) =>
                    {
                        Some(operand_type)
                    }
                    _ => None,
                }
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let left_type = Self::infer_simple_bound_expr_type(left, bindings)?;
                let right_type = Self::infer_simple_bound_expr_type(right, bindings)?;
                Self::infer_simple_binary_result_type(*op, &left_type, &right_type)
            }
            Expr::Call { function, args, .. } => {
                let arg_types: Option<Vec<ValueType>> = args
                    .iter()
                    .map(|arg| Self::infer_simple_bound_expr_type(arg, bindings))
                    .collect();
                Self::infer_simple_call_result_type(function, &arg_types?)
            }
            _ => None,
        }
    }

    fn literal_value_type(literal: &Literal) -> Option<ValueType> {
        match literal {
            Literal::Int(_) => Some(ValueType::I64),
            Literal::Float(_) => Some(ValueType::F64),
            Literal::Bool(_) => Some(ValueType::Bool),
            Literal::Str(_) => Some(ValueType::Str),
            Literal::Char(_) => Some(ValueType::Char),
            _ => None,
        }
    }

    fn infer_simple_call_result_type(function: &str, arg_types: &[ValueType]) -> Option<ValueType> {
        if arg_types.is_empty() {
            return None;
        }
        match function {
            "+" => Self::fold_numeric_result_types(arg_types),
            "*" => {
                if arg_types.iter().all(|ty| matches!(ty, ValueType::Str)) {
                    Some(ValueType::Str)
                } else {
                    Self::fold_numeric_result_types(arg_types)
                }
            }
            "-" => Self::fold_numeric_result_types(arg_types),
            "/" => {
                if arg_types.iter().all(Self::is_known_numeric_value_type) {
                    Some(ValueType::F64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn infer_simple_binary_result_type(
        op: BinaryOp,
        left: &ValueType,
        right: &ValueType,
    ) -> Option<ValueType> {
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                promote_numeric_value_types(left, right)
            }
            BinaryOp::Div => {
                if Self::is_known_numeric_value_type(left)
                    && Self::is_known_numeric_value_type(right)
                {
                    Some(ValueType::F64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn fold_numeric_result_types(arg_types: &[ValueType]) -> Option<ValueType> {
        let mut iter = arg_types.iter();
        let first = iter.next()?.clone();
        if !Self::is_known_numeric_value_type(&first) {
            return None;
        }
        iter.try_fold(first, |acc, ty| promote_numeric_value_types(&acc, ty))
    }

    fn is_known_numeric_value_type(value_type: &ValueType) -> bool {
        matches!(
            value_type,
            ValueType::I8
                | ValueType::I16
                | ValueType::I32
                | ValueType::I64
                | ValueType::I128
                | ValueType::BigInt
                | ValueType::U8
                | ValueType::U16
                | ValueType::U32
                | ValueType::U64
                | ValueType::U128
                | ValueType::F16
                | ValueType::F32
                | ValueType::F64
                | ValueType::BigFloat
                | ValueType::Bool
        )
    }

    fn infer_generator_iter_element_type(&mut self, iter_arg: &Expr) -> Option<ValueType> {
        match iter_arg {
            Expr::TypedEmptyArray { element_type, .. } => {
                return Self::value_type_for_type_name(element_type);
            }
            Expr::Range { .. } => return Some(ValueType::I64),
            Expr::ArrayLiteral { elements, .. } => {
                let mut element_type: Option<ValueType> = None;
                for element in elements {
                    let current = match element {
                        Expr::Literal(Literal::Int(_), _) => ValueType::I64,
                        Expr::Literal(Literal::Float(_), _) => ValueType::F64,
                        Expr::Literal(Literal::Bool(_), _) => ValueType::Bool,
                        Expr::Literal(Literal::Str(_), _) => ValueType::Str,
                        Expr::Literal(Literal::Char(_), _) => ValueType::Char,
                        _ => self.infer_expr_type(element),
                    };
                    element_type = Some(match element_type {
                        Some(previous) if previous == current => previous,
                        Some(_) => ValueType::Any,
                        None => current,
                    });
                }
                if let Some(element_type) = element_type {
                    return Some(element_type);
                }
            }
            _ => {}
        }

        let iter_type = self.infer_expr_type(iter_arg);
        match iter_type {
            ValueType::ArrayOf(ref elem, _) => Some(elem.to_value_type()),
            ValueType::Array => None,
            ValueType::Range => Some(ValueType::I64),
            ValueType::Tuple => None,
            _ => None,
        }
    }

    fn value_type_for_type_name(name: &str) -> Option<ValueType> {
        match name {
            "Int8" => Some(ValueType::I8),
            "Int16" => Some(ValueType::I16),
            "Int32" => Some(ValueType::I32),
            "Int" if crate::types::native_int_type_name() == "Int32" => Some(ValueType::I32),
            "Int64" | "Int" => Some(ValueType::I64),
            "Int128" => Some(ValueType::I128),
            "UInt8" => Some(ValueType::U8),
            "UInt16" => Some(ValueType::U16),
            "UInt32" => Some(ValueType::U32),
            "UInt" if crate::types::native_uint_type_name() == "UInt32" => Some(ValueType::U32),
            "UInt64" | "UInt" => Some(ValueType::U64),
            "UInt128" => Some(ValueType::U128),
            "Float32" => Some(ValueType::F32),
            "Float64" | "Float" => Some(ValueType::F64),
            "Bool" => Some(ValueType::Bool),
            "String" => Some(ValueType::Str),
            "Char" => Some(ValueType::Char),
            _ => None,
        }
    }

    fn generator_type_object(expr: &Expr) -> Option<JuliaType> {
        match expr {
            Expr::Var(name, _) | Expr::FunctionRef { name, .. } => JuliaType::from_name(name),
            Expr::Builtin {
                name: BuiltinOp::TypeOf,
                args,
                ..
            } if args.len() == 1 => match &args[0] {
                Expr::Literal(Literal::Str(type_name), _) => JuliaType::from_name(type_name)
                    .or_else(|| Some(JuliaType::Struct(type_name.clone()))),
                _ => None,
            },
            _ => None,
        }
    }

    fn array_element_type_for_tuple_splat_generator_type(
        julia_type: &JuliaType,
        iter_args: &[Expr],
    ) -> Option<ArrayElementType> {
        let type_name = julia_type.name();
        if iter_args.len() == 2 && type_name.starts_with("Complex{") {
            Some(ArrayElementType::Abstract(type_name.into_owned()))
        } else {
            Some(ArrayElementType::UnionOf(Vec::new()))
        }
    }

    fn array_element_type_for_generator_type(julia_type: &JuliaType) -> Option<ArrayElementType> {
        match julia_type {
            JuliaType::Float32 => Some(ArrayElementType::F32),
            JuliaType::Float64 => Some(ArrayElementType::F64),
            JuliaType::Int8 => Some(ArrayElementType::I8),
            JuliaType::Int16 => Some(ArrayElementType::I16),
            JuliaType::Int32 => Some(ArrayElementType::I32),
            JuliaType::Int64 => Some(ArrayElementType::I64),
            JuliaType::Int128 => Some(ArrayElementType::I128),
            JuliaType::UInt8 => Some(ArrayElementType::U8),
            JuliaType::UInt16 => Some(ArrayElementType::U16),
            JuliaType::UInt32 => Some(ArrayElementType::U32),
            JuliaType::UInt64 => Some(ArrayElementType::U64),
            JuliaType::UInt128 => Some(ArrayElementType::U128),
            JuliaType::Bool => Some(ArrayElementType::Bool),
            JuliaType::String => Some(ArrayElementType::String),
            JuliaType::Char => Some(ArrayElementType::Char),
            _ => None,
        }
    }

    /// Core-projection read of a method's first declared parameter for the
    /// collect/iterate candidate-shape heuristics (Issue #6495, stage 6b):
    /// the decision is made on the CoreType image of the structured
    /// `core_signature` projection. Stage 7c-ii: the legacy `params`
    /// fallback is retired — a test-only `Bottom` placeholder (unobservable
    /// in production since stage 7b) conservatively reports `false`.
    /// `pub(super)`: also consumed by the stage 6b-ii `expr/call` heuristics.
    pub(super) fn method_first_param_matches(
        method: &crate::compile::method_table::MethodSig,
        core_pred: impl FnOnce(&CoreType) -> bool,
    ) -> bool {
        method.param_matches_at(0, core_pred)
    }

    /// CoreType-native port of [`Self::is_runtime_iterate_candidate_type`]
    /// over the canonical `core_signature` projection (Issue #6495, stage 6b).
    ///
    /// Decision rule: true exactly when the canonical inverse
    /// (`inference_core::core_type_to_julia_type`) reconstructs one of the
    /// legacy arms `JuliaType::Struct(_) | Array | VectorOf(_) | MatrixOf(_)`.
    /// Base-corpus parity with the legacy predicate is pinned by
    /// `compile::cache::tests::base_method_core_param_heuristics_parity_issue_6495`.
    pub(crate) fn core_is_runtime_iterate_candidate_type(core: &CoreType) -> bool {
        match core {
            // Struct images whose canonical inverse normalizes to a dedicated
            // non-Struct `JuliaType` variant that the legacy predicate's
            // `Struct(_)` arm never saw (and that is not Array/Vector/Matrix).
            // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue #6721),
            // so the bare `iterate(s::Set)` Base method (which delegates to the
            // backing dict's `KeySet`) must be a runtime IterateDynamic candidate
            // for a `StructRef` Set value (e.g. a direct `iterate(x)` or the
            // specialized `collect` body where `x::Any` binds a Set struct). A
            // legacy native `Value::Set` is not a `Struct`/`StructRef`, so it never
            // reaches candidate scoring (`can_score_iterate_dynamic_candidates`)
            // and keeps using the VM builtin iterator. `("Set", 0)` is therefore
            // no longer excluded here.
            CoreType::Struct { name, params } => !matches!(
                (name.as_str(), params.len()),
                ("Tuple", 0)
                    | ("Dict", 0)
                    | ("NamedTuple", 0)
                    | ("UnitRange", 0)
                    | ("StepRange", 0)
                    | ("Generator", 0)
                    | ("IOBuffer", 0)
                    | ("Expr", 0)
                    | ("QuoteNode", 0)
                    | ("LineNumberNode", 0)
                    | ("GlobalRef", 0)
            ),
            // Abstract families WITHOUT a dedicated `JuliaType` variant keep a
            // `JuliaType::Struct(name)` spelling, which the legacy predicate
            // accepted via its `Struct(_)` arm.
            CoreType::Abstract(a) => matches!(
                a,
                CoreAbstract::AbstractVector
                    | CoreAbstract::AbstractMatrix
                    | CoreAbstract::DenseArray
                    | CoreAbstract::AbstractDict
                    | CoreAbstract::AbstractSet
                    | CoreAbstract::AbstractUnitRange
                    | CoreAbstract::Builtin
            ),
            // These shapes also reconstruct as `JuliaType::Struct(rendered)`.
            CoreType::Named(_)
            | CoreType::Vararg(_)
            | CoreType::VarargLen { .. }
            | CoreType::NamedTuple(_)
            | CoreType::Value(_) => true,
            _ => false,
        }
    }

    /// CoreType-native port of [`Self::is_runtime_collect_candidate_type`]
    /// (Issue #6495, stage 6b). Follows the canonical-inverse spelling where
    /// the `CoreType::from` bridge is non-injective: `Struct {"Vector", 1}` /
    /// `Struct {"Matrix", 1}` follow the `VectorOf`/`MatrixOf` verdict (true)
    /// and `Struct {"StepRange", 0}` follows the bare `StepRange` verdict
    /// (true), while a parametric `StepRange{...}` struct spelling stays
    /// false, exactly as the legacy name lists decided.
    pub(crate) fn core_is_runtime_collect_candidate_type(core: &CoreType) -> bool {
        match core {
            CoreType::Primitive(CorePrimitive::String) => true,
            CoreType::Struct { name, params } => match (name.as_str(), params.len()) {
                // Canonical inverses of JT::VectorOf / MatrixOf / StepRange,
                // accepted by the legacy predicate's dedicated-variant arms.
                ("Vector", 1) | ("Matrix", 1) | ("StepRange", 0) => true,
                (
                    "Array" | "UnitRange" | "LinRange" | "LogRange" | "StepRangeLen" | "SubArray"
                    | "ReshapedArray" | "Zip" | "Zip3" | "Zip4" | "Zip5" | "Zip6" | "Zip7"
                    | "Enumerate" | "Take" | "Drop" | "TakeWhile" | "DropWhile" | "Rest" | "Filter"
                    | "Flatten" | "FlatMap",
                    _,
                ) => true,
                _ => false,
            },
            _ => false,
        }
    }

    /// CoreType-native port of [`Self::is_range_collect_signature_type`]
    /// (Issue #6495, stage 6b). Core `Struct` names are already module- and
    /// parameter-stripped by `CoreType::from_julia_name`, so the legacy
    /// `split('{')` / `rsplit('.')` base-name extraction collapses into a
    /// direct family-name match; the bare `AbstractRange` spelling images as
    /// `CoreType::Abstract` while the parametric `AbstractRange{T}` container
    /// spelling keeps its invariant `Struct` image (both accepted, exactly
    /// like the legacy name list).
    pub(crate) fn core_is_range_collect_signature_type(core: &CoreType) -> bool {
        match core {
            CoreType::Struct { name, .. } => matches!(
                name.as_str(),
                "UnitRange"
                    | "StepRange"
                    | "StepRangeLen"
                    | "LinRange"
                    | "OneTo"
                    | "LogRange"
                    | "AbstractRange"
            ),
            CoreType::Abstract(CoreAbstract::AbstractRange) => true,
            CoreType::UnionAll { body, .. } => Self::core_is_range_collect_signature_type(body),
            _ => false,
        }
    }

    /// CoreType-native port of [`Self::is_generator_collect_signature_type`]
    /// (Issue #6495, stage 6b). Both the dedicated `JuliaType::Generator`
    /// variant and parametric `Generator{F, I}` struct spellings image into
    /// the `Struct {"Generator", ..}` family.
    pub(crate) fn core_is_generator_collect_signature_type(core: &CoreType) -> bool {
        match core {
            CoreType::Struct { name, .. } => name == "Generator",
            CoreType::UnionAll { body, .. } => Self::core_is_generator_collect_signature_type(body),
            _ => false,
        }
    }

    /// The struct-family base name the legacy iterate dispatch compared — a
    /// `JuliaType::Struct(name)` first parameter with its `{...}` suffix
    /// stripped — read from the canonical core projection: `Some` exactly for
    /// images whose canonical inverse reconstructs a `JuliaType::Struct(_)`
    /// spelling (Issue #6495, stage 6b). Core `Struct` names are already
    /// module-stripped, matching the canonical-inverse rendering the legacy
    /// extraction sees post-deserialization. `CoreType::Value` images render
    /// as bare value literals that can never equal a runtime struct family
    /// name, so they stay `None`.
    pub(crate) fn core_param_struct_base(core: &CoreType) -> Option<&str> {
        match core {
            CoreType::Struct { name, params } => match (name.as_str(), params.len()) {
                // Canonical inverses of the dedicated JuliaType variants —
                // the legacy `Struct(name)` arm never saw these.
                ("Vector", 1)
                | ("Matrix", 1)
                | ("Tuple", 0)
                | ("Array", 0)
                | ("Set", 0)
                | ("Dict", 0)
                | ("NamedTuple", 0)
                | ("UnitRange", 0)
                | ("StepRange", 0)
                | ("Generator", 0)
                | ("IOBuffer", 0)
                | ("Expr", 0)
                | ("QuoteNode", 0)
                | ("LineNumberNode", 0)
                | ("GlobalRef", 0) => None,
                _ => Some(name.as_str()),
            },
            CoreType::Named(name) => Some(name.as_str()),
            CoreType::NamedTuple(_) => Some("NamedTuple"),
            CoreType::Vararg(_) | CoreType::VarargLen { .. } => Some("Vararg"),
            // Abstract families WITHOUT a dedicated `JuliaType` variant
            // reconstruct as `JuliaType::Struct(name)` spellings.
            CoreType::Abstract(a) => match a {
                CoreAbstract::AbstractVector => Some("AbstractVector"),
                CoreAbstract::AbstractMatrix => Some("AbstractMatrix"),
                CoreAbstract::DenseArray => Some("DenseArray"),
                CoreAbstract::AbstractDict => Some("AbstractDict"),
                CoreAbstract::AbstractSet => Some("AbstractSet"),
                CoreAbstract::AbstractUnitRange => Some("AbstractUnitRange"),
                CoreAbstract::Builtin => Some("Core.Builtin"),
                _ => None,
            },
            _ => None,
        }
    }

    /// Retired from production at Issue #6495 stage 7c-ii: the projection-side
    /// reads now consume the `core_signature` projection only. Retained as the
    /// parity-gate / unit-test oracle until the projection fields are deleted.
    #[cfg(test)]
    pub(crate) fn is_runtime_collect_candidate_type(julia_type: &JuliaType) -> bool {
        match julia_type {
            JuliaType::Array
            | JuliaType::VectorOf(_)
            | JuliaType::MatrixOf(_)
            | JuliaType::String
            | JuliaType::UnitRange
            | JuliaType::StepRange => true,
            JuliaType::Struct(name) => {
                let base_name = name
                    .split('{')
                    .next()
                    .unwrap_or(name.as_str())
                    .rsplit('.')
                    .next()
                    .unwrap_or(name.as_str());
                matches!(
                    base_name,
                    "Array"
                        | "UnitRange"
                        | "LinRange"
                        | "LogRange"
                        | "StepRangeLen"
                        | "SubArray"
                        | "ReshapedArray"
                        | "Zip"
                        | "Zip3"
                        | "Zip4"
                        | "Zip5"
                        | "Zip6"
                        | "Zip7"
                        | "Enumerate"
                        | "Take"
                        | "Drop"
                        | "TakeWhile"
                        | "DropWhile"
                        | "Rest"
                        | "Filter"
                        | "Flatten"
                        | "FlatMap"
                )
            }
            _ => false,
        }
    }

    /// Retired from production at Issue #6495 stage 7c-ii: the projection-side
    /// reads now consume the `core_signature` projection only. Retained as the
    /// parity-gate / unit-test oracle until the projection fields are deleted.
    #[cfg(test)]
    pub(crate) fn is_runtime_iterate_candidate_type(julia_type: &JuliaType) -> bool {
        matches!(
            julia_type,
            JuliaType::Struct(_)
                | JuliaType::Array
                | JuliaType::VectorOf(_)
                | JuliaType::MatrixOf(_)
                // `Set` is a pure-Julia struct over `Dict{T,Nothing}` (Issue
                // #6721); its bare `iterate(s::Set)` Base method is a runtime
                // IterateDynamic candidate. Kept in parity with the
                // canonical-inverse `core_is_runtime_iterate_candidate_type`,
                // whose `("Set", 0)` exclusion was removed.
                | JuliaType::Set
        )
    }

    fn is_user_range_collect_method(
        &self,
        method: &crate::compile::method_table::MethodSig,
    ) -> bool {
        self.shared_ctx
            .function_ir_by_global_index
            .contains_key(&method.global_index)
            && Self::method_first_param_matches(method, Self::core_is_range_collect_signature_type)
    }

    /// Retired from production at Issue #6495 stage 7c-ii: the projection-side
    /// reads now consume the `core_signature` projection only. Retained as the
    /// parity-gate / unit-test oracle until the projection fields are deleted.
    #[cfg(test)]
    pub(crate) fn is_range_collect_signature_type(julia_type: &JuliaType) -> bool {
        match julia_type {
            JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange => true,
            JuliaType::Struct(name) => {
                let base_name = name
                    .split('{')
                    .next()
                    .unwrap_or(name.as_str())
                    .rsplit('.')
                    .next()
                    .unwrap_or(name.as_str());
                matches!(
                    base_name,
                    "UnitRange"
                        | "StepRange"
                        | "StepRangeLen"
                        | "LinRange"
                        | "OneTo"
                        | "LogRange"
                        | "AbstractRange"
                )
            }
            JuliaType::UnionAll { body, .. } => Self::is_range_collect_signature_type(body),
            _ => false,
        }
    }

    fn has_user_range_collect_method_for_builtin(&self) -> bool {
        ["collect", "Base.collect"]
            .iter()
            .filter_map(|name| self.method_tables.get(*name))
            .flat_map(|table| table.methods.iter())
            .any(|method| method.param_count() == 1 && self.is_user_range_collect_method(method))
    }

    fn has_user_generator_collect_method(&self) -> bool {
        ["collect", "Base.collect"]
            .iter()
            .filter_map(|name| self.method_tables.get(*name))
            .flat_map(|table| table.methods.iter())
            .any(|method| {
                method.param_count() == 1 && self.is_user_generator_collect_method(method)
            })
    }

    fn is_user_generator_collect_method(
        &self,
        method: &crate::compile::method_table::MethodSig,
    ) -> bool {
        self.shared_ctx
            .function_ir_by_global_index
            .contains_key(&method.global_index)
            && Self::method_first_param_matches(
                method,
                Self::core_is_generator_collect_signature_type,
            )
    }

    /// Retired from production at Issue #6495 stage 7c-ii: the projection-side
    /// reads now consume the `core_signature` projection only. Retained as the
    /// parity-gate / unit-test oracle until the projection fields are deleted.
    #[cfg(test)]
    pub(crate) fn is_generator_collect_signature_type(julia_type: &JuliaType) -> bool {
        match julia_type {
            JuliaType::Generator => true,
            JuliaType::Struct(name) => {
                let base_name = name
                    .split('{')
                    .next()
                    .unwrap_or(name.as_str())
                    .rsplit('.')
                    .next()
                    .unwrap_or(name.as_str());
                base_name == "Generator"
            }
            JuliaType::UnionAll { body, .. } => Self::is_generator_collect_signature_type(body),
            _ => false,
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
        self.declared_globals.contains(name)
            || self.module_constant_qualified_name(name).is_some()
            || (!self.locals.contains_key(name) && self.shared_ctx.global_types.contains_key(name))
    }

    fn module_constant_qualified_name(&self, name: &str) -> Option<String> {
        let module_path = self.current_module_path.as_ref()?;
        let const_names = self.module_constants.get(module_path)?;
        if const_names.contains(name) {
            Some(format!("{}.{}", module_path, name))
        } else {
            None
        }
    }

    /// Resolve an unqualified call `name` to the module-qualified method-table
    /// name (`"{current_module}.{name}"`) when the module currently being
    /// compiled defines its OWN function of that name (Issue #7575).
    ///
    /// sjulia registers every module function under both its bare name (a flat
    /// pool shared by every module) and a module-qualified name. Multiple
    /// dispatch over the shared bare-name pool lets a parent module's
    /// same-named, more-specific *typed* method win an unqualified call made
    /// from inside a child module — e.g. `A.B.g(1)` selecting `A.f(::Number)`
    /// instead of the child's own `A.B.f(x)`. Upstream Julia treats a bare
    /// `f(x) = ...` inside a submodule as a NEW binding that shadows the parent;
    /// it does not pool dispatch candidates across module boundaries unless `f`
    /// is explicitly imported/qualified.
    ///
    /// The module-qualified table is created (in `pipeline_ctx`) only for
    /// functions that the module itself defines, so its existence is necessary
    /// (but not sufficient) evidence that the current module owns `name`. The
    /// redirect is restricted to genuinely module-local generic functions — see
    /// the guards below.
    pub(crate) fn module_owned_function_table_name(&self, name: &str) -> Option<String> {
        // Already-qualified names (`Base.foo`, `M.bar`) are resolved elsewhere.
        if name.contains('.') {
            return None;
        }
        let module_path = self.current_module_path.as_ref()?;
        // A name imported via `using`/`import` shares one generic function with
        // its source module, so the unqualified call must keep pooling dispatch
        // candidates across both modules — do not redirect it (Issue #7575).
        if self.current_module_imports.contains(name) {
            return None;
        }
        let qualified = format!("{}.{}", module_path, name);
        let (Some(bare_table), Some(qualified_table)) = (
            self.method_tables.get(name),
            self.method_tables.get(&qualified),
        ) else {
            return None;
        };

        // Only redirect a generic function the module genuinely OWNS. A module
        // that extends a Base/prelude generic (`Base.:*(::Diagonal, ...)` inside
        // LinearAlgebra) does NOT create a new module-local generic — the
        // method-qualified `LinearAlgebra.*` table is only a partial shard of
        // `Base.*` that omits every Base/prelude method (scalar `*`, etc.).
        // Redirecting an unqualified internal call (a scalar multiply, say) to
        // that shard would drop the needed Base candidates and mis-dispatch.
        // The bare-name pool carrying any Base/prelude method or Base extension
        // is therefore the signal that the generic is Base-owned, never
        // module-local (Issue #7575 / regression guard for #7468 LinearAlgebra).
        let base_count = bare_table.base_function_count();
        let has_base_method = bare_table
            .methods
            .iter()
            .any(|m| m.is_base_extension || m.is_base_program_method(base_count));
        if has_base_method {
            return None;
        }

        // Redirect only when the bare pool actually pooled FOREIGN methods —
        // i.e. it carries a method the current module's own table does not.
        // When both tables hold exactly the same methods (a single-module
        // generic such as `LinearAlgebra.det`, whose wrapper forwards to a
        // builtin via LinearAlgebra's private VM-kernel bridge), the redirect is a behavioral
        // no-op that would only break the module-call builtin-forwarding path,
        // so skip it. The cross-module shadow that #7575 fixes is exactly the
        // case where the bare pool is strictly larger.
        let qualified_indices: std::collections::HashSet<usize> = qualified_table
            .methods
            .iter()
            .map(|m| m.global_index)
            .collect();
        let bare_has_foreign_method = bare_table
            .methods
            .iter()
            .any(|m| !qualified_indices.contains(&m.global_index));
        if !bare_has_foreign_method {
            return None;
        }

        Some(qualified)
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

    fn array_pop_result_type(&mut self, array: &Expr) -> ValueType {
        match self.infer_expr_type(array) {
            ValueType::ArrayOf(element, _) => array_element_value_type(&element),
            _ => ValueType::Any,
        }
    }
}

fn array_element_value_type(element: &ArrayElementType) -> ValueType {
    match element {
        ArrayElementType::F32 => ValueType::F32,
        ArrayElementType::F64 => ValueType::F64,
        ArrayElementType::ComplexF32 => ValueType::ComplexF32,
        ArrayElementType::ComplexF64 => ValueType::ComplexF64,
        ArrayElementType::I8 => ValueType::I8,
        ArrayElementType::I16 => ValueType::I16,
        ArrayElementType::I32 => ValueType::I32,
        ArrayElementType::I64 => ValueType::I64,
        ArrayElementType::I128 => ValueType::I128,
        ArrayElementType::U8 => ValueType::U8,
        ArrayElementType::U16 => ValueType::U16,
        ArrayElementType::U32 => ValueType::U32,
        ArrayElementType::U64 => ValueType::U64,
        ArrayElementType::U128 => ValueType::U128,
        ArrayElementType::Bool => ValueType::Bool,
        ArrayElementType::String | ArrayElementType::SubString => ValueType::Str,
        ArrayElementType::Char => ValueType::Char,
        ArrayElementType::Symbol => ValueType::Symbol,
        ArrayElementType::Nothing => ValueType::Nothing,
        ArrayElementType::Struct => ValueType::Any,
        ArrayElementType::StructOf(id) | ArrayElementType::StructInlineOf(id, _) => {
            ValueType::Struct(*id)
        }
        ArrayElementType::Any
        | ArrayElementType::TupleOf(_)
        | ArrayElementType::UnionOf(_)
        | ArrayElementType::Abstract(_) => ValueType::Any,
    }
}

fn exact_isa_codegen_type(ty: &ValueType) -> bool {
    // Struct ids are not exact enough for `isa` false-folding: a concrete
    // parametric instance can still satisfy a bare family or user abstract
    // target, so keep non-identical struct checks on the runtime path.
    matches!(
        ty,
        ValueType::I8
            | ValueType::I16
            | ValueType::I32
            | ValueType::I64
            | ValueType::I128
            | ValueType::U8
            | ValueType::U16
            | ValueType::U32
            | ValueType::U64
            | ValueType::U128
            | ValueType::F16
            | ValueType::F32
            | ValueType::F64
            | ValueType::Bool
            | ValueType::Char
            | ValueType::Str
    )
}

#[cfg(test)]
mod core_param_heuristics_tests {
    //! Issue #6495 (stage 6b): the CoreType-native collect/iterate
    //! candidate-shape predicates must agree with the legacy
    //! `params`-projection predicates. Two layers:
    //!
    //! 1. The definitional invariant: `core_pred(core)` equals
    //!    `legacy_pred(core_type_to_julia_type(core))` — the legacy predicate
    //!    applied to the canonical inverse, which is exactly what the legacy
    //!    path sees after a cache round-trip (and what stage 7 deletes).
    //! 2. Direct parity on lowering-produced spellings (round-tripping
    //!    shapes) against the build-time `JuliaType` itself.
    //!
    //! Base-corpus-wide parity is pinned separately by
    //! `compile::cache::tests::base_method_core_param_heuristics_parity_issue_6495`.

    use super::CoreCompiler;
    use crate::inference_core::{core_type_to_julia_type, CoreType};
    use crate::types::JuliaType;

    fn legacy_struct_base(ty: &JuliaType) -> Option<&str> {
        match ty {
            JuliaType::Struct(name) => Some(name.split('{').next().unwrap_or(name.as_str())),
            _ => None,
        }
    }

    fn user_shapes() -> Vec<JuliaType> {
        vec![
            JuliaType::Array,
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::MatrixOf(Box::new(JuliaType::Float64)),
            JuliaType::String,
            JuliaType::UnitRange,
            JuliaType::StepRange,
            JuliaType::AbstractRange,
            JuliaType::Generator,
            JuliaType::Tuple,
            JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64]),
            JuliaType::Any,
            JuliaType::Int64,
            JuliaType::Number,
            JuliaType::Dict,
            JuliaType::Set,
            JuliaType::NamedTuple,
            JuliaType::IOBuffer,
            JuliaType::Function,
            JuliaType::AbstractArray,
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::String]),
            JuliaType::Struct("EachCol".to_string()),
            JuliaType::Struct("Zip3{Any, Any, Any}".to_string()),
            JuliaType::Struct("Enumerate{I}".to_string()),
            JuliaType::Struct("StepRangeLen{Float64}".to_string()),
            JuliaType::Struct("LinRange{T}".to_string()),
            JuliaType::Struct("OneTo{Int64}".to_string()),
            JuliaType::Struct("Filter{F, I}".to_string()),
            JuliaType::Struct("AbstractVector".to_string()),
            JuliaType::Struct("MyStruct{Int64}".to_string()),
            JuliaType::Struct("AbstractRange{T}".to_string()),
            JuliaType::Struct("SubArray{Float64, 2}".to_string()),
            JuliaType::Struct("ReshapedArray{Int64, 1, Vector{Int64}, Tuple{}}".to_string()),
            JuliaType::UnionAll {
                var: "T".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("LinRange{T}".to_string())),
            },
            JuliaType::UnionAll {
                var: "F".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("Generator{F, I}".to_string())),
            },
        ]
    }

    #[test]
    fn core_predicates_match_legacy_on_canonical_inverse_issue_6495() {
        for ty in &user_shapes() {
            let core = CoreType::from(ty);
            let inverse = core_type_to_julia_type(&core);
            assert_eq!(
                CoreCompiler::core_is_runtime_iterate_candidate_type(&core),
                CoreCompiler::is_runtime_iterate_candidate_type(&inverse),
                "iterate candidate (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                CoreCompiler::core_is_runtime_collect_candidate_type(&core),
                CoreCompiler::is_runtime_collect_candidate_type(&inverse),
                "collect candidate (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                CoreCompiler::core_is_range_collect_signature_type(&core),
                CoreCompiler::is_range_collect_signature_type(&inverse),
                "range collect (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                CoreCompiler::core_is_generator_collect_signature_type(&core),
                CoreCompiler::is_generator_collect_signature_type(&inverse),
                "generator collect (inverse) parity for {ty:?} (core {core:?})"
            );
            assert_eq!(
                CoreCompiler::core_param_struct_base(&core),
                legacy_struct_base(&inverse),
                "struct-base (inverse) parity for {ty:?} (core {core:?})"
            );
        }
    }

    #[test]
    fn core_predicates_match_legacy_for_lowering_spellings_issue_6495() {
        for ty in &user_shapes() {
            let core = CoreType::from(ty);
            // Only round-tripping spellings are guaranteed direct parity (the
            // canonical inverse is what production reconstructs; the #6336
            // round-trip gate pins Base to these spellings).
            if &core_type_to_julia_type(&core) != ty {
                continue;
            }
            assert_eq!(
                CoreCompiler::core_is_runtime_iterate_candidate_type(&core),
                CoreCompiler::is_runtime_iterate_candidate_type(ty),
                "iterate candidate parity for {ty:?}"
            );
            assert_eq!(
                CoreCompiler::core_is_runtime_collect_candidate_type(&core),
                CoreCompiler::is_runtime_collect_candidate_type(ty),
                "collect candidate parity for {ty:?}"
            );
            assert_eq!(
                CoreCompiler::core_is_range_collect_signature_type(&core),
                CoreCompiler::is_range_collect_signature_type(ty),
                "range collect parity for {ty:?}"
            );
            assert_eq!(
                CoreCompiler::core_is_generator_collect_signature_type(&core),
                CoreCompiler::is_generator_collect_signature_type(ty),
                "generator collect parity for {ty:?}"
            );
            assert_eq!(
                CoreCompiler::core_param_struct_base(&core),
                legacy_struct_base(ty),
                "struct-base parity for {ty:?}"
            );
        }
    }
}
