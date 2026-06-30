//! Type builtin functions for the VM.
//!
//! Type operations: typeof, isa, convert, promote, subtype checks.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::inference_core::CoreType;
use crate::vm::value::is_native_array_value;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_objects::RuntimeTypeRegistry;
use super::type_utils::{normalize_type_for_isa, type_values_subtype};
use super::value::{
    julia_array_type_for_ndims, native_array_value_ref, ArrayElementType, DictKey,
    GeneratorCallable, MemoryValue, RuntimeTypeVarValue, SymbolValue, Value,
};
use super::Vm;

/// Validate the argument count of a fixed-arity builtin, mirroring upstream
/// Julia's `JL_NARGS(fname, min, max)` machinery (`julia/src/julia.h` /
/// `julia/src/rtutils.c`). On a mismatch this raises a catchable
/// `ArgumentError` whose message matches `jl_too_few_args` /
/// `jl_too_many_args` exactly (Issue #5493). The message is carried via
/// `VmError::TypeError` with the `ArgumentError:` prefix, the established
/// convention for VM-surfaced `ArgumentError`s.
fn check_builtin_arity(fname: &str, argc: usize, expected: usize) -> Result<(), VmError> {
    if argc < expected {
        Err(VmError::TypeError(format!(
            "ArgumentError: {}: too few arguments (expected {})",
            fname, expected
        )))
    } else if argc > expected {
        Err(VmError::TypeError(format!(
            "ArgumentError: {}: too many arguments (expected {})",
            fname, expected
        )))
    } else {
        Ok(())
    }
}

fn reflection_type_name(type_val: &Value) -> Option<String> {
    match type_val {
        Value::DataType(jt) => Some(jt.name().to_string()),
        Value::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn reflection_julia_type(type_name: &str) -> crate::types::JuliaType {
    crate::types::JuliaType::from_name(type_name)
        .unwrap_or_else(|| crate::types::JuliaType::Struct(type_name.to_string()))
}

/// Recover the type name an operand of `<:`/`>:` denotes.
///
/// Bare `Ref`/`RefValue` evaluate to their callable constructors rather than
/// `DataType` values, so the abstract-type name must be recovered from the
/// function name for `<:` to work (e.g. `Ref{Int} <: Ref`) (Issue #5130).
/// Shared by the `<:` (Subtype) and `>:` (SupertypeOp) builtins (Issue #5115).
fn subtype_operand_name(v: &Value) -> String {
    match v {
        Value::DataType(jt) => jt.name().to_string(),
        Value::Str(s) => s.clone(),
        Value::Struct(s) => s
            .array_wrapper_julia_type()
            .map(|jt| jt.name().to_string())
            .unwrap_or_else(|| s.struct_name.to_string()),
        Value::Function(f) if f.name == "Ref" || f.name == "RefValue" => f.name.clone(),
        _ => format!("{:?}", v),
    }
}

fn is_core_datatype_subtype_operand(ty: &crate::types::JuliaType) -> bool {
    !matches!(
        ty,
        crate::types::JuliaType::Struct(_) | crate::types::JuliaType::AbstractUser(_, _)
    )
}

fn core_datatype_typeintersect_subtype_result(
    left: &Value,
    right: &Value,
) -> Option<crate::types::JuliaType> {
    let (Value::DataType(left_ty), Value::DataType(right_ty)) = (left, right) else {
        return None;
    };
    if !is_core_datatype_subtype_operand(left_ty) || !is_core_datatype_subtype_operand(right_ty) {
        return None;
    }

    let left_core = CoreType::from(left_ty.as_ref());
    let right_core = CoreType::from(right_ty.as_ref());
    if left_core.is_subtype_of(&right_core) {
        Some(*left_ty.clone())
    } else if right_core.is_subtype_of(&left_core) {
        Some(*right_ty.clone())
    } else {
        None
    }
}

/// Issue #4694: walk a `JuliaType` looking for a `TypeVar` whose `name`
/// matches `var_name`, also scanning embedded parametric type strings such
/// as `"Vector{T}"` or `"Dict{K, V}"`. Used by the `UnionAll(var, body)`
/// constructor to skip wrapping when the body does not reference the
/// bound variable (matching upstream `jl_type_unionall`).
fn julia_type_references_typevar(ty: &crate::types::JuliaType, var_name: &str) -> bool {
    use crate::types::JuliaType;
    match ty {
        JuliaType::TypeVar(name, bound) => {
            name == var_name
                || bound
                    .as_deref()
                    .is_some_and(|bound| type_name_references_typevar(bound, var_name))
        }
        JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_references_typevar(inner, var_name)
        }
        JuliaType::TypeOf(inner) => julia_type_references_typevar(inner, var_name),
        JuliaType::TupleOf(elements) | JuliaType::Union(elements) => elements
            .iter()
            .any(|elem| julia_type_references_typevar(elem, var_name)),
        JuliaType::UnionAll {
            lower_bound,
            var,
            bound,
            body,
        } => {
            // The inner UnionAll shadows the same-named variable; only
            // recurse into its body when the names differ.
            lower_bound
                .as_deref()
                .is_some_and(|lower| type_name_references_typevar(lower, var_name))
                || bound
                    .as_deref()
                    .is_some_and(|upper| type_name_references_typevar(upper, var_name))
                || (var != var_name && julia_type_references_typevar(body, var_name))
        }
        JuliaType::Struct(name) => type_name_references_typevar(name, var_name),
        _ => false,
    }
}

fn type_name_references_typevar(name: &str, var_name: &str) -> bool {
    let bytes = name.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b'_' {
            let start = idx;
            while idx < bytes.len() && (bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b'_') {
                idx += 1;
            }
            if &name[start..idx] == var_name {
                return true;
            }
        } else {
            idx += 1;
        }
    }
    false
}

fn type_object_argument(value: Value) -> Result<crate::types::JuliaType, VmError> {
    match value {
        Value::DataType(jt) => Ok(*jt),
        Value::RuntimeTypeVar(tv) => Ok(tv.projection()),
        other => Err(VmError::TypeError(format!(
            "TypeVar bound must be a Type or TypeVar, got {:?}",
            other.value_type()
        ))),
    }
}

fn array_value_size_bytes(element_type: &ArrayElementType, shape: &[usize]) -> i64 {
    let total_elems: i64 = shape.iter().map(|&d| d as i64).product();
    array_element_type_size_bytes(element_type) * total_elems
}

fn array_element_type_size_bytes(element_type: &ArrayElementType) -> i64 {
    match element_type {
        ArrayElementType::F64 | ArrayElementType::I64 | ArrayElementType::U64 => 8,
        ArrayElementType::F32 | ArrayElementType::I32 | ArrayElementType::U32 => 4,
        ArrayElementType::I16 | ArrayElementType::U16 => 2,
        ArrayElementType::I8 | ArrayElementType::U8 | ArrayElementType::Bool => 1,
        ArrayElementType::Char => 4,
        ArrayElementType::I128 | ArrayElementType::U128 | ArrayElementType::ComplexF64 => 16,
        ArrayElementType::ComplexF32 => 8,
        ArrayElementType::Nothing => 0,
        ArrayElementType::String
        | ArrayElementType::SubString
        | ArrayElementType::Symbol
        | ArrayElementType::Struct
        | ArrayElementType::StructOf(_)
        | ArrayElementType::StructInlineOf(_, _)
        | ArrayElementType::Any
        | ArrayElementType::TupleOf(_)
        | ArrayElementType::UnionOf(_)
        | ArrayElementType::Abstract(_) => 8,
    }
}

fn memory_value_size_bytes(memory: &MemoryValue) -> i64 {
    array_element_type_size_bytes(memory.element_type()) * memory.len() as i64
}

fn type_param_matches_memory_element(param: &str, element_type_name: &str) -> bool {
    let normalized = normalize_type_for_isa(param);
    let param = normalized.as_ref();
    if let Some(bound) = param.strip_prefix("<:") {
        let element_type = crate::types::JuliaType::from_name_or_struct(element_type_name);
        let bound_type = crate::types::JuliaType::from_name_or_struct(bound.trim());
        return type_values_subtype(&element_type, &bound_type);
    }
    param == element_type_name
}

fn memory_isa_target(memory: &MemoryValue, target_type_name: &str) -> bool {
    let normalized_target = normalize_type_for_isa(target_type_name);
    let target = normalized_target.as_ref();
    let base = target.find('{').map_or(target, |idx| &target[..idx]);
    let params = crate::vm::util::parse_parametric_params(target);
    let element_type_name = memory.element_type().julia_type_name();

    let element_param_matches = |idx: usize| {
        params
            .get(idx)
            .is_some_and(|param| type_param_matches_memory_element(param, &element_type_name))
    };
    let rank_param_matches = |idx: usize| params.get(idx).is_none_or(|rank| *rank == "1");

    match base {
        "Any" | "Memory" => params.is_empty() || element_param_matches(0),
        "GenericMemory" => {
            params.is_empty()
                || match params.len() {
                    1 => element_param_matches(0),
                    _ => {
                        let kind_matches = params
                            .first()
                            .is_none_or(|kind| matches!(*kind, ":not_atomic" | "not_atomic"));
                        kind_matches && element_param_matches(1)
                    }
                }
        }
        "AbstractArray" | "DenseArray" => {
            params.is_empty() || (element_param_matches(0) && rank_param_matches(1))
        }
        "AbstractVector" | "DenseVector" => params.is_empty() || element_param_matches(0),
        "Array" | "Vector" | "Matrix" | "AbstractMatrix" | "DenseMatrix" => false,
        _ => false,
    }
}

impl<R: crate::rng::RngLike> Vm<R> {
    pub(in crate::vm) fn reflection_supertype_name(&self, type_name: &str) -> String {
        RuntimeTypeRegistry::new_with_struct_defs(
            self.compile_context.as_ref(),
            &self.abstract_types,
            &self.struct_defs,
        )
        .supertype_name(type_name)
    }

    /// Execute type builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a type builtin.
    pub(super) fn execute_builtin_types(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        if self
            .execute_builtin_types_conversion(builtin, argc)?
            .is_some()
        {
            return Ok(Some(()));
        }

        match builtin {
            BuiltinId::TypeVar => {
                if argc != 1 && argc != 3 {
                    return Err(VmError::TypeError(
                        "TypeVar requires 1 or 3 arguments".to_string(),
                    ));
                }

                let (name_val, lower_bound, upper_bound) = if argc == 1 {
                    (
                        self.stack.pop_value()?,
                        crate::types::JuliaType::Bottom,
                        crate::types::JuliaType::Any,
                    )
                } else {
                    let upper = self.stack.pop_value()?;
                    let lower = self.stack.pop_value()?;
                    let name = self.stack.pop_value()?;
                    (
                        name,
                        type_object_argument(lower)?,
                        type_object_argument(upper)?,
                    )
                };

                let name = match name_val {
                    Value::Symbol(symbol) => symbol.into_string(),
                    Value::QuoteNode(inner) => match *inner {
                        Value::Symbol(symbol) => symbol.into_string(),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "TypeVar name must be a Symbol, got {:?}",
                                other.value_type()
                            )));
                        }
                    },
                    other => {
                        return Err(VmError::TypeError(format!(
                            "TypeVar name must be a Symbol, got {:?}",
                            other.value_type()
                        )));
                    }
                };

                let id = self.runtime_typevar_counter;
                self.runtime_typevar_counter += 1;
                self.stack
                    .push(Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
                        id,
                        name,
                        lower_bound,
                        upper_bound,
                    })));
            }

            BuiltinId::UnionAll => {
                // Issue #4694: UnionAll(var::TypeVar, body) constructs a
                // `JuliaType::UnionAll` so Pure Julia helpers such as
                // `Base.rewrap_unionall` and `Base.rename_unionall` can wrap a
                // body back into a UnionAll after `unwrap_unionall` peeled the
                // existing layers (Issue #3909).
                if argc != 2 {
                    return Err(VmError::TypeError(
                        "UnionAll requires 2 arguments (var::TypeVar, body)".to_string(),
                    ));
                }
                let body_val = self.stack.pop_value()?;
                let var_val = self.stack.pop_value()?;

                let (var_name, lower, bound) = match &var_val {
                    Value::RuntimeTypeVar(tv) => {
                        let bound = if matches!(tv.upper_bound, crate::types::JuliaType::Any) {
                            None
                        } else {
                            Some(tv.upper_bound.name().to_string())
                        };
                        // A `Union{}` (Bottom) lower bound is the implicit default
                        // and is not displayed; only a declared lower bound (e.g.
                        // `where Int<:T`) is carried through (#5650).
                        let lower = if matches!(tv.lower_bound, crate::types::JuliaType::Bottom) {
                            None
                        } else {
                            Some(tv.lower_bound.name().to_string())
                        };
                        (tv.name.clone(), lower, bound)
                    }
                    Value::DataType(jt)
                        if matches!(jt.as_ref(), crate::types::JuliaType::TypeVar(..)) =>
                    {
                        let crate::types::JuliaType::TypeVar(name, bound) = jt.as_ref() else {
                            unreachable!()
                        };
                        (name.clone(), None, bound.clone())
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "UnionAll var must be a TypeVar, got {:?}",
                            other.value_type()
                        )));
                    }
                };

                let body = match body_val {
                    Value::DataType(jt) => jt,
                    Value::RuntimeTypeVar(tv) => Box::new(tv.projection()),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "UnionAll body must be a Type, got {:?}",
                            other.value_type()
                        )));
                    }
                };

                // Match upstream `jl_type_unionall`: if the body does not
                // reference the bound variable, return the body unchanged.
                // This keeps `rewrap_unionall(Int64, Vector) === Int64` and
                // `UnionAll(T, Vector{Int64}) === Vector{Int64}` in agreement
                // with Julia's `Vector{Int64}` (not a UnionAll).
                let projection = if matches!(body.as_ref(), crate::types::JuliaType::TypeVar(name, _) if name == &var_name)
                {
                    // A bare bound variable body denotes its upper bound:
                    // `T where T === Any`, `T where T<:Real === Real`
                    // (Issue #5570).
                    bound
                        .as_deref()
                        .map(crate::types::JuliaType::from_name_or_struct)
                        .unwrap_or(crate::types::JuliaType::Any)
                } else if julia_type_references_typevar(&body, &var_name) {
                    crate::types::JuliaType::UnionAll {
                        lower_bound: lower.map(Box::new),
                        var: var_name,
                        bound: bound.map(Box::new),
                        body,
                    }
                } else {
                    *body
                };
                self.stack.push(Value::DataType(Box::new(projection)));
            }

            BuiltinId::TypeOf => {
                let val = self.stack.pop_value()?;

                // Check if this is a type name literal (from lowered ParametrizedTypeExpression)
                if let Value::Str(type_name_str) = &val {
                    if let Some(parsed_type) = crate::types::JuliaType::from_name(type_name_str) {
                        self.stack.push(Value::DataType(Box::new(parsed_type)));
                        return Ok(Some(()));
                    }
                    // Parametric type-name literals start with an uppercase letter
                    // (e.g. `Vector{Int64}`). The canonical named-tuple form
                    // `@NamedTuple{a::Int64, b::String}` (emitted by the
                    // `@NamedTuple` macro, Issue #5120) starts with `@` instead,
                    // so accept that prefix explicitly.
                    if type_name_str.contains('{')
                        && (type_name_str
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                            || type_name_str.starts_with("@NamedTuple{"))
                    {
                        let parsed_type =
                            crate::types::JuliaType::from_name_or_struct(type_name_str);
                        self.stack.push(Value::DataType(Box::new(parsed_type)));
                        return Ok(Some(()));
                    }
                }

                let julia_type = match &val {
                    // Issue #5335: typeof(Union{...}) is `Union`, not `DataType`.
                    // A union type object projects to the `Union` meta-type (which
                    // renders as "Union"), matching upstream Julia.
                    Value::DataType(jt)
                        if matches!(jt.as_ref(), crate::types::JuliaType::Union(_)) =>
                    {
                        crate::types::JuliaType::Struct("Union".to_string())
                    }
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        registry.object(jt).runtime_type_projection()
                    }
                    Value::RuntimeTypeVar(_) => {
                        crate::types::JuliaType::Struct("TypeVar".to_string())
                    }
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            // Special case: Generator struct maps to Base.Generator type
                            if let Some(array_type) = self.array_wrapper_julia_type_resolved(s) {
                                array_type
                            } else if &*s.struct_name == "Generator" {
                                crate::types::JuliaType::Generator
                            } else {
                                crate::types::JuliaType::Struct(s.struct_name.to_string())
                            }
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                    Value::Struct(s) => {
                        if let Some(array_type) = self.array_wrapper_julia_type_resolved(s) {
                            array_type
                        } else if s.struct_name.is_empty() {
                            crate::types::JuliaType::Any
                        } else if &*s.struct_name == "Generator" {
                            // Special case: Generator struct maps to Base.Generator type
                            crate::types::JuliaType::Generator
                        } else {
                            crate::types::JuliaType::Struct(s.struct_name.to_string())
                        }
                    }
                    _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                        Some(arr) => {
                            let arr_borrow = arr.borrow();
                            if let Some(container_type) = arr_borrow.array_type_override() {
                                crate::types::JuliaType::Struct(container_type.to_string())
                            } else {
                                let elem_type =
                                    self.array_value_declared_element_julia_type(&arr_borrow);
                                julia_array_type_for_ndims(elem_type, arr_borrow.shape.len())
                            }
                        }
                        None => val.runtime_type(),
                    },
                    Value::Memory(mem) => {
                        let mem = mem.borrow();
                        let elem_type_name = self.memory_element_type_name(mem.element_type());
                        crate::types::JuliaType::Struct(format!("Memory{{{}}}", elem_type_name))
                    }
                    Value::NamedTuple(nt) => {
                        // Julia shows @NamedTuple{a::T1, b::T2} (short form)
                        let fields: Vec<String> = nt
                            .names
                            .iter()
                            .zip(nt.values.iter())
                            .map(|(name, val)| format!("{}::{}", name, self.get_type_name(val)))
                            .collect();
                        crate::types::JuliaType::Struct(format!(
                            "@NamedTuple{{{}}}",
                            fields.join(", ")
                        ))
                    }
                    Value::Pairs(p) => {
                        crate::types::JuliaType::Struct(self.pairs_runtime_type_name(p))
                    }
                    Value::Tuple(t) => {
                        let types = t
                            .elements
                            .iter()
                            .map(|element| {
                                let type_name = self.get_type_name(element);
                                crate::types::JuliaType::from_name_or_struct(&type_name)
                            })
                            .collect();
                        crate::types::JuliaType::TupleOf(types)
                    }
                    Value::Generator(g) => {
                        let iter_type = self.get_type_name(g.iter.as_ref());
                        let callable_type = match &g.callable {
                            GeneratorCallable::TypeObject(jt) => {
                                format!("Type{{{}}}", jt.name())
                            }
                            GeneratorCallable::TupleSplatTypeObject(jt) => {
                                format!("Type{{{}}}", jt.name())
                            }
                            GeneratorCallable::FunctionIndex(func_index) => self
                                .functions
                                .get(*func_index)
                                .map(|func| format!("typeof({})", func.name))
                                .unwrap_or_else(|| "Function".to_string()),
                            GeneratorCallable::FilteredFunctionIndex { .. } => {
                                "Function".to_string()
                            }
                            GeneratorCallable::TupleSplatFunctionIndex(func_index) => self
                                .functions
                                .get(*func_index)
                                .map(|func| format!("typeof({})", func.name))
                                .unwrap_or_else(|| "Function".to_string()),
                            GeneratorCallable::RuntimeValue(callable)
                            | GeneratorCallable::TupleSplatRuntimeValue(callable) => {
                                if let Value::Function(function) = callable.as_ref() {
                                    format!("typeof({})", function.name)
                                } else {
                                    self.get_type_name(callable.as_ref())
                                }
                            }
                            GeneratorCallable::Eager => "Any".to_string(),
                        };
                        crate::types::JuliaType::Struct(format!(
                            "Base.Generator{{{}, {}}}",
                            iter_type, callable_type
                        ))
                    }
                    Value::Function(function) => {
                        crate::types::JuliaType::Struct(format!("typeof({})", function.name))
                    }
                    _ => val.runtime_type(),
                };
                self.stack.push(Value::DataType(Box::new(julia_type)));
            }

            BuiltinId::Isa => {
                // Upstream `jl_f_isa` validates `JL_NARGS(isa, 2, 2)` before
                // touching its arguments (Issue #5493). Without this the
                // immediate call path underflowed on `isa(x)` and silently
                // ignored extras on `isa(x, T, extra)`.
                check_builtin_arity("isa", argc, 2)?;
                let type_val = self.stack.pop_value()?;
                let val = self.stack.pop_value()?;

                let target_type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    // Allow constructor functions as type arguments (e.g., isa(x, Ref))
                    // In Julia, Ref is both a type and a callable constructor (Issue #2687)
                    Value::Function(f) => f.name.clone(),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "isa: second argument must be a type, got {:?}",
                            type_val
                        )));
                    }
                };

                // `isa(x, Type{B})` is true iff `x` is itself a type and `x`
                // satisfies the `Type{B}` parameter: invariant `x === B` for a
                // concrete `B`, covariant `x <: B` for the `Type{<:B}` spelling,
                // and any type for the unbounded `Type{T} where T` ≡ `Type`
                // (Issue #5068). A non-type value (e.g. the integer `3`) is never
                // `isa` a `Type{...}`.
                if let Some(crate::types::JuliaType::TypeOf(inner)) =
                    crate::types::JuliaType::from_name(&target_type_name)
                {
                    let value_type_object = match &val {
                        Value::DataType(jt) => Some(*jt.clone()),
                        Value::RuntimeTypeVar(tv) => Some(tv.projection()),
                        _ => None,
                    };
                    let is_match = value_type_object.is_some_and(|x| {
                        let value_type = crate::types::JuliaType::TypeOf(Box::new(x)).name();
                        let target_type = crate::types::JuliaType::TypeOf(inner.clone()).name();
                        self.check_subtype(value_type.as_ref(), target_type.as_ref())
                    });
                    self.stack.push(Value::Bool(is_match));
                    return Ok(Some(()));
                }

                let (struct_name_opt, resolved_val_type) = match &val {
                    // Issue #3909: route type-object values through the runtime
                    // type registry so the projected kind (DataType / UnionAll /
                    // TypeVar) matches typeof(). Without this, isa(Vector,
                    // UnionAll) would disagree with typeof(Vector) === UnionAll.
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        (None, registry.object(jt).runtime_type_projection())
                    }
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            if let Some(array_type) = si.array_wrapper_julia_type() {
                                (None, array_type)
                            } else {
                                (
                                    Some(si.struct_name.to_string()),
                                    crate::types::JuliaType::Struct(si.struct_name.to_string()),
                                )
                            }
                        } else {
                            (None, crate::types::JuliaType::Any)
                        }
                    }
                    Value::Struct(si) => {
                        if let Some(array_type) = si.array_wrapper_julia_type() {
                            (None, array_type)
                        } else {
                            (
                                Some(si.struct_name.to_string()),
                                crate::types::JuliaType::Struct(si.struct_name.to_string()),
                            )
                        }
                    }
                    _ if is_native_array_value(&val) => (None, self.get_value_julia_type(&val)),
                    Value::Memory(mem) => {
                        let mem_ref = mem.borrow();
                        self.stack
                            .push(Value::Bool(memory_isa_target(&mem_ref, &target_type_name)));
                        return Ok(Some(()));
                    }
                    Value::NamedTuple(nt) => {
                        // NamedTuple is a special type - use concrete type name
                        let fields: Vec<String> = nt
                            .names
                            .iter()
                            .zip(nt.values.iter())
                            .map(|(name, val)| format!("{}::{}", name, self.get_type_name(val)))
                            .collect();
                        let type_name = format!("@NamedTuple{{{}}}", fields.join(", "));
                        (
                            Some(type_name.clone()),
                            crate::types::JuliaType::Struct(type_name),
                        )
                    }
                    // Ref / RefValue: isa(Ref(x), Ref), isa(Ref(5), Ref{Int}) and
                    // isa(Ref(5), Base.RefValue{Int}) should all be true (Issue #2687, #5130).
                    // typeof() yields "Base.RefValue{T}"; reuse the dispatch matcher so the
                    // element type and the Ref/RefValue alias are handled consistently.
                    Value::Ref(_) => {
                        let target = crate::types::JuliaType::Struct(target_type_name.clone());
                        let is_match = self.value_matches_param(&val, &target);
                        self.stack.push(Value::Bool(is_match));
                        return Ok(Some(()));
                    }
                    // Core.SimpleVector: isa(<DataType>.parameters, Core.SimpleVector)
                    // should be true (Issue #4722). Route through the struct-name
                    // path so module-prefix normalization matches the target name.
                    Value::SimpleVector(_) => (
                        Some("Core.SimpleVector".to_string()),
                        crate::types::JuliaType::Struct("Core.SimpleVector".to_string()),
                    ),
                    // Tuple values: build the value's full type name through the
                    // VM context (`Tuple{Foo{Int64}}`, `Tuple{Dog}`, ...) so
                    // user-defined struct elements keep their parametric name and
                    // route the covariant comparison through the runtime
                    // `check_subtype`, which knows the user abstract hierarchy
                    // (`Dog <: Animal`) and the `Int`/`Int64` alias. The
                    // context-free `Value::runtime_type()` reports `Any` for
                    // `StructRef` tuple elements, breaking covariant Tuple `isa`
                    // such as `(Foo(1),) isa Tuple{Foo{Int}}` (Issue #5064).
                    Value::Tuple(_) => {
                        let tuple_type_name = self.get_type_name(&val);
                        let is_match = self.check_subtype(&tuple_type_name, &target_type_name);
                        self.stack.push(Value::Bool(is_match));
                        return Ok(Some(()));
                    }
                    // RNG values: route the concrete RNG type name through the
                    // struct-name path so `check_subtype` resolves
                    // `Xoshiro`/`StableRNG`/`TaskLocalRNG <: AbstractRNG`
                    // (Issues #7230, #7231).
                    Value::Rng(_) => {
                        let rng_type_name = self.get_type_name(&val);
                        (
                            Some(rng_type_name.clone()),
                            crate::types::JuliaType::Struct(rng_type_name),
                        )
                    }
                    // StaticArray flat representations carry their concrete Julia
                    // type; route through the struct-name path so abstract-type
                    // checks (e.g. `v isa AbstractArray`) resolve via
                    // `check_isa_with_abstract_resolved` (Issue #7964).
                    Value::StaticArray(sv) => {
                        let type_name = sv.julia_type_name().to_string();
                        (
                            Some(type_name.clone()),
                            crate::types::JuliaType::Struct(type_name),
                        )
                    }
                    Value::StaticArrayInline(sv) => {
                        let type_name = sv.julia_type_name_owned().to_string();
                        (
                            Some(type_name.clone()),
                            crate::types::JuliaType::Struct(type_name),
                        )
                    }
                    _ => (None, val.runtime_type()),
                };

                let normalized_target = normalize_type_for_isa(&target_type_name);

                // A bare (unqualified) type name that resolves to a *builtin
                // concrete* DataType (e.g. `Module`, `Int64`, `String`) binds to
                // that builtin in the current scope. A user struct value is `isa`
                // such a type only when its own concrete type matches — never via
                // short-name family matching against a same-short-name
                // module-local user type. Without this gate `Box() isa Module`
                // (bare `Base.Module`) wrongly matched a module-local
                // `TypeOwner7955.Module` abstract type, whose short name `Module`
                // is also recorded as `Box`'s supertype in the struct hierarchy
                // and the abstract-type index (Issue #7963). Qualified targets
                // (`TypeOwner7955.Module`) keep the dot, so they skip this gate
                // and still resolve through the family/abstract paths.
                let target_is_bare_builtin_concrete = !target_type_name.contains('.')
                    && crate::types::JuliaType::from_name(&target_type_name)
                        .is_some_and(|t| CoreType::from(&t).is_builtin_concrete_datatype());

                let is_match = if let Some(ref struct_name) = struct_name_opt {
                    let normalized_struct = normalize_type_for_isa(struct_name);
                    if !target_is_bare_builtin_concrete
                        && (normalized_struct == normalized_target
                            || self.check_subtype(&normalized_struct, &normalized_target))
                    {
                        true
                    } else {
                        let is_abstract_type = !target_is_bare_builtin_concrete
                            && self
                                .abstract_type_name_index
                                .contains_key(&target_type_name);

                        if is_abstract_type {
                            self.check_isa_with_abstract_resolved(
                                &struct_name_opt,
                                &target_type_name,
                            )
                        } else if self.struct_hierarchy.contains_name(&target_type_name) {
                            // A registered struct/abstract target spelled without
                            // parameters: `check_subtype` (consulted above with the
                            // hierarchy + the typevar-vs-nominal classification, see
                            // `classify_subtype_operand`) is authoritative for a
                            // declared struct value, so its `false` is final here.
                            // The permissive enum-only `type_values_subtype` residue
                            // wrongly accepted `P{Int}() isa Q` for an unrelated
                            // registered parametric struct `Q` because `from_name`
                            // infers a free type variable for short uppercase[+digit]
                            // names (#8092).
                            false
                        } else {
                            let target_type = crate::types::JuliaType::from_name(&target_type_name)
                                .unwrap_or_else(|| {
                                    crate::types::JuliaType::Struct(target_type_name.clone())
                                });
                            type_values_subtype(&resolved_val_type, &target_type)
                        }
                    }
                } else {
                    let target_type = crate::types::JuliaType::from_name(&target_type_name)
                        .unwrap_or_else(|| {
                            crate::types::JuliaType::Struct(target_type_name.clone())
                        });
                    type_values_subtype(&resolved_val_type, &target_type)
                };
                self.stack.push(Value::Bool(is_match));
            }

            BuiltinId::Subtype => {
                // Upstream `jl_f_issubtype` validates `JL_NARGS(<:, 2, 2)`
                // before touching its arguments (Issue #5493). Without this the
                // immediate call path underflowed on `(<:)(A)` and silently
                // ignored extras on `(<:)(A, B, extra)`.
                check_builtin_arity("<:", argc, 2)?;
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                let left_type = subtype_operand_name(&left);
                let right_type = subtype_operand_name(&right);
                let is_subtype = self.check_subtype(&left_type, &right_type);
                self.stack.push(Value::Bool(is_subtype));
            }
            BuiltinId::SupertypeOp => {
                // `A >: B` is the supertype operator, equivalent to `B <: A`
                // (julia/base/operators.jl). Reachable when `>:` is used as a
                // first-class function value, e.g. `(>:)(Number, Int)` or
                // `f = (>:); f(Number, Int)` (Issue #5115). The binary infix
                // `A >: B` is instead lowered to `BinaryOp::Subtype` with the
                // operands swapped (lowering/expr/binary.rs).
                //
                // Unlike `<:`/`isa`, `>:` is an ordinary 2-arg Julia function
                // (`>:(a, b) = (b <: a)`), not a builtin, so upstream raises a
                // `MethodError` — not an `ArgumentError` — on the wrong arity
                // (Issue #5493). Without this guard the immediate call path
                // underflowed on `(>:)(A)` and silently ignored extras on
                // `(>:)(A, B, extra)`.
                if argc != 2 {
                    let args = self.peek_stack_top(argc);
                    let arg_type_names = args
                        .iter()
                        .map(|arg| self.get_type_name(arg))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(VmError::MethodError(format!(
                        "no method matching >:({})",
                        arg_type_names
                    )));
                }
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                // A >: B  ⟺  B <: A
                let left_type = subtype_operand_name(&left);
                let right_type = subtype_operand_name(&right);
                let is_supertype = self.check_subtype(&right_type, &left_type);
                self.stack.push(Value::Bool(is_supertype));
            }
            BuiltinId::_Typeintersect => {
                // _typeintersect(a, b) - semantic type intersection for Pure Julia
                // typeintersect(). User-defined hierarchy still uses the VM
                // registry via check_subtype; structured built-ins use CoreType.
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;
                if let Some(result) = core_datatype_typeintersect_subtype_result(&left, &right) {
                    self.stack.push(Value::DataType(Box::new(result)));
                    return Ok(Some(()));
                }
                let Some(left_type) = reflection_type_name(&left) else {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Bottom)));
                    return Ok(Some(()));
                };
                let Some(right_type) = reflection_type_name(&right) else {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Bottom)));
                    return Ok(Some(()));
                };

                let result_name = if self.check_subtype(&left_type, &right_type) {
                    left_type
                } else if self.check_subtype(&right_type, &left_type) {
                    right_type
                } else if left_type.starts_with("Type{") || right_type.starts_with("Type{") {
                    // Type{T} intersection is singleton-type sensitive; keep
                    // unsupported non-subtype cases conservative for now.
                    "Union{}".to_string()
                } else if let (Value::DataType(left_ty), Value::DataType(right_ty)) =
                    (&left, &right)
                {
                    CoreType::from(left_ty.as_ref())
                        .type_intersect(&CoreType::from(right_ty.as_ref()))
                        .to_julia_name()
                } else {
                    CoreType::from_julia_name(&left_type)
                        .type_intersect(&CoreType::from_julia_name(&right_type))
                        .to_julia_name()
                };
                self.stack
                    .push(Value::DataType(Box::new(reflection_julia_type(
                        &result_name,
                    ))));
            }
            BuiltinId::Sizeof => {
                // sizeof(x) - return size of value in bytes
                // For primitive types, use the shared CoreType layout table.
                // For composite types (structs, arrays), return approximate size.
                let val = self.stack.pop_value()?;
                let size: i64 =
                    match &val {
                        // Scalar bits-type values must report the logical TYPE
                        // size (== sizeof(typeof(x))), NOT the boxed `Value`
                        // representation size. Each narrow variant maps to its
                        // own type so e.g. Int32(4) is 4, not 8 (Issue #6766).
                        Value::Bool(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Bool")
                            .unwrap_or(8) as i64,
                        Value::I8(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int8")
                            .unwrap_or(1) as i64,
                        Value::I16(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int16")
                            .unwrap_or(2) as i64,
                        Value::I32(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int32")
                            .unwrap_or(4) as i64,
                        Value::I64(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int64")
                            .unwrap_or(8) as i64,
                        Value::I128(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Int128")
                            .unwrap_or(16) as i64,
                        Value::U8(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt8")
                            .unwrap_or(1) as i64,
                        Value::U16(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt16")
                            .unwrap_or(2) as i64,
                        Value::U32(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt32")
                            .unwrap_or(4) as i64,
                        Value::U64(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt64")
                            .unwrap_or(8) as i64,
                        Value::U128(_) => CoreType::builtin_sizeof_bytes_for_julia_name("UInt128")
                            .unwrap_or(16) as i64,
                        Value::F16(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Float16")
                            .unwrap_or(2) as i64,
                        Value::F32(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Float32")
                            .unwrap_or(4) as i64,
                        Value::F64(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Float64")
                            .unwrap_or(8) as i64,
                        Value::Char(_) => CoreType::builtin_sizeof_bytes_for_julia_name("Char")
                            .unwrap_or(8) as i64,
                        Value::Str(s) => s.len() as i64, // Number of bytes in UTF-8 encoding
                        _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                            Some(arr) => {
                                let arr_ref = arr.borrow();
                                array_value_size_bytes(&arr_ref.element_type(), &arr_ref.shape)
                            }
                            None => 8,
                        },
                        _ if self.array_wrapper_memory_and_shape(&val).is_some() => {
                            match self.array_wrapper_memory_and_shape(&val) {
                                Some((mem, shape)) => {
                                    let mem_ref = mem.borrow();
                                    array_value_size_bytes(mem_ref.element_type(), &shape)
                                }
                                None => 8,
                            }
                        }
                        Value::Memory(mem) => {
                            let mem_ref = mem.borrow();
                            memory_value_size_bytes(&mem_ref)
                        }
                        Value::Tuple(t) => {
                            // Sum of sizes of all elements
                            t.elements.len() as i64 * 8 // Approximate: 8 bytes per element pointer
                        }
                        Value::Nothing => CoreType::builtin_sizeof_bytes_for_julia_name("Nothing")
                            .unwrap_or(8) as i64,
                        Value::Missing => CoreType::builtin_sizeof_bytes_for_julia_name("Missing")
                            .unwrap_or(8) as i64,
                        Value::DataType(jt) => {
                            let registry = RuntimeTypeRegistry::new(
                                self.compile_context.as_ref(),
                                &self.abstract_types,
                            );
                            registry.object(jt).size_bytes().unwrap_or(8) as i64
                        }
                        Value::StructRef(_) | Value::Struct(_) => 8, // Pointer/reference size
                        _ => 8, // Default to pointer size for other types
                    };
                self.stack.push(Value::I64(size));
            }

            // BuiltinId::Isbits removed - pure Julia (Issue #6738)
            BuiltinId::Isbitstype => {
                // isbitstype(T) - check if T is a bits type
                let type_val = self.stack.pop_value()?;
                let type_object = match &type_val {
                    Value::DataType(jt) => jt.clone(),
                    Value::Str(s) => Box::new(reflection_julia_type(s)),
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };

                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let is_bits = registry.object(&type_object).is_bits_type();
                self.stack.push(Value::Bool(is_bits));
            }

            BuiltinId::_Supertype => {
                // _supertype(T) - get parent type for Pure Julia supertype().
                let type_val = self.stack.pop_value()?;
                let Some(type_name) = reflection_type_name(&type_val) else {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Any)));
                    return Ok(Some(()));
                };

                let supertype = self.reflection_supertype_name(&type_name);
                self.stack
                    .push(Value::DataType(Box::new(reflection_julia_type(&supertype))));
            }

            BuiltinId::_Typename => {
                // _typename(T) - canonical TypeName symbol for Pure Julia
                // nameof(::Type) / Base.typename (Issue #5106). Resolves the
                // base name through the type registry, collapsing the `Array`
                // display aliases (`Vector`/`Matrix`) onto the shared
                // `TypeName` so `nameof(Vector{Int}) === :Array`.
                let type_val = self.stack.pop_value()?;
                let symbol = match &type_val {
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        registry.object(jt).typename_symbol()
                    }
                    Value::RuntimeTypeVar(tv) => tv.name.clone(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_typename expects a type, got {:?}",
                            other
                        )));
                    }
                };
                self.stack.push(Value::Symbol(SymbolValue::new(&symbol)));
            }

            BuiltinId::_FunctionName => {
                // _function_name(f) - direct function-name symbol for Pure
                // Julia nameof(::Function), avoiding string slicing of
                // `string(f)` (Issue #5580).
                let val = self.stack.pop_value()?;
                let name = match &val {
                    Value::Function(fv) => fv.name.clone(),
                    Value::Closure(cv) => cv.name.clone(),
                    Value::Str(s) => s.clone(),
                    Value::Symbol(sym) => sym.as_str().to_string(),
                    Value::DataType(jt) => jt.name().to_string(),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_function_name expects a function, got {:?}",
                            other
                        )));
                    }
                };
                self.stack.push(Value::Symbol(SymbolValue::new(&name)));
            }

            BuiltinId::Subtypes => {
                // subtypes(T) - vector of direct subtypes
                let type_val = self.stack.pop_value()?;
                let type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    _ => {
                        // For non-type values, return empty array
                        self.push_array_value_as_wrapper(super::value::ArrayValue::any_vector(
                            Vec::new(),
                        ))?;
                        return Ok(Some(()));
                    }
                };

                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let subtypes: Vec<Value> = registry
                    .direct_subtypes(&type_name)
                    .into_iter()
                    .map(|t| Value::DataType(Box::new(t)))
                    .collect();

                self.push_array_value_as_wrapper(super::value::ArrayValue::any_vector(subtypes))?;
            }

            // BuiltinId::Typeintersect/Typejoin removed - now Pure Julia (base/reflection.jl)
            // BuiltinId::Fieldcount removed - now Pure Julia (base/reflection.jl)
            // BuiltinId::Hasfield removed - pure Julia (Issue #6738)

            // BuiltinId::Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype
            // removed - now Pure Julia (base/reflection.jl) with internal intrinsics
            BuiltinId::_Isabstracttype => {
                // _isabstracttype(T) - internal intrinsic: check if T is an abstract type
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_abstract = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_abstract_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_abstract));
            }

            BuiltinId::_Isconcretetype => {
                // _isconcretetype(T) - internal intrinsic: check if T is a concrete type
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_concrete = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_concrete_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_concrete));
            }

            BuiltinId::_Isprimitivetype => {
                // _isprimitivetype(T) - internal intrinsic: check if T is a primitive
                // type (fixed bit-width, no fields). Mirrors upstream Julia's
                // `isprimitivetype`, which `unwrap_unionall`s then flag-checks the
                // DataType (`(t.flags & 0x0080) == 0x0080`). Routed through the
                // runtime type registry so all five type predicates share one
                // classification path (Issue #5102). CoreType owns the built-in
                // primitive DataType set; in particular `String` is not primitive.
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_primitive = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_primitive_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_primitive));
            }

            BuiltinId::_Isstructtype => {
                // _isstructtype(T) - internal intrinsic: check Julia's DataType
                // struct flag. CoreType owns built-in struct facts; VM registry
                // supplies user-defined struct definitions.
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_struct_type = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_struct_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_struct_type));
            }

            BuiltinId::_Ismutabletype => {
                // _ismutabletype(T) - internal intrinsic: check if T is a mutable type
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let is_mutable_type = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_mutable_type(),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_mutable_type));
            }

            // BuiltinId::Ismutable removed - pure Julia (Issue #6738)

            // BuiltinId::Ismutabletype removed - now Pure Julia (base/reflection.jl)
            // BuiltinId::NameOf removed - now Pure Julia (base/reflection.jl)
            BuiltinId::Objectid => {
                // objectid(x) - unique object identifier
                // In Julia, this returns a UInt that uniquely identifies the object.
                // We use a hash-based approach for simplicity.
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let val = self.stack.pop_value()?;
                let mut hasher = DefaultHasher::new();

                // Hash based on value type and content
                match &val {
                    Value::I64(n) => {
                        "I64".hash(&mut hasher);
                        n.hash(&mut hasher);
                    }
                    Value::F64(f) => {
                        "F64".hash(&mut hasher);
                        f.to_bits().hash(&mut hasher);
                    }
                    Value::Bool(b) => {
                        "Bool".hash(&mut hasher);
                        b.hash(&mut hasher);
                    }
                    Value::Str(s) => {
                        "Str".hash(&mut hasher);
                        s.hash(&mut hasher);
                    }
                    Value::Char(c) => {
                        "Char".hash(&mut hasher);
                        c.hash(&mut hasher);
                    }
                    Value::Nothing => {
                        "Nothing".hash(&mut hasher);
                    }
                    Value::Missing => {
                        "Missing".hash(&mut hasher);
                    }
                    _ if is_native_array_value(&val) => {
                        "Array".hash(&mut hasher);
                        // Use pointer-like identity for arrays (mutable objects)
                        if let Some(arr) = native_array_value_ref(&val) {
                            (arr.as_ptr() as usize).hash(&mut hasher);
                        }
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        "Array".hash(&mut hasher);
                        // Use pointer-like identity for memory-backed arrays (mutable objects)
                        (mem.as_ptr() as usize).hash(&mut hasher);
                    }
                    Value::Struct(s) => {
                        "Struct".hash(&mut hasher);
                        s.struct_name.hash(&mut hasher);
                        s.type_id.hash(&mut hasher);
                    }
                    Value::Tuple(t) => {
                        "Tuple".hash(&mut hasher);
                        t.len().hash(&mut hasher);
                    }
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new_with_struct_defs(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                            &self.struct_defs,
                        );
                        let object = registry.object(jt);
                        object.kind().hash(&mut hasher);
                        object.identity().stable_hash().hash(&mut hasher);
                    }
                    Value::RuntimeTypeVar(tv) => {
                        "RuntimeTypeVar".hash(&mut hasher);
                        tv.id.hash(&mut hasher);
                    }
                    _ => {
                        // For other types, use a simple discriminant
                        std::mem::discriminant(&val).hash(&mut hasher);
                    }
                }

                let id = hasher.finish();
                self.stack.push(Value::U64(id));
            }

            BuiltinId::Isunordered => {
                // isunordered(x) - check if x is unordered (NaN, Missing)
                // Returns true for values where comparisons are undefined
                let val = self.stack.pop_value()?;
                let is_unordered = match &val {
                    Value::F64(f) => f.is_nan(),
                    Value::Missing => true,
                    // Complex with NaN components
                    Value::Struct(s) if &*s.struct_name == "Complex" => s
                        .values
                        .iter()
                        .any(|v| matches!(v, Value::F64(f) if f.is_nan())),
                    _ => false,
                };
                self.stack.push(Value::Bool(is_unordered));
            }

            BuiltinId::In => {
                // in(x, collection) - check if element is in collection
                let collection = self.stack.pop_value()?;
                let element = self.stack.pop_value()?;

                let heap = self.struct_heap.as_slice();

                let complex_parts = |value: &Value| match value {
                    Value::StructRef(idx) => heap.get(*idx).and_then(|s| s.as_complex_parts()),
                    _ => value.as_complex_parts(),
                };

                // `as_complex_parts` also reports bare reals as `(v as f64, 0.0)`,
                // so the complex branch below must only fire when at least one
                // operand is GENUINELY complex; otherwise two reals would compare
                // via the lossy `v as f64` instead of the value-based numeric arms
                // (Issue #8187).
                let is_complex = |value: &Value| match value {
                    Value::StructRef(idx) => heap
                        .get(*idx)
                        .is_some_and(|s| s.as_complex_parts().is_some()),
                    Value::Struct(s) => s.as_complex_parts().is_some(),
                    _ => false,
                };

                #[derive(Clone, Copy, Debug, Eq, PartialEq)]
                enum NumericInteger {
                    NonNegative(u128),
                    Negative(i128),
                }

                let signed_integer_value = |value: i128| {
                    if value >= 0 {
                        NumericInteger::NonNegative(value.cast_unsigned())
                    } else {
                        NumericInteger::Negative(value)
                    }
                };

                let integer_value = |value: &Value| match value {
                    Value::I8(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I16(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I32(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I64(v) => Some(signed_integer_value(i128::from(*v))),
                    Value::I128(v) => Some(signed_integer_value(*v)),
                    Value::U8(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U16(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U32(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U64(v) => Some(NumericInteger::NonNegative(u128::from(*v))),
                    Value::U128(v) => Some(NumericInteger::NonNegative(*v)),
                    _ => None,
                };

                // Helper function to compare two values for equality (like Julia's ==)
                let values_equal = |a: &Value, b: &Value| -> bool {
                    if is_complex(a) || is_complex(b) {
                        if let (Some((a_re, a_im)), Some((b_re, b_im))) =
                            (complex_parts(a), complex_parts(b))
                        {
                            return a_re == b_re && a_im == b_im;
                        }
                    }

                    if let (Some(a_int), Some(b_int)) = (integer_value(a), integer_value(b)) {
                        return a_int == b_int;
                    }

                    match (a, b) {
                        (Value::I64(x), Value::I64(y)) => x == y,
                        (Value::F64(x), Value::F64(y)) => x == y,
                        // Mixed integer/float `==` (membership uses `==`, not
                        // `isequal`, so no sign-of-zero rule): value-based, no
                        // rounding of the integer (Issue #8187, all widths #8199).
                        (x, y)
                            if crate::vm::numeric_identity::mixed_int_float_values_equal(x, y)
                                .is_some() =>
                        {
                            crate::vm::numeric_identity::mixed_int_float_values_equal(x, y)
                                .unwrap_or(false)
                        }
                        (Value::Bool(x), Value::Bool(y)) => x == y,
                        (Value::Str(x), Value::Str(y)) => x == y,
                        (Value::Char(x), Value::Char(y)) => x == y,
                        (Value::Symbol(x), Value::Symbol(y)) => x == y,
                        (Value::Nothing, Value::Nothing) => true,
                        (Value::Missing, Value::Missing) => true,
                        // Ranges compare element-wise (`(1:3) in [1:3, 4:6]`,
                        // Issue #5725). Two ranges are equal iff they produce the
                        // same elements — including all empty ranges being equal.
                        (Value::Range(x), Value::Range(y)) => x.to_vec() == y.to_vec(),
                        // Type objects compare by canonical structural identity
                        // so `Int in [Float64, Int]` works (Issue #5108). Route
                        // both operands through the same DictKey canonicalization
                        // used for type-keyed Dict/Set membership.
                        (Value::DataType(_) | Value::RuntimeTypeVar(_), _)
                        | (_, Value::DataType(_) | Value::RuntimeTypeVar(_)) => {
                            match (DictKey::from_value(a), DictKey::from_value(b)) {
                                (Ok(ka), Ok(kb)) => ka == kb,
                                _ => false,
                            }
                        }
                        // Tuple / named-tuple / struct elements: the scalar arms
                        // above cannot compare them, so route through the shared
                        // `==` helper, which resolves heap struct refs and folds
                        // `==` over elements (Issue #6691, builds on #6685). This
                        // makes `(1, 2) in [(1, 2)]` and `(OneTo(3),) in [...]`
                        // behave like upstream.
                        _ => super::builtins_equality::values_equal_for_membership(a, b, heap),
                    }
                };

                let found = match &collection {
                    _ if is_native_array_value(&collection) => {
                        match native_array_value_ref(&collection) {
                            Some(arr) => {
                                let arr_ref = arr.borrow();
                                let len = arr_ref.element_count();
                                let mut found = false;
                                for i in 0..len {
                                    let v = arr_ref.get_linear(i)?;
                                    if values_equal(&element, &v) {
                                        found = true;
                                        break;
                                    }
                                }
                                found
                            }
                            None => false,
                        }
                    }
                    _ if self.array_wrapper_memory_and_shape(&collection).is_some() => {
                        match self.array_wrapper_memory_and_shape(&collection) {
                            Some((mem, shape)) => {
                                let mem_ref = mem.borrow();
                                let len = shape.iter().product::<usize>().min(mem_ref.len());
                                let mut found = false;
                                for i in 0..len {
                                    if let Some(v) = mem_ref.data.get_value(i) {
                                        if values_equal(&element, &v) {
                                            found = true;
                                            break;
                                        }
                                    }
                                }
                                found
                            }
                            None => false,
                        }
                    }
                    Value::Memory(mem) => {
                        let mem_ref = mem.borrow();
                        let len = mem_ref.len();
                        let mut found = false;
                        for i in 0..len {
                            if let Some(v) = mem_ref.data.get_value(i) {
                                if values_equal(&element, &v) {
                                    found = true;
                                    break;
                                }
                            }
                        }
                        found
                    }
                    Value::Tuple(t) => t.elements.iter().any(|v| values_equal(&element, v)),
                    Value::Str(s) => {
                        // For strings, check if element is a Char contained in the string
                        match &element {
                            Value::Char(c) => s.contains(*c),
                            Value::Str(substr) => s.contains(substr.as_str()),
                            _ => false,
                        }
                    }
                    // `x in start:step:stop` — membership in a range (Issue #5728).
                    Value::Range(r) => r.contains_value(&element),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "in requires Array, Tuple, String, Dict, or Set, got {:?}",
                            collection
                        )));
                    }
                };
                self.stack.push(Value::Bool(found));
            }

            BuiltinId::NonMissingType => {
                // nonmissingtype(T) - remove Missing from Union type
                // If T is Union{X, Missing}, return X
                // If T is Missing, return Union{} (Bottom)
                // If T doesn't contain Missing, return T unchanged
                let type_val = self.stack.pop_value()?;

                let result_type = match &type_val {
                    Value::DataType(jt) => {
                        match jt.as_ref() {
                            // If the type is Missing itself, return Bottom (Union{})
                            crate::types::JuliaType::Missing => crate::types::JuliaType::Bottom,

                            // If it's a Union type, filter out Missing
                            crate::types::JuliaType::Union(types) => {
                                let filtered: Vec<crate::types::JuliaType> = types
                                    .iter()
                                    .filter(|t| !matches!(t, crate::types::JuliaType::Missing))
                                    .cloned()
                                    .collect();

                                match filtered.len() {
                                    0 => crate::types::JuliaType::Bottom, // All types were Missing
                                    1 => {
                                        // Safety: len()==1 guarantees next() is Some
                                        match filtered.into_iter().next() {
                                            Some(t) => t,
                                            None => crate::types::JuliaType::Bottom,
                                        }
                                    }
                                    _ => crate::types::JuliaType::Union(filtered), // Multiple types remaining
                                }
                            }

                            // For any other JuliaType (Int64, Float64, etc.), return unchanged
                            // nonmissingtype only filters Missing from Union types
                            other => other.clone(),
                        }
                    }
                    Value::Str(type_name) => {
                        // Parse the type name
                        if type_name == "Missing" {
                            crate::types::JuliaType::Bottom
                        } else if type_name.starts_with("Union{") && type_name.ends_with('}') {
                            // Parse Union{T1, T2, ...} and filter out Missing
                            let inner = &type_name[6..type_name.len() - 1];
                            let types: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                            let filtered: Vec<crate::types::JuliaType> = types
                                .iter()
                                .filter(|&t| *t != "Missing")
                                .filter_map(|t| crate::types::JuliaType::from_name(t))
                                .collect();

                            match filtered.len() {
                                0 => crate::types::JuliaType::Bottom,
                                1 => {
                                    // Safety: len()==1 guarantees next() is Some
                                    match filtered.into_iter().next() {
                                        Some(t) => t,
                                        None => crate::types::JuliaType::Bottom,
                                    }
                                }
                                _ => crate::types::JuliaType::Union(filtered),
                            }
                        } else {
                            // Not a Union, return as-is
                            crate::types::JuliaType::from_name(type_name).unwrap_or_else(|| {
                                crate::types::JuliaType::Struct(type_name.clone())
                            })
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "nonmissingtype requires a type, got {:?}",
                            type_val.value_type()
                        )));
                    }
                };

                self.stack.push(Value::DataType(Box::new(result_type)));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JuliaType;

    #[test]
    fn core_datatype_typeintersect_subtype_result_keeps_subtype_side() {
        assert_eq!(
            core_datatype_typeintersect_subtype_result(
                &Value::DataType(Box::new(JuliaType::Int64)),
                &Value::DataType(Box::new(JuliaType::Real)),
            ),
            Some(JuliaType::Int64)
        );
        assert_eq!(
            core_datatype_typeintersect_subtype_result(
                &Value::DataType(Box::new(JuliaType::Real)),
                &Value::DataType(Box::new(JuliaType::Int64)),
            ),
            Some(JuliaType::Int64)
        );
    }

    #[test]
    fn core_datatype_typeintersect_subtype_result_defers_non_subtype_cases() {
        assert_eq!(
            core_datatype_typeintersect_subtype_result(
                &Value::DataType(Box::new(JuliaType::String)),
                &Value::DataType(Box::new(JuliaType::Number)),
            ),
            None
        );
        assert_eq!(
            core_datatype_typeintersect_subtype_result(
                &Value::DataType(Box::new(JuliaType::Struct("Dog".to_string()))),
                &Value::DataType(Box::new(JuliaType::Any)),
            ),
            None
        );
    }

    #[test]
    fn julia_type_references_typevar_scans_unionall_bounds_issue_7924() {
        let ty = JuliaType::UnionAll {
            lower_bound: None,
            var: "S".to_string(),
            bound: Some(Box::new("T".to_string())),
            body: Box::new(JuliaType::TupleOf(vec![JuliaType::TypeVar(
                "S".to_string(),
                None,
            )])),
        };

        assert!(julia_type_references_typevar(&ty, "T"));
        assert!(julia_type_references_typevar(
            &JuliaType::TypeVar("S".to_string(), Some("T".to_string())),
            "T"
        ));
    }
}
