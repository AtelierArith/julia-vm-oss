//! Function variable and GlobalRef call instructions.
//!
//! Handles: CallGlobalRef, CallFunctionVariable, CallFunctionVariableWithSplat
//!
//! These instructions handle calling functions stored in variables,
//! GlobalRef-based builtin calls, and function calls with splat arguments.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::intrinsics_exec::{apply_unary_float_op_with_heap, value_to_f64_with_heap};
use super::super::util::resolve_any_type_param;
use super::super::*;
use super::array_basic::array_element_type_from_julia_type;
use super::call::{
    bind_kwargs_defaults, bind_kwargs_with_map, build_specializable_callable_registry,
};
use super::util::bind_value_to_slot;
use super::DispatchAction;
use crate::builtins::BuiltinId;
use crate::inference_core::dispatch_resolver::{
    call_resolutions_differ, call_resolver_compare_enabled, call_resolver_compare_log,
    dispatch_core_type_from_julia, resolve_callable_value_candidates, CallResolutionError,
    CallableValueCandidate, CalleeIdentity, MethodId, ResolvedCall, TypeBindings,
};
use crate::rng::RngLike;
use crate::types::parse_parametric_call;
use crate::types::{JuliaType, TypeExpr};
use crate::vm::hof_exec::state::RuntimeCallableResult;
use crate::vm::specialize;
use crate::vm::splat::{KwargsMap, SplatPreparation};
use crate::vm::value::{
    array_wrapper_value_to_array_value, FunctionValue, GeneratorCallable, GeneratorValue,
    RangeElementType,
};
use std::collections::HashMap;
use std::rc::Rc;
use subset_julia_vm_bytecode::infer_simple_function_return_type_for_value_args;

fn module_path_from_function_name(name: &str) -> Option<String> {
    let base = name.split('#').next().unwrap_or(name);
    base.rsplit_once('.')
        .map(|(module_path, _)| module_path.to_string())
}

/// A resolved function value distinguishes "no frozen resolution" (`None`)
/// from "resolution proved that no methods are eligible" (`Some([])`). The
/// latter is an authoritative empty whitelist: falling back to the function
/// name would silently widen the call surface that the compiler selected.
fn strict_empty_resolved_function_name(value: &Value) -> Option<&str> {
    match value {
        Value::Function(function)
            if function
                .candidate_indices
                .as_ref()
                .is_some_and(Vec::is_empty) =>
        {
            Some(&function.name)
        }
        _ => None,
    }
}

impl<R: RngLike> Vm<R> {
    /// The error upstream raises for calling a value that is not callable —
    /// `z = 5; z(1)` (Issue #11146, corpus row `method_error_noncallable`).
    ///
    /// Verified against julia 1.12.6: `MethodError: objects of type Int64 are
    /// not callable`. sjulia raised `TypeError` ("Expected Function or Closure,
    /// got I64(5)") from four independent call paths — the same
    /// TypeError-vs-MethodError class Issue #10481 closed for `sqrt(::String)`,
    /// re-chosen ad hoc at each site because there was no taxonomy to consult.
    /// All four now funnel through here.
    pub(in crate::vm) fn not_callable_error(&self, func_val: &Value) -> VmError {
        VmError::MethodError(format!(
            "objects of type {} are not callable",
            self.get_type_name(func_val)
        ))
    }

    fn runtime_zip_value(&mut self, values: Vec<Value>) -> Result<Value, VmError> {
        let struct_name = match values.len() {
            2 => "Zip",
            3 => "Zip3",
            4 => "Zip4",
            5 => "Zip5",
            6 => "Zip6",
            7 => "Zip7",
            n => {
                return Err(VmError::MethodError(format!(
                    "Iterators.map with {} iterators is not supported",
                    n
                )))
            }
        };
        let type_id = self
            .struct_defs
            .iter()
            .position(|def| {
                def.name == struct_name
                    || def
                        .name
                        .strip_prefix(struct_name)
                        .is_some_and(|suffix| suffix.starts_with('{'))
            })
            .ok_or_else(|| VmError::TypeError(format!("{} type is not loaded", struct_name)))?;
        let idx = self.struct_heap.len();
        self.struct_heap.push(StructInstance::with_name(
            type_id,
            struct_name.to_string(),
            values,
        ));
        Ok(Value::StructRef(idx))
    }

    fn runtime_generator_from_args(&mut self, args: Vec<Value>) -> Result<Value, VmError> {
        if args.len() < 2 {
            return Err(VmError::MethodError(
                "Generator requires at least 2 arguments".to_string(),
            ));
        }
        let mut args = args;
        let callable = args.remove(0);
        let (generator_callable, iter, result_element_type) = if args.len() == 1 {
            let iter = args.remove(0);
            let (generator_callable, result_element_type) =
                self.runtime_generator_callable_and_eltype(callable, &iter, false, None);
            (generator_callable, iter, result_element_type)
        } else {
            let iter = self.runtime_zip_value(args)?;
            let (generator_callable, result_element_type) =
                self.runtime_generator_callable_and_eltype(callable, &iter, true, None);
            (generator_callable, iter, result_element_type)
        };
        Ok(Value::Generator(Box::new(
            GeneratorValue::with_result_element_type(generator_callable, iter, result_element_type),
        )))
    }

    fn runtime_filter_from_args(&mut self, args: Vec<Value>) -> Result<Value, VmError> {
        if args.len() != 2 {
            return Err(VmError::MethodError(
                "Iterators.filter requires exactly 2 arguments".to_string(),
            ));
        }
        let type_id = self
            .struct_defs
            .iter()
            .position(|def| {
                def.name == "Filter"
                    || def
                        .name
                        .strip_prefix("Filter")
                        .is_some_and(|suffix| suffix.starts_with('{'))
            })
            .ok_or_else(|| VmError::TypeError("Filter type is not loaded".to_string()))?;
        let idx = self.struct_heap.len();
        self.struct_heap.push(StructInstance::with_name(
            type_id,
            "Filter".to_string(),
            args,
        ));
        Ok(Value::StructRef(idx))
    }

    fn runtime_function_lookup_name(func_name: &str) -> &str {
        let short_name = func_name
            .rsplit_once('.')
            .map_or(func_name, |(_, short_name)| short_name);
        match short_name {
            // Macro expansion can reify comparison expressions as function calls
            // to the Unicode operator names (e.g. `@assert N ≥ 2`). Source
            // binary expressions already lower through the ASCII BinaryOp path;
            // runtime callable lookup must normalize the callable spelling too.
            // (Issue #8326)
            "≤" => "<=",
            "≥" => ">=",
            "≠" => "!=",
            _ => short_name,
        }
    }

    /// `true` when `func_name` (or its runtime-normalized spelling) has at
    /// least one `FunctionInfo` registered under it, but none has ever been
    /// published: every matching body still carries the `u64::MAX` sentinel.
    /// This is the hoisted-but-not-yet-active case, most commonly a definition
    /// nested inside an untaken `if`/zero-iteration loop branch (Issue #11320).
    ///
    /// A method published by `@eval` can also be invisible to an older caller
    /// world. That is a different state: the generic binding exists, so a call
    /// from the old world must fall through to `MethodError`, not
    /// `UndefVarError` (Issue #8452).
    ///
    /// This is the single visibility decision the splat, kwargs-splat, and
    /// plain callable-variable dispatch paths all consult before falling
    /// back to a `MethodError`/"not found" `TypeError` — the same decision
    /// `Vm::direct_function_visible_or_raise` (`call.rs`) already applies to
    /// a statically resolved `Instr::Call`/`CallResolved` site (siblings
    /// #11286/#10461 track centralizing this project-wide).
    pub(in crate::vm) fn function_name_exists_only_as_unactivated(&self, func_name: &str) -> bool {
        let world = self.current_dispatch_world();
        let lookup_name = Self::runtime_function_lookup_name(func_name);
        let mut any_unactivated_entry = false;
        for &idx in self
            .get_function_indices_by_name(func_name)
            .iter()
            .chain(self.get_function_indices_by_name(lookup_name).iter())
        {
            if self.function_visible_in_world(idx, world) {
                return false;
            }
            // A definition that HAS been activated (finite `min_world`), just
            // in a world newer than the calling frame's, is upstream's
            // world-age situation: the generic function exists and the call
            // must fall through to ordinary dispatch, which reports
            // `MethodError` ("the applicable method may be too new") —
            // observable via `Base.invokelatest` succeeding on the same name
            // (Issue #8452 fixture). Only a definition that never activated at
            // all (still `u64::MAX`, e.g. hoisted from an untaken branch) is
            // a genuinely undefined name -> `UndefVarError` (Issue #11320).
            match self.functions.get(idx) {
                Some(func) if func.min_world == u64::MAX => {
                    any_unactivated_entry = true;
                }
                Some(_) => return false,
                None => {}
            }
        }
        any_unactivated_entry
    }

    fn try_native_range_unary_accessor_function_value(
        lookup_name: &str,
        args: &[Value],
    ) -> Option<Value> {
        if args.len() != 1 {
            return None;
        }
        let Value::Range(range) = &args[0] else {
            return None;
        };
        match lookup_name {
            "first" => range.first_value(),
            "last" => range.last_value(),
            "step" => Some(range.typed_step()),
            "length" => Some(range.length_value()),
            _ => None,
        }
    }

    fn runtime_callable_has_user_function_name(
        &self,
        candidates: &[(usize, &FunctionInfo)],
        target: &str,
    ) -> bool {
        candidates.iter().any(|(idx, func)| {
            *idx >= self.base_function_count
                && Self::runtime_function_lookup_name(func.name.as_str()) == target
        })
    }

    fn callable_dispatch_type_name(&self, value: &Value) -> String {
        match value {
            // Issue #4580: a DataType value has singleton type `Type{T}` for
            // method dispatch. `get_type_name` returns `DataType`, which is
            // correct for `typeof(T)` display but too coarse for callable
            // method selection such as `_array_undef_from_dims(::Type{T}, ...)`.
            Value::DataType(jt) => format!("Type{{{}}}", jt.name()),
            _ => self.dispatch_julia_type_for_value(value).name().to_string(),
        }
    }

    pub(crate) fn callable_dispatch_type_names(&self, args: &[Value]) -> Vec<String> {
        args.iter()
            .map(|arg| self.callable_dispatch_type_name(arg))
            .collect()
    }

    /// Dispatch name for a callable struct instance.
    ///
    /// Callable methods are registered under the bare type name
    /// (`__callable_Fix1`), but a parametric struct instance carries its full
    /// parametric name (`Fix1{typeof(-), Int64}`). Strip any `{...}` suffix so
    /// parametric callable structs resolve to the same method (Issue #5127).
    ///
    /// A struct defined inside a module also carries a module-qualified
    /// `struct_name` ("M.Foo{Int64}"), while the functor method is registered
    /// under the bare `__callable_Foo` (the lowering uses the unqualified type
    /// name from source). Drop the leading module path from the type head too so
    /// module-defined callable structs resolve to their method (Issue #7185).
    /// Only the head (before the first `{`) is considered, leaving any qualified
    /// type parameters untouched.
    fn callable_method_name(struct_name: &str) -> String {
        let head = struct_name.split('{').next().unwrap_or(struct_name);
        let base = head.rsplit('.').next().unwrap_or(head);
        format!("__callable_{}", base)
    }

    fn callable_method_names_for_struct(&self, struct_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut current = Some(struct_name.to_string());
        while let Some(type_name) = current {
            let callable_name = Self::callable_method_name(&type_name);
            if !names.contains(&callable_name) {
                names.push(callable_name);
            }

            current = self
                .struct_hierarchy
                .parent_for(&type_name)
                .and_then(|parent| parent)
                .filter(|parent| parent != &type_name);
        }
        names
    }

    /// Does `info` declare a bound-form callable receiver (`(self::Type)(args)`)?
    ///
    /// Ground-truth structural check, not a guess from the candidate's
    /// parameter *type* or from call-site arity. Lowering
    /// (`parse_callable_self_param`) unambiguously knows at definition time
    /// whether a method was written `(self::Type)(...)` (bound) or
    /// `(::Type)(...)` (anonymous), and marks the synthesized receiver
    /// parameter's *name* with `CALLABLE_SELF_BOUND_MARKER` accordingly; that
    /// marker survives compilation into `FunctionInfo::params[0].0`
    /// unchanged (`FunctionInfo::callable_binds_self`).
    ///
    /// Earlier heuristics guessed instead: call-site arity alone is ambiguous
    /// for vararg candidates — `(::Foo)(a, xs...)` (anonymous) and
    /// `(self::Foo)(xs...)` (bound) both present the same `fixed = 1` /
    /// `vararg_param_index = Some(1)` shape (Issue #11386) — and comparing
    /// the first parameter's *type* against the method's own
    /// `__callable_<TypeName>` registration suffix misclassifies an
    /// anonymous-form method whose own first parameter happens to be
    /// annotated with the struct's own type, e.g. `(::B)(x::B, xs...)`
    /// (Issue #11553).
    fn callable_candidate_binds_self(info: &FunctionInfo) -> bool {
        info.callable_binds_self()
    }

    /// Bound callable structs `(self::Type)(args)` register a `__callable_<Type>`
    /// method whose first parameter is the struct instance. The runtime must
    /// prepend that instance to the call arguments so it binds to `self`,
    /// enabling field access like `f.f(f.x, arg)` (Issue #5127).
    ///
    /// Anonymous callable structs `(::Type)(args)` have no such leading
    /// parameter, so their methods match the bare argument count and no
    /// prepend happens. Whether a candidate is bound-form is determined
    /// structurally by `callable_candidate_binds_self` (not guessed from
    /// arity — see its doc comment for why arity alone is ambiguous for
    /// vararg candidates, Issue #11386); the supplied argument count is then
    /// used only to check applicability once the receiver's presence is
    /// known, accounting for the receiver occupying one fixed slot in the
    /// bound form (including bound-form vararg overloads).
    fn callable_struct_needs_self(
        &self,
        candidates: &[(usize, &FunctionInfo)],
        args_len: usize,
    ) -> bool {
        let mut needs_self = false;
        for (_, info) in candidates {
            let fixed = match info.vararg_param_index {
                Some(idx) => idx, // params before the vararg slot
                None => info.params.len(),
            };
            if Self::callable_candidate_binds_self(info) {
                // Bound form: the receiver occupies the first fixed slot, so
                // the visible call arguments must supply exactly one fewer
                // (fixed-arity) or leave room for it ahead of the vararg tail.
                if info.vararg_param_index.is_none() {
                    if fixed == args_len + 1 {
                        needs_self = true;
                    }
                } else if args_len + 1 >= fixed {
                    needs_self = true;
                }
                continue;
            }
            // Anonymous form: matches the bare argument count directly, so a
            // candidate that already accepts `args_len` visible arguments
            // never needs the receiver prepended.
            if info.vararg_param_index.is_none() {
                if fixed == args_len {
                    return false;
                }
            } else if args_len >= fixed {
                return false;
            }
        }
        needs_self
    }

    fn unary_type_constructor_builtin_name(builtin_id: BuiltinId) -> Option<&'static str> {
        match builtin_id {
            BuiltinId::BigInt => Some("BigInt"),
            BuiltinId::BigFloat => Some("BigFloat"),
            BuiltinId::Int8 => Some("Int8"),
            BuiltinId::Int16 => Some("Int16"),
            BuiltinId::Int32 => Some("Int32"),
            BuiltinId::Int64 => Some("Int64"),
            BuiltinId::Int128 => Some("Int128"),
            BuiltinId::UInt8 => Some("UInt8"),
            BuiltinId::UInt16 => Some("UInt16"),
            BuiltinId::UInt32 => Some("UInt32"),
            BuiltinId::UInt64 => Some("UInt64"),
            BuiltinId::UInt128 => Some("UInt128"),
            BuiltinId::Float16 => Some("Float16"),
            BuiltinId::Float32 => Some("Float32"),
            BuiltinId::Float64 => Some("Float64"),
            _ => None,
        }
    }

    pub(in crate::vm::exec) fn execute_runtime_builtin_immediate(
        &mut self,
        builtin_id: BuiltinId,
        func_name: &str,
        args: &[Value],
    ) -> Result<Option<Value>, VmError> {
        if let Some(default_name) = Self::unary_type_constructor_builtin_name(builtin_id) {
            if args.len() != 1 {
                let display_name = if matches!(func_name, "Int") {
                    "Int"
                } else {
                    default_name
                };
                let arg_type_names = args
                    .iter()
                    .map(|arg| self.get_type_name(arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.raise(VmError::MethodError(format!(
                    "no method matching {}({})",
                    display_name, arg_type_names
                )))?;
                return Ok(None);
            }
        }

        let saved_stack_len = self.stack.len();
        for arg in args {
            self.stack.push(arg.clone());
        }
        if let Err(err) = self.execute_builtin(builtin_id, args.len()) {
            self.raise(err)?;
            return Ok(None);
        }
        if self.stack.len() == saved_stack_len {
            // Some side-effecting builtins (`print`, `println`, `sleep`, ...)
            // consume their arguments without pushing a result in normal
            // bytecode form. Runtime callable dispatch still needs a Julia
            // return value for eval/HOF callers, so surface `nothing` here
            // instead of popping an unrelated caller value (Issue #8373).
            return Ok(Some(Value::Nothing));
        }
        self.stack.pop_value().map(Some)
    }

    fn collect_function_variable_candidates<'a>(
        &'a self,
        func_name: &str,
    ) -> Vec<(usize, &'a FunctionInfo)> {
        self.collect_function_variable_candidates_for_names([func_name])
    }

    fn collect_function_variable_candidates_for_names<I, S>(
        &self,
        func_names: I,
    ) -> Vec<(usize, &FunctionInfo)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.collect_function_variable_candidates_for_names_with_visibility(func_names, true)
    }

    fn collect_function_variable_candidates_for_names_with_visibility<I, S>(
        &self,
        func_names: I,
        honor_world: bool,
    ) -> Vec<(usize, &FunctionInfo)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut candidates = Vec::new();
        for func_name in func_names {
            self.collect_function_variable_candidates_into(
                func_name.as_ref(),
                &mut candidates,
                honor_world,
            );
        }
        candidates
    }

    fn collect_function_variable_candidates_into<'a>(
        &'a self,
        func_name: &str,
        candidates: &mut Vec<(usize, &'a FunctionInfo)>,
        honor_world: bool,
    ) {
        let world = self.current_dispatch_world();
        let exact_indices = self.get_function_indices_by_name(func_name);
        if func_name.contains('.') && !exact_indices.is_empty() {
            for &idx in exact_indices.iter().rev() {
                if (!honor_world || self.function_visible_in_world(idx, world))
                    && !candidates
                        .iter()
                        .any(|(existing_idx, _)| *existing_idx == idx)
                {
                    candidates.push((idx, self.functions[idx].as_ref()));
                }
            }
            return;
        }

        let lookup_names = [
            Some(func_name),
            Some(Self::runtime_function_lookup_name(func_name)),
        ];
        for name in lookup_names.into_iter().flatten() {
            for &idx in self.get_function_indices_by_name(name).iter().rev() {
                if (!honor_world || self.function_visible_in_world(idx, world))
                    && !candidates
                        .iter()
                        .any(|(existing_idx, _)| *existing_idx == idx)
                {
                    candidates.push((idx, self.functions[idx].as_ref()));
                }
            }
        }
    }

    fn type_heads_match(left: &str, right: &str) -> bool {
        Self::runtime_function_lookup_name(left) == Self::runtime_function_lookup_name(right)
    }

    fn constructor_type_heads_match(left: &str, right: &str) -> bool {
        if left.contains('.') || right.contains('.') {
            left == right
        } else {
            Self::type_heads_match(left, right)
        }
    }

    fn constructor_self_pattern_matches(
        &self,
        patterns: &[TypeExpr],
        actuals: &[TypeExpr],
        func: &FunctionInfo,
    ) -> bool {
        if patterns.len() != actuals.len() {
            return false;
        }
        let binder_names: Vec<&str> = func
            .type_params
            .iter()
            .map(|param| param.name.as_str())
            .collect();
        let mut bindings: HashMap<String, JuliaType> = HashMap::new();
        fn matches(
            pattern: &TypeExpr,
            actual: &TypeExpr,
            binder_names: &[&str],
            bindings: &mut HashMap<String, JuliaType>,
        ) -> bool {
            match pattern {
                TypeExpr::TypeVar(name) if binder_names.contains(&name.as_str()) => {
                    let actual_type = JuliaType::from_name_or_struct(&actual.to_string());
                    if let Some(existing) = bindings.get(name) {
                        existing == &actual_type
                    } else {
                        bindings.insert(name.clone(), actual_type);
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
                        && params.iter().zip(actual_params).all(|(pattern, actual)| {
                            matches(pattern, actual, binder_names, bindings)
                        })
                }
                _ => pattern.to_string() == actual.to_string(),
            }
        }
        let matched = patterns.iter().zip(actuals).all(|(pattern, actual)| {
            let pattern = pattern.canonicalize_constructor_array_aliases();
            let actual = actual.canonicalize_constructor_array_aliases();
            matches(&pattern, &actual, &binder_names, &mut bindings)
        });
        matched
            && bindings.iter().all(|(name, actual)| {
                self.static_type_binding_satisfies_declared_bounds(
                    name,
                    actual,
                    &bindings,
                    &func.type_params,
                )
            })
    }

    fn constructor_actual_julia_type(expr: &TypeExpr) -> JuliaType {
        JuliaType::from_name_or_struct(&expr.to_string())
    }

    fn datatype_dispatch_surface(datatype: &JuliaType) -> String {
        // Runtime constructor registries still use canonical Julia surface
        // spellings. Keep that compatibility projection at one named boundary
        // until the registry accepts structured type identity (Issue #11241).
        datatype.to_string()
    }

    fn call_resolver_callee_identity(func_name: &str) -> CalleeIdentity {
        let Some((base, params)) = parse_parametric_call(func_name) else {
            return CalleeIdentity::from_function_name(func_name);
        };
        let ty = JuliaType::RuntimeParametric {
            base,
            params: params
                .iter()
                .map(Self::constructor_actual_julia_type)
                .collect(),
        };
        CalleeIdentity::Constructor {
            ty: dispatch_core_type_from_julia(&ty),
        }
    }

    fn collect_parametric_datatype_callable_candidates_into<'a>(
        &'a self,
        type_name: &str,
        candidates: &mut Vec<(usize, &'a FunctionInfo)>,
        honor_world: bool,
    ) {
        let Some((runtime_base, runtime_args)) = parse_parametric_call(type_name) else {
            return;
        };
        let world = self.current_dispatch_world();
        for (idx, func) in self.functions.iter().enumerate() {
            if honor_world && !self.function_visible_in_world(idx, world) {
                continue;
            }
            let Some((candidate_base, candidate_args)) = parse_parametric_call(&func.name) else {
                continue;
            };
            if candidate_args.len() != runtime_args.len()
                || !Self::constructor_type_heads_match(&candidate_base, &runtime_base)
                || !self.constructor_self_pattern_matches(&candidate_args, &runtime_args, func)
            {
                continue;
            }
            if !candidates
                .iter()
                .any(|(existing_idx, _)| *existing_idx == idx)
            {
                candidates.push((idx, func.as_ref()));
            }
        }
    }

    fn collect_frozen_callable_candidates<'a>(
        &'a self,
        name: &str,
        candidate_indices: Option<&[usize]>,
    ) -> Vec<(usize, &'a FunctionInfo)> {
        if let Some(indices) = candidate_indices {
            if indices.is_empty() {
                return Vec::new();
            }
            let hinted_candidates: Vec<(usize, &'a FunctionInfo)> = indices
                .iter()
                .filter_map(|&idx| self.functions.get(idx).map(|func| (idx, func.as_ref())))
                .collect();
            let mut hinted_names: Vec<String> = Vec::new();
            for (_, func) in &hinted_candidates {
                if !hinted_names.contains(&func.name) {
                    hinted_names.push(func.name.clone());
                }
            }
            if hinted_names.is_empty() {
                hinted_names.push(name.to_string());
            }
            let mut candidates = Vec::new();
            for name in hinted_names {
                self.collect_function_variable_candidates_into(&name, &mut candidates, true);
            }
            if let Some(helper_provenance) = hinted_candidates
                .first()
                .map(|(_, function)| function.is_lowering_helper)
            {
                if hinted_candidates
                    .iter()
                    .all(|(_, function)| function.is_lowering_helper == helper_provenance)
                {
                    candidates
                        .retain(|(_, function)| function.is_lowering_helper == helper_provenance);
                } else {
                    // A frozen candidate set must never bridge the private
                    // lowering-helper and Julia-visible source namespaces.
                    candidates.clear();
                }
            }
            for (idx, func) in hinted_candidates {
                if !candidates
                    .iter()
                    .any(|(existing_idx, _)| *existing_idx == idx)
                {
                    candidates.push((idx, func));
                }
            }
            return candidates;
        }

        self.collect_function_variable_candidates(name)
            .into_iter()
            .filter(|(_, candidate)| !candidate.is_lowering_helper)
            .collect()
    }

    fn collect_function_value_candidates<'a>(
        &'a self,
        function: &FunctionValue,
    ) -> Vec<(usize, &'a FunctionInfo)> {
        self.collect_frozen_callable_candidates(
            &function.name,
            function.candidate_indices.as_deref(),
        )
    }

    pub(in crate::vm) fn collect_runtime_callable_candidates<'a>(
        &'a self,
        func_val: &Value,
        func_name: &str,
    ) -> Result<Vec<(usize, &'a FunctionInfo)>, VmError> {
        match func_val {
            Value::Function(fv) => Ok(self.collect_function_value_candidates(fv)),
            Value::Closure(cv) => {
                Ok(self
                    .collect_frozen_callable_candidates(func_name, cv.candidate_indices.as_deref()))
            }
            Value::Struct(si) => Ok(self.collect_function_variable_candidates_for_names(
                self.callable_method_names_for_struct(&si.struct_name),
            )),
            Value::StructRef(idx) => {
                let si = self.struct_heap.get(*idx).ok_or_else(|| {
                    VmError::TypeError(format!(
                        "Invalid struct reference: index {} out of bounds",
                        idx
                    ))
                })?;
                Ok(self.collect_function_variable_candidates_for_names(
                    self.callable_method_names_for_struct(&si.struct_name),
                ))
            }
            Value::DataType(jt) => {
                // A qualified type object keeps its declaring-module identity.
                // Falling back from `A.T` to the bare `T` can select an unrelated
                // function declared by another module (for example a facade
                // forwarder), turning a default constructor call into recursion.
                // Qualified outer constructors are registered under the exact
                // name; when there is no such method, leave the list empty so the
                // caller can use the default-field constructor (Issue #11242).
                let mut candidates = if func_name.contains('.') {
                    let world = self.current_dispatch_world();
                    let exact: Vec<_> = self
                        .get_function_indices_by_name(func_name)
                        .iter()
                        .rev()
                        .filter(|&&idx| self.function_visible_in_world(idx, world))
                        .map(|&idx| (idx, self.functions[idx].as_ref()))
                        .collect();
                    if exact.is_empty() {
                        // Inner constructors can retain a bare registry name even
                        // when their DataType is owner-qualified. Admit only bare
                        // methods proven by their inferred return type to construct
                        // this exact owner-qualified type; a same-named facade
                        // forwarder returning `Any` is not constructor evidence.
                        self.collect_function_variable_candidates(func_name)
                            .into_iter()
                            .filter(|(_, func)| {
                                self.function_returns_datatype(func, jt.as_ref())
                                    || self.function_is_inner_constructor_for_datatype(
                                        func,
                                        jt.as_ref(),
                                    )
                            })
                            .collect()
                    } else {
                        exact
                    }
                } else {
                    self.collect_function_variable_candidates(func_name)
                };
                self.collect_parametric_datatype_callable_candidates_into(
                    &Self::datatype_dispatch_surface(jt),
                    &mut candidates,
                    true,
                );
                Ok(candidates)
            }
            _ => Ok(self.collect_function_variable_candidates(func_name)),
        }
    }

    fn function_returns_datatype(&self, func: &FunctionInfo, datatype: &JuliaType) -> bool {
        let datatype_name = Self::datatype_dispatch_surface(datatype);
        let expected = super::super::util::extract_base_type(&datatype_name);
        let returned_name = match func.return_type {
            ValueType::Struct(type_id) => self.struct_defs.get(type_id).map(|def| def.name.clone()),
            _ => func
                .return_julia_type
                .as_ref()
                .map(Self::datatype_dispatch_surface),
        };
        returned_name.is_some_and(|name| super::super::util::extract_base_type(&name) == expected)
    }

    fn function_is_inner_constructor_for_datatype(
        &self,
        func: &FunctionInfo,
        datatype: &JuliaType,
    ) -> bool {
        let datatype_name = Self::datatype_dispatch_surface(datatype);
        let expected = super::super::util::extract_base_type(&datatype_name);
        let bare = expected.rsplit('.').next().unwrap_or(expected);
        if func.name.contains('.') || func.name != bare {
            return false;
        }

        let parametric_inner =
            self.resolve_runtime_parametric_def(expected)
                .is_some_and(|(canonical, def)| {
                    super::super::util::extract_base_type(&canonical) == expected
                        && !def.inner_constructors.is_empty()
                });
        parametric_inner
    }

    fn bind_callable_datatype_type_expr(
        &self,
        pattern: &TypeExpr,
        actual: &TypeExpr,
        func: &FunctionInfo,
        frame: &mut super::super::frame::Frame,
    ) {
        match (pattern, actual) {
            (TypeExpr::TypeVar(name), _) if func.type_params.iter().any(|tp| tp.name == *name) => {
                frame
                    .type_bindings
                    .insert(name.clone(), Self::constructor_actual_julia_type(actual));
            }
            (
                TypeExpr::Parameterized {
                    base: pattern_base,
                    params: pattern_params,
                },
                TypeExpr::Parameterized {
                    base: actual_base,
                    params: actual_params,
                },
            ) if pattern_params.len() == actual_params.len()
                && Self::type_heads_match(pattern_base, actual_base) =>
            {
                for (pattern_param, actual_param) in pattern_params.iter().zip(actual_params) {
                    self.bind_callable_datatype_type_expr(pattern_param, actual_param, func, frame);
                }
            }
            _ => {}
        }
    }

    fn bind_callable_datatype_type_params(
        &self,
        callable_type: &JuliaType,
        func: &FunctionInfo,
        frame: &mut super::super::frame::Frame,
    ) {
        if func.type_params.is_empty() {
            return;
        }
        let Some((callable_base, callable_args)) = parse_parametric_call(&callable_type.name())
        else {
            return;
        };
        let Some((method_base, method_args)) = parse_parametric_call(&func.name) else {
            return;
        };
        if callable_args.len() != method_args.len()
            || !Self::type_heads_match(&callable_base, &method_base)
        {
            return;
        }
        for (method_arg, callable_arg) in method_args.iter().zip(callable_args.iter()) {
            let method_arg = method_arg.canonicalize_constructor_array_aliases();
            let callable_arg = callable_arg.canonicalize_constructor_array_aliases();
            self.bind_callable_datatype_type_expr(&method_arg, &callable_arg, func, frame);
        }
    }

    fn prefer_candidates_declaring_kwargs<'a>(
        candidates: &[(usize, &'a FunctionInfo)],
        kwargs_map: &KwargsMap<Value>,
    ) -> Vec<(usize, &'a FunctionInfo)> {
        if kwargs_map.is_empty() {
            return candidates.to_vec();
        }

        // Keyword presence filters out methods that cannot accept the supplied
        // names, but it must not outrank positional dispatch. A `kwargs...`
        // method can be the most-specific positional match even when a fallback
        // declares the keyword explicitly (Issue #8396).
        let mut accepted = Vec::new();
        for &(idx, func) in candidates {
            let accepts_all_kwargs = kwargs_map.keys().all(|key| {
                func.kwparams
                    .iter()
                    .any(|kwparam| kwparam.is_varargs || &kwparam.name == key)
            });
            if accepts_all_kwargs {
                accepted.push((idx, func));
            }
        }

        if accepted.is_empty() {
            candidates.to_vec()
        } else {
            accepted
        }
    }

    fn runtime_iterator_element_type(&self, iter: &Value) -> Option<ValueType> {
        if let Some(arr) = super::super::value::native_array_value_ref(iter) {
            return Some(arr.borrow().element_type().to_value_type());
        }
        if let Ok(Some(arr)) = array_wrapper_value_to_array_value(iter, &self.struct_heap) {
            return Some(arr.element_type().to_value_type());
        }
        match iter {
            Value::Memory(mem) => Some(mem.borrow().element_type().to_value_type()),
            Value::Range(range) => Some(match range.element_type {
                RangeElementType::Int8 => ValueType::I8,
                RangeElementType::Int16 => ValueType::I16,
                RangeElementType::Int32 => ValueType::I32,
                RangeElementType::Int64 => ValueType::I64,
                RangeElementType::UInt8 => ValueType::U8,
                RangeElementType::UInt16 => ValueType::U16,
                RangeElementType::UInt32 => ValueType::U32,
                RangeElementType::UInt64 => ValueType::U64,
                RangeElementType::Float16 => ValueType::F16,
                RangeElementType::Float32 => ValueType::F32,
                RangeElementType::Float64 => ValueType::F64,
                RangeElementType::Char => ValueType::Char,
                RangeElementType::BigInt => ValueType::BigInt,
                RangeElementType::Default if range.is_float => ValueType::F64,
                RangeElementType::Default => ValueType::I64,
            }),
            Value::Tuple(tuple) => {
                let first = tuple.elements.first()?;
                let first_type = self.get_value_type(first);
                if tuple
                    .elements
                    .iter()
                    .all(|value| self.get_value_type(value) == first_type)
                {
                    Some(first_type)
                } else {
                    Some(ValueType::Any)
                }
            }
            _ => None,
        }
    }

    pub(in crate::vm) fn runtime_generator_arg_types(
        &self,
        iter: &Value,
        tuple_splat: bool,
    ) -> Option<Vec<ValueType>> {
        if !tuple_splat {
            return self.runtime_iterator_element_type(iter).map(|ty| vec![ty]);
        }

        let Value::StructRef(idx) = iter else {
            return None;
        };
        let zip = self.struct_heap.get(*idx)?;
        if !(&*zip.struct_name == "Zip"
            || &*zip.struct_name == "Zip3"
            || &*zip.struct_name == "Zip4"
            || &*zip.struct_name == "Zip5"
            || &*zip.struct_name == "Zip6"
            || &*zip.struct_name == "Zip7"
            || zip.struct_name.starts_with("Zip{")
            || zip.struct_name.starts_with("Zip3{")
            || zip.struct_name.starts_with("Zip4{")
            || zip.struct_name.starts_with("Zip5{")
            || zip.struct_name.starts_with("Zip6{")
            || zip.struct_name.starts_with("Zip7{"))
        {
            return None;
        }

        zip.values
            .iter()
            .map(|value| self.runtime_iterator_element_type(value))
            .collect()
    }

    fn runtime_generator_function_value_index(
        &self,
        function: &FunctionValue,
        arg_types: &[ValueType],
    ) -> Option<usize> {
        let arg_type_names: Vec<String> = arg_types
            .iter()
            .map(|ty| Self::runtime_generator_value_type_name(ty).to_string())
            .collect();
        let candidates = self.collect_function_value_candidates(function);
        self.dispatch_function_variable(&function.name, &candidates, &arg_type_names)
            .ok()
    }

    fn runtime_generator_value_type_name(ty: &ValueType) -> &'static str {
        match ty {
            ValueType::I8 => "Int8",
            ValueType::I16 => "Int16",
            ValueType::I32 => "Int32",
            ValueType::I64 => "Int64",
            ValueType::I128 => "Int128",
            ValueType::U8 => "UInt8",
            ValueType::U16 => "UInt16",
            ValueType::U32 => "UInt32",
            ValueType::U64 => "UInt64",
            ValueType::U128 => "UInt128",
            ValueType::Bool => "Bool",
            ValueType::F16 => "Float16",
            ValueType::F32 => "Float32",
            ValueType::F64 => "Float64",
            ValueType::ComplexF32 => "Complex{Float32}",
            ValueType::ComplexF64 => "Complex{Float64}",
            ValueType::BigInt => "BigInt",
            ValueType::BigFloat => "BigFloat",
            ValueType::Str => "String",
            ValueType::Char => "Char",
            ValueType::Symbol => "Symbol",
            _ => "Any",
        }
    }

    fn runtime_generator_specialized_return_type(
        &self,
        func_index: usize,
        arg_types: &[ValueType],
    ) -> Option<ValueType> {
        let struct_defs = &self.struct_defs;
        let type_object_names = specialize::collect_type_object_names(
            &self.struct_defs,
            self.compile_context.as_ref(),
            &self.abstract_types,
        );
        let disable_array_index = self.disable_array_getindex_specialization();
        let disable_field_access = self.disable_field_access_specialization();
        let module_path = self
            .functions
            .get(func_index)
            .and_then(|func| module_path_from_function_name(&func.name));
        let callable_registry =
            build_specializable_callable_registry(&self.functions, &self.specializable_functions);
        let recursion_guard =
            std::cell::RefCell::new(specialize::SpecializationRecursionGuard::new());
        self.specializable_functions
            .iter()
            .position(|func| func.fallback_index == func_index)
            .and_then(|spec_idx| {
                let func = &self.specializable_functions[spec_idx];
                specialize::specialize_function_with_callees(
                    &func.ir,
                    arg_types,
                    struct_defs,
                    &type_object_names,
                    module_path.as_deref(),
                    disable_array_index,
                    disable_field_access,
                    &callable_registry,
                    &recursion_guard,
                    Some(spec_idx),
                )
                .ok()
            })
            .map(|result| result.return_type)
    }

    fn runtime_generator_result_eltype_for_function(
        &self,
        func_index: usize,
        arg_types: &[ValueType],
    ) -> Option<ArrayElementType> {
        let return_type = self
            .runtime_generator_specialized_return_type(func_index, arg_types)
            .filter(|return_type| !matches!(return_type, ValueType::Any))
            .or_else(|| {
                self.specializable_functions
                    .iter()
                    .find(|func| func.fallback_index == func_index)
                    .and_then(|func| {
                        infer_simple_function_return_type_for_value_args(&func.ir, arg_types)
                    })
            })
            .or_else(|| {
                self.functions
                    .get(func_index)
                    .map(|func| func.return_type.clone())
            })?;
        if matches!(return_type, ValueType::Any) {
            None
        } else {
            Some(ArrayElementType::from_value_type(&return_type))
        }
    }

    fn runtime_generator_arg_types_are_specializable(arg_types: &[ValueType]) -> bool {
        arg_types
            .iter()
            .all(|arg_type| !matches!(arg_type, ValueType::Any))
    }

    pub(super) fn runtime_generator_callable_and_eltype(
        &self,
        callable: Value,
        iter: &Value,
        tuple_splat: bool,
        result_element_type: Option<ArrayElementType>,
    ) -> (GeneratorCallable, Option<ArrayElementType>) {
        match callable {
            Value::DataType(jt) if !tuple_splat => {
                let element_type =
                    result_element_type.or_else(|| Some(array_element_type_from_julia_type(&jt)));
                (
                    GeneratorCallable::RuntimeValue(Box::new(Value::DataType(jt))),
                    element_type,
                )
            }
            Value::DataType(jt) => {
                let element_type = result_element_type.or_else(|| {
                    let arg_count = self
                        .runtime_generator_arg_types(iter, true)
                        .map(|arg_types| arg_types.len())
                        .unwrap_or(0);
                    let type_name = jt.name();
                    let has_matching_struct_constructor = self.struct_defs.iter().any(|def| {
                        def.fields.len() == arg_count
                            && (def.name == type_name
                                || type_name
                                    .split_once('{')
                                    .is_some_and(|(base, _)| def.name == base))
                    });
                    if has_matching_struct_constructor {
                        Some(array_element_type_from_julia_type(&jt))
                    } else {
                        Some(ArrayElementType::UnionOf(Vec::new()))
                    }
                });
                (
                    GeneratorCallable::TupleSplatRuntimeValue(Box::new(Value::DataType(jt))),
                    element_type,
                )
            }
            Value::Function(fv) => {
                if let Some(arg_types) = self.runtime_generator_arg_types(iter, tuple_splat) {
                    if Self::runtime_generator_arg_types_are_specializable(&arg_types) {
                        if let Some(func_index) =
                            self.runtime_generator_function_value_index(&fv, &arg_types)
                        {
                            let inferred_element_type = || {
                                self.runtime_generator_result_eltype_for_function(
                                    func_index, &arg_types,
                                )
                            };
                            let element_type = if matches!(
                                result_element_type,
                                None | Some(ArrayElementType::Any)
                            ) {
                                inferred_element_type().or(result_element_type.clone())
                            } else {
                                result_element_type.clone()
                            };
                            let callable = if tuple_splat {
                                GeneratorCallable::TupleSplatFunctionIndex(func_index)
                            } else {
                                GeneratorCallable::FunctionIndex(func_index)
                            };
                            return (callable, element_type);
                        }
                    }
                }

                let callable = if tuple_splat {
                    GeneratorCallable::TupleSplatRuntimeValue(Box::new(Value::Function(fv)))
                } else {
                    GeneratorCallable::RuntimeValue(Box::new(Value::Function(fv)))
                };
                (callable, result_element_type)
            }
            other => {
                let callable = if tuple_splat {
                    GeneratorCallable::TupleSplatRuntimeValue(Box::new(other))
                } else {
                    GeneratorCallable::RuntimeValue(Box::new(other))
                };
                (callable, result_element_type)
            }
        }
    }

    /// Resolve a plain function value to the method index the runtime
    /// dispatcher would pick for `args`, WITHOUT calling it (Issue #8797).
    /// Used by the broadcast typed-kernel bulk path to dispatch once per
    /// broadcast instead of once per element. Conservative: anything beyond a
    /// named Function/Closure value. Composed functions, callable structs, and
    /// builtin/intrinsic fallbacks return `None` and the caller keeps its
    /// generic path.
    pub(in crate::vm) fn resolve_runtime_callable_function_index(
        &mut self,
        func_val: &Value,
        args: &[Value],
    ) -> Option<usize> {
        let func_name = match func_val {
            Value::Function(fv) => fv.name.clone(),
            Value::Closure(cv) => cv.name.clone(),
            _ => return None,
        };
        let candidates = self
            .collect_runtime_callable_candidates(func_val, &func_name)
            .ok()?;
        if candidates.is_empty() {
            return None;
        }
        let arg_type_names = self.callable_dispatch_type_names(args);
        self.dispatch_function_variable_for_values(&func_name, &candidates, &arg_type_names, args)
            .ok()
            .flatten()
    }

    pub(crate) fn call_runtime_callable_value(
        &mut self,
        func_val: Value,
        mut args: Vec<Value>,
    ) -> Result<RuntimeCallableResult, VmError> {
        if let Some(func_name) = strict_empty_resolved_function_name(&func_val) {
            let arg_type_names = self.callable_dispatch_type_names(&args);
            self.raise(VmError::MethodError(format!(
                "no method matching {}({})",
                func_name,
                arg_type_names.join(", ")
            )))?;
            return Ok(RuntimeCallableResult::Raised);
        }

        if let Value::Function(fv) = &func_val {
            if matches!(fv.name.as_str(), "Iterators.map" | "Base.Iterators.map") {
                return Ok(RuntimeCallableResult::Immediate(
                    self.runtime_generator_from_args(args)?,
                ));
            }
            if matches!(
                fv.name.as_str(),
                "Iterators.filter" | "Base.Iterators.filter"
            ) {
                return Ok(RuntimeCallableResult::Immediate(
                    self.runtime_filter_from_args(args)?,
                ));
            }
            if matches!(fv.name.as_str(), "Generator" | "Base.Generator") {
                return Ok(RuntimeCallableResult::Immediate(
                    self.runtime_generator_from_args(args)?,
                ));
            }
        }

        if let Value::ComposedFunction(cf) = &func_val {
            self.setup_composed_call(cf.outer.as_ref().clone(), cf.inner.as_ref().clone(), args)?;
            return Ok(RuntimeCallableResult::StartedFrame);
        }

        let (func_name, closure_captures) = match &func_val {
            Value::Function(fv) => (fv.name.clone(), None),
            Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
            Value::DataType(jt) => (jt.name().to_string(), None),
            Value::Struct(si) => (Self::callable_method_name(&si.struct_name), None),
            Value::StructRef(idx) => {
                let si = self.struct_heap.get(*idx).ok_or_else(|| {
                    VmError::TypeError(format!(
                        "Invalid struct reference: index {} out of bounds",
                        idx
                    ))
                })?;
                (Self::callable_method_name(&si.struct_name), None)
            }
            _ => return Err(self.not_callable_error(&func_val)),
        };

        let lookup_name = Self::runtime_function_lookup_name(&func_name);
        let native_range_value =
            Self::try_native_range_unary_accessor_function_value(lookup_name, &args);
        if let Some(value) = native_range_value {
            let candidates = self.collect_runtime_callable_candidates(&func_val, &func_name)?;
            if !self.runtime_callable_has_user_function_name(&candidates, lookup_name) {
                return Ok(RuntimeCallableResult::Immediate(value));
            }
        }

        let candidates = self.collect_runtime_callable_candidates(&func_val, &func_name)?;

        // Bound callable struct `(self::Type)(args)`: prepend the struct instance
        // so it binds to `self` (Issue #5127). Candidate lookup includes abstract
        // parent callable methods so parent-declared functors dispatch too (Issue
        // #8264).
        if matches!(&func_val, Value::Struct(_) | Value::StructRef(_))
            && self.callable_struct_needs_self(&candidates, args.len())
        {
            args.insert(0, func_val.clone());
        }

        let arg_type_names = self.callable_dispatch_type_names(&args);

        if candidates.is_empty() {
            // Same centralized visibility decision as the other call paths
            // (Issue #11320; siblings #11286/#10461): a hoisted-but-not-yet-
            // active top-level/inline definition raises `UndefVarError`, not
            // a "not found" error that would wrongly imply the generic
            // function itself already exists.
            if matches!(&func_val, Value::Function(_))
                && self.function_name_exists_only_as_unactivated(&func_name)
            {
                self.raise(VmError::UndefVarError(func_name))?;
                return Ok(RuntimeCallableResult::Raised);
            }
            if let Value::DataType(_) = &func_val {
                if self.try_construct_default_datatype(&func_name, &args)? {
                    let value = self.stack.pop_value()?;
                    return Ok(RuntimeCallableResult::Immediate(value));
                }
            }
            if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                if let Some(value) =
                    self.execute_runtime_builtin_immediate(builtin_id, &func_name, &args)?
                {
                    return Ok(RuntimeCallableResult::Immediate(value));
                }
                return Ok(RuntimeCallableResult::Raised);
            }
            if let Some(result) = self.try_call_intrinsic(lookup_name, &args)? {
                return Ok(RuntimeCallableResult::Immediate(result));
            }
            return Err(VmError::TypeError(format!(
                "Function '{}' not found",
                func_name
            )));
        }

        let func_index = match self.dispatch_function_variable_for_values(
            &func_name,
            &candidates,
            &arg_type_names,
            &args,
        ) {
            Ok(Some(idx)) => idx,
            Ok(None) => {
                if let Value::DataType(_) = &func_val {
                    if self.try_construct_default_datatype(&func_name, &args)? {
                        let value = self.stack.pop_value()?;
                        return Ok(RuntimeCallableResult::Immediate(value));
                    }
                }
                if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                    if let Some(value) =
                        self.execute_runtime_builtin_immediate(builtin_id, &func_name, &args)?
                    {
                        return Ok(RuntimeCallableResult::Immediate(value));
                    }
                    return Ok(RuntimeCallableResult::Raised);
                }
                if let Some(result) = self.try_call_intrinsic(lookup_name, &args)? {
                    return Ok(RuntimeCallableResult::Immediate(result));
                }
                // Issue #9409: a runtime dispatch miss is a catchable
                // MethodError in Julia — route it through the exception
                // machinery instead of returning a fatal `Err` that
                // escapes try/catch.
                self.raise(VmError::MethodError(format!(
                    "no method matching {}({})",
                    func_name,
                    arg_type_names.join(", ")
                )))?;
                return Ok(RuntimeCallableResult::Raised);
            }
            Err(error) => {
                self.raise(error)?;
                return Ok(RuntimeCallableResult::Raised);
            }
        };

        let func = self.get_function_checked(func_index)?.clone();
        let (target_entry, slot_count) = if closure_captures.is_some() {
            None
        } else {
            self.try_specialized_entry_for_runtime_call(func_index, &args)
        }
        .unwrap_or((func.entry, func.local_slot_count));
        let mut frame = if let Some(captures) = closure_captures {
            self.acquire_frame_with_captures(slot_count, Some(func_index), &captures)
        } else {
            self.acquire_frame(slot_count, Some(func_index))
        };
        self.bind_type_params(&func, &args, &mut frame);
        if let Value::DataType(callable_type) = &func_val {
            self.bind_callable_datatype_type_params(callable_type, &func, &mut frame);
        }

        if let Some(vararg_idx) = func.vararg_param_index {
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: args[vararg_idx..].to_vec(),
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            for (idx, slot) in func.param_slots.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        bind_kwargs_defaults(
            &func,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        if let Some(result) =
            self.try_eval_cached_generated_expr(func_index, &func, &args, &frame)?
        {
            return Ok(RuntimeCallableResult::Immediate(result));
        }

        let generated_eval_frame = func.is_generated.then(|| frame.clone());
        self.bind_generated_body_arg_types(&func, &args, &mut frame);
        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.remember_current_generated_expr_cache_key(
            &func,
            func_index,
            &args,
            generated_eval_frame,
        );
        self.ip = target_entry;
        Ok(RuntimeCallableResult::StartedFrame)
    }

    fn invoke_runtime_callable_value_with_signature(
        &mut self,
        func_val: Value,
        args: Vec<Value>,
        declared_arg_type_names: &[String],
    ) -> Result<RuntimeCallableResult, VmError> {
        self.invoke_runtime_callable_value_with_signature_and_kwargs(
            func_val,
            args,
            declared_arg_type_names,
            &KwargsMap::new(),
        )
    }

    pub(crate) fn invoke_runtime_callable_value_with_signature_and_kwargs(
        &mut self,
        func_val: Value,
        args: Vec<Value>,
        declared_arg_type_names: &[String],
        kwargs_map: &KwargsMap<Value>,
    ) -> Result<RuntimeCallableResult, VmError> {
        let (func_name, closure_captures) = match &func_val {
            Value::Function(fv) => (fv.name.clone(), None),
            Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
            _ => {
                return Err(VmError::TypeError(format!(
                    "invoke expects a Function or Closure, got {:?}",
                    func_val
                )))
            }
        };

        let candidates = self.collect_runtime_callable_candidates(&func_val, &func_name)?;
        if candidates.is_empty() {
            if strict_empty_resolved_function_name(&func_val).is_some() {
                self.raise(VmError::MethodError(format!(
                    "no method matching {}({})",
                    func_name,
                    declared_arg_type_names.join(", ")
                )))?;
                return Ok(RuntimeCallableResult::Raised);
            }
            // Same centralized visibility decision as the other call paths
            // (Issue #11320; siblings #11286/#10461).
            if matches!(&func_val, Value::Function(_))
                && self.function_name_exists_only_as_unactivated(&func_name)
            {
                self.raise(VmError::UndefVarError(func_name))?;
                return Ok(RuntimeCallableResult::Raised);
            }
            return Err(VmError::TypeError(format!(
                "Function '{}' not found",
                func_name
            )));
        }

        let dispatch_candidates = Self::prefer_candidates_declaring_kwargs(&candidates, kwargs_map);
        let func_index = self.dispatch_function_variable_for_declared_signature(
            &func_name,
            &dispatch_candidates,
            declared_arg_type_names,
            &args,
        )?;
        let func = self.get_function_checked(func_index)?.clone();
        let mut frame = if let Some(captures) = closure_captures {
            self.acquire_frame_with_captures(func.local_slot_count, Some(func_index), &captures)
        } else {
            self.acquire_frame(func.local_slot_count, Some(func_index))
        };
        self.bind_type_params(&func, &args, &mut frame);

        if let Some(vararg_idx) = func.vararg_param_index {
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                    }
                }
            }
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: args[vararg_idx..].to_vec(),
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            for (idx, slot) in func.param_slots.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        // Issue #11024: assert supplied keyword arguments against their declared
        // types before binding (upstream raises TypeError, catchably).
        self.check_supplied_kwarg_types(&func, kwargs_map, &frame.type_bindings)?;
        bind_kwargs_with_map(
            &func,
            kwargs_map,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;
        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.ip = func.entry;
        Ok(RuntimeCallableResult::StartedFrame)
    }

    fn runtime_invoke_signature_type_names(sig_val: &Value) -> Result<Vec<String>, VmError> {
        let jt = match sig_val {
            Value::DataType(jt) => jt.as_ref(),
            _ => {
                return Err(VmError::TypeError(format!(
                    "invoke type argument must be a Tuple type, got {:?}",
                    sig_val
                )))
            }
        };

        let types = match jt {
            JuliaType::TupleOf(types) => types,
            JuliaType::Tuple => return Ok(Vec::new()),
            other => {
                return Err(VmError::TypeError(format!(
                    "invoke type argument must be Tuple{{...}}, got {}",
                    other
                )))
            }
        };
        Ok(types.iter().map(|ty| ty.name().to_string()).collect())
    }

    /// Push a call frame for an already-resolved `func_index`, given the
    /// (possibly empty) closure captures and the final positional `args`.
    ///
    /// Extracted from the tail of the `CallFunctionVariable` handler (Issue
    /// #9739) so both the normal candidate-resolution path and the per-call-site
    /// dispatch cache hit path (below) share one frame-setup implementation —
    /// caching the resolved `func_index` must not risk drifting from how a
    /// freshly-resolved call is bound.
    fn dispatch_resolved_function_variable_call(
        &mut self,
        func_index: usize,
        closure_captures: Option<Rc<Vec<(String, Value)>>>,
        args: Vec<Value>,
        callable_datatype: Option<JuliaType>,
    ) -> Result<DispatchAction, VmError> {
        let func = self.get_function_checked(func_index)?.clone();
        let (target_entry, slot_count) = if closure_captures.is_some() {
            None
        } else {
            self.try_specialized_entry_for_runtime_call(func_index, &args)
        }
        .unwrap_or((func.entry, func.local_slot_count));

        let mut frame = if let Some(captures) = closure_captures {
            self.acquire_frame_with_captures(slot_count, Some(func_index), &captures)
        } else {
            self.acquire_frame(slot_count, Some(func_index))
        };

        // Bind type parameters from where clauses (Issue #2468)
        self.bind_type_params(&func, &args, &mut frame);
        if let Some(callable_type) = callable_datatype.as_ref() {
            self.bind_callable_datatype_type_params(callable_type, &func, &mut frame);
        }

        // Bind arguments to parameter slots
        if let Some(vararg_idx) = func.vararg_param_index {
            // Function has varargs
            for idx in 0..vararg_idx {
                if let Some(val) = args.get(idx) {
                    if let Some(slot) = func.param_slots.get(idx) {
                        bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
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
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        // Bind keyword arguments with their defaults.
        // Use bind_kwargs_defaults() so kwargs... varargs get empty Pairs, not Nothing.
        bind_kwargs_defaults(
            &func,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        if let Some(result) =
            self.try_eval_cached_generated_expr(func_index, &func, &args, &frame)?
        {
            self.stack.push(result);
            return Ok(DispatchAction::Continue);
        }

        let generated_eval_frame = func.is_generated.then(|| frame.clone());
        self.bind_generated_body_arg_types(&func, &args, &mut frame);
        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.remember_current_generated_expr_cache_key(
            &func,
            func_index,
            &args,
            generated_eval_frame,
        );
        self.ip = target_entry;
        Ok(DispatchAction::Continue)
    }

    /// Execute function variable and GlobalRef call instructions.
    ///
    /// Returns an `unhandled` error if the instruction is not handled.
    #[inline]
    pub(super) fn execute_call_function_variable(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
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

                // Resolve the GlobalRef to a function name.
                // Format: "module.name" for qualified lookup.
                let func_name = globalref.name.as_str();
                let module_name = &globalref.module;
                let qualified_name = format!("{}.{}", module_name, func_name);

                if module_name == "Base" {
                    if matches!(func_name, "println" | "print" | "string") {
                        if let Some(builtin_id) = BuiltinId::from_name(func_name) {
                            let result_stack_len = self.stack.len();
                            for arg in &args {
                                self.stack.push(arg.clone());
                            }
                            self.execute_builtin(builtin_id, args.len())?;
                            if self.stack.len() == result_stack_len {
                                self.stack.push(Value::Nothing);
                            }
                            return Ok(DispatchAction::Continue);
                        }
                    }

                    match self.call_runtime_callable_value(
                        Value::Function(FunctionValue::new(qualified_name)),
                        args,
                    )? {
                        RuntimeCallableResult::Immediate(value) => {
                            self.stack.push(value);
                        }
                        RuntimeCallableResult::StartedFrame => {}
                        RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                    }
                    return Ok(DispatchAction::Continue);
                }

                let arg_type_names = self.callable_dispatch_type_names(&args);
                let candidates = self.collect_function_variable_candidates(&qualified_name);
                let func_index = if candidates.is_empty() {
                    None
                } else {
                    match self.dispatch_function_variable_for_values(
                        &qualified_name,
                        &candidates,
                        &arg_type_names,
                        &args,
                    ) {
                        Ok(Some(idx)) => Some(idx),
                        Ok(None) => {
                            // Issue #9409: a runtime dispatch miss is a catchable
                            // MethodError in Julia — route it through the
                            // exception machinery instead of returning a fatal
                            // `Err` that escapes try/catch.
                            self.raise(VmError::MethodError(format!(
                                "no method matching {}({})",
                                qualified_name,
                                arg_type_names.join(", ")
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                        Err(error) => {
                            self.raise(error)?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                };

                if let Some(func_index) = func_index {
                    // User-defined function found - call it
                    let func = self.get_function_checked(func_index)?.clone();

                    let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

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

                    bind_kwargs_defaults(
                        &func,
                        &mut frame,
                        &mut self.struct_heap,
                        &self.code,
                        &self.functions,
                        self.frames.first(),
                        &self.global_slot_map,
                    )?;

                    self.return_ips.push(self.ip);
                    self.try_push_call_frame(frame)?;
                    self.ip = func.entry;
                    Ok(DispatchAction::Continue)
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
                // Also handles DataType values as constructors/converters
                // (e.g., map(Float64, arr), or Any-typed T(x, y)) (Issues #3480, #3895)
                // Stack layout: [args..., function_value]
                // Pop the Function/Closure value first
                let func_val = self.stack.pop_value()?;

                // Pop arguments
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Upstream evaluates every argv before `jl_apply`
                // (`julia/src/interpreter.c::do_call`) and only raises a
                // MethodError after generic lookup misses
                // (`julia/src/gf.c::jl_lookup_generic_`). The stack arguments
                // are fully evaluated here, so now enforce the compiler's
                // authoritative empty method view (Issue #11147).
                if let Some(func_name) = strict_empty_resolved_function_name(&func_val) {
                    let arg_type_names = self.callable_dispatch_type_names(&args);
                    self.raise(VmError::MethodError(format!(
                        "no method matching {}({})",
                        func_name,
                        arg_type_names.join(", ")
                    )))?;
                    return Ok(DispatchAction::Continue);
                }

                if matches!(
                    &func_val,
                    Value::Function(fv)
                        if matches!(
                            fv.name.as_str(),
                            "Iterators.map"
                                | "Base.Iterators.map"
                                | "Iterators.filter"
                                | "Base.Iterators.filter"
                                | "Generator"
                                | "Base.Generator"
                        )
                ) {
                    match self.call_runtime_callable_value(func_val, args)? {
                        RuntimeCallableResult::Immediate(value) => {
                            self.stack.push(value);
                        }
                        RuntimeCallableResult::StartedFrame => {}
                        RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                    }
                    return Ok(DispatchAction::Continue);
                }

                if let Value::ComposedFunction(cf) = &func_val {
                    self.setup_composed_call(
                        cf.outer.as_ref().clone(),
                        cf.inner.as_ref().clone(),
                        args,
                    )?;
                    return Ok(DispatchAction::Continue);
                }

                let (func_name, closure_captures) = match &func_val {
                    Value::Function(fv) => (fv.name.clone(), None),
                    Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
                    Value::DataType(jt) => (Self::datatype_dispatch_surface(jt), None),
                    Value::Struct(si) => {
                        // Callable struct instance: look up __callable_<TypeName>
                        let callable_name = Self::callable_method_name(&si.struct_name);
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
                        let callable_name = Self::callable_method_name(&si.struct_name);
                        (callable_name, None)
                    }
                    _ => return Err(self.not_callable_error(&func_val)),
                };

                let lookup_name = Self::runtime_function_lookup_name(&func_name);
                let native_range_value =
                    Self::try_native_range_unary_accessor_function_value(lookup_name, &args);
                if let Some(value) = native_range_value {
                    let candidates =
                        self.collect_runtime_callable_candidates(&func_val, &func_name)?;
                    if !self.runtime_callable_has_user_function_name(&candidates, lookup_name) {
                        self.stack.push(value);
                        return Ok(DispatchAction::Continue);
                    }
                }

                // Issue #9739: general per-call-site dispatch cache for dynamic
                // callee values, reusing the L1 (`call_site_caches`) / L2
                // (`dispatch_cache`) inline-cache infrastructure `CallDynamic`
                // already uses for statically-named calls (`call_dynamic.rs`).
                // A `Function`/`Closure` value's runtime dispatch identity is
                // its `typeof(name)` singleton — captured env is not part of
                // the type (see `call_site_arg_type_id`) — so prepending
                // `func_val` to the argument fingerprint yields a sound cache
                // key: same callee identity + same argument types + same
                // dispatch generation (bumped on `eval`-defined redefinition)
                // implies the same resolved `func_index`. This is what lets a
                // repeated call to the same function value (e.g. every element
                // of `f.(A)` inside Pure Julia's `_broadcast_apply`, or any
                // HOF/map/filter callback) skip
                // `collect_runtime_callable_candidates` +
                // `dispatch_function_variable`'s O(candidates) type-matching
                // scan — for any Pure Julia call site, not just broadcast.
                // Struct/StructRef/DataType callees are intentionally left
                // uncached here: the struct-self-prepend arity below depends
                // on `candidates`, so there is no fingerprint available before
                // the miss-path resolution that would already do that work,
                // and negative/fallback resolutions (builtin, intrinsic,
                // default-constructor) are not cached either — this covers the
                // hot positive-dispatch path the broadcast/HOF cost analysis
                // in Issue #9739 identified, without adding a fallback-aware
                // cache protocol this instruction never had.
                let call_site_ip = self.ip - 1;
                let arg_fingerprint = if matches!(&func_val, Value::Function(_) | Value::Closure(_))
                {
                    let mut fingerprint_values: Vec<&Value> = Vec::with_capacity(args.len() + 1);
                    fingerprint_values.push(&func_val);
                    fingerprint_values.extend(args.iter());
                    self.call_site_arg_fingerprints(&fingerprint_values)
                } else {
                    None
                };

                if let Some(fp) = arg_fingerprint.as_deref() {
                    if let Some(func_index) = self.lookup_call_site_inline_cache(call_site_ip, fp) {
                        return self.dispatch_resolved_function_variable_call(
                            func_index,
                            closure_captures,
                            args,
                            None,
                        );
                    }
                    if let Some(func_index) = self.lookup_call_site_dispatch_cache(call_site_ip, fp)
                    {
                        self.store_call_site_inline_cache(call_site_ip, Some(fp), func_index);
                        return self.dispatch_resolved_function_variable_call(
                            func_index,
                            closure_captures,
                            args,
                            None,
                        );
                    }
                }

                // Find all methods with the matching function name and do proper dispatch
                // based on runtime argument types.
                // Issue #1658: We must check if argument types match the declared parameter
                // types, not just pick the first method with matching name.
                // Use function_name_index for O(1) lookup (Issue #3361)
                let candidates = self.collect_runtime_callable_candidates(&func_val, &func_name)?;

                // Bound callable struct `(self::Type)(args)`: prepend the struct
                // instance so it binds to `self` (Issue #5127). Candidate lookup
                // includes parent callable methods (Issue #8264).
                if matches!(&func_val, Value::Struct(_) | Value::StructRef(_))
                    && self.callable_struct_needs_self(&candidates, args.len())
                {
                    args.insert(0, func_val.clone());
                }

                // Get runtime type names for all arguments
                let arg_type_names = self.callable_dispatch_type_names(&args);

                if candidates.is_empty() {
                    // A bare callee name with `FunctionInfo` entries that are
                    // ALL currently outside the dispatch world is a hoisted-
                    // but-not-yet-active top-level/inline definition, not a
                    // genuinely missing generic function: raise the same
                    // `UndefVarError` a static call site would (Issue #11320;
                    // siblings #11286/#10461's centralized visibility
                    // decision), instead of falling through to a
                    // MethodError/"not found" message that would wrongly
                    // imply the generic function itself already exists.
                    if matches!(&func_val, Value::Function(_))
                        && self.function_name_exists_only_as_unactivated(&func_name)
                    {
                        self.raise(VmError::UndefVarError(func_name))?;
                        return Ok(DispatchAction::Continue);
                    }
                    if matches!(&func_val, Value::DataType(_) | Value::Function(_))
                        && self.try_construct_default_datatype(&func_name, &args)?
                    {
                        return Ok(DispatchAction::Continue);
                    }

                    // Fallback: try to dispatch as a builtin function (Issue #2070)
                    // Builtin functions (uppercase, lowercase, string, etc.) are not
                    // in the user-defined method table, but can be passed as arguments
                    // to higher-order functions like map/filter/reduce.
                    if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                        if let Some(value) =
                            self.execute_runtime_builtin_immediate(builtin_id, &func_name, &args)?
                        {
                            self.stack.push(value);
                            return Ok(DispatchAction::Continue);
                        }
                        return Ok(DispatchAction::Continue);
                    }
                    if let Some(result) = self.try_call_intrinsic(lookup_name, &args)? {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }
                    let arg_type_names = args
                        .iter()
                        .map(|arg| self.get_type_name(arg))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.raise(VmError::MethodError(format!(
                        "no method matching {}({})",
                        func_name, arg_type_names
                    )))?;
                    return Ok(DispatchAction::Continue);
                }

                // Find best matching method based on runtime types.
                //
                // Prefer the value-based runtime dispatcher like the splat
                // paths do. Function values such as `f = map` carry only a
                // generic-function identity at the call site; the legacy
                // string scorer sees several `map(::Any, ::Any)`-shaped
                // candidates as equal and can select a lazy iterator shim
                // instead of the eager Base method (Issue #9981).
                // If user-defined dispatch fails, try builtin fallback (Issue #2546).
                // This handles cases like sqrt(Float64) where user-defined methods only
                // exist for Complex types but the builtin handles Float64.
                let func_index = match self.dispatch_function_variable_for_values(
                    &func_name,
                    &candidates,
                    &arg_type_names,
                    &args,
                ) {
                    Ok(Some(idx)) => idx,
                    Ok(None) => {
                        // A constructor function value can still reach the
                        // synthesized field-count default constructor after
                        // runtime dispatch misses, e.g. `Foo(xs..., y)`.
                        // (Issue #8321)
                        if matches!(&func_val, Value::DataType(_) | Value::Function(_))
                            && self.try_construct_default_datatype(&func_name, &args)?
                        {
                            return Ok(DispatchAction::Continue);
                        }
                        // Try BuiltinId-registered builtins first
                        if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                            if let Some(value) = self
                                .execute_runtime_builtin_immediate(builtin_id, &func_name, &args)?
                            {
                                self.stack.push(value);
                                return Ok(DispatchAction::Continue);
                            }
                            return Ok(DispatchAction::Continue);
                        }
                        // Try intrinsic math functions (sqrt, abs, etc.) (Issue #2546)
                        if let Some(result) = self.try_call_intrinsic(lookup_name, &args)? {
                            self.stack.push(result);
                            return Ok(DispatchAction::Continue);
                        }
                        // Issue #9409: a runtime dispatch miss is a catchable
                        // MethodError in Julia — route it through the
                        // exception machinery instead of returning a fatal
                        // `Err` that escapes try/catch.
                        self.raise(VmError::MethodError(format!(
                            "no method matching {}({})",
                            func_name,
                            arg_type_names.join(", ")
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }
                    Err(error) => {
                        self.raise(error)?;
                        return Ok(DispatchAction::Continue);
                    }
                };

                // Issue #9739: populate the per-call-site cache on a successful
                // resolution so the next call at this bytecode site with the
                // same callee identity/argument types/dispatch generation can
                // skip straight to `func_index` (see the lookup above).
                if let Some(fp) = arg_fingerprint.as_deref() {
                    self.store_call_site_dispatch_cache(call_site_ip, fp, func_index);
                    self.store_call_site_inline_cache(call_site_ip, Some(fp), func_index);
                }

                let callable_datatype = match &func_val {
                    Value::DataType(jt) => Some((**jt).clone()),
                    _ => None,
                };
                self.dispatch_resolved_function_variable_call(
                    func_index,
                    closure_captures,
                    args,
                    callable_datatype,
                )
            }

            Instr::InvokeFunctionVariable(arg_count, declared_arg_type_names) => {
                let func_val = self.stack.pop_value()?;

                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                match self.invoke_runtime_callable_value_with_signature(
                    func_val,
                    args,
                    declared_arg_type_names,
                )? {
                    RuntimeCallableResult::Immediate(value) => {
                        self.stack.push(value);
                    }
                    RuntimeCallableResult::StartedFrame => {}
                    RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                }
                Ok(DispatchAction::Continue)
            }

            Instr::InvokeFunctionVariableWithKwargs(operands) => {
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    let arg_count = operands.arg_count;
                    let declared_arg_type_names = &operands.declared_signature;
                    let kwarg_names = &operands.kwarg_names;
                    let kwargs_splat_mask = &operands.kwargs_splat_mask;
                    let func_val = self.stack.pop_value()?;
                    let func_val = self.push_transient_root(func_val)?;

                    let mut kwarg_values = Vec::with_capacity(kwarg_names.len());
                    for _ in 0..kwarg_names.len() {
                        let value = self.stack.pop_value()?;
                        kwarg_values.push(self.push_transient_root(value)?);
                    }
                    kwarg_values.reverse();

                    let kwargs_roots = match self.prepare_kwarg_value_roots(
                        kwarg_names,
                        kwargs_splat_mask,
                        &kwarg_values,
                    ) {
                        Ok(SplatPreparation::Ready(kwargs_map)) => kwargs_map,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };

                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        let value = self.stack.pop_value()?;
                        args.push(self.push_transient_root(value)?);
                    }
                    args.reverse();

                    let func_val = self.clone_transient_root(func_val)?;
                    let args = self.clone_transient_roots(&args)?;
                    let kwargs_map = kwargs_roots
                        .into_iter()
                        .map(|(name, value)| Ok((name, self.clone_transient_root(value)?)))
                        .collect::<Result<KwargsMap<_>, VmError>>()?;

                    match self.invoke_runtime_callable_value_with_signature_and_kwargs(
                        func_val,
                        args,
                        declared_arg_type_names,
                        &kwargs_map,
                    )? {
                        RuntimeCallableResult::Immediate(value) => {
                            self.stack.push(value);
                        }
                        RuntimeCallableResult::StartedFrame => {}
                        RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                    }
                    Ok(DispatchAction::Continue)
                })();
                self.end_transient_root_frame(root_base);
                result
            }

            Instr::InvokeFunctionVariableDynamicSignature(arg_count) => {
                let sig_val = self.stack.pop_value()?;
                let declared_arg_type_names = Self::runtime_invoke_signature_type_names(&sig_val)?;
                let func_val = self.stack.pop_value()?;

                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                match self.invoke_runtime_callable_value_with_signature(
                    func_val,
                    args,
                    &declared_arg_type_names,
                )? {
                    RuntimeCallableResult::Immediate(value) => {
                        self.stack.push(value);
                    }
                    RuntimeCallableResult::StartedFrame => {}
                    RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                }
                Ok(DispatchAction::Continue)
            }

            Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(
                arg_count,
                kwarg_names,
                kwargs_splat_mask,
            ) => {
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    let sig_val = self.stack.pop_value()?;
                    let sig_val = self.push_transient_root(sig_val)?;
                    let func_val = self.stack.pop_value()?;
                    let func_val = self.push_transient_root(func_val)?;

                    let mut kwarg_values = Vec::with_capacity(kwarg_names.len());
                    for _ in 0..kwarg_names.len() {
                        let value = self.stack.pop_value()?;
                        kwarg_values.push(self.push_transient_root(value)?);
                    }
                    kwarg_values.reverse();

                    // Upstream evaluates/merges keyword sources before validating
                    // invoke's dynamic signature tuple (Issue #11372).
                    let kwargs_roots = match self.prepare_kwarg_value_roots(
                        kwarg_names,
                        kwargs_splat_mask,
                        &kwarg_values,
                    ) {
                        Ok(SplatPreparation::Ready(kwargs_map)) => kwargs_map,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let sig_value = self.clone_transient_root(sig_val)?;
                    let declared_arg_type_names =
                        Self::runtime_invoke_signature_type_names(&sig_value)?;

                    let mut args = Vec::with_capacity(*arg_count);
                    for _ in 0..*arg_count {
                        let value = self.stack.pop_value()?;
                        args.push(self.push_transient_root(value)?);
                    }
                    args.reverse();

                    let func_val = self.clone_transient_root(func_val)?;
                    let args = self.clone_transient_roots(&args)?;
                    let kwargs_map = kwargs_roots
                        .into_iter()
                        .map(|(name, value)| Ok((name, self.clone_transient_root(value)?)))
                        .collect::<Result<KwargsMap<_>, VmError>>()?;

                    match self.invoke_runtime_callable_value_with_signature_and_kwargs(
                        func_val,
                        args,
                        &declared_arg_type_names,
                        &kwargs_map,
                    )? {
                        RuntimeCallableResult::Immediate(value) => {
                            self.stack.push(value);
                        }
                        RuntimeCallableResult::StartedFrame => {}
                        RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                    }
                    Ok(DispatchAction::Continue)
                })();
                self.end_transient_root_frame(root_base);
                result
            }

            Instr::CallFunctionVariableWithKwargsSplat(operands) => {
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    let arg_count = &operands.arg_count;
                    let splat_mask = &operands.pos_splat_mask;
                    let kwarg_names = &operands.kwarg_names;
                    let kwargs_splat_mask = &operands.kwargs_splat_mask;
                    // Stack layout: [args..., kwarg_values..., function_value]
                    let func_val = self.stack.pop_value()?;
                    let func_val = self.push_transient_root(func_val)?;

                    let mut kwarg_values = Vec::with_capacity(kwarg_names.len());
                    for _ in 0..kwarg_names.len() {
                        let value = self.stack.pop_value()?;
                        kwarg_values.push(self.push_transient_root(value)?);
                    }
                    kwarg_values.reverse();

                    // Upstream lowers the keyword-source merge before positional
                    // `_apply_iterate`; preserve that observable error order.
                    let kwargs_roots = match self.prepare_kwarg_value_roots(
                        kwarg_names,
                        kwargs_splat_mask,
                        &kwarg_values,
                    ) {
                        Ok(SplatPreparation::Ready(kwargs_roots)) => kwargs_roots,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };

                    let mut args = Vec::with_capacity(*arg_count);
                    for _ in 0..*arg_count {
                        let value = self.stack.pop_value()?;
                        args.push(self.push_transient_root(value)?);
                    }
                    args.reverse();
                    let expanded_roots = match self.prepare_splat_argument_roots(&args, splat_mask)
                    {
                        Ok(SplatPreparation::Ready(expanded_roots)) => expanded_roots,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let func_val = self.clone_transient_root(func_val)?;
                    let kwargs_map = kwargs_roots
                        .into_iter()
                        .map(|(name, value)| Ok((name, self.clone_transient_root(value)?)))
                        .collect::<Result<KwargsMap<_>, VmError>>()?;
                    let mut expanded_args = self.clone_transient_roots(&expanded_roots)?;

                    // Keep upstream `_apply_iterate` ordering: it exhausts every
                    // positional splat before calling `jl_apply`
                    // (`julia/src/builtins.c::jl_f__apply_iterate`). Keyword
                    // sources were merged above as well; only after both phases may
                    // the strict empty candidate view become a MethodError. This
                    // also preserves expansion/validation errors (Issue #11147).
                    if let Some(func_name) = strict_empty_resolved_function_name(&func_val) {
                        let arg_type_names = self.callable_dispatch_type_names(&expanded_args);
                        self.raise(VmError::MethodError(format!(
                            "no method matching {}({})",
                            func_name,
                            arg_type_names.join(", ")
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }

                    if matches!(
                        &func_val,
                        Value::Function(fv)
                            if kwargs_map.is_empty()
                                && matches!(
                                    fv.name.as_str(),
                                    "Iterators.map"
                                        | "Base.Iterators.map"
                                        | "Iterators.filter"
                                        | "Base.Iterators.filter"
                                        | "Generator"
                                        | "Base.Generator"
                                )
                    ) {
                        match self.call_runtime_callable_value(func_val, expanded_args)? {
                            RuntimeCallableResult::Immediate(value) => {
                                self.stack.push(value);
                            }
                            RuntimeCallableResult::StartedFrame => {}
                            RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                        }
                        return Ok(DispatchAction::Continue);
                    }

                    if kwargs_map.is_empty() {
                        if let Value::ComposedFunction(cf) = &func_val {
                            self.setup_composed_call(
                                cf.outer.as_ref().clone(),
                                cf.inner.as_ref().clone(),
                                expanded_args,
                            )?;
                            return Ok(DispatchAction::Continue);
                        }
                    }

                    let (func_name, closure_captures) = match &func_val {
                        Value::Function(fv) => (fv.name.clone(), None),
                        Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
                        // A DataType may have keyword outer constructors. Keep it
                        // callable when kwargs are present so runtime module/import
                        // lookup can dispatch `T(args...; kwargs...)` through those
                        // methods; only the raw field-count default constructor below
                        // remains keyword-free (Issue #11216).
                        Value::DataType(jt) => (jt.name().to_string(), None),
                        Value::Struct(si) => (Self::callable_method_name(&si.struct_name), None),
                        Value::StructRef(idx) => {
                            let si = self.struct_heap.get(*idx).ok_or_else(|| {
                                VmError::TypeError(format!(
                                    "Invalid struct reference: index {} out of bounds",
                                    idx
                                ))
                            })?;
                            (Self::callable_method_name(&si.struct_name), None)
                        }
                        _ => return Err(self.not_callable_error(&func_val)),
                    };

                    let candidates =
                        self.collect_runtime_callable_candidates(&func_val, &func_name)?;

                    // Bound callable struct `(self::Type)(args)`: prepend the struct
                    // instance so it binds to `self` (Issue #5127). Candidate lookup
                    // includes parent callable methods (Issue #8264).
                    if matches!(&func_val, Value::Struct(_) | Value::StructRef(_))
                        && self.callable_struct_needs_self(&candidates, expanded_args.len())
                    {
                        expanded_args.insert(0, func_val.clone());
                    }

                    let arg_type_names = self.callable_dispatch_type_names(&expanded_args);
                    let lookup_name = Self::runtime_function_lookup_name(&func_name);

                    if candidates.is_empty() {
                        // Same centralized visibility decision as the plain
                        // and positional-splat call paths (Issue #11320;
                        // siblings #11286/#10461): a hoisted-but-not-yet-
                        // active top-level/inline definition raises
                        // `UndefVarError`, not a "not found"/MethodError that
                        // would wrongly imply the generic function exists.
                        if matches!(&func_val, Value::Function(_))
                            && self.function_name_exists_only_as_unactivated(&func_name)
                        {
                            self.raise(VmError::UndefVarError(func_name))?;
                            return Ok(DispatchAction::Continue);
                        }
                        if kwargs_map.is_empty() {
                            if let Value::DataType(_) = &func_val {
                                if self
                                    .try_construct_default_datatype(&func_name, &expanded_args)?
                                {
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                                if let Some(value) = self.execute_runtime_builtin_immediate(
                                    builtin_id,
                                    &func_name,
                                    &expanded_args,
                                )? {
                                    self.stack.push(value);
                                    return Ok(DispatchAction::Continue);
                                }
                                return Ok(DispatchAction::Continue);
                            }
                            if let Some(result) =
                                self.try_call_intrinsic(lookup_name, &expanded_args)?
                            {
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                        }
                        if let Value::DataType(_) = &func_val {
                            self.raise(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_name,
                                arg_type_names.join(", ")
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                        return Err(VmError::TypeError(format!(
                            "Function '{}' not found",
                            func_name
                        )));
                    }

                    let dispatch_candidates =
                        Self::prefer_candidates_declaring_kwargs(&candidates, &kwargs_map);
                    // Keep kwargs-splat dispatch on the same value-based path as
                    // ordinary splats. The string scorer only sees coarse names
                    // like `Any` / `Function`, which can let a generic fallback beat
                    // a callable-specific method such as `plot(x, f::Function)`.
                    let func_index = match self.dispatch_function_variable_for_values(
                        &func_name,
                        &dispatch_candidates,
                        &arg_type_names,
                        &expanded_args,
                    ) {
                        Ok(Some(idx)) => idx,
                        Ok(None) => {
                            if kwargs_map.is_empty() {
                                if let Value::DataType(_) = &func_val {
                                    if self.try_construct_default_datatype(
                                        &func_name,
                                        &expanded_args,
                                    )? {
                                        return Ok(DispatchAction::Continue);
                                    }
                                }
                                if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                                    if let Some(value) = self.execute_runtime_builtin_immediate(
                                        builtin_id,
                                        &func_name,
                                        &expanded_args,
                                    )? {
                                        self.stack.push(value);
                                        return Ok(DispatchAction::Continue);
                                    }
                                    return Ok(DispatchAction::Continue);
                                }
                                if let Some(result) =
                                    self.try_call_intrinsic(lookup_name, &expanded_args)?
                                {
                                    self.stack.push(result);
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                            // Issue #9409: a runtime dispatch miss is a catchable
                            // MethodError in Julia — route it through the exception
                            // machinery instead of returning a fatal `Err` that
                            // escapes try/catch.
                            self.raise(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_name,
                                arg_type_names.join(", ")
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                        Err(error) => {
                            self.raise(error)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };

                    let func = self.get_function_checked(func_index)?.clone();
                    let (target_entry, slot_count) =
                        if closure_captures.is_some() || !kwargs_map.is_empty() {
                            None
                        } else {
                            self.try_specialized_entry_for_runtime_call(func_index, &expanded_args)
                        }
                        .unwrap_or((func.entry, func.local_slot_count));

                    let mut frame = if let Some(captures) = closure_captures {
                        self.acquire_frame_with_captures(slot_count, Some(func_index), &captures)
                    } else {
                        self.acquire_frame(slot_count, Some(func_index))
                    };

                    self.bind_type_params(&func, &expanded_args, &mut frame);
                    if let Value::DataType(callable_type) = &func_val {
                        self.bind_callable_datatype_type_params(callable_type, &func, &mut frame);
                    }

                    if let Some(vararg_idx) = func.vararg_param_index {
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
                        let vararg_tuple = Value::Tuple(TupleValue {
                            elements: expanded_args[vararg_idx..].to_vec(),
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

                    // Issue #11024: assert supplied keyword arguments against their
                    // declared types before binding (upstream raises TypeError).
                    self.check_supplied_kwarg_types(&func, &kwargs_map, &frame.type_bindings)?;
                    bind_kwargs_with_map(
                        &func,
                        &kwargs_map,
                        &mut frame,
                        &mut self.struct_heap,
                        &self.code,
                        &self.functions,
                        self.frames.first(),
                        &self.global_slot_map,
                    )?;

                    if let Some(result) = self.try_eval_cached_generated_expr(
                        func_index,
                        &func,
                        &expanded_args,
                        &frame,
                    )? {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }

                    let generated_eval_frame = func.is_generated.then(|| frame.clone());
                    self.bind_generated_body_arg_types(&func, &expanded_args, &mut frame);
                    self.return_ips.push(self.ip);
                    self.try_push_call_frame(frame)?;
                    self.remember_current_generated_expr_cache_key(
                        &func,
                        func_index,
                        &expanded_args,
                        generated_eval_frame,
                    );
                    self.ip = target_entry;
                    Ok(DispatchAction::Continue)
                })();
                self.end_transient_root_frame(root_base);
                result
            }

            Instr::CallFunctionVariableWithSplat(arg_count, ref splat_mask) => {
                let root_base = self.begin_transient_root_frame();
                let result: Result<DispatchAction, VmError> = (|| {
                    // Call a Function or Closure stored in a local variable with splatted arguments.
                    // This handles patterns like: function apply_variadic(f, args...); f(args...); end
                    // Stack layout: [args..., function_value]

                    // Pop the Function/Closure value first
                    let func_val = self.stack.pop_value()?;
                    let func_val = self.push_transient_root(func_val)?;

                    // Pop arguments
                    let mut args = Vec::with_capacity(*arg_count);
                    for _ in 0..*arg_count {
                        let value = self.stack.pop_value()?;
                        args.push(self.push_transient_root(value)?);
                    }
                    args.reverse();

                    // Expand splatted arguments
                    let expanded_roots = match self.prepare_splat_argument_roots(&args, splat_mask)
                    {
                        Ok(SplatPreparation::Ready(expanded_roots)) => expanded_roots,
                        Ok(SplatPreparation::Raised) => return Ok(DispatchAction::Continue),
                        Err(err) => {
                            self.raise(err)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };
                    let func_val = self.clone_transient_root(func_val)?;
                    let mut expanded_args = self.clone_transient_roots(&expanded_roots)?;

                    // `jl_f__apply_iterate` expands/validates splats and only then
                    // enters `jl_apply`; `jl_lookup_generic_` raises MethodError on
                    // the subsequent lookup miss. Preserve that order for an
                    // explicitly empty resolved candidate set (Issue #11147).
                    if let Some(func_name) = strict_empty_resolved_function_name(&func_val) {
                        let arg_type_names = self.callable_dispatch_type_names(&expanded_args);
                        self.raise(VmError::MethodError(format!(
                            "no method matching {}({})",
                            func_name,
                            arg_type_names.join(", ")
                        )))?;
                        return Ok(DispatchAction::Continue);
                    }

                    if matches!(
                        &func_val,
                        Value::Function(fv)
                            if matches!(
                                fv.name.as_str(),
                                "Iterators.map"
                                    | "Base.Iterators.map"
                                    | "Iterators.filter"
                                    | "Base.Iterators.filter"
                                    | "Generator"
                                    | "Base.Generator"
                            )
                    ) {
                        match self.call_runtime_callable_value(func_val, expanded_args)? {
                            RuntimeCallableResult::Immediate(value) => {
                                self.stack.push(value);
                            }
                            RuntimeCallableResult::StartedFrame => {}
                            RuntimeCallableResult::Raised => return Ok(DispatchAction::Continue),
                        }
                        return Ok(DispatchAction::Continue);
                    }

                    if let Value::ComposedFunction(cf) = &func_val {
                        self.setup_composed_call(
                            cf.outer.as_ref().clone(),
                            cf.inner.as_ref().clone(),
                            expanded_args,
                        )?;
                        return Ok(DispatchAction::Continue);
                    }

                    // A runtime `DataType` value (e.g. the concrete instantiation
                    // built by applying a caller `where`-bound type variable or an
                    // inline value-parameter expression to a parametric struct,
                    // `Foo{M,N,T,n}`) and a callable struct instance are just as
                    // callable via splat forwarding as a plain Function/Closure —
                    // this mirrors `CallFunctionVariable`'s (non-splat) callee
                    // resolution below so `f(xs...)` and `f(xs)` agree on which
                    // values are callable (Issue #11539).
                    let (func_name, closure_captures) = match &func_val {
                        Value::Function(fv) => (fv.name.clone(), None),
                        Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
                        Value::DataType(jt) => (Self::datatype_dispatch_surface(jt), None),
                        Value::Struct(si) => (Self::callable_method_name(&si.struct_name), None),
                        Value::StructRef(idx) => {
                            let si = self.struct_heap.get(*idx).ok_or_else(|| {
                                VmError::TypeError(format!(
                                    "Invalid struct reference: index {} out of bounds",
                                    idx
                                ))
                            })?;
                            (Self::callable_method_name(&si.struct_name), None)
                        }
                        _ => return Err(self.not_callable_error(&func_val)),
                    };

                    // Find all methods with the matching function name and do proper dispatch
                    // Use function_name_index for O(1) lookup (Issue #3361)
                    let candidates =
                        self.collect_runtime_callable_candidates(&func_val, &func_name)?;

                    // Bound callable struct `(self::Type)(args)`: prepend the struct
                    // instance so it binds to `self` (Issue #5127), mirroring
                    // `CallFunctionVariable`'s non-splat handling.
                    if matches!(&func_val, Value::Struct(_) | Value::StructRef(_))
                        && self.callable_struct_needs_self(&candidates, expanded_args.len())
                    {
                        expanded_args.insert(0, func_val.clone());
                    }

                    // Get runtime type names for all expanded arguments
                    let arg_type_names = self.callable_dispatch_type_names(&expanded_args);

                    let lookup_name = Self::runtime_function_lookup_name(&func_name);

                    if candidates.is_empty() {
                        // Same centralized visibility decision as the plain
                        // call path (Issue #11320; siblings #11286/#10461):
                        // a hoisted-but-not-yet-active top-level/inline
                        // definition (e.g. one nested inside an untaken
                        // `if`/zero-iteration loop branch) raises
                        // `UndefVarError`, not a "not found" error that would
                        // wrongly imply the generic function already exists.
                        // The compile-time guard
                        // (`Instr::RaiseUndefVarErrorIfFunctionInvisible`)
                        // already catches this for a statically-known bare
                        // callee before any argument is evaluated; this is
                        // the runtime backstop for every other route that
                        // reaches this dispatch (dynamically-resolved
                        // callees, cache-restored programs, etc.).
                        if matches!(&func_val, Value::Function(_))
                            && self.function_name_exists_only_as_unactivated(&func_name)
                        {
                            self.raise(VmError::UndefVarError(func_name))?;
                            return Ok(DispatchAction::Continue);
                        }
                        if matches!(&func_val, Value::Function(_) | Value::DataType(_))
                            && self.try_construct_default_datatype(&func_name, &expanded_args)?
                        {
                            return Ok(DispatchAction::Continue);
                        }
                        // Fallback: try to dispatch as a builtin function (Issue #2070)
                        if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                            if let Some(value) = self.execute_runtime_builtin_immediate(
                                builtin_id,
                                &func_name,
                                &expanded_args,
                            )? {
                                self.stack.push(value);
                                return Ok(DispatchAction::Continue);
                            }
                            return Ok(DispatchAction::Continue);
                        }
                        if let Some(result) =
                            self.try_call_intrinsic(lookup_name, &expanded_args)?
                        {
                            self.stack.push(result);
                            return Ok(DispatchAction::Continue);
                        }
                        // User-visible: user can call a function variable with splat that resolves to no compiled methods
                        return Err(VmError::TypeError(format!(
                            "Function '{}' not found",
                            func_name
                        )));
                    }
                    // Find best matching method based on runtime types.
                    //
                    // Prefer the value-based runtime dispatcher, which selects on the
                    // concrete argument *values* and mirrors the compile-time
                    // `MethodTable` (diagonal `Tuple{T,T}`, bounded-`where`, and
                    // container-vs-abstract relations included). The string-name scorer
                    // used by `dispatch_function_variable` scores a bare `where`-TypeVar
                    // parameter as minimally specific, so a splatted self-recursive call
                    // in upstream's canonical form
                    // `f(x::Integer,y::Integer)=f(promote(x,y)...)` re-selects the same
                    // `(Integer,Integer)` method instead of the more specific diagonal
                    // `f(x::T,y::T) where {T<:Integer}`, recursing to a StackOverflow
                    // (Issue #9513). The two-variable form already dispatches through the
                    // value-based path, so routing the splat's expanded arguments through
                    // the same dispatcher makes the two shapes agree.
                    // If user-defined dispatch fails, try builtin fallback (Issue #2546).
                    let func_index = match self.dispatch_function_variable_for_values(
                        &func_name,
                        &candidates,
                        &arg_type_names,
                        &expanded_args,
                    ) {
                        Ok(Some(idx)) => idx,
                        Ok(None) => {
                            // Splat expansion can reveal a field-count default
                            // constructor arity even when the named function has
                            // only outer-constructor methods registered.
                            // (Issue #8321; extended to runtime `DataType`
                            // callees alongside named `Function` values for
                            // Issue #11539.)
                            if matches!(&func_val, Value::Function(_) | Value::DataType(_))
                                && self
                                    .try_construct_default_datatype(&func_name, &expanded_args)?
                            {
                                return Ok(DispatchAction::Continue);
                            }
                            if let Some(builtin_id) = BuiltinId::from_name(lookup_name) {
                                if let Some(value) = self.execute_runtime_builtin_immediate(
                                    builtin_id,
                                    &func_name,
                                    &expanded_args,
                                )? {
                                    self.stack.push(value);
                                    return Ok(DispatchAction::Continue);
                                }
                                return Ok(DispatchAction::Continue);
                            }
                            // Try intrinsic math functions (sqrt, abs, etc.) (Issue #2546)
                            if let Some(result) =
                                self.try_call_intrinsic(lookup_name, &expanded_args)?
                            {
                                self.stack.push(result);
                                return Ok(DispatchAction::Continue);
                            }
                            // Issue #9409: a runtime dispatch miss is a catchable
                            // MethodError in Julia — route it through the
                            // exception machinery instead of returning a fatal
                            // `Err` that escapes try/catch.
                            self.raise(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_name,
                                arg_type_names.join(", ")
                            )))?;
                            return Ok(DispatchAction::Continue);
                        }
                        Err(error) => {
                            self.raise(error)?;
                            return Ok(DispatchAction::Continue);
                        }
                    };

                    let func = self.get_function_checked(func_index)?.clone();
                    let (target_entry, slot_count) = if closure_captures.is_some() {
                        None
                    } else {
                        self.try_specialized_entry_for_runtime_call(func_index, &expanded_args)
                    }
                    .unwrap_or((func.entry, func.local_slot_count));

                    let mut frame = if let Some(captures) = closure_captures {
                        self.acquire_frame_with_captures(slot_count, Some(func_index), &captures)
                    } else {
                        self.acquire_frame(slot_count, Some(func_index))
                    };

                    // Bind type parameters from where clauses (Issue #2468)
                    self.bind_type_params(&func, &expanded_args, &mut frame);
                    if let Value::DataType(callable_type) = &func_val {
                        self.bind_callable_datatype_type_params(callable_type, &func, &mut frame);
                    }

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
                    bind_kwargs_defaults(
                        &func,
                        &mut frame,
                        &mut self.struct_heap,
                        &self.code,
                        &self.functions,
                        self.frames.first(),
                        &self.global_slot_map,
                    )?;

                    if let Some(result) = self.try_eval_cached_generated_expr(
                        func_index,
                        &func,
                        &expanded_args,
                        &frame,
                    )? {
                        self.stack.push(result);
                        return Ok(DispatchAction::Continue);
                    }

                    let generated_eval_frame = func.is_generated.then(|| frame.clone());
                    self.bind_generated_body_arg_types(&func, &expanded_args, &mut frame);
                    self.return_ips.push(self.ip);
                    self.try_push_call_frame(frame)?;
                    self.remember_current_generated_expr_cache_key(
                        &func,
                        func_index,
                        &expanded_args,
                        generated_eval_frame,
                    );
                    self.ip = target_entry;
                    Ok(DispatchAction::Continue)
                })();
                self.end_transient_root_frame(root_base);
                result
            }

            _ => Err(super::unhandled(instr)),
        }
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
            "!" if args.len() == 1 => {
                let Value::Bool(value) = args[0] else {
                    return Ok(None);
                };
                return Ok(Some(Value::Bool(!value)));
            }
            "+" if !args.is_empty() => {
                let mut result = args[0].clone();
                for arg in &args[1..] {
                    result = self.dynamic_add(&result, arg)?;
                }
                return Ok(Some(result));
            }
            "*" if !args.is_empty() => {
                let mut result = args[0].clone();
                for arg in &args[1..] {
                    result = self.dynamic_mul(&result, arg)?;
                }
                return Ok(Some(result));
            }
            "sqrt" if args.len() == 1 => {
                // BigFloat keeps full precision through the callable/HOF lane,
                // mirroring `BuiltinId::Sqrt`'s arm — without this,
                // `map(sqrt, [big"2.0"])` raised MethodError while direct
                // `sqrt(big"2.0")` worked (Issue #10604).
                if let Value::BigFloat(x) = &args[0] {
                    if x.is_negative() {
                        return Err(VmError::DomainError(format!(
                            "sqrt was called with a negative real argument ({}) but will only return a complex result if called with a complex argument. Try sqrt(Complex(x)).",
                            x
                        )));
                    }
                    return Ok(Some(Value::BigFloat(x.sqrt(
                        crate::vm::value::get_bigfloat_precision(),
                        crate::vm::value::get_bigfloat_rounding(),
                    ))));
                }
                // Issue #10481: a non-numeric operand is a dispatch miss in
                // upstream Julia (`map(sqrt, ["a"])` raises MethodError).
                // Decline the intrinsic fallback so the caller raises the
                // genuine `no method matching sqrt(...)` MethodError — the
                // same pattern as the sin/cos arm below (Issue #4341).
                let Ok(v) = value_to_f64_with_heap(&args[0], &self.struct_heap) else {
                    return Ok(None);
                };
                if v < 0.0 {
                    return Err(VmError::DomainError(format!(
                        "sqrt was called with a negative real argument ({}) but will only return a complex result if called with a complex argument. Try sqrt(Complex(x)).",
                        v
                    )));
                }
                return Ok(Some(apply_unary_float_op_with_heap(
                    args[0].clone(),
                    &self.struct_heap,
                    f64::sqrt,
                )?));
            }
            "abs" if args.len() == 1 => {
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
                        )));
                    }
                };
                return Ok(Some(result));
            }
            "sin" | "cos" | "tan" | "exp" | "log" if args.len() == 1 => {
                let v = match self.convert_to_f64(&args[0]) {
                    Ok(v) => v,
                    // Issue #4341: these names are Pure Julia-dispatched for
                    // Complex and other structs. The function-value intrinsic
                    // fallback is only valid for primitive numeric values.
                    Err(_) => return Ok(None),
                };
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
            "floor" | "ceil" | "round" | "trunc" if args.len() == 1 => {
                let op = match func_name {
                    "floor" => f64::floor,
                    "ceil" => f64::ceil,
                    // Julia's default RoundNearest is round-half-to-even.
                    "round" => f64::round_ties_even,
                    "trunc" => f64::trunc,
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "unexpected rounding intrinsic '{}'",
                            func_name
                        )))
                    }
                };
                // Issue #10481: a non-numeric operand is a dispatch miss in
                // upstream Julia — decline the intrinsic fallback so the
                // caller raises the genuine MethodError (Issue #4341 pattern).
                return match apply_unary_float_op_with_heap(args[0].clone(), &self.struct_heap, op)
                {
                    Ok(v) => Ok(Some(v)),
                    Err(VmError::TypeError(_)) => Ok(None),
                    Err(err) => Err(err),
                };
            }
            _ => {}
        }
        Ok(None)
    }

    fn try_construct_default_datatype(
        &mut self,
        type_name: &str,
        args: &[Value],
    ) -> Result<bool, VmError> {
        // Typed comprehensions convert each produced element with the declared
        // element type. When the body already produced that exact struct type,
        // the constructor is an identity conversion (`T(x::T) = x`) rather than
        // a field-count construction. (Issue #8321)
        if let [arg] = args {
            if self.get_type_name(arg) == type_name {
                self.stack.push(arg.clone());
                return Ok(true);
            }
        }

        // Resolve the struct definition by exact name, falling back to a UNIQUE
        // match on the bare (last `.`-separated) segment. A `DataType` value held
        // in a local can carry a short alias name (`Bar`) when it is a re-exported
        // `const` type alias brought into scope via `using` — `t = Bar; t(7)`
        // dynamically calls a `Value::DataType(Struct("Bar"))` whose underlying
        // default-field-constructor struct is registered as `A.Bar`. Resolving the
        // bare segment routes the dynamic call to that struct's field constructor
        // (Issue #8058). The fallback only fires when the bare name is unambiguous,
        // so it never silently picks the wrong struct.
        let resolved = self
            .struct_defs
            .iter()
            .enumerate()
            .find(|(_, def)| def.name == type_name)
            .map(|(type_id, def)| (type_id, def.name.clone(), def.fields.len()))
            .or_else(|| {
                let bare = type_name.rsplit('.').next().unwrap_or(type_name);
                let mut matches = self.struct_defs.iter().enumerate().filter(|(_, def)| {
                    !def.name.contains('{')
                        && def.name.rsplit('.').next().unwrap_or(&def.name) == bare
                });
                match (matches.next(), matches.next()) {
                    (Some((type_id, def)), None) => {
                        Some((type_id, def.name.clone(), def.fields.len()))
                    }
                    _ => None,
                }
            });
        let Some((type_id, struct_name, field_count)) = resolved else {
            // No concrete `struct_defs` row matched. A PARAMETRIC base (`Pt`,
            // registered only in `parametric_structs`) has no concrete entry
            // until it is instantiated, so a dynamic call of its `DataType`
            // value (`t = A.Pt; t(1.0, 2.0)`, including a re-exported
            // `const Pt = A.Pt`) lands here. Infer the type parameters from the
            // argument values and construct the parametric instance, mirroring
            // the compile-time default-constructor path (Issue #8070).
            return self.try_construct_parametric_datatype(type_name, args);
        };

        // A concrete instantiation row may already have been materialized by a
        // successful inner-constructor call.  That row does not restore Julia's
        // suppressed default field constructor: after a runtime dispatch miss,
        // consult the parametric definition before raw construction just as the
        // uninstantiated fallback below does (Issue #10959).
        let concrete_base = super::super::util::extract_base_type(&struct_name);
        if self
            .resolve_runtime_parametric_def(concrete_base)
            .is_some_and(|(_, def)| !def.inner_constructors.is_empty())
        {
            return Ok(false);
        }

        if field_count != args.len() {
            return Err({
                let message = format!(
                    "no method matching {}({})",
                    type_name,
                    args.iter()
                        .map(|arg| self.get_type_name(arg))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.method_error_with_payload(message, type_name, args)
            });
        }

        let mut resolved_name = struct_name;
        if let Some(name) = resolve_any_type_param(&resolved_name, args) {
            resolved_name = name;
        }

        // Coerce field values to their declared concrete primitive field
        // types, matching Julia's default constructor `convert(fieldtype, x)`
        // step (Issue #4990).
        let mut field_values = args.to_vec();
        let struct_def = self.struct_defs.get(type_id).cloned();
        super::struct_ops::coerce_fields_to_declared_types(
            &self.struct_defs,
            &self.struct_heap,
            struct_def.as_ref(),
            &mut field_values,
        );

        let idx = self.struct_heap.len();
        self.struct_heap.push(StructInstance::with_name(
            type_id,
            resolved_name,
            field_values,
        ));
        self.stack.push(Value::StructRef(idx));
        Ok(true)
    }

    /// Runtime fallback for invoking a `Value::DataType` whose base is registered
    /// only as a PARAMETRIC struct (no concrete `struct_defs` row yet).
    ///
    /// `t = A.Pt; t(1.0, 2.0)` pushes `Value::DataType(Pt)`; when it is called
    /// the dispatcher finds no method candidates and
    /// [`Self::try_construct_default_datatype`] finds no concrete struct, so we
    /// land here. The type parameters are inferred from the argument value types
    /// and the instance is built — exactly mirroring the compile-time default
    /// constructor path ([`crate::runtime_types::infer_parametric_type_args`]), so the
    /// dynamic call agrees with the static `A.Pt(1.0, 2.0)` form. Resolves the
    /// short / qualified / re-exported alias name to the registered parametric
    /// base the same way the compiler's `resolve_parametric_struct_name` does
    /// (Issue #8070).
    ///
    /// Returns `Ok(true)` when an instance was constructed (and pushed),
    /// `Ok(false)` when `type_name` is not a parametric base (let the caller try
    /// builtins / intrinsics), or an error on arity / inference mismatch.
    fn try_construct_parametric_datatype(
        &mut self,
        type_name: &str,
        args: &[Value],
    ) -> Result<bool, VmError> {
        // `type_name` may carry explicit type args (`Pt{Float64}`); the base is
        // all we need to find the parametric registry entry.
        let base_query = super::super::util::extract_base_type(type_name);
        let Some((base_name, def)) = self.resolve_runtime_parametric_def(base_query) else {
            return Ok(false);
        };

        // Only the default field constructor is handled here. A parametric
        // struct with inner constructors has no synthesized field-count default
        // constructor in Julia; when a `Function` constructor call reaches this
        // runtime fallback after a splat-expanded dispatch miss, do not invent
        // one. (Issue #8321)
        if !def.inner_constructors.is_empty() {
            return Ok(false);
        }

        if def.fields.len() != args.len() {
            return Err({
                let message = format!(
                    "no method matching {}({})",
                    type_name,
                    args.iter()
                        .map(|arg| self.get_type_name(arg))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.method_error_with_payload(message, type_name, args)
            });
        }

        // Determine the concrete type arguments. When `type_name` carries
        // *explicit* parameters (`Base{Float64}`, from `t{Float64}(args)`,
        // Issue #8101) they are used directly and the arguments are converted,
        // matching upstream's explicit-`{T}` constructor (which permits
        // conversion). Otherwise — the no-type-argument dynamic form
        // `t(args)` (Issue #8070) — the parameters are *inferred* from the
        // argument value types, which fails with a `MethodError` when a single
        // type variable cannot unify the arguments (e.g. `Pt(1, 2.0)` for
        // `Pt{T}(x::T, y::T)`, Issue #8102).
        let explicit_type_args = super::struct_ops::parse_explicit_parametric_type_args(
            type_name,
            def.type_params.len(),
        );
        let explicit = explicit_type_args.is_some();
        let type_args = match explicit_type_args {
            Some(type_args) if type_args.len() == def.type_params.len() => type_args,
            Some(type_args) => {
                let arg_types: Vec<JuliaType> = args
                    .iter()
                    .map(|arg| JuliaType::from_name_or_struct(&self.get_type_name(arg)))
                    .collect();
                super::struct_ops::infer_runtime_parametric_type_args_with_explicit_prefix(
                    &def, &base_name, &arg_types, &type_args,
                )?
            }
            None => {
                let arg_types: Vec<JuliaType> = args
                    .iter()
                    .map(|arg| JuliaType::from_name_or_struct(&self.get_type_name(arg)))
                    .collect();
                match crate::runtime_types::infer_parametric_type_args(&def, &base_name, &arg_types)
                {
                    Ok(type_args) => type_args,
                    Err(_) => {
                        return Err({
                            let message = format!(
                                "no method matching {}({})",
                                type_name,
                                args.iter()
                                    .map(|arg| self.get_type_name(arg))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            self.method_error_with_payload(message, type_name, args)
                        });
                    }
                }
            }
        };

        let type_arg_names: Vec<String> =
            type_args.iter().map(|ty| ty.name().to_string()).collect();
        let struct_name = format!("{}{{{}}}", base_name, type_arg_names.join(", "));

        // Reuse an existing concrete instantiation row when one is present (so
        // field access binds to a real `type_id`); otherwise fall back to 0 —
        // display and field access resolve the parametric base by name (the same
        // pattern the `new{T}` / `NewDynamicParametricStruct` paths use,
        // Issue #7958).
        let type_id = self
            .struct_defs
            .iter()
            .position(|d| d.name == struct_name)
            .unwrap_or(0);

        // Coerce field values to their declared concrete primitive field types.
        let mut field_values = args.to_vec();
        if explicit {
            // Explicit `Base{T...}(args)`: convert each argument to the field
            // type obtained by substituting the explicit type parameters, even
            // when no concrete instantiation row exists yet (Issue #8101).
            super::struct_ops::coerce_fields_to_explicit_type_args(
                &def,
                &type_args,
                &mut field_values,
            );
        } else {
            // Implicit `t(args)`: the values already carry the types the
            // parameters were inferred from, so coercion only applies when the
            // concrete instantiation row exists (Issue #4990 / #8070).
            let struct_def = self
                .struct_defs
                .get(type_id)
                .filter(|d| d.name == struct_name)
                .cloned();
            super::struct_ops::coerce_fields_to_declared_types(
                &self.struct_defs,
                &self.struct_heap,
                struct_def.as_ref(),
                &mut field_values,
            );
        }

        let idx = self.struct_heap.len();
        self.struct_heap.push(StructInstance::with_name(
            type_id,
            struct_name,
            field_values,
        ));
        self.stack.push(Value::StructRef(idx));
        Ok(true)
    }

    /// Resolve a (possibly short, qualified, or re-exported) type name to its
    /// registered parametric struct base, returning the canonical base name and
    /// the parametric `StructDef`. Mirrors the compiler's
    /// `resolve_parametric_struct_name`: prefer the exact key (preferring its
    /// qualified `Module.Name` variant for correct display), then a `Base.`
    /// alias, then any qualified key ending in `.Name` (Issue #8070).
    pub(in crate::vm) fn resolve_runtime_parametric_def(
        &self,
        name: &str,
    ) -> Option<(String, crate::ir::core::StructDef)> {
        let ctx = self.compile_context.as_ref()?;
        let parametric_structs = &ctx.parametric_structs;

        if let Some(unqualified) = name.strip_prefix("Base.") {
            if let Some(def) = parametric_structs.get(unqualified) {
                return Some((unqualified.to_string(), def.def.clone()));
            }
        }

        if parametric_structs.contains_key(name) {
            // Prefer a qualified `Module.name` key so the instantiated name
            // (and hence `typeof`) carries the module path, matching the static
            // `A.Pt(...)` constructor form.
            let qualified_suffix = format!(".{}", name);
            for (key, def) in parametric_structs {
                if key != name && key.ends_with(&qualified_suffix) {
                    return Some((key.clone(), def.def.clone()));
                }
            }
            let def = parametric_structs.get(name)?;
            return Some((name.to_string(), def.def.clone()));
        }

        // Only a qualified key is registered (calling with a short / re-exported
        // alias from inside or outside the defining module).
        let qualified_suffix = format!(".{}", name);
        for (key, def) in parametric_structs {
            if key.ends_with(&qualified_suffix) {
                return Some((key.clone(), def.def.clone()));
            }
        }

        None
    }

    /// Call a function by name with a single argument.
    /// Uses proper dispatch to check if argument type matches parameter type.
    /// Issue #1658: Previously just called the first method found, without type checking.
    pub(in crate::vm) fn dispatch_function_variable(
        &self,
        func_name: &str,
        candidates: &[(usize, &FunctionInfo)],
        arg_type_names: &[String],
    ) -> Result<usize, VmError> {
        let best_match = resolve_callable_value_candidates(
            &self.struct_hierarchy,
            candidates.iter().map(|(idx, func)| CallableValueCandidate {
                idx: *idx,
                param_types: &func.param_julia_types,
                param_count: func.params.len(),
                vararg_param_index: func.vararg_param_index,
                vararg_fixed_count: func.vararg_fixed_count,
                type_params: &func.type_params,
            }),
            arg_type_names,
            |arg_type_name, param_jt| self.check_type_match(arg_type_name, param_jt),
            |arg_type_name, param_jt| self.is_exact_type_match(arg_type_name, param_jt),
        );

        best_match.map(|(idx, _)| idx).ok_or_else(|| {
            VmError::MethodError(format!(
                "MethodError: no method matching {}({})",
                func_name,
                arg_type_names.join(", ")
            ))
        })
    }

    pub(in crate::vm) fn dispatch_function_variable_for_values(
        &self,
        func_name: &str,
        candidates: &[(usize, &FunctionInfo)],
        arg_type_names: &[String],
        args: &[Value],
    ) -> Result<Option<usize>, VmError> {
        let origin_compatible = self.origin_compatible_function_candidates(candidates, args);
        let candidate_indices: Vec<usize> = origin_compatible.iter().map(|(idx, _)| *idx).collect();
        let request = self.runtime_call_request(
            Self::call_resolver_callee_identity(func_name),
            &candidate_indices,
            args,
        );
        let value_based = self.resolve_runtime_call_request(&request, args);
        // Constructor methods encode the callable `Type{...}` head in
        // `FunctionInfo::name`, outside the positional signature consumed by
        // the value matcher. Until CallRequest drives full-signature matching,
        // retain the legacy constructor result as an explicit migration bridge
        // instead of imposing a lexicographic head/argument order (Issues
        // #10461/#11610).
        let constructor_bridge = parse_parametric_call(func_name).is_some();
        let legacy = if call_resolver_compare_enabled()
            || constructor_bridge
            || matches!(&value_based, Ok(None))
        {
            Some(self.dispatch_function_variable(func_name, &origin_compatible, arg_type_names))
        } else {
            None
        };

        if call_resolver_compare_enabled() {
            let legacy_resolution = match legacy.as_ref() {
                Some(Ok(idx)) => ResolvedCall::JuliaMethod {
                    method: MethodId(*idx),
                    bindings: TypeBindings::NotObserved,
                },
                Some(Err(error)) => ResolvedCall::Error(Self::call_resolution_error(error)),
                None => ResolvedCall::Error(CallResolutionError::Unsupported(
                    "legacy comparison result missing".to_string(),
                )),
            };
            let proposed_resolution = match &value_based {
                Ok(Some(idx)) => ResolvedCall::JuliaMethod {
                    method: MethodId(*idx),
                    bindings: TypeBindings::NotObserved,
                },
                Ok(None) => ResolvedCall::Error(CallResolutionError::NoMatchingMethod),
                Err(error) => ResolvedCall::Error(Self::call_resolution_error(error)),
            };
            if call_resolutions_differ(&legacy_resolution, &proposed_resolution) {
                call_resolver_compare_log(format_args!(
                    "SJULIA_CALL_RESOLVER_COMPARE: request={request:?} legacy={legacy_resolution:?} proposed={proposed_resolution:?}"
                ));
            }
        }

        if constructor_bridge {
            return Ok(legacy.and_then(Result::ok));
        }

        match value_based {
            Ok(Some(idx)) => Ok(Some(idx)),
            Ok(None) => Ok(legacy.and_then(Result::ok)),
            Err(error) => Err(error),
        }
    }

    /// `invoke` selects a method from its declared signature, not the runtime
    /// value types. Keep that semantic mode explicit while ordinary calls use
    /// the shared value-aware resolver above (Issue #10461).
    fn dispatch_function_variable_for_declared_signature(
        &self,
        func_name: &str,
        candidates: &[(usize, &FunctionInfo)],
        declared_arg_type_names: &[String],
        args: &[Value],
    ) -> Result<usize, VmError> {
        let origin_compatible = self.origin_compatible_function_candidates(candidates, args);
        self.dispatch_function_variable(func_name, &origin_compatible, declared_arg_type_names)
    }

    fn origin_compatible_function_candidates<'a>(
        &self,
        candidates: &[(usize, &'a FunctionInfo)],
        args: &[Value],
    ) -> Vec<(usize, &'a FunctionInfo)> {
        candidates
            .iter()
            .copied()
            .filter(|(idx, func)| {
                crate::vm::expanded_param_types_for_call(func, args.len()).is_none_or(
                    |param_types| {
                        !self.function_candidate_has_nominal_origin_conflict(
                            *idx,
                            args,
                            &param_types,
                            &func.type_params,
                        )
                    },
                )
            })
            .collect()
    }

    fn call_resolution_error(error: &VmError) -> CallResolutionError {
        match error {
            VmError::MethodError(message) if message.contains("ambiguous") => {
                CallResolutionError::AmbiguousMethod
            }
            VmError::MethodError(_) => CallResolutionError::NoMatchingMethod,
            other => CallResolutionError::Unsupported(other.to_string()),
        }
    }
}

#[cfg(test)]
mod call_resolution_tests {
    use super::Vm;
    use crate::inference_core::dispatch_resolver::CalleeIdentity;
    use crate::rng::StableRng;

    #[test]
    fn parametric_constructor_has_structured_callee_identity_10461() {
        assert!(matches!(
            Vm::<StableRng>::call_resolver_callee_identity("Rational{Int64}"),
            CalleeIdentity::Constructor { .. }
        ));
    }
}

#[cfg(test)]
mod merge_kwargs_splat_tests {
    use super::Vm;
    use crate::rng::StableRng;
    use crate::vm::splat::{KwargsMap, SplatPreparation};
    use crate::vm::value::{NamedTupleValue, PairsValue, SymbolValue, TupleValue, Value};
    use crate::vm::VmError;

    fn prepare(source: Value) -> Result<KwargsMap<Value>, VmError> {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        match vm.prepare_kwarg_values(&["options".to_string()], &[true], vec![source])? {
            SplatPreparation::Ready(kwargs) => Ok(kwargs),
            SplatPreparation::Raised => panic!("preparation unexpectedly transferred control"),
        }
    }

    #[test]
    fn splat_named_tuple_inserts_all_fields() {
        let nt = NamedTupleValue {
            names: vec!["a".to_string(), "b".to_string()],
            values: vec![Value::I64(1), Value::I64(2)],
        };
        let map = match prepare(Value::NamedTuple(nt)) {
            Ok(map) => map,
            Err(err) => panic!("NamedTuple preparation failed: {err}"),
        };
        assert!(matches!(map.get("a"), Some(Value::I64(1))));
        assert!(matches!(map.get("b"), Some(Value::I64(2))));
    }

    #[test]
    fn splat_pairs_inserts_all_fields() {
        let pairs = PairsValue::from_named_tuple(NamedTupleValue {
            names: vec!["a".to_string(), "b".to_string()],
            values: vec![Value::I64(1), Value::I64(2)],
        });
        let map = match prepare(Value::Pairs(pairs)) {
            Ok(map) => map,
            Err(err) => panic!("Pairs preparation failed: {err}"),
        };
        assert!(matches!(map.get("a"), Some(Value::I64(1))));
        assert!(matches!(map.get("b"), Some(Value::I64(2))));
    }

    #[test]
    fn splat_tuple_entry_inserts_first_two_fields_and_ignores_extras() {
        let pair = Value::Tuple(TupleValue::new(vec![
            Value::Symbol(SymbolValue::new("k")),
            Value::I64(7),
            Value::I64(99),
        ]));
        let map = match prepare(Value::Tuple(TupleValue::new(vec![pair]))) {
            Ok(map) => map,
            Err(err) => panic!("tuple-entry preparation failed: {err}"),
        };
        assert!(matches!(map.get("k"), Some(Value::I64(7))));
    }

    #[test]
    fn splat_one_field_entry_raises_bounds_error() {
        let source = Value::Tuple(TupleValue::new(vec![Value::Tuple(TupleValue::new(vec![
            Value::Symbol(SymbolValue::new("k")),
        ]))]));
        let error = match prepare(source) {
            Ok(map) => panic!("one-field entry unexpectedly produced {map:?}"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            VmError::TupleIndexOutOfBounds {
                index: 2,
                length: 1
            }
        ));
    }

    #[test]
    fn splat_non_symbol_key_raises_type_error() {
        let source = Value::Tuple(TupleValue::new(vec![Value::Tuple(TupleValue::new(vec![
            Value::I64(0),
            Value::I64(9),
        ]))]));
        let error = match prepare(source) {
            Ok(map) => panic!("non-Symbol key unexpectedly produced {map:?}"),
            Err(error) => error,
        };
        assert!(matches!(error, VmError::TypeError(message) if message.contains("Symbol")));
    }

    #[test]
    fn splat_empty_source_is_valid() {
        let map = match prepare(Value::Tuple(TupleValue::new(Vec::new()))) {
            Ok(map) => map,
            Err(err) => panic!("empty keyword source failed: {err}"),
        };
        assert!(map.is_empty());
    }
}

#[cfg(test)]
mod prefer_candidates_tests {
    use super::Vm;
    use crate::rng::StableRng;
    use crate::types::{JuliaType, TypeParam};
    use crate::vm::splat::KwargsMap;
    use crate::vm::types::{FunctionInfo, KwParamInfo};
    use crate::vm::value::{
        ClosureValue, FunctionValue, NamedTupleValue, PairsValue, SymbolValue, TupleValue, Value,
        ValueType,
    };
    use crate::vm::{CallVarKwargsSplat, Instr, InvokeWithKwargs, VmError};
    use std::rc::Rc;

    fn kwparam(name: &str, is_varargs: bool) -> KwParamInfo {
        KwParamInfo {
            name: name.to_string(),
            default: Value::Nothing,
            default_expr: None,
            ty: ValueType::Any,
            declared_type: None,
            slot: 0,
            required: false,
            is_varargs,
        }
    }

    fn function_info(
        name: &str,
        param_julia_types: Vec<JuliaType>,
        type_params: Vec<TypeParam>,
        kwparams: Vec<KwParamInfo>,
        vararg_param_index: Option<usize>,
    ) -> FunctionInfo {
        FunctionInfo {
            name: name.to_string(),
            params: param_julia_types
                .iter()
                .enumerate()
                .map(|(idx, _)| (format!("x{idx}"), ValueType::Any))
                .collect(),
            kwparams,
            entry: 0,
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            is_lowering_helper: false,
            definition_order: 0,
            min_world: 1,
            type_params,
            param_julia_types,
            code_start: 0,
            code_end: 0,
            slot_names: vec![],
            slot_types: vec![],
            local_slot_count: 0,
            param_slots: vec![],
            vararg_param_index,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            def_line: 0,
            suppress_short_name_alias: false,
            shared_plan: None,
        }
    }

    #[test]
    fn declaring_kw_preference_keeps_kwargs_vararg_candidates_issue_8407() {
        let generic = function_info(
            "quadgk",
            vec![JuliaType::Any, JuliaType::Any],
            vec![],
            vec![kwparam("maxevals", false)],
            Some(1),
        );
        let wrapper = function_info(
            "quadgk",
            vec![
                JuliaType::Struct("BatchIntegrand{Y, Nothing}".to_string()),
                JuliaType::TypeVar("T".to_string(), None),
                JuliaType::TypeVar("T".to_string(), None),
                JuliaType::TypeVar("T".to_string(), None),
            ],
            vec![
                TypeParam::new("Y".to_string()),
                TypeParam::new("T".to_string()),
            ],
            vec![kwparam("kws", true)],
            Some(3),
        );
        let mut kwargs = KwargsMap::new();
        kwargs.insert("maxevals".to_string(), Value::I64(1));
        let candidates = vec![(10, &generic), (20, &wrapper)];

        let preferred = Vm::<StableRng>::prefer_candidates_declaring_kwargs(&candidates, &kwargs);
        let indices: Vec<usize> = preferred.iter().map(|(idx, _)| *idx).collect();
        assert_eq!(indices, vec![10, 20]);
    }

    #[test]
    fn resolved_empty_candidate_list_is_not_refreshed_by_name_11147() {
        let mut vm = Vm::new(Vec::new(), StableRng::new(0));
        vm.functions.push(Rc::new(function_info(
            "QualifiedOnly.Box",
            vec![JuliaType::Int64],
            vec![],
            vec![],
            None,
        )));
        vm.function_name_index
            .insert("QualifiedOnly.Box".to_string(), vec![0]);

        let resolved_empty = FunctionValue::with_candidates("QualifiedOnly.Box", vec![]);
        assert!(
            vm.collect_function_value_candidates(&resolved_empty)
                .is_empty(),
            "Some([]) is an authoritative empty method whitelist"
        );

        let unresolved = FunctionValue::new("QualifiedOnly.Box");
        assert_eq!(vm.collect_function_value_candidates(&unresolved).len(), 1);
    }

    include!("../../../tests/internal/closure_candidate_9784_test.rs");

    #[test]
    fn resolved_empty_splat_expands_before_method_error_11147() {
        let code = vec![Instr::CallFunctionVariableWithSplat(1, vec![true])];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::Tuple(TupleValue::new(vec![
            Value::I64(1),
            Value::F64(2.0),
        ])));
        vm.stack
            .push(Value::Function(FunctionValue::with_candidates(
                "QualifiedOnly.Box",
                vec![],
            )));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("empty whitelist unexpectedly returned {value:?}"),
        };
        assert!(
            matches!(&error, VmError::MethodError(message) if message.contains("Int64, Float64")),
            "MethodError must describe the expanded positional arguments: {error}"
        );
    }

    #[test]
    fn resolved_empty_splat_validates_nothing_before_dispatch_11372() {
        let code = vec![Instr::CallFunctionVariableWithSplat(1, vec![true])];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::Nothing);
        vm.stack
            .push(Value::Function(FunctionValue::with_candidates(
                "QualifiedOnly.Box",
                vec![],
            )));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("invalid `nothing...` unexpectedly returned {value:?}"),
        };
        assert!(
            matches!(&error, VmError::MethodError(message)
                if message.contains("iterate")
                    && message.contains("Nothing")
                    && !message.contains("QualifiedOnly.Box")),
            "`nothing...` must fail through iterate before target dispatch: {error}"
        );
    }

    #[test]
    fn resolved_empty_splat_expands_string_before_dispatch_11372() {
        let code = vec![Instr::CallFunctionVariableWithSplat(1, vec![true])];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::str_new("ab"));
        vm.stack
            .push(Value::Function(FunctionValue::with_candidates(
                "QualifiedOnly.Box",
                vec![],
            )));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("empty whitelist unexpectedly returned {value:?}"),
        };
        assert!(
            matches!(&error, VmError::MethodError(message)
                if message.contains("Char, Char") && !message.contains("String")),
            "String splat must iterate into characters before target dispatch: {error}"
        );
    }

    #[test]
    fn resolved_empty_splat_keeps_number_singleton_before_dispatch_11372() {
        let code = vec![Instr::CallFunctionVariableWithSplat(1, vec![true])];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::I64(2));
        vm.stack
            .push(Value::Function(FunctionValue::with_candidates(
                "QualifiedOnly.Box",
                vec![],
            )));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("empty whitelist unexpectedly returned {value:?}"),
        };
        assert!(
            matches!(&error, VmError::MethodError(message)
                if message.contains("QualifiedOnly.Box(Int64)")),
            "Number splat must remain a valid singleton iterable: {error}"
        );
    }

    #[test]
    fn resolved_empty_kw_splat_merges_before_catchable_method_error_11147() {
        let code = vec![Instr::CallFunctionVariableWithKwargsSplat(Box::new(
            CallVarKwargsSplat {
                arg_count: 1,
                pos_splat_mask: vec![false],
                kwarg_names: vec!["options".to_string()],
                kwargs_splat_mask: vec![true],
            },
        ))];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::I64(1));
        vm.stack.push(Value::NamedTuple(NamedTupleValue {
            names: vec!["unused".to_string()],
            values: vec![Value::I64(2)],
        }));
        vm.stack
            .push(Value::Function(FunctionValue::with_candidates(
                "QualifiedOnly.Box",
                vec![],
            )));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("empty whitelist unexpectedly returned {value:?}"),
        };
        assert!(
            matches!(&error, VmError::MethodError(message) if message.contains("QualifiedOnly.Box(Int64)")),
            "kw-splat dispatch must raise MethodError after argument preparation: {error}"
        );
    }

    fn run_resolved_empty_kw_splat(source: Value) -> VmError {
        let code = vec![Instr::CallFunctionVariableWithKwargsSplat(Box::new(
            CallVarKwargsSplat {
                arg_count: 1,
                pos_splat_mask: vec![false],
                kwarg_names: vec!["options".to_string()],
                kwargs_splat_mask: vec![true],
            },
        ))];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::I64(1));
        vm.stack.push(source);
        vm.stack
            .push(Value::Function(FunctionValue::with_candidates(
                "QualifiedOnly.Box",
                vec![],
            )));
        match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("empty whitelist unexpectedly returned {value:?}"),
        }
    }

    #[test]
    fn resolved_empty_kw_splat_validates_scalar_before_dispatch_11372() {
        let error = run_resolved_empty_kw_splat(Value::I64(2));
        assert!(
            matches!(
                &error,
                VmError::TupleIndexOutOfBounds {
                    index: 2,
                    length: 1
                }
            ),
            "scalar kw splat must raise BoundsError while destructuring before dispatch: {error}"
        );
    }

    #[test]
    fn resolved_empty_kw_splat_validates_tuple_arity_before_dispatch_11372() {
        let source = Value::Tuple(TupleValue::new(vec![Value::Tuple(TupleValue::new(vec![
            Value::Symbol(SymbolValue::new("only_key")),
        ]))]));
        let error = run_resolved_empty_kw_splat(source);
        assert!(
            matches!(
                &error,
                VmError::TupleIndexOutOfBounds {
                    index: 2,
                    length: 1
                }
            ),
            "one-field kw entry must raise BoundsError before dispatch: {error}"
        );
    }

    #[test]
    fn resolved_empty_kw_splat_validates_symbol_key_before_dispatch_11372() {
        let source = Value::Tuple(TupleValue::new(vec![Value::Tuple(TupleValue::new(vec![
            Value::I64(1),
            Value::I64(2),
        ]))]));
        let error = run_resolved_empty_kw_splat(source);
        assert!(
            matches!(&error, VmError::TypeError(message) if message.contains("Symbol")),
            "non-Symbol kw key must raise TypeError before dispatch: {error}"
        );
    }

    #[test]
    fn resolved_empty_kw_splat_accepts_pairs_and_tuple_entries_11372() {
        let pairs = Value::Pairs(PairsValue::from_named_tuple(NamedTupleValue {
            names: vec!["a".to_string()],
            values: vec![Value::I64(1)],
        }));
        let tuple_entries =
            Value::Tuple(TupleValue::new(vec![Value::Tuple(TupleValue::new(vec![
                Value::Symbol(SymbolValue::new("a")),
                Value::I64(1),
                Value::I64(99),
            ]))]));

        for source in [pairs, tuple_entries] {
            let error = run_resolved_empty_kw_splat(source);
            assert!(
                matches!(&error, VmError::MethodError(message)
                    if message.contains("QualifiedOnly.Box(Int64)")),
                "valid kw splat source must reach target dispatch: {error}"
            );
        }
    }

    fn install_accept_all_function(vm: &mut Vm<StableRng>, with_kwargs: bool) {
        let mut func = function_info(
            "accept_all",
            if with_kwargs {
                vec![]
            } else {
                vec![JuliaType::Any]
            },
            vec![],
            if with_kwargs {
                vec![kwparam("options", true)]
            } else {
                vec![]
            },
            (!with_kwargs).then_some(0),
        );
        func.entry = 1;
        func.code_start = 1;
        func.code_end = 3;
        func.local_slot_count = 1;
        func.param_slots = if with_kwargs { vec![] } else { vec![0] };
        vm.functions.push(Rc::new(func));
    }

    #[test]
    fn legacy_call_with_splat_validates_nothing_before_target_11372() {
        let code = vec![
            Instr::CallWithSplat(0, 1, vec![true]),
            Instr::PushBool(true),
            Instr::ReturnAny,
        ];
        let mut vm = Vm::new(code, StableRng::new(0));
        install_accept_all_function(&mut vm, false);
        vm.stack.push(Value::Nothing);

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("legacy splat unexpectedly called target: {value:?}"),
        };
        assert!(
            matches!(&error, VmError::MethodError(message)
                if message.contains("iterate") && message.contains("Nothing")),
            "legacy CallWithSplat must validate through iterate first: {error}"
        );
    }

    #[test]
    fn legacy_call_with_kw_splat_validates_scalar_before_target_11372() {
        let code = vec![
            Instr::CallWithKwargsSplat(0, 0, vec!["options".to_string()], vec![true]),
            Instr::PushBool(true),
            Instr::ReturnAny,
        ];
        let mut vm = Vm::new(code, StableRng::new(0));
        install_accept_all_function(&mut vm, true);
        vm.stack.push(Value::I64(2));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("legacy kw splat unexpectedly called target: {value:?}"),
        };
        assert!(
            matches!(
                &error,
                VmError::TupleIndexOutOfBounds {
                    index: 2,
                    length: 1
                }
            ),
            "legacy CallWithKwargsSplat must validate entry shape first: {error}"
        );
    }

    #[test]
    fn invoke_function_variable_with_kwargs_validates_splat_source_11372() {
        let code = vec![Instr::InvokeFunctionVariableWithKwargs(Box::new(
            InvokeWithKwargs {
                arg_count: 0,
                declared_signature: vec![],
                kwarg_names: vec!["options".to_string()],
                kwargs_splat_mask: vec![true],
            },
        ))];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::I64(2));
        vm.stack
            .push(Value::Function(FunctionValue::new("accept_all")));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("invoke unexpectedly called target: {value:?}"),
        };
        assert!(matches!(
            error,
            VmError::TupleIndexOutOfBounds {
                index: 2,
                length: 1
            }
        ));
    }

    #[test]
    fn dynamic_invoke_function_variable_with_kwargs_validates_splat_source_11372() {
        let code = vec![Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(
            0,
            vec!["options".to_string()],
            vec![true],
        )];
        let mut vm = Vm::new(code, StableRng::new(0));
        vm.stack.push(Value::I64(2));
        vm.stack
            .push(Value::Function(FunctionValue::new("accept_all")));
        // Upstream prepares keyword splats before validating invoke's dynamic
        // signature tuple. The malformed keyword source therefore wins over
        // this deliberately invalid signature value (Issue #11372).
        vm.stack.push(Value::I64(1));

        let error = match vm.run() {
            Err(error) => error,
            Ok(value) => panic!("dynamic invoke unexpectedly called target: {value:?}"),
        };
        assert!(matches!(
            error,
            VmError::TupleIndexOutOfBounds {
                index: 2,
                length: 1
            }
        ));
    }
}
