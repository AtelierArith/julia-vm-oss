//! Module-qualified function call compilation.
//!
//! Handles:
//! - `Module.func(args)` calls (e.g., `Base.push!`, `Random.seed!`)
//! - Module function references (e.g., `Base.println` as a value)

use crate::builtins::BuiltinId;
use crate::bytecode::{
    DynamicCallCandidate, Instr, ModuleOperands, ResolvedFunctionOperands, ValueType,
};
use crate::ir::core::{BuiltinOp, Expr, UnaryOp};
use crate::types::JuliaType;

use super::dispatch::{
    is_dict_annotation, is_runtime_unknown_struct_arg, should_runtime_dispatch,
    should_use_dynamic_call_for_runtime_dispatch,
};
use super::ITERATORS_FUNCTIONS;
use crate::compile::{
    base_function_to_builtin_op, err, function_name_to_binary_op, get_math_constant_value,
    is_base_function, is_base_submodule_function, is_method_dispatch_first_base_function,
    is_random_function, julia_type_to_value_type, parse_parametric_call, CResult, CompileError,
    CoreCompiler, MethodTable,
};

fn unsupported_opaque_closure_message() -> &'static str {
    "Core.OpaqueClosure opaque closures are not supported yet (Issue #4289)"
}

/// Base reflection / conversion / promotion helpers that resolve as callable
/// function values via the bare-identifier path even though they are not in the
/// `is_base_function` allowlist. `Base.<fn>` for these names must produce the
/// same callable function value as the unqualified `<fn>` (Issues #4960-#4966).
fn is_base_reflection_function_value(name: &str) -> bool {
    matches!(
        name,
        // Compiler reflection helpers (#4960-#4963)
        "return_types"
            | "infer_return_type"
            | "code_typed"
            | "code_lowered"
            // Conversion / promotion helpers (#4964-#4966)
            | "widen"
            | "promote_type"
            | "promote_rule"
            | "convert"
            | "oftype"
            // Pure-Julia HOF whose direct call is intercepted for a fast path
            // but which is otherwise an ordinary function value (#4973). Route
            // `Base.ntuple` through the bare-identifier path so it resolves to
            // the base/tuple.jl method instead of erroring.
            | "ntuple"
    )
}

fn is_signature_tuple_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::DynamicTypeConstruct { base, .. } if base == "Tuple"
    )
}

impl CoreCompiler<'_> {
    /// Return the qualified-call view of a bare parametric constructor table.
    ///
    /// Julia treats `Box(...)` and `Box{T}(...)` as different constructor-self
    /// families. The compiler projects that self argument out of method
    /// signatures, so a qualified bare call must remove methods recorded with
    /// the explicit `Box{T}` self before every dispatch decision. Keep the
    /// serialized `ConstructorSelfFamily` carrier on `MethodTable` as the sole
    /// authority; names are used only to establish that this is a known bare
    /// parametric-struct call (Issues #10959, #11076, #7302, #10342, #11147).
    fn bare_parametric_constructor_table(
        &self,
        method_table_name: &str,
        table: &MethodTable,
    ) -> Option<MethodTable> {
        (!method_table_name.contains('{')
            && self
                .resolve_parametric_struct_name(method_table_name)
                .is_some()
            && table.has_explicit_parametric_inner_constructors())
        .then(|| {
            table.clone_with_methods_for_compile(
                table
                    .methods
                    .iter()
                    .filter(|method| {
                        !table.is_explicit_parametric_inner_constructor(method.global_index)
                    })
                    .cloned()
                    .collect(),
            )
        })
    }

    fn bare_parametric_constructor_dispatch_matches(
        &self,
        method_table_name: &str,
        arg_types: &[JuliaType],
    ) -> bool {
        self.method_tables
            .get(method_table_name)
            .is_some_and(|table| {
                let bare_constructor_table =
                    self.bare_parametric_constructor_table(method_table_name, table);
                bare_constructor_table
                    .as_ref()
                    .unwrap_or(table)
                    .dispatch(arg_types)
                    .is_ok()
            })
    }

    /// Emit runtime keyword dispatch without discarding the filtered
    /// constructor-self-family view. Ordinary module calls keep the shared
    /// name-resolution path; bare parametric constructors carry their exact
    /// eligible method indices in the function value (Issues #11076, #11147).
    fn emit_module_runtime_dispatched_kwargs_call(
        &mut self,
        method_table_name: &str,
        bare_constructor_table: Option<&MethodTable>,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        kwargs_splat_mask: &[bool],
        args_already_compiled: bool,
    ) -> CResult<ValueType> {
        let Some(table) = bare_constructor_table else {
            return self.emit_runtime_dispatched_kwargs_call(
                method_table_name,
                args,
                kwargs,
                kwargs_splat_mask,
                args_already_compiled,
            );
        };

        if !args_already_compiled {
            for arg in args {
                self.compile_expr(arg)?;
            }
        }
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        self.emit(Instr::PushResolvedFunction(Box::new(
            ResolvedFunctionOperands {
                name: method_table_name.to_string(),
                candidate_indices: table
                    .methods
                    .iter()
                    .map(|method| method.global_index)
                    .collect(),
            },
        )));
        self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
            crate::bytecode::CallVarKwargsSplat {
                arg_count: args.len(),
                pos_splat_mask: vec![false; args.len()],
                kwarg_names,
                kwargs_splat_mask: kwargs_splat_mask.to_vec(),
            },
        )));
        Ok(ValueType::Any)
    }

    fn emit_qualified_catchable_no_method_error_call(
        &mut self,
        method_table_name: &str,
        args: &[Expr],
        arg_types: &[JuliaType],
        args_already_compiled: bool,
    ) -> CResult<ValueType> {
        if !args_already_compiled {
            for arg in args {
                self.compile_expr(arg)?;
            }
        }
        for _ in args {
            self.emit(Instr::Pop);
        }
        let arg_sig: Vec<String> = arg_types.iter().map(|ty| format!("::{ty}")).collect();
        self.emit(Instr::ThrowMethodError(format!(
            "no method matching {}({})",
            method_table_name,
            arg_sig.join(", ")
        )));
        Ok(ValueType::Any)
    }

    pub(super) fn compile_resolved_module_call(
        &mut self,
        module: &str,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        let has_splat = !splat_mask.is_empty() && splat_mask.iter().any(|&b| b);

        let root = module.split_once('.').map_or(module, |(root, _)| root);
        if self.resolved_module_alias(root).is_none()
            && (self.locals.contains_key(root) || self.captured_vars.contains(root))
        {
            return self.compile_local_module_syntax_call(
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }

        let owned_module_path = self.module_path_in_current_scope(module);
        if owned_module_path.is_none()
            && !crate::compile::constants::is_stdlib_module(root)
            && self.imported_binding_root(module).is_some()
        {
            return self.compile_imported_module_alias_call(
                module,
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }

        // Resolve module aliases: S.mean() -> Statistics.mean() if S = Statistics.
        // Also resolves a dotted path whose ROOT is an alias, so an alias-rooted
        // nested-module call resolves: `AA.B.C.g()` with `const AA = A` ->
        // `A.B.C.g()` (Issue #8114). Without the root resolution only the
        // whole-name alias (`MA` -> `Mod1`) was handled, and a nested aliased call
        // failed with "Unknown module: AA.B.C".
        let resolved_module_owned =
            owned_module_path.unwrap_or_else(|| self.resolve_module_alias_path(module));
        let resolved_module_owned = self.canonical_module_path(&resolved_module_owned);
        let resolved_module = resolved_module_owned.as_str();

        // Whole-program type metadata knows module-owned structs before their
        // Julia bindings execute. A qualified constructor nevertheless resolves
        // that binding at the call's source position, before evaluating type,
        // positional, keyword, or splatted arguments. Reject a source-later
        // current-input type here; Base/precompiled types have no position in
        // this input and retain their static PushDataType path (Issues #11713,
        // #11716, and #11720).
        let constructor_base = parse_parametric_call(function)
            .map(|(base, _)| base)
            .unwrap_or_else(|| function.to_string());
        let qualified_constructor = format!("{resolved_module}.{constructor_base}");
        if self.current_function_name.is_none() {
            if let (Some(type_position), Some(call_span)) = (
                self.shared_ctx
                    .type_definition_positions
                    .get(&qualified_constructor),
                self.current_span(),
            ) {
                if !type_position.is_before(call_span.definition_order, call_span.start) {
                    self.emit(Instr::ThrowUndefVarError(constructor_base));
                    return Ok(ValueType::Any);
                }
            }
        }

        // Special handling for Base module - maps to built-in functions
        if resolved_module == "Base" {
            // `Base.eval(m, expr)` targets module `m`, the same primitive as
            // `Core.eval(m, expr)` below (Issue #11421).
            if function == "eval" && !has_splat && kwargs.is_empty() && args.len() == 2 {
                return self.compile_builtin(&BuiltinOp::Eval, args);
            }
            if function == "collect_similar" && args.len() == 2 {
                let container_is_memory = matches!(
                    self.infer_expr_type(&args[0]),
                    ValueType::Memory | ValueType::MemoryOf(_)
                ) || matches!(&args[0], Expr::Call { function, .. }
                    if function == "Memory"
                        || function == "Base.Memory"
                        || function.starts_with("Memory{")
                        || function.starts_with("Base.Memory{"));
                let is_generator_call = matches!(&args[1], Expr::Call {
                    function,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    ..
                } if (function == "Generator" || function == "Base.Generator")
                    && kwargs.is_empty()
                    && !splat_mask.iter().any(|&is_splat| is_splat)
                    && !kwargs_splat_mask.iter().any(|&is_splat| is_splat));
                if !container_is_memory
                    && (is_generator_call
                        || matches!(self.infer_expr_type(&args[1]), ValueType::Generator))
                {
                    let collect_args = vec![args[1].clone()];
                    return self.compile_builtin(&BuiltinOp::Collect, &collect_args);
                }
                if matches!(self.infer_expr_type(&args[0]), ValueType::Any)
                    || matches!(self.infer_expr_type(&args[1]), ValueType::Any)
                {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::PushFunction("Base.collect_similar".to_string()));
                    self.emit(Instr::CallFunctionVariable(args.len()));
                    return Ok(ValueType::Any);
                }
            }

            if function == "_collect" && args.len() == 4 {
                let container_is_memory = matches!(
                    self.infer_expr_type(&args[0]),
                    ValueType::Memory | ValueType::MemoryOf(_)
                );
                let iter_is_generator =
                    matches!(self.infer_expr_type(&args[1]), ValueType::Generator)
                        || matches!(&self.infer_julia_type(&args[1]), JuliaType::Struct(name) if {
                            let base_name = name.split('{').next().unwrap_or(name.as_str());
                            matches!(base_name, "Generator" | "Base.Generator")
                        });
                if container_is_memory && iter_is_generator {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::Pop);
                    self.emit(Instr::Pop);
                    self.emit(Instr::PushFunction(
                        "_collect_memory_generator_values".to_string(),
                    ));
                    self.emit(Instr::CallFunctionVariable(2));
                    return Ok(ValueType::Any);
                }
            }

            if function == "Generator" && has_splat {
                if !kwargs.is_empty()
                    || (!kwargs_splat_mask.is_empty() && kwargs_splat_mask.iter().any(|&b| b))
                {
                    return err("Base.Generator does not accept keyword arguments");
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::PushFunction("Base.Generator".to_string()));
                self.emit(Instr::CallFunctionVariableWithSplat(
                    args.len(),
                    splat_mask.to_vec(),
                ));
                return Ok(ValueType::Generator);
            }

            if has_splat {
                return self.compile_module_splat_call(
                    &format!("Base.{}", function),
                    None,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                );
            }

            // `Base.:(<:)(A, B)`, `Base.:(>:)(A, B)`, `Base.:(isa)(x, T)` in call
            // position: route to the dedicated type-operator builtins rather than
            // the numeric `compile_builtin_binary_op` path, which mishandles
            // DataType operands. `>:` swaps operands (A >: B ⟺ B <: A), matching
            // the infix lowering in lowering/expr/binary.rs (Issue #5115).
            if matches!(function, "<:" | ">:" | "isa") && args.len() == 2 {
                match function {
                    "<:" => {
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::Subtype, 2));
                    }
                    ">:" => {
                        self.compile_expr(&args[1])?;
                        self.compile_expr(&args[0])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::Subtype, 2));
                    }
                    _ => {
                        // isa(x, T)
                        self.compile_expr(&args[0])?;
                        self.compile_expr(&args[1])?;
                        self.emit(Instr::CallBuiltin(BuiltinId::Isa, 2));
                    }
                }
                return Ok(ValueType::Bool);
            }

            // Handle operators: Base.:+(a, b), Base.:-(a, b), etc.
            // This bypasses user-defined operator overloads to access the builtin operators.
            if let Some(op) = function_name_to_binary_op(function) {
                if args.len() == 2 {
                    return self.compile_builtin_binary_op(&op, &args[0], &args[1]);
                } else if args.len() == 1 && function == "-" {
                    // Unary minus: Base.:-(x)
                    return self.compile_unary_op(&UnaryOp::Neg, &args[0], args[0].span());
                }
                return err(format!(
                    "Wrong number of arguments for operator {}: expected 2, got {}",
                    function,
                    args.len()
                ));
            }

            if function == "inv" {
                return self.compile_call(function, args, kwargs, &[], &[]);
            }

            if function == "convert" {
                return self.compile_call("Base.convert", args, kwargs, &[], &[]);
            }

            if function == "isexpr" {
                return self.compile_module_call("Meta", "isexpr", args, kwargs, &[], &[]);
            }

            if matches!(function, "print" | "println")
                && kwargs.is_empty()
                && args
                    .iter()
                    .any(|arg| matches!(self.infer_julia_type(arg), JuliaType::Any))
            {
                // Issue #4580: keep Base.print/println with Any-typed arguments
                // on the builtin I/O path instead of coercing through a
                // statically selected singleton method.
                return self.compile_builtin_call(function, args);
            }

            // A user declaration can replace the shared bare table for a Base
            // name (including constructor families such as Set/Dict). Consult
            // the preserved Base-qualified snapshot before collection helpers
            // dispatch through that contaminated bare table (Issue #11367).
            let base_qualified = format!("Base.{}", function);
            if let Some(table) = self.method_tables.get(&base_qualified) {
                let base_count = table.base_function_count();
                if table
                    .methods
                    .iter()
                    .any(|m| m.is_base_program_method(base_count))
                {
                    return self.compile_module_call_via_method_table(
                        &base_qualified,
                        args,
                        kwargs,
                        kwargs_splat_mask,
                    );
                }
            }

            if let Some(result) = self.try_compile_public_collection_constructor_call(
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                kwargs_splat_mask.iter().any(|is_splat| *is_splat),
            )? {
                return Ok(result);
            }

            if is_method_dispatch_first_base_function(function) {
                return self.compile_call(function, args, kwargs, &[], &[]);
            }

            // Try to map to BuiltinOp first (handles types properly)
            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                return self.compile_builtin(&builtin_op, args);
            }
            // Fall back to string-based builtin call for functions not in BuiltinOp
            if is_base_function(function) {
                return self.compile_builtin_call(function, args);
            }
            // `Base.isexpr(...)` is already qualified, so treating it like bare
            // `isexpr(...)` incorrectly applies the unqualified import guard
            // (Issue #7527). Keep this narrow: other Base method-table-backed
            // calls rely on existing fallback behavior.
            if function == "isexpr" && self.method_tables.contains_key(function) {
                return self.compile_module_call_via_method_table(
                    function,
                    args,
                    kwargs,
                    kwargs_splat_mask,
                );
            }
            // For Pure Julia functions defined in Base (transpose, adjoint, etc.),
            // fall back to normal function call which uses the method table.
            return self.compile_call(function, args, kwargs, &[], &[]);
        }

        // Special handling for Base submodules: Base.Math, Base.IO, Base.Collections, etc.
        if let Some(submodule) = resolved_module.strip_prefix("Base.") {
            // Strip "Base." prefix

            // Special handling for MathConstants - these are constants, not functions
            if submodule == "MathConstants" {
                if let Some(value) = get_math_constant_value(function) {
                    if !args.is_empty() {
                        return err(format!(
                            "MathConstants.{} is a constant, not a function",
                            function
                        ));
                    }
                    self.emit(Instr::PushF64(value));
                    return Ok(ValueType::F64);
                }
                return err(format!(
                    "Base.MathConstants has no constant named {}",
                    function
                ));
            }

            // Special handling for Base.Iterators — forward to Pure Julia functions
            if submodule == "Iterators" {
                if function == "filter" {
                    if !kwargs.is_empty() {
                        return err("Base.Iterators.filter does not accept keyword arguments");
                    }
                    if has_splat {
                        return self.compile_module_splat_call(
                            "Base.Iterators.filter",
                            None,
                            args,
                            kwargs,
                            splat_mask,
                            kwargs_splat_mask,
                        );
                    }
                    if args.len() != 2 {
                        return err("Base.Iterators.filter requires exactly 2 arguments");
                    }
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::PushFunction("Base.Iterators.filter".to_string()));
                    self.emit(Instr::CallFunctionVariable(args.len()));
                    return Ok(ValueType::Any);
                }
                if matches!(function, "filter" | "map") {
                    if !kwargs.is_empty() {
                        return err(format!(
                            "Base.Iterators.{} does not accept keyword arguments",
                            function
                        ));
                    }
                    if has_splat {
                        return self.compile_module_splat_call(
                            &format!("Base.Iterators.{}", function),
                            None,
                            args,
                            kwargs,
                            splat_mask,
                            kwargs_splat_mask,
                        );
                    }
                    let method_table_name = format!("Iterators.{}", function);
                    if self.method_tables.contains_key(&method_table_name) {
                        return self.compile_module_call_via_method_table(
                            &method_table_name,
                            args,
                            kwargs,
                            kwargs_splat_mask,
                        );
                    }
                    if function == "map" {
                        return self.compile_builtin(&BuiltinOp::Generator, args);
                    }
                }
                if ITERATORS_FUNCTIONS.contains(&function) {
                    if has_splat {
                        return self.compile_module_splat_call(
                            &format!("Base.Iterators.{}", function),
                            None,
                            args,
                            kwargs,
                            splat_mask,
                            kwargs_splat_mask,
                        );
                    }
                    return self.compile_call(function, args, kwargs, &[], &[]);
                }
                return err(format!("Base.Iterators has no function named {}", function));
            }

            if submodule == "Collections" && matches!(function, "zeros" | "ones") {
                return self.compile_call(function, args, kwargs, &[], &[]);
            }

            // Forward Pure Julia math functions (sin, cos, etc.) through Base.Math
            if submodule == "Math"
                && matches!(
                    function,
                    "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "log"
                )
            {
                return self.compile_call(function, args, kwargs, &[], &[]);
            }

            if is_base_submodule_function(submodule, function) {
                // Try to map to BuiltinOp first (handles types properly)
                if let Some(builtin_op) = base_function_to_builtin_op(function) {
                    return self.compile_builtin(&builtin_op, args);
                }
                // Fall back to string-based builtin call
                if is_base_function(function) {
                    return self.compile_builtin_call(function, args);
                }
            }
            return err(format!(
                "Base.{} has no function named {}",
                submodule, function
            ));
        }

        // Special handling for Meta submodule (accessible as just "Meta" or "Base.Meta")
        if resolved_module == "Meta" {
            if is_base_submodule_function("Meta", function) {
                let argc = args.len();

                // isexpr and quot are Pure Julia - delegate to _meta_isexpr and _meta_quot
                match function {
                    "isexpr" => {
                        if !(2..=3).contains(&argc) {
                            return err(format!(
                                "Meta.isexpr expects 2 or 3 arguments, got {}",
                                argc
                            ));
                        }
                        return self.compile_call("_meta_isexpr", args, &[], &[], &[]);
                    }
                    "quot" => {
                        if argc != 1 {
                            return err(format!("Meta.quot expects 1 argument, got {}", argc));
                        }
                        return self.compile_call("_meta_quot", args, &[], &[], &[]);
                    }
                    "unblock" => {
                        if argc != 1 {
                            return err(format!("Meta.unblock expects 1 argument, got {}", argc));
                        }
                        return self.compile_call("_meta_unblock", args, &[], &[], &[]);
                    }
                    "unescape" => {
                        if argc != 1 {
                            return err(format!("Meta.unescape expects 1 argument, got {}", argc));
                        }
                        return self.compile_call("_meta_unescape", args, &[], &[], &[]);
                    }
                    "show_sexpr" => {
                        if argc != 1 {
                            return err(format!(
                                "Meta.show_sexpr expects 1 argument, got {}",
                                argc
                            ));
                        }
                        return self.compile_call("_meta_show_sexpr", args, &[], &[], &[]);
                    }
                    "lower" => {
                        if argc == 1 {
                            self.compile_expr(&args[0])?;
                            self.emit(Instr::CallBuiltin(BuiltinId::MetaLower, 1));
                            return Ok(ValueType::Any);
                        } else if argc == 2 {
                            self.compile_expr(&args[1])?;
                            self.emit(Instr::CallBuiltin(BuiltinId::MetaLower, 1));
                            return Ok(ValueType::Any);
                        } else {
                            return err(format!(
                                "Meta.lower expects 1 or 2 arguments, got {}",
                                argc
                            ));
                        }
                    }
                    _ => {}
                }

                // Compile arguments for Rust builtins
                for arg in args {
                    self.compile_expr(arg)?;
                }

                // Handle remaining Meta functions as Rust builtins
                match function {
                    "parse" => {
                        if argc == 1 {
                            self.emit(Instr::CallBuiltin(BuiltinId::MetaParse, 1));
                        } else if argc == 2 {
                            self.emit(Instr::CallBuiltin(BuiltinId::MetaParseAt, 2));
                        } else {
                            return err(format!(
                                "Meta.parse expects 1 or 2 arguments, got {}",
                                argc
                            ));
                        }
                        return Ok(ValueType::Any);
                    }
                    "isidentifier" => {
                        if argc != 1 {
                            return err(format!(
                                "Meta.isidentifier expects 1 argument, got {}",
                                argc
                            ));
                        }
                        self.emit(Instr::CallBuiltin(BuiltinId::MetaIsIdentifier, 1));
                        return Ok(ValueType::Bool);
                    }
                    "isoperator" => {
                        if argc != 1 {
                            return err(format!(
                                "Meta.isoperator expects 1 argument, got {}",
                                argc
                            ));
                        }
                        self.emit(Instr::CallBuiltin(BuiltinId::MetaIsOperator, 1));
                        return Ok(ValueType::Bool);
                    }
                    "isunaryoperator" => {
                        if argc != 1 {
                            return err(format!(
                                "Meta.isunaryoperator expects 1 argument, got {}",
                                argc
                            ));
                        }
                        self.emit(Instr::CallBuiltin(BuiltinId::MetaIsUnaryOperator, 1));
                        return Ok(ValueType::Bool);
                    }
                    "isbinaryoperator" => {
                        if argc != 1 {
                            return err(format!(
                                "Meta.isbinaryoperator expects 1 argument, got {}",
                                argc
                            ));
                        }
                        self.emit(Instr::CallBuiltin(BuiltinId::MetaIsBinaryOperator, 1));
                        return Ok(ValueType::Bool);
                    }
                    "ispostfixoperator" => {
                        if argc != 1 {
                            return err(format!(
                                "Meta.ispostfixoperator expects 1 argument, got {}",
                                argc
                            ));
                        }
                        self.emit(Instr::CallBuiltin(BuiltinId::MetaIsPostfixOperator, 1));
                        return Ok(ValueType::Bool);
                    }
                    _ => {
                        return err(format!("Meta.{} is not implemented", function));
                    }
                }
            }
            return err(format!("Meta has no function named {}", function));
        }

        // Special handling for Random module (stdlib)
        if resolved_module == "Random" {
            match function {
                "seed!" => {
                    return self.compile_builtin(&crate::ir::core::BuiltinOp::Seed, args);
                }
                "Xoshiro" => {
                    return self.compile_builtin(&crate::ir::core::BuiltinOp::XoshiroRNG, args);
                }
                "StableRNG" => {
                    return self.compile_builtin(&crate::ir::core::BuiltinOp::StableRNG, args);
                }
                "MersenneTwister" => {
                    return self
                        .compile_builtin(&crate::ir::core::BuiltinOp::MersenneTwisterRNG, args);
                }
                "rand" => {
                    return self.compile_builtin(&crate::ir::core::BuiltinOp::Rand, args);
                }
                "randn" => {
                    return self.compile_builtin(&crate::ir::core::BuiltinOp::Randn, args);
                }
                // default_rng()/GLOBAL_RNG return a handle to the VM's global
                // RNG so rand(default_rng())/randn(default_rng()) advance the
                // SAME stream as bare rand()/randn() (Issue #7230).
                "default_rng" | "GLOBAL_RNG" => {
                    if !args.is_empty() {
                        return err(format!("Random.{} takes no arguments", function));
                    }
                    self.emit(Instr::PushGlobalRng);
                    return Ok(ValueType::Rng);
                }
                _ => {
                    return err(format!("Random has no function named {}", function));
                }
            }
        }

        // Special handling for Core.Intrinsics - direct intrinsic calls
        if resolved_module == "Core.Intrinsics" {
            if let Some(intrinsic) = crate::intrinsics::Intrinsic::from_name(function) {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallIntrinsic(intrinsic));
                let return_type = match intrinsic {
                    crate::intrinsics::Intrinsic::EqInt
                    | crate::intrinsics::Intrinsic::NeInt
                    | crate::intrinsics::Intrinsic::SltInt
                    | crate::intrinsics::Intrinsic::SleInt
                    | crate::intrinsics::Intrinsic::SgtInt
                    | crate::intrinsics::Intrinsic::SgeInt
                    | crate::intrinsics::Intrinsic::EqFloat
                    | crate::intrinsics::Intrinsic::NeFloat
                    | crate::intrinsics::Intrinsic::LtFloat
                    | crate::intrinsics::Intrinsic::LeFloat
                    | crate::intrinsics::Intrinsic::GtFloat
                    | crate::intrinsics::Intrinsic::GeFloat => ValueType::Bool,
                    crate::intrinsics::Intrinsic::AddInt
                    | crate::intrinsics::Intrinsic::SubInt
                    | crate::intrinsics::Intrinsic::MulInt
                    | crate::intrinsics::Intrinsic::SdivInt
                    | crate::intrinsics::Intrinsic::SremInt
                    | crate::intrinsics::Intrinsic::NegInt
                    | crate::intrinsics::Intrinsic::AndInt
                    | crate::intrinsics::Intrinsic::OrInt
                    | crate::intrinsics::Intrinsic::XorInt
                    | crate::intrinsics::Intrinsic::NotInt
                    | crate::intrinsics::Intrinsic::ShlInt
                    | crate::intrinsics::Intrinsic::LshrInt
                    | crate::intrinsics::Intrinsic::AshrInt
                    | crate::intrinsics::Intrinsic::Fptosi => ValueType::I64,
                    _ => ValueType::F64,
                };
                return Ok(return_type);
            }
            return err(format!(
                "Core.Intrinsics has no intrinsic named {}",
                function
            ));
        }

        // Core.Compiler reflection aliases. sjulia does not expose Julia's full
        // compiler module, but the supported return-type query should route to
        // the same representative Base reflection surface (Issue #4288).
        if resolved_module == "Core.Compiler" {
            return match function {
                "return_type" => {
                    let reflect_args: Vec<Expr> = if args.len() == 3 {
                        args[..2].to_vec()
                    } else if args.len() == 2
                        && (is_signature_tuple_expr(&args[0])
                            || matches!(
                                self.resolve_static_datatype_value(&args[0]),
                                Some(JuliaType::TupleOf(_))
                            ))
                    {
                        vec![args[0].clone()]
                    } else {
                        args.to_vec()
                    };
                    self.compile_call("infer_return_type", &reflect_args, kwargs, &[], &[])
                }
                _ => err(format!("Core.Compiler.{} is not implemented", function)),
            };
        }

        if resolved_module == "Core" {
            return match function {
                "invoke" if !has_splat => self.compile_invoke_call(args, kwargs, kwargs_splat_mask),
                "invokelatest" => {
                    self.compile_call("invokelatest", args, kwargs, splat_mask, kwargs_splat_mask)
                }
                "OpaqueClosure" => err(unsupported_opaque_closure_message()),
                // `Core.eval(m, expr)` evaluates in module `m` — the primitive
                // that upstream's per-module `eval(x)` delegates to (Issue
                // #11421). Shares the BuiltinOp::Eval two-argument path.
                "eval" if !has_splat && kwargs.is_empty() && args.len() == 2 => {
                    self.compile_builtin(&BuiltinOp::Eval, args)
                }
                // Issue #4722: `Core.svec(...)` constructs a Core.SimpleVector.
                "svec" if !has_splat && kwargs.is_empty() => {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::MakeSimpleVector(args.len()));
                    Ok(ValueType::Any)
                }
                // Issue #5112: `Core.apply_type(T, params...)` constructs a
                // parametric type from computed type values, mirroring the
                // `T{params...}` literal path. The base `T` must resolve to a
                // static type name at compile time; the remaining parameters
                // are evaluated at runtime and may themselves be `...`-splats.
                "apply_type" if kwargs.is_empty() => self.compile_apply_type(args, splat_mask),
                _ => err(format!("Core.{} is not implemented", function)),
            };
        }

        // Special handling for Iterators module (Issue #2066, #2159)
        if resolved_module == "Iterators" {
            if function == "filter" {
                if !kwargs.is_empty() {
                    return err("Iterators.filter does not accept keyword arguments");
                }
                if has_splat {
                    return self.compile_module_splat_call(
                        "Iterators.filter",
                        None,
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                    );
                }
                if args.len() != 2 {
                    return err("Iterators.filter requires exactly 2 arguments");
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::PushFunction("Iterators.filter".to_string()));
                self.emit(Instr::CallFunctionVariable(args.len()));
                return Ok(ValueType::Any);
            }
            if matches!(function, "filter" | "map") {
                if !kwargs.is_empty() {
                    return err(format!(
                        "Iterators.{} does not accept keyword arguments",
                        function
                    ));
                }
                if has_splat {
                    return self.compile_module_splat_call(
                        &format!("Iterators.{}", function),
                        None,
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                    );
                }
                let method_table_name = format!("Iterators.{}", function);
                if self.method_tables.contains_key(&method_table_name) {
                    return self.compile_module_call_via_method_table(
                        &method_table_name,
                        args,
                        kwargs,
                        kwargs_splat_mask,
                    );
                }
                if function == "map" {
                    return self.compile_builtin(&BuiltinOp::Generator, args);
                }
            }
            if ITERATORS_FUNCTIONS.contains(&function) {
                if has_splat {
                    return self.compile_module_splat_call(
                        &format!("Iterators.{}", function),
                        None,
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                    );
                }
                return self.compile_call(function, args, kwargs, &[], &[]);
            }
            return err(format!(
                "Iterators module has no function named {}",
                function
            ));
        }

        // Private stdlib escape hatch: LinearAlgebra wrappers use these names
        // to reach VM kernels without exposing the invalid public
        // `Base.LinearAlgebra` module path (Issue #8276).
        if resolved_module == "LinearAlgebra"
            && function.starts_with("__sjulia_builtin_")
            && kwargs.is_empty()
            && !has_splat
        {
            let public_name = function.trim_start_matches("__sjulia_builtin_");
            if let Some(builtin) = linalg_builtin_for_function(public_name) {
                if args.len() != 1 {
                    return err(format!(
                        "{} requires exactly 1 argument: {}(A)",
                        public_name, public_name
                    ));
                }
                self.compile_expr(&args[0])?;
                self.emit(Instr::CallBuiltin(builtin, 1));
                return Ok(linalg_builtin_return_type(public_name));
            }
        }

        // Verify the module exists and contains the function
        let has_func = self
            .module_functions
            .get(resolved_module)
            .ok_or_else(|| CompileError::Msg(format!("Unknown module: {}", resolved_module)))?
            .contains(function);

        if !has_func {
            // A struct type defined inside the module is a valid module-qualified
            // constructor call (`M.Point(...)`), even though it is not registered in
            // `module_functions`. Route positional calls to the struct-table
            // constructor under the qualified name "M.Point" (Issue #7172). Use owned
            // names so no immutable borrow of `self` is held across the `&mut self`
            // constructor call below.
            let module_name = resolved_module.to_string();
            let func_name = function.to_string();
            let qualified = format!("{}.{}", module_name, func_name);
            if self.shared_ctx.enum_types.contains_key(&qualified)
                && args.len() == 1
                && kwargs.is_empty()
                && !has_splat
                && !kwargs_splat_mask.iter().any(|&is_splat| is_splat)
            {
                self.compile_expr(&args[0])?;
                self.emit(Instr::ConstructEnum(qualified));
                return Ok(ValueType::Enum);
            }
            if self
                .shared_ctx
                .runtime_nominal_callable_names
                .contains(&qualified)
            {
                return self.compile_runtime_datatype_value_call(
                    qualified,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                );
            }
            if kwargs.is_empty() && !has_splat {
                if self.shared_ctx.struct_table.contains_key(&qualified) {
                    if let Some(vt) =
                        self.try_compile_struct_table_constructor_call(&qualified, args)?
                    {
                        return Ok(vt);
                    }
                    let has_inner_constructor = self
                        .shared_ctx
                        .struct_table
                        .get(&qualified)
                        .map(|info| info.has_inner_constructor)
                        .unwrap_or(false);
                    if has_inner_constructor {
                        let method_table_name = if self.method_tables.contains_key(&qualified) {
                            qualified.as_str()
                        } else {
                            func_name.as_str()
                        };
                        if self.method_tables.contains_key(method_table_name) {
                            // Issue #7631: module-qualified inner constructors
                            // are registered by short struct name, while macro
                            // hygiene can emit `M.S()` calls. Route those calls
                            // through the constructor method table before reporting
                            // that the module has no function named `S`.
                            return self.compile_module_call_via_method_table(
                                method_table_name,
                                args,
                                kwargs,
                                kwargs_splat_mask,
                            );
                        }
                    }
                }
                if self.shared_ctx.parametric_structs.contains_key(&qualified) {
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    let method_table_name = if self.method_tables.contains_key(&qualified) {
                        qualified.as_str()
                    } else {
                        func_name.as_str()
                    };
                    let has_matching_constructor_method = self
                        .bare_parametric_constructor_dispatch_matches(
                            method_table_name,
                            &arg_types,
                        );
                    if let Some(vt) = (!has_matching_constructor_method)
                        .then(|| {
                            self.try_compile_inferred_parametric_constructor_call(&qualified, args)
                        })
                        .transpose()?
                        .flatten()
                    {
                        return Ok(vt);
                    }
                    if self.method_tables.contains_key(method_table_name) {
                        return self.compile_module_call_via_method_table(
                            method_table_name,
                            args,
                            kwargs,
                            kwargs_splat_mask,
                        );
                    }
                }
            }
            // The qualified name may be a binding re-exported from another module
            // via a selective `import/using Src: name`. Resolve the re-export chain
            // to its source and re-run the call there so `Facade.g(t)` dispatches
            // against `Defn.g` (Issue #8053).
            if let Some(source) = self.resolve_reexport_chain(&qualified) {
                if let Some((src_module, src_name)) = source.rsplit_once('.') {
                    let (src_module, src_name) = (src_module.to_string(), src_name.to_string());
                    return self.compile_resolved_module_call(
                        &src_module,
                        &src_name,
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                    );
                }
            }
            return err(format!(
                "Module {} has no function named {}",
                module_name, func_name
            ));
        }

        // Dispatch-first routing for bare `LinearAlgebra.<fn>(A)` (Issue #4020).
        // The qualified `LinearAlgebra.lu`/`det`/... method table holds only the
        // stdlib forwarder, so dispatching against it would ignore a user
        // override (`lu(A::Array) = ...`). Compile these exactly like the
        // unqualified `<fn>(A)` generic call instead: a more specific user
        // method wins, and with no override the forwarder runs and reaches the
        // builtin through LinearAlgebra's private VM-kernel bridge. Public
        // `Base.LinearAlgebra` remains invalid, matching upstream Julia
        // (Issue #8276).
        if resolved_module == "LinearAlgebra"
            && is_linalg_dispatch_first_function(function)
            && kwargs.is_empty()
        {
            return self.compile_call(function, args, kwargs, splat_mask, kwargs_splat_mask);
        }

        let qualified_function = format!("{}.{}", resolved_module, function);
        if let Some(target) = self
            .shared_ctx
            .type_aliases
            .get(&qualified_function)
            .cloned()
        {
            return self.compile_datatype_value_call(
                target,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }
        if self
            .module_constants
            .get(resolved_module)
            .is_some_and(|constants| constants.contains(function))
        {
            return self.compile_module_global_value_call(
                qualified_function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }
        let constructor_base = parse_parametric_call(function)
            .map(|(base, _)| base)
            .unwrap_or_else(|| function.to_string());
        let qualified_constructor_base = format!("{}.{}", resolved_module, constructor_base);
        let module_owns_constructor = self
            .shared_ctx
            .struct_table
            .contains_key(&qualified_constructor_base)
            || self
                .shared_ctx
                .parametric_structs
                .contains_key(&qualified_constructor_base);
        let has_exact_constructor_methods = self
            .method_tables
            .get(&qualified_function)
            .is_some_and(|table| !table.methods.is_empty());
        let explicit_parametric_constructor = parse_parametric_call(function).is_some();
        if module_owns_constructor
            && (has_splat
                || !kwargs.is_empty()
                || kwargs_splat_mask.iter().any(|is_splat| *is_splat))
            && (explicit_parametric_constructor || !has_exact_constructor_methods)
        {
            return self.compile_runtime_datatype_value_call(
                qualified_function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }
        let method_table_name = if self.method_tables.contains_key(&qualified_function) {
            qualified_function.as_str()
        } else {
            function
        };
        if kwargs.is_empty()
            && !has_splat
            && self
                .shared_ctx
                .struct_table
                .contains_key(&qualified_function)
        {
            let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
            let exact_constructor_matches =
                self.bare_parametric_constructor_dispatch_matches(&qualified_function, &arg_types);
            if let Some(vt) = (!exact_constructor_matches)
                .then(|| self.try_compile_struct_table_constructor_call(&qualified_function, args))
                .transpose()?
                .flatten()
            {
                return Ok(vt);
            }
        }
        if kwargs.is_empty()
            && !has_splat
            && self
                .shared_ctx
                .parametric_structs
                .contains_key(&qualified_function)
        {
            let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
            let exact_constructor_matches =
                self.bare_parametric_constructor_dispatch_matches(&qualified_function, &arg_types);
            if let Some(vt) = (!exact_constructor_matches)
                .then(|| {
                    self.try_compile_inferred_parametric_constructor_call(&qualified_function, args)
                })
                .transpose()?
                .flatten()
            {
                return Ok(vt);
            }
        }
        if has_splat {
            return self.compile_module_splat_call(
                &qualified_function,
                Some(method_table_name),
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
            );
        }
        self.compile_module_call_via_method_table(
            method_table_name,
            args,
            kwargs,
            kwargs_splat_mask,
        )
    }

    fn compile_module_global_value_call(
        &mut self,
        qualified_name: String,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        for arg in args {
            self.compile_expr(arg)?;
        }
        let has_splat = splat_mask.iter().any(|&is_splat| is_splat);
        let has_kwargs = !kwargs.is_empty();
        let has_kwargs_splat = kwargs_splat_mask.iter().any(|&is_splat| is_splat);
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        self.emit(Instr::LoadGlobalAny(qualified_name));
        if has_kwargs || has_splat || has_kwargs_splat {
            self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
                crate::bytecode::CallVarKwargsSplat {
                    arg_count: args.len(),
                    pos_splat_mask: splat_mask.to_vec(),
                    kwarg_names,
                    kwargs_splat_mask: kwargs_splat_mask.to_vec(),
                },
            )));
        } else {
            self.emit(Instr::CallFunctionVariable(args.len()));
        }
        Ok(ValueType::Any)
    }

    /// Compile `Core.apply_type(T, params...)` (Issue #5112).
    ///
    /// All calls use the runtime type-application path, even when the base is a
    /// statically visible type name. This keeps `Core.apply_type` arity/bound
    /// validation identical for literal, local, and splatted bases (#10554).
    fn compile_apply_type(&mut self, args: &[Expr], splat_mask: &[bool]) -> CResult<ValueType> {
        let Some((_, param_args)) = args.split_first() else {
            return err("Core.apply_type requires at least the base type argument");
        };

        for arg in args {
            self.compile_expr(arg)?;
        }
        if splat_mask.iter().any(|&is_splat| is_splat) {
            self.emit(Instr::ApplyTypeDynamicSplat(splat_mask.to_vec()));
        } else {
            self.emit(Instr::ApplyTypeDynamic(param_args.len()));
        }
        Ok(ValueType::DataType)
    }

    fn compile_module_splat_call(
        &mut self,
        qualified_function: &str,
        bare_constructor_method_table_name: Option<&str>,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        let resolved_candidate_indices =
            bare_constructor_method_table_name.and_then(|method_table_name| {
                self.method_tables.get(method_table_name).map(|table| {
                    let bare_constructor_table =
                        self.bare_parametric_constructor_table(method_table_name, table);
                    bare_constructor_table
                        .as_ref()
                        .unwrap_or(table)
                        .methods
                        .iter()
                        .map(|method| method.global_index)
                        .collect()
                })
            });
        for arg in args {
            self.compile_expr(arg)?;
        }
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        if let Some(candidate_indices) = resolved_candidate_indices {
            self.emit(Instr::PushResolvedFunction(Box::new(
                ResolvedFunctionOperands {
                    name: qualified_function.to_string(),
                    candidate_indices,
                },
            )));
        } else {
            self.emit(Instr::PushFunction(qualified_function.to_string()));
        }
        if kwargs.is_empty() && kwargs_splat_mask.iter().all(|is_splat| !*is_splat) {
            self.emit(Instr::CallFunctionVariableWithSplat(
                args.len(),
                splat_mask.to_vec(),
            ));
        } else {
            self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
                crate::bytecode::CallVarKwargsSplat {
                    arg_count: args.len(),
                    pos_splat_mask: splat_mask.to_vec(),
                    kwarg_names,
                    kwargs_splat_mask: kwargs_splat_mask.to_vec(),
                },
            )));
        }
        Ok(ValueType::Any)
    }

    fn compile_datatype_value_call(
        &mut self,
        type_name: String,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        if type_name.contains('{') {
            return self.compile_call(&type_name, args, kwargs, splat_mask, kwargs_splat_mask);
        }

        self.compile_runtime_datatype_value_call(
            type_name,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
        )
    }

    pub(super) fn compile_runtime_datatype_value_call(
        &mut self,
        type_name: String,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        let callee_temp = if let Some((base_name, type_args)) = parse_parametric_call(&type_name) {
            let _ = self
                .shared_ctx
                .resolve_instantiation_with_type_expr(&base_name, &type_args)?;
            if let Some(runtime_binding) = self.runtime_nominal_binding_name(&base_name) {
                // A runtime-conditional parametric family may be known to the
                // compiler even though its source branch was skipped. Resolve
                // the executed base binding before evaluating type arguments,
                // just as the non-parametric callee path resolves before value
                // arguments (Issue #11713).
                self.emit(Instr::ProbeRuntimeBinding(runtime_binding));
                self.emit(Instr::Pop);
            }
            for type_arg in &type_args {
                self.compile_type_expr_as_value(type_arg)?;
            }
            self.emit(Instr::ConstructParametricType(base_name, type_args.len()));
            let callee_temp = self.new_temp("parametric_constructor_callee");
            self.emit(Instr::StoreAny(callee_temp.clone()));
            callee_temp
        } else {
            // Julia resolves the callee before evaluating positional or
            // keyword arguments. Compiler metadata may know a later
            // runtime-conditional type, but only the executed binding is
            // callable at this source position (Issue #11713).
            if let Some(runtime_binding) = self.runtime_nominal_binding_name(&type_name) {
                self.emit(Instr::ProbeRuntimeBinding(runtime_binding));
            } else {
                // Statically installed nominal definitions do not enter the
                // runtime-publication set. Preserve their existing DataType
                // materialization while still evaluating the callee first
                // (#11716).
                self.emit(Instr::PushDataType(type_name));
            }
            let callee_temp = self.new_temp("runtime_constructor_callee");
            self.emit(Instr::StoreAny(callee_temp.clone()));
            callee_temp
        };

        for arg in args {
            self.compile_expr(arg)?;
        }
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        self.emit(Instr::LoadAny(callee_temp));
        let has_splat = splat_mask.iter().any(|&is_splat| is_splat);
        let has_kwargs = !kwargs.is_empty();
        let has_kwargs_splat = kwargs_splat_mask.iter().any(|&is_splat| is_splat);
        if has_kwargs || has_splat || has_kwargs_splat {
            self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
                crate::bytecode::CallVarKwargsSplat {
                    arg_count: args.len(),
                    pos_splat_mask: splat_mask.to_vec(),
                    kwarg_names,
                    kwargs_splat_mask: kwargs_splat_mask.to_vec(),
                },
            )));
        } else {
            self.emit(Instr::CallFunctionVariable(args.len()));
        }
        Ok(ValueType::Any)
    }

    fn compile_module_call_via_method_table(
        &mut self,
        method_table_name: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        let table = self.method_tables.get(method_table_name).ok_or_else(|| {
            CompileError::Msg(format!(
                "Internal error: function {} not found in method tables",
                method_table_name
            ))
        })?;
        // Use exactly the same constructor-self-family view for static
        // dispatch, runtime-dispatch policy, and every emitted candidate list.
        // Otherwise a compile-time miss may be filtered correctly while a
        // runtime candidate reintroduces the explicit `Box{T}` inner method.
        let bare_constructor_table =
            self.bare_parametric_constructor_table(method_table_name, table);
        let table = bare_constructor_table.as_ref().unwrap_or(table);

        let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
        if kwargs.is_empty()
            && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
            && args.len() > 1
            && arg_types.iter().any(|arg| {
                matches!(arg, JuliaType::Any)
                    || is_runtime_unknown_struct_arg(arg)
                    || arg.is_abstract_container()
            })
        {
            let has_any_arg = arg_types.iter().any(|ty| matches!(ty, JuliaType::Any));
            let static_dispatch_is_sufficient =
                table.dispatch(&arg_types).ok().is_some_and(|method| {
                    !should_runtime_dispatch(table, method, &arg_types, args.len(), has_any_arg)
                });
            if !static_dispatch_is_sufficient {
                let candidates = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| DynamicCallCandidate::Method(m.global_index))
                    .collect::<Vec<_>>();
                if !candidates.is_empty() {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit_dynamic_call(method_table_name, usize::MAX, args.len(), candidates);
                    if let Some(hof_ty) =
                        self.infer_hof_call_site_return_type(method_table_name, args)
                    {
                        return Ok(hof_ty);
                    }
                    return Ok(ValueType::Any);
                }
            }
        }
        let has_any_arg = arg_types.iter().any(|ty| matches!(ty, JuliaType::Any));
        let method = match table.dispatch(&arg_types) {
            Ok(method) => method,
            Err(crate::types::DispatchError::NoMethodFound { .. }) if has_any_arg => {
                if !kwargs.is_empty() || kwargs_splat_mask.iter().any(|is_splat| *is_splat) {
                    return self.emit_module_runtime_dispatched_kwargs_call(
                        method_table_name,
                        bare_constructor_table.as_ref(),
                        args,
                        kwargs,
                        kwargs_splat_mask,
                        false,
                    );
                }

                for arg in args {
                    self.compile_expr(arg)?;
                }

                if args.len() == 1 {
                    let candidates: Vec<DynamicCallCandidate> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 1)
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect();
                    if !candidates.is_empty() {
                        self.emit_dynamic_call(method_table_name, usize::MAX, 1, candidates);
                        return Ok(ValueType::Any);
                    }
                } else {
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();
                    if let Some(&fallback_index) = candidates.first() {
                        if candidates.len() == 1 {
                            self.emit_call_or_specialize(
                                method_table_name,
                                fallback_index,
                                args.len(),
                            );
                        } else {
                            self.emit(Instr::CallTypedDispatch(
                                method_table_name.to_string(),
                                args.len(),
                                fallback_index,
                                candidates,
                            ));
                        }
                        return Ok(ValueType::Any);
                    }
                }

                if bare_constructor_table
                    .as_ref()
                    .is_some_and(|table| table.methods.is_empty())
                {
                    // The argument expressions are already compiled above. Keep
                    // the compiler's empty bare-constructor view authoritative
                    // at runtime instead of rediscovering the excluded
                    // `Box{T}` inner by name. `CallFunctionVariable` raises the
                    // catchable MethodError only after those argument values
                    // have been evaluated (Issue #11147).
                    self.emit(Instr::PushResolvedFunction(Box::new(
                        ResolvedFunctionOperands {
                            name: method_table_name.to_string(),
                            candidate_indices: vec![],
                        },
                    )));
                    self.emit(Instr::CallFunctionVariable(args.len()));
                    return Ok(ValueType::Any);
                }

                // Issue #7793: the qualified analog of the bare-call fix in
                // `compile_generic_dispatch_call`. An outer constructor registers
                // the struct under the qualified method-table name (`M.Foo`),
                // whose declared constructors miss this call's arity, but the
                // synthesized field-count default constructor still exists in
                // `struct_table`. Fall back to it when the arity matches the
                // field count (the field-count built-in ctor, not a re-dispatch).
                if let Some(vt) =
                    self.try_struct_field_count_default_ctor_fallback(method_table_name, args)?
                {
                    return Ok(vt);
                }
                if kwargs.is_empty() {
                    // Issue #11078: collect eagerly (see the note in
                    // `dispatch.rs` — the registry's opaque iterator would hold
                    // the `&self` borrow across the `&mut self` call). `take(2)`
                    // preserves the original "exactly one match" test without
                    // walking the whole table.
                    let mut matches: Vec<_> = self
                        .shared_ctx
                        .struct_table
                        .iter()
                        .filter(|(name, info)| {
                            (name.as_str() == method_table_name
                                || name.rsplit('.').next() == Some(method_table_name))
                                && !info.has_inner_constructor
                                && info.fields.len() == args.len()
                        })
                        .map(|(_, info)| info.clone())
                        .take(2)
                        .collect();
                    if matches.len() == 1 {
                        return self.compile_struct_constructor(matches.remove(0), args);
                    }
                }

                if self.in_catchable_runtime_error_region() {
                    return self.emit_qualified_catchable_no_method_error_call(
                        method_table_name,
                        args,
                        &arg_types,
                        true,
                    );
                }

                return err(format!(
                    "No method matching {}({:?})",
                    method_table_name, arg_types
                ));
            }
            Err(err @ crate::types::DispatchError::NoMethodFound { .. }) => {
                // Issue #7793: also cover the fully-concrete (no `Any` arg) miss.
                // A same-arity declared constructor whose types do not match
                // leaves the synthesized field-count default constructor as the
                // only valid candidate; fall back to it before surfacing the
                // dispatch error. Restricted to `NoMethodFound` so ambiguity
                // resolution is left unchanged.
                if kwargs.is_empty() {
                    if let Some(vt) =
                        self.try_struct_field_count_default_ctor_fallback(method_table_name, args)?
                    {
                        return Ok(vt);
                    }
                }
                if self.in_catchable_runtime_error_region() {
                    if !kwargs.is_empty() || kwargs_splat_mask.iter().any(|is_splat| *is_splat) {
                        return self.emit_module_runtime_dispatched_kwargs_call(
                            method_table_name,
                            bare_constructor_table.as_ref(),
                            args,
                            kwargs,
                            kwargs_splat_mask,
                            false,
                        );
                    }
                    return self.emit_qualified_catchable_no_method_error_call(
                        method_table_name,
                        args,
                        &arg_types,
                        false,
                    );
                }
                return Err(err.into());
            }
            Err(err) => return Err(err.into()),
        };
        // Shared dispatch policy (Issue #8158): a qualified `Module.f(x)` call
        // must defer to runtime multiple dispatch in exactly the same cases as
        // the unqualified `f(x)` call (`compile_generic_dispatch_call`). Without
        // this, a wide `Any` argument statically bound the catch-all `f(::Any)`
        // here even though the unqualified path runtime-dispatched — so
        // `SciMLBase._callbacks(cb::CallbackSet)` selected the `(cb,)` catch-all
        // and silently disabled every callback in a `CallbackSet`.
        let use_runtime_dispatch =
            should_runtime_dispatch(table, method, &arg_types, args.len(), has_any_arg);

        let has_callsite_kwargs =
            !kwargs.is_empty() || kwargs_splat_mask.iter().any(|is_splat| *is_splat);
        if has_callsite_kwargs
            && (use_runtime_dispatch
                || self.keyword_call_requires_runtime_dispatch(
                    table,
                    method,
                    &arg_types,
                    kwargs,
                    kwargs_splat_mask,
                ))
        {
            return self.emit_module_runtime_dispatched_kwargs_call(
                method_table_name,
                bare_constructor_table.as_ref(),
                args,
                kwargs,
                kwargs_splat_mask,
                false,
            );
        }

        // Compile positional arguments with expected types.
        // For abstract numeric types (Number, Real, Integer, etc.) and narrow integers,
        // do not coerce — preserve the actual argument type so the function body receives
        // the correct Value variant (e.g., Value::I64 not Value::F64 for domath(37)).
        // This mirrors the identical guard in compile_call (mod.rs:1692).
        if let Some(vararg_idx) = method.vararg_param_index {
            for (idx, arg) in args.iter().enumerate() {
                if idx < vararg_idx {
                    if use_runtime_dispatch {
                        self.compile_expr(arg)?;
                    } else if idx < method.param_count() {
                        // Coercion gate sourced core-projection-first via the
                        // canonical inverse (Issue #6495, stage 7a);
                        // `params.len()` is an arity read.
                        let param_ty = method.projected_param_julia_type(idx);
                        if *param_ty == JuliaType::Any
                            || param_ty.is_narrow_integer()
                            || param_ty.is_abstract_integer()
                            || param_ty.is_abstract_container()
                            || param_ty.is_abstract_with_struct_subtypes()
                            || is_dict_annotation(&param_ty)
                        {
                            self.compile_expr(arg)?;
                        } else {
                            let vt = julia_type_to_value_type(&param_ty);
                            self.compile_expr_as(arg, vt)?;
                        }
                    } else {
                        self.compile_expr(arg)?;
                    }
                } else {
                    self.compile_expr(arg)?;
                }
            }
        } else {
            // Non-varargs: compile args paired with params (the
            // `take(params.len())` mirrors the historical `zip` truncation;
            // the coercion gate reads the canonical `core_signature`
            // projection first — Issue #6495, stage 7a).
            for (idx, arg) in args.iter().enumerate().take(method.param_count()) {
                // Runtime dispatch, an `Any` param, and narrow/abstract integer
                // params all compile the argument as-is (no static coercion);
                // every other concrete param coerces to the param's value type.
                if use_runtime_dispatch {
                    self.compile_expr(arg)?;
                    continue;
                }
                let param_ty = method.projected_param_julia_type(idx);
                if *param_ty == JuliaType::Any
                    || param_ty.is_narrow_integer()
                    || param_ty.is_abstract_integer()
                    || param_ty.is_abstract_container()
                    || param_ty.is_abstract_with_struct_subtypes()
                    || is_dict_annotation(&param_ty)
                {
                    self.compile_expr(arg)?;
                } else {
                    let vt = julia_type_to_value_type(&param_ty);
                    self.compile_expr_as(arg, vt)?;
                }
            }
        }

        if kwargs.is_empty() && kwargs_splat_mask.iter().all(|is_splat| !*is_splat) {
            if use_runtime_dispatch {
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();
                if candidates.is_empty() {
                    self.emit_call_or_specialize(
                        method_table_name,
                        method.global_index,
                        args.len(),
                    );
                } else if has_any_arg
                    || should_use_dynamic_call_for_runtime_dispatch(method, &arg_types, args.len())
                {
                    self.emit_dynamic_call(
                        method_table_name,
                        method.global_index,
                        args.len(),
                        candidates
                            .into_iter()
                            .map(DynamicCallCandidate::Method)
                            .collect(),
                    );
                } else {
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        method.global_index,
                        candidates,
                    ));
                }
            } else {
                self.emit_call_or_specialize(method_table_name, method.global_index, args.len());
            }
        } else {
            if use_runtime_dispatch {
                return self.emit_module_runtime_dispatched_kwargs_call(
                    method_table_name,
                    bare_constructor_table.as_ref(),
                    args,
                    kwargs,
                    kwargs_splat_mask,
                    true,
                );
            }
            let kwarg_names: Vec<String> =
                kwargs.iter().map(|(name, _)| name.to_string()).collect();
            for (_, value) in kwargs {
                self.compile_expr(value)?;
            }
            if kwargs_splat_mask.iter().any(|is_splat| *is_splat) {
                self.emit(Instr::CallWithKwargsSplat(
                    method.global_index,
                    args.len(),
                    kwarg_names,
                    kwargs_splat_mask.to_vec(),
                ));
            } else {
                self.emit(Instr::CallWithKwargs(
                    method.global_index,
                    args.len(),
                    kwarg_names,
                ));
            }
        }

        Ok(method.return_type.clone())
    }

    /// Compile a module-qualified function reference: Module.func
    pub(in crate::compile) fn compile_module_function_ref(
        &mut self,
        module: &str,
        function: &str,
    ) -> CResult<ValueType> {
        let owned_module_path = self.module_path_in_current_scope(module);
        if owned_module_path.is_none()
            && !crate::compile::constants::is_stdlib_module(
                module.split_once('.').map_or(module, |(root, _)| root),
            )
        {
            if let Some(root) = self.imported_binding_root(module).map(str::to_string) {
                self.emit_load_imported_binding(&root);
                for segment in module.split('.').skip(1) {
                    self.emit(Instr::GetFieldByName(segment.to_string()));
                }
                self.emit(Instr::GetFieldByName(function.to_string()));
                return Ok(ValueType::Any);
            }
        }
        let visible_static_binding = owned_module_path.is_some()
            || self.resolved_module_alias(module).is_some()
            || crate::compile::constants::is_stdlib_module(
                module.split_once('.').map_or(module, |(root, _)| root),
            );
        let resolved_module_owned = owned_module_path.unwrap_or_else(|| {
            self.resolved_module_alias(module)
                .unwrap_or(module)
                .to_string()
        });
        let canonical = self.resolve_module_alias_path(&resolved_module_owned);
        if !visible_static_binding
            && (self.is_known_module_path(&canonical)
                || self.is_ambiguous_module_alias_root(module))
        {
            return Ok(self.emit_unbound_module_name(module));
        }

        self.compile_resolved_module_function_ref(&resolved_module_owned, function)
    }

    pub(in crate::compile) fn compile_resolved_module_function_ref(
        &mut self,
        module: &str,
        function: &str,
    ) -> CResult<ValueType> {
        let resolved_module_owned = self
            .resolved_module_alias(module)
            .unwrap_or(module)
            .to_string();
        let resolved_module_owned = self.canonical_module_path(&resolved_module_owned);
        let resolved_module = resolved_module_owned.as_str();

        if resolved_module == "Base" {
            // `Base.RefValue` is the concrete `RefValue{T}` box's UnionAll type
            // object (Issue #5130/#5223). As a bare value it must resolve to a
            // `DataType`-style type object (so `typeof(Base.RefValue) ===
            // UnionAll` and the type predicates work) rather than the `Ref`
            // constructor function. Construction (`Base.RefValue{T}(x)`) is
            // intercepted earlier in the call path.
            if function == "RefValue" {
                self.emit(Instr::PushDataType("Base.RefValue".to_string()));
                return Ok(ValueType::DataType);
            }
            // `Base.<X>` where X is a Base TYPE (e.g. `Base.OneTo`, a range/struct
            // type) is a type object, not a function — resolve it to a `DataType`
            // so `Base.OneTo <: AbstractUnitRange` works, mirroring the unqualified
            // `OneTo` type-name path (Issue #5874). This MUST run before the
            // `is_base_function` allowlist check below: a handful of allowlist
            // entries are themselves type/constructor names (`Int`, `String`,
            // `Char`, `Ref`, `IOBuffer`, `MersenneTwister`) that the allowlist
            // needs recognized for their CALL-path conversion behavior
            // (`Int(3.0)`, `String(chars)`, …), but as a bare VALUE (not
            // called) they must resolve to the type object — `Base.Int isa
            // Function` is `false` upstream, matching the bare (unqualified)
            // `Int` value path in `compile/expr/mod.rs`, which likewise checks
            // `is_builtin_type_name` before `is_base_function` (Issue #10284
            // follow-up: emitting a clean bare function-value name for these
            // after the #10284 fix made `isa Function` wrongly flip to `true`
            // for them, since the pre-fix qualified-name corruption had
            // accidentally also produced `false`).
            if crate::compile::expr::is_builtin_type_name(function)
                || self.abstract_type_names.contains(function)
                || self.shared_ctx.struct_table.contains_key(function)
                || self.shared_ctx.parametric_structs.contains_key(function)
                || self.shared_ctx.enum_types.contains_key(function)
                || self.shared_ctx.is_primitive_type_name(function)
            {
                self.emit(Instr::PushDataType(function.to_string()));
                return Ok(ValueType::DataType);
            }
            if is_base_function(function)
                || base_function_to_builtin_op(function).is_some()
                || function_name_to_binary_op(function).is_some()
            {
                // Issue #10077/#10284: a qualified `Base.<fn>` value must
                // carry the same runtime type identity as the unqualified
                // `<fn>` (e.g. `typeof(Base.sqrt) === typeof(sqrt)`, both
                // `isa Function`). These allowlisted names (BuiltinOp/
                // intrinsic-backed math/IO functions and operators) are
                // NEVER registered in `method_tables` under a genuine
                // `"Base.<fn>"` qualified key — Base's own Pure Julia
                // extensions/methods for them live under the bare `<fn>` key
                // (mirroring the general `Base.<fn>`-as-value convention a
                // few lines below for #8137, e.g. `Base.sin`/`Base.map`).
                // Prefer a qualified key if one ever exists (keeps candidate
                // resolution correct while still emitting the bare display
                // name, exactly like `emit_function_value_named`'s general
                // contract), otherwise resolve through the bare key so
                // `Base.sqrt`/`Base.:+` share the exact same candidate set
                // (and hence calling/HOF-passing behavior) as bare
                // `sqrt`/`+` instead of falling back to the qualified
                // spelling as the value's baked-in identity.
                let qualified_function = format!("Base.{}", function);
                if self.method_tables.contains_key(&qualified_function) {
                    self.emit_function_value_named(function, &qualified_function);
                } else {
                    self.emit_function_value(function);
                }
                return Ok(ValueType::Any);
            }
            // Issues #4960-#4966: these Base reflection / conversion / promotion
            // helpers are not in the `is_base_function` allowlist but DO resolve as
            // callable function values through the bare-identifier path (which emits
            // a `LoadAny` that the VM resolves to the named function at runtime).
            // Delegate to that path so `Base.<fn>` behaves identically to the
            // unqualified `<fn>` function value instead of erroring.
            if is_base_reflection_function_value(function) {
                let span = crate::span::Span::new(0, 0, 0, 0, 0, 0);
                return self.compile_expr(&crate::ir::core::Expr::Var(
                    function.to_string().into(),
                    span,
                ));
            }
            // General `Base.<fn>`-as-value access (Issue #8137): any ordinary
            // Base function backed by a method table — i.e. the now-Pure-Julia
            // Base functions like `map`, `filter`, `sin`, `cos`, `reduce`,
            // `foldl`, … which are NOT in the `is_base_function` allowlist —
            // resolves to the SAME callable function value as the unqualified
            // `<fn>`. Emit `PushResolvedFunction` directly (via
            // `emit_function_value`) rather than delegating to the bare
            // identifier path: qualified `Base.<fn>` must resolve to the
            // function regardless of an import or a same-named local shadow
            // (`map = 5; Base.map` is still the function), which the bare-`Var`
            // path would not guarantee. Continues the `Base.<fn>` value-lookup
            // series (#4960-#4966 / umbrella #4119) for the general case.
            let qualified_function = format!("Base.{}", function);
            if self.method_tables.contains_key(&qualified_function) {
                self.emit_function_value(&qualified_function);
                return Ok(ValueType::Function);
            }
            if self.method_tables.contains_key(function) {
                self.emit_function_value(function);
                return Ok(ValueType::Function);
            }
            // Base-internal const type aliases (Base.Bottom, Base.BitSigned,
            // ...) resolve as DataType values (Issue #10579); see
            // `prelude_base_type_alias_target`.
            if let Some(target) =
                crate::compile::pipeline_ctx::prelude_base_type_alias_target(function)
            {
                self.emit(Instr::PushDataType(target));
                return Ok(ValueType::DataType);
            }
            return err(format!("Base has no function named {}", function));
        }

        if self
            .module_constants
            .get(resolved_module)
            .is_some_and(|constants| constants.contains(function))
        {
            self.emit(Instr::LoadGlobalAny(format!(
                "{}.{}",
                resolved_module, function
            )));
            return Ok(ValueType::Any);
        }

        if let Some(submodule) = resolved_module.strip_prefix("Base.") {
            if submodule == "Iterators" {
                if function == "map" {
                    self.emit(Instr::PushFunction("Base.Iterators.map".to_string()));
                    return Ok(ValueType::Any);
                }
                if ITERATORS_FUNCTIONS.contains(&function) {
                    self.emit(Instr::PushFunction(format!("Base.Iterators.{}", function)));
                    return Ok(ValueType::Any);
                }
                return err(format!("Base.Iterators has no function named {}", function));
            }

            if submodule == "Math"
                && matches!(
                    function,
                    "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "exp" | "log"
                )
            {
                self.emit(Instr::PushFunction(function.to_string()));
                return Ok(ValueType::Any);
            }

            if is_base_submodule_function(submodule, function) {
                self.emit(Instr::PushFunction(format!(
                    "Base.{}.{}",
                    submodule, function
                )));
                return Ok(ValueType::Any);
            }
            return err(format!(
                "Base.{} has no function named {}",
                submodule, function
            ));
        }

        if resolved_module == "Random" {
            // default_rng()/GLOBAL_RNG accessed without a call (e.g. as
            // `Random.GLOBAL_RNG`) push the global RNG handle (Issue #7230).
            if function == "default_rng" || function == "GLOBAL_RNG" {
                self.emit(Instr::PushGlobalRng);
                return Ok(ValueType::Rng);
            }
            if is_random_function(function) {
                self.emit(Instr::PushFunction(format!("Random.{}", function)));
                return Ok(ValueType::Any);
            }
            return err(format!("Random has no function named {}", function));
        }

        // Special handling for Iterators module (Issue #2066, #2159)
        if resolved_module == "Iterators" {
            if function == "map" {
                self.emit(Instr::PushFunction("Iterators.map".to_string()));
                return Ok(ValueType::Any);
            }
            if ITERATORS_FUNCTIONS.contains(&function) {
                self.emit(Instr::PushFunction(format!("Iterators.{}", function)));
                return Ok(ValueType::Any);
            }
            return err(format!(
                "Iterators module has no function named {}",
                function
            ));
        }

        if resolved_module == "Core" && function == "OpaqueClosure" {
            return err(unsupported_opaque_closure_message());
        }

        if self
            .module_constants
            .get(resolved_module)
            .is_some_and(|constants| constants.contains(function))
        {
            self.emit(Instr::LoadGlobalAny(format!(
                "{}.{}",
                resolved_module, function
            )));
            return Ok(ValueType::Any);
        }

        // Issue #4722: `Core.SimpleVector` is the type of `<DataType>.parameters`
        // (svec). Surface it as a DataType value so `isa(x, Core.SimpleVector)`
        // and `typeof(x) === Core.SimpleVector` resolve. `Core.svec` is its
        // constructor function value.
        if resolved_module == "Core" {
            match function {
                "SimpleVector" => {
                    self.emit(Instr::PushDataType("Core.SimpleVector".to_string()));
                    return Ok(ValueType::DataType);
                }
                "svec" => {
                    self.emit(Instr::PushFunction("Core.svec".to_string()));
                    return Ok(ValueType::Function);
                }
                // Issue #5129: `Core.Builtin` is the abstract supertype of
                // genuine built-in functions (`Core.Builtin <: Function`).
                // Surface it as a DataType value so `isa(===, Core.Builtin)`,
                // `typeof(===) <: Core.Builtin`, and `Core.Builtin <: Function`
                // all resolve.
                "Builtin" => {
                    self.emit(Instr::PushDataType("Core.Builtin".to_string()));
                    return Ok(ValueType::DataType);
                }
                // Issue #9967: `Core.TypeofBottom` is the singleton type of the
                // bottom type object `Union{}` (`typeof(Union{}) ===
                // Core.TypeofBottom`). Surface it as the same `DataType` value
                // the `TypeOf` builtin already produces for `Union{}`
                // (`vm/builtins_types.rs`), so the qualified reference and the
                // `typeof` result are `===`-identical without a bespoke
                // by-name branch elsewhere.
                "TypeofBottom" => {
                    self.emit(Instr::PushDataType("Core.TypeofBottom".to_string()));
                    return Ok(ValueType::DataType);
                }
                _ => {}
            }
            // `Core` is never registered in `module_functions` (it is not a
            // user-defined module), so any name reaching here is not one of
            // the `Core` bindings sjulia models above. Without this, the
            // generic `module_functions.get(resolved_module)` lookup below
            // fails with "Unknown module: Core" (Issue #9967) — misleading
            // now that `Core` itself resolves. Report the unresolved binding
            // instead, mirroring the `Base has no function named {}` message
            // for the analogous `Base` case above.
            return err(format!("Core has no binding named {}", function));
        }

        let qualified_name = format!("{}.{}", resolved_module, function);
        let module_funcs = self
            .module_functions
            .get(resolved_module)
            .ok_or_else(|| CompileError::Msg(format!("Unknown module: {}", resolved_module)))?;

        if self.module_exports.contains_key(&qualified_name)
            || self.module_functions.contains_key(&qualified_name)
        {
            let exports = self
                .module_exports
                .get(&qualified_name)
                .map(|set| {
                    let mut exports: Vec<String> = set.iter().cloned().collect();
                    exports.sort();
                    exports
                })
                .unwrap_or_default();
            self.emit(Instr::PushModule(Box::new(ModuleOperands {
                name: qualified_name,
                exports,
                publics: vec![],
                base_exports_visible: true,
                implicit_standard_bindings: true,
            })));
            return Ok(ValueType::Module);
        }

        if let Some(target) = self.shared_ctx.type_aliases.get(&qualified_name) {
            self.emit(Instr::PushDataType(target.clone()));
            return Ok(ValueType::DataType);
        }

        if self.abstract_type_names.contains(&qualified_name)
            || self.shared_ctx.enum_types.contains_key(&qualified_name)
            || self.shared_ctx.is_primitive_type_name(&qualified_name)
            || self.shared_ctx.struct_table.contains_key(&qualified_name)
            || self
                .shared_ctx
                .parametric_structs
                .contains_key(&qualified_name)
        {
            self.emit(Instr::PushDataType(qualified_name.clone()));
            return Ok(ValueType::DataType);
        }

        if !module_funcs.contains(function) {
            // `Module.X` where X is a TYPE the module defines (concrete struct,
            // parametric struct, abstract type, type alias, `@enum`, or user primitive) is a
            // type object, not a function — resolve it to a `DataType` value so
            // `isa(v, Module.T)`, `Module.T <: U`, `Module.T(args)`, and a bare
            // `Module.T` reference all work. This mirrors the unqualified
            // type-name path (`compile_expr` on `Expr::Var`) and the
            // `Base.<Type>` branch above. Checked only after the module's own
            // functions so a real function binding still wins. Ownership is
            // proven with the qualified name to avoid leaking unrelated bare
            // types into arbitrary modules.
            if self.abstract_type_names.contains(&qualified_name)
                || self.shared_ctx.enum_types.contains_key(&qualified_name)
                || self.shared_ctx.is_primitive_type_name(&qualified_name)
                || self.shared_ctx.type_aliases.contains_key(&qualified_name)
                || self.shared_ctx.struct_table.contains_key(&qualified_name)
                || self
                    .shared_ctx
                    .parametric_structs
                    .contains_key(&qualified_name)
            {
                self.emit(Instr::PushDataType(qualified_name));
                return Ok(ValueType::DataType);
            }
            // The qualified name may be a binding re-exported from another module
            // via a selective `import/using Src: name`. Resolve the re-export chain
            // to its source and re-run the reference there so a bare `Facade.T`
            // value (or `Facade.g` function value) resolves to `Defn.T` / `Defn.g`
            // (Issue #8053).
            if let Some(source) = self.resolve_reexport_chain(&qualified_name) {
                if let Some((src_module, src_name)) = source.rsplit_once('.') {
                    let (src_module, src_name) = (src_module.to_string(), src_name.to_string());
                    return self.compile_resolved_module_function_ref(&src_module, &src_name);
                }
            }
            self.emit_module_value(resolved_module);
            self.emit(Instr::GetFieldByName(function.to_string()));
            return Ok(ValueType::Any);
        }

        // Issue #10077: `Module.func` (module-qualified) and `func`
        // (bare/imported) capture the exact same generic function upstream,
        // with the exact same `typeof`/`isa Function` identity. The registry
        // key `qualified_name` disambiguates same-named functions across
        // modules, but must not leak into the captured value's runtime type
        // — `emit_function_value_named` resolves candidates through the
        // qualified key while emitting the bare display name.
        self.emit_function_value_named(function, &qualified_name);
        Ok(ValueType::Any)
    }
}

/// LinearAlgebra factorization/query functions that are backed by a nalgebra
/// builtin but may be overridden by user methods. A bare `LinearAlgebra.<fn>(A)`
/// qualifier honours dispatch-first routing for these (Issue #4020): it is
/// compiled exactly like the unqualified `<fn>(A)` generic call, so a more
/// specific user method (`lu(A::Array) = ...`) wins, and otherwise the stdlib
/// forwarder runs and reaches the builtin through LinearAlgebra's private
/// VM-kernel bridge.
fn is_linalg_dispatch_first_function(function: &str) -> bool {
    matches!(
        function,
        "inv" | "svd" | "qr" | "eigen" | "eigvals" | "cholesky" | "cond" | "lu" | "det"
    )
}

fn linalg_builtin_for_function(function: &str) -> Option<BuiltinId> {
    match function {
        "det" => Some(BuiltinId::Det),
        "lu" => Some(BuiltinId::Lu),
        "inv" => Some(BuiltinId::Inv),
        "svd" => Some(BuiltinId::Svd),
        "qr" => Some(BuiltinId::Qr),
        "eigen" => Some(BuiltinId::Eigen),
        "eigvals" => Some(BuiltinId::Eigvals),
        "cholesky" => Some(BuiltinId::Cholesky),
        "cond" => Some(BuiltinId::Cond),
        _ => None,
    }
}

fn linalg_builtin_return_type(function: &str) -> ValueType {
    match function {
        "det" | "cond" => ValueType::F64,
        "svd" | "qr" | "eigen" | "cholesky" => ValueType::NamedTuple,
        "lu" => ValueType::Tuple,
        _ => ValueType::Array,
    }
}
