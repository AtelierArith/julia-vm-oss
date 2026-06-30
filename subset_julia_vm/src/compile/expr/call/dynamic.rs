//! Dynamic dispatch call compilation.
//!
//! Handles compilation of calls where the callee is determined at runtime:
//! - GlobalRef calls: `ref(args...)` where `ref` is a GlobalRef variable
//! - Function variable calls: `f(args...)` where `f` is a Function variable
//! - Dynamic parametric struct constructors: `Point{Tnew}(x, y)`

use crate::ir::core::Expr;
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use crate::compile::{CResult, CoreCompiler, TypeExpr};

impl CoreCompiler<'_> {
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
        kwargs: &[(String, Expr)],
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
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.clone()).collect();
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
                crate::vm::CallVarKwargsSplat {
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
        kwargs: &[(String, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> CResult<ValueType> {
        for arg in args {
            self.compile_expr(arg)?;
        }

        let has_splat = splat_mask.iter().any(|&is_splat| is_splat);
        let has_kwargs = !kwargs.is_empty();
        let has_kwargs_splat = kwargs_splat_mask.iter().any(|&is_splat| is_splat);
        let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.clone()).collect();
        for (_, value) in kwargs {
            self.compile_expr(value)?;
        }

        self.emit(Instr::PushFunction(function.to_string()));
        if has_kwargs || has_splat || has_kwargs_splat {
            self.emit(Instr::CallFunctionVariableWithKwargsSplat(Box::new(
                crate::vm::CallVarKwargsSplat {
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
        // Compile all field arguments
        for arg in args {
            self.compile_expr(arg)?;
        }

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

        // When the call arity is not the raw field count, this is an outer
        // constructor/conversion call such as `Rational{T}(1)` where `T` is a
        // method type variable. Build the concrete DataType and call it so the
        // normal constructor methods run instead of allocating a malformed
        // one-field struct (Issue #8253).
        if declared_field_count.is_some_and(|field_count| field_count != args.len()) {
            self.emit(Instr::PushDataType(qualified_base_name));
            for type_arg in type_args {
                self.emit_parametric_type_arg_value(type_arg)?;
            }
            self.emit(Instr::ApplyTypeDynamic(type_args.len()));
            self.emit(Instr::CallFunctionVariable(args.len()));
            return Ok(ValueType::Any);
        }

        // Load type parameter DataType values onto stack
        for type_arg in type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
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
                // Push the parameterized type as a DataType string
                // Uses TypeExpr::Display impl which handles nested types recursively
                let type_str = TypeExpr::format_parameterized(base, params);
                self.emit(Instr::PushDataType(type_str));
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
        // Target stack layout for `CallFunctionVariable`: [args..., callee].
        // Evaluate the positional arguments first (they sit deepest).
        for arg in args {
            self.compile_expr(arg)?;
        }
        // Build the concrete `Base{T...}` DataType on top of the arguments:
        // push the base DataType local, then each explicit type parameter, and
        // apply them at runtime. `ApplyTypeDynamic` pops exactly the base plus
        // its `type_args.len()` parameters, leaving the arguments untouched.
        self.load_local(base_name)?;
        for type_arg in type_args {
            self.emit_parametric_type_arg_value(type_arg)?;
        }
        self.emit(Instr::ApplyTypeDynamic(type_args.len()));
        // Call the resulting DataType value as a constructor.
        self.emit(Instr::CallFunctionVariable(args.len()));
        Ok(ValueType::Any)
    }
}
