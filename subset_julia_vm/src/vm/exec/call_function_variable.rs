//! Function variable and GlobalRef call instructions.
//!
//! Handles: CallGlobalRef, CallFunctionVariable, CallFunctionVariableWithSplat
//!
//! These instructions handle calling functions stored in variables,
//! GlobalRef-based builtin calls, and function calls with splat arguments.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::util::format_value;
use super::super::*;
use super::call::bind_kwargs_defaults;
use super::call_dynamic::CallDynamicResult;
use super::util::bind_value_to_slot;
use crate::builtins::BuiltinId;
use crate::rng::RngLike;

impl<R: RngLike> Vm<R> {
    /// Execute function variable and GlobalRef call instructions.
    ///
    /// Returns `CallDynamicResult::NotHandled` if the instruction is not handled.
    #[inline]
    pub(super) fn execute_call_function_variable(
        &mut self,
        instr: &Instr,
    ) -> Result<CallDynamicResult, VmError> {
        match instr {
            Instr::CallGlobalRef(arg_count) => {
                // Call a GlobalRef as a function: ref(args...)
                // Stack layout: [args..., globalref]
                // Pop the GlobalRef first
                let globalref_val = self.stack.pop_value()?;
                let globalref = match globalref_val {
                    Value::GlobalRef(gr) => gr,
                    _ => {
                        // INTERNAL: CallGlobalRef is emitted only when the compiler resolves a GlobalRef; wrong type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "Expected GlobalRef, got {:?}",
                            globalref_val
                        )));
                    }
                };

                // Pop arguments
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Resolve the GlobalRef to a function name
                // Format: "module.name" for qualified lookup
                let func_name = globalref.name.as_str();
                let module_name = &globalref.module;

                // For Base module, try builtins first since they're more reliable
                // for common functions like sqrt, sin, cos, etc.
                if module_name == "Base" {
                    if let Some(result) =
                        self.call_globalref_builtin(module_name, func_name, &args)?
                    {
                        self.stack.push(result);
                        return Ok(CallDynamicResult::Handled);
                    }
                }

                // Try to find user-defined function using name index (Issue #3361)
                // First, try simple name, then qualified name (Module.func)
                let qualified_name = format!("{}.{}", module_name, func_name);
                let func_index = self
                    .get_function_indices_by_name(func_name)
                    .first()
                    .copied()
                    .or_else(|| {
                        self.get_function_indices_by_name(&qualified_name)
                            .first()
                            .copied()
                    });

                if let Some(func_index) = func_index {
                    // User-defined function found - call it
                    let func = self.get_function_checked(func_index)?.clone();

                    let mut frame = Frame::new_with_slots(func.local_slot_count, Some(func_index));

                    // Bind type parameters from where clauses (Issue #2468)
                    self.bind_type_params(&func, &args, &mut frame);

                    // Bind arguments to parameter slots
                    if let Some(vararg_idx) = func.vararg_param_index {
                        // Function has varargs
                        for idx in 0..vararg_idx {
                            if let Some(val) = args.get(idx) {
                                if let Some(slot) = func.param_slots.get(idx) {
                                    bind_value_to_slot(
                                        &mut frame,
                                        *slot,
                                        val.clone(),
                                        &mut self.struct_heap,
                                    );
                                }
                            }
                        }
                        // Collect remaining args into a Tuple
                        let vararg_values: Vec<Value> = args[vararg_idx..].to_vec();
                        let vararg_tuple = Value::Tuple(TupleValue {
                            elements: vararg_values,
                        });
                        if let Some(slot) = func.param_slots.get(vararg_idx) {
                            bind_value_to_slot(
                                &mut frame,
                                *slot,
                                vararg_tuple,
                                &mut self.struct_heap,
                            );
                        }
                    } else {
                        // No varargs: bind 1-to-1
                        for (idx, slot) in func.param_slots.iter().enumerate() {
                            if let Some(val) = args.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }

                    // Set up default values for keyword parameters
                    for kwparam in &func.kwparams {
                        if kwparam.required {
                            return Err(VmError::UndefKeywordError(kwparam.name.clone()));
                        }
                        bind_value_to_slot(
                            &mut frame,
                            kwparam.slot,
                            kwparam.default.clone(),
                            &mut self.struct_heap,
                        );
                    }

                    self.return_ips.push(self.ip);
                    self.frames.push(frame);
                    self.ip = func.entry;
                    Ok(CallDynamicResult::Handled)
                } else {
                    // No matching function found in functions table
                    // For non-Base modules, this is an error
                    // (Base module builtins were already tried above)
                    Err(VmError::TypeError(format!(
                        "Function '{}' not found in module '{}'",
                        func_name, module_name
                    )))
                }
            }

            Instr::CallFunctionVariable(arg_count) => {
                // Call a Function or Closure stored in a local variable: f(args...)
                // This handles patterns like: function setprecision(f::Function, ...); f(); end
                // Also handles callable struct instances: (::Type)(args) = body
                // Stack layout: [args..., function_value]
                // Pop the Function/Closure value first
                let func_val = self.stack.pop_value()?;
                let (func_name, closure_captures) = match &func_val {
                    Value::Function(fv) => (fv.name.clone(), None),
                    Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
                    Value::Struct(si) => {
                        // Callable struct instance: look up __callable_<TypeName>
                        let callable_name = format!("__callable_{}", si.struct_name);
                        (callable_name, None)
                    }
                    Value::StructRef(idx) => {
                        // Callable struct reference: resolve and look up __callable_<TypeName>
                        let si = self.struct_heap.get(*idx).ok_or_else(|| {
                            VmError::TypeError(format!(
                                "Invalid struct reference: index {} out of bounds",
                                idx
                            ))
                        })?;
                        let callable_name = format!("__callable_{}", si.struct_name);
                        (callable_name, None)
                    }
                    _ => {
                        // User-visible: user can call a non-function value stored in a variable
                        return Err(VmError::TypeError(format!(
                            "Expected Function or Closure, got {:?}",
                            func_val
                        )));
                    }
                };

                // Pop arguments
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Get runtime type names for all arguments
                let arg_type_names: Vec<String> =
                    args.iter().map(|a| self.get_type_name(a)).collect();

                // Find all methods with the matching function name and do proper dispatch
                // based on runtime argument types.
                // Issue #1658: We must check if argument types match the declared parameter
                // types, not just pick the first method with matching name.
                // Use function_name_index for O(1) lookup (Issue #3361)
                let candidates: Vec<(usize, &FunctionInfo)> = self
                    .get_function_indices_by_name(&func_name)
                    .iter()
                    .map(|&idx| (idx, &self.functions[idx]))
                    .collect();

                if candidates.is_empty() {
                    // Fallback: try to dispatch as a builtin function (Issue #2070)
                    // Builtin functions (uppercase, lowercase, string, etc.) are not
                    // in the user-defined method table, but can be passed as arguments
                    // to higher-order functions like map/filter/reduce.
                    if let Some(builtin_id) = BuiltinId::from_name(&func_name) {
                        // Push args back onto stack (builtins expect args on the stack)
                        for arg in &args {
                            self.stack.push(arg.clone());
                        }
                        self.execute_builtin(builtin_id, args.len())?;
                        return Ok(CallDynamicResult::Handled);
                    }
                    // User-visible: user can call a function variable that resolves to no compiled methods
                    return Err(VmError::TypeError(format!(
                        "Function '{}' not found",
                        func_name
                    )));
                }

                // Find best matching method based on runtime types.
                // If user-defined dispatch fails, try builtin fallback (Issue #2546).
                // This handles cases like sqrt(Float64) where user-defined methods only
                // exist for Complex types but the builtin handles Float64.
                let func_index =
                    match self.dispatch_function_variable(&func_name, &candidates, &arg_type_names)
                    {
                        Ok(idx) => idx,
                        Err(_) => {
                            // Try BuiltinId-registered builtins first
                            if let Some(builtin_id) = BuiltinId::from_name(&func_name) {
                                for arg in &args {
                                    self.stack.push(arg.clone());
                                }
                                self.execute_builtin(builtin_id, args.len())?;
                                return Ok(CallDynamicResult::Handled);
                            }
                            // Try intrinsic math functions (sqrt, abs, etc.) (Issue #2546)
                            if let Some(result) = self.try_call_intrinsic(&func_name, &args)? {
                                self.stack.push(result);
                                return Ok(CallDynamicResult::Handled);
                            }
                            return Err(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_name,
                                arg_type_names.join(", ")
                            )));
                        }
                    };

                let func = self.get_function_checked(func_index)?.clone();

                let mut frame = if let Some(captures) = closure_captures {
                    Frame::new_with_captures(func.local_slot_count, Some(func_index), captures)
                } else {
                    Frame::new_with_slots(func.local_slot_count, Some(func_index))
                };

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &args, &mut frame);

                // Bind arguments to parameter slots
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining args into a Tuple
                    let vararg_values: Vec<Value> = args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, slot) in func.param_slots.iter().enumerate() {
                        if let Some(val) = args.get(idx) {
                            bind_value_to_slot(
                                &mut frame,
                                *slot,
                                val.clone(),
                                &mut self.struct_heap,
                            );
                        }
                    }
                }

                // Bind keyword arguments with their defaults.
                // Use bind_kwargs_defaults() so kwargs... varargs get empty Pairs, not Nothing.
                bind_kwargs_defaults(&func, &mut frame, &mut self.struct_heap)?;

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = func.entry;
                Ok(CallDynamicResult::Handled)
            }

            Instr::CallFunctionVariableWithSplat(arg_count, ref splat_mask) => {
                // Call a Function or Closure stored in a local variable with splatted arguments.
                // This handles patterns like: function apply_variadic(f, args...); f(args...); end
                // Stack layout: [args..., function_value]

                // Pop the Function/Closure value first
                let func_val = self.stack.pop_value()?;
                let (func_name, closure_captures) = match &func_val {
                    Value::Function(fv) => (fv.name.clone(), None),
                    Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
                    _ => {
                        // User-visible: user can call a non-function value with splatted args via dynamic dispatch
                        return Err(VmError::TypeError(format!(
                            "Expected Function or Closure, got {:?}",
                            func_val
                        )));
                    }
                };

                // Pop arguments
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Expand splatted arguments
                let expanded_args = super::super::splat::expand_splat_arguments(args, splat_mask);

                // Get runtime type names for all expanded arguments
                let arg_type_names: Vec<String> = expanded_args
                    .iter()
                    .map(|a| self.get_type_name(a))
                    .collect();

                // Find all methods with the matching function name and do proper dispatch
                // Use function_name_index for O(1) lookup (Issue #3361)
                let candidates: Vec<(usize, &FunctionInfo)> = self
                    .get_function_indices_by_name(&func_name)
                    .iter()
                    .map(|&idx| (idx, &self.functions[idx]))
                    .collect();

                if candidates.is_empty() {
                    // Fallback: try to dispatch as a builtin function (Issue #2070)
                    if let Some(builtin_id) = BuiltinId::from_name(&func_name) {
                        for arg in &expanded_args {
                            self.stack.push(arg.clone());
                        }
                        self.execute_builtin(builtin_id, expanded_args.len())?;
                        return Ok(CallDynamicResult::Handled);
                    }
                    // User-visible: user can call a function variable with splat that resolves to no compiled methods
                    return Err(VmError::TypeError(format!(
                        "Function '{}' not found",
                        func_name
                    )));
                }

                // Find best matching method based on runtime types.
                // If user-defined dispatch fails, try builtin fallback (Issue #2546).
                let func_index =
                    match self.dispatch_function_variable(&func_name, &candidates, &arg_type_names)
                    {
                        Ok(idx) => idx,
                        Err(_) => {
                            if let Some(builtin_id) = BuiltinId::from_name(&func_name) {
                                for arg in &expanded_args {
                                    self.stack.push(arg.clone());
                                }
                                self.execute_builtin(builtin_id, expanded_args.len())?;
                                return Ok(CallDynamicResult::Handled);
                            }
                            // Try intrinsic math functions (sqrt, abs, etc.) (Issue #2546)
                            if let Some(result) =
                                self.try_call_intrinsic(&func_name, &expanded_args)?
                            {
                                self.stack.push(result);
                                return Ok(CallDynamicResult::Handled);
                            }
                            return Err(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_name,
                                arg_type_names.join(", ")
                            )));
                        }
                    };

                let func = self.get_function_checked(func_index)?.clone();

                let mut frame = if let Some(captures) = closure_captures {
                    Frame::new_with_captures(func.local_slot_count, Some(func_index), captures)
                } else {
                    Frame::new_with_slots(func.local_slot_count, Some(func_index))
                };

                // Bind type parameters from where clauses (Issue #2468)
                self.bind_type_params(&func, &expanded_args, &mut frame);

                // Bind expanded arguments to parameters (with varargs support)
                if let Some(vararg_idx) = func.vararg_param_index {
                    // Function has varargs
                    for idx in 0..vararg_idx {
                        if let Some(val) = expanded_args.get(idx) {
                            if let Some(slot) = func.param_slots.get(idx) {
                                bind_value_to_slot(
                                    &mut frame,
                                    *slot,
                                    val.clone(),
                                    &mut self.struct_heap,
                                );
                            }
                        }
                    }
                    // Collect remaining expanded args into a Tuple
                    let vararg_values: Vec<Value> = expanded_args[vararg_idx..].to_vec();
                    let vararg_tuple = Value::Tuple(TupleValue {
                        elements: vararg_values,
                    });
                    if let Some(slot) = func.param_slots.get(vararg_idx) {
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
                    }
                } else {
                    // No varargs: bind 1-to-1
                    for (idx, slot) in func.param_slots.iter().enumerate() {
                        if let Some(val) = expanded_args.get(idx) {
                            bind_value_to_slot(
                                &mut frame,
                                *slot,
                                val.clone(),
                                &mut self.struct_heap,
                            );
                        }
                    }
                }

                // Bind keyword arguments with their defaults.
                // Use bind_kwargs_defaults() so kwargs... varargs get empty Pairs, not Nothing.
                bind_kwargs_defaults(&func, &mut frame, &mut self.struct_heap)?;

                self.return_ips.push(self.ip);
                self.frames.push(frame);
                self.ip = func.entry;
                Ok(CallDynamicResult::Handled)
            }

            _ => Ok(CallDynamicResult::NotHandled),
        }
    }

    fn call_globalref_builtin(
        &mut self,
        module: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        // Handle Base module functions
        if module == "Base" {
            match func_name {
                "println" => {
                    // Print all arguments
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            print!(" ");
                        }
                        print!("{}", format_value(arg));
                    }
                    println!();
                    return Ok(Some(Value::Nothing));
                }
                "print" => {
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            print!(" ");
                        }
                        print!("{}", format_value(arg));
                    }
                    return Ok(Some(Value::Nothing));
                }
                "string" => {
                    let result: String = args.iter().map(format_value).collect();
                    return Ok(Some(Value::Str(result)));
                }
                "length" => {
                    if args.len() == 1 {
                        let len = match &args[0] {
                            Value::Array(arr) => arr.borrow().element_count() as i64,
                            Value::Memory(mem) => mem.borrow().len() as i64,
                            Value::Tuple(t) => t.len() as i64,
                            Value::NamedTuple(nt) => nt.values.len() as i64,
                            Value::Dict(dict) => dict.len() as i64,
                            Value::Set(set) => set.len() as i64,
                            Value::Str(s) => s.chars().count() as i64,
                            Value::Range(r) => {
                                if r.step == 0.0 {
                                    0
                                } else if r.step > 0.0 {
                                    if r.stop >= r.start {
                                        ((r.stop - r.start) / r.step).floor() as i64 + 1
                                    } else {
                                        0
                                    }
                                } else if r.start >= r.stop {
                                    ((r.start - r.stop) / (-r.step)).floor() as i64 + 1
                                } else {
                                    0
                                }
                            }
                            _ => {
                                // User-visible: user can invoke Base.length via GlobalRef on an unsupported type
                                return Err(VmError::TypeError(format!(
                                    "length not defined for {:?}",
                                    args[0]
                                )))
                            }
                        };
                        return Ok(Some(Value::I64(len)));
                    }
                }
                "typeof" => {
                    if args.len() == 1 {
                        let jt = self.get_value_julia_type(&args[0]);
                        return Ok(Some(Value::DataType(jt)));
                    }
                }
                "abs" => {
                    if args.len() == 1 {
                        let result = match &args[0] {
                            Value::I64(v) => Value::I64(v.abs()),
                            Value::F64(v) => Value::F64(v.abs()),
                            Value::I32(v) => Value::I32(v.abs()),
                            Value::F32(v) => Value::F32(v.abs()),
                            _ => {
                                // User-visible: user can invoke Base.abs via GlobalRef on an unsupported type
                                return Err(VmError::TypeError(format!(
                                    "abs not supported for {:?}",
                                    args[0]
                                )))
                            }
                        };
                        return Ok(Some(result));
                    }
                }
                "sqrt" => {
                    if args.len() == 1 {
                        let v = self.convert_to_f64(&args[0])?;
                        return Ok(Some(Value::F64(v.sqrt())));
                    }
                }
                "sin" | "cos" | "tan" | "exp" | "log" => {
                    if args.len() == 1 {
                        let v = self.convert_to_f64(&args[0])?;
                        let result = match func_name {
                            "sin" => v.sin(),
                            "cos" => v.cos(),
                            "tan" => v.tan(),
                            "exp" => v.exp(),
                            "log" => v.ln(),
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "unexpected unary intrinsic '{}'",
                                    func_name
                                )))
                            }
                        };
                        return Ok(Some(Value::F64(result)));
                    }
                }
                "floor" | "ceil" | "round" | "trunc" => {
                    if args.len() == 1 {
                        let v = self.convert_to_f64(&args[0])?;
                        let result = match func_name {
                            "floor" => v.floor(),
                            "ceil" => v.ceil(),
                            "round" => v.round(),
                            "trunc" => v.trunc(),
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "unexpected rounding intrinsic '{}'",
                                    func_name
                                )))
                            }
                        };
                        return Ok(Some(Value::F64(result)));
                    }
                }
                _ => {}
            }
        }

        // Not a builtin we recognize
        Ok(None)
    }

    /// Try to call a function as a math/intrinsic function (Issue #2546).
    /// This handles functions like sqrt, abs, sin, cos that are compiled as direct
    /// instructions when called statically, but need runtime dispatch when called
    /// via Value::Function (e.g., through broadcast infrastructure).
    ///
    /// Returns Ok(Some(result)) if handled, Ok(None) if not recognized.
    pub(super) fn try_call_intrinsic(
        &mut self,
        func_name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        match func_name {
            "sqrt" => {
                if args.len() == 1 {
                    let v = self.convert_to_f64(&args[0])?;
                    return Ok(Some(Value::F64(v.sqrt())));
                }
            }
            "abs" => {
                if args.len() == 1 {
                    let result = match &args[0] {
                        Value::I64(v) => Value::I64(v.abs()),
                        Value::F64(v) => Value::F64(v.abs()),
                        Value::I32(v) => Value::I32(v.abs()),
                        Value::F32(v) => Value::F32(v.abs()),
                        _ => {
                            // User-visible: user can call abs as an intrinsic on an unsupported type
                            return Err(VmError::TypeError(format!(
                                "abs not supported for {:?}",
                                args[0]
                            )))
                        }
                    };
                    return Ok(Some(result));
                }
            }
            "sin" | "cos" | "tan" | "exp" | "log" => {
                if args.len() == 1 {
                    let v = self.convert_to_f64(&args[0])?;
                    let result = match func_name {
                        "sin" => v.sin(),
                        "cos" => v.cos(),
                        "tan" => v.tan(),
                        "exp" => v.exp(),
                        "log" => v.ln(),
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "unexpected unary intrinsic '{}'",
                                func_name
                            )))
                        }
                    };
                    return Ok(Some(Value::F64(result)));
                }
            }
            "floor" | "ceil" | "round" | "trunc" => {
                if args.len() == 1 {
                    let v = self.convert_to_f64(&args[0])?;
                    let result = match func_name {
                        "floor" => v.floor(),
                        "ceil" => v.ceil(),
                        "round" => v.round(),
                        "trunc" => v.trunc(),
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "unexpected rounding intrinsic '{}'",
                                func_name
                            )))
                        }
                    };
                    return Ok(Some(Value::F64(result)));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    /// Call a function by name with a single argument.
    /// Uses proper dispatch to check if argument type matches parameter type.
    /// Issue #1658: Previously just called the first method found, without type checking.
    pub(super) fn dispatch_function_variable(
        &self,
        func_name: &str,
        candidates: &[(usize, &FunctionInfo)],
        arg_type_names: &[String],
    ) -> Result<usize, VmError> {
        // If only one candidate, we still need to check arity
        // but we can't do full type checking without JuliaType info
        if candidates.len() == 1 {
            let (idx, func) = candidates[0];
            // Check arity first
            let expected_arity = func.vararg_param_index.unwrap_or(func.params.len());
            if func.vararg_param_index.is_some() {
                if let Some(fixed_count) = func.vararg_fixed_count {
                    // Vararg{T, N}: exactly expected_arity + N args (Issue #2525)
                    if arg_type_names.len() != expected_arity + fixed_count {
                        return Err(VmError::MethodError(format!(
                            "no method matching {}({}) - expected exactly {} arguments",
                            func_name,
                            arg_type_names.join(", "),
                            expected_arity + fixed_count
                        )));
                    }
                } else if arg_type_names.len() < expected_arity {
                    return Err(VmError::MethodError(format!(
                        "no method matching {}({}) - expected at least {} arguments",
                        func_name,
                        arg_type_names.join(", "),
                        expected_arity
                    )));
                }
            } else if arg_type_names.len() != expected_arity {
                return Err(VmError::MethodError(format!(
                    "no method matching {}({}) - expected {} arguments",
                    func_name,
                    arg_type_names.join(", "),
                    expected_arity
                )));
            }

            // Check if argument types match parameter types using JuliaType info
            for (arg_idx, (param_name, _param_vt)) in func.params.iter().enumerate() {
                if arg_idx >= arg_type_names.len() {
                    break;
                }
                // Get JuliaType from param_julia_types if available
                if let Some(param_jt) = func.param_julia_types.get(arg_idx) {
                    let arg_type_name = &arg_type_names[arg_idx];
                    // Check if the runtime argument type is compatible with the declared parameter type
                    if !self.check_type_match(arg_type_name, param_jt) {
                        return Err(VmError::MethodError(format!(
                            "MethodError: no method matching {}({})\n  Closest candidate is: {}({}) where argument {} has type {}, expected {}",
                            func_name,
                            arg_type_names.join(", "),
                            func_name,
                            func.params.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>().join(", "),
                            param_name,
                            arg_type_name,
                            param_jt.name()
                        )));
                    }
                }
            }
            return Ok(idx);
        }

        // Multiple candidates: find best matching method
        let mut best_match: Option<(usize, u32)> = None;

        for (idx, func) in candidates {
            // Check arity
            let required_arity = func.vararg_param_index.unwrap_or(func.params.len());

            let arity_match = if func.vararg_param_index.is_some() {
                if let Some(fixed_count) = func.vararg_fixed_count {
                    // Vararg{T, N}: exactly required_arity fixed params + N varargs (Issue #2525)
                    arg_type_names.len() == required_arity + fixed_count
                } else {
                    arg_type_names.len() >= required_arity
                }
            } else {
                arg_type_names.len() == required_arity
            };

            if !arity_match {
                continue;
            }

            // Check if all arguments match
            let mut all_match = true;
            let mut specificity: u32 = 0;

            for (arg_idx, arg_type_name) in arg_type_names.iter().enumerate() {
                if arg_idx >= func.param_julia_types.len() {
                    // Varargs case - remaining args don't need specific type check
                    break;
                }
                let param_jt = &func.param_julia_types[arg_idx];
                if self.check_type_match(arg_type_name, param_jt) {
                    // Add specificity score based on parameter type
                    specificity += param_jt.specificity() as u32;
                    // Bonus for exact match
                    if self.is_exact_type_match(arg_type_name, param_jt) {
                        specificity += 10;
                    }
                } else {
                    all_match = false;
                    break;
                }
            }

            if all_match {
                match &best_match {
                    None => best_match = Some((*idx, specificity)),
                    Some((_, best_spec)) if specificity > *best_spec => {
                        best_match = Some((*idx, specificity));
                    }
                    _ => {}
                }
            }
        }

        best_match.map(|(idx, _)| idx).ok_or_else(|| {
            VmError::MethodError(format!(
                "MethodError: no method matching {}({})",
                func_name,
                arg_type_names.join(", ")
            ))
        })
    }
}
