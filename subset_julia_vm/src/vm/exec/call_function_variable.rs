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
use super::call::{bind_kwargs_defaults, bind_kwargs_with_map};
use super::util::bind_value_to_slot;
use super::DispatchAction;
use crate::builtins::BuiltinId;
use crate::inference_core::dispatch_resolver::{
    resolve_callable_value_candidates, CallableValueCandidate,
};
use crate::rng::RngLike;
use crate::types::JuliaType;
use crate::vm::hof_exec::state::RuntimeCallableResult;
use crate::vm::specialize;
use crate::vm::value::{FunctionValue, GeneratorCallable, GeneratorValue, RangeElementType};
use std::collections::HashMap;

fn module_path_from_function_name(name: &str) -> Option<String> {
    let base = name.split('#').next().unwrap_or(name);
    base.rsplit_once('.')
        .map(|(module_path, _)| module_path.to_string())
}

/// Merge a splatted keyword-argument source (`f(; kw...)`) into `kwargs_map`.
/// Accepts a `NamedTuple`, a `Base.Pairs`, or a tuple of `(:sym, value)` pairs.
/// Extracted from the variadic-kwargs call handler to keep it flat (Issue #6833).
fn merge_kwargs_splat_value(value: &Value, kwargs_map: &mut HashMap<String, Value>) {
    match value {
        Value::NamedTuple(named_tuple) => {
            for (k, v) in named_tuple.names.iter().zip(named_tuple.values.iter()) {
                kwargs_map.insert(k.clone(), v.clone());
            }
        }
        Value::Pairs(pairs) => {
            for (k, v) in pairs.data.names.iter().zip(pairs.data.values.iter()) {
                kwargs_map.insert(k.clone(), v.clone());
            }
        }
        Value::Tuple(tuple) => {
            for elem in &tuple.elements {
                let Value::Tuple(pair) = elem else { continue };
                if pair.elements.len() != 2 {
                    continue;
                }
                let Value::Symbol(key) = &pair.elements[0] else {
                    continue;
                };
                kwargs_map.insert(key.as_str().to_string(), pair.elements[1].clone());
            }
        }
        _ => {}
    }
}

impl<R: RngLike> Vm<R> {
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

    /// Bound callable structs `(self::Type)(args)` register a `__callable_<Type>`
    /// method whose first parameter is the struct instance. The runtime must
    /// prepend that instance to the call arguments so it binds to `self`,
    /// enabling field access like `f.f(f.x, arg)` (Issue #5127).
    ///
    /// Anonymous callable structs `(::Type)(args)` have no such leading
    /// parameter, so their methods match the bare argument count and no
    /// prepend happens. The decision is made by comparing each candidate's
    /// fixed (non-vararg) arity against the supplied argument count.
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
            // A method that already matches the bare argument count is an
            // anonymous-form (or arity-compatible) method — never prepend.
            if info.vararg_param_index.is_none() && fixed == args_len {
                return false;
            }
            if let Some(idx) = info.vararg_param_index {
                if args_len >= idx {
                    return false;
                }
            }
            // A bound-form method expects one extra leading `self` argument.
            if fixed == args_len + 1 {
                needs_self = true;
            } else if let Some(idx) = info.vararg_param_index {
                if args_len + 1 >= idx {
                    needs_self = true;
                }
            }
        }
        needs_self
    }

    fn unary_type_constructor_builtin_name(builtin_id: BuiltinId) -> Option<&'static str> {
        match builtin_id {
            BuiltinId::Bool => Some("Bool"),
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

    fn execute_runtime_builtin_immediate(
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
        let mut candidates = Vec::new();
        for func_name in func_names {
            self.collect_function_variable_candidates_into(func_name.as_ref(), &mut candidates);
        }
        candidates
    }

    fn collect_function_variable_candidates_into<'a>(
        &'a self,
        func_name: &str,
        candidates: &mut Vec<(usize, &'a FunctionInfo)>,
    ) {
        let world = self.current_dispatch_world();
        let exact_indices = self.get_function_indices_by_name(func_name);
        if func_name.contains('.') && !exact_indices.is_empty() {
            for &idx in exact_indices.iter().rev() {
                if self.function_visible_in_world(idx, world)
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
                if self.function_visible_in_world(idx, world)
                    && !candidates
                        .iter()
                        .any(|(existing_idx, _)| *existing_idx == idx)
                {
                    candidates.push((idx, self.functions[idx].as_ref()));
                }
            }
        }
    }

    fn collect_function_value_candidates<'a>(
        &'a self,
        function: &FunctionValue,
    ) -> Vec<(usize, &'a FunctionInfo)> {
        if let Some(indices) = &function.candidate_indices {
            return indices
                .iter()
                .filter_map(|&idx| self.functions.get(idx).map(|func| (idx, func.as_ref())))
                .collect();
        }

        self.collect_function_variable_candidates(&function.name)
    }

    fn collect_runtime_callable_candidates<'a>(
        &'a self,
        func_val: &Value,
        func_name: &str,
    ) -> Result<Vec<(usize, &'a FunctionInfo)>, VmError> {
        match func_val {
            Value::Function(fv) => Ok(self.collect_function_value_candidates(fv)),
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
            _ => Ok(self.collect_function_variable_candidates(func_name)),
        }
    }

    fn prefer_candidates_declaring_kwargs<'a>(
        candidates: &[(usize, &'a FunctionInfo)],
        kwargs_map: &HashMap<String, Value>,
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
                RangeElementType::Float32 => ValueType::F32,
                RangeElementType::Float64 => ValueType::F64,
                RangeElementType::Char => ValueType::Char,
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

    fn runtime_generator_arg_types(
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
        self.specializable_functions
            .iter()
            .find(|func| func.fallback_index == func_index)
            .and_then(|func| {
                specialize::specialize_function(
                    &func.ir,
                    arg_types,
                    struct_defs,
                    &type_object_names,
                    module_path.as_deref(),
                    disable_array_index,
                    disable_field_access,
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
                            let element_type = result_element_type.or_else(|| {
                                self.runtime_generator_result_eltype_for_function(
                                    func_index, &arg_types,
                                )
                            });
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

    pub(crate) fn call_runtime_callable_value(
        &mut self,
        func_val: Value,
        mut args: Vec<Value>,
    ) -> Result<RuntimeCallableResult, VmError> {
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
            _ => {
                return Err(VmError::TypeError(format!(
                    "Expected Function or Closure, got {:?}",
                    func_val
                )))
            }
        };

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
        let lookup_name = Self::runtime_function_lookup_name(&func_name);

        if candidates.is_empty() {
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

        let func_index =
            match self.dispatch_function_variable(&func_name, &candidates, &arg_type_names) {
                Ok(idx) => idx,
                Err(_) => {
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
                    return Err(VmError::MethodError(format!(
                        "no method matching {}({})",
                        func_name,
                        arg_type_names.join(", ")
                    )));
                }
            };

        let func = self.get_function_checked(func_index)?.clone();
        let target_entry = if closure_captures.is_some() {
            None
        } else {
            self.try_specialized_entry_for_runtime_call(func_index, &args)
        }
        .unwrap_or(func.entry);
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
            &HashMap::new(),
        )
    }

    pub(crate) fn invoke_runtime_callable_value_with_signature_and_kwargs(
        &mut self,
        func_val: Value,
        args: Vec<Value>,
        declared_arg_type_names: &[String],
        kwargs_map: &HashMap<String, Value>,
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

        let candidates = match &func_val {
            Value::Function(fv) => self.collect_function_value_candidates(fv),
            _ => self.collect_function_variable_candidates(&func_name),
        };
        if candidates.is_empty() {
            return Err(VmError::TypeError(format!(
                "Function '{}' not found",
                func_name
            )));
        }

        let dispatch_candidates = Self::prefer_candidates_declaring_kwargs(&candidates, kwargs_map);
        let func_index = self.dispatch_function_variable(
            &func_name,
            &dispatch_candidates,
            declared_arg_type_names,
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

    fn expand_kwarg_values(
        kwarg_names: &[String],
        kwargs_splat_mask: &[bool],
        kwarg_values: Vec<Value>,
    ) -> HashMap<String, Value> {
        let mut kwargs_map = HashMap::new();
        for (idx, (name, value)) in kwarg_names.iter().zip(kwarg_values).enumerate() {
            if kwargs_splat_mask.get(idx).copied().unwrap_or(false) {
                match value {
                    Value::NamedTuple(named_tuple) => {
                        for (k, v) in named_tuple.names.into_iter().zip(named_tuple.values) {
                            kwargs_map.insert(k, v);
                        }
                    }
                    Value::Pairs(pairs) => {
                        for (k, v) in pairs.data.names.into_iter().zip(pairs.data.values) {
                            kwargs_map.insert(k, v);
                        }
                    }
                    Value::Tuple(tuple) => {
                        for elem in tuple.elements {
                            let Value::Tuple(pair) = elem else { continue };
                            if pair.elements.len() != 2 {
                                continue;
                            }
                            let Value::Symbol(key) = &pair.elements[0] else {
                                continue;
                            };
                            kwargs_map.insert(key.as_str().to_string(), pair.elements[1].clone());
                        }
                    }
                    _ => {}
                }
            } else {
                kwargs_map.insert(name.clone(), value);
            }
        }
        kwargs_map
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
                    match self.dispatch_function_variable(
                        &qualified_name,
                        &candidates,
                        &arg_type_names,
                    ) {
                        Ok(idx) => Some(idx),
                        Err(_) => {
                            return Err(VmError::MethodError(format!(
                                "no method matching {}({})",
                                qualified_name,
                                arg_type_names.join(", ")
                            )));
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
                    Value::DataType(jt) => (jt.name().to_string(), None),
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
                    _ => {
                        // User-visible: user can call a non-function value stored in a variable
                        return Err(VmError::TypeError(format!(
                            "Expected Function or Closure, got {:?}",
                            func_val
                        )));
                    }
                };

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
                let lookup_name = Self::runtime_function_lookup_name(&func_name);

                if candidates.is_empty() {
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
                // If user-defined dispatch fails, try builtin fallback (Issue #2546).
                // This handles cases like sqrt(Float64) where user-defined methods only
                // exist for Complex types but the builtin handles Float64.
                let func_index =
                    match self.dispatch_function_variable(&func_name, &candidates, &arg_type_names)
                    {
                        Ok(idx) => idx,
                        Err(_) => {
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
                                if let Some(value) = self.execute_runtime_builtin_immediate(
                                    builtin_id, &func_name, &args,
                                )? {
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
                            return Err(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_name,
                                arg_type_names.join(", ")
                            )));
                        }
                    };

                let func = self.get_function_checked(func_index)?.clone();
                let target_entry = if closure_captures.is_some() {
                    None
                } else {
                    self.try_specialized_entry_for_runtime_call(func_index, &args)
                }
                .unwrap_or(func.entry);

                let mut frame = if let Some(captures) = closure_captures {
                    self.acquire_frame_with_captures(
                        func.local_slot_count,
                        Some(func_index),
                        &captures,
                    )
                } else {
                    self.acquire_frame(func.local_slot_count, Some(func_index))
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
                let arg_count = operands.arg_count;
                let declared_arg_type_names = &operands.declared_signature;
                let kwarg_names = &operands.kwarg_names;
                let kwargs_splat_mask = &operands.kwargs_splat_mask;
                let func_val = self.stack.pop_value()?;

                let mut kwarg_values = Vec::with_capacity(kwarg_names.len());
                for _ in 0..kwarg_names.len() {
                    kwarg_values.push(self.stack.pop_value()?);
                }
                kwarg_values.reverse();

                let kwargs_map =
                    Self::expand_kwarg_values(kwarg_names, kwargs_splat_mask, kwarg_values);

                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

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
                let sig_val = self.stack.pop_value()?;
                let declared_arg_type_names = Self::runtime_invoke_signature_type_names(&sig_val)?;
                let func_val = self.stack.pop_value()?;

                let mut kwarg_values = Vec::with_capacity(kwarg_names.len());
                for _ in 0..kwarg_names.len() {
                    kwarg_values.push(self.stack.pop_value()?);
                }
                kwarg_values.reverse();

                let kwargs_map =
                    Self::expand_kwarg_values(kwarg_names, kwargs_splat_mask, kwarg_values);

                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

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
            }

            Instr::CallFunctionVariableWithKwargsSplat(operands) => {
                let arg_count = &operands.arg_count;
                let splat_mask = &operands.pos_splat_mask;
                let kwarg_names = &operands.kwarg_names;
                let kwargs_splat_mask = &operands.kwargs_splat_mask;
                // Stack layout: [args..., kwarg_values..., function_value]
                let func_val = self.stack.pop_value()?;

                let mut kwarg_values: Vec<Value> = Vec::with_capacity(kwarg_names.len());
                for _ in 0..kwarg_names.len() {
                    kwarg_values.push(self.stack.pop_value()?);
                }
                kwarg_values.reverse();

                let mut kwargs_map: HashMap<String, Value> = HashMap::new();
                for (idx, (name, value)) in kwarg_names.iter().zip(kwarg_values).enumerate() {
                    if kwargs_splat_mask.get(idx).copied().unwrap_or(false) {
                        merge_kwargs_splat_value(&value, &mut kwargs_map);
                    } else {
                        kwargs_map.insert(name.clone(), value);
                    }
                }

                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();
                let mut expanded_args = super::super::splat::expand_splat_arguments_with_heap(
                    args,
                    splat_mask,
                    &self.struct_heap,
                )?;

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
                    Value::DataType(jt) if kwargs_map.is_empty() => (jt.name().to_string(), None),
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
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "Expected Function or Closure, got {:?}",
                            func_val
                        )));
                    }
                };

                let candidates = self.collect_runtime_callable_candidates(&func_val, &func_name)?;

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
                    if kwargs_map.is_empty() {
                        if let Value::DataType(_) = &func_val {
                            if self.try_construct_default_datatype(&func_name, &expanded_args)? {
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
                    return Err(VmError::TypeError(format!(
                        "Function '{}' not found",
                        func_name
                    )));
                }

                let dispatch_candidates =
                    Self::prefer_candidates_declaring_kwargs(&candidates, &kwargs_map);
                let func_index = match self.dispatch_function_variable(
                    &func_name,
                    &dispatch_candidates,
                    &arg_type_names,
                ) {
                    Ok(idx) => idx,
                    Err(_) => {
                        if kwargs_map.is_empty() {
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
                        return Err(VmError::MethodError(format!(
                            "no method matching {}({})",
                            func_name,
                            arg_type_names.join(", ")
                        )));
                    }
                };

                let func = self.get_function_checked(func_index)?.clone();
                let target_entry = if closure_captures.is_some() || !kwargs_map.is_empty() {
                    None
                } else {
                    self.try_specialized_entry_for_runtime_call(func_index, &expanded_args)
                }
                .unwrap_or(func.entry);

                let mut frame = if let Some(captures) = closure_captures {
                    self.acquire_frame_with_captures(
                        func.local_slot_count,
                        Some(func_index),
                        &captures,
                    )
                } else {
                    self.acquire_frame(func.local_slot_count, Some(func_index))
                };

                self.bind_type_params(&func, &expanded_args, &mut frame);

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
                        bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
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

                if let Some(result) =
                    self.try_eval_cached_generated_expr(func_index, &func, &expanded_args, &frame)?
                {
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
            }

            Instr::CallFunctionVariableWithSplat(arg_count, ref splat_mask) => {
                // Call a Function or Closure stored in a local variable with splatted arguments.
                // This handles patterns like: function apply_variadic(f, args...); f(args...); end
                // Stack layout: [args..., function_value]

                // Pop the Function/Closure value first
                let func_val = self.stack.pop_value()?;

                // Pop arguments
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    args.push(self.stack.pop_value()?);
                }
                args.reverse();

                // Expand splatted arguments
                let expanded_args = super::super::splat::expand_splat_arguments_with_heap(
                    args,
                    splat_mask,
                    &self.struct_heap,
                )?;

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

                // Get runtime type names for all expanded arguments
                let arg_type_names = self.callable_dispatch_type_names(&expanded_args);

                // Find all methods with the matching function name and do proper dispatch
                // Use function_name_index for O(1) lookup (Issue #3361)
                let candidates = match &func_val {
                    Value::Function(fv) => self.collect_function_value_candidates(fv),
                    _ => self.collect_function_variable_candidates(&func_name),
                };
                let lookup_name = Self::runtime_function_lookup_name(&func_name);

                if candidates.is_empty() {
                    if matches!(&func_val, Value::Function(_))
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
                    if let Some(result) = self.try_call_intrinsic(lookup_name, &expanded_args)? {
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
                // If user-defined dispatch fails, try builtin fallback (Issue #2546).
                let func_index =
                    match self.dispatch_function_variable(&func_name, &candidates, &arg_type_names)
                    {
                        Ok(idx) => idx,
                        Err(_) => {
                            // Splat expansion can reveal a field-count default
                            // constructor arity even when the named function has
                            // only outer-constructor methods registered.
                            // (Issue #8321)
                            if matches!(&func_val, Value::Function(_))
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
                            return Err(VmError::MethodError(format!(
                                "no method matching {}({})",
                                func_name,
                                arg_type_names.join(", ")
                            )));
                        }
                    };

                let func = self.get_function_checked(func_index)?.clone();
                let target_entry = if closure_captures.is_some() {
                    None
                } else {
                    self.try_specialized_entry_for_runtime_call(func_index, &expanded_args)
                }
                .unwrap_or(func.entry);

                let mut frame = if let Some(captures) = closure_captures {
                    self.acquire_frame_with_captures(
                        func.local_slot_count,
                        Some(func_index),
                        &captures,
                    )
                } else {
                    self.acquire_frame(func.local_slot_count, Some(func_index))
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
                    self.try_eval_cached_generated_expr(func_index, &func, &expanded_args, &frame)?
                {
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
                let v = value_to_f64_with_heap(&args[0], &self.struct_heap)?;
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
                return Ok(Some(apply_unary_float_op_with_heap(
                    args[0].clone(),
                    &self.struct_heap,
                    op,
                )?));
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

        if field_count != args.len() {
            return Err(VmError::MethodError(format!(
                "no method matching {}({})",
                type_name,
                args.iter()
                    .map(|arg| self.get_type_name(arg))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
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
        super::struct_ops::coerce_fields_to_declared_types(struct_def.as_ref(), &mut field_values);

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
    /// constructor path ([`crate::compile::infer_parametric_type_args`]), so the
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
            return Err(VmError::MethodError(format!(
                "no method matching {}({})",
                type_name,
                args.iter()
                    .map(|arg| self.get_type_name(arg))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
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
                match crate::compile::infer_parametric_type_args(&def, &base_name, &arg_types) {
                    Ok(type_args) => type_args,
                    Err(_) => {
                        return Err(VmError::MethodError(format!(
                            "no method matching {}({})",
                            type_name,
                            args.iter()
                                .map(|arg| self.get_type_name(arg))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )));
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
    fn resolve_runtime_parametric_def(
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
    pub(super) fn dispatch_function_variable(
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
}

#[cfg(test)]
mod merge_kwargs_splat_tests {
    use super::merge_kwargs_splat_value;
    use crate::vm::value::{NamedTupleValue, SymbolValue, TupleValue, Value};
    use std::collections::HashMap;

    #[test]
    fn splat_named_tuple_inserts_all_fields() {
        let mut map: HashMap<String, Value> = HashMap::new();
        let nt = NamedTupleValue {
            names: vec!["a".to_string(), "b".to_string()],
            values: vec![Value::I64(1), Value::I64(2)],
        };
        merge_kwargs_splat_value(&Value::NamedTuple(nt), &mut map);
        assert!(matches!(map.get("a"), Some(Value::I64(1))));
        assert!(matches!(map.get("b"), Some(Value::I64(2))));
    }

    #[test]
    fn splat_tuple_of_symbol_pairs_inserts() {
        let mut map: HashMap<String, Value> = HashMap::new();
        let pair = Value::Tuple(TupleValue::new(vec![
            Value::Symbol(SymbolValue::new("k")),
            Value::I64(7),
        ]));
        merge_kwargs_splat_value(&Value::Tuple(TupleValue::new(vec![pair])), &mut map);
        assert!(matches!(map.get("k"), Some(Value::I64(7))));
    }

    #[test]
    fn splat_malformed_tuple_pairs_are_skipped() {
        let mut map: HashMap<String, Value> = HashMap::new();
        // A 1-element "pair" and a non-Symbol key are both ignored.
        let bad_arity = Value::Tuple(TupleValue::new(vec![Value::I64(1)]));
        let bad_key = Value::Tuple(TupleValue::new(vec![Value::I64(0), Value::I64(9)]));
        merge_kwargs_splat_value(
            &Value::Tuple(TupleValue::new(vec![bad_arity, bad_key])),
            &mut map,
        );
        assert!(map.is_empty());
    }

    #[test]
    fn splat_other_value_is_noop() {
        let mut map: HashMap<String, Value> = HashMap::new();
        merge_kwargs_splat_value(&Value::I64(5), &mut map);
        assert!(map.is_empty());
    }
}

#[cfg(test)]
mod prefer_candidates_tests {
    use super::Vm;
    use crate::rng::StableRng;
    use crate::types::{JuliaType, TypeParam};
    use crate::vm::types::{FunctionInfo, KwParamInfo};
    use crate::vm::value::{Value, ValueType};
    use std::collections::HashMap;

    fn kwparam(name: &str, is_varargs: bool) -> KwParamInfo {
        KwParamInfo {
            name: name.to_string(),
            default: Value::Nothing,
            default_expr: None,
            ty: ValueType::Any,
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
        let mut kwargs = HashMap::new();
        kwargs.insert("maxevals".to_string(), Value::I64(1));
        let candidates = vec![(10, &generic), (20, &wrapper)];

        let preferred = Vm::<StableRng>::prefer_candidates_declaring_kwargs(&candidates, &kwargs);
        let indices: Vec<usize> = preferred.iter().map(|(idx, _)| *idx).collect();
        assert_eq!(indices, vec![10, 20]);
    }
}
