//! Generic method-table dispatch tail of `compile_call` (Issue #6332).
//!
//! This module hosts the former tail of `compile_call` — the generic
//! multiple-dispatch path over `method_tables` (including its dispatch-error
//! fallback arms and per-function return-type overrides) and the
//! no-method-table builtin fallback. It was moved here **verbatim** as a pure
//! extraction: `compile_call` tail-calls
//! [`CoreCompiler::compile_generic_dispatch_call`] after all table-driven
//! special-case handlers and constructor resolution have fallen through, so
//! evaluation order and behavior are unchanged.

use crate::builtins::BuiltinId;
use crate::inference_core::CoreType;
use crate::ir::core::{BuiltinOp, Expr, Literal};
use crate::types::{nominal_family_name, JuliaType};
use crate::vm::{DynamicCallCandidate, Instr, ValueType};

use crate::compile::{
    base_function_to_builtin_op, err, is_base_function, is_builtin_type_name,
    is_method_dispatch_first_base_function, is_random_function, is_reducible_nary_operator,
    julia_type_to_value_type, CResult, CompileError, CoreCompiler,
};

use super::{core_is_abstract_array_family_type, is_rank_unknown_array_julia_type};

pub(super) fn is_dict_annotation(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::Dict)
        || matches!(ty, JuliaType::Struct(name) if name.split('{').next() == Some("Dict"))
}

fn is_truncated_result_call(function: &str, args: &[Expr], kwargs: &[(String, Expr)]) -> bool {
    matches!(function, "truncated" | "Distributions.truncated")
        && (args.len() >= 2
            || kwargs
                .iter()
                .any(|(_, value)| !matches!(value, Expr::Literal(Literal::Nothing, _))))
}

pub(super) fn is_runtime_unknown_struct_arg(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::Struct(name) if !is_callable_singleton_struct_name(name))
}

fn is_callable_singleton_struct_name(name: &str) -> bool {
    name.starts_with("typeof(") && name.ends_with(')')
}

impl CoreCompiler<'_> {
    pub(in crate::compile) fn emit_runtime_dispatched_kwargs_call(
        &mut self,
        method_table_name: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        kwargs_splat_mask: &[bool],
        args_already_compiled: bool,
    ) -> CResult<ValueType> {
        if !args_already_compiled {
            for arg in args {
                self.compile_expr(arg)?;
            }
        }

        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.clone()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        // Keyword runtime dispatch must preserve the exact method-table
        // candidates. Some visible exported tables, such as the bare
        // `solve` table after `using OrdinaryDiffEq`, contain qualified method
        // bodies only (`SciMLBase.solve` / `OrdinaryDiffEq.solve`), so a plain
        // `PushFunction("solve")` cannot recover them at runtime (Issue #8396).
        self.emit_function_value(method_table_name);
        self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
            crate::vm::CallVarKwargsSplat {
                arg_count: args.len(),
                pos_splat_mask: vec![false; args.len()],
                kwarg_names,
                kwargs_splat_mask: kwargs_splat_mask.to_vec(),
            },
        )));
        Ok(ValueType::Any)
    }

    /// Issue #7793: synthesized field-count default-constructor fallback for
    /// the multi-arg / static-miss `NoMethodFound` recovery arms.
    ///
    /// Defining any user **outer** constructor registers the struct name as a
    /// function with a method table that contains only the declared
    /// constructors — never the synthesized field-count default constructor.
    /// A bare (or short-name-routed qualified) top-level call whose arity
    /// differs from every declared constructor therefore misses dispatch with
    /// `NoMethodFound`, and the multi-arg / static-miss arms below build their
    /// candidate set from `accepts_arity(args.len())`, find none, and would
    /// error — even though upstream Julia still synthesizes (and keeps
    /// reachable) the field-count default constructor `Foo(::F1, ..., ::Fn)`.
    ///
    /// When `function` names a struct in `struct_table` and the call arity
    /// equals its field count, fall back to `compile_struct_constructor`
    /// (the field-count **built-in** constructor — NOT a re-dispatch to the
    /// user method, which would re-enter this same miss / recurse). This
    /// mirrors the single-arg arm (the `args.len() == 1` recovery already does
    /// the same via `struct_table.get(function)`), so all arities behave
    /// consistently. `struct_table` keeps both the qualified `M.Foo` and the
    /// bare `Foo` keys, so this also covers the qualified analog routed here
    /// under the short name (same root-cause family as #7729).
    pub(super) fn try_struct_field_count_default_ctor_fallback(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        // Upstream Julia only synthesizes (and keeps reachable) the field-count
        // default constructor when the struct declares NO inner constructor. A
        // struct WITH an inner constructor that does not accept this call is a
        // genuine `MethodError`, so do not manufacture a default constructor for
        // it here (would silently build from raw fields and diverge from
        // upstream). Clone the `StructInfo` out of `shared_ctx` first so the
        // immutable borrow ends before the `&mut self` calls below.
        let qualified_function = self
            .current_module_path
            .as_ref()
            .filter(|_| !function.contains('.'))
            .map(|module_path| format!("{}.{}", module_path, function));
        let direct_struct_info = self.shared_ctx.struct_table.get(function).or_else(|| {
            qualified_function
                .as_ref()
                .and_then(|name| self.shared_ctx.struct_table.get(name))
        });
        let struct_info = match direct_struct_info {
            Some(info) if !info.has_inner_constructor && info.fields.len() == args.len() => {
                info.clone()
            }
            _ => {
                let mut short_matches =
                    self.shared_ctx.struct_table.iter().filter(|(name, info)| {
                        name.rsplit('.').next() == Some(function)
                            && !info.has_inner_constructor
                            && info.fields.len() == args.len()
                    });
                let Some((_, first)) = short_matches.next() else {
                    return Ok(None);
                };
                if short_matches.next().is_some() {
                    return Ok(None);
                }
                first.clone()
            }
        };
        // Issue #7793 regression guard: only synthesize the field-count default
        // constructor when the argument types are actually convertible to the
        // (concrete) field types. When they are NOT (e.g. an outer ctor exists
        // but this call matches neither it nor the field types), fall through to
        // normal dispatch so it raises a catchable runtime `MethodError`,
        // matching upstream Julia — instead of `compile_struct_constructor`
        // emitting an uncatchable compile-time `Cannot convert ...` error.
        let has_runtime_unknown_arg = args
            .iter()
            .any(|arg| matches!(self.infer_julia_type(arg), JuliaType::Any));
        if has_runtime_unknown_arg
            || self.struct_field_count_ctor_args_convertible(&struct_info, args)
        {
            return self.compile_struct_constructor(struct_info, args).map(Some);
        }
        Ok(None)
    }

    /// Generic dispatch tail of `compile_call`: user/Base method-table
    /// multiple dispatch with runtime-dispatch candidate emission,
    /// dispatch-error fallbacks, return-type overrides, and the
    /// builtin/no-method-table fallback path.
    pub(super) fn compile_generic_dispatch_call(
        &mut self,
        function: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
        kwargs_splat_mask: &[bool],
        has_kwargs_splat: bool,
    ) -> CResult<ValueType> {
        // Issue #7575: when compiling inside a module that defines its OWN
        // function `function`, an unqualified call resolves to that module's
        // method table — never the shared bare-name pool that also holds a
        // parent module's same-named (possibly more-specific, typed) methods.
        let module_owned_table = self.module_owned_function_table_name(function);
        let base_qualified_function;
        let method_table_name = if let Some(owned) = module_owned_table.as_deref() {
            owned
        } else if self.method_tables.contains_key(function) {
            function
        } else if is_method_dispatch_first_base_function(function) {
            base_qualified_function = format!("Base.{}", function);
            if self.method_tables.contains_key(&base_qualified_function) {
                base_qualified_function.as_str()
            } else {
                function
            }
        } else {
            function
        };
        // Check if this is a user-defined function with potential multiple dispatch
        if let Some(table) = self.method_tables.get(method_table_name) {
            // Check if the function is accessible (top-level or imported via using)
            if !self.imported_functions.contains(function) {
                return err(format!(
                    "function '{}' is not imported. Use 'using ModuleName' or 'using ModuleName: {}' to import it, or use 'ModuleName.{}()' for qualified access.",
                    function, function, function
                ));
            }

            // Infer argument types for dispatch
            let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
            if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && is_reducible_nary_operator(function)
                && args.len() > 2
            {
                // Julia's n-ary `+`/`*` calls reduce to a left fold when no
                // more-specific method is known at compile time. Do this
                // before the broad Any-argument dynamic-call path so untyped
                // keyword/default frames do not emit a 3-arg call site with
                // only the string-concat vararg candidate (Issue #8369).
                return self.compile_nary_operator_reduction(function, args);
            }
            if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && args.len() == 2
                && (matches!(
                    base_function_to_builtin_op(function),
                    Some(BuiltinOp::Iterate)
                ) || matches!(
                    base_function_to_builtin_op(method_table_name),
                    Some(BuiltinOp::Iterate)
                ))
            {
                // The iterator protocol has VM-side fallback logic for
                // primitive collections and Pure Julia iterator structs. Do
                // not let the broad Any/Struct multi-arg dynamic-call path
                // below turn `iterate(collection, state)` into a generic
                // CallDynamic; that path cannot apply the iterator sentinel
                // handling needed by wrappers such as Iterators.Filter
                // (Issue #8370).
                return self.compile_builtin(&BuiltinOp::Iterate, args);
            }
            if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|is_splat| !*is_splat)
                && args.len() > 1
                && arg_types.iter().any(|arg| {
                    matches!(arg, JuliaType::Any)
                        || is_runtime_unknown_struct_arg(arg)
                        || arg.is_abstract_container()
                })
            {
                let has_any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
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
                        self.emit(Instr::CallDynamic(usize::MAX, args.len(), candidates));
                        if let Some(hof_ty) = self.infer_hof_call_site_return_type(function, args) {
                            return Ok(hof_ty);
                        }
                        if is_truncated_result_call(function, args, kwargs) {
                            return Ok(self
                                .shared_ctx
                                .get_struct_type_id("Distributions.Truncated")
                                .or_else(|| self.shared_ctx.get_struct_type_id("Truncated"))
                                .map(ValueType::Struct)
                                .unwrap_or(ValueType::Any));
                        }
                        return Ok(ValueType::Any);
                    }
                }
            }
            if kwargs.is_empty() && !table.methods.iter().any(|m| m.accepts_arity(args.len())) {
                if let Some(vt) =
                    self.try_struct_field_count_default_ctor_fallback(method_table_name, args)?
                {
                    return Ok(vt);
                }
            }

            if matches!(function, "length" | "Base.length")
                && args.len() == 1
                && matches!(
                    arg_types.first(),
                    Some(JuliaType::Tuple | JuliaType::TupleOf(_))
                )
            {
                return self.compile_builtin(&BuiltinOp::Length, args);
            }

            // Check if any argument type is Any - this requires runtime dispatch
            let has_any_arg = arg_types.iter().any(|t| matches!(t, JuliaType::Any));
            let has_multiple_methods = table.methods.len() > 1;

            if has_any_arg
                && args.len() == 1
                && matches!(function, "length" | "size" | "ndims" | "eltype" | "collect")
            {
                if matches!(function, "length" | "size" | "ndims" | "eltype") {
                    let candidates = self.user_unary_runtime_candidates(table);
                    if !candidates.is_empty() {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        let builtin_id = match function {
                            "length" => BuiltinId::Length,
                            "size" => BuiltinId::Size,
                            "ndims" => BuiltinId::Ndims,
                            "eltype" => BuiltinId::Eltype,
                            _ => unreachable!("guarded by matches! above"),
                        };
                        self.emit(Instr::CallDynamicOrBuiltin(builtin_id, candidates));
                        return Ok(match function {
                            "length" | "ndims" => ValueType::I64,
                            "size" => ValueType::Tuple,
                            "eltype" => ValueType::DataType,
                            _ => unreachable!("guarded by matches! above"),
                        });
                    }
                }
                if let Some(builtin_op) = base_function_to_builtin_op(function) {
                    return self.compile_builtin(&builtin_op, args);
                }
                return self.compile_builtin_call(function, args);
            }

            if function == "show" && has_any_arg && has_multiple_methods {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();
                if !candidates.is_empty() {
                    // Issue #4347: `IOBuffer()` is still often Any-typed at
                    // statement boundaries, so `show(buf, ())` must dispatch
                    // on runtime IOBuffer/Tuple{} instead of statically picking
                    // an arbitrary specific show method such as CartesianIndex.
                    let fallback_index = candidates[0];
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        fallback_index,
                        candidates,
                    ));
                    return Ok(ValueType::Any);
                }
            }

            if matches!(function, "promote_type" | "promote_rule")
                && arg_types.iter().all(|t| matches!(t, JuliaType::DataType))
                && table.methods.iter().any(method_has_typeof_param)
            {
                for arg in args {
                    self.compile_expr(arg)?;
                }

                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();

                if !candidates.is_empty() {
                    let fallback_index = table
                        .methods
                        .iter()
                        .find(|m| method_has_typeof_typevar_param(m))
                        .map(|m| m.global_index)
                        .unwrap_or(candidates[0]);
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        fallback_index,
                        candidates,
                    ));
                    return Ok(ValueType::DataType);
                }
            }

            // floor(Int, x) / ceil(Int, x) / round(Int, x) / trunc(Int, x):
            // compile-time rounding with a constant target type.  The second
            // arg may be Any-typed (e.g. inside a loop over StaticArrayInline
            // elements) so static dispatch fails and the has_datatype_arg path
            // below would emit CallTypedDispatch — full resolution every call.
            // Short-circuit to compile_builtin_call so the FloorF64 + integer
            // conversion intrinsics are emitted instead (Issue #7964).
            if matches!(function, "round" | "floor" | "ceil" | "trunc")
                && args.len() == 2
                && matches!(&args[0], Expr::Var(n, _) if is_builtin_type_name(n))
            {
                return self.compile_builtin_call(function, args);
            }

            let has_datatype_arg = arg_types.iter().any(|t| matches!(t, JuliaType::DataType));
            let has_typeof_methods = table.methods.iter().any(method_has_typeof_param);
            if has_datatype_arg && has_typeof_methods {
                for arg in args {
                    self.compile_expr(arg)?;
                }

                let candidates: Vec<usize> = table
                    .methods
                    .iter()
                    .filter(|m| m.accepts_arity(args.len()))
                    .map(|m| m.global_index)
                    .collect();

                if !candidates.is_empty() {
                    let fallback_index = table
                        .methods
                        .iter()
                        .find(|m| method_has_typeof_typevar_param(m))
                        .map(|m| m.global_index)
                        .unwrap_or(candidates[0]);
                    self.emit(Instr::CallTypedDispatch(
                        method_table_name.to_string(),
                        args.len(),
                        fallback_index,
                        candidates,
                    ));
                    let return_type = match function {
                        "promote_type" | "promote_rule" | "typeof" | "eltype" | "keytype"
                        | "valtype" => ValueType::DataType,
                        _ => ValueType::Any,
                    };
                    return Ok(return_type);
                }
            }

            // Find the best matching method
            // If dispatch fails for a known base function, fall back to the builtin implementation
            let method = match table.dispatch(&arg_types) {
                Ok(m) => m,
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if function == "size"
                        && args.len() == 2
                        && arg_types
                            .iter()
                            .any(|ty| matches!(ty, JuliaType::Struct(_))) =>
                {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }

                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();

                    if let Some(fallback_index) = candidates.first().copied() {
                        self.emit(Instr::CallTypedDispatch(
                            method_table_name.to_string(),
                            args.len(),
                            fallback_index,
                            candidates,
                        ));
                        return Ok(ValueType::Any);
                    }

                    if let Some(builtin_op) = base_function_to_builtin_op(function) {
                        return self.compile_builtin(&builtin_op, args);
                    }
                    return self.compile_builtin_call(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if function == "IteratorEltype" && args.len() == 1 && !has_any_arg =>
                {
                    if let Some(
                        JuliaType::UnitRange
                        | JuliaType::StepRange
                        | JuliaType::Array
                        | JuliaType::VectorOf(_)
                        | JuliaType::MatrixOf(_)
                        | JuliaType::Tuple
                        | JuliaType::TupleOf(_)
                        | JuliaType::String,
                    ) = arg_types.first()
                    {
                        return self.compile_call("HasEltype", &[], &[], &[], &[]);
                    }
                    return Err(CompileError::Dispatch(
                        crate::types::DispatchError::NoMethodFound {
                            name: method_table_name.to_string(),
                            arg_types,
                        },
                    ));
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if self
                        .shared_ctx
                        .struct_table
                        .get(function)
                        .is_some_and(|info| info.is_mutable && info.fields.len() == args.len()) =>
                {
                    let struct_info = self
                        .shared_ctx
                        .struct_table
                        .get(function)
                        .cloned()
                        .ok_or_else(|| {
                            CompileError::Msg(format!(
                                "Internal error: struct {} not found",
                                function
                            ))
                        })?;
                    return self.compile_struct_constructor(struct_info, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if matches!(function, "round" | "floor" | "ceil" | "trunc")
                        && args.len() == 2
                        && matches!(&args[0], Expr::Var(n, _) if is_builtin_type_name(n)) =>
                {
                    // `round(T, x)` / `floor(T, x)` / `ceil(T, x)` / `trunc(T, x)`
                    // type-conversion form. The builtin handler recognizes the
                    // `(TypeName, value)` shape; route there even when `x` is
                    // Any-typed (inside a function/loop/comprehension), where static
                    // dispatch otherwise fails with NoMethodFound (Issue #5657). A
                    // `round(x, mode)` / `round(x; digits)` call has a non-type first
                    // argument and is unaffected.
                    return self.compile_builtin_call(function, args);
                }
                Err(_) if is_base_function(function) && function != "convert" && !has_any_arg => {
                    // Fallback to builtin for known base functions (e.g., floor(Float64))
                    // BUT only when argument types are known at compile time.
                    // When has_any_arg is true, we fall through to runtime dispatch instead,
                    // which allows user-defined methods (like Float64(::MyType)) to be called.
                    // Try BuiltinOp first (handles iterate, typeof, etc. with proper types)
                    if let Some(builtin_op) = base_function_to_builtin_op(function) {
                        return self.compile_builtin(&builtin_op, args);
                    }
                    return self.compile_builtin_call(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if is_builtin_type_name(function) && args.len() == 1 && !has_any_arg =>
                {
                    // Fallback to builtin type constructor when user-defined method doesn't match
                    // AND the argument type is known at compile time (not Any).
                    // This handles cases like Float64(42) when user defined Float64(::MyType)
                    // but there's no Float64(::Int64) method.
                    // When has_any_arg is true, we fall through to runtime dispatch instead.
                    return self.compile_builtin_call(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if is_reducible_nary_operator(function) && args.len() > 2 =>
                {
                    // Julia semantics: when no specific n-arg method exists for operators like +/*,
                    // n-arg calls like +(a, b, c) reduce to +(+(a, b), c).
                    // This is Julia's generic: +(a, b, c, xs...) = afoldl(+, a+b, c, xs...)
                    // This works regardless of whether the methods are Base extensions or user-defined,
                    // as long as there's no specific n-arg method that matches the argument types.
                    return self.compile_nary_operator_reduction(function, args);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if function == "convert" || function == "Base.convert" =>
                {
                    if args.len() != 2 {
                        return err("convert requires exactly 2 arguments: convert(T, x)");
                    }
                    self.compile_expr(&args[0])?;
                    self.compile_expr(&args[1])?;
                    self.emit(Instr::CallBuiltin(BuiltinId::Convert, 2));
                    return Ok(ValueType::Any);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if args.len() == 1
                        && kwargs.is_empty()
                        && arg_types
                            .first()
                            .is_some_and(is_rank_unknown_array_julia_type)
                        && table.methods.iter().any(|m| {
                            m.param_count() == 1
                                && m.param_matches_at_call_position(
                                    0,
                                    core_is_abstract_array_family_type,
                                )
                        }) =>
                {
                    // Issue #7266: a single array-family argument whose element
                    // type is unknown at compile time (most notably a
                    // comprehension `[expr for ...]`, imaged as the bare
                    // `JuliaType::Struct("Vector")`) statically matches NO method
                    // — the parametric `::AbstractVector{<:Real}` / `::Vector{T}`
                    // arms need a concrete element type. The correct concrete
                    // `Vector{Float64}` value DOES select the right method at
                    // runtime, so route to runtime dispatch with the no-match
                    // sentinel (mirroring the single-arg `has_any_arg` arm) rather
                    // than throwing a static MethodError or loose-matching an
                    // unrelated abstract-scalar method (the pre-fix bug routed
                    // `Vector` to `::Integer`).
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    let candidates: Vec<DynamicCallCandidate> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 1)
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect();
                    self.emit(Instr::CallDynamic(usize::MAX, 1, candidates));
                    return Ok(ValueType::Any);
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if has_any_arg && args.len() == 1 =>
                {
                    if !kwargs.is_empty() {
                        return self.emit_runtime_dispatched_kwargs_call(
                            method_table_name,
                            args,
                            kwargs,
                            kwargs_splat_mask,
                            false,
                        );
                    }

                    // For functions that have builtin fallbacks (floor, ceil, etc.), use the builtin path
                    // which has CallDynamicOrBuiltin support for runtime dispatch with builtin fallback.
                    // This includes:
                    // - Rounding functions (floor, ceil, round, trunc) with Rational methods
                    // - Math functions (sqrt, abs, sign) with struct methods
                    // - I/O functions (take!) that have both builtin (IOBuffer) and Julia (Channel) methods
                    // All should fall back to builtin for Float64/Int64 or IO types.
                    // Note: sin, cos, tan, exp, log removed — now Pure Julia (base/math.jl)
                    match function {
                        // Note: sin, cos, tan, exp, log removed — now Pure Julia (base/math.jl)
                        "floor" | "ceil" | "round" | "trunc" | "sqrt" | "abs" | "sign"
                        | "take!" | "takestring!" => {
                            return self.compile_builtin_call(function, args);
                        }
                        // Shape/eltype protocol (Issues #3736/#4066): when the argument type is
                        // unknown at compile time and no Pure Julia method matches
                        // the inferred (Any) type, route to BuiltinId::{Length,Size,Ndims}.
                        // The runtime handlers there dispatch to the method table
                        // for Struct/StructRef values and otherwise fall back to
                        // primitive container behavior (Array, Tuple, String,
                        // Range, Dict, Set, Generator).
                        "length" | "size" | "ndims" | "eltype" | "objectid" => {
                            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                                return self.compile_builtin(&builtin_op, args);
                            }
                            return self.compile_builtin_call(function, args);
                        }
                        // Iterator protocol (Issue #3735): same fallback story as
                        // length/size — route to BuiltinOp::Iterate / BuiltinOp::Collect
                        // so the runtime handler can do its own struct-vs-primitive
                        // dispatch (BuiltinId::Iterate / BuiltinId::RangeCollect).
                        "iterate" | "collect" => {
                            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                                return self.compile_builtin(&builtin_op, args);
                            }
                            return self.compile_builtin_call(function, args);
                        }
                        _ => {}
                    }

                    // When argument type is Any (compile-time unknown) and there are methods,
                    // use runtime dispatch. This handles cases like inv(x) where x::Rational{T}.
                    // At compile time we don't know the concrete type, so we dispatch at runtime.
                    for arg in args {
                        self.compile_expr(arg)?;
                    }

                    // Build candidates for runtime dispatch from all single-arg
                    // methods. The expected type name is derived from each
                    // candidate's FunctionInfo at runtime (Issue #6496).
                    let candidates: Vec<DynamicCallCandidate> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 1)
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect();

                    if !candidates.is_empty() {
                        // No compile-time method accepted `Any`, so there is no
                        // valid Julia fallback when runtime candidate scoring
                        // also misses. Use the no-match sentinel and let the VM
                        // raise MethodError instead of calling an arbitrary
                        // specific candidate (Issue #4020).
                        self.emit(Instr::CallDynamic(usize::MAX, 1, candidates));
                        // Return Any since we don't know the concrete return type
                        // (iterate() returns Tuple or Nothing - IndexLoad handles both at runtime)
                        return Ok(ValueType::Any);
                    }

                    // No method candidates - check if this is a struct constructor
                    // If so, fall back to the default struct constructor
                    if let Some(struct_info) = self.shared_ctx.struct_table.get(function) {
                        if struct_info.fields.len() == args.len() {
                            return self.compile_struct_constructor(struct_info.clone(), args);
                        }
                    }

                    // No candidates found - fall through to error
                    return err(format!("No method matching {}({:?})", function, arg_types));
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if has_any_arg && args.len() >= 2 =>
                {
                    if !kwargs.is_empty() {
                        return self.emit_runtime_dispatched_kwargs_call(
                            method_table_name,
                            args,
                            kwargs,
                            kwargs_splat_mask,
                            false,
                        );
                    }

                    // Shape protocol (Issue #4314): `size(x, dim)` with an
                    // Any-typed first argument must still let runtime method
                    // dispatch select Pure Julia struct methods such as
                    // `Base.size(::Diagonal, ::Int)`. Primitive containers are
                    // covered by the runtime candidates from Base's `size`
                    // methods below, so do not short-circuit to BuiltinId::Size
                    // here.
                    // Iterator protocol (Issue #3735): `iterate(coll, state)` with
                    // unknown argument types should route to BuiltinOp::Iterate so
                    // the runtime handler can dispatch via its own struct/primitive
                    // logic.
                    if matches!(function, "iterate") && args.len() == 2 {
                        if let Some(builtin_op) = base_function_to_builtin_op(function) {
                            return self.compile_builtin(&builtin_op, args);
                        }
                    }
                    // When argument types include Any (compile-time unknown) for multi-arg functions,
                    // use runtime dispatch. This handles cases like gcd(a, b) where a, b have type T.
                    for arg in args {
                        self.compile_expr(arg)?;
                    }

                    // Build candidates for runtime dispatch from all matching-arity methods
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();

                    if !candidates.is_empty() {
                        if candidates.len() == 1 {
                            self.emit_call_or_specialize(
                                method_table_name,
                                candidates[0],
                                args.len(),
                            );
                        } else {
                            // Use the first method as fallback
                            let fallback_index = candidates[0];
                            self.emit(Instr::CallTypedDispatch(
                                method_table_name.to_string(),
                                args.len(),
                                fallback_index,
                                candidates,
                            ));
                        }
                        // The runtime dispatch above selects the concrete method,
                        // but for higher-order functions whose callable argument
                        // is an inline lambda (now `Any`-typed since its bare
                        // nested name left the short-name table — Issue #8105) the
                        // result type is still statically inferable from the
                        // call-site expressions. Recover it so `y = reduce(op, xs)`
                        // keeps its precise (e.g. Float64) binding type instead of
                        // widening to `Any`; non-HOF callees stay `Any` as before.
                        if let Some(hof_ty) = self.infer_hof_call_site_return_type(function, args) {
                            return Ok(hof_ty);
                        }
                        // Return Any since we don't know the concrete return type
                        // (iterate() returns Tuple or Nothing - IndexLoad handles both at runtime)
                        return Ok(ValueType::Any);
                    }

                    // Issue #7793: no arity-matching declared constructor, but
                    // the name is a struct whose field count equals the call
                    // arity — fall back to the synthesized field-count default
                    // constructor (mirrors the single-arg arm above).
                    if let Some(vt) =
                        self.try_struct_field_count_default_ctor_fallback(function, args)?
                    {
                        return Ok(vt);
                    }

                    // No candidates found - fall through to error
                    return err(format!("No method matching {}({:?})", function, arg_types));
                }
                Err(crate::types::DispatchError::NoMethodFound { .. })
                    if kwargs.is_empty()
                        && table.methods.iter().any(|m| m.param_count() == args.len()) =>
                {
                    if has_any_arg {
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
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
                    // Issue #7793: a same-arity declared constructor exists but
                    // its types did not match. The struct still has its
                    // synthesized field-count default constructor, so when the
                    // call arity equals the field count fall back to it instead
                    // of throwing (mirrors the single-arg arm). Routes to the
                    // field-count built-in constructor, never a re-dispatch.
                    if let Some(vt) =
                        self.try_struct_field_count_default_ctor_fallback(function, args)?
                    {
                        return Ok(vt);
                    }
                    if method_table_name != function {
                        if let Some(vt) = self
                            .try_struct_field_count_default_ctor_fallback(method_table_name, args)?
                        {
                            return Ok(vt);
                        }
                    }
                    let mut struct_matches =
                        self.shared_ctx.struct_table.iter().filter(|(name, info)| {
                            (name.as_str() == function || name.as_str() == method_table_name)
                                && !info.has_inner_constructor
                                && info.fields.len() == args.len()
                        });
                    if let Some((_, info)) = struct_matches.next() {
                        let struct_info = info.clone();
                        let has_runtime_unknown_arg = args
                            .iter()
                            .any(|arg| matches!(self.infer_julia_type(arg), JuliaType::Any));
                        if has_runtime_unknown_arg
                            || self.struct_field_count_ctor_args_convertible(&struct_info, args)
                        {
                            return self.compile_struct_constructor(struct_info, args);
                        }
                    }

                    // Issue #6007: a fully static method miss is still a runtime
                    // MethodError in Julia. Evaluate arguments for side effects,
                    // then raise a catchable runtime MethodError instead of
                    // aborting compilation with Dispatch(NoMethodFound).
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    for _ in args {
                        self.emit(Instr::Pop);
                    }

                    let arg_sig: Vec<String> =
                        arg_types.iter().map(|t| format!("::{}", t)).collect();
                    self.emit(Instr::ThrowMethodError(format!(
                        "no method matching {}({})",
                        method_table_name,
                        arg_sig.join(", ")
                    )));
                    return Ok(ValueType::Any);
                }
                Err(crate::types::DispatchError::AmbiguousMethod { .. }) => {
                    // Check if this is a Type{T} dispatch scenario:
                    // - At least one argument only inferred as DataType (a type value)
                    // - Methods have TypeOf patterns in their parameters
                    //
                    // Julia only needs runtime type-object dispatch for the Type{...}
                    // positions, not for every argument. This lets mixed calls with a
                    // runtime DataType value and concrete non-type arguments reach the
                    // runtime Type{...} fallback instead of being rejected here.
                    let has_datatype_arg =
                        arg_types.iter().any(|t| matches!(t, JuliaType::DataType));
                    let has_typeof_methods = table.methods.iter().any(method_has_typeof_param);

                    if has_datatype_arg && has_typeof_methods {
                        // Compile arguments - they are type values (DataType)
                        for arg in args {
                            self.compile_expr(arg)?;
                        }

                        // Build candidates for runtime typed dispatch
                        // (candidate function indices; expected type names are
                        // derived at runtime, Issue #6496)
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| m.accepts_arity(args.len()))
                            .map(|m| m.global_index)
                            .collect();

                        // Find the fallback method (one with TypeVar patterns - generic version)
                        let fallback_index = table
                            .methods
                            .iter()
                            .find(|m| method_has_typeof_typevar_param(m))
                            .map(|m| m.global_index)
                            .unwrap_or(candidates.first().copied().unwrap_or(0));

                        if candidates.len() == 1 && candidates[0] == fallback_index {
                            self.emit_call_or_specialize(
                                method_table_name,
                                candidates[0],
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

                        // Return type is typically Any, but override for type-returning functions
                        let return_type = match function {
                            "promote_type" | "promote_rule" | "typeof" | "eltype" | "keytype"
                            | "valtype" => ValueType::DataType,
                            _ => ValueType::Any,
                        };
                        return Ok(return_type);
                    }

                    // Check if any argument is Any OR a concrete Struct - use
                    // runtime dispatch in that case.
                    //
                    // Issue #4827: a `Struct(T)` argument can statically tie
                    // several method-table arms (e.g. `show(::IO, ::Struct(X))`
                    // for many built-in `X`, plus the generic `show(::IO, ::Any)`)
                    // when the dispatcher's tie-breakers can't pick a unique best
                    // — even though only the runtime concrete struct type selects
                    // the correct arm. Before #4827 this surfaced rarely because a
                    // local `IOBuffer()` inferred as `Any` (so `has_any_arg` was
                    // already true and we ran the runtime-dispatch path). Now that
                    // an `IOBuffer()` slot is statically `IO`, neither arg is `Any`,
                    // so an ambiguous `show(buf::IO, x::Struct)` without a specific
                    // user method would error at compile time. Defer to runtime
                    // dispatch (matching upstream Julia semantics, and what
                    // `CallTypedDispatch` does): it scores the candidates against
                    // the runtime concrete type and raises a proper MethodError if
                    // none applies.
                    let has_struct_arg =
                        arg_types.iter().any(|t| matches!(t, JuliaType::Struct(_)));
                    if has_any_arg || has_struct_arg {
                        if !kwargs.is_empty() {
                            return self.emit_runtime_dispatched_kwargs_call(
                                method_table_name,
                                args,
                                kwargs,
                                kwargs_splat_mask,
                                false,
                            );
                        }

                        // Compile arguments
                        for arg in args {
                            self.compile_expr(arg)?;
                        }

                        // Build candidates for runtime dispatch
                        // (candidate function indices; expected type names are
                        // derived at runtime, Issue #6496)
                        let candidates: Vec<usize> = table
                            .methods
                            .iter()
                            .filter(|m| m.accepts_arity(args.len()))
                            .map(|m| m.global_index)
                            .collect();

                        if !candidates.is_empty() {
                            if candidates.len() == 1 {
                                self.emit_call_or_specialize(
                                    method_table_name,
                                    candidates[0],
                                    args.len(),
                                );
                            } else {
                                // Use the first candidate as fallback
                                let fallback_index = candidates[0];
                                self.emit(Instr::CallTypedDispatch(
                                    method_table_name.to_string(),
                                    args.len(),
                                    fallback_index,
                                    candidates,
                                ));
                            }
                            // Return Any since we don't know the concrete return type
                            // (iterate() returns Tuple or Nothing - IndexLoad handles both at runtime)
                            return Ok(ValueType::Any);
                        }
                    }

                    // Genuinely ambiguous call with no most-specific resolution
                    // and no runtime-dispatch fallback. Upstream Julia raises a
                    // *catchable* runtime `MethodError` (ambiguity) here rather
                    // than aborting; mirror that instead of returning a hard
                    // `CompileError::Dispatch(AmbiguousMethod{..})` that exits the
                    // process (Issue #5071).
                    // Candidate rows are sourced core-projection-first (the
                    // canonical-inverse reconstruction renders identically for
                    // every round-tripping spelling; Issue #6495, stage
                    // 6b-iii). `params.len()` is an arity read.
                    let candidate_sigs: Vec<Vec<JuliaType>> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == args.len())
                        .map(|m| m.projected_param_julia_types())
                        .collect();

                    // Build the runtime message. `VmError::MethodError` already
                    // prepends "MethodError: ", so omit that prefix here. The
                    // shape resembles upstream's
                    // "f(::Int64, ::Int64) is ambiguous. Candidates: ...".
                    let arg_sig: Vec<String> =
                        arg_types.iter().map(|t| format!("::{}", t)).collect();
                    let mut message = format!(
                        "{}({}) is ambiguous. Candidates:",
                        method_table_name,
                        arg_sig.join(", ")
                    );
                    for sig in &candidate_sigs {
                        let sig_str: Vec<String> = sig.iter().map(|t| format!("::{}", t)).collect();
                        message.push_str(&format!(
                            "\n  {}({})",
                            method_table_name,
                            sig_str.join(", ")
                        ));
                    }

                    // Evaluate the arguments for side-effect fidelity (upstream
                    // evaluates call arguments before dispatch fails), then drop
                    // them and throw the runtime MethodError.
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    for _ in args {
                        self.emit(Instr::Pop);
                    }
                    self.emit(Instr::ThrowMethodError(message));

                    // The throw is unconditional; the value type after it is
                    // unreachable, so report Any.
                    return Ok(ValueType::Any);
                }
                Err(e) => return Err(CompileError::Dispatch(e)),
            };
            // Compile positional arguments with expected types.
            //
            // IMPORTANT: When runtime dispatch will decide the final method, do not
            // coerce arguments based on the statically selected fallback method. For
            // `ones(T, 2)` with T as an unknown runtime type object, static dispatch
            // may pick a dims fallback, but Julia dispatch must still see the first
            // argument as a DataType value and select `ones(::Type{T}, ...)`.
            // Issue #8158: the qualified `Module.f(x)` path
            // (`compile_module_call_via_method_table`) mirrors this exact policy
            // through the shared `should_runtime_dispatch` helper so a qualified
            // call defers to runtime dispatch in the same cases as this
            // unqualified call. The single-vs-multi split is kept here because
            // the two emit different instructions below (single → `CallDynamic`,
            // multi → `CallTypedDispatch`).
            let use_single_arg_runtime_dispatch =
                has_any_arg && has_multiple_methods && args.len() == 1;
            let use_multi_arg_runtime_dispatch = has_multiple_methods && args.len() > 1 && {
                // The per-slot `Any` probes read the canonical
                // `core_signature` projection first (Issue #6495, stage
                // 6b-iii); `params.len()` comparisons are arity reads.
                // Out-of-range slots are non-matches, preserving the
                // legacy `zip` truncation / `params.get` `Option` gates.
                let (case1, case2, case3) = if has_any_arg {
                    let case1 = arg_types.iter().enumerate().any(|(idx, arg_ty)| {
                        matches!(arg_ty, JuliaType::Any)
                            && method_param_is_not_any_at_call_position(method, idx)
                    });
                    let case2 = !case1 && {
                        let matched_all_any = method.all_params_match(core_is_any_param);
                        matched_all_any
                            && table.methods.iter().any(|m| {
                                m.accepts_arity(args.len())
                                    && m.global_index != method.global_index
                                    && m.any_param_matches(|core| !core_is_any_param(core))
                            })
                    };
                    let case3 = !case1
                        && !case2
                        && table.methods.iter().any(|m| {
                            m.accepts_arity(args.len())
                                && m.global_index != method.global_index
                                && (0..args.len()).any(|idx| {
                                    matches!(arg_types.get(idx), Some(JuliaType::Any))
                                        && method_param_is_any_at_call_position(method, idx)
                                        && method_param_is_not_any_at_call_position(m, idx)
                                })
                        });
                    (case1, case2, case3)
                } else {
                    (false, false, false)
                };
                // Abstract-array-family probe sourced from the canonical
                // `core_signature` projection (Issue #6495, stages
                // 7a/7c-ii).
                let case4 = table.methods.iter().any(|m| {
                    m.global_index != method.global_index
                        && m.accepts_arity(args.len())
                        && (0..args.len()).any(|idx| {
                            arg_types
                                .get(idx)
                                .is_some_and(is_rank_unknown_array_julia_type)
                                && m.param_matches_at_call_position(
                                    idx,
                                    core_is_abstract_array_family_type,
                                )
                        })
                });
                let case5 =
                    runtime_unknown_struct_arg_requires_dispatch(method, &arg_types, args.len())
                        || method_has_anonymous_bounded_parametric_struct_for_struct_arg(
                            method,
                            &arg_types,
                            args.len(),
                        );
                case1 || case2 || case3 || case4 || case5
            };
            let use_runtime_dispatch =
                use_single_arg_runtime_dispatch || use_multi_arg_runtime_dispatch;
            // Cross-check: the extracted shared policy must agree with this inline
            // computation (the qualified path relies on it — Issue #8158).
            debug_assert_eq!(
                use_runtime_dispatch,
                should_runtime_dispatch(table, method, &arg_types, args.len(), has_any_arg),
                "should_runtime_dispatch drifted from the inline bare-call policy"
            );

            // Handle varargs functions differently - compile all args
            if let Some(vararg_idx) = method.vararg_param_index {
                // Compile fixed params with their expected types
                for (idx, arg) in args.iter().enumerate() {
                    if idx < vararg_idx {
                        // Fixed parameter - use expected type ONLY if not using runtime dispatch
                        if use_runtime_dispatch {
                            // Runtime dispatch: don't coerce, preserve original type
                            self.compile_expr(arg)?;
                        } else if idx < method.param_count() {
                            // Coercion gate sourced core-projection-first via
                            // the canonical inverse (Issue #6495, stage
                            // 6b-iii); `params.len()` is an arity read.
                            let param_ty = method.projected_param_julia_type(idx);
                            if *param_ty == JuliaType::Any
                                || param_ty.is_narrow_integer()
                                || param_ty.is_abstract_integer()
                                || param_ty.is_abstract_container()
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
                        // Varargs - compile as-is
                        self.compile_expr(arg)?;
                    }
                }
            } else {
                // Non-varargs: compile args paired with params (the
                // `take(params.len())` mirrors the historical `zip`
                // truncation; the coercion gate reads the canonical
                // `core_signature` projection first — Issue #6495, stage
                // 6b-iii).
                for (idx, arg) in args.iter().enumerate().take(method.param_count()) {
                    // When using runtime dispatch, don't coerce - preserve original type
                    if use_runtime_dispatch {
                        self.compile_expr(arg)?;
                        continue;
                    }
                    let param_ty = method.projected_param_julia_type(idx);
                    if *param_ty == JuliaType::Any {
                        // For `Any` typed parameters, don't coerce - just compile the argument as-is
                        self.compile_expr(arg)?;
                    } else if param_ty.is_narrow_integer() || param_ty.is_abstract_integer() {
                        // For narrow integer types (Int8, Int16, Int32, UInt*, Bool, Int128)
                        // and abstract integer supertypes (Integer, Signed, Unsigned, Real, Number),
                        // don't coerce to I64 - preserve the specific type so the function
                        // body receives the correct Value variant (e.g., Value::I32 not Value::I64).
                        self.compile_expr(arg)?;
                    } else if param_ty.is_abstract_container() || is_dict_annotation(&param_ty) {
                        // Abstract container params (`AbstractArray` / `AbstractRange`) and
                        // public Dict params during the struct-backed migration:
                        // a concrete subtype value may be a struct (`OneTo`, `SubArray`),
                        // or a struct-backed `Dict{K,V}`, not the native `ValueType`
                        // `julia_type_to_value_type` maps to, so compile as-is rather
                        // than coercing. Issue #5842 / #6619.
                        self.compile_expr(arg)?;
                    } else {
                        let vt = julia_type_to_value_type(&param_ty);
                        self.compile_expr_as(arg, vt)?;
                    }
                }
            }

            if kwargs.is_empty() {
                // Check if runtime dispatch is needed
                if use_single_arg_runtime_dispatch {
                    // Build candidates for runtime dispatch (single-arg). The
                    // expected type name is derived from each candidate's
                    // FunctionInfo at runtime (Issue #6496).
                    let candidates: Vec<DynamicCallCandidate> = table
                        .methods
                        .iter()
                        .filter(|m| m.param_count() == 1 && method_param_is_not_any_at(m, 0))
                        .map(|m| DynamicCallCandidate::Method(m.global_index))
                        .collect();

                    if !candidates.is_empty() {
                        let candidates_are_base_only = candidates.iter().all(|c| {
                            matches!(c, DynamicCallCandidate::Method(idx)
                                if table.is_base_program_global_index(*idx))
                        });
                        let fallback_index = if candidates_are_base_only
                            || method.all_params_match(core_is_any_param)
                        {
                            method.global_index
                        } else {
                            usize::MAX
                        };
                        // Use CallDynamic for runtime dispatch
                        self.emit(Instr::CallDynamic(fallback_index, args.len(), candidates));
                    } else {
                        // No specific candidates, use static dispatch (with Lazy AoT check)
                        self.emit_call_or_specialize(
                            method_table_name,
                            method.global_index,
                            args.len(),
                        );
                    }
                } else if use_multi_arg_runtime_dispatch {
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();

                    if !candidates.is_empty() {
                        if has_any_arg
                            || should_use_dynamic_call_for_runtime_dispatch(
                                method,
                                &arg_types,
                                args.len(),
                            )
                        {
                            self.emit(Instr::CallDynamic(
                                method.global_index,
                                args.len(),
                                candidates
                                    .into_iter()
                                    .map(DynamicCallCandidate::Method)
                                    .collect(),
                            ));
                        } else {
                            self.emit(Instr::CallTypedDispatch(
                                method_table_name.to_string(),
                                args.len(),
                                method.global_index,
                                candidates,
                            ));
                        }
                    } else {
                        // No specific candidates, use static dispatch
                        self.emit_call_or_specialize(
                            method_table_name,
                            method.global_index,
                            args.len(),
                        );
                    }
                } else {
                    // No kwargs - use Call instruction (with Lazy AoT check)
                    self.emit_call_or_specialize(
                        method_table_name,
                        method.global_index,
                        args.len(),
                    );
                }
            } else {
                if use_runtime_dispatch {
                    return self.emit_runtime_dispatched_kwargs_call(
                        method_table_name,
                        args,
                        kwargs,
                        kwargs_splat_mask,
                        true,
                    );
                }

                // Compile kwarg values (they go on stack after positional args)
                let kwarg_names: Vec<String> =
                    kwargs.iter().map(|(name, _)| name.clone()).collect();
                for (_, value) in kwargs {
                    // Infer type and compile value
                    let ty = self.compile_expr(value)?;
                    // For now, leave as is - VM will coerce if needed
                    let _ = ty;
                }
                // Emit CallWithKwargs or CallWithKwargsSplat instruction
                if has_kwargs_splat {
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
            let hof_function_name = method_table_name
                .strip_prefix("Base.")
                .unwrap_or(method_table_name);
            let has_hof_callsite_return_inference = hof_function_name == "map" && args.len() >= 3;
            let has_known_callsite_return_override =
                is_truncated_result_call(function, args, kwargs);
            if has_any_arg
                && has_multiple_methods
                && kwargs.is_empty()
                && !has_hof_callsite_return_inference
                && !has_known_callsite_return_override
            {
                return Ok(ValueType::Any);
            }
            if self.function_index_is_generated(method.global_index) {
                return Ok(ValueType::Any);
            }
            // Override return type for functions known to return DataType or specific struct types
            let mut return_type = match function {
                "zeros" | "ones" => self.infer_zeros_ones_array_type(args),
                "typeof" | "promote_type" | "promote_rule" | "eltype" | "keytype" | "valtype" => {
                    ValueType::DataType
                }
                "copy" | "Base.copy"
                    if args.len() == 1
                        && matches!(self.infer_expr_type(&args[0]), ValueType::Dict) =>
                {
                    ValueType::Dict
                }
                // `copy(s::Set{T})` returns a fresh `Set{T}` struct (Issue #6721),
                // so the result keeps the Set struct ValueType. Without this, the
                // parametric `copy(::Set{T})` method's primitive return metadata
                // widens to Any, and a following `x in c` would dispatch through
                // the `Any` path and loosely match `in(_, ::KeySet)`.
                "copy" | "Base.copy"
                    if args.len() == 1
                        && matches!(&self.infer_expr_type(&args[0]), ValueType::Struct(type_id)
                            if self.shared_ctx.get_struct_name(*type_id)
                                .is_some_and(|name| name == "Set" || name.starts_with("Set{"))) =>
                {
                    self.infer_expr_type(&args[0])
                }
                "truncated" | "Distributions.truncated"
                    if args.len() >= 2
                        || kwargs.iter().any(|(_, value)| {
                            !matches!(value, Expr::Literal(Literal::Nothing, _))
                        }) =>
                {
                    self.shared_ctx
                        .get_struct_type_id("Distributions.Truncated")
                        .or_else(|| self.shared_ctx.get_struct_type_id("Truncated"))
                        .map(ValueType::Struct)
                        .unwrap_or_else(|| method.return_type.clone())
                }
                // abs, abs2, sign: preserve argument type for BigInt and other numeric types
                // Issue #2383: abs(BigInt) should return BigInt, not Any
                "abs" | "abs2" | "sign" if args.len() == 1 => {
                    let arg_type = self.infer_expr_type(&args[0]);
                    match arg_type {
                        ValueType::Bool
                        | ValueType::I8
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
                        | ValueType::BigInt
                        | ValueType::BigFloat => arg_type,
                        // Complex abs returns F64 (magnitude), use method.return_type
                        _ => method.return_type.clone(),
                    }
                }
                // Issue #4341: Complex overloads of these Pure Julia math
                // functions return Complex. Some precomputed method return
                // metadata is still too primitive and can say F64, which makes
                // top-level `r = tan(z::Complex)` emit StoreF64.
                "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh"
                | "tanh" | "asinh" | "acosh" | "atanh" | "exp" | "log"
                    if args.len() == 1 =>
                {
                    let arg_julia_type = self.infer_julia_type(&args[0]);
                    if matches!(arg_julia_type, JuliaType::Struct(ref name) if name.starts_with("Complex"))
                    {
                        let arg_value_type = self.infer_expr_type(&args[0]);
                        if matches!(arg_value_type, ValueType::Struct(_)) {
                            arg_value_type
                        } else {
                            ValueType::Any
                        }
                    } else {
                        method.return_type.clone()
                    }
                }
                // gcd, lcm: preserve BigInt type when arguments include BigInt
                // Issue #2383: gcd(BigInt, BigInt) should return BigInt, not Any
                "gcd" | "lcm" if args.len() == 2 => {
                    let has_bigint = args
                        .iter()
                        .any(|arg| matches!(self.infer_expr_type(arg), ValueType::BigInt));
                    if has_bigint {
                        ValueType::BigInt
                    } else {
                        // For I64 arguments, use method's return type
                        method.return_type.clone()
                    }
                }
                // typemin/typemax: return type matches the type argument
                // e.g., typemin(Float64) → F64, typemin(Int64) → I64
                "typemin" | "typemax" if args.len() == 1 => {
                    // Infer the type argument to determine the return type
                    let julia_ty = self.infer_julia_type(&args[0]);
                    match julia_ty {
                        JuliaType::TypeOf(inner) => match *inner {
                            JuliaType::Float64 => ValueType::F64,
                            JuliaType::Float32 => ValueType::F32,
                            JuliaType::Float16 => ValueType::F16,
                            JuliaType::Int64 => ValueType::I64,
                            JuliaType::Int32 => ValueType::I32,
                            JuliaType::Int16 => ValueType::I16,
                            JuliaType::Int8 => ValueType::I8,
                            JuliaType::Int128 => ValueType::I128,
                            JuliaType::UInt64 => ValueType::U64,
                            JuliaType::UInt32 => ValueType::U32,
                            JuliaType::UInt16 => ValueType::U16,
                            JuliaType::UInt8 => ValueType::U8,
                            JuliaType::UInt128 => ValueType::U128,
                            JuliaType::Bool => ValueType::Bool,
                            _ => method.return_type.clone(),
                        },
                        _ => method.return_type.clone(),
                    }
                }
                "view" | "Base.view" => self
                    .infer_view_call_return_type(function, args, &arg_types)
                    .unwrap_or_else(|| method.return_type.clone()),
                // HOF (Higher-Order Functions) - call-site specialization for better type inference
                "map" | "Base.map" if args.len() == 2 => {
                    // map(f, arr) - infer return type based on f's return type
                    if let Some(ty) = self.infer_map_call_return_type(&args[0], &args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "map" | "Base.map" if args.len() == 3 => {
                    // binary map(f, left, right) - infer from element-wise callable.
                    if let Some(ty) =
                        self.infer_binary_map_call_return_type(&args[0], &args[1], &args[2])
                    {
                        ty
                    } else if let Some(ty) = self
                        .infer_binary_map_call_return_type_from_julia_types(
                            &args[0],
                            &arg_types[1],
                            &arg_types[2],
                        )
                    {
                        ty
                    } else if method.param_count() > 2 {
                        // Element-type fallback sourced core-projection-first
                        // via the canonical inverse (Issue #6495, stage
                        // 6b-iii); `params.len()` is an arity read.
                        let left_param_ty = method.projected_param_julia_type(1).into_owned();
                        let right_param_ty = method.projected_param_julia_type(2).into_owned();
                        self.infer_binary_map_call_return_type_from_julia_types(
                            &args[0],
                            &left_param_ty,
                            &right_param_ty,
                        )
                        .unwrap_or_else(|| method.return_type.clone())
                    } else {
                        method.return_type.clone()
                    }
                }
                "map" | "Base.map" if args.len() >= 4 => {
                    // n-ary map(f, left, right, rest...) - infer from element-wise callable.
                    if let Some(ty) = self.infer_nary_map_call_return_type(&args[0], &args[1..]) {
                        ty
                    } else if let Some(ty) = self
                        .infer_nary_map_call_return_type_from_julia_types(&args[0], &arg_types[1..])
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "broadcast" | "Base.broadcast" if args.len() == 2 => {
                    // unary broadcast(f, arr) - infer return type like map(f, arr)
                    if let Some(ty) = self.infer_map_call_return_type(&args[0], &args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "broadcast" | "Base.broadcast" if args.len() == 3 => {
                    // binary broadcast(f, left, right) - infer from element-wise callable.
                    if let Some(ty) =
                        self.infer_binary_map_call_return_type(&args[0], &args[1], &args[2])
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "broadcast" | "Base.broadcast" if args.len() >= 4 => {
                    // n-ary broadcast(f, left, right, rest...) - infer from element-wise callable.
                    if let Some(ty) = self.infer_nary_map_call_return_type(&args[0], &args[1..]) {
                        ty
                    } else if let Some(ty) = self
                        .infer_nary_map_call_return_type_from_julia_types(&args[0], &arg_types[1..])
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "filter" | "Base.filter" if args.len() == 2 => {
                    // filter(pred, arr) - return type has same element type as input
                    if let Some(ty) = self.infer_filter_call_return_type(&args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "mapreduce" | "mapfoldl" | "mapfoldr" | "Base.mapreduce" | "Base.mapfoldl"
                | "Base.mapfoldr"
                    if args.len() >= 3 =>
                {
                    // mapreduce(f, op, itr) / mapfoldl / mapfoldr - infer from
                    // mapped element type and reducer when both are visible.
                    if let Some(ty) =
                        self.infer_mapreduce_call_return_type(&args[0], &args[1], &args[2])
                    {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "reduce" | "foldl" | "Base.reduce" | "Base.foldl" if args.len() >= 2 => {
                    // reduce(op, itr) or reduce(op, itr, init)
                    // Return type is the element type of the iterator
                    if let Some(ty) = self.infer_reduce_call_return_type(&args[0], &args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                "foldr" | "Base.foldr" if args.len() >= 2 => {
                    // foldr(op, itr) - same as reduce for return type inference
                    if let Some(ty) = self.infer_reduce_call_return_type(&args[0], &args[1]) {
                        ty
                    } else {
                        method.return_type.clone()
                    }
                }
                // Iterator wrapper functions return specific struct types
                "enumerate" => {
                    // enumerate(iter) returns Enumerate{typeof(iter)}
                    // Instantiate Enumerate{Any} since we don't track the concrete type
                    self.shared_ctx
                        .resolve_instantiation("Enumerate", &[JuliaType::Any])
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "zip" => {
                    // zip returns Zip/Zip3/... depending on arity (Issues #1990/#4281)
                    let any_types: Vec<JuliaType> =
                        (0..args.len()).map(|_| JuliaType::Any).collect();
                    let struct_name = match args.len() {
                        3 => "Zip3",
                        4 => "Zip4",
                        5 => "Zip5",
                        6 => "Zip6",
                        7 => "Zip7",
                        _ => "Zip", // 2 args (default)
                    };
                    self.shared_ctx
                        .resolve_instantiation(struct_name, &any_types)
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "take" => {
                    // take(iter, n) returns Take{typeof(iter)}
                    // Instantiate Take{Any} since we don't track concrete inner type
                    self.shared_ctx
                        .resolve_instantiation("Take", &[JuliaType::Any])
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "drop" => {
                    // drop(iter, n) returns Drop{typeof(iter)}
                    // Instantiate Drop{Any} since we don't track concrete inner type
                    self.shared_ctx
                        .resolve_instantiation("Drop", &[JuliaType::Any])
                        .map(ValueType::Struct)
                        .unwrap_or(method.return_type.clone())
                }
                "rest" => {
                    // rest(iter, state) returns Rest{typeof(iter), typeof(state)}.
                    // rest(iter) is the identity; preserve the method return type there.
                    if args.len() == 2 {
                        self.shared_ctx
                            .resolve_instantiation("Rest", &[JuliaType::Any, JuliaType::Any])
                            .map(ValueType::Struct)
                            .unwrap_or(method.return_type.clone())
                    } else {
                        method.return_type.clone()
                    }
                }
                "iterate" => {
                    // iterate(collection) and iterate(collection, state) return (element, state) or nothing
                    // For compilation purposes, treat as Tuple to enable proper tuple indexing (y[2])
                    // This is safe because code should check `y === nothing` before accessing y[2]
                    ValueType::Tuple
                }
                _ => method.return_type.clone(),
            };

            if matches!(return_type, ValueType::Any) {
                let arg_value_types: Vec<ValueType> =
                    args.iter().map(|arg| self.infer_expr_type(arg)).collect();
                if !arg_value_types
                    .iter()
                    .any(|ty| matches!(ty, ValueType::Any))
                {
                    if let Some(func_ir) = self
                        .shared_ctx
                        .function_ir_by_global_index
                        .get(&method.global_index)
                    {
                        let inferred = self.infer_shared_function_return_type_with_arg_types(
                            func_ir,
                            &arg_value_types,
                        );
                        if self.should_accept_body_reinferred_call_return_type(&inferred) {
                            return_type = inferred;
                        }
                    }
                }
            }

            // Fix for parametric struct constructors: If return type is Any but this is actually
            // a struct constructor call (function name matches a struct in struct_table),
            // override the return type to the correct struct type.
            // This handles cases where the constructor was compiled and cached before the struct
            // was instantiated, causing it to have return type Any.
            if matches!(return_type, ValueType::Any) {
                // Extract base name (e.g., "Rational" from "Rational{T}")
                let base_name = if let Some(brace_pos) = function.find('{') {
                    &function[..brace_pos]
                } else {
                    function
                };

                // First, check if this matches any already-instantiated struct in struct_table
                let mut found_struct = false;
                for (name, struct_info) in self.shared_ctx.struct_table.iter() {
                    let struct_base = if let Some(pos) = name.find('{') {
                        &name[..pos]
                    } else {
                        name.as_str()
                    };

                    if struct_base == base_name {
                        // Found a matching struct - override return type
                        return_type = ValueType::Struct(struct_info.type_id);
                        found_struct = true;
                        break;
                    }
                }

                // If not found in struct_table, check if it's a parametric struct
                // that needs to be instantiated on demand
                if !found_struct && self.shared_ctx.parametric_structs.contains_key(base_name) {
                    // This is a parametric struct constructor that hasn't been instantiated yet
                    // Instantiate it with Any as the type parameter
                    match self
                        .shared_ctx
                        .resolve_instantiation(base_name, &[JuliaType::Any])
                    {
                        Ok(type_id) => {
                            return_type = ValueType::Struct(type_id);
                        }
                        Err(_) => {
                            // Failed to instantiate - keep Any
                        }
                    }
                }
            }

            Ok(return_type)
        } else {
            if self.usings.contains("Random") && is_random_function(function) {
                return self.compile_builtin(&BuiltinOp::Seed, args);
            }

            // Note: mean is now Pure Julia (stdlib/Statistics/src/Statistics.jl)
            // It's dispatched through the method table like other user-defined functions.

            // Handle n-arg reducible operators (+ and *) when there's no method table
            // This happens when flattening produces +(a, b, c, ...) with no user-defined +
            if is_reducible_nary_operator(function) && args.len() > 2 {
                // Reduce to chained binary ops using builtin operators
                return self.compile_nary_builtin_reduction(function, args);
            }

            // Try to map to BuiltinOp first (handles types properly)
            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                return self.compile_builtin(&builtin_op, args);
            }
            if is_builtin_type_name(function) && args.len() != 1 {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::PushDataType(function.to_string()));
                self.emit(Instr::CallFunctionVariable(args.len()));
                return Ok(ValueType::Any);
            }
            // Fall back to string-based builtin call for functions not in BuiltinOp
            match self.compile_builtin_call(function, args) {
                Ok(ty) => Ok(ty),
                Err(CompileError::Msg(msg)) if msg.starts_with("Unknown function: ") => {
                    self.emit(Instr::PushStr(msg));
                    self.emit(Instr::ThrowError);
                    Ok(ValueType::Any)
                }
                Err(err) => Err(err),
            }
        }
    }
}

/// CoreType-native port of the `Type{...}` parameter probe
/// (`matches!(ty, JuliaType::TypeOf(_))`) over the canonical `core_signature`
/// projection (Issue #6495, stage 6b-iii).
///
/// Image analysis: `JuliaType::TypeOf(_)` images exactly as
/// `CoreType::TypeOf(_)`. The only other spelling producing that image is a
/// `JuliaType::Struct("Type{..}")` name string (via `from_julia_name`'s
/// `"Type"` parametric arm), which lowering never emits for a `::Type{T}`
/// annotation and which the canonical inverse never reconstructs — pinned
/// over the Base corpus by `compile::cache::tests::`
/// `base_method_core_call_dispatch_heuristics_parity_issue_6495`.
pub(crate) fn core_is_typeof_param(core: &CoreType) -> bool {
    matches!(core, CoreType::TypeOf(_))
}

/// CoreType-native port of the generic `Type{T}`-with-`TypeVar` fallback
/// probe (`matches!(ty, JuliaType::TypeOf(inner) if inner is TypeVar)`) over
/// the canonical `core_signature` projection (Issue #6495, stage 6b-iii).
///
/// Known image-collision caveat (same as the stage-4 singleton scorers): a
/// parameter spelled `JuliaType::TypeOf(JuliaType::Struct("Q"))` — a
/// single-letter non-var name inside `Type{...}` — images as
/// `TypeOf(TypeVar)` and would satisfy the core probe where the legacy probe
/// did not. That spelling is unreachable from lowering and from the
/// canonical inverse; the parity gate + full suite referee (zero hits).
pub(crate) fn core_is_typeof_typevar_param(core: &CoreType) -> bool {
    matches!(core, CoreType::TypeOf(inner) if matches!(inner.as_ref(), CoreType::TypeVar(_)))
}

/// CoreType-native `Any`-parameter probe (Issue #6495, stage 6b-iii).
///
/// Accepted-divergence note (same as the stage-6a Any-count tie-breaker): a
/// parameter spelled `JuliaType::Struct("Any")` images as `CoreType::Any` —
/// unreachable from lowering (`from_name` resolves `Any`) and from the
/// canonical inverse; parity gate + suite referee.
pub(crate) fn core_is_any_param(core: &CoreType) -> bool {
    matches!(core, CoreType::Any)
}

/// Whether any declared parameter is a `Type{...}` pattern — the
/// `CallTypedDispatch` eligibility probe of the type-object dispatch
/// heuristics, read from the `core_signature` projection (Issue #6495,
/// stage 6b-iii).
fn method_has_typeof_param(m: &crate::compile::method_table::MethodSig) -> bool {
    m.any_param_matches(core_is_typeof_param)
}

/// Whether any declared parameter is the generic `Type{T}` (TypeVar) pattern
/// — the runtime-fallback method finder of the type-object dispatch
/// heuristics, read from the `core_signature` projection (Issue #6495,
/// stage 6b-iii).
fn method_has_typeof_typevar_param(m: &crate::compile::method_table::MethodSig) -> bool {
    m.any_param_matches(core_is_typeof_typevar_param)
}

/// Whether declared parameter `idx` exists and is `Any`, read from the
/// `core_signature` projection (Issue #6495, stage 6b-iii).
#[cfg(test)]
fn method_param_is_any_at(m: &crate::compile::method_table::MethodSig, idx: usize) -> bool {
    m.param_matches_at(idx, core_is_any_param)
}

fn method_param_is_any_at_call_position(
    m: &crate::compile::method_table::MethodSig,
    idx: usize,
) -> bool {
    m.param_matches_at_call_position(idx, core_is_any_param)
}

/// Whether declared parameter `idx` exists and is NOT `Any`, read from the
/// `core_signature` projection; `false` for out-of-range `idx` (preserving
/// the `zip`/`params.get` truncation of the legacy readers — Issue #6495,
/// stage 6b-iii).
fn method_param_is_not_any_at(m: &crate::compile::method_table::MethodSig, idx: usize) -> bool {
    m.param_matches_at(idx, |core| !core_is_any_param(core))
}

fn method_param_is_not_any_at_call_position(
    m: &crate::compile::method_table::MethodSig,
    idx: usize,
) -> bool {
    m.param_matches_at_call_position(idx, |core| !core_is_any_param(core))
}

/// Shared dispatch policy: does a call whose static dispatch selected `method`
/// need to defer to runtime multiple dispatch rather than statically bind it?
///
/// Used by BOTH the unqualified bare-call path (`compile_generic_dispatch_call`)
/// and the qualified `Module.f(x)` path (`compile_module_call_via_method_table`)
/// so a qualified call dispatches identically to the same unqualified call
/// (Issue #8158). A wide `Any` argument statically selects the catch-all
/// `f(::Any)`, but the runtime value may match a more-specific method; the
/// unqualified path already runtime-dispatched here, the qualified path did not —
/// so `SciMLBase._callbacks(cb::CallbackSet)` mis-dispatched to the `(cb,)`
/// catch-all and silently disabled every callback in a `CallbackSet`.
///
/// - single `Any` arg with multiple methods: the statically-selected method may
///   be the catch-all while the runtime value matches a more-specific method.
/// - multi-arg `Any` cases (case1/2/3) plus the abstract-array-family probe
///   (case4, Issue #6495 stages 7a/7c-ii).
pub(crate) fn should_runtime_dispatch(
    table: &crate::compile::method_table::MethodTable,
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
    has_any_arg: bool,
) -> bool {
    let has_multiple_methods = table.methods.len() > 1;
    let use_single_arg_runtime_dispatch = has_any_arg && has_multiple_methods && args_len == 1;
    let use_multi_arg_runtime_dispatch = has_multiple_methods && args_len > 1 && {
        // The per-slot `Any` probes read the canonical `core_signature`
        // projection first (Issue #6495, stage 6b-iii); `param_count()`
        // comparisons are arity reads. Out-of-range slots are non-matches,
        // preserving the legacy `zip` truncation / `params.get` `Option` gates.
        let (case1, case2, case3) = if has_any_arg {
            let case1 = arg_types.iter().enumerate().any(|(idx, arg_ty)| {
                matches!(arg_ty, JuliaType::Any)
                    && method_param_is_not_any_at_call_position(method, idx)
            });
            let case2 = !case1 && {
                let matched_all_any = method.all_params_match(core_is_any_param);
                matched_all_any
                    && table.methods.iter().any(|m| {
                        m.accepts_arity(args_len)
                            && m.global_index != method.global_index
                            && m.any_param_matches(|core| !core_is_any_param(core))
                    })
            };
            let case3 = !case1
                && !case2
                && table.methods.iter().any(|m| {
                    m.accepts_arity(args_len)
                        && m.global_index != method.global_index
                        && (0..args_len).any(|idx| {
                            matches!(arg_types.get(idx), Some(JuliaType::Any))
                                && method_param_is_any_at_call_position(method, idx)
                                && method_param_is_not_any_at_call_position(m, idx)
                        })
                });
            (case1, case2, case3)
        } else {
            (false, false, false)
        };
        // Abstract-array-family probe sourced from the canonical
        // `core_signature` projection (Issue #6495, stages 7a/7c-ii).
        let case4 = table.methods.iter().any(|m| {
            m.global_index != method.global_index
                && m.accepts_arity(args_len)
                && (0..args_len).any(|idx| {
                    arg_types
                        .get(idx)
                        .is_some_and(is_rank_unknown_array_julia_type)
                        && m.param_matches_at_call_position(idx, core_is_abstract_array_family_type)
                })
        });
        let case5 = runtime_unknown_struct_arg_requires_dispatch(method, arg_types, args_len)
            || method_has_anonymous_bounded_parametric_struct_for_struct_arg(
                method, arg_types, args_len,
            );
        case1 || case2 || case3 || case4 || case5
    };
    use_single_arg_runtime_dispatch || use_multi_arg_runtime_dispatch
}

pub(crate) fn should_use_dynamic_call_for_runtime_dispatch(
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
) -> bool {
    runtime_unknown_struct_arg_requires_dispatch(method, arg_types, args_len)
        || method_has_anonymous_bounded_parametric_struct_for_struct_arg(
            method, arg_types, args_len,
        )
}

fn runtime_unknown_struct_arg_requires_dispatch(
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
) -> bool {
    let Some(core_params) = method.expanded_core_param_types_for_arity(args_len) else {
        return arg_types.iter().any(is_runtime_unknown_struct_arg);
    };
    core_params
        .iter()
        .zip(arg_types.iter())
        .any(|(param, arg)| {
            is_runtime_unknown_struct_arg(arg)
                && !julia_struct_arg_matches_param(
                    &crate::inference_core::core_type_to_julia_type(param),
                    arg,
                )
        })
}

fn julia_struct_arg_matches_param(param: &JuliaType, arg: &JuliaType) -> bool {
    match (param, arg) {
        (JuliaType::Struct(param_name), JuliaType::Struct(arg_name)) => {
            nominal_family_name(param_name) == nominal_family_name(arg_name)
        }
        _ => param == arg,
    }
}

fn method_has_anonymous_bounded_parametric_struct_for_struct_arg(
    method: &crate::compile::method_table::MethodSig,
    arg_types: &[JuliaType],
    args_len: usize,
) -> bool {
    let Some(core_params) = method.expanded_core_param_types_for_arity(args_len) else {
        return false;
    };
    let where_vars = method
        .core_signature_type_vars()
        .into_iter()
        .map(|var| var.name)
        .collect::<std::collections::HashSet<_>>();
    core_params
        .iter()
        .zip(arg_types.iter())
        .any(|(param, arg)| {
            is_runtime_unknown_struct_arg(arg)
                && has_anonymous_bounded_typevar_inside_parametric_struct(param, &where_vars)
        })
        || method
            .projected_param_julia_types()
            .iter()
            .zip(arg_types.iter())
            .any(|(param, arg)| {
                is_runtime_unknown_struct_arg(arg)
                    && matches!(param, JuliaType::Struct(name) if name.contains("<:"))
            })
}

fn has_anonymous_bounded_typevar_inside_parametric_struct(
    ty: &CoreType,
    where_vars: &std::collections::HashSet<String>,
) -> bool {
    match ty {
        CoreType::Struct { params, .. } => params.iter().any(|param| {
            matches!(param, CoreType::TypeVar(var)
                if !where_vars.contains(&var.name)
                    && (var.lower_bound.is_some() || var.upper_bound.is_some()))
                || has_anonymous_bounded_typevar_inside_parametric_struct(param, where_vars)
        }),
        CoreType::Named(name) => name.contains("<:"),
        CoreType::Tuple(items) | CoreType::Union(items) => items
            .iter()
            .any(|item| has_anonymous_bounded_typevar_inside_parametric_struct(item, where_vars)),
        CoreType::Vararg(inner) | CoreType::TypeOf(inner) => {
            has_anonymous_bounded_typevar_inside_parametric_struct(inner, where_vars)
        }
        CoreType::VarargLen { element, len } => {
            has_anonymous_bounded_typevar_inside_parametric_struct(element, where_vars)
                || has_anonymous_bounded_typevar_inside_parametric_struct(len, where_vars)
        }
        CoreType::NamedTuple(fields) => fields.iter().any(|(_, field_ty)| {
            has_anonymous_bounded_typevar_inside_parametric_struct(field_ty, where_vars)
        }),
        CoreType::UnionAll { var, body } => {
            let mut nested_where_vars = where_vars.clone();
            nested_where_vars.insert(var.name.clone());
            has_anonymous_bounded_typevar_inside_parametric_struct(body, &nested_where_vars)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        core_is_any_param, core_is_typeof_param, core_is_typeof_typevar_param,
        method_has_typeof_param, method_has_typeof_typevar_param, method_param_is_any_at,
        method_param_is_not_any_at, should_runtime_dispatch,
    };
    use crate::compile::method_table::{MethodSig, MethodTable};
    use crate::inference_core::{core_type_to_julia_type, CoreType};
    use crate::types::{JuliaType, TypeParam};
    use crate::vm::ValueType;

    /// Round-tripping parameter spellings the call-dispatch heuristics see:
    /// the CoreType-native predicates must agree with the canonical inverse of
    /// the same core row (Issue #6495, stage 7c-ii).
    #[test]
    fn call_dispatch_predicates_match_canonical_inverse_issue_6495() {
        let shapes = vec![
            JuliaType::Any,
            JuliaType::Int64,
            JuliaType::Float64,
            JuliaType::String,
            JuliaType::DataType,
            JuliaType::Number,
            JuliaType::Struct("Complex{Float64}".to_string()),
            JuliaType::VectorOf(Box::new(JuliaType::Int64)),
            JuliaType::TypeVar("T".to_string(), None),
            JuliaType::TypeOf(Box::new(JuliaType::Int64)),
            JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
            JuliaType::TypeOf(Box::new(JuliaType::TypeVar(
                "T".to_string(),
                Some("Real".to_string()),
            ))),
            JuliaType::Union(vec![JuliaType::Int64, JuliaType::Float64]),
        ];
        for ty in shapes {
            let core = CoreType::from(&ty);
            let projected = core_type_to_julia_type(&core);
            assert_eq!(
                core_is_typeof_param(&core),
                matches!(projected, JuliaType::TypeOf(_)),
                "typeof_param diverges for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_typeof_typevar_param(&core),
                matches!(
                    &projected,
                    JuliaType::TypeOf(inner)
                        if matches!(inner.as_ref(), JuliaType::TypeVar(_, _))
                ),
                "typeof_typevar_param diverges for {ty:?} (core {core:?})"
            );
            assert_eq!(
                core_is_any_param(&core),
                matches!(projected, JuliaType::Any),
                "any_param diverges for {ty:?} (core {core:?})"
            );
        }
    }

    /// The method-level wrappers read the structured `core_signature` path;
    /// on a Bottom placeholder they report the conservative defaults (Issue
    /// #6495, stage 7c-ii).
    #[test]
    fn call_dispatch_method_probes_read_canonical_signature_issue_6495() {
        let make_params = |tys: Vec<JuliaType>| {
            tys.into_iter()
                .enumerate()
                .map(|(i, ty)| (format!("x{i}"), ty))
                .collect::<Vec<_>>()
        };
        let shape_rows = vec![
            vec![JuliaType::Any, JuliaType::Int64],
            vec![JuliaType::Any, JuliaType::Any],
            vec![
                JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".to_string(), None))),
                JuliaType::Int64,
            ],
            vec![JuliaType::TypeOf(Box::new(JuliaType::Int64))],
            vec![JuliaType::Struct("Complex{Float64}".to_string())],
        ];
        for tys in shape_rows {
            let params = make_params(tys);
            let bottom = MethodSig::bottom_for_tests(
                0,
                7,
                params.clone(),
                ValueType::Any,
                None,
                false,
                None,
                None,
            );

            assert!(bottom.structured_arg_core_types().is_none());
            assert!(!method_has_typeof_param(&bottom));
            assert!(!method_has_typeof_typevar_param(&bottom));
            for i in 0..=bottom.param_count() {
                assert!(!method_param_is_any_at(&bottom, i));
                assert!(!method_param_is_not_any_at(&bottom, i));
            }
            assert!(bottom.projected_param_julia_types().is_empty());

            let sig = MethodSig::for_tests(
                0,
                7,
                params,
                ValueType::Any,
                None,
                false,
                vec![],
                CoreType::Bottom,
                None,
                None,
            );
            assert!(sig.structured_arg_core_types().is_some());
            let row = sig.projected_param_julia_types();
            let expected = (
                row.iter().any(|ty| matches!(ty, JuliaType::TypeOf(_))),
                row.iter().any(|ty| {
                    matches!(
                        ty,
                        JuliaType::TypeOf(inner)
                            if matches!(inner.as_ref(), JuliaType::TypeVar(_, _))
                    )
                }),
                (0..=sig.param_count()) // one past the end: out-of-range is false
                    .map(|i| {
                        let at = row.get(i);
                        (
                            at.is_some_and(|ty| matches!(ty, JuliaType::Any)),
                            at.is_some_and(|ty| !matches!(ty, JuliaType::Any)),
                        )
                    })
                    .collect::<Vec<_>>(),
                row.clone(),
            );
            let structured = (
                method_has_typeof_param(&sig),
                method_has_typeof_typevar_param(&sig),
                (0..=sig.param_count())
                    .map(|i| {
                        (
                            method_param_is_any_at(&sig, i),
                            method_param_is_not_any_at(&sig, i),
                        )
                    })
                    .collect::<Vec<_>>(),
                sig.projected_param_julia_types(),
            );
            assert_eq!(expected, structured, "probe divergence for {row:?}");
        }
    }

    #[test]
    fn runtime_dispatch_probe_sees_vararg_call_positions_issue_8407() {
        let generic = MethodSig::for_tests(
            0,
            10,
            vec![
                ("x".to_string(), JuliaType::Any),
                ("ys".to_string(), JuliaType::Any),
            ],
            ValueType::I64,
            None,
            false,
            vec![],
            CoreType::Bottom,
            Some(1),
            None,
        );
        let specific = MethodSig::for_tests(
            1,
            20,
            vec![
                (
                    "x".to_string(),
                    JuliaType::Struct("QuadGK.BatchIntegrand{Y, Nothing}".to_string()),
                ),
                ("y".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                ("z".to_string(), JuliaType::TypeVar("T".to_string(), None)),
                (
                    "rest".to_string(),
                    JuliaType::TypeVar("T".to_string(), None),
                ),
            ],
            ValueType::I64,
            None,
            false,
            vec![
                TypeParam::new("Y".to_string()),
                TypeParam::new("T".to_string()),
            ],
            CoreType::Bottom,
            Some(3),
            None,
        );
        let mut table = MethodTable::new("myq".to_string());
        table.add_method(generic);
        table.add_method(specific);
        let selected = table
            .dispatch(&[JuliaType::Any, JuliaType::Float64, JuliaType::Float64])
            .expect("dispatch");

        assert_eq!(selected.global_index, 10);
        assert!(should_runtime_dispatch(
            &table,
            selected,
            &[JuliaType::Any, JuliaType::Float64, JuliaType::Float64],
            3,
            true
        ));
    }
}
