//! Typed dispatch call instructions.
//!
//! Handles: CallTypedDispatch, CallTypeConstructor
//!
//! These instructions handle method dispatch when parameter types are
//! declared in function signatures, and type constructor calls.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::call_dynamic::CallDynamicResult;
use super::util::{bind_value_to_slot, is_rust_dict_parametric_mismatch};
use crate::builtins::BuiltinId;
use crate::rng::RngLike;

impl<R: RngLike> Vm<R> {
    /// Execute typed dispatch call instructions.
    ///
    /// Returns `CallDynamicResult::NotHandled` if the instruction is not a typed dispatch operation.
    #[inline]
    pub(super) fn execute_call_dynamic_typed(
        &mut self,
        instr: &Instr,
    ) -> Result<CallDynamicResult, VmError> {
        match instr {
            Instr::CallTypedDispatch(ref _func_name, arg_count, fallback_index, ref candidates) => {
                // Runtime dispatch for Type{T} patterns
                // Pop arguments (expected to be DataType values)
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Extract type names from DataType values
                let arg_type_names: Vec<String> = args
                    .iter()
                    .map(|arg| {
                        match arg {
                            Value::DataType(jt) => jt.name().to_string(),
                            // For non-DataType values, use their runtime type name
                            _ => self.get_type_name(arg),
                        }
                    })
                    .collect();

                // Helper: check if a pattern element is a TypeVar (short uppercase name like T, S, T1)
                fn is_type_var(s: &str) -> bool {
                    !s.is_empty()
                        && s.len() <= 2
                        && s.chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                }

                // Helper: parse parametric type like "Complex{T}" or "Complex{Float64}"
                // Returns (base, params) e.g., ("Complex", vec!["T"]) or ("Complex", vec!["Float64"])
                fn parse_parametric(s: &str) -> (&str, Vec<&str>) {
                    if let Some(brace_start) = s.find('{') {
                        if let Some(brace_end) = s.rfind('}') {
                            let base = &s[..brace_start];
                            let params_str = &s[brace_start + 1..brace_end];
                            // Split by comma for multi-param types
                            let params: Vec<&str> =
                                params_str.split(',').map(|p| p.trim()).collect();
                            return (base, params);
                        }
                    }
                    (s, vec![])
                }

                // Helper: check if expected is an abstract type that subsumes actual
                // This handles Julia's type hierarchy for runtime dispatch
                fn is_abstract_supertype(expected: &str, actual: &str) -> bool {
                    // Any matches everything
                    if expected == "Any" {
                        return true;
                    }
                    // Array family: Array matches Vector{T}, Matrix{T}, Array{T,N}
                    let act_base = if let Some(idx) = actual.find('{') {
                        &actual[..idx]
                    } else {
                        actual
                    };
                    if expected == "Array"
                        && (act_base == "Vector" || act_base == "Matrix" || act_base == "Array")
                    {
                        return true;
                    }
                    // Abstract numeric hierarchy
                    let is_numeric_actual = matches!(
                        act_base,
                        "Int8"
                            | "Int16"
                            | "Int32"
                            | "Int64"
                            | "Int128"
                            | "UInt8"
                            | "UInt16"
                            | "UInt32"
                            | "UInt64"
                            | "UInt128"
                            | "Float16"
                            | "Float32"
                            | "Float64"
                            | "Bool"
                            | "BigInt"
                            | "BigFloat"
                    );
                    if expected == "Number" && is_numeric_actual {
                        return true;
                    }
                    let is_real_actual = is_numeric_actual; // All concrete numerics are Real in Julia
                    if expected == "Real" && is_real_actual {
                        return true;
                    }
                    let is_integer_actual = matches!(
                        act_base,
                        "Int8"
                            | "Int16"
                            | "Int32"
                            | "Int64"
                            | "Int128"
                            | "UInt8"
                            | "UInt16"
                            | "UInt32"
                            | "UInt64"
                            | "UInt128"
                            | "Bool"
                            | "BigInt"
                    );
                    if expected == "Integer" && is_integer_actual {
                        return true;
                    }
                    let is_signed_actual = matches!(
                        act_base,
                        "Int8" | "Int16" | "Int32" | "Int64" | "Int128" | "BigInt"
                    );
                    if expected == "Signed" && is_signed_actual {
                        return true;
                    }
                    let is_unsigned_actual = matches!(
                        act_base,
                        "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128" | "Bool"
                    );
                    if expected == "Unsigned" && is_unsigned_actual {
                        return true;
                    }
                    let is_float_actual =
                        matches!(act_base, "Float16" | "Float32" | "Float64" | "BigFloat");
                    if expected == "AbstractFloat" && is_float_actual {
                        return true;
                    }
                    // AbstractString matches String
                    if expected == "AbstractString"
                        && (act_base == "String" || act_base == "SubString")
                    {
                        return true;
                    }
                    false
                }

                // Helper: check if a single type pattern matches an actual type
                // Handles both simple TypeVars (T) and parametric types (Complex{T})
                fn type_matches<'a>(
                    expected: &'a str,
                    actual: &'a str,
                    bindings: &mut std::collections::HashMap<&'a str, &'a str>,
                ) -> bool {
                    // Case 0: Abstract supertype check BEFORE parametric matching.
                    // This handles cases like expected="Any" matching actual="Vector{Int64}".
                    // Without this, the parametric branch would fail because "Any" != "Vector".
                    // (Issue #2119)
                    if is_abstract_supertype(expected, actual) {
                        return true;
                    }

                    // Case 0.5: Covariant bound pattern (_<:Bound) (Issue #2526)
                    // Handles Type{<:Animal} where inner becomes "_<:Animal"
                    if let Some(bound) = expected.strip_prefix("_<:") {
                        return is_abstract_supertype(bound, actual);
                    }

                    // Case 1: Simple TypeVar
                    if is_type_var(expected) {
                        if let Some(&bound_type) = bindings.get(expected) {
                            return bound_type == actual;
                        } else {
                            bindings.insert(expected, actual);
                            return true;
                        }
                    }

                    // Case 2: Parametric type pattern like "Complex{T}"
                    let (exp_base, exp_params) = parse_parametric(expected);
                    let (act_base, act_params) = parse_parametric(actual);

                    // If expected has type parameters but actual doesn't, or vice versa with different bases
                    if !exp_params.is_empty() || !act_params.is_empty() {
                        // Base types must match (with Array family support)
                        let bases_match = exp_base == act_base
                            || (exp_base == "Array"
                                && (act_base == "Vector"
                                    || act_base == "Matrix"
                                    || act_base == "Array"));
                        if !bases_match {
                            return false;
                        }
                        // Parameter count must match (or expected has none - base type match)
                        if exp_params.is_empty() {
                            // Expected is just base type like "Complex", matches any Complex{...}
                            return true;
                        }
                        if exp_params.len() != act_params.len() {
                            return false;
                        }
                        // Check each type parameter
                        for (exp_param, act_param) in exp_params.iter().zip(act_params.iter()) {
                            if is_type_var(exp_param) {
                                // TypeVar in parametric position - check/bind
                                if let Some(&bound_type) = bindings.get(*exp_param) {
                                    if bound_type != *act_param {
                                        return false;
                                    }
                                } else {
                                    bindings.insert(*exp_param, *act_param);
                                }
                            } else if let Some(bound) = exp_param.strip_prefix("_<:") {
                                // Covariant bound in parametric position (Issue #2526)
                                // e.g., Vector{<:Number} → exp_param="_<:Number"
                                if !is_abstract_supertype(bound, act_param) {
                                    return false;
                                }
                            } else {
                                // Concrete type in parametric position - must match exactly
                                if exp_param != act_param {
                                    return false;
                                }
                            }
                        }
                        return true;
                    }

                    // Case 3: Exact match for non-parametric types
                    if expected == actual {
                        return true;
                    }

                    // Case 4: Abstract type hierarchy check (Issue #1922)
                    // Handles cases like "Array" matching "Matrix{Float64}",
                    // "Any" matching anything, "Real" matching "Float64", etc.
                    is_abstract_supertype(expected, actual)
                }

                // Helper: check if a pattern matches the given argument types
                // TypeVar patterns (T, S, etc.) match any type, but same TypeVars must match same types
                // Also handles parametric types like Complex{T} matching Complex{Float64}
                fn pattern_matches(expected_types: &[String], arg_types: &[String]) -> bool {
                    if expected_types.len() != arg_types.len() {
                        return false;
                    }
                    // Track TypeVar bindings: T -> "Int64", S -> "Float64"
                    let mut bindings: std::collections::HashMap<&str, &str> =
                        std::collections::HashMap::new();
                    for (expected, actual) in expected_types.iter().zip(arg_types.iter()) {
                        if !type_matches(expected, actual, &mut bindings) {
                            return false;
                        }
                    }
                    true
                }

                // Helper: calculate specificity of a pattern
                // Higher specificity = more concrete (less TypeVars, same TypeVars)
                // Abstract types (Any, Array, Real, Number, etc.) get lower specificity
                // than concrete types, so more specific methods are preferred.
                fn pattern_specificity(expected_types: &[String]) -> i32 {
                    let mut specificity = 0;
                    let mut type_var_count = 0;
                    let mut same_type_var_bonus = 0;
                    let mut seen_type_vars: std::collections::HashSet<&str> =
                        std::collections::HashSet::new();
                    for expected in expected_types {
                        if is_type_var(expected) {
                            type_var_count += 1;
                            if seen_type_vars.contains(expected.as_str()) {
                                // Same TypeVar appears multiple times - this is MORE specific
                                // (e.g., [T, T] is more specific than [T, S] when args match)
                                same_type_var_bonus += 100;
                            }
                            seen_type_vars.insert(expected);
                        } else {
                            // Score based on type specificity (Issue #1922)
                            let has_params = expected.contains('{');
                            let base = if let Some(idx) = expected.find('{') {
                                &expected[..idx]
                            } else {
                                expected.as_str()
                            };
                            let type_score = match base {
                                "Any" => 0,    // Least specific
                                "Number" => 2, // Abstract numeric
                                "Real" => 3,   // More specific than Number
                                "Integer" | "AbstractFloat" => 4,
                                "Signed" | "Unsigned" => 5,
                                "Array" => 6, // Abstract container
                                "AbstractString" => 6,
                                _ if base.starts_with("_<:") => 3, // Covariant bound (Issue #2526)
                                _ => 10,                           // Concrete type - most specific
                            };
                            // Parametric types with type variables (Dict{K,V}, Rational{T})
                            // are more specific than bare types (Dict, Rational) because
                            // they constrain the type to be parametric. (Issue #2748)
                            let param_bonus = if has_params { 1 } else { 0 };
                            specificity += type_score + param_bonus;
                        }
                    }
                    // Fewer TypeVars = more specific
                    // But same TypeVars appearing multiple times = even more specific
                    specificity - type_var_count + same_type_var_bonus
                }

                // Find the best matching candidate:
                // 1. First try exact match (all concrete types)
                // 2. Then try pattern match with TypeVars, preferring more specific patterns
                let mut best_match: Option<(usize, i32)> = None;
                for (idx, expected_types) in candidates.iter() {
                    // Value::Dict (Rust-backed) must not match parametric Dict{K,V}
                    // Pure Julia methods that expect StructRef (Issue #2748).
                    let has_dict_mismatch = args
                        .iter()
                        .zip(expected_types.iter())
                        .any(|(arg, exp)| is_rust_dict_parametric_mismatch(arg, exp));
                    if has_dict_mismatch {
                        continue;
                    }
                    if pattern_matches(expected_types, &arg_type_names) {
                        let specificity = pattern_specificity(expected_types);
                        match &best_match {
                            None => best_match = Some((*idx, specificity)),
                            Some((_, best_specificity)) if specificity > *best_specificity => {
                                best_match = Some((*idx, specificity));
                            }
                            _ => {}
                        }
                    }
                }

                // Covariant bound fallback: if no match via static matching,
                // try VM-level subtype check for user-defined abstract types (Issue #2526).
                // This handles Type{<:Animal} where Animal is user-defined.
                if best_match.is_none() {
                    for (idx, expected_types) in candidates.iter() {
                        if expected_types.len() != arg_type_names.len() {
                            continue;
                        }
                        // Value::Dict (Rust-backed) must not match parametric Dict{K,V} (Issue #2748).
                        let has_dict_mismatch = args
                            .iter()
                            .zip(expected_types.iter())
                            .any(|(arg, exp)| is_rust_dict_parametric_mismatch(arg, exp));
                        if has_dict_mismatch {
                            continue;
                        }
                        // Only try if at least one pattern has covariant bound
                        if !expected_types.iter().any(|e| e.contains("_<:")) {
                            continue;
                        }
                        let all_match =
                            expected_types
                                .iter()
                                .zip(arg_type_names.iter())
                                .all(|(exp, act)| {
                                    if let Some(bound) = exp.strip_prefix("_<:") {
                                        self.check_subtype(act, bound)
                                    } else {
                                        let mut bindings = std::collections::HashMap::new();
                                        type_matches(exp, act, &mut bindings)
                                    }
                                });
                        if all_match {
                            let specificity = pattern_specificity(expected_types);
                            match &best_match {
                                None => best_match = Some((*idx, specificity)),
                                Some((_, best_specificity)) if specificity > *best_specificity => {
                                    best_match = Some((*idx, specificity));
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // If no specific match found (only TypeVar fallback with negative specificity),
                // try runtime method search to find user-defined methods not in the
                // frozen candidate list (Issue #2557).
                let selected_func_index = if best_match.is_some_and(|(_, s)| s > 0) {
                    // Found a specific (non-TypeVar) match in compiled candidates
                    best_match.map(|(idx, _)| idx).unwrap_or(*fallback_index)
                } else {
                    // Search functions by name at runtime for a better match (Issue #3361).
                    // Uses function_name_index for O(1) name lookup instead of O(f) scan.
                    let mut runtime_best: Option<(usize, i32)> = None;
                    for &idx in self.get_function_indices_by_name(_func_name) {
                        let func = &self.functions[idx];
                        if func.param_julia_types.len() != args.len() {
                            continue;
                        }
                        // Build candidate pattern from param_julia_types
                        let pattern: Vec<String> = func
                            .param_julia_types
                            .iter()
                            .map(|jt| match jt {
                                crate::types::JuliaType::TypeOf(inner) => inner.name().to_string(),
                                other => other.name().to_string(),
                            })
                            .collect();
                        // Value::Dict must not match parametric Dict{K,V} (Issue #2748).
                        let has_dict_mismatch = args
                            .iter()
                            .zip(pattern.iter())
                            .any(|(arg, exp)| is_rust_dict_parametric_mismatch(arg, exp));
                        if has_dict_mismatch {
                            continue;
                        }
                        if pattern_matches(&pattern, &arg_type_names) {
                            let specificity = pattern_specificity(&pattern);
                            if runtime_best.is_none_or(|(_, s)| specificity > s) {
                                runtime_best = Some((idx, specificity));
                            }
                        // Covariant bound fallback for runtime search (Issue #2526)
                        } else if pattern.iter().any(|p| p.contains("_<:"))
                            && pattern.len() == arg_type_names.len()
                            && pattern.iter().zip(arg_type_names.iter()).all(|(exp, act)| {
                                if let Some(bound) = exp.strip_prefix("_<:") {
                                    self.check_subtype(act, bound)
                                } else {
                                    let mut bindings = std::collections::HashMap::new();
                                    type_matches(exp, act, &mut bindings)
                                }
                            })
                        {
                            let specificity = pattern_specificity(&pattern);
                            if runtime_best.is_none_or(|(_, s)| specificity > s) {
                                runtime_best = Some((idx, specificity));
                            }
                        }
                    }
                    // Use runtime match only if it's more specific than the candidate match
                    match (runtime_best, best_match) {
                        (Some((rt_idx, rt_spec)), Some((_, cand_spec))) if rt_spec > cand_spec => {
                            rt_idx
                        }
                        (Some((rt_idx, _)), None) => rt_idx,
                        _ => best_match.map(|(idx, _)| idx).unwrap_or(*fallback_index),
                    }
                };

                let func = match self.get_function_cloned_or_raise(selected_func_index)? {
                    Some(f) => f,
                    None => return Ok(CallDynamicResult::Continue),
                };

                let mut frame =
                    Frame::new_with_slots(func.local_slot_count, Some(selected_func_index));
                for (idx, slot) in func.param_slots.iter().enumerate() {
                    if let Some(val) = args.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }

                // For Type{T} patterns with where clause, bind type parameters
                // Extract types from DataType values and use func.type_params for names
                for (idx, arg) in args.iter().enumerate() {
                    if let Value::DataType(jt) = arg {
                        // Use the type parameter name from func.type_params if available
                        if let Some(type_param) = func.type_params.get(idx) {
                            frame
                                .type_bindings
                                .insert(type_param.name.clone(), jt.clone());
                        }
                    }
                }

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
            }

            Instr::CallTypeConstructor => {
                // Dynamic call: T(x) where T can be:
                // - DataType: type conversion
                // - Function: call the function
                // - ComposedFunction: call inner, then outer
                // Stack: [value, callable] -> [result]
                let callable = self.stack.pop_value()?;
                let value = self.stack.pop_value()?;

                match callable {
                    Value::DataType(jt) => {
                        // Type conversion
                        let type_name = jt.name();
                        let result = match type_name.as_ref() {
                            "Int8" => Value::I8(self.convert_to_i8(&value)?),
                            "Int16" => Value::I16(self.convert_to_i16(&value)?),
                            "Int32" => Value::I32(self.convert_to_i32(&value)?),
                            "Int64" => Value::I64(self.convert_to_i64(&value)?),
                            "Int128" => Value::I128(self.convert_to_i128(&value)?),
                            "UInt8" => Value::U8(self.convert_to_u8(&value)?),
                            "UInt16" => Value::U16(self.convert_to_u16(&value)?),
                            "UInt32" => Value::U32(self.convert_to_u32(&value)?),
                            "UInt64" => Value::U64(self.convert_to_u64(&value)?),
                            "UInt128" => Value::U128(self.convert_to_u128(&value)?),
                            "Float16" => Value::F16(self.convert_to_f16(&value)?),
                            "Float32" => Value::F32(self.convert_to_f32(&value)?),
                            "Float64" => Value::F64(self.convert_to_f64(&value)?),
                            _ => {
                                // User-visible: user can request construction of an unsupported numeric type at runtime
                                return Err(VmError::TypeError(format!(
                                    "Cannot construct {} from value",
                                    type_name
                                )));
                            }
                        };
                        self.stack.push(result);
                        Ok(CallDynamicResult::Handled)
                    }
                    Value::Function(fv) => {
                        // Call function by name
                        self.call_function_by_name_with_arg(&fv.name, value)?;
                        Ok(CallDynamicResult::Handled)
                    }
                    Value::Closure(cv) => {
                        // Call closure with captured variables
                        self.call_closure_with_arg(&cv.name, cv.captures.clone(), value)?;
                        Ok(CallDynamicResult::Handled)
                    }
                    Value::ComposedFunction(cf) => {
                        // (f ∘ g)(x) = f(g(x))
                        // Set up to call inner first, then outer
                        self.setup_composed_call(*cf.outer, *cf.inner, value)?;
                        Ok(CallDynamicResult::Handled)
                    }
                    Value::Struct(si) => {
                        // Callable struct instance: (::Type)(x) = body
                        let callable_name = format!("__callable_{}", si.struct_name);
                        self.call_function_by_name_with_arg(&callable_name, value)?;
                        Ok(CallDynamicResult::Handled)
                    }
                    Value::StructRef(idx) => {
                        // Callable struct reference: resolve and dispatch
                        let si = self.struct_heap.get(idx).ok_or_else(|| {
                            VmError::TypeError(format!(
                                "Invalid struct reference: index {} out of bounds",
                                idx
                            ))
                        })?;
                        let callable_name = format!("__callable_{}", si.struct_name);
                        self.call_function_by_name_with_arg(&callable_name, value)?;
                        Ok(CallDynamicResult::Handled)
                    }
                    _ => Err(VmError::TypeError(format!(
                        "Expected callable (DataType, Function, Closure, or ComposedFunction), got {:?}",
                        callable
                    ))),
                }
            }

            _ => Ok(CallDynamicResult::NotHandled),
        }
    }

    fn call_function_by_name_with_arg(
        &mut self,
        func_name: &str,
        arg: Value,
    ) -> Result<(), VmError> {
        // Get runtime type of the argument
        let arg_type_name = self.get_type_name(&arg);

        // Use function_name_index for O(1) lookup (Issue #3361)
        let indices = self.get_function_indices_by_name(func_name);
        if indices.is_empty() {
            // Fallback: try to dispatch as a builtin function (Issue #2070)
            if let Some(builtin_id) = BuiltinId::from_name(func_name) {
                self.stack.push(arg);
                self.execute_builtin(builtin_id, 1)?;
                return Ok(());
            }
            // INTERNAL: closure function name is compiler-assigned; function not found is a compiler bug
            return Err(VmError::InternalError(format!(
                "Function '{}' not found",
                func_name
            )));
        }
        let candidates: Vec<(usize, &FunctionInfo)> =
            indices.iter().map(|&idx| (idx, &self.functions[idx])).collect();

        // Find best matching method based on runtime types.
        // If user-defined dispatch fails, try builtin fallback (Issue #2546).
        let func_index = match self.dispatch_function_variable(
            func_name,
            &candidates,
            std::slice::from_ref(&arg_type_name),
        ) {
                Ok(idx) => idx,
                Err(_) => {
                    if let Some(builtin_id) = BuiltinId::from_name(func_name) {
                        self.stack.push(arg);
                        self.execute_builtin(builtin_id, 1)?;
                        return Ok(());
                    }
                    // Try intrinsic math functions (sqrt, abs, etc.) (Issue #2546)
                    if let Some(result) = self.try_call_intrinsic(func_name, &[arg])? {
                        self.stack.push(result);
                        return Ok(());
                    }
                    return Err(VmError::MethodError(format!(
                        "no method matching {}({})",
                        func_name, arg_type_name
                    )));
                }
            };

        let func = self.get_function_checked(func_index)?.clone();

        let mut frame = Frame::new_with_slots(func.local_slot_count, Some(func_index));

        // Bind argument to first parameter slot
        if let Some(slot) = func.param_slots.first() {
            bind_value_to_slot(&mut frame, *slot, arg, &mut self.struct_heap);
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
        Ok(())
    }

    /// Call a closure with captured variables and one argument
    fn call_closure_with_arg(
        &mut self,
        func_name: &str,
        captures: Vec<(String, Value)>,
        arg: Value,
    ) -> Result<(), VmError> {
        // Get runtime type of the argument
        let arg_type_name = self.get_type_name(&arg);

        // Use function_name_index for O(1) lookup (Issue #3361)
        let indices = self.get_function_indices_by_name(func_name);
        if indices.is_empty() {
            // INTERNAL: closure function name is compiler-assigned; function not found is a compiler bug
            return Err(VmError::InternalError(format!(
                "Function '{}' not found",
                func_name
            )));
        }
        let candidates: Vec<(usize, &FunctionInfo)> =
            indices.iter().map(|&idx| (idx, &self.functions[idx])).collect();

        // Find best matching method based on runtime types
        let func_index =
            self.dispatch_function_variable(func_name, &candidates, &[arg_type_name])?;

        let func = self.get_function_checked(func_index)?.clone();

        // Create frame with captured variables
        let mut frame = Frame::new_with_captures(func.local_slot_count, Some(func_index), captures);

        // Bind argument to first parameter slot
        if let Some(slot) = func.param_slots.first() {
            bind_value_to_slot(&mut frame, *slot, arg, &mut self.struct_heap);
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
        Ok(())
    }

    /// Set up a composed function call: (f ∘ g)(x) = f(g(x))
    /// Calls inner function first, saves outer for after inner returns
    /// Supports nested composition: (a ∘ b ∘ c)(x) = a(b(c(x)))
    fn setup_composed_call(
        &mut self,
        outer: Value,
        inner: Value,
        arg: Value,
    ) -> Result<(), VmError> {
        use super::super::frame::ComposedCallState;

        // Flatten the entire composition: collect all pending outers and find the innermost function
        // This handles both right-associative (a ∘ (b ∘ c)) and left-associative ((a ∘ b) ∘ c) forms
        let mut pending_outers = Vec::new();

        // Helper to flatten a Value, adding callable values to pending_outers in reverse call order
        fn flatten_composition(val: Value, outers: &mut Vec<Value>) -> Result<Value, VmError> {
            match val {
                Value::Function(_) | Value::Closure(_) => Ok(val),
                Value::ComposedFunction(cf) => {
                    // First flatten the outer (it will be called after inner)
                    let flattened_outer = flatten_composition(*cf.outer, outers)?;
                    outers.push(flattened_outer);
                    // Then recursively process inner
                    flatten_composition(*cf.inner, outers)
                }
                _ => Err(VmError::TypeError(format!(
                    "Expected Function or Closure in composition, got {:?}",
                    val
                ))),
            }
        }

        // Flatten outer first
        let flattened_outer = flatten_composition(outer, &mut pending_outers)?;
        pending_outers.push(flattened_outer);

        // Flatten inner to get the innermost callable
        let innermost = flatten_composition(inner, &mut pending_outers)?;

        // Save state with all pending outers
        self.composed_call_state = Some(ComposedCallState {
            pending_outers,
            return_ip: self.ip,
            call_frame_depth: self.frames.len(),
        });

        // Call the innermost function/closure with the argument
        match innermost {
            Value::Function(fv) => {
                self.call_function_by_name_with_arg(&fv.name, arg)?;
            }
            Value::Closure(cv) => {
                self.call_closure_with_arg(&cv.name, cv.captures.clone(), arg)?;
            }
            _ => {
                // INTERNAL: composed call innermost must be Function or Closure; other type is a compiler bug
                return Err(VmError::InternalError(
                    "Expected Function or Closure as innermost".to_string(),
                ))
            }
        }
        Ok(())
    }

    /// Dispatch a function variable call based on runtime argument types.
    /// This fixes Issue #1658 where abstract types like Number incorrectly matched
    /// concrete types like Array because no type checking was done.
    ///
    /// Returns the index of the best matching function, or an error if no method matches.
    pub(super) fn check_type_match(
        &self,
        arg_type_name: &str,
        param_jt: &crate::types::JuliaType,
    ) -> bool {
        use crate::types::JuliaType;

        // Any parameter type matches any argument
        if matches!(param_jt, JuliaType::Any) {
            return true;
        }

        // Get the parameter type name for comparison
        let param_type_name = param_jt.name();

        // Exact match
        if arg_type_name == param_type_name.as_ref() {
            return true;
        }

        // Use the existing check_subtype function for subtype checking
        self.check_subtype(arg_type_name, &param_type_name)
    }

    /// Check if argument type is an exact match (not just a subtype)
    pub(super) fn is_exact_type_match(
        &self,
        arg_type_name: &str,
        param_jt: &crate::types::JuliaType,
    ) -> bool {
        let param_type_name = param_jt.name();
        arg_type_name == param_type_name.as_ref()
    }
}
