//! Dynamic dispatch call compilation.
//!
//! Handles compilation of calls where the callee is determined at runtime:
//! - Dynamic type constructor calls: `T(x)` where `T` is a DataType variable
//! - GlobalRef calls: `ref(args...)` where `ref` is a GlobalRef variable
//! - Function variable calls: `f(args...)` where `f` is a Function variable
//! - Dynamic parametric struct constructors: `Point{Tnew}(x, y)`

use crate::ir::core::Expr;
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use crate::compile::{err, CResult, CoreCompiler, TypeExpr};

impl CoreCompiler<'_> {
    /// Compile a dynamic type constructor call: T(x) where T is a DataType variable.
    /// This handles patterns like:
    ///   T = Float64; T(1)
    ///   function f(T, x); T(x); end
    pub(in crate::compile) fn compile_dynamic_type_call(
        &mut self,
        var_name: &str,
        args: &[Expr],
    ) -> CResult<ValueType> {
        if args.len() != 1 {
            return err(format!(
                "Type constructor {} requires exactly 1 argument, got {}",
                var_name,
                args.len()
            ));
        }

        // Compile the value to convert
        self.compile_expr(&args[0])?;

        // Load the DataType variable onto stack
        self.emit(Instr::LoadAny(var_name.to_string()));

        // Emit the dynamic type conversion instruction
        self.emit(Instr::CallTypeConstructor);

        // Return type depends on the runtime DataType value
        Ok(ValueType::Any)
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
    pub(in crate::compile) fn compile_function_variable_call(
        &mut self,
        var_name: &str,
        args: &[Expr],
    ) -> CResult<ValueType> {
        // Compile all arguments onto the stack
        for arg in args {
            self.compile_expr(arg)?;
        }

        // Load the Function variable onto stack
        self.emit(Instr::LoadAny(var_name.to_string()));

        // Emit the dynamic function call instruction
        self.emit(Instr::CallFunctionVariable(args.len()));

        // Return type depends on the runtime function being called
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

        // Load type parameter DataType values onto stack
        for type_arg in type_args {
            match type_arg {
                TypeExpr::TypeVar(name) => {
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
                    let type_str = format!(
                        "{}{{{}}}",
                        base,
                        params
                            .iter()
                            .map(|p| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
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

        // Emit instruction to construct struct with dynamic type parameters
        self.emit(Instr::NewDynamicParametricStruct(
            qualified_base_name,
            args.len(),
            type_args.len(),
        ));

        // Return Any since the actual struct type is determined at runtime
        Ok(ValueType::Any)
    }
}
