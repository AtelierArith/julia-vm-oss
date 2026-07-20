//! Dynamic dispatch call compilation.
//!
//! Handles compilation of calls where the callee is determined at runtime:
//! - GlobalRef calls: `ref(args...)` where `ref` is a GlobalRef variable
//! - Function variable calls: `f(args...)` where `f` is a Function variable
//! - Dynamic parametric struct constructors: `Point{Tnew}(x, y)`

use crate::bytecode::{Instr, ValueType};
use crate::ir::core::Expr;
use crate::types::JuliaType;

use crate::compile::{CResult, CoreCompiler, TypeExpr};

impl CoreCompiler<'_> {
    pub(super) fn type_is_base_origin(&self, type_name: &str) -> bool {
        let leaf = type_name.rsplit('.').next().unwrap_or(type_name);
        type_name.starts_with("Base.")
            || self
                .shared_ctx
                .parametric_structs
                .get(type_name)
                .or_else(|| self.shared_ctx.parametric_structs.get(leaf))
                .or_else(|| self.shared_ctx.base_parametric_structs.get(leaf))
                .is_some_and(|definition| definition.def.is_base_origin)
    }

    pub(super) fn runtime_nominal_binding_name(&self, type_name: &str) -> Option<String> {
        let lexical_qualified = if type_name.contains('.') {
            None
        } else {
            self.current_module_path
                .as_ref()
                .map(|module| format!("{module}.{type_name}"))
        };
        let is_current_input = self
            .shared_ctx
            .current_input_runtime_nominal_names
            .contains(type_name)
            || lexical_qualified.as_ref().is_some_and(|qualified| {
                self.shared_ctx
                    .current_input_runtime_nominal_names
                    .contains(qualified)
            });
        if !is_current_input {
            return None;
        }
        if self.type_is_base_origin(type_name) {
            return None;
        }
        // A module-local declaration owns its bare spelling even when a
        // same-leaf Main/runtime alias is also present. Prefer the lexical
        // qualified binding before consulting the process-wide bare-name set
        // (Issue #11733).
        let runtime_binding = lexical_qualified
            .filter(|qualified| {
                self.shared_ctx
                    .runtime_nominal_callable_names
                    .contains(qualified)
                    && !self.shared_ctx.struct_table.contains_key(qualified)
                    && !self.shared_ctx.parametric_structs.contains_key(qualified)
            })
            .or_else(|| {
                self.shared_ctx
                    .runtime_nominal_callable_names
                    .contains(type_name)
                    .then(|| type_name.to_string())
            })
            .or_else(|| {
                let module_path = self.current_module_path.as_deref()?;
                let qualified = format!("{module_path}.{type_name}");
                self.shared_ctx
                    .runtime_nominal_callable_names
                    .contains(&qualified)
                    .then_some(qualified)
            })?;
        Some(runtime_binding)
    }

    /// Compile a GlobalRef call: ref(args...) where ref is a GlobalRef variable.
    /// This handles patterns like:
    ///   ref = GlobalRef(Base, :println); ref("hello")
    ///   ref = GlobalRef(Main, :myfunc); ref(1, 2, 3)
    pub(in crate::compile) fn compile_globalref_call(
        &mut self,
        var_name: &str,
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Compile all arguments onto the stack
        for arg in args {
            self.compile_expr(arg)?;
        }

        // Load the GlobalRef variable onto stack
        self.emit(Instr::LoadAny(var_name.to_string()));

        // Emit the dynamic GlobalRef call instruction
        self.emit(Instr::CallGlobalRef(args.len()));

        // Return type depends on the runtime function being called
        Ok(ValueType::Any)
    }

    /// Compile a function variable call: f(args...) where f is a Function variable.
    /// This handles patterns like:
    ///   function setprecision(f::Function, prec); f(); end
    ///   map(f, arr) where f is passed as a function parameter
    pub(in crate::compile) fn compile_function_variable_call_with_kwargs(
        &mut self,
        var_name: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        // Compile all arguments onto the stack
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

        // Load the Function variable onto stack. A variable CAPTURED from an
        // enclosing scope lives in the frame's closure environment, not its
        // locals, so it must be loaded with `LoadCaptured` — otherwise calling a
        // captured function value (`makeapply(f) = x -> f(x)`) fails at runtime
        // with "UndefVarError: f not defined" (Issue #5723).
        //
        // Same-module `const` aliases are stored under their qualified module
        // binding (`M.f`) so later method bodies must load that binding, not the
        // unqualified global (`f`) (Issue #8254).
        if self.captured_vars.contains(var_name) && !self.locals.contains_key(var_name) {
            self.emit(Instr::LoadCaptured(var_name.to_string()));
        } else if !self.locals.contains_key(var_name) {
            if let Some(module_path) = &self.current_module_path {
                if self
                    .module_constants
                    .get(module_path)
                    .is_some_and(|constants| constants.contains(var_name))
                {
                    self.emit(Instr::LoadGlobalAny(format!(
                        "{}.{}",
                        module_path, var_name
                    )));
                } else {
                    self.emit(Instr::LoadAny(var_name.to_string()));
                }
            } else {
                self.emit(Instr::LoadAny(var_name.to_string()));
            }
        } else {
            self.emit(Instr::LoadAny(var_name.to_string()));
        }

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
            // Emit the dynamic function call instruction
            self.emit(Instr::CallFunctionVariable(args.len()));
        }

        // Return type depends on the runtime function being called
        Ok(ValueType::Any)
    }

    pub(in crate::compile) fn compile_runtime_global_function_call(
        &mut self,
        function: &str,
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

        self.emit(Instr::PushFunction(function.to_string()));
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

    /// Compile a dynamic parametric struct constructor: Point{Tnew}(x, y)
    /// where Tnew is a local variable holding a DataType value.
    /// At runtime, the type parameter is resolved from the variable.
    pub(in crate::compile) fn compile_dynamic_parametric_struct(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Resolve to qualified name if available (e.g., Point -> MyModule.Point)
        let qualified_base_name = self
            .resolve_parametric_struct_name(base_name)
            .unwrap_or_else(|| base_name.to_string());

        // Ensure a fallback {Any, ...} instantiation exists so runtime field access works
        // even when the concrete type parameters are only known at runtime.
        if !type_args.is_empty() {
            let any_args = vec![JuliaType::Any; type_args.len()];
            let _ = self
                .shared_ctx
                .resolve_instantiation(&qualified_base_name, &any_args);
        }

        let declared_field_count = self
            .shared_ctx
            .parametric_structs
            .get(&qualified_base_name)
            .or_else(|| self.shared_ctx.parametric_structs.get(base_name))
            .map(|def| def.def.fields.len());

        if let Some(runtime_binding) = self.runtime_nominal_binding_name(&qualified_base_name) {
            // Whole-program metadata also contains runtime-conditional
            // parametric families whose branch may not execute. Resolve the
            // source-position binding before evaluating type or value
            // arguments (Issue #11713).
            self.emit(Instr::ProbeRuntimeBinding(runtime_binding));
            self.emit(Instr::Pop);
        }

        // Julia evaluates the complete type application before the constructor
        // arguments. Preserve each runtime type-argument value while compiling
        // the fields needed by the allocator's [fields..., type_args...] layout.
        let mut type_arg_temps = Vec::with_capacity(type_args.len());
        for type_arg in type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
            let type_arg_temp = self.new_temp("dynamic_parametric_type_arg");
            self.emit(Instr::StoreAny(type_arg_temp.clone()));
            type_arg_temps.push(type_arg_temp);
        }

        // When the call arity is not the raw field count, this is an outer
        // constructor/conversion call such as `Rational{T}(1)` where `T` is a
        // method type variable. Build the concrete DataType and call it so the
        // normal constructor methods run instead of allocating a malformed
        // one-field struct (Issue #8253).
        if declared_field_count.is_some_and(|field_count| field_count != args.len()) {
            self.emit(Instr::PushDataType(qualified_base_name));
            for type_arg_temp in &type_arg_temps {
                self.emit(Instr::LoadAny(type_arg_temp.clone()));
            }
            self.emit(Instr::ApplyTypeDynamic(type_args.len()));
            let callee_temp = self.new_temp("dynamic_parametric_callee");
            self.emit(Instr::StoreAny(callee_temp.clone()));
            for arg in args {
                self.compile_expr(arg)?;
            }
            self.emit(Instr::LoadAny(callee_temp));
            self.emit(Instr::CallFunctionVariable(args.len()));
            return Ok(ValueType::Any);
        }

        for arg in args {
            self.compile_expr(arg)?;
        }
        for type_arg_temp in type_arg_temps {
            self.emit(Instr::LoadAny(type_arg_temp));
        }

        // Emit instruction to construct struct with dynamic type parameters
        self.emit(Instr::NewDynamicParametricStruct(
            qualified_base_name,
            args.len(),
            type_args.len(),
        ));

        // Return Any since the actual struct type is determined at runtime
        Ok(ValueType::Any)
    }

    /// Build a runtime `DataType` and invoke it as a constructor even when the
    /// positional arity equals the raw field count. Declared inner constructors
    /// suppress the synthesized field constructor, so their runtime self
    /// constraints/bounds must dispatch before allocation (Issue #10959).
    pub(in crate::compile) fn compile_dynamic_parametric_constructor_method_call(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Julia evaluates the complete `Foo{T...}` callee before its value
        // arguments. Park that DataType while producing CallFunctionVariable's
        // required [args..., callee] stack layout (Issue #11375).
        let qualified_base_name = self
            .resolve_parametric_struct_name(base_name)
            .unwrap_or_else(|| base_name.to_string());
        if let Some(runtime_binding) = self.runtime_nominal_binding_name(&qualified_base_name) {
            self.emit(Instr::ProbeRuntimeBinding(runtime_binding));
        } else {
            self.emit(Instr::PushDataType(qualified_base_name));
        }
        for type_arg in type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
        }
        self.emit(Instr::ApplyTypeDynamic(type_args.len()));
        let callee_temp = self.new_temp("dynamic_parametric_constructor_callee");
        self.emit(Instr::StoreAny(callee_temp.clone()));
        for arg in args {
            self.compile_expr(arg)?;
        }
        self.emit(Instr::LoadAny(callee_temp));
        self.emit(Instr::CallFunctionVariable(args.len()));
        Ok(ValueType::Any)
    }

    /// Push a single parametric type-argument as a runtime value (a `DataType`,
    /// a value parameter such as `5` / `true`, or a `Symbol`). Shared by the
    /// dynamic struct-constructor path and the runtime apply-type path so both
    /// build identical type-parameter values.
    pub(in crate::compile) fn emit_parametric_type_arg_value(
        &mut self,
        type_arg: &TypeExpr,
    ) -> CResult<()> {
        match type_arg {
            TypeExpr::TypeVar(name) => {
                if let Ok(value) = name.parse::<i64>() {
                    self.emit(Instr::PushI64(value));
                    return Ok(());
                }
                if name == "true" || name == "false" {
                    self.emit(Instr::PushBool(name == "true"));
                    return Ok(());
                }
                // Load the DataType variable value.
                // Use LoadAny because type parameters from where clauses are stored
                // in type_bindings, and LoadAny has fallback logic to search
                // through all frames' type_bindings (important for nested calls
                // like constructors where the type binding is in a parent frame).
                self.emit(Instr::LoadAny(name.to_string()));
            }
            TypeExpr::Concrete(jt) => {
                // Push concrete type as DataType value
                self.emit(Instr::PushDataType(jt.name().to_string()));
            }
            TypeExpr::Parameterized { base, params } => {
                for param in params {
                    self.emit_parametric_type_arg_value(param)?;
                }
                self.emit(Instr::ConstructParametricType(base.clone(), params.len()));
            }
            TypeExpr::RuntimeExpr(expr_str) => {
                // Parse and compile the expression at runtime
                // This handles cases like Symbol(s) in MIME{Symbol(s)}
                // The expression result will be used as the type parameter value
                if let Ok(expr) = crate::lowering::lower_expr_from_text(expr_str) {
                    self.compile_expr(&expr)?;
                } else {
                    // Fallback: treat as a variable name
                    self.emit(Instr::LoadAny(expr_str.clone()));
                }
            }
        }
        Ok(())
    }

    /// Compile `t{T...}(args)` where `t` is a *local variable* holding a runtime
    /// `DataType` value rather than a statically-known parametric struct name
    /// (Issue #8101). The base type is only known at runtime, so apply the
    /// explicit type parameters to it (`Core.apply_type`-style) to obtain the
    /// concrete `Base{T...}` `DataType`, then call it as a constructor — exactly
    /// the explicit-type-argument analogue of the no-type-argument dynamic form
    /// `t(args)` (Issue #8070). The resulting runtime constructor honours the
    /// *explicit* type parameters (converting the arguments), matching upstream
    /// `Base{T...}(args)` semantics.
    pub(in crate::compile) fn compile_local_datatype_parametric_call(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
    ) -> CResult<ValueType> {
        self.compile_lexical_datatype_parametric_call(base_name, type_args, args, &[], &[], &[])
    }

    /// `t{T...}(args)` where `t` is a module-level VALUE binding rather than a
    /// lexical local — e.g. a module constant that shadowed a conflicting
    /// import (Issue #11426). Same runtime `Core.apply_type` semantics as the
    /// lexical form below, loading the qualified module global as the base.
    pub(in crate::compile) fn compile_module_value_datatype_parametric_call(
        &mut self,
        qualified_base: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        self.emit(Instr::LoadGlobalAny(qualified_base.to_string()));
        self.compile_datatype_parametric_apply_tail(
            type_args,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
        )
    }

    pub(in crate::compile) fn compile_lexical_datatype_parametric_call(
        &mut self,
        base_name: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        // Julia evaluates the callee expression before its arguments. Build the
        // concrete runtime DataType first, then park it while producing the call
        // instruction's required [args..., kwargs..., callee] stack layout.
        self.load_local(base_name)?;
        self.compile_datatype_parametric_apply_tail(
            type_args,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
        )
    }

    fn compile_datatype_parametric_apply_tail(
        &mut self,
        type_args: &[TypeExpr],
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        for type_arg in type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
        }
        self.emit(Instr::ApplyTypeDynamic(type_args.len()));
        let callee_temp = self.new_temp("parametric_datatype_callee");
        self.emit(Instr::StoreAny(callee_temp.clone()));

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
}
