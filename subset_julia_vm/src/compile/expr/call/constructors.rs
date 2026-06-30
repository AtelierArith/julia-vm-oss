//! Struct-constructor resolution helpers for `compile_call` (Issue #6332).
//!
//! Pure extraction of the constructor-resolution chain that sits between the
//! pre-match special-case handler table and the post-struct handler table in
//! `compile_call`: explicit parametric constructors (`Point{Float64}(...)`,
//! `Dict{K,V}()`, ...), direct `struct_table` constructors, and
//! `resolve_parametric_struct_name`-inferred parametric constructors. Each
//! helper returns `Ok(None)` when its case does not apply so `compile_call`
//! falls through to the next stage, exactly like the original inline blocks.

use std::borrow::Cow;

use crate::ir::core::Expr;
use crate::types::JuliaType;
use crate::vm::{Instr, StaticParamBinding, StaticParametricCall, ValueType};

use crate::compile::{
    parse_parametric_call, CResult, CompileError, CoreCompiler, MethodSig, TypeExpr,
};

use super::is_static_val_runtime_expr;

impl CoreCompiler<'_> {
    // Check for explicit parametric type constructor: Point{Float64}(...)
    // (public Dict{K,V}(...) calls are routed to Julia methods before this
    // chain; only Dict's internal 8-field constructor reaches this path).
    /// (`Point{Float64}(...)`, `Dict{String, Int}()`, ...). `Ok(None)` =
    /// the name is not a parametric call, or it needs the generic fallback.
    pub(super) fn try_compile_parametric_constructor_call(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        let Some((base_name, type_args)) = parse_parametric_call(function) else {
            return Ok(None);
        };
        // Issue #8101: `t{T...}(args)` where `t` is a *local variable* holding a
        // runtime `DataType` value, not a statically-known parametric struct.
        // The base type is unknown at compile time, so apply the explicit type
        // parameters and call the resulting concrete `DataType` dynamically —
        // the explicit-type-argument analogue of the no-type-argument dynamic
        // form `t(args)` (Issue #8070). Guarded so it only fires when `base_name`
        // is genuinely a local `DataType` that does not resolve to any known
        // (parametric or concrete) struct, leaving every static path untouched.
        if self.locals.get(&base_name) == Some(&ValueType::DataType)
            && !self.shared_ctx.parametric_structs.contains_key(&base_name)
            && !self.shared_ctx.struct_table.contains_key(&base_name)
            && self.resolve_parametric_struct_name(&base_name).is_none()
        {
            return self
                .compile_local_datatype_parametric_call(&base_name, &type_args, args)
                .map(Some);
        }
        // `Set{T}(...)` is routed to pure-Julia `Set{T}` constructor methods
        // before this chain (see `is_public_set_constructor_method_call` in
        // `compile_call`); it no longer emits a native `Value::Set` carrier
        // (Issue #6721).
        // Handle Array{T}(), Vector{T}(), and Matrix{T}() - built-in parametric types
        // Only intercept known built-in patterns; fall through for struct constructor patterns
        // (e.g., Array{T,N}(mem, size) for Pure Julia Array struct) (Issue #2760)
        if base_name == "Array" || base_name == "Vector" || base_name == "Matrix" {
            let is_undef_alloc =
                matches!(args.first(), Some(Expr::Var(name, _)) if name == "undef");
            let is_builtin_pattern = if base_name == "Matrix" {
                is_undef_alloc
            } else {
                args.is_empty() || args.len() == 1 || is_undef_alloc
            };
            if is_builtin_pattern {
                return self
                    .compile_array_constructor(&type_args, args, function)
                    .map(Some);
            }
            // Fall through to struct constructor for non-builtin patterns
        }
        // Handle Memory{T}(n) - built-in parametric type
        if base_name == "Memory" {
            return self.compile_memory_constructor(&type_args, args).map(Some);
        }
        // Handle Ref{T}(x) / Base.RefValue{T}(x) - mutable single-element box (Issue #5130).
        // The element type parameter is dropped at the value level (Value::Ref carries the
        // boxed value directly); typeof() reconstructs Base.RefValue{T} from the inner value.
        if base_name == "Ref" || base_name == "RefValue" || base_name == "Base.RefValue" {
            if args.len() == 1 {
                self.compile_expr(&args[0])?;
                self.emit(Instr::MakeRef);
                return Ok(Some(ValueType::Any));
            } else if args.is_empty() {
                // Ref{T}() - uninitialized box. The no-JIT VM has no #undef-typed
                // RefValue; approximate with a Ref wrapping `#undef`.
                self.emit(Instr::PushUndef);
                self.emit(Instr::MakeRef);
                return Ok(Some(ValueType::Any));
            }
        }
        // Check if any type_arg is a type variable (like Rational{T} in a where T function)
        // or a local DataType variable (like Point{Tnew} where Tnew = promote_type(...))
        // or a runtime expression (like Symbol(s) in MIME{Symbol(s)})
        // If so, we can't instantiate at compile time - need runtime construction
        let has_type_var = type_args.iter().any(|arg| {
            match arg {
                TypeExpr::TypeVar(name) => {
                    // Pure numeric strings (like "5" in Val{5}) are VALUE parameters, not type variables
                    // They should be preserved as-is in the type name
                    if name.chars().all(|c| c.is_ascii_digit()) {
                        return false; // Not a type variable, just a value parameter
                    }
                    // Type variable from where clause (short uppercase like T, T1)
                    let is_where_type_var = name.len() <= 2
                        && name
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
                    // Local variable holding a DataType value. If the local's
                    // compile-time type has widened to Any, still treat it as a
                    // runtime type/value parameter so calls like
                    // `Segment{Tx,...}` with `Tx = float(eltype(xs))` do not
                    // fall into the static default-constructor path. (Issue #8321)
                    let is_local_datatype = self.locals.get(name) == Some(&ValueType::DataType);
                    let is_local_runtime_value = self.locals.contains_key(name);
                    is_where_type_var || is_local_datatype || is_local_runtime_value
                }
                TypeExpr::RuntimeExpr(expr_str) => {
                    !(base_name == "Val" && is_static_val_runtime_expr(expr_str))
                }
                _ => false,
            }
        });
        // Check if we need dynamic struct construction (type arg is a local DataType variable
        // or a type parameter from where clause or a runtime expression)
        let needs_dynamic_construction = type_args.iter().any(|arg| {
            match arg {
                TypeExpr::TypeVar(name) => {
                    // Local DataType/runtime value variable. Even when inference
                    // widens a local type object to Any, `LoadAny` can recover
                    // the runtime DataType for dynamic type application.
                    // (Issue #8321)
                    let is_local_datatype = self.locals.get(name) == Some(&ValueType::DataType);
                    let is_local_runtime_value = self.locals.contains_key(name);
                    // Val(x) lowers to Val{x}(); unlike ordinary type
                    // parameters, x is the runtime value being lifted into
                    // the Val type parameter.
                    let is_val_runtime_value = base_name == "Val" && self.locals.contains_key(name);
                    // Type parameter from where clause (current_type_params)
                    let is_type_param = self.current_type_param_index.contains_key(name.as_str());
                    is_local_datatype
                        || is_local_runtime_value
                        || is_val_runtime_value
                        || is_type_param
                }
                TypeExpr::RuntimeExpr(expr_str) => {
                    !(base_name == "Val" && is_static_val_runtime_expr(expr_str))
                }
                _ => false,
            }
        });
        if needs_dynamic_construction {
            // Use dynamic parametric struct construction
            return self
                .compile_dynamic_parametric_struct(&base_name, &type_args, args)
                .map(Some);
        }
        if !has_type_var {
            if !self.is_fully_applied_default_field_constructor_call(&base_name, &type_args, args) {
                if let Some((func_index, bindings)) =
                    self.static_parametric_constructor_method(&base_name, &type_args, args.len())
                {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::CallStaticParametric(Box::new(
                        StaticParametricCall {
                            func_index,
                            arg_count: args.len(),
                            bindings,
                        },
                    )));
                    return Ok(Some(ValueType::Any));
                }
            }

            let resolved_base_name = self
                .resolve_parametric_struct_name(&base_name)
                .unwrap_or_else(|| base_name.clone());
            // Explicit-type-parameter outer constructor (Issue #5132).
            // A user may define a constructor keyed on a *concrete* parametric
            // name, e.g. `function Rational{Int8}(num::Integer, den::Integer)`.
            // Such methods register in the method table under their full name
            // ("Rational{Int8}"), not the bare base name ("Rational"), so the
            // arity-based inner-constructor lookup below never finds them and
            // the raw inner `Rational{T}(num::T, den::T)` wins instead — which
            // infers T from the argument values (dropping the explicit `{Int8}`)
            // and skips normalization. Consult the full parametric name first so
            // these element-type-coercing / normalizing constructors take
            // precedence. This is opt-in: it only fires when such a method is
            // explicitly defined, leaving the raw-inner fast path untouched for
            // every other parametric struct (and for the generic where-T calls
            // these constructors delegate to, which route through the has_type_var
            // path below and so are never intercepted here — no recursion).
            if function != base_name {
                if let Some(table) = self.method_tables.get(function) {
                    let arg_types: Vec<JuliaType> =
                        args.iter().map(|a| self.infer_julia_type(a)).collect();
                    let static_method_index =
                        table.dispatch(&arg_types).ok().map(|m| m.global_index);
                    // Compile-time inference may be too broad for single-argument
                    // conversions such as `Rational{Int64}(r)` where `r` is
                    // constructed by an expression. If an explicit constructor table
                    // has same-arity runtime candidates, keep the call in method
                    // dispatch instead of falling through to the raw struct
                    // constructor (Issue #6267).
                    let candidates: Vec<usize> = table
                        .methods
                        .iter()
                        .filter(|m| m.accepts_arity(args.len()))
                        .map(|m| m.global_index)
                        .collect();
                    if static_method_index.is_some() || !candidates.is_empty() {
                        let fallback_index = static_method_index
                            .or_else(|| candidates.first().copied())
                            .ok_or_else(|| {
                                CompileError::Msg(
                                    "Internal error: explicit constructor dispatch had no fallback"
                                        .to_string(),
                                )
                            })?;
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        // Compile-time inference of constructor arguments is often
                        // imprecise (e.g. `Int8(3)//Int8(4)` infers to a broad
                        // numeric type, not `Rational{Int8}`), which can make the
                        // static pick choose `(x::Integer)` over `(x::Rational)`.
                        // Emit runtime multiple dispatch over the concrete-typed
                        // candidates of this explicit-name table so the actual
                        // argument types select the right method (Issue #5132).
                        if candidates.len() > 1 || static_method_index.is_none() {
                            self.emit(Instr::CallTypedDispatch(
                                function.to_string(),
                                args.len(),
                                fallback_index,
                                candidates,
                            ));
                        } else if let Some(fallback_index) = candidates.first() {
                            self.emit(Instr::Call(*fallback_index, args.len()));
                        } else {
                            self.emit(Instr::Call(fallback_index, args.len()));
                        }
                        let resolved_type_id =
                            self.shared_ctx.resolve_instantiation_with_type_expr(
                                &resolved_base_name,
                                &type_args,
                            )?;
                        return Ok(Some(ValueType::Struct(resolved_type_id)));
                    }
                }
            }
            if base_name == "Matrix" {
                let arg_types: Vec<JuliaType> =
                    args.iter().map(|a| self.infer_julia_type(a)).collect();
                for arg in args {
                    self.compile_expr(arg)?;
                }
                for _ in args {
                    self.emit(Instr::Pop);
                }
                self.emit(Instr::ThrowMethodError(format!(
                    "no method matching {}({})",
                    function,
                    arg_types
                        .iter()
                        .map(|t| format!("::{}", t))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
                return Ok(Some(ValueType::Any));
            }
            // Explicit type parameters provided for user-defined structs (including Complex)
            // Check if there's an inner constructor for this struct in method_tables
            // Inner constructors are identified by having type_params (where clause)
            // This distinguishes them from outer constructors like Rational(num::Int64, den::Int64)
            if let Some(table) = self.method_tables.get(&base_name) {
                // Find inner constructors only (those with type_params, not outer constructors)
                let arg_types: Vec<JuliaType> =
                    args.iter().map(|a| self.infer_julia_type(a)).collect();
                // Try to find a matching method that has type parameters (inner constructor)
                // `where`-clause presence read from the canonical
                // `core_signature` `UnionAll` wrappers (legacy `type_params`
                // as the structured-unavailable fallback — Issue #6495,
                // stage 7a); `param_count()` is an arity read.
                let arity_matched_inner_ctor = table
                    .methods
                    .iter()
                    .filter(|m| m.has_where_params()) // Inner constructors have where clause
                    .filter(|m| m.param_count() == arg_types.len());
                let mut saw_arity_matched_inner_ctor = false;
                let inner_ctor_match = arity_matched_inner_ctor
                    .inspect(|_| saw_arity_matched_inner_ctor = true)
                    .find(|m| self.explicit_type_args_satisfy_inner_ctor_bounds(m, &type_args));
                if let Some(method) = inner_ctor_match {
                    let global_index = method.global_index;
                    // Issue #8121: bind the explicit `{...}` type parameters into
                    // the inner constructor's frame so its body can reference them
                    // as runtime *values* — e.g. `Angle2d{T}(theta) =
                    // new{T}(T(theta))` converts via `T(theta)` and `RotMatrix`
                    // (or `new{T}`) lifts T. Invoking the inner ctor with a bare
                    // `Call` (no bindings) leaves those type vars unbound and the
                    // body raises `UndefVarError: T`. Mirror the binding the
                    // `static_parametric_constructor_method` /
                    // `CallStaticParametric` path already performs for the
                    // non-default-field route: zip the inner constructor's own
                    // `where` vars (which name the type params the body uses,
                    // honoring inner-ctor renames like `Q{S}(x) where {S}`) with
                    // the explicit `{...}` type arguments, positionally. When the
                    // counts disagree (no/partial explicit params), fall back to a
                    // bare `Call`, preserving the prior behavior.
                    let type_vars = method.core_signature_type_vars();
                    let bindings: Vec<StaticParamBinding> = if type_vars.len() == type_args.len() {
                        type_vars
                            .iter()
                            .zip(type_args.iter())
                            .map(|(var, value)| StaticParamBinding {
                                name: var.name.clone(),
                                value: value.clone(),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    // Compile arguments
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    // Call the inner constructor (with type-param bindings when the
                    // explicit `{...}` parameters are fully supplied).
                    if bindings.is_empty() {
                        self.emit(Instr::Call(global_index, args.len()));
                    } else {
                        self.emit(Instr::CallStaticParametric(Box::new(
                            StaticParametricCall {
                                func_index: global_index,
                                arg_count: args.len(),
                                bindings,
                            },
                        )));
                    }
                    // For parametric structs with explicit type params like Rational{Int64}(...),
                    // resolve the concrete type_id and return Struct type instead of Any
                    // This ensures the variable gets the correct struct type for method dispatch
                    let resolved_type_id = self
                        .shared_ctx
                        .resolve_instantiation_with_type_expr(&resolved_base_name, &type_args)?;
                    return Ok(Some(ValueType::Struct(resolved_type_id)));
                }
                if saw_arity_matched_inner_ctor {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    for _ in args {
                        self.emit(Instr::Pop);
                    }
                    self.emit(Instr::ThrowMethodError(format!(
                        "no method matching {}{{{}}}({})",
                        base_name,
                        TypeExpr::render_param_list(&type_args),
                        arg_types
                            .iter()
                            .map(|t| format!("::{}", t))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                    return Ok(Some(ValueType::Any));
                }
                // If no inner constructor found, fall through to default constructor
            }
            // No inner constructors or dispatch failed - use default struct constructor
            let declared_param_count = self
                .shared_ctx
                .parametric_structs
                .get(&resolved_base_name)
                .or_else(|| self.shared_ctx.parametric_structs.get(&base_name))
                .map(|parametric| parametric.def.type_params.len())
                .unwrap_or(type_args.len());
            if type_args.len() < declared_param_count {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::PushDataType(resolved_base_name));
                for type_arg in &type_args {
                    self.emit_parametric_type_arg_value(type_arg)?;
                }
                self.emit(Instr::ApplyTypeDynamic(type_args.len()));
                self.emit(Instr::CallFunctionVariable(args.len()));
                return Ok(Some(ValueType::Any));
            }
            let type_id = self
                .shared_ctx
                .resolve_instantiation_with_type_expr(&resolved_base_name, &type_args)?;
            let struct_info = self
                .shared_ctx
                .struct_table
                .values()
                .find(|s| s.type_id == type_id)
                .cloned()
                .ok_or_else(|| {
                    CompileError::Msg("Internal error: instantiation not found".to_string())
                })?;
            return self.compile_struct_constructor(struct_info, args).map(Some);
        }
        // Type variable detected (e.g., Rational{T}(...) in a where T function)
        // When type variables are present, we cannot know the exact type at compile time.
        // Instead of inferring from arguments (which may fail for type bounds),
        // instantiate with Any to defer type resolution to runtime.
        // Resolve to qualified name for module structs (e.g., Point -> MyGeometry.Point)
        if let Some(resolved_name) = self.resolve_parametric_struct_name(&base_name) {
            // Check if the struct has inner constructors (methods in method_tables)
            // If so, use method dispatch to call the inner constructor instead of
            // bypassing it with compile_struct_constructor
            if let Some(table) = self.method_tables.get(&base_name) {
                // Inner constructors exist - dispatch to them
                // Infer argument types for dispatch
                let arg_types: Vec<JuliaType> =
                    args.iter().map(|a| self.infer_julia_type(a)).collect();

                // Find the best matching method
                if let Ok(method) = table.dispatch(&arg_types) {
                    // Compile arguments
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    // Call the inner constructor
                    self.emit(Instr::Call(method.global_index, args.len()));
                    // For type variable constructors like Rational{T}(...), return Struct type
                    // with the generic instantiation (Rational{Any}) so method dispatch works
                    let type_id = self
                        .shared_ctx
                        .resolve_instantiation(&resolved_name, &[JuliaType::Any])?;
                    return Ok(Some(ValueType::Struct(type_id)));
                }
                // If dispatch fails, fall through to default constructor
            }
            // No inner constructors or dispatch failed - use default struct constructor
            // Use Any as the type parameter - this will pass all bound checks
            // and the runtime will handle the actual type
            let type_id = self
                .shared_ctx
                .resolve_instantiation(&resolved_name, &[JuliaType::Any])?;
            let struct_info = self
                .shared_ctx
                .struct_table
                .values()
                .find(|s| s.type_id == type_id)
                .cloned()
                .ok_or_else(|| {
                    CompileError::Msg("Internal error: instantiation not found".to_string())
                })?;
            return self.compile_struct_constructor(struct_info, args).map(Some);
        }
        Ok(None)
    }

    fn is_fully_applied_default_field_constructor_call(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> bool {
        let Some(parametric_def) =
            self.shared_ctx
                .parametric_structs
                .get(base_name)
                .or_else(|| {
                    self.resolve_parametric_struct_name(base_name)
                        .and_then(|resolved| self.shared_ctx.parametric_structs.get(&resolved))
                })
        else {
            return false;
        };
        let arity_matches = type_args.len() == parametric_def.def.type_params.len()
            && args.len() == parametric_def.def.fields.len();
        // Release the immutable `shared_ctx` borrow before the `&mut self` check.
        if !arity_matches {
            return false;
        }
        // Issue #8103: arity alone is insufficient. The synthesized default
        // field constructor only applies when the argument types are actually
        // convertible to the (instantiated) field types. When they are NOT (e.g.
        // `RotMatrix{2,Float32}(theta::Number)` whose field is an `SMatrix`), a
        // user-defined typed outer constructor `Foo{N,T}(::Number)` must win, so
        // report "not a default field constructor call" and let the static
        // parametric-constructor lookup find it. Without this, the call was
        // forced into the default constructor and raised a compile-time
        // `Cannot convert ...` (the parametric analogue of the non-parametric
        // #7793 `struct_field_count_ctor_args_convertible` guard).
        self.parametric_default_ctor_args_convertible(base_name, type_args, args)
    }

    /// True iff every argument is statically convertible to the corresponding
    /// (instantiated) field type of the parametric struct — i.e. the synthesized
    /// default field constructor would accept this call (Issue #8103). Mirrors
    /// the field-branch selection of [`Self::compile_struct_constructor`]: only
    /// plain concrete typed fields go through `compile_expr_as` (and can fail to
    /// convert); `Any`/`Function`/abstract-numeric/runtime-coercion fields accept
    /// any value type. Returns `true` conservatively when the instantiation
    /// cannot be resolved (preserving the prior arity-only behavior).
    fn parametric_default_ctor_args_convertible(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> bool {
        let resolved = self
            .resolve_parametric_struct_name(base_name)
            .unwrap_or_else(|| base_name.to_string());
        let Ok(type_id) = self
            .shared_ctx
            .resolve_instantiation_with_type_expr(&resolved, type_args)
        else {
            return true;
        };
        let field_jts: Vec<crate::types::JuliaType> =
            match self.shared_ctx.field_julia_types_by_type_id(type_id) {
                Some(fts) => fts.to_vec(),
                None => return true,
            };
        if field_jts.len() != args.len() {
            return true;
        }
        for (arg, field_jt) in args.iter().zip(field_jts.iter()) {
            // The field's JuliaType is authoritative. A scalar argument is NOT
            // convertible to a non-numeric container/struct field — but the lossy
            // ValueType collapses an unregistered concrete struct (e.g.
            // `SMatrix{2,2,Float32}`, the `RotMatrix` field) to `Any`, which then
            // "accepts" any numeric scalar. Reject that directly off the
            // JuliaType so the typed outer constructor wins (#8103).
            let arg_jt = self.infer_julia_type(arg);
            if julia_type_is_scalar_numeric(&arg_jt) && julia_type_is_nonscalar_field(field_jt) {
                return false;
            }
            // Remaining (scalar↔scalar / genuine-`Any`) cases reuse the ValueType
            // convertibility, mirroring `compile_struct_constructor`'s field-branch
            // selection (Any/Function/abstract-numeric/runtime-coercion fields
            // accept any value type).
            let field_vt = crate::compile::type_helpers::julia_type_to_value_type(field_jt);
            let goes_through_compile_expr_as = field_vt != ValueType::Any
                && field_vt != ValueType::Function
                && !crate::compile::expr::struct_::field_type_is_abstract_numeric(field_jt)
                && !crate::compile::expr::struct_::field_type_needs_runtime_coercion(field_jt);
            if goes_through_compile_expr_as {
                let actual = self.infer_expr_type(arg);
                if matches!(actual, ValueType::Any) {
                    continue;
                }
                if !self.coercion_accepts(&actual, &field_vt) {
                    return false;
                }
            }
        }
        true
    }

    fn static_parametric_constructor_method(
        &self,
        base_name: &str,
        type_args: &[TypeExpr],
        arg_count: usize,
    ) -> Option<(usize, Vec<StaticParamBinding>)> {
        let resolved_base_name = self.resolve_parametric_struct_name(base_name);
        for (table_name, table) in self.method_tables {
            let Some((table_base, pattern_args)) = parse_parametric_call(table_name) else {
                continue;
            };
            if !same_parametric_constructor_base(
                &table_base,
                base_name,
                resolved_base_name.as_deref(),
            ) {
                continue;
            }
            if pattern_args.len() != type_args.len() {
                continue;
            }
            let mut bindings = Vec::with_capacity(type_args.len());
            let mut all_params_are_where_vars = true;
            for (pattern, value) in pattern_args.iter().zip(type_args.iter()) {
                let TypeExpr::TypeVar(name) = pattern else {
                    all_params_are_where_vars = false;
                    break;
                };
                bindings.push(StaticParamBinding {
                    name: name.clone(),
                    value: value.clone(),
                });
            }
            if !all_params_are_where_vars {
                continue;
            }

            if let Some(method) = table.methods.iter().find(|method| {
                method.accepts_arity(arg_count) && method.has_where_params() && {
                    let vars = method.core_signature_type_vars();
                    bindings
                        .iter()
                        .all(|binding| vars.iter().any(|var| var.name == binding.name))
                }
            }) {
                return Some((method.global_index, bindings));
            }
        }
        None
    }

    // Check if this is a concrete struct constructor
    // If method_tables has an entry for this struct name, use dispatch (inner constructors)
    // Otherwise, use the default constructor
    /// `Ok(None)` = not a known struct name, or inner-constructor method
    /// dispatch must run instead.
    pub(super) fn try_compile_struct_table_constructor_call(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        if let Some(struct_info) = self.shared_ctx.struct_table.get(function).cloned() {
            let short_name = short_constructor_name(function);
            let method_table_name = if self.method_tables.contains_key(function) {
                Some(Cow::Borrowed(function))
            } else if function.contains('.')
                && struct_info.has_inner_constructor
                && self.method_tables.contains_key(short_name.as_ref())
            {
                Some(short_name)
            } else {
                None
            };
            if method_table_name.is_none() {
                return self
                    .compile_struct_constructor(struct_info.clone(), args)
                    .map(Some);
            }
            let method_table_name = method_table_name.expect("checked above");
            // Check if argument types exactly match struct field types. In this
            // case, use the default constructor to avoid infinite recursion when
            // outer constructors like `Year(v::Int) = Year(Int64(v))` exist.
            // Issue #4343: macro-generated keyword constructors such as @kwdef
            // can coexist with constructor-table entries that are not matching
            // inner constructors. The exact field-layout match remains the narrow
            // guard for the default constructor fallback.
            if args.len() == struct_info.fields.len() {
                let arg_types: Vec<JuliaType> =
                    args.iter().map(|a| self.infer_julia_type(a)).collect();
                let has_same_arity_declared_constructor = self
                    .method_tables
                    .get(method_table_name.as_ref())
                    .is_some_and(|t| t.methods.iter().any(|m| m.accepts_arity(args.len())));
                if !struct_info.has_inner_constructor
                    && !has_same_arity_declared_constructor
                    && arg_types.iter().any(|arg| matches!(arg, JuliaType::Any))
                {
                    return self
                        .compile_struct_constructor(struct_info.clone(), args)
                        .map(Some);
                }
                if !struct_info.has_inner_constructor && !has_same_arity_declared_constructor {
                    return self
                        .compile_struct_constructor(struct_info.clone(), args)
                        .map(Some);
                }

                // Issue #7345: a struct that declares its own inner constructor
                // has NO synthesized default field constructor in upstream Julia,
                // so the inner constructor body (validation, `new(...)` argument
                // massaging) must run instead of silently building the struct
                // from the raw fields. Route to method dispatch whenever a
                // declared constructor matches this call. Only fall back to the
                // synthetic field constructor when none matches — which is what
                // REPL global reconstruction relies on: it re-emits a full-field
                // positional call (`Animation([frame, …])`) for structs whose
                // inner constructors take fewer arguments (`Animation() =
                // new(Any[])`), where dispatch would otherwise fail outright.
                // For structs without an inner constructor the synthetic field
                // constructor *is* the only constructor, so this guard is inert
                // and the long-standing recursion-termination fast path (Year)
                // is preserved unchanged.
                let declared_ctor_matches = struct_info.has_inner_constructor
                    && self
                        .method_tables
                        .get(method_table_name.as_ref())
                        .map(|t| t.dispatch(&arg_types).is_ok())
                        .unwrap_or(false);

                if !declared_ctor_matches {
                    let field_types: Vec<JuliaType> = struct_info
                        .fields
                        .iter()
                        .map(|(_, vt)| self.value_type_to_julia_type(vt))
                        .collect();
                    // Check if types match, with Any matching any type
                    // This prevents infinite recursion when outer constructors exist
                    // and the argument type is not statically known
                    let all_match = arg_types
                        .iter()
                        .zip(field_types.iter())
                        .all(|(arg, field)| {
                            arg == field
                            // Field is Any - accepts any argument type (for untyped struct fields like CartesianIndices.dims)
                            || matches!(field, JuliaType::Any)
                            // Any-typed arguments are runtime values; let the
                            // synthesized field-count constructor perform the
                            // same runtime convert(fieldtype, x) that Julia's
                            // default constructor would. (This already subsumes
                            // the Any-arg-vs-numeric-field case, so no separate
                            // `&& field.is_builtin_numeric()` term is needed —
                            // clippy::overly_complex_bool_expr, a #8339 leftover.)
                            || matches!(arg, JuliaType::Any)
                        });
                    if all_match {
                        return self
                            .compile_struct_constructor(struct_info.clone(), args)
                            .map(Some);
                    }
                }
            }
            if method_table_name.as_ref() != function {
                // Issue #7631: hygiene can emit a module-qualified constructor
                // such as `Plots.Animation()`, while non-parametric inner
                // constructors are registered under the short struct name.
                return self
                    .compile_generic_dispatch_call(
                        method_table_name.as_ref(),
                        args,
                        &[],
                        &[],
                        false,
                    )
                    .map(Some);
            }
            // Fall through to method dispatch for inner constructors
        }
        Ok(None)
    }

    // Check if this is a parametric struct constructor with type inference
    /// `Ok(None)` = not a parametric struct name, or custom constructors
    /// exist and method dispatch must run instead.
    pub(super) fn try_compile_inferred_parametric_constructor_call(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        if let Some(resolved_name) = self.resolve_parametric_struct_name(function) {
            let short_name = short_constructor_name(function);
            let has_short_inner_constructor_table = function.contains('.')
                && self
                    .shared_ctx
                    .parametric_structs
                    .get(function)
                    .or_else(|| self.shared_ctx.parametric_structs.get(&resolved_name))
                    .is_some_and(|parametric| !parametric.def.inner_constructors.is_empty())
                && self.method_tables.contains_key(short_name.as_ref());
            // If there are methods defined for this name (like Complex(x::Int64)), try method dispatch first
            // This allows user-defined constructors to take precedence over the default struct constructor
            let has_same_arity_constructor = self
                .method_tables
                .get(function)
                .is_some_and(|table| table.methods.iter().any(|m| m.accepts_arity(args.len())));
            if !has_same_arity_constructor && !has_short_inner_constructor_table {
                // No custom constructors - use default struct constructor
                // Use resolved (qualified) name for instantiation so method dispatch works correctly
                let arg_types: Vec<JuliaType> =
                    args.iter().map(|a| self.infer_julia_type(a)).collect();
                let type_args = match self.shared_ctx.infer_type_args(function, &arg_types) {
                    Ok(type_args) => type_args,
                    Err(_) => {
                        if arg_types
                            .iter()
                            .any(|arg| matches!(arg, JuliaType::Any) || arg.is_abstract_container())
                        {
                            for arg in args {
                                self.compile_expr(arg)?;
                            }
                            self.emit(Instr::NewParametricStruct(
                                resolved_name.clone(),
                                args.len(),
                            ));
                            return Ok(Some(ValueType::Any));
                        }
                        // The argument types do not unify the struct's type
                        // parameters (e.g. `Pt9{T}(x::T, y::T)` called as
                        // `Pt9(1, 2.0)` — a single `T` cannot be both `Int64`
                        // and `Float64`). The default constructor does not match;
                        // upstream raises a `MethodError` at runtime rather than
                        // aborting compilation, so evaluate the arguments (for
                        // their side effects) and raise the same catchable error
                        // (Issue #8102).
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        for _ in args {
                            self.emit(Instr::Pop);
                        }
                        self.emit(Instr::ThrowMethodError(format!(
                            "no method matching {}({})",
                            function,
                            arg_types
                                .iter()
                                .map(|t| format!("::{}", t))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )));
                        return Ok(Some(ValueType::Any));
                    }
                };
                let type_id = self
                    .shared_ctx
                    .resolve_instantiation(&resolved_name, &type_args)?;
                let struct_info = self
                    .shared_ctx
                    .struct_table
                    .values()
                    .find(|s| s.type_id == type_id)
                    .cloned()
                    .ok_or_else(|| {
                        CompileError::Msg("Internal error: instantiation not found".to_string())
                    })?;
                return self.compile_struct_constructor(struct_info, args).map(Some);
            }
            // Fall through to method dispatch for custom constructors
        }
        Ok(None)
    }

    fn explicit_type_args_satisfy_inner_ctor_bounds(
        &self,
        method: &MethodSig,
        type_args: &[TypeExpr],
    ) -> bool {
        method
            .core_signature_type_vars()
            .iter()
            .zip(type_args.iter())
            .all(|(param, arg)| {
                let Some(bound_name) = param
                    .upper_bound
                    .as_deref()
                    .map(crate::inference_core::CoreType::to_julia_name)
                else {
                    return true;
                };
                match arg {
                    TypeExpr::Concrete(jt) => self
                        .shared_ctx
                        .concrete_type_satisfies_bound(jt, &bound_name),
                    TypeExpr::TypeVar(type_name) => {
                        if type_name.len() <= 2
                            && type_name
                                .chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                        {
                            true
                        } else {
                            self.shared_ctx
                                .type_name_satisfies_bound(type_name, &bound_name)
                        }
                    }
                    TypeExpr::Parameterized { .. } | TypeExpr::RuntimeExpr(_) => true,
                }
            })
    }
}

fn same_parametric_constructor_base(
    table_base: &str,
    call_base: &str,
    resolved_call_base: Option<&str>,
) -> bool {
    if table_base == call_base || resolved_call_base.is_some_and(|resolved| table_base == resolved)
    {
        return true;
    }

    if table_base.contains('.') {
        return false;
    }
    table_base == call_base.rsplit('.').next().unwrap_or(call_base)
}

fn short_constructor_name(name: &str) -> Cow<'_, str> {
    let Some(brace_idx) = name.find('{') else {
        return Cow::Borrowed(name.rsplit('.').next().unwrap_or(name));
    };

    let base = &name[..brace_idx];
    let params = &name[brace_idx..];
    let short_base = base.rsplit('.').next().unwrap_or(base);
    if short_base.len() == base.len() {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("{short_base}{params}"))
    }
}

/// A primitive scalar numeric `JuliaType` (the kind of value a `convert` to a
/// non-numeric container/struct field would reject). Abstract numeric types
/// (`Number`/`Real`/...) are intentionally excluded — a field declared with one
/// of those still accepts the value (Issue #8103).
fn julia_type_is_scalar_numeric(jt: &JuliaType) -> bool {
    matches!(
        jt,
        JuliaType::Int8
            | JuliaType::Int16
            | JuliaType::Int32
            | JuliaType::Int64
            | JuliaType::Int128
            | JuliaType::UInt8
            | JuliaType::UInt16
            | JuliaType::UInt32
            | JuliaType::UInt64
            | JuliaType::UInt128
            | JuliaType::Float16
            | JuliaType::Float32
            | JuliaType::Float64
            | JuliaType::Bool
            | JuliaType::BigInt
            | JuliaType::BigFloat
    )
}

/// A field whose declared `JuliaType` is a non-numeric container or struct, so a
/// scalar numeric argument is NOT convertible to it via the default
/// constructor's `convert(fieldtype, x)` (Issue #8103). `Complex`/`Rational`
/// are numeric structs that DO accept a scalar, so they are excluded.
fn julia_type_is_nonscalar_field(jt: &JuliaType) -> bool {
    match jt {
        JuliaType::Array
        | JuliaType::VectorOf(_)
        | JuliaType::MatrixOf(_)
        | JuliaType::Tuple
        | JuliaType::TupleOf(_)
        | JuliaType::NamedTuple
        | JuliaType::Dict
        | JuliaType::Set => true,
        JuliaType::Struct(name) => {
            let base = name.split('{').next().unwrap_or(name);
            let base = base.rsplit('.').next().unwrap_or(base);
            !matches!(base, "Complex" | "Rational")
        }
        _ => false,
    }
}
