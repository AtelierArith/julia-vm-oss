//! Struct-constructor resolution helpers for `compile_call` (Issue #6332).
//!
//! Pure extraction of the constructor-resolution chain that sits between the
//! pre-match special-case handler table and the post-struct handler table in
//! `compile_call`: explicit parametric constructors (`Point{Float64}(...)`,
//! `Dict{K,V}()`, ...), direct `struct_table` constructors, and
//! `resolve_parametric_struct_name`-inferred parametric constructors. Each
//! helper returns `Ok(None)` when its case does not apply so `compile_call`
//! falls through to the next stage, exactly like the original inline blocks.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::borrow::Cow;
use std::collections::HashSet;

use crate::bytecode::{
    CallVarKwargsSplat, Instr, ParametricConstructorCandidate, ParametricConstructorDispatchCall,
    StaticParamBinding, StaticParametricCall, StaticParametricFallback, ValueType,
};
use crate::inference_core::{CoreType, CoreTypeSubstitution, CoreTypeVar};
use crate::ir::core::Expr;
use crate::types::JuliaType;

use crate::compile::method_table::ConstructorSelfFamily;
use crate::compile::{
    parse_parametric_call, CResult, CompileError, CoreCompiler, MethodSig, TypeExpr,
};

use super::{dispatch::should_runtime_dispatch, is_static_val_runtime_expr};

enum DynamicParametricOuterSelection {
    None,
    Unique(usize, Vec<StaticParamBinding>),
    Ambiguous,
}

impl CoreCompiler<'_> {
    pub(super) fn owned_constructor_name_in_scope(&self, function: &str) -> Option<String> {
        let constructor_base = parse_parametric_call(function)
            .map(|(base, _)| base)
            .unwrap_or_else(|| function.to_string());
        if self.locals.contains_key(&constructor_base)
            || self.captured_vars.contains(&constructor_base)
        {
            return None;
        }
        if constructor_base.contains('.')
            && (self.shared_ctx.struct_table.contains_key(&constructor_base)
                || self
                    .shared_ctx
                    .parametric_structs
                    .contains_key(&constructor_base))
        {
            return Some(function.to_string());
        }

        let module_path = self.current_module_path.as_deref()?;
        let qualified_base = format!("{}.{}", module_path, constructor_base);
        (self.shared_ctx.struct_table.contains_key(&qualified_base)
            || self
                .shared_ctx
                .parametric_structs
                .contains_key(&qualified_base))
        .then(|| format!("{}.{}", module_path, function))
    }

    /// Splat-aware fallback for a runtime type-application curly whose base
    /// resolves to a known parametric struct but whose trailing value
    /// parameter is a genuinely runtime expression (a caller `where`-bound
    /// type variable such as `M`/`N`/`T`, or an inline call like
    /// `length(xs)`) — `Foo{M,N,T,n}(xs...)`, splatting the SAME vararg
    /// collection forward into the fully-parameterized constructor.
    ///
    /// `owned_constructor_name_in_scope` only recognizes an explicit
    /// `Module.Base` qualification already present in `function`, or a
    /// `current_module_path` to qualify a bare name against — both `None` for
    /// a struct defined at true top level (`Main`). Even when it does match,
    /// its target `compile_runtime_datatype_value_call` eagerly resolves the
    /// type arguments through `resolve_instantiation_with_type_expr`, which
    /// rejects short-uppercase-name type variables outright (they are not a
    /// literal numeric value parameter it can freeze at compile time). Build
    /// the runtime `DataType` the same way the non-splat dynamic-constructor
    /// path already does (`compile_dynamic_parametric_constructor_method_call`
    /// via `emit_parametric_type_arg_value` + `ApplyTypeDynamic`, which DOES
    /// load a `where`-bound runtime value), then invoke it through the
    /// ordinary splat-aware runtime call convention instead of falling
    /// through to `compile_splat_call`, which mis-resolved the whole curly
    /// text (`Foo{M, N, T, n}`) as an undefined variable name (Issue #11539).
    pub(super) fn try_compile_splat_parametric_constructor_call(
        &mut self,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<Option<ValueType>> {
        let Some((base_name, mut type_args)) = parse_parametric_call(function) else {
            return Ok(None);
        };
        if self.locals.contains_key(&base_name) || self.captured_vars.contains(&base_name) {
            return Ok(None);
        }
        self.resolve_static_typeof_type_args(&mut type_args);
        for type_arg in &mut type_args {
            self.canonicalize_constructor_type_arg_in_scope(type_arg);
        }
        let Some(resolved_base_name) = self
            .resolve_parametric_struct_name(&base_name)
            .or_else(|| self.runtime_nominal_binding_name(&base_name))
            .or_else(|| {
                self.shared_ctx
                    .struct_table
                    .contains_key(&base_name)
                    .then(|| base_name.clone())
            })
        else {
            return Ok(None);
        };

        // Julia evaluates the complete `Base{T...}` callee before its
        // splatted value arguments. Park the runtime DataType in a temp
        // (mirrors `compile_dynamic_parametric_constructor_method_call`),
        // then compile the (possibly splatted) call arguments and invoke the
        // callee value through the same splat-aware runtime call mechanism a
        // plain `Foo(xs...)` forward already uses.
        if let Some(runtime_binding) = self.runtime_nominal_binding_name(&resolved_base_name) {
            self.emit(Instr::ProbeRuntimeBinding(runtime_binding));
        } else {
            self.emit(Instr::PushDataType(resolved_base_name));
        }
        for type_arg in &type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
        }
        self.emit(Instr::ApplyTypeDynamic(type_args.len()));
        let callee_temp = self.new_temp("splat_parametric_constructor_callee");
        self.emit(Instr::StoreAny(callee_temp.clone()));

        for arg in args {
            self.compile_expr(arg)?;
        }
        let has_kwargs = !kwargs.is_empty();
        let has_kwargs_splat = kwargs_splat_mask.iter().any(|&is_splat| is_splat);
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.to_string()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }
        self.emit(Instr::LoadAny(callee_temp));
        if has_kwargs || has_kwargs_splat {
            self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
                CallVarKwargsSplat {
                    arg_count: args.len(),
                    pos_splat_mask: splat_mask.to_vec(),
                    kwarg_names,
                    kwargs_splat_mask: kwargs_splat_mask.to_vec(),
                },
            )));
        } else {
            self.emit(Instr::CallFunctionVariableWithSplat(
                args.len(),
                splat_mask.to_vec(),
            ));
        }
        Ok(Some(ValueType::Any))
    }

    pub(super) fn try_compile_lexical_datatype_parametric_call(
        &mut self,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<Option<ValueType>> {
        let Some((base_name, type_args)) = parse_parametric_call(function) else {
            return Ok(None);
        };
        if self.locals.contains_key(&base_name) || self.captured_vars.contains(&base_name) {
            return self
                .compile_lexical_datatype_parametric_call(
                    &base_name,
                    &type_args,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                )
                .map(Some);
        }
        // A module-level VALUE binding that shadowed an ignored conflicting
        // import (Issue #11426) is a runtime callee, not a static type:
        // apply the type parameters dynamically so Julia's TypeError
        // surfaces catchably instead of the imported type constructing. The
        // module's own types (registered under their qualified name) and
        // type aliases keep their static authority.
        if let Some(qualified_base) = self.conflict_winning_module_value_binding(&base_name) {
            if !self
                .shared_ctx
                .parametric_structs
                .contains_key(&qualified_base)
                && !self.shared_ctx.struct_table.contains_key(&qualified_base)
                && !self.shared_ctx.type_aliases.contains_key(&qualified_base)
                && !crate::compile::expr::is_builtin_type_name(&base_name)
            {
                return self
                    .compile_module_value_datatype_parametric_call(
                        &qualified_base,
                        &type_args,
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                    )
                    .map(Some);
            }
        }
        Ok(None)
    }

    // (public Dict{K,V}(...) calls are routed to Julia methods before this
    // chain; only Dict's internal 8-field constructor reaches this path).
    /// (`Point{Float64}(...)`, `Dict{String, Int}()`, ...). `Ok(None)` =
    /// the name is not a parametric call, or it needs the generic fallback.
    pub(super) fn try_compile_parametric_constructor_call(
        &mut self,
        function: &str,
        args: &[Expr],
    ) -> CResult<Option<ValueType>> {
        let Some((mut base_name, mut type_args)) = parse_parametric_call(function) else {
            return Ok(None);
        };
        self.resolve_static_typeof_type_args(&mut type_args);
        // Preserve the source spelling's runtime/static classification before
        // lexical qualification: a bare module-local nominal such as `V` still
        // needs hierarchy-aware runtime validation after becoming `Owner.V`.
        // Generic MethodTable subtype checks do not carry SharedCompileContext
        // ancestry, so reclassifying it as static would reject `V <: Bound`
        // (Issue #11034).
        let has_runtime_dependent_type_arg = type_args
            .iter()
            .any(|arg| self.parametric_type_arg_requires_runtime(&base_name, arg));
        if !base_name.contains('.') {
            if let Some(module_path) = &self.current_module_path {
                let qualified_base = format!("{module_path}.{base_name}");
                if self
                    .shared_ctx
                    .parametric_structs
                    .contains_key(&qualified_base)
                {
                    base_name = qualified_base;
                }
            }
        }
        for type_arg in &mut type_args {
            self.canonicalize_constructor_type_arg_in_scope(type_arg);
        }
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
        // A caller type variable can occur at any depth (`Foo{Rational{T}}`),
        // not only as a top-level type argument. Treat the whole application
        // as runtime-dependent when any nested component is dynamic; otherwise
        // the compiler freezes the textual `T` into a concrete struct identity.
        if has_runtime_dependent_type_arg {
            let resolved_base_name = self
                .resolve_parametric_struct_name(&base_name)
                .unwrap_or_else(|| base_name.clone());
            let complete_explicit_inner_identity = |method: &MethodSig| {
                !method.explicit_constructor_type_arguments.is_empty()
                    && constructor_method_owner_matches(
                        method,
                        &base_name,
                        Some(&resolved_base_name),
                    )
            };
            let has_complete_explicit_inner_identity = self.method_tables.values().any(|table| {
                table.methods.iter().any(|method| {
                    table.is_explicit_parametric_inner_constructor(method.global_index)
                        && complete_explicit_inner_identity(method)
                })
            });
            let has_declared_inner_constructor = self
                .explicit_parametric_struct_has_inner_constructor(
                    &resolved_base_name,
                    &base_name,
                    &type_args,
                );
            let default_field_call_applicability =
                self.is_fully_applied_default_field_constructor_call(&base_name, &type_args, args);
            let is_default_field_call = default_field_call_applicability == Some(true);

            // A runtime-valued type argument (for example the `F` in
            // `SVector{1,F}(a)`) does not make a source-written outer
            // constructor disappear. When the synthesized field constructor
            // is statically inapplicable, select only among source outers and
            // pass the concrete type-argument values into the selected method's
            // `where` bindings. Otherwise the sole synthetic inner row wins by
            // its projected `(::Any)` signature and attempts an invalid field
            // conversion such as `convert(Tuple, a)` (Issue #11147).
            if !has_declared_inner_constructor && default_field_call_applicability == Some(false) {
                let arg_types: Vec<JuliaType> = args
                    .iter()
                    .map(|arg| self.infer_constructor_arg_julia_type(arg))
                    .collect();
                if let Some((func_index, selected_bindings, validate_argument_types)) = self
                    .static_parametric_constructor_method(&base_name, &type_args, &arg_types, true)
                {
                    let mut bindings = Vec::new();
                    let mut runtime_binding_names = Vec::new();
                    let mut type_arg_temps = Vec::new();
                    for binding in selected_bindings {
                        if !self.parametric_type_arg_is_runtime_binding(&binding.value) {
                            bindings.push(binding);
                            continue;
                        }
                        self.emit_parametric_type_arg_value(&binding.value)?;
                        let temp = self.new_temp("runtime_parametric_type_arg");
                        self.emit(Instr::StoreAny(temp.clone()));
                        type_arg_temps.push(temp);
                        runtime_binding_names.push(binding.name);
                    }
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    for temp in type_arg_temps {
                        self.emit(Instr::LoadAny(temp));
                    }
                    self.emit(Instr::CallStaticParametric(Box::new(
                        StaticParametricCall {
                            func_index,
                            arg_count: args.len(),
                            bindings,
                            forward_caller_type_bindings: true,
                            validate_argument_types,
                            validation_fallback: None,
                            runtime_binding_names,
                        },
                    )));
                    return Ok(Some(ValueType::Any));
                }
            }
            if !has_declared_inner_constructor
                && !has_complete_explicit_inner_identity
                && is_default_field_call
            {
                return self
                    .compile_dynamic_parametric_struct(&base_name, &type_args, args)
                    .map(Some);
            }
            let complete_identity_requires_runtime_bound_validation = type_args
                .iter()
                .any(|arg| self.parametric_type_arg_is_runtime_binding(arg))
                && self.method_tables.values().any(|table| {
                    table.methods.iter().any(|method| {
                        table.is_explicit_parametric_inner_constructor(method.global_index)
                            && complete_explicit_inner_identity(method)
                            && method.core_signature_type_vars().iter().any(|param| {
                                param.upper_bound.is_some() || param.lower_bound.is_some()
                            })
                    })
                });
            if complete_identity_requires_runtime_bound_validation {
                return self
                    .compile_dynamic_parametric_constructor_method_call(
                        &base_name, &type_args, args,
                    )
                    .map(Some);
            }
            if let Some((func_index, bindings, validate_argument_types, validation_fallback)) =
                self.dynamic_parametric_inner_constructor_method(&base_name, &type_args, args)
            {
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(Instr::CallStaticParametric(Box::new(
                    StaticParametricCall {
                        func_index,
                        arg_count: args.len(),
                        bindings,
                        forward_caller_type_bindings: true,
                        validate_argument_types,
                        validation_fallback,
                        runtime_binding_names: Vec::new(),
                    },
                )));
                return Ok(Some(ValueType::Any));
            }
            // The type argument is an arbitrary runtime expression
            // (`Foo{typeof(x)}(x)`) or a local `DataType` variable, so it cannot
            // be serialized as a literal `StaticParamBinding`. A struct with an
            // inner constructor has no default field constructor, so the raw
            // dynamic allocator at the end of this chain would bypass the
            // declared constructor entirely (skipping its field transformation
            // and its `where` bound). Select the inner method statically and
            // bind its `where` binders from the runtime type-argument values
            // instead (Issue #10998).
            if let Some((func_index, runtime_binding_names)) = self
                .runtime_bound_parametric_inner_constructor_method(
                    &base_name,
                    &type_args,
                    args.len(),
                )
            {
                // Evaluate the complete `Foo{T...}` callee first, as Julia
                // does, but reload the runtime type bindings after positional
                // arguments to retain CallStaticParametric's
                // [args..., bindings...] stack layout (Issue #11375).
                let mut type_arg_temps = Vec::with_capacity(type_args.len());
                for type_arg in &type_args {
                    self.emit_parametric_type_arg_value(type_arg)?;
                    let temp = self.new_temp("runtime_parametric_type_arg");
                    self.emit(Instr::StoreAny(temp.clone()));
                    type_arg_temps.push(temp);
                }
                for arg in args {
                    self.compile_expr(arg)?;
                }
                for temp in type_arg_temps {
                    self.emit(Instr::LoadAny(temp));
                }
                self.emit(Instr::CallStaticParametric(Box::new(
                    StaticParametricCall {
                        func_index,
                        arg_count: args.len(),
                        bindings: Vec::new(),
                        forward_caller_type_bindings: true,
                        validate_argument_types: true,
                        validation_fallback: None,
                        runtime_binding_names,
                    },
                )));
                return Ok(Some(ValueType::Any));
            }
            if has_complete_explicit_inner_identity {
                return self
                    .compile_dynamic_parametric_constructor_method_call(
                        &base_name, &type_args, args,
                    )
                    .map(Some);
            }
            if self.multiple_forwardable_parametric_inner_candidates_are_ambiguous(
                &base_name,
                &type_args,
                args.len(),
            ) {
                let arg_types: Vec<JuliaType> = args
                    .iter()
                    .map(|arg| self.infer_constructor_arg_julia_type(arg))
                    .collect();
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
                        .map(|ty| format!("::{ty}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
                return Ok(Some(ValueType::Any));
            }
            // Use dynamic parametric struct construction. Forwardable sole
            // inners have already been retained above; arbitrary runtime type
            // expressions still need this raw dynamic-instantiation path.
            return self
                .compile_dynamic_parametric_struct(&base_name, &type_args, args)
                .map(Some);
        }
        if !has_runtime_dependent_type_arg {
            let resolved_base_name = self
                .resolve_parametric_struct_name(&base_name)
                .unwrap_or_else(|| base_name.clone());
            let has_declared_inner_constructor = self
                .explicit_parametric_struct_has_inner_constructor(
                    &resolved_base_name,
                    &base_name,
                    &type_args,
                );
            let default_field_call_applicability =
                self.is_fully_applied_default_field_constructor_call(&base_name, &type_args, args);
            // A source-written explicit parametric outer (registered under its
            // `Base{T}`-shaped table) participates in dispatch even when the
            // call also looks like the automatic field constructor: upstream
            // lets `ExplicitOuterGap{T}(x::T) where {T} = ...` replace the
            // synthetic default inner, so the fully-applied fast path must not
            // hide it (Issue #11404).
            let has_source_explicit_parametric_outer =
                self.method_tables.iter().any(|(table_name, table)| {
                    parse_parametric_call(table_name).is_some_and(|(table_base, _)| {
                        same_parametric_constructor_base(
                            &table_base,
                            &base_name,
                            Some(&resolved_base_name),
                        )
                    }) && table
                        .methods
                        .iter()
                        .any(|method| method.accepts_arity(args.len()))
                });
            // The applicable default field constructor participates in
            // dispatch alongside the source outers, so only a CONFIDENT
            // static match may displace it: the resolver's single-candidate
            // fallback (validate_argument_types = true) would force an
            // unrelated-signature source method — e.g. `RotMatrix{2,
            // Float64}(::SMatrix)` must stay on the default field lane even
            // though a `RotMatrix{N,T}(::Number)` outer exists (Issue #11404).
            let entered_for_source_outer = default_field_call_applicability == Some(true)
                && !has_declared_inner_constructor
                && has_source_explicit_parametric_outer;
            if default_field_call_applicability != Some(true)
                || has_declared_inner_constructor
                || has_source_explicit_parametric_outer
            {
                let arg_types: Vec<JuliaType> = args
                    .iter()
                    .map(|arg| self.infer_constructor_arg_julia_type(arg))
                    .collect();
                if let Some((func_index, bindings, validate_argument_types)) = self
                    .static_parametric_constructor_method(
                        &base_name,
                        &type_args,
                        &arg_types,
                        !has_declared_inner_constructor
                            && default_field_call_applicability == Some(false),
                    )
                    .filter(|(_, _, validate_argument_types)| {
                        // A runtime-unknown argument cannot confidently select
                        // a source outer over the applicable default field
                        // constructor either: `table.dispatch` treats `Any` as
                        // compatible with every row, so an SMatrix-valued
                        // argument inferred as `Any` would "match" the
                        // `(::Number)` outer (Issue #11404).
                        !(entered_for_source_outer
                            && (*validate_argument_types
                                || arg_types.iter().any(|ty| matches!(ty, JuliaType::Any))))
                    })
                {
                    for arg in args {
                        self.compile_expr(arg)?;
                    }
                    self.emit(Instr::CallStaticParametric(Box::new(
                        StaticParametricCall {
                            func_index,
                            arg_count: args.len(),
                            bindings,
                            forward_caller_type_bindings: false,
                            validate_argument_types,
                            validation_fallback: None,
                            runtime_binding_names: Vec::new(),
                        },
                    )));
                    return Ok(Some(ValueType::Any));
                }
            }

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
            let method_table_names = self.parametric_constructor_method_table_names(&base_name);

            // Find inner constructors only (those with type_params, not outer constructors).
            let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
            let mut saw_arity_matched_inner_ctor = false;
            for method_table_name in method_table_names {
                let Some(table) = self.method_tables.get(method_table_name.as_ref()) else {
                    continue;
                };
                let complete_identity_methods: Vec<_> = table
                    .methods
                    .iter()
                    .filter(|method| {
                        table.is_explicit_parametric_inner_constructor(method.global_index)
                            && !method.explicit_constructor_type_arguments.is_empty()
                    })
                    .collect();
                if !complete_identity_methods.is_empty() {
                    let mut candidates = Vec::new();
                    let mut candidate_bindings = Vec::new();
                    for method in complete_identity_methods {
                        if !method.accepts_arity(arg_types.len())
                            || !constructor_method_owner_matches(
                                method,
                                &base_name,
                                Some(&resolved_base_name),
                            )
                        {
                            continue;
                        }
                        let Some(bindings) =
                            explicit_inner_constructor_bindings(method, &type_args)
                        else {
                            continue;
                        };
                        if !static_param_bindings_satisfy_bounds(self.shared_ctx, method, &bindings)
                        {
                            continue;
                        }
                        saw_arity_matched_inner_ctor = true;
                        candidates.push(instantiate_constructor_dispatch_method(method, &bindings));
                        candidate_bindings.push((method.global_index, bindings));
                    }
                    let eligible = table.clone_with_methods_for_compile(candidates);
                    let selected = eligible
                        .dispatch(&arg_types)
                        .ok()
                        .map(|method| (method, false))
                        .or_else(|| {
                            (eligible.methods.len() == 1
                                && constructor_single_candidate_fallback_allowed(&arg_types))
                            .then(|| (&eligible.methods[0], true))
                        });
                    if let Some((method, validate_argument_types)) = selected {
                        let global_index = method.global_index;
                        let bindings = candidate_bindings
                            .into_iter()
                            .find_map(|(index, bindings)| {
                                (index == global_index).then_some(bindings)
                            })
                            .unwrap_or_default();
                        for arg in args {
                            self.compile_expr(arg)?;
                        }
                        self.emit(Instr::CallStaticParametric(Box::new(
                            StaticParametricCall {
                                func_index: global_index,
                                arg_count: args.len(),
                                bindings,
                                forward_caller_type_bindings: false,
                                validate_argument_types,
                                validation_fallback: None,
                                // This call site resolves its type arguments statically
                                // (`bindings` above), so there are no RUNTIME type-argument
                                // binders to push (Issue #10998's `Foo{typeof(x)}(x)` path).
                                runtime_binding_names: Vec::new(),
                            },
                        )));
                        let resolved_type_id =
                            self.shared_ctx.resolve_instantiation_with_type_expr(
                                &resolved_base_name,
                                &type_args,
                            )?;
                        return Ok(Some(ValueType::Struct(resolved_type_id)));
                    }
                    // Issue #10971: the static dispatcher above found no unique
                    // winner. When a runtime-unknown value argument (`Any`/
                    // `TypeVar`) leaves more than one bound-satisfying candidate,
                    // that is genuine ambiguity a compile-time MethodError would
                    // wrongly foreclose — the runtime argument VALUE still
                    // determines a unique method. Route through runtime candidate
                    // selection by value signature, installing each candidate's
                    // own resolved `where` bindings (already computed above from
                    // the static explicit type arguments) into whichever frame
                    // gets selected.
                    if eligible.methods.len() > 1
                        && arg_types
                            .iter()
                            .any(|ty| matches!(ty, JuliaType::Any | JuliaType::TypeVar(_, _)))
                    {
                        let candidates: Vec<ParametricConstructorCandidate> = eligible
                            .methods
                            .iter()
                            .filter_map(|method| {
                                candidate_bindings.iter().find_map(|(idx, bindings)| {
                                    (*idx == method.global_index).then(|| {
                                        ParametricConstructorCandidate {
                                            func_index: method.global_index,
                                            bindings: bindings.clone(),
                                            runtime_binding_names: Vec::new(),
                                        }
                                    })
                                })
                            })
                            .collect();
                        if candidates.len() > 1 {
                            for arg in args {
                                self.compile_expr(arg)?;
                            }
                            self.emit(Instr::CallParametricConstructorDispatch(Box::new(
                                ParametricConstructorDispatchCall {
                                    base_name: base_name.clone(),
                                    arg_count: args.len(),
                                    type_arg_value_count: 0,
                                    candidates,
                                },
                            )));
                            let resolved_type_id =
                                self.shared_ctx.resolve_instantiation_with_type_expr(
                                    &resolved_base_name,
                                    &type_args,
                                )?;
                            return Ok(Some(ValueType::Struct(resolved_type_id)));
                        }
                    }
                    continue;
                }
                // The table's own serialized constructor-self-family carrier
                // is the sole source of truth for which methods are this
                // struct's explicit-parametric (`Foo{T}(...)`) inner
                // constructors (Issue #10959, #10962, #10974). An outer
                // constructor may itself have `where` parameters and an
                // identical projected value signature, so origin — never a
                // `has_where_params()` guess — is what distinguishes the
                // implicit `Type{Foo}` / `Type{Foo{T}}` self arguments that
                // sjulia's table omits. The carrier is part of MethodTable
                // itself, so it is populated identically whether the table
                // was just built from source or restored from a serialized
                // Base cache — there is no separate fallback for either case.
                let allowed: HashSet<usize> = table
                    .methods
                    .iter()
                    .filter(|method| {
                        table.is_explicit_parametric_inner_constructor(method.global_index)
                    })
                    .filter(|method| {
                        if method.accepts_arity(arg_types.len())
                            && method.core_signature_type_vars().len() == type_args.len()
                            && self.explicit_type_args_satisfy_inner_ctor_bounds(method, &type_args)
                        {
                            saw_arity_matched_inner_ctor = true;
                        }
                        self.explicit_type_args_satisfy_inner_ctor_bounds(method, &type_args)
                    })
                    .map(|method| method.global_index)
                    .collect();
                let explicit_type_args: Vec<JuliaType> = type_args
                    .iter()
                    .map(TypeExpr::to_julia_type_lossy)
                    .collect();
                let inner_ctor_match = table
                    .dispatch_among_global_indices_with_positional_type_args(
                        &arg_types,
                        &allowed,
                        &explicit_type_args,
                    )
                    .ok();
                let selected_inner_ctor = inner_ctor_match
                    .map(|method| {
                        (
                            method.global_index,
                            method.core_signature_type_vars(),
                            false,
                        )
                    })
                    .or_else(|| {
                        // A broad compile-time argument type does not prove
                        // that a sole constructor candidate is inapplicable.
                        // Route that one candidate through the existing
                        // runtime signature validator so `Channel{Any}(sz)`
                        // accepts an Integer runtime value while preserving a
                        // MethodError for an actual mismatch. More than one
                        // compatible candidate needs runtime candidate-specific
                        // dispatch (#10971), so it remains fail-closed below.
                        if !arg_types
                            .iter()
                            .any(|ty| matches!(ty, JuliaType::Any | JuliaType::TypeVar(_, _)))
                        {
                            return None;
                        }
                        let mut candidates = table.methods.iter().filter(|method| {
                            allowed.contains(&method.global_index)
                                && method.accepts_arity(args.len())
                                && method.core_signature_type_vars().len() == type_args.len()
                        });
                        let method = candidates.next()?;
                        if candidates.next().is_some() {
                            return None;
                        }
                        Some((method.global_index, method.core_signature_type_vars(), true))
                    });
                if let Some((global_index, type_vars, validate_argument_types)) =
                    selected_inner_ctor
                {
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
                    if bindings.is_empty() && !validate_argument_types {
                        self.emit(Instr::Call(global_index, args.len()));
                    } else {
                        self.emit(Instr::CallStaticParametric(Box::new(
                            StaticParametricCall {
                                func_index: global_index,
                                arg_count: args.len(),
                                bindings,
                                forward_caller_type_bindings: false,
                                validate_argument_types,
                                validation_fallback: None,
                                runtime_binding_names: Vec::new(),
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
            }
            let user_declared_inner_constructor = self
                .shared_ctx
                .parametric_structs
                .get(&resolved_base_name)
                .or_else(|| self.shared_ctx.parametric_structs.get(&base_name))
                .is_some_and(|parametric| {
                    !parametric.def.is_base_origin && !parametric.def.inner_constructors.is_empty()
                });
            if saw_arity_matched_inner_ctor || user_declared_inner_constructor {
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
            // If no inner constructor found, fall through to default constructor.
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
            if let Some(value_type) =
                self.try_compile_zero_field_instantiated_constructor(type_id, args)
            {
                return Ok(Some(value_type));
            }
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
            if let Some(value_type) =
                self.try_compile_zero_field_instantiated_constructor(type_id, args)
            {
                return Ok(Some(value_type));
            }
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

    fn try_compile_zero_field_instantiated_constructor(
        &mut self,
        type_id: usize,
        args: &[Expr],
    ) -> Option<ValueType> {
        if !args.is_empty() {
            return None;
        }

        let is_zero_field = self
            .shared_ctx
            .type_id_to_struct_name
            .get(&type_id)
            .and_then(|name| self.shared_ctx.struct_name_to_def_index.get(name))
            .and_then(|idx| self.shared_ctx.struct_defs.get(*idx))
            .or_else(|| self.shared_ctx.struct_defs.get(type_id))
            .is_some_and(|def| def.fields.is_empty());
        if !is_zero_field {
            return None;
        }

        // Some cached value-parameter instantiations can be known by type_id
        // before this call path has a fresh struct_table row. A zero-field
        // constructor needs no field coercion, so emit the same direct NewStruct
        // form used when the expression is bound first (Issue #9401).
        self.emit(Instr::NewStruct(type_id, 0));
        Some(ValueType::Struct(type_id))
    }

    fn is_fully_applied_default_field_constructor_call(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> Option<bool> {
        let Some(parametric_def) =
            self.shared_ctx
                .parametric_structs
                .get(base_name)
                .or_else(|| {
                    self.resolve_parametric_struct_name(base_name)
                        .and_then(|resolved| self.shared_ctx.parametric_structs.get(&resolved))
                })
        else {
            // Method-table-only callers do not prove that a synthesized field
            // constructor exists. Keep this distinct from a known definition
            // whose field constructor is inapplicable (Issues #10969/#10971).
            return None;
        };
        let arity_matches = type_args.len() == parametric_def.def.type_params.len()
            && args.len() == parametric_def.def.fields.len();
        // Release the immutable `shared_ctx` borrow before the `&mut self` check.
        if !arity_matches {
            return Some(false);
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
        Some(self.parametric_default_ctor_args_convertible(base_name, type_args, args))
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
        let declared_field_jts: Vec<Option<JuliaType>> = self
            .shared_ctx
            .parametric_structs
            .get(&resolved)
            .or_else(|| self.shared_ctx.parametric_structs.get(base_name))
            .map(|parametric| {
                parametric
                    .def
                    .fields
                    .iter()
                    .map(|field| field.as_julia_type())
                    .collect()
            })
            .unwrap_or_default();
        let field_jts: Vec<Option<JuliaType>> = self
            .shared_ctx
            .resolve_instantiation_with_type_expr(&resolved, type_args)
            .ok()
            .and_then(|type_id| self.shared_ctx.field_julia_types_by_type_id(type_id))
            .map(|field_types| field_types.iter().cloned().map(Some).collect())
            .unwrap_or(declared_field_jts);
        if field_jts.len() != args.len() {
            return true;
        }
        for (arg, field_jt) in args.iter().zip(field_jts.iter()) {
            let Some(field_jt) = field_jt else {
                continue;
            };
            // The field's JuliaType is authoritative. A scalar argument is NOT
            // convertible to a non-numeric container/struct field — but the lossy
            // ValueType collapses an unregistered concrete struct (e.g.
            // `SMatrix{2,2,Float32}`, the `RotMatrix` field) to `Any`, which then
            // "accepts" any numeric scalar. Reject that directly off the
            // JuliaType so the typed outer constructor wins (#8103).
            let arg_jt = self.infer_constructor_arg_julia_type(arg);
            if !matches!(arg_jt, JuliaType::Any) && arg_jt.is_subtype_of(field_jt) {
                continue;
            }
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

    fn resolve_static_typeof_type_args(&self, type_args: &mut [TypeExpr]) {
        for type_arg in type_args {
            match type_arg {
                TypeExpr::RuntimeExpr(expr) => {
                    if let Some(julia_type) = self.static_typeof_runtime_expr(expr) {
                        *type_arg = TypeExpr::Concrete(julia_type);
                    }
                }
                TypeExpr::Parameterized { params, .. } => {
                    self.resolve_static_typeof_type_args(params);
                }
                TypeExpr::Concrete(_) | TypeExpr::TypeVar(_) => {}
            }
        }
    }

    fn canonicalize_constructor_type_arg_in_scope(&self, type_arg: &mut TypeExpr) {
        match type_arg {
            TypeExpr::TypeVar(name) => {
                if self.current_type_param_index.contains_key(name.as_str())
                    || self.locals.contains_key(name)
                {
                    return;
                }
                if let Some(resolved) = self.resolve_visible_type_object_name(name) {
                    *name = resolved;
                }
            }
            TypeExpr::Parameterized { base, params } => {
                if !self.current_type_param_index.contains_key(base.as_str())
                    && !self.locals.contains_key(base)
                {
                    if let Some(resolved) = self.resolve_visible_type_object_name(base) {
                        *base = resolved;
                    }
                }
                for param in params {
                    self.canonicalize_constructor_type_arg_in_scope(param);
                }
            }
            TypeExpr::Concrete(_) | TypeExpr::RuntimeExpr(_) => {}
        }
    }

    fn static_typeof_runtime_expr(&self, expr: &str) -> Option<JuliaType> {
        let name = expr.strip_prefix("typeof(")?.strip_suffix(')')?.trim();
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return None;
        }
        if self.abstract_numeric_params.contains(name) {
            return None;
        }
        let julia_type = self.value_type_to_julia_type(self.locals.get(name)?);
        (!matches!(julia_type, JuliaType::Any | JuliaType::DataType)).then_some(julia_type)
    }

    fn explicit_parametric_struct_has_inner_constructor(
        &mut self,
        resolved_base_name: &str,
        base_name: &str,
        type_args: &[TypeExpr],
    ) -> bool {
        self.shared_ctx
            .resolve_instantiation_with_type_expr(resolved_base_name, type_args)
            .ok()
            .and_then(|type_id| {
                self.shared_ctx
                    .struct_table
                    .values()
                    .find(|struct_info| struct_info.type_id == type_id)
                    .map(|struct_info| struct_info.has_inner_constructor)
            })
            .unwrap_or_else(|| {
                self.shared_ctx
                    .parametric_structs
                    .get(resolved_base_name)
                    .or_else(|| self.shared_ctx.parametric_structs.get(base_name))
                    .is_some_and(|parametric| !parametric.def.inner_constructors.is_empty())
            })
    }

    fn static_parametric_constructor_method(
        &self,
        base_name: &str,
        type_args: &[TypeExpr],
        arg_types: &[JuliaType],
        default_field_call_is_inapplicable: bool,
    ) -> Option<(usize, Vec<StaticParamBinding>, bool)> {
        let resolved_base_name = self.resolve_parametric_struct_name(base_name);
        let has_complete_identity = self.method_tables.values().any(|table| {
            table.methods.iter().any(|method| {
                table.is_explicit_parametric_inner_constructor(method.global_index)
                    && !method.explicit_constructor_type_arguments.is_empty()
                    && constructor_method_owner_matches(
                        method,
                        base_name,
                        resolved_base_name.as_deref(),
                    )
            })
        });
        if !has_complete_identity {
            return self
                .legacy_static_parametric_constructor_method(
                    base_name,
                    resolved_base_name.as_deref(),
                    type_args,
                    arg_types.len(),
                )
                .map(|(global_index, bindings)| (global_index, bindings, false));
        }

        let mut candidates = Vec::new();
        let mut candidate_bindings = Vec::new();
        let mut source_fixed_outer_candidates = Vec::new();
        let mut source_fixed_outer_bindings = Vec::new();
        let mut projection_table = None;
        let mut matching_tables: Vec<(&String, _, Vec<TypeExpr>)> = self
            .method_tables
            .iter()
            .filter_map(|(table_name, table)| {
                let (table_base, pattern_args) = parse_parametric_call(table_name)?;
                same_parametric_constructor_base(
                    &table_base,
                    base_name,
                    resolved_base_name.as_deref(),
                )
                .then_some((table_name, table, pattern_args))
            })
            .collect();
        matching_tables.sort_by_key(|(table_name, _, _)| table_name.as_str());
        for (_, table, pattern_args) in matching_tables {
            if pattern_args.len() != type_args.len() {
                continue;
            }
            projection_table.get_or_insert(table);
            for method in table
                .methods
                .iter()
                .filter(|method| method.accepts_arity(arg_types.len()))
            {
                let binder_names: Vec<String> = method
                    .core_signature_type_vars()
                    .iter()
                    .map(|var| var.name.clone())
                    .collect();
                let Some(bindings) =
                    constructor_pattern_bindings(&pattern_args, type_args, &binder_names)
                else {
                    continue;
                };
                if !static_param_bindings_satisfy_bounds(self.shared_ctx, method, &bindings) {
                    continue;
                }
                let instantiated = instantiate_constructor_dispatch_method(method, &bindings);
                if method.vararg_param_index.is_none() {
                    source_fixed_outer_candidates.push(instantiated.clone());
                    source_fixed_outer_bindings.push((method.global_index, bindings.clone()));
                }
                candidates.push(instantiated);
                candidate_bindings.push((method.global_index, bindings));
            }
        }

        // The generated default inner has a fixed `(::Any, ...)` value
        // signature. Julia still lets an applicable source-written fixed-arity
        // explicit constructor win by its implicit callable-self and/or value
        // signature (for example `SVector{2,T}(::AbstractVector)`). Sjulia's
        // projected method rows omit that callable-self, so dispatching it
        // together with a generic source vararg and the generated inner makes
        // the fixed `Any` row win incorrectly. For structs without a declared
        // user inner, select only applicable source fixed-arity rows first;
        // try source fixed methods before source varargs (Issue #11147).
        if default_field_call_is_inapplicable && !source_fixed_outer_candidates.is_empty() {
            let table = projection_table?;
            let eligible = table.clone_with_methods_for_compile(source_fixed_outer_candidates);
            // A rank-erased native `Array` may satisfy `AbstractVector` or
            // `AbstractMatrix` only once the runtime dimensions are visible.
            // Keep this deferral local to a sole fixed source outer: widening
            // the shared single-candidate fallback would make unrelated
            // mismatched constructor signatures look applicable.
            let array_imprecision_fallback = eligible.methods.first().is_some_and(|method| {
                if eligible.methods.len() != 1 {
                    return false;
                }
                let params = method.projected_param_julia_types();
                let mut saw_rank_erased_array = false;
                let all_compatible = params.len() == arg_types.len()
                    && arg_types.iter().zip(params.iter()).all(|(actual, param)| {
                        if matches!(actual, JuliaType::Array) {
                            let accepts_runtime_array_rank = match param {
                                JuliaType::AbstractArray => true,
                                JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
                                    matches!(
                                        name.rsplit('.').next(),
                                        Some("AbstractArray" | "AbstractVector" | "AbstractMatrix")
                                    )
                                }
                                _ => false,
                            };
                            saw_rank_erased_array |= accepts_runtime_array_rank;
                            accepts_runtime_array_rank
                        } else {
                            matches!(actual, JuliaType::Any) || actual.is_subtype_of(param)
                        }
                    });
                saw_rank_erased_array && all_compatible
            });
            let selected = eligible
                .dispatch(arg_types)
                .ok()
                .map(|method| (method, false))
                .or_else(|| {
                    (eligible.methods.len() == 1
                        && (constructor_single_candidate_fallback_allowed(arg_types)
                            || array_imprecision_fallback))
                        .then(|| (&eligible.methods[0], true))
                });
            if let Some((method, validate_argument_types)) = selected {
                let bindings =
                    source_fixed_outer_bindings
                        .into_iter()
                        .find_map(|(index, bindings)| {
                            (index == method.global_index).then_some(bindings)
                        })?;
                return Some((method.global_index, bindings, validate_argument_types));
            }
        }

        // If the synthesized default field constructor cannot accept these
        // argument types, none of its synthetic rows is an eligible fallback.
        // Dispatch across the source-written outer methods only, including
        // varargs such as `SVector{N,T}(xs...)`; mixing the fixed synthetic
        // `(::Any)` row back in would incorrectly select it for a scalar and
        // attempt `convert(Tuple, scalar)` instead of calling the source outer.
        // If no source outer applies, report no static constructor match and
        // let the caller continue to the ordinary MethodError path (Issue
        // #11147).
        if default_field_call_is_inapplicable {
            let table = projection_table?;
            let eligible = table.clone_with_methods_for_compile(candidates);
            let (method, validate_argument_types) = eligible
                .dispatch(arg_types)
                .ok()
                .map(|method| (method, false))
                .or_else(|| {
                    (eligible.methods.len() == 1
                        && constructor_single_candidate_fallback_allowed(arg_types))
                    .then(|| (&eligible.methods[0], true))
                })?;
            let bindings = candidate_bindings
                .into_iter()
                .find_map(|(index, bindings)| (index == method.global_index).then_some(bindings))?;
            return Some((method.global_index, bindings, validate_argument_types));
        }

        let short_name = short_constructor_name(base_name);
        let mut base_table_names = vec![base_name];
        if let Some(resolved) = resolved_base_name.as_deref() {
            if !base_table_names.contains(&resolved) {
                base_table_names.push(resolved);
            }
        }
        if !base_table_names.contains(&short_name.as_ref()) {
            base_table_names.push(short_name.as_ref());
        }
        for table_name in base_table_names {
            let Some(table) = self.method_tables.get(table_name) else {
                continue;
            };
            projection_table.get_or_insert(table);
            for method in table.methods.iter().filter(|method| {
                table.is_explicit_parametric_inner_constructor(method.global_index)
                    && method.accepts_arity(arg_types.len())
                    && constructor_method_owner_matches(
                        method,
                        base_name,
                        resolved_base_name.as_deref(),
                    )
            }) {
                let Some(bindings) = explicit_inner_constructor_bindings(method, type_args) else {
                    continue;
                };
                if !static_param_bindings_satisfy_bounds(self.shared_ctx, method, &bindings) {
                    continue;
                }
                candidates.push(instantiate_constructor_dispatch_method(method, &bindings));
                candidate_bindings.push((method.global_index, bindings));
            }
        }

        let table = projection_table?;
        let eligible = table.clone_with_methods_for_compile(candidates);
        let (method, validate_argument_types) = eligible
            .dispatch(arg_types)
            .ok()
            .map(|method| (method, false))
            .or_else(|| {
                (eligible.methods.len() == 1
                    && constructor_single_candidate_fallback_allowed(arg_types))
                .then(|| (&eligible.methods[0], true))
            })?;
        let bindings = candidate_bindings
            .into_iter()
            .find_map(|(index, bindings)| (index == method.global_index).then_some(bindings))?;
        Some((method.global_index, bindings, validate_argument_types))
    }

    fn legacy_static_parametric_constructor_method(
        &self,
        base_name: &str,
        resolved_base_name: Option<&str>,
        type_args: &[TypeExpr],
        arg_count: usize,
    ) -> Option<(usize, Vec<StaticParamBinding>)> {
        // `method_tables` is a HashMap: its iteration order is seed-dependent,
        // and this scan returns the FIRST matching constructor method. Sort
        // the matching parametric-constructor tables by name so the selection
        // is deterministic across processes (Issue #8658).
        let mut matching_tables: Vec<(&String, _, Vec<TypeExpr>)> = self
            .method_tables
            .iter()
            .filter_map(|(table_name, table)| {
                let (table_base, pattern_args) = parse_parametric_call(table_name)?;
                if !same_parametric_constructor_base(&table_base, base_name, resolved_base_name) {
                    return None;
                }
                Some((table_name, table, pattern_args))
            })
            .collect();
        matching_tables.sort_by_key(|(table_name, _, _)| table_name.as_str());
        for (_, table, pattern_args) in matching_tables {
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

            // `table` was found via `parse_parametric_call` on its OWN
            // literal key (e.g. "Boxed8103{N,T}", "Rational{T}") — i.e. it is
            // a sibling table keyed by an explicit-parametric self spelling,
            // distinct from the struct's bare-named table where inner
            // constructors register (`add_inner_constructor_method` always
            // targets `struct_def.name`, never a "{...}"-suffixed key). Every
            // method here is therefore already scoped to the explicit
            // `Type{Foo{T}}` self family by table membership alone — no
            // additional origin filter (`is_explicit_parametric_inner_constructor`
            // or the former `has_where_params()`) is needed or correct here;
            // adding one previously rejected legitimate user-declared OUTER
            // constructors such as `Boxed8103{N,T}(x::Number) where {N,T}`
            // that only ever live in this sibling table (Issue #8103
            // regression found while migrating to the persisted origin
            // carrier — Issue #10962, #10974). The `bindings` check below
            // already requires every needed static parameter to appear among
            // the method's own `where` type vars, which is what actually
            // needs verifying here.
            if let Some(method) = table.methods.iter().find(|method| {
                method.accepts_arity(arg_count) && {
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

    /// Resolve an explicit parametric inner call whose type arguments are
    /// runtime values, such as `Foo{T}(...)` inside `where T` outer method.
    /// The ordinary dynamic-struct path is a raw allocator for field-count
    /// calls, which is invalid when `Foo` declares an inner constructor.
    fn dynamic_parametric_inner_constructor_method(
        &self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> Option<(
        usize,
        Vec<StaticParamBinding>,
        bool,
        Option<StaticParametricFallback>,
    )> {
        // `CallStaticParametric` can forward caller `where` bindings, but it
        // cannot evaluate arbitrary runtime type expressions. Keep those (and
        // ordinary local DataType values) on the dynamic type-application path
        // instead of serializing their source text as a literal binding.
        if !type_args
            .iter()
            .all(|arg| self.parametric_binding_is_statically_forwardable(arg))
        {
            return None;
        }
        let table_names = self.parametric_constructor_method_table_names(base_name);

        let arg_types: Vec<JuliaType> = args
            .iter()
            .map(|arg| self.infer_constructor_arg_julia_type(arg))
            .collect();
        let explicit_type_args: Vec<JuliaType> = type_args
            .iter()
            .map(TypeExpr::to_julia_type_lossy)
            .collect();
        let mut has_registered_explicit_inner = false;
        let mut compatible_inner_fallbacks = Vec::new();
        for table_name in table_names {
            let Some(table) = self.method_tables.get(table_name.as_ref()) else {
                continue;
            };
            has_registered_explicit_inner |= table.has_explicit_parametric_inner_constructors();
            let allowed: HashSet<usize> = table
                .methods
                .iter()
                .filter(|method| {
                    table.is_explicit_parametric_inner_constructor(method.global_index)
                })
                .filter(|method| {
                    self.explicit_type_args_satisfy_inner_ctor_bounds(method, type_args)
                })
                .map(|method| method.global_index)
                .collect();
            for method in table.methods.iter().filter(|method| {
                allowed.contains(&method.global_index) && method.accepts_arity(args.len())
            }) {
                let type_vars = method.core_signature_type_vars();
                if type_vars.len() != type_args.len()
                    || compatible_inner_fallbacks
                        .iter()
                        .any(|(global_index, _)| *global_index == method.global_index)
                {
                    continue;
                }
                let bindings = type_vars
                    .iter()
                    .zip(type_args.iter())
                    .map(|(var, value)| StaticParamBinding {
                        name: var.name.clone(),
                        value: value.clone(),
                    })
                    .collect();
                compatible_inner_fallbacks.push((method.global_index, bindings));
            }
            let Ok(method) = table.dispatch_among_global_indices_with_positional_type_args(
                &arg_types,
                &allowed,
                &explicit_type_args,
            ) else {
                continue;
            };
            let type_vars = method.core_signature_type_vars();
            if type_vars.len() != type_args.len() {
                continue;
            }
            let bindings = type_vars
                .iter()
                .zip(type_args.iter())
                .map(|(var, value)| StaticParamBinding {
                    name: var.name.clone(),
                    value: value.clone(),
                })
                .collect();
            return Some((method.global_index, bindings, false, None));
        }
        // Runtime candidate-specific dispatch is tracked by #10971. Selecting
        // an outer here would silently bypass multiple explicit inners whose
        // value signatures cannot be distinguished statically. Fail closed;
        // the caller emits MethodError through the unresolved-forwardable gate.
        if compatible_inner_fallbacks.len() > 1 {
            return None;
        }
        let sole_inner = (compatible_inner_fallbacks.len() == 1)
            .then(|| compatible_inner_fallbacks.pop())
            .flatten();
        match self.single_dynamic_parametric_outer_constructor_method(
            base_name,
            type_args,
            args.len(),
        ) {
            DynamicParametricOuterSelection::Unique(global_index, bindings) => {
                if let Some((inner_index, inner_bindings)) = sole_inner {
                    return Some((
                        inner_index,
                        inner_bindings,
                        true,
                        Some(StaticParametricFallback {
                            func_index: global_index,
                            bindings,
                        }),
                    ));
                }
                return Some((global_index, bindings, true, None));
            }
            DynamicParametricOuterSelection::Ambiguous => {
                // Runtime DataType dispatch cannot see the bare-table inner
                // row. Keep the sole inner and validate it at runtime; if it
                // rejects the value we fail closed rather than silently
                // choosing one of several outer candidates (#10971).
                return sole_inner
                    .map(|(global_index, bindings)| (global_index, bindings, true, None));
            }
            DynamicParametricOuterSelection::None => {}
        }
        if !has_registered_explicit_inner {
            return None;
        }
        sole_inner.map(|(global_index, bindings)| (global_index, bindings, true, None))
    }

    /// Select the explicit-parametric inner constructor for a call whose type
    /// arguments are only known at runtime (`Foo{typeof(x)}(x)`, `Foo{t}(x)`),
    /// returning its global index and the `where` binder names to bind from the
    /// runtime type-argument values (Issue #10998).
    ///
    /// Only a *sole* arity-compatible inner method is selected: with the type
    /// arguments unknown, competing inners that differ only by their `where`
    /// bounds cannot be told apart statically (that needs the runtime
    /// candidate-set dispatch tracked by #10971), so this fails closed and the
    /// caller keeps its existing behavior.
    fn runtime_bound_parametric_inner_constructor_method(
        &self,
        base_name: &str,
        type_args: &[TypeExpr],
        arg_count: usize,
    ) -> Option<(usize, Vec<String>)> {
        let mut selected: Option<(usize, Vec<String>)> = None;
        for table_name in self.parametric_constructor_method_table_names(base_name) {
            let Some(table) = self.method_tables.get(table_name.as_ref()) else {
                continue;
            };
            for method in table.methods.iter().filter(|method| {
                table.is_explicit_parametric_inner_constructor(method.global_index)
                    && method.accepts_arity(arg_count)
            }) {
                let type_vars = method.core_signature_type_vars();
                if type_vars.len() != type_args.len() {
                    continue;
                }
                let names: Vec<String> = type_vars.iter().map(|var| var.name.clone()).collect();
                match &selected {
                    Some((global_index, _)) if *global_index == method.global_index => {}
                    // More than one candidate: fail closed (see doc comment).
                    Some(_) => return None,
                    None => selected = Some((method.global_index, names)),
                }
            }
        }
        selected
    }

    fn infer_constructor_arg_julia_type(&self, arg: &Expr) -> JuliaType {
        if let Expr::Var(name, _) = arg {
            if let Some(declared) = self.julia_type_locals.get(name.as_str()) {
                return declared.clone();
            }
        }
        self.infer_julia_type(arg)
    }

    fn single_dynamic_parametric_outer_constructor_method(
        &self,
        base_name: &str,
        type_args: &[TypeExpr],
        arg_count: usize,
    ) -> DynamicParametricOuterSelection {
        let resolved_base_name = self.resolve_parametric_struct_name(base_name);
        let mut candidates = Vec::new();
        for (table_name, table) in self.method_tables {
            let Some((table_base, pattern_args)) = parse_parametric_call(table_name) else {
                continue;
            };
            if !same_parametric_constructor_base(
                &table_base,
                base_name,
                resolved_base_name.as_deref(),
            ) || pattern_args.len() != type_args.len()
            {
                continue;
            }
            let Some(bindings) = pattern_args
                .iter()
                .zip(type_args.iter())
                .map(|(pattern, value)| match pattern {
                    TypeExpr::TypeVar(name) => Some(StaticParamBinding {
                        name: name.clone(),
                        value: value.clone(),
                    }),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            candidates.extend(table.methods.iter().filter_map(|method| {
                if table.is_inner_constructor(method.global_index)
                    || !method.accepts_arity(arg_count)
                    || !self.explicit_type_args_satisfy_inner_ctor_bounds(method, type_args)
                {
                    return None;
                }
                let vars = method.core_signature_type_vars();
                bindings
                    .iter()
                    .all(|binding| vars.iter().any(|var| var.name == binding.name))
                    .then_some((method.global_index, bindings.clone()))
            }));
        }
        match candidates.len() {
            0 => DynamicParametricOuterSelection::None,
            1 => match candidates.pop() {
                Some((global_index, bindings)) => {
                    DynamicParametricOuterSelection::Unique(global_index, bindings)
                }
                None => DynamicParametricOuterSelection::None,
            },
            _ => DynamicParametricOuterSelection::Ambiguous,
        }
    }

    fn parametric_binding_is_statically_forwardable(&self, arg: &TypeExpr) -> bool {
        match arg {
            TypeExpr::Concrete(_) => true,
            TypeExpr::TypeVar(name) => {
                self.current_type_param_index.contains_key(name.as_str())
                    || !self.locals.contains_key(name)
            }
            TypeExpr::Parameterized { params, .. } => params
                .iter()
                .all(|param| self.parametric_binding_is_statically_forwardable(param)),
            TypeExpr::RuntimeExpr(_) => false,
        }
    }

    /// Whether an explicit constructor type argument still depends on the
    /// caller's runtime frame after lexical type-object qualification. A bare
    /// module-local nominal such as `V` becomes `Owner.V` and is static; a
    /// caller `where T`, local DataType, or nested runtime expression must use
    /// `apply_type` so constructor-self bounds are validated at runtime
    /// (Issues #11019 and #11034).
    fn parametric_type_arg_is_runtime_binding(&self, arg: &TypeExpr) -> bool {
        match arg {
            TypeExpr::Concrete(_) => false,
            TypeExpr::TypeVar(name) => {
                self.current_type_param_index.contains_key(name.as_str())
                    || self.locals.contains_key(name)
            }
            TypeExpr::Parameterized { params, .. } => params
                .iter()
                .any(|param| self.parametric_type_arg_is_runtime_binding(param)),
            TypeExpr::RuntimeExpr(_) => true,
        }
    }

    fn parametric_type_arg_requires_runtime(&self, base_name: &str, arg: &TypeExpr) -> bool {
        match arg {
            TypeExpr::Concrete(_) => false,
            TypeExpr::TypeVar(name) => {
                // Numeric spellings in value-parameter positions are literals,
                // not caller type variables (`Val{5}`).
                if name.chars().all(|ch| ch.is_ascii_digit()) {
                    return false;
                }
                let looks_like_where_var = name.len() <= 2
                    && name
                        .chars()
                        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit());
                looks_like_where_var
                    || self.current_type_param_index.contains_key(name.as_str())
                    || self.locals.contains_key(name)
            }
            TypeExpr::Parameterized { params, .. } => params
                .iter()
                .any(|param| self.parametric_type_arg_requires_runtime(base_name, param)),
            TypeExpr::RuntimeExpr(expr) => {
                !(base_name == "Val" && is_static_val_runtime_expr(expr))
            }
        }
    }

    fn multiple_forwardable_parametric_inner_candidates_are_ambiguous(
        &self,
        base_name: &str,
        type_args: &[TypeExpr],
        arg_count: usize,
    ) -> bool {
        if !type_args
            .iter()
            .all(|arg| self.parametric_binding_is_statically_forwardable(arg))
        {
            return false;
        }
        let mut compatible_explicit_inner_indices = self
            .parametric_constructor_method_table_names(base_name)
            .into_iter()
            .filter_map(|name| self.method_tables.get(name.as_ref()))
            .flat_map(|table| {
                table.methods.iter().filter_map(|method| {
                    (table.is_explicit_parametric_inner_constructor(method.global_index)
                        && method.accepts_arity(arg_count)
                        && method.core_signature_type_vars().len() == type_args.len()
                        && self.explicit_type_args_satisfy_inner_ctor_bounds(method, type_args))
                    .then_some(method.global_index)
                })
            })
            .collect::<Vec<_>>();
        compatible_explicit_inner_indices.sort_unstable();
        compatible_explicit_inner_indices.dedup();
        compatible_explicit_inner_indices.len() > 1
    }

    fn parametric_constructor_method_table_names<'b>(
        &self,
        base_name: &'b str,
    ) -> Vec<Cow<'b, str>> {
        // Qualified constructor tables are owner-authoritative. The short
        // table is a compatibility fallback for older/unqualified surfaces,
        // but may have been replaced by a same-named sibling module and must
        // not contribute cross-owner candidates when this table exists
        // (Issue #11034).
        let resolved_base_name = self.resolve_parametric_struct_name(base_name);
        if let Some(resolved) = resolved_base_name.as_deref() {
            if resolved.contains('.') && self.method_tables.contains_key(resolved) {
                return vec![Cow::Owned(resolved.to_string())];
            }
        }
        if base_name.contains('.') && self.method_tables.contains_key(base_name) {
            return vec![Cow::Borrowed(base_name)];
        }

        let mut table_names: Vec<Cow<'b, str>> = Vec::new();
        if let Some(resolved) = resolved_base_name {
            if self.method_tables.contains_key(&resolved) {
                table_names.push(Cow::Owned(resolved));
            }
        }
        if self.method_tables.contains_key(base_name)
            && !table_names.iter().any(|name| name.as_ref() == base_name)
        {
            table_names.push(Cow::Borrowed(base_name));
        }
        let short_name = short_constructor_name(base_name);
        if self.method_tables.contains_key(short_name.as_ref())
            && !table_names
                .iter()
                .any(|name| name.as_ref() == short_name.as_ref())
        {
            table_names.push(short_name);
        }
        table_names
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
        // Origin-aware lookup (Issue #10078): while compiling Base/prelude's
        // OWN function bodies, a bare constructor name must resolve to
        // Base's own struct, never a same-named module alias that was
        // registered later in the same compile pass (only reachable when
        // `should_skip_base_cache_for_program` forces those bodies to be
        // recompiled at all).
        if let Some(struct_info) = self.resolve_struct_info_scoped(function).cloned() {
            let short_name = short_constructor_name(function);
            let method_table_name = if self.method_tables.contains_key(function) {
                Some(Cow::Borrowed(function))
            } else if function.contains('.')
                && struct_info.has_inner_constructor
                && self.method_tables.contains_key(short_name.as_ref())
            {
                // Inner-constructor tables still have a legacy leaf key, but
                // that key is only an existence witness. Invoke the exact
                // qualified DataType so runtime dispatch cannot select a
                // sibling module's same-leaf constructor (Issues #11436/#11469).
                return self
                    .compile_runtime_datatype_value_call(function.to_string(), args, &[], &[], &[])
                    .map(Some);
            } else {
                None
            };
            // Bind the resolved name directly from the match instead of a
            // separate `is_none()` check followed by a later raw unwrap
            // (Issue #10905, Phase 1b of #10869).
            let method_table_name = match method_table_name {
                None => {
                    return self
                        .compile_struct_constructor(struct_info.clone(), args)
                        .map(Some);
                }
                Some(name) => name,
            };
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
                let constructor_owner = self
                    .shared_ctx
                    .get_struct_name(struct_info.type_id)
                    .unwrap_or_else(|| function.to_string());
                let (
                    has_synthetic_default_declaration,
                    selected_synthetic_default_outer,
                    only_owner_synthetic_defaults_for_arity,
                ) = self.method_tables.get(method_table_name.as_ref()).map_or(
                    (false, false, false),
                    |table| {
                        let has_declaration =
                            table.has_synthetic_default_constructor_declaration(&constructor_owner);
                        let has_any_arg = arg_types.iter().any(|arg| matches!(arg, JuliaType::Any));
                        let selected_outer = has_declaration
                            && table
                                .dispatch_among_outer_constructors(&arg_types)
                                .is_ok_and(|method| {
                                    table.is_synthetic_default_outer_for_owner(
                                        method.global_index,
                                        &constructor_owner,
                                    ) && !should_runtime_dispatch(
                                        table,
                                        method,
                                        &arg_types,
                                        args.len(),
                                        has_any_arg,
                                    )
                                });
                        let mut saw_same_arity = false;
                        let only_owner_defaults = has_declaration
                            && table.methods.iter().all(|method| {
                                if !method.accepts_arity(args.len()) {
                                    return true;
                                }
                                saw_same_arity = true;
                                table.is_synthetic_default_outer_for_owner(
                                    method.global_index,
                                    &constructor_owner,
                                ) || (table.constructor_self_family(method.global_index)
                                    == Some(ConstructorSelfFamily::BareInner)
                                    && method.explicit_constructor_type_name.as_deref()
                                        == Some(constructor_owner.as_str()))
                            })
                            && saw_same_arity;
                        (has_declaration, selected_outer, only_owner_defaults)
                    },
                );
                // A no-user-inner user struct now exposes its upstream-shaped
                // defaults as ordinary method-table rows. Owner-exact declaration
                // provenance keeps those rows (and later source-written
                // replacements) on dispatch while cached Base declarations retain
                // the W-72 raw-allocation recursion terminator (Issue #11062).
                // The transient per-row marker then distinguishes the typed
                // synthetic outer from a source-written replacement.
                // Once that synthetic row itself wins and the shared policy says
                // runtime dispatch is unnecessary, its body is exactly raw field
                // allocation (`jl_outer_ctor_body` in upstream), so preserve the
                // legacy allocation path. This also keeps simple struct calls
                // appendable in a persistent REPL. A mismatch/Any argument uses
                // the inline BareInner-equivalent path below, whose guarded
                // runtime `convert` calls preserve catchable errors without
                // embedding a synthetic function index (Issue #11147).
                if !struct_info.has_inner_constructor && selected_synthetic_default_outer {
                    return self
                        .compile_struct_constructor(struct_info.clone(), args)
                        .map(Some);
                }
                if !struct_info.has_inner_constructor && only_owner_synthetic_defaults_for_arity {
                    if !arg_types.iter().any(|arg| matches!(arg, JuliaType::Any))
                        && self.struct_field_count_ctor_args_convertible(&struct_info, args)
                    {
                        return self
                            .compile_struct_constructor(struct_info.clone(), args)
                            .map(Some);
                    }
                    if let Some(value_type) =
                        self.try_compile_synthetic_default_inner_inline(&struct_info, args)?
                    {
                        return Ok(Some(value_type));
                    }
                }
                if !struct_info.has_inner_constructor && has_synthetic_default_declaration {
                    if method_table_name.as_ref() != function {
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
                    return Ok(None);
                }
                if !struct_info.has_inner_constructor
                    && !has_synthetic_default_declaration
                    && arg_types.iter().any(|arg| matches!(arg, JuliaType::Any))
                {
                    return self
                        .compile_struct_constructor(struct_info.clone(), args)
                        .map(Some);
                }
                if !struct_info.has_inner_constructor && !has_synthetic_default_declaration {
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
            let constructor_table = self
                .method_tables
                .get(&resolved_name)
                .or_else(|| self.method_tables.get(function));
            let has_same_arity_constructor = constructor_table
                .is_some_and(|table| table.methods.iter().any(|m| m.accepts_arity(args.len())));
            let arg_types: Vec<JuliaType> =
                args.iter().map(|arg| self.infer_julia_type(arg)).collect();
            let has_any_arg = arg_types.iter().any(|arg| matches!(arg, JuliaType::Any));
            let selected_synthetic_default_outer = constructor_table.is_some_and(|table| {
                table.has_synthetic_default_constructor_declaration(&resolved_name)
                    && table
                        .dispatch_among_outer_constructors(&arg_types)
                        .is_ok_and(|method| {
                            table.is_synthetic_default_outer_for_owner(
                                method.global_index,
                                &resolved_name,
                            ) && !should_runtime_dispatch(
                                table,
                                method,
                                &arg_types,
                                args.len(),
                                has_any_arg,
                            )
                        })
            });
            let only_owner_synthetic_defaults_for_arity = constructor_table.is_some_and(|table| {
                let mut saw_same_arity = false;
                table.has_synthetic_default_constructor_declaration(&resolved_name)
                    && table.methods.iter().all(|method| {
                        if !method.accepts_arity(args.len()) {
                            return true;
                        }
                        saw_same_arity = true;
                        table.is_synthetic_default_outer_for_owner(
                            method.global_index,
                            &resolved_name,
                        ) || (table.constructor_self_family(method.global_index)
                            == Some(ConstructorSelfFamily::ExplicitParametricInner)
                            && method.explicit_constructor_type_name.as_deref()
                                == Some(resolved_name.as_str()))
                    })
                    && saw_same_arity
            });
            if (!has_same_arity_constructor
                || selected_synthetic_default_outer
                || (only_owner_synthetic_defaults_for_arity && has_any_arg))
                && !has_short_inner_constructor_table
            {
                // No custom constructor won, or ordinary dispatch selected the
                // typed synthetic default outer. Its upstream body directly
                // allocates the inferred concrete `Foo{T}` without conversion, so
                // the legacy raw path is equivalent after dispatch selection.
                // Besides avoiding an unnecessary call, resolving the concrete
                // instantiation here makes its type/field facts available to
                // return-type inference without speculative registry mutation
                // during unrelated method compilation (Issues #8638/#11147).
                // Use resolved (qualified) name for instantiation so method dispatch works correctly
                let inference_name = if selected_synthetic_default_outer {
                    resolved_name.as_str()
                } else {
                    function
                };
                let type_args = match self.shared_ctx.infer_type_args(inference_name, &arg_types) {
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
                if let Some(value_type) =
                    self.try_compile_zero_field_instantiated_constructor(type_id, args)
                {
                    return Ok(Some(value_type));
                }
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

fn constructor_single_candidate_fallback_allowed(arg_types: &[JuliaType]) -> bool {
    arg_types
        .iter()
        .any(|arg| matches!(arg, JuliaType::Any) || arg.is_abstract_container())
}

fn constructor_method_owner_matches(
    method: &MethodSig,
    base_name: &str,
    resolved_base_name: Option<&str>,
) -> bool {
    let Some(owner) = method.explicit_constructor_type_name.as_deref() else {
        return true;
    };
    let target = resolved_base_name.unwrap_or(base_name);
    owner == target || (!target.contains('.') && owner == base_name)
}

fn static_param_bindings_satisfy_bounds(
    shared_ctx: &crate::compile::context::SharedCompileContext,
    method: &MethodSig,
    bindings: &[StaticParamBinding],
) -> bool {
    let type_vars = method.core_signature_type_vars();
    let substitutions = core_type_substitutions(&type_vars, bindings);
    let subtype_engine = crate::inference_core::CoreSubtypeEngine::new();
    bindings.iter().all(|binding| {
        let Some(param) = type_vars.iter().find(|param| param.name == binding.name) else {
            return false;
        };
        let resolve_bound = |bound: &crate::inference_core::CoreType| {
            crate::inference_core::instantiate_unionall_typevars(bound, &substitutions)
        };
        let upper = param.upper_bound.as_deref().map(&resolve_bound);
        let lower = param.lower_bound.as_deref().map(&resolve_bound);
        let actual_core = crate::inference_core::CoreType::from(&binding.value);
        let upper_ok = upper.as_ref().is_none_or(|bound| {
            let bound_name = bound.to_julia_name();
            subtype_engine.is_subtype(&actual_core, bound)
                || match &binding.value {
                    TypeExpr::Concrete(jt) => {
                        shared_ctx.concrete_type_satisfies_bound(jt, &bound_name)
                    }
                    TypeExpr::TypeVar(type_name) => {
                        shared_ctx.type_name_satisfies_bound(type_name, &bound_name)
                    }
                    TypeExpr::Parameterized { .. } => shared_ctx.concrete_type_satisfies_bound(
                        &JuliaType::from_name_or_struct(&binding.value.to_string()),
                        &bound_name,
                    ),
                    TypeExpr::RuntimeExpr(_) => false,
                }
        });
        let lower_ok = lower.as_ref().is_none_or(|bound| {
            let bound_name = bound.to_julia_name();
            subtype_engine.is_subtype(bound, &actual_core)
                || match &binding.value {
                    TypeExpr::Concrete(jt) => {
                        shared_ctx.bound_satisfies_concrete_type(&bound_name, jt)
                    }
                    TypeExpr::TypeVar(type_name) => shared_ctx.bound_satisfies_concrete_type(
                        &bound_name,
                        &JuliaType::from_name_or_struct(type_name),
                    ),
                    TypeExpr::Parameterized { .. } => shared_ctx.bound_satisfies_concrete_type(
                        &bound_name,
                        &JuliaType::from_name_or_struct(&binding.value.to_string()),
                    ),
                    TypeExpr::RuntimeExpr(_) => false,
                }
        });
        upper_ok && lower_ok
    })
}

fn instantiate_constructor_dispatch_method(
    method: &MethodSig,
    bindings: &[StaticParamBinding],
) -> MethodSig {
    let type_vars = method.core_signature_type_vars();
    let substitutions = core_type_substitutions(&type_vars, bindings);
    let mut instantiated = method.clone();
    instantiated.core_signature = crate::inference_core::instantiate_unionall_typevars(
        &method.core_signature,
        &substitutions,
    );
    instantiated
}

fn core_type_substitutions(
    type_vars: &[CoreTypeVar],
    bindings: &[StaticParamBinding],
) -> Vec<CoreTypeSubstitution> {
    bindings
        .iter()
        .map(|binding| {
            let variable = type_vars
                .iter()
                .find(|variable| variable.name == binding.name)
                .cloned()
                .unwrap_or_else(|| CoreTypeVar::unscoped(&binding.name));
            CoreTypeSubstitution::new(variable, CoreType::from(&binding.value))
        })
        .collect()
}

fn explicit_inner_constructor_bindings(
    method: &MethodSig,
    type_args: &[TypeExpr],
) -> Option<Vec<StaticParamBinding>> {
    constructor_pattern_bindings(
        &method.explicit_constructor_type_arguments,
        type_args,
        &method.explicit_constructor_type_parameter_names,
    )
}

fn constructor_pattern_bindings(
    patterns: &[TypeExpr],
    actuals: &[TypeExpr],
    binder_names: &[String],
) -> Option<Vec<StaticParamBinding>> {
    if patterns.len() != actuals.len() {
        return None;
    }
    let mut bindings: Vec<StaticParamBinding> = Vec::new();
    fn unify(
        pattern: &TypeExpr,
        actual: &TypeExpr,
        binder_names: &[String],
        bindings: &mut Vec<StaticParamBinding>,
    ) -> bool {
        match pattern {
            TypeExpr::TypeVar(name) if binder_names.contains(name) => {
                if let Some(existing) = bindings.iter().find(|binding| binding.name == *name) {
                    existing.value.to_string() == actual.to_string()
                } else {
                    bindings.push(StaticParamBinding {
                        name: name.clone(),
                        value: actual.clone(),
                    });
                    true
                }
            }
            TypeExpr::Parameterized { base, params } => {
                let TypeExpr::Parameterized {
                    base: actual_base,
                    params: actual_params,
                } = actual
                else {
                    return false;
                };
                base == actual_base
                    && params.len() == actual_params.len()
                    && params
                        .iter()
                        .zip(actual_params)
                        .all(|(pattern, actual)| unify(pattern, actual, binder_names, bindings))
            }
            _ => pattern.to_string() == actual.to_string(),
        }
    }
    patterns
        .iter()
        .zip(actuals)
        .all(|(pattern, actual)| {
            let pattern = pattern.canonicalize_constructor_array_aliases();
            let actual = actual.canonicalize_constructor_array_aliases();
            unify(&pattern, &actual, binder_names, &mut bindings)
        })
        .then_some(bindings)
}

fn same_parametric_constructor_base(
    table_base: &str,
    call_base: &str,
    resolved_call_base: Option<&str>,
) -> bool {
    if let Some(resolved) = resolved_call_base.filter(|resolved| resolved.contains('.')) {
        return table_base == resolved;
    }
    if call_base.contains('.') {
        return table_base == call_base;
    }
    if table_base == call_base || resolved_call_base.is_some_and(|resolved| table_base == resolved)
    {
        return true;
    }

    if table_base.contains('.') {
        return false;
    }
    table_base == call_base
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
    if matches!(
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
    ) {
        return true;
    }
    let JuliaType::TypeVar(_, Some(bound_name)) = jt else {
        return false;
    };
    let bound_name = bound_name.rsplit('.').next().unwrap_or(bound_name);
    crate::compile::promotion::is_numeric_type_name(bound_name)
        || matches!(
            bound_name,
            "Bool"
                | "BigInt"
                | "BigFloat"
                | "Number"
                | "Real"
                | "Integer"
                | "Signed"
                | "Unsigned"
                | "AbstractFloat"
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
