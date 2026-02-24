//! Module-qualified function call compilation.
//!
//! Handles:
//! - `Module.func(args)` calls (e.g., `Base.push!`, `Random.seed!`)
//! - Module function references (e.g., `Base.println` as a value)

use crate::builtins::BuiltinId;
use crate::ir::core::{Expr, UnaryOp};
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use super::ITERATORS_FUNCTIONS;
use crate::compile::{
    base_function_to_builtin_op, err, function_name_to_binary_op, get_math_constant_value,
    is_base_function, is_base_submodule_function, is_random_function, julia_type_to_value_type,
    CResult, CompileError, CoreCompiler,
};

impl CoreCompiler<'_> {
    /// Compile a module-qualified function call: Module.func(args)
    pub(in crate::compile) fn compile_module_call(
        &mut self,
        module: &str,
        function: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
    ) -> CResult<ValueType> {
        // Resolve module aliases: S.mean() -> Statistics.mean() if S = Statistics
        let resolved_module = self
            .module_aliases
            .get(module)
            .map(|s| s.as_str())
            .unwrap_or(module);

        // Special handling for Base module - maps to built-in functions
        if resolved_module == "Base" {
            // Handle operators: Base.:+(a, b), Base.:-(a, b), etc.
            // This bypasses user-defined operator overloads to access the builtin operators.
            if let Some(op) = function_name_to_binary_op(function) {
                if args.len() == 2 {
                    return self.compile_builtin_binary_op(&op, &args[0], &args[1]);
                } else if args.len() == 1 && function == "-" {
                    // Unary minus: Base.:-(x)
                    return self.compile_unary_op(&UnaryOp::Neg, &args[0]);
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

            // Try to map to BuiltinOp first (handles types properly)
            if let Some(builtin_op) = base_function_to_builtin_op(function) {
                return self.compile_builtin(&builtin_op, args);
            }
            // Fall back to string-based builtin call for functions not in BuiltinOp
            if is_base_function(function) {
                return self.compile_builtin_call(function, args);
            }
            // For Pure Julia functions defined in Base (transpose, adjoint, etc.),
            // fall back to normal function call which uses the method table
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

            if submodule == "LinearAlgebra" {
                // Handle functions that forward to Pure Julia implementations in Base
                if function == "transpose" {
                    // transpose is implemented in Pure Julia (base/array.jl)
                    // Forward to the base transpose function
                    return self.compile_call("transpose", args, &[], &[], &[]);
                }

                let builtin = match function {
                    "inv" => Some(BuiltinId::Inv),
                    "svd" => Some(BuiltinId::Svd),
                    "qr" => Some(BuiltinId::Qr),
                    "eigen" => Some(BuiltinId::Eigen),
                    "eigvals" => Some(BuiltinId::Eigvals),
                    "cholesky" => Some(BuiltinId::Cholesky),
                    "rank" => Some(BuiltinId::Rank),
                    "cond" => Some(BuiltinId::Cond),
                    "lu" => Some(BuiltinId::Lu),
                    "det" => Some(BuiltinId::Det),
                    _ => None,
                };
                if let Some(builtin) = builtin {
                    if args.len() != 1 {
                        return err(format!(
                            "{} requires exactly 1 argument: {}(A)",
                            function, function
                        ));
                    }
                    self.compile_expr(&args[0])?;
                    self.emit(Instr::CallBuiltin(builtin, 1));
                    return Ok(match function {
                        "det" | "cond" => ValueType::F64,
                        "rank" => ValueType::I64,
                        "svd" | "qr" | "eigen" | "cholesky" => ValueType::NamedTuple,
                        "lu" => ValueType::Tuple,
                        _ => ValueType::Array,
                    });
                }
            }

            // Special handling for Base.Iterators — forward to Pure Julia functions
            if submodule == "Iterators" {
                if ITERATORS_FUNCTIONS.contains(&function) {
                    return self.compile_call(function, args, kwargs, &[], &[]);
                }
                return err(format!("Base.Iterators has no function named {}", function));
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

        // Special handling for Iterators module (Issue #2066, #2159)
        if resolved_module == "Iterators" {
            if ITERATORS_FUNCTIONS.contains(&function) {
                return self.compile_call(function, args, kwargs, &[], &[]);
            }
            return err(format!(
                "Iterators module has no function named {}",
                function
            ));
        }

        // Verify the module exists and contains the function
        let module_funcs = self
            .module_functions
            .get(resolved_module)
            .ok_or_else(|| CompileError::Msg(format!("Unknown module: {}", resolved_module)))?;

        if !module_funcs.contains(function) {
            return err(format!(
                "Module {} has no function named {}",
                resolved_module, function
            ));
        }

        // Look up the function in the global method tables
        let table = self.method_tables.get(function).ok_or_else(|| {
            CompileError::Msg(format!(
                "Internal error: function {} not found in method tables",
                function
            ))
        })?;

        let arg_types: Vec<JuliaType> = args.iter().map(|a| self.infer_julia_type(a)).collect();
        let method = table.dispatch(&arg_types)?;

        // Compile positional arguments with expected types
        if let Some(vararg_idx) = method.vararg_param_index {
            for (idx, arg) in args.iter().enumerate() {
                if idx < vararg_idx {
                    if let Some((_, param_ty)) = method.params.get(idx) {
                        if *param_ty == JuliaType::Any {
                            self.compile_expr(arg)?;
                        } else {
                            let vt = julia_type_to_value_type(param_ty);
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
            for (arg, (_, param_ty)) in args.iter().zip(method.params.iter()) {
                if *param_ty == JuliaType::Any {
                    self.compile_expr(arg)?;
                } else {
                    let vt = julia_type_to_value_type(param_ty);
                    self.compile_expr_as(arg, vt)?;
                }
            }
        }

        if kwargs.is_empty() {
            self.emit_call_or_specialize(function, method.global_index, args.len());
        } else {
            let kwarg_names: Vec<String> = kwargs.iter().map(|(name, _)| name.clone()).collect();
            for (_, value) in kwargs {
                self.compile_expr(value)?;
            }
            self.emit(Instr::CallWithKwargs(
                method.global_index,
                args.len(),
                kwarg_names,
            ));
        }

        Ok(method.return_type.clone())
    }

    /// Compile a module-qualified function reference: Module.func
    pub(in crate::compile) fn compile_module_function_ref(
        &mut self,
        module: &str,
        function: &str,
    ) -> CResult<ValueType> {
        let resolved_module = self
            .module_aliases
            .get(module)
            .map(|s| s.as_str())
            .unwrap_or(module);

        if resolved_module == "Base" {
            if is_base_function(function)
                || base_function_to_builtin_op(function).is_some()
                || function_name_to_binary_op(function).is_some()
            {
                self.emit(Instr::PushFunction(format!("Base.{}", function)));
                return Ok(ValueType::Any);
            }
            return err(format!("Base has no function named {}", function));
        }

        if let Some(submodule) = resolved_module.strip_prefix("Base.") {
            if submodule == "Iterators" {
                if ITERATORS_FUNCTIONS.contains(&function) {
                    self.emit(Instr::PushFunction(function.to_string()));
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
            if is_random_function(function) {
                self.emit(Instr::PushFunction(format!("Random.{}", function)));
                return Ok(ValueType::Any);
            }
            return err(format!("Random has no function named {}", function));
        }

        // Special handling for Iterators module (Issue #2066, #2159)
        if resolved_module == "Iterators" {
            if ITERATORS_FUNCTIONS.contains(&function) {
                self.emit(Instr::PushFunction(function.to_string()));
                return Ok(ValueType::Any);
            }
            return err(format!(
                "Iterators module has no function named {}",
                function
            ));
        }

        let module_funcs = self
            .module_functions
            .get(resolved_module)
            .ok_or_else(|| CompileError::Msg(format!("Unknown module: {}", resolved_module)))?;

        if !module_funcs.contains(function) {
            return err(format!(
                "Module {} has no function named {}",
                resolved_module, function
            ));
        }

        self.emit(Instr::PushFunction(format!(
            "{}.{}",
            resolved_module, function
        )));
        Ok(ValueType::Any)
    }
}
