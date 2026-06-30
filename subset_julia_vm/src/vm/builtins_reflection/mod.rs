//! Reflection builtin functions for the VM.
//!
//! Internal introspection operations: _fieldnames, _fieldtypes, deepcopy, methods, hasmethod, which.
//! These are internal VM builtins that are wrapped by Pure Julia functions
//! in subset_julia_vm/src/julia/base/reflection.jl.

// SAFETY: i64/i32→usize casts for field index access are guarded by `if index == 0`
// checks that reject non-positive values before the cast.
#![allow(clippy::cast_sign_loss)]

pub(super) mod primitives;

use crate::builtins::BuiltinId;
use crate::compile::abstract_interp::{BaseCalleeExceptionClassifier, ExceptionType, TypeEnv};
use crate::compile::bridge::{lattice_to_parametric_julia_type, lattice_to_value_type};
use crate::compile::build_shared_inference_engine;
use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
use crate::compile::{MethodSig, MethodTable};
use crate::inference_core::{CorePrimitive, CoreType};
use crate::rng::RngLike;
use crate::types::{DispatchError, JuliaType, StructHierarchy};
use std::collections::{HashMap, HashSet};

use super::error::VmError;
use super::instr::Instr;
use super::stack_ops::StackOps;
use super::type_objects::{ReflectionParameter, RuntimeTypeRegistry};
use super::type_utils::type_values_subtype;
use super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, ArrayData, ArrayElementType,
    ArrayValue, ExprValue, FunctionValue, ModuleValue, RuntimeTypeNameValue, StructInstance,
    SymbolValue, TupleValue, Value, ValueType,
};
use super::{FunctionInfo, Vm};
use primitives::{
    extract_func_name, extract_kw_names_from_value, extract_signature_tuple_from_value,
    extract_types_from_value, value_type_to_julia_type,
};

fn expr_value_field_by_index(expr: &ExprValue, field_idx: usize) -> Option<Value> {
    match field_idx {
        0 => Some(Value::Symbol(expr.head.clone())),
        1 => Some(expr.get_args()),
        _ => None,
    }
}

fn expr_value_field_by_name(expr: &ExprValue, field_name: &str) -> Option<Value> {
    match field_name {
        "head" => Some(Value::Symbol(expr.head.clone())),
        "args" => Some(expr.get_args()),
        _ => None,
    }
}

impl<R: RngLike> Vm<R> {
    /// Execute reflection builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a reflection builtin.
    pub(super) fn execute_builtin_reflection(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::ComposeExceptionType => {
                // _compose_exception_type(f, types) — interprocedural exception
                // type composed from the function body's callees (Issue #5600).
                // Returns the composed exception DataType, or `nothing` when no
                // known exception is proven (the caller keeps its Union{} default).
                let types_val = self.stack.pop_value()?;
                let arg_types = extract_types_from_value(&types_val, &self.struct_heap)?;
                let func_val = self.stack.pop_value()?;
                let closure_captures = match &func_val {
                    Value::Closure(cv) => Some(cv.captures.as_slice()),
                    _ => None,
                };
                let func_name = extract_func_name(&func_val)?;
                let composed = {
                    let mut acc: Option<JuliaType> = None;
                    if let Some(infos) = self.find_matching_methods(&func_name, Some(&arg_types)) {
                        for info in &infos {
                            if let Some(jt) = self.compose_function_exception_type(
                                info,
                                &arg_types,
                                closure_captures,
                            ) {
                                acc = Some(match acc {
                                    None => jt,
                                    Some(prev) => merge_exception_julia_types(prev, jt),
                                });
                            }
                        }
                    }
                    acc
                };
                match composed {
                    Some(jt) => self.stack.push(Value::DataType(Box::new(jt))),
                    None => self.stack.push(Value::Nothing),
                }
            }
            BuiltinId::_ReturnTypesByFtype => {
                // _return_types_by_ftype(f, types) — return-type reflection
                // through call dispatch (Issue #5603). A concrete ambiguous
                // call has no selected method, so it returns an empty vector;
                // Base.infer_return_type maps that to Union{}.
                let types_val = self.stack.pop_value()?;
                let arg_types = extract_types_from_value(&types_val, &self.struct_heap)?;
                let func_val = self.stack.pop_value()?;
                let closure_captures = match &func_val {
                    Value::Closure(cv) => Some(cv.captures.as_slice()),
                    _ => None,
                };
                let func_name = extract_func_name(&func_val)?;
                let return_types =
                    self.return_types_by_ftype(&func_name, &arg_types, closure_captures)?;
                self.push_array_value_as_wrapper(ArrayValue::any_vector(return_types))?;
            }
            BuiltinId::_Fieldnames => {
                // fieldnames(T) - tuple of field names as symbols/strings
                let val = self.stack.pop_value()?;
                let names: Vec<Value> = match &val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            if let Some(def) = self
                                .struct_def_name_index
                                .get(&*si.struct_name)
                                .and_then(|&idx| self.struct_defs.get(idx))
                            {
                                def.fields
                                    .iter()
                                    .map(|(name, _)| Value::Str(name.clone()))
                                    .collect()
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    }
                    Value::Struct(si) => {
                        if let Some(def) = self
                            .struct_def_name_index
                            .get(&*si.struct_name)
                            .and_then(|&idx| self.struct_defs.get(idx))
                        {
                            def.fields
                                .iter()
                                .map(|(name, _)| Value::Str(name.clone()))
                                .collect()
                        } else {
                            vec![]
                        }
                    }
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                        );
                        registry
                            .object(jt)
                            .field_names()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|name| Value::Symbol(SymbolValue::new(&name)))
                            .collect()
                    }
                    Value::NamedTuple(nt) => {
                        nt.names.iter().map(|n| Value::Str(n.clone())).collect()
                    }
                    // Handle type name passed as string (e.g., fieldnames(Person))
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        let registry = RuntimeTypeRegistry::new(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                        );
                        registry
                            .object(&jt)
                            .field_names()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|name| Value::Symbol(SymbolValue::new(&name)))
                            .collect()
                    }
                    Value::LineNumberNode(_) => {
                        let jt = JuliaType::LineNumberNode;
                        let registry = RuntimeTypeRegistry::new(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                        );
                        registry
                            .object(&jt)
                            .builtin_field_names()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|name| Value::Symbol(SymbolValue::new(&name)))
                            .collect()
                    }
                    _ => vec![],
                };
                self.stack
                    .push(Value::Tuple(TupleValue { elements: names }));
            }

            BuiltinId::_Fieldtypes => {
                // _fieldtypes(T) - tuple of field types as DataType values
                let val = self.stack.pop_value()?;
                let types: Vec<Value> = match &val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            if let Some(def) = self
                                .struct_def_name_index
                                .get(&*si.struct_name)
                                .and_then(|&idx| self.struct_defs.get(idx))
                            {
                                def.fields
                                    .iter()
                                    .map(|(_, field_type)| {
                                        Value::DataType(Box::new(value_type_to_julia_type(
                                            field_type,
                                            &self.struct_defs,
                                        )))
                                    })
                                    .collect()
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    }
                    Value::Struct(si) => {
                        if let Some(def) = self
                            .struct_def_name_index
                            .get(&*si.struct_name)
                            .and_then(|&idx| self.struct_defs.get(idx))
                        {
                            def.fields
                                .iter()
                                .map(|(_, field_type)| {
                                    Value::DataType(Box::new(value_type_to_julia_type(
                                        field_type,
                                        &self.struct_defs,
                                    )))
                                })
                                .collect()
                        } else {
                            vec![]
                        }
                    }
                    Value::DataType(jt) => {
                        let registry = RuntimeTypeRegistry::new(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                        );
                        registry
                            .object(jt)
                            .field_types()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|t| Value::DataType(Box::new(t)))
                            .collect()
                    }
                    // Handle type name passed as string (e.g., fieldtypes(Person))
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        let registry = RuntimeTypeRegistry::new(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                        );
                        registry
                            .object(&jt)
                            .field_types()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|t| Value::DataType(Box::new(t)))
                            .collect()
                    }
                    Value::LineNumberNode(_) => {
                        let jt = JuliaType::LineNumberNode;
                        let registry = RuntimeTypeRegistry::new(
                            self.compile_context.as_ref(),
                            &self.abstract_types,
                        );
                        registry
                            .object(&jt)
                            .builtin_field_types()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|t| Value::DataType(Box::new(t)))
                            .collect()
                    }
                    _ => vec![],
                };
                self.stack
                    .push(Value::Tuple(TupleValue { elements: types }));
            }

            BuiltinId::_Fieldoffset => {
                // _fieldoffset(T, i) - field byte offset by 1-based index.
                let index_val = self.stack.pop_value()?;
                let type_val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);

                let index = match &index_val {
                    Value::I64(i) if *i > 0 => usize::try_from(*i).map_err(|_| {
                        VmError::TypeError(format!("_fieldoffset index out of range: {}", i))
                    })?,
                    Value::I32(i) if *i > 0 => usize::try_from(*i).map_err(|_| {
                        VmError::TypeError(format!("_fieldoffset index out of range: {}", i))
                    })?,
                    Value::U64(i) if *i > 0 => usize::try_from(*i).map_err(|_| {
                        VmError::TypeError(format!("_fieldoffset index out of range: {}", i))
                    })?,
                    Value::I64(_) | Value::I32(_) | Value::U64(_) => {
                        return Err(VmError::FieldIndexOutOfBounds {
                            index: 0,
                            field_count: 0,
                        });
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_fieldoffset index must be an integer, got {:?}",
                            index_val
                        )));
                    }
                };

                let offset = match &type_val {
                    Value::DataType(jt) => registry.object(jt).field_offset(index),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).field_offset(index)
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_fieldoffset requires a type object, got {:?}",
                            type_val
                        )));
                    }
                };

                let Some(offset) = offset else {
                    return Err(VmError::FieldIndexOutOfBounds {
                        index,
                        field_count: match &type_val {
                            Value::DataType(jt) => registry.object(jt).layout().nfields,
                            Value::Str(type_name) => {
                                let jt = JuliaType::from_name_or_struct(type_name);
                                registry.object(&jt).layout().nfields
                            }
                            _ => 0,
                        },
                    });
                };
                self.stack.push(Value::U64(offset as u64));
            }

            BuiltinId::_DatatypeAlignment => {
                // _datatype_alignment(T) - byte alignment of a type's inline
                // layout, backing Pure Julia `Base.datatype_alignment` /
                // `Base.aligned_sizeof` (Issue #5107). Mirrors upstream
                // `datatype_alignment` in `julia/base/runtime_internals.jl`.
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let align = match &type_val {
                    Value::DataType(jt) => registry.object(jt).alignment_bytes(),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).alignment_bytes()
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_datatype_alignment requires a type object, got {:?}",
                            type_val
                        )));
                    }
                };
                // Unknown layouts (parametric instantiations whose concrete def
                // is unavailable) fall back to the pointer width, as upstream's
                // boxed representation would.
                self.stack.push(Value::I64(align.unwrap_or(8) as i64));
            }

            BuiltinId::_Allocatedinline => {
                // _allocatedinline(T) - whether instances of T are stored inline
                // (unboxed) when held in a container, backing Pure Julia
                // `Base.allocatedinline` (Issue #5107). Mirrors upstream
                // `jl_stored_inline` (`julia/src/datatype.c`) for the
                // concrete/immutable subset this VM supports.
                let type_val = self.stack.pop_value()?;
                let registry = RuntimeTypeRegistry::new_with_struct_defs(
                    self.compile_context.as_ref(),
                    &self.abstract_types,
                    &self.struct_defs,
                );
                let inline = match &type_val {
                    Value::DataType(jt) => registry.object(jt).is_stored_inline(),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).is_stored_inline()
                    }
                    _ => false,
                };
                self.stack.push(Value::Bool(inline));
            }

            BuiltinId::_TypeParameters => {
                // _type_parameters(T) - tuple of concrete type parameters.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let params = match &val {
                    Value::DataType(jt) => registry.object(jt).parameters_with_values(),
                    Value::RuntimeTypeVar(_) => vec![],
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).parameters_with_values()
                    }
                    _ => vec![],
                };
                // Issue #4698: if a parameter is a `TypeVar` whose identity was
                // recorded at parametric-type construction time, hand back the
                // original id-bearing `RuntimeTypeVar` so that
                // `Vector{T}.parameters[1] === T` holds. Concrete parameters and
                // unrecorded TypeVars fall back to a plain `DataType`.
                // Issue #5162: integer/value parameters (array dim `N`, `Val{5}`,
                // ...) surface as concrete values, not `DataType`.
                let elements = params
                    .into_iter()
                    .map(|param| self.reflection_parameter_to_value(param))
                    .collect();
                // Issue #4722: <DataType>.parameters is a Core.SimpleVector (svec),
                // not a Tuple, matching upstream Julia.
                self.stack
                    .push(Value::SimpleVector(TupleValue { elements }));
            }

            BuiltinId::_UnionAllVar => {
                // _unionall_var(T) - TypeVar bound by UnionAll-like parametric type T.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let var = match &val {
                    Value::DataType(jt) => registry.object(jt).unionall_var(),
                    Value::RuntimeTypeVar(_) => None,
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).unionall_var()
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    VmError::TypeError(format!("type {:?} has no field var", val.value_type()))
                })?;
                self.stack.push(Value::DataType(Box::new(var)));
            }

            BuiltinId::_UnionAllBody => {
                // _unionall_body(T) - body of UnionAll-like parametric type T.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let body = match &val {
                    Value::DataType(jt) => registry.object(jt).unionall_body(),
                    Value::RuntimeTypeVar(_) => None,
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).unionall_body()
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    VmError::TypeError(format!("type {:?} has no field body", val.value_type()))
                })?;
                self.stack.push(Value::DataType(Box::new(body)));
            }

            BuiltinId::_TypeVarName => {
                // _type_var_name(T) - TypeVar.name as Symbol.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let name = match &val {
                    Value::DataType(jt) => registry.object(jt).typevar_name(),
                    Value::RuntimeTypeVar(tv) => Some(tv.name.clone()),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).typevar_name()
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    VmError::TypeError(format!("type {:?} has no field name", val.value_type()))
                })?;
                self.stack.push(Value::Symbol(SymbolValue::new(&name)));
            }

            BuiltinId::_TypeVarLowerBound => {
                // _type_var_lower_bound(T) - TypeVar.lb. Julia's default is Union{}.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let lb = match &val {
                    Value::DataType(jt) => registry.object(jt).typevar_lower_bound(),
                    Value::RuntimeTypeVar(tv) => Some(tv.lower_bound.clone()),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).typevar_lower_bound()
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    VmError::TypeError(format!("type {:?} has no field lb", val.value_type()))
                })?;
                self.stack.push(Value::DataType(Box::new(lb)));
            }

            BuiltinId::_TypeVarUpperBound => {
                // _type_var_upper_bound(T) - TypeVar.ub. Unbounded TypeVars use Any.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                let ub = match &val {
                    Value::DataType(jt) => registry.object(jt).typevar_upper_bound(),
                    Value::RuntimeTypeVar(tv) => Some(tv.upper_bound.clone()),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        registry.object(&jt).typevar_upper_bound()
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    VmError::TypeError(format!("type {:?} has no field ub", val.value_type()))
                })?;
                self.stack.push(Value::DataType(Box::new(ub)));
            }

            BuiltinId::_Getfield => {
                // _getfield(x, i) - get field value by index (1-based, like Julia)
                let index_val = self.stack.pop_value()?;
                let obj_val = self.stack.pop_value()?;

                let index = match &index_val {
                    Value::I64(i) => *i as usize,
                    Value::I32(i) => *i as usize,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_getfield index must be an integer, got {:?}",
                            index_val
                        )))
                    }
                };

                // Convert from 1-based to 0-based indexing
                if index == 0 {
                    return Err(VmError::FieldIndexOutOfBounds {
                        index: 0,
                        field_count: 0,
                    });
                }
                let field_idx = index - 1;

                let field_value = match &obj_val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            si.get_field(field_idx).cloned()
                        } else {
                            None
                        }
                    }
                    Value::Struct(si) => si.get_field(field_idx).cloned(),
                    Value::Tuple(t) => t.elements.get(field_idx).cloned(),
                    Value::NamedTuple(nt) => nt.values.get(field_idx).cloned(),
                    Value::Expr(expr) => expr_value_field_by_index(expr, field_idx),
                    Value::Generator(generator) => {
                        Some(self.generator_projected_field_by_index(generator, field_idx)?)
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_getfield requires a struct or tuple, got {:?}",
                            obj_val
                        )))
                    }
                };

                match field_value {
                    Some(v) => self.stack.push(v),
                    None => {
                        let field_count = match &obj_val {
                            Value::StructRef(idx) => self
                                .struct_heap
                                .get(*idx)
                                .map(|s| s.values.len())
                                .unwrap_or(0),
                            Value::Struct(si) => si.values.len(),
                            Value::Tuple(t) => t.elements.len(),
                            Value::NamedTuple(nt) => nt.values.len(),
                            Value::Expr(_) => 2,
                            Value::Generator(_) => 2,
                            _ => 0,
                        };
                        return Err(VmError::FieldIndexOutOfBounds {
                            index: field_idx,
                            field_count,
                        });
                    }
                }
            }

            BuiltinId::Getfield => {
                // getfield(x, name) or getfield(x, i) - get field by name (Symbol) or index (Int)
                let field_arg = self.stack.pop_value()?;
                let obj_val = self.stack.pop_value()?;

                // Determine if field access is by name (Symbol) or by index (Int)
                match &field_arg {
                    Value::Symbol(sym) => {
                        // Access by field name (Symbol)
                        let field_name = sym.as_str();
                        let field_value = match &obj_val {
                            Value::StructRef(idx) => {
                                if let Some(si) = self.struct_heap.get(*idx) {
                                    // Look up field index by name from struct definition
                                    let type_id = si.type_id;
                                    if let Some(def) = self.struct_defs.get(type_id) {
                                        if let Some(field_idx) = def
                                            .fields
                                            .iter()
                                            .position(|(name, _)| name == field_name)
                                        {
                                            si.get_field(field_idx).cloned()
                                        } else {
                                            return Err(VmError::TypeError(format!(
                                                "type {} has no field {}",
                                                si.struct_name, field_name
                                            )));
                                        }
                                    } else {
                                        return Err(VmError::TypeError(format!(
                                            "struct definition not found for type_id {}",
                                            type_id
                                        )));
                                    }
                                } else {
                                    return Err(VmError::TypeError(format!(
                                        "invalid StructRef({})",
                                        idx
                                    )));
                                }
                            }
                            Value::Struct(si) => {
                                let type_id = si.type_id;
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    if let Some(field_idx) =
                                        def.fields.iter().position(|(name, _)| name == field_name)
                                    {
                                        si.get_field(field_idx).cloned()
                                    } else {
                                        return Err(VmError::TypeError(format!(
                                            "type {} has no field {}",
                                            si.struct_name, field_name
                                        )));
                                    }
                                } else {
                                    return Err(VmError::TypeError(format!(
                                        "struct definition not found for type_id {}",
                                        type_id
                                    )));
                                }
                            }
                            Value::NamedTuple(nt) => nt.get_by_name(field_name).ok().cloned(),
                            // Issue #7614: explicit `getfield(ex, :head/:args)` and
                            // `getproperty(ex, ...)` can reach this generic path when
                            // the receiver is carried through an Any-typed parameter.
                            Value::Expr(expr) => match expr_value_field_by_name(expr, field_name) {
                                Some(value) => Some(value),
                                None => {
                                    return Err(VmError::TypeError(format!(
                                        "type Expr has no field {}",
                                        field_name
                                    )));
                                }
                            },
                            Value::Generator(generator) => {
                                Some(self.generator_projected_field(generator, field_name)?)
                            }
                            Value::Module(module) => {
                                self.get_module_binding(&module.name, field_name)
                            }
                            Value::DataType(jt) => {
                                let registry = RuntimeTypeRegistry::new(
                                    self.compile_context.as_ref(),
                                    &self.abstract_types,
                                );
                                let object = registry.object(jt);
                                match field_name {
                                    // Issue #4722: getfield(T, :parameters) is a
                                    // Core.SimpleVector (svec), matching upstream.
                                    // Issue #5162: include integer/value params
                                    // (array dim `N`, `Val{5}`, ...).
                                    "parameters" => {
                                        let params = object.parameters_with_values();
                                        let elements = params
                                            .into_iter()
                                            .map(|p| self.reflection_parameter_to_value(p))
                                            .collect();
                                        Some(Value::SimpleVector(TupleValue { elements }))
                                    }
                                    "var" => {
                                        object.unionall_var().map(|t| Value::DataType(Box::new(t)))
                                    }
                                    "body" => {
                                        object.unionall_body().map(|t| Value::DataType(Box::new(t)))
                                    }
                                    "name" => Some(Value::RuntimeTypeName(Box::new(
                                        RuntimeTypeNameValue {
                                            name: object.typename_symbol(),
                                        },
                                    ))),
                                    "lb" => object
                                        .typevar_lower_bound()
                                        .map(|t| Value::DataType(Box::new(t))),
                                    "ub" => object
                                        .typevar_upper_bound()
                                        .map(|t| Value::DataType(Box::new(t))),
                                    _ => None,
                                }
                            }
                            Value::RuntimeTypeVar(tv) => match field_name {
                                // Issue #4722: empty parameters svec for a TypeVar.
                                "parameters" => {
                                    Some(Value::SimpleVector(TupleValue { elements: vec![] }))
                                }
                                "name" => Some(Value::Symbol(SymbolValue::new(&tv.name))),
                                "lb" => Some(Value::DataType(Box::new(tv.lower_bound.clone()))),
                                "ub" => Some(Value::DataType(Box::new(tv.upper_bound.clone()))),
                                _ => None,
                            },
                            Value::RuntimeTypeName(type_name) => match field_name {
                                "name" => Some(Value::Symbol(SymbolValue::new(&type_name.name))),
                                _ => None,
                            },
                            // Base.RefValue{T} has a single field `x` (Issue #5130).
                            // `r.x` / `getfield(r, :x)` return the boxed value, matching
                            // upstream; this also lets the generic `show(io, ::Any)`
                            // fallback (which enumerates fieldnames) render Ref correctly.
                            Value::Ref(cell) => {
                                if field_name == "x" {
                                    Some(cell.borrow().clone())
                                } else {
                                    return Err(VmError::TypeError(format!(
                                        "type Base.RefValue has no field {}",
                                        field_name
                                    )));
                                }
                            }
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "getfield with Symbol requires a struct, NamedTuple, Module, or DataType, got {:?}",
                                    obj_val
                                )));
                            }
                        };

                        match field_value {
                            Some(v) => self.stack.push(v),
                            None => {
                                return Err(VmError::TypeError(format!(
                                    "type has no field {}",
                                    field_name
                                )));
                            }
                        }
                    }
                    Value::I64(i) => {
                        // Access by integer index (1-based)
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        let field_idx = index - 1;

                        let field_value = match &obj_val {
                            Value::StructRef(idx) => {
                                if let Some(si) = self.struct_heap.get(*idx) {
                                    si.get_field(field_idx).cloned()
                                } else {
                                    None
                                }
                            }
                            Value::Struct(si) => si.get_field(field_idx).cloned(),
                            Value::Tuple(t) => t.elements.get(field_idx).cloned(),
                            Value::NamedTuple(nt) => nt.values.get(field_idx).cloned(),
                            // Issue #7614: `Expr` field access by 1-based index
                            // (1 => head, 2 => args), matching upstream.
                            Value::Expr(expr) => expr_value_field_by_index(expr, field_idx),
                            Value::Generator(generator) => {
                                Some(self.generator_projected_field_by_index(generator, field_idx)?)
                            }
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "getfield requires a struct or tuple, got {:?}",
                                    obj_val
                                )));
                            }
                        };

                        match field_value {
                            Some(v) => self.stack.push(v),
                            None => {
                                let field_count = match &obj_val {
                                    Value::StructRef(idx) => self
                                        .struct_heap
                                        .get(*idx)
                                        .map(|s| s.values.len())
                                        .unwrap_or(0),
                                    Value::Struct(si) => si.values.len(),
                                    Value::Tuple(t) => t.elements.len(),
                                    Value::NamedTuple(nt) => nt.values.len(),
                                    Value::Generator(_) => 2,
                                    Value::Expr(_) => 2,
                                    _ => 0,
                                };
                                return Err(VmError::FieldIndexOutOfBounds {
                                    index: field_idx,
                                    field_count,
                                });
                            }
                        }
                    }
                    Value::I32(i) => {
                        // Handle I32 index as well
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        let field_idx = index - 1;

                        let field_value = match &obj_val {
                            Value::StructRef(idx) => {
                                if let Some(si) = self.struct_heap.get(*idx) {
                                    si.get_field(field_idx).cloned()
                                } else {
                                    None
                                }
                            }
                            Value::Struct(si) => si.get_field(field_idx).cloned(),
                            Value::Tuple(t) => t.elements.get(field_idx).cloned(),
                            Value::NamedTuple(nt) => nt.values.get(field_idx).cloned(),
                            // Issue #7614: `Expr` field access by 1-based index
                            // (1 => head, 2 => args), matching upstream.
                            Value::Expr(expr) => expr_value_field_by_index(expr, field_idx),
                            Value::Generator(generator) => {
                                Some(self.generator_projected_field_by_index(generator, field_idx)?)
                            }
                            _ => {
                                return Err(VmError::TypeError(format!(
                                    "getfield requires a struct or tuple, got {:?}",
                                    obj_val
                                )));
                            }
                        };

                        match field_value {
                            Some(v) => self.stack.push(v),
                            None => {
                                let field_count = match &obj_val {
                                    Value::StructRef(idx) => self
                                        .struct_heap
                                        .get(*idx)
                                        .map(|s| s.values.len())
                                        .unwrap_or(0),
                                    Value::Struct(si) => si.values.len(),
                                    Value::Tuple(t) => t.elements.len(),
                                    Value::NamedTuple(nt) => nt.values.len(),
                                    Value::Generator(_) => 2,
                                    Value::Expr(_) => 2,
                                    _ => 0,
                                };
                                return Err(VmError::FieldIndexOutOfBounds {
                                    index: field_idx,
                                    field_count,
                                });
                            }
                        }
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "getfield field argument must be a Symbol or Int, got {:?}",
                            field_arg
                        )));
                    }
                }
            }

            BuiltinId::Setfield => {
                // setfield!(x, name, v) or setfield!(x, i, v) - set field by name (Symbol) or index (Int)
                let value = self.stack.pop_value()?;
                let field_arg = self.stack.pop_value()?;
                let obj_val = self.stack.pop_value()?;

                // Determine field index from name (Symbol) or index (Int)
                let field_idx = match &field_arg {
                    Value::Symbol(sym) => {
                        // Access by field name (Symbol)
                        let field_name = sym.as_str();
                        let type_id = match &obj_val {
                            Value::StructRef(idx) => self.struct_heap.get(*idx).map(|s| s.type_id),
                            Value::Struct(si) => Some(si.type_id),
                            _ => None,
                        };
                        if let Some(tid) = type_id {
                            if let Some(def) = self.struct_defs.get(tid) {
                                def.fields
                                    .iter()
                                    .position(|(name, _)| name == field_name)
                                    .ok_or_else(|| {
                                        VmError::TypeError(format!(
                                            "type has no field {}",
                                            field_name
                                        ))
                                    })?
                            } else {
                                return Err(VmError::TypeError(format!(
                                    "struct definition not found for type_id {}",
                                    tid
                                )));
                            }
                        } else {
                            return Err(VmError::TypeError(
                                "setfield! requires a mutable struct".into(),
                            ));
                        }
                    }
                    Value::I64(i) => {
                        // Access by integer index (1-based)
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        index - 1
                    }
                    Value::I32(i) => {
                        let index = *i as usize;
                        if index == 0 {
                            return Err(VmError::FieldIndexOutOfBounds {
                                index: 0,
                                field_count: 0,
                            });
                        }
                        index - 1
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "setfield! field argument must be a Symbol or Int, got {:?}",
                            field_arg
                        )));
                    }
                };

                // Perform the field assignment
                match obj_val {
                    Value::StructRef(idx) => {
                        // Get type_id from heap
                        let type_id = self.struct_heap.get(idx).map(|s| s.type_id).unwrap_or(0);

                        // Check if struct is mutable
                        let is_mutable = self
                            .struct_defs
                            .get(type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);

                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            return Err(VmError::ImmutableFieldAssign(struct_name));
                        }

                        // Modify struct in heap directly
                        if let Some(s) = self.struct_heap.get_mut(idx) {
                            s.set_field(field_idx, value.clone())?;
                        }
                        // Return the assigned value (Julia semantics)
                        self.stack.push(value);
                    }
                    Value::Struct(mut s) => {
                        // Check if struct is mutable
                        let is_mutable = self
                            .struct_defs
                            .get(s.type_id)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false);

                        if !is_mutable {
                            let struct_name = self
                                .struct_defs
                                .get(s.type_id)
                                .map(|def| def.name.clone())
                                .unwrap_or_else(|| "unknown".to_string());
                            return Err(VmError::ImmutableFieldAssign(struct_name));
                        }

                        s.set_field(field_idx, value.clone())?;
                        // Allocate on heap for mutation tracking
                        self.struct_heap.push(s);
                        // Return the assigned value (Julia semantics)
                        self.stack.push(value);
                    }
                    _ => {
                        return Err(VmError::TypeError(
                            "setfield! requires a mutable struct".into(),
                        ));
                    }
                }
            }

            BuiltinId::Deepcopy => {
                // deepcopy(x) - recursive deep copy
                let val = self.stack.pop_value()?;
                let copied = self.deep_copy_value(&val)?;
                self.stack.push(copied);
            }

            BuiltinId::HasMethod => {
                // hasmethod(f, types[, kwnames]) - check if a method exists for
                // the given function, positional types, and optional keyword names.
                let world = if argc == 4 {
                    Some(self.stack.pop_value()?)
                } else {
                    None
                };
                let third_arg = if argc == 3 || argc == 4 {
                    Some(self.stack.pop_value()?)
                } else {
                    None
                };
                let (kw_names, world) = match (third_arg, world) {
                    (Some(Value::Tuple(tuple)), world) => {
                        let kw_val = Value::Tuple(tuple);
                        (Some(extract_kw_names_from_value(&kw_val)?), world)
                    }
                    (Some(world), None) => (None, Some(world)),
                    (Some(other), Some(_world)) => {
                        return Err(VmError::TypeError(format!(
                            "Expected tuple of Symbols for keyword names, got {:?}",
                            other.runtime_type()
                        )));
                    }
                    (None, world) => (None, world),
                };
                let types_val = self.stack.pop_value()?;
                let func_val = self.stack.pop_value()?;

                let has_explicit_world = world.is_some();
                let is_current_world = matches!(world, Some(Value::U64(1)) | Some(Value::I64(1)));
                if has_explicit_world && !is_current_world {
                    if kw_names.is_some() {
                        return Err(VmError::ErrorException(
                            "code reflection cannot be used from generated functions".into(),
                        ));
                    }
                    self.stack.push(Value::Bool(false));
                    return Ok(Some(()));
                }

                let func_name = extract_func_name(&func_val)?;
                let arg_types = extract_types_from_value(&types_val, &self.struct_heap)?;

                let has_match = self
                    .find_matching_methods(&func_name, Some(&arg_types))
                    .is_some_and(|methods| {
                        methods.iter().any(|info| match kw_names.as_deref() {
                            Some(names) => accepts_kw_names(info, names),
                            None => true,
                        })
                    });
                self.stack.push(Value::Bool(has_match));
            }

            BuiltinId::Which => {
                // which(f, types) - get the specific method that would be called
                let types_val = self.stack.pop_value()?;
                let func_val = self.stack.pop_value()?;

                let func_name = extract_func_name(&func_val)?;
                let arg_types = extract_types_from_value(&types_val, &self.struct_heap)?;

                match self.find_matching_methods(&func_name, Some(&arg_types)) {
                    Some(methods) if !methods.is_empty() => {
                        // Return the best matching method (first in the sorted list)
                        let info = &methods[0];
                        let method_struct =
                            self.create_method_struct(info, Some(&arg_types), None)?;
                        self.stack.push(method_struct);
                    }
                    _ => {
                        let type_str = arg_types
                            .iter()
                            .map(|t| t.name().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Err(VmError::MethodError(format!(
                            "no method matching {}({})",
                            func_name, type_str
                        )));
                    }
                }
            }

            BuiltinId::_MethodsByFtype => {
                // _methods_by_ftype(f) — all methods for a function
                // _methods_by_ftype(f, types) — type-filtered method lookup (Issue #3257)
                let mut arg_types = if argc == 2 {
                    // Two-argument form: pop types argument first (stack is LIFO)
                    let types_val = self.stack.pop_value()?;
                    Some(extract_types_from_value(&types_val, &self.struct_heap)?)
                } else {
                    None
                };
                let func_val = self.stack.pop_value()?;
                let closure_captures = match &func_val {
                    Value::Closure(cv) => Some(cv.captures.as_slice()),
                    _ => None,
                };
                let func_name = if argc == 1 {
                    if let Some((sig_func_name, sig_arg_types)) =
                        extract_signature_tuple_from_value(&func_val)?
                    {
                        arg_types = Some(sig_arg_types);
                        sig_func_name
                    } else {
                        extract_func_name(&func_val)?
                    }
                } else {
                    extract_func_name(&func_val)?
                };

                let methods = self.find_matching_methods(&func_name, arg_types.as_deref());
                let method_values: Vec<Value> = match methods {
                    Some(infos) => infos
                        .iter()
                        .map(|info| {
                            self.create_method_struct(info, arg_types.as_deref(), closure_captures)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    None => self.create_builtin_method_structs(&func_name, arg_types.as_deref())?,
                };

                self.push_array_value_as_wrapper(ArrayValue::any_vector(method_values))?;
            }

            BuiltinId::_TypeUnion => {
                let mut types = Vec::with_capacity(argc);
                for _ in 0..argc {
                    let value = self.stack.pop_value()?;
                    match value {
                        Value::DataType(ty) => types.push(*ty),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "_type_union expects type objects, got {:?}",
                                other
                            )));
                        }
                    }
                }
                types.reverse();
                // Issue #5066: canonicalize the runtime `Union{...}` (flatten /
                // dedup / subtype-absorb / sort / collapse) so equal Unions
                // share one normal form and compare `===` regardless of nesting
                // depth, member order, or duplicates. `canonicalize_union`
                // absorbs members into `Any` and folds `Bottom` for us, so the
                // ad-hoc special cases here are no longer needed.
                let result = crate::types::canonicalize_union(types);
                self.stack.push(Value::DataType(Box::new(result)));
            }

            BuiltinId::_MakeTupleType => {
                // _make_tuple_type(types) - construct `Tuple{types...}` from a
                // runtime collection of type objects (Tuple / Core.SimpleVector /
                // Vector). Backs Pure Julia `tuple_type_tail`/`tuple_type_cons`,
                // which cannot splat `T.parameters` into `Tuple{...}` directly
                // (Issue #5119). The collection elements must be type objects.
                let collection = self.stack.pop_value()?;
                let elements = if let Value::Tuple(t) | Value::SimpleVector(t) = &collection {
                    t.elements.clone()
                } else if let Some(arr) = native_array_value_ref(&collection) {
                    arr.borrow().to_value_vec()
                } else if let Some(arr) =
                    array_wrapper_value_to_array_value(&collection, &self.struct_heap)?
                {
                    arr.to_value_vec()
                } else {
                    return Err(VmError::TypeError(format!(
                        "_make_tuple_type expects a collection of type objects, got {:?}",
                        collection
                    )));
                };
                let mut tuple_types = Vec::with_capacity(elements.len());
                for value in elements {
                    match value {
                        Value::DataType(ty) => tuple_types.push(*ty),
                        other => {
                            return Err(VmError::TypeError(format!(
                                "_make_tuple_type expects type objects, got {:?}",
                                other
                            )));
                        }
                    }
                }
                self.stack
                    .push(Value::DataType(Box::new(JuliaType::TupleOf(tuple_types))));
            }

            BuiltinId::Names => {
                // names(m::Module) - default upstream form returns a
                // Vector{Symbol} containing the module's own binding and
                // exported names. `imported=true`/`all=true` are intentionally
                // left to the Julia wrapper surface once needed (Issue #7938).
                if argc != 1 {
                    return Err(VmError::TypeError(format!(
                        "names: expected 1 argument, got {}",
                        argc
                    )));
                }
                let module = self.stack.pop_value()?;
                let Value::Module(m) = module else {
                    return Err(VmError::TypeError(format!(
                        "names: expected Module, got {}",
                        super::util::value_type_name(&module)
                    )));
                };

                let self_name = m.name.rsplit('.').next().unwrap_or(&m.name).to_string();
                let mut names = Vec::with_capacity(m.exports.len() + 1);
                names.push(self_name.clone());
                for name in &m.exports {
                    if name != &m.name && name != &self_name {
                        names.push(name.clone());
                    }
                }
                names.sort();
                names.dedup();

                let values = names
                    .into_iter()
                    .map(|name| Value::Symbol(SymbolValue::new(name)))
                    .collect::<Vec<_>>();
                let len = values.len();
                let array = ArrayValue::memory_first_from_array_data_with_element_type(
                    ArrayData::Any(values),
                    vec![len],
                    ArrayElementType::Symbol,
                );
                self.push_array_value_as_wrapper(array)?;
            }

            BuiltinId::IsdefinedModuleBinding => {
                // _isdefined_module_binding(m::Module, s::Symbol) -> Bool
                // Backs function-form isdefined(::Module, ::Symbol) by checking
                // whether the module exposes a binding for the symbol — a
                // function, type, or (for Main) a global value. (Issue #5002/#4958)
                let symbol = self.stack.pop_value()?;
                let module = self.stack.pop_value()?;
                match (&module, &symbol) {
                    (Value::Module(m), Value::Symbol(s)) => {
                        let defined = self.module_binding_is_defined(&m.name, s.as_str());
                        self.stack.push(Value::Bool(defined));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_isdefined_module_binding: expected (Module, Symbol), got ({}, {})",
                            super::util::value_type_name(&module),
                            super::util::value_type_name(&symbol)
                        )));
                    }
                }
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }

    /// Returns `true` if `module_name` exposes a binding for `field_name`.
    ///
    /// Map a single `.parameters` reflection entry to its `Value`, matching
    /// upstream Julia (Issue #5162). Type parameters become a `DataType` (or an
    /// id-bearing `RuntimeTypeVar` when the TypeVar identity was recorded at
    /// construction time, so `Vector{T}.parameters[1] === T` holds — Issue
    /// #4698); integer/value parameters become the concrete value they denote
    /// (`Array{T,N}.parameters == svec(T, N)`, `Val{5}.parameters == svec(5)`).
    pub(crate) fn reflection_parameter_to_value(&self, param: ReflectionParameter) -> Value {
        match param {
            ReflectionParameter::Type(JuliaType::TypeVar(name, upper)) => self
                .runtime_typevar_identities
                .get(&(name.clone(), upper.clone()))
                .map(|tv| Value::RuntimeTypeVar(Box::new(tv.clone())))
                .unwrap_or(Value::DataType(Box::new(JuliaType::TypeVar(name, upper)))),
            ReflectionParameter::Type(jt) => Value::DataType(Box::new(jt)),
            ReflectionParameter::Int(n) => Value::I64(n),
            ReflectionParameter::Int8(n) => Value::I8(n),
            ReflectionParameter::Int16(n) => Value::I16(n),
            ReflectionParameter::Int32(n) => Value::I32(n),
            ReflectionParameter::Int128(n) => Value::I128(n),
            ReflectionParameter::UInt8(n) => Value::U8(n),
            ReflectionParameter::UInt16(n) => Value::U16(n),
            ReflectionParameter::UInt32(n) => Value::U32(n),
            ReflectionParameter::UInt64(n) => Value::U64(n),
            ReflectionParameter::UInt128(n) => Value::U128(n),
            ReflectionParameter::Bool(b) => Value::Bool(b),
            ReflectionParameter::Symbol(s) => Value::Symbol(SymbolValue::new(&s)),
            ReflectionParameter::Str(s) => Value::Str(s),
        }
    }

    /// Backs function-form `isdefined(::Module, ::Symbol)` (Issue #5002/#4958).
    /// A binding is considered defined when the symbol resolves to:
    /// - a global value or unqualified function in `Main`,
    /// - a qualified (`Module.name`) or unqualified function in the registry,
    /// - a builtin function/intrinsic,
    /// - a user-defined struct/abstract type,
    /// - a builtin type name, or
    /// - a macro binding visible in the module (`Symbol("@name")`, Issue #7948).
    fn module_binding_is_defined(&self, module_name: &str, field_name: &str) -> bool {
        // Macro bindings (`@name`) are erased during lowering, so they never reach
        // the global/function registry consulted below. Consult the per-module
        // macro binding table the compiler recorded instead (Issue #7948).
        if field_name.starts_with('@')
            && self
                .macro_bindings
                .get(module_name)
                .is_some_and(|names| names.contains(field_name))
        {
            return true;
        }

        // Values and functions resolvable via the existing getfield(::Module)
        // path (Main globals + qualified functions).
        if self.get_module_binding(module_name, field_name).is_some() {
            return true;
        }

        // Base/Core pure-Julia functions are stored under their unqualified
        // name in the function registry.
        if !self.get_function_indices_by_name(field_name).is_empty() {
            return true;
        }

        // Builtin functions / intrinsics (e.g. `sum`, `getfield`, `typeof`).
        if crate::builtins::BuiltinId::from_name(field_name).is_some() {
            return true;
        }

        // User-defined struct / abstract types.
        //
        // Types declared inside a module are registered under a
        // module-qualified key (`M.Box`), while top-level types keep their
        // unqualified name. Match both so `isdefined(M, :Box)` recognizes a
        // struct/type defined inside `module M ... end` (Issue #7916). The
        // qualified form is the one struct definitions inside a module use
        // (verified: `module M; struct Box ...; end` keys `struct_defs` as
        // `"M.Box"`), and construction (`M.Box(3)`) already resolves through it.
        let qualified_name = format!("{}.{}", module_name, field_name);
        let top_level_lookup = is_top_level_module_binding_scope(module_name);
        if self
            .struct_defs
            .iter()
            .any(|d| d.name == qualified_name || (top_level_lookup && d.name == field_name))
            || self
                .abstract_types
                .iter()
                .any(|d| d.name == qualified_name || (top_level_lookup && d.name == field_name))
        {
            return true;
        }

        if let Some(ctx) = &self.compile_context {
            if ctx.parametric_structs.contains_key(&qualified_name)
                || (top_level_lookup && ctx.parametric_structs.contains_key(field_name))
                || ctx.type_aliases.contains_key(&qualified_name)
                || (top_level_lookup && ctx.type_aliases.contains_key(field_name))
                || ctx
                    .primitive_types
                    .iter()
                    .any(|d| d.name == qualified_name || (top_level_lookup && d.name == field_name))
            {
                return true;
            }
        }

        // Builtin type names (Int64, Vector, Module, ...).
        is_builtin_module_type_name(field_name)
    }

    pub(in crate::vm) fn get_module_binding(
        &self,
        module_name: &str,
        field_name: &str,
    ) -> Option<Value> {
        if module_name == "Sys" && field_name == "WORD_SIZE" {
            return Some(Value::I64(i64::from(usize::BITS)));
        }

        if module_name == "Main" {
            if let Some(value) = self.get_global(field_name) {
                return Some(value);
            }
        }

        let qualified_name = format!("{}.{}", module_name, field_name);
        if let Some(value) = self.get_global(&qualified_name) {
            return Some(value);
        }
        if !self
            .get_function_indices_by_name(&qualified_name)
            .is_empty()
        {
            return Some(Value::Function(FunctionValue::new(qualified_name)));
        }
        if module_name == "Main" && !self.get_function_indices_by_name(field_name).is_empty() {
            return Some(Value::Function(FunctionValue::new(field_name)));
        }

        None
    }

    /// Find methods matching the given function name and optionally argument types.
    /// Returns None if no methods found, otherwise returns a vector of matching FunctionInfo
    /// sorted by specificity (most specific first).
    fn find_matching_methods(
        &self,
        func_name: &str,
        arg_types: Option<&[JuliaType]>,
    ) -> Option<Vec<FunctionInfo>> {
        if let Some(types) = arg_types {
            if let Some(variants) = split_reflection_union_arg_types(types) {
                let mut split_matches: Vec<FunctionInfo> = Vec::new();
                for variant in variants {
                    if let Some(matches) = self.find_matching_methods(func_name, Some(&variant)) {
                        for info in matches {
                            if !split_matches.iter().any(|existing| {
                                existing.name == info.name
                                    && existing.param_julia_types == info.param_julia_types
                                    && existing.vararg_param_index == info.vararg_param_index
                                    && existing.vararg_fixed_count == info.vararg_fixed_count
                            }) {
                                split_matches.push(info);
                            }
                        }
                    }
                }
                if split_matches.is_empty() {
                    return None;
                }
                return Some(split_matches);
            }
        }

        let mut matches: Vec<(FunctionInfo, u32)> = Vec::new();

        for info in &self.functions {
            if !reflection_function_name_matches(info, func_name) {
                continue;
            }

            // If no type filter, include all methods for this function
            let types = match arg_types {
                None => {
                    let score: u32 = info
                        .param_julia_types
                        .iter()
                        .map(|ty| ty.specificity() as u32)
                        .sum();
                    matches.push((info.as_ref().clone(), score));
                    continue;
                }
                Some(types) => types,
            };

            // Check arity (handle varargs)
            let arity_match = if let Some(vararg_idx) = info.vararg_param_index {
                if let Some(fixed_count) = info.vararg_fixed_count {
                    // Vararg{T, N}: exactly vararg_idx + N args (Issue #2525)
                    types.len() == vararg_idx + fixed_count
                } else {
                    types.len() >= vararg_idx
                }
            } else {
                info.param_julia_types.len() == types.len()
            };

            if !arity_match {
                continue;
            }

            let param_types: Vec<_> = if let Some(vararg_idx) = info.vararg_param_index {
                let vararg_ty = info
                    .param_julia_types
                    .get(vararg_idx)
                    .cloned()
                    .unwrap_or(JuliaType::Any);
                let mut expanded: Vec<_> = info
                    .param_julia_types
                    .iter()
                    .take(vararg_idx)
                    .cloned()
                    .collect();
                for _ in vararg_idx..types.len() {
                    expanded.push(vararg_ty.clone());
                }
                expanded
            } else {
                info.param_julia_types.clone()
            };

            if let Some(binding_count) =
                crate::inference_core::dispatch_resolver::julia_signature_match_with_bindings(
                    &param_types,
                    types,
                    &info.type_params,
                )
            {
                let score = crate::inference_core::dispatch_resolver::score_julia_signature_with_binding_count(
                    &param_types,
                    types,
                    binding_count,
                    info.vararg_param_index.is_some(),
                    info.vararg_fixed_count.is_some(),
                )
                .score;

                matches.push((info.as_ref().clone(), score));
            }
        }

        if matches.is_empty() {
            return None;
        }

        // Sort by score (descending - higher score = more specific)
        matches.sort_by_key(|b| std::cmp::Reverse(b.1));

        if arg_types.is_some_and(|types| types.iter().all(JuliaType::is_concrete)) {
            let best_score = matches[0].1;
            matches.retain(|(_, score)| *score == best_score);

            // Ambiguous dispatch: 2+ methods remain tied at the best score for an
            // all-concrete signature, and none is strictly more specific than every
            // other (e.g. `g(x::Int, y)` vs `g(x, y::Int)` on `(Int, Int)`).
            // Upstream reports no applicable method here, so the reflection channels
            // must too — `methods()` -> empty, `which()` -> error (`reflection.jl`
            // turns an empty match set into the right `return_types`/`infer` result).
            // Return an *empty* match set (NOT `None`, which would fall back to the
            // builtin path). Only the all-concrete case is gated; an abstract
            // signature with no unique most-specific method is a separate gap
            // (Issue #5937).
            if matches.len() >= 2 {
                let has_dominant = matches.iter().enumerate().any(|(idx, candidate)| {
                    matches.iter().enumerate().all(|(other_idx, other)| {
                        other_idx == idx
                            || function_info_params_strictly_more_specific(&candidate.0, &other.0)
                    })
                });
                if !has_dominant {
                    return Some(Vec::new());
                }
            }
        }

        Some(matches.into_iter().map(|(info, _)| info).collect())
    }

    /// Source-location reflection fields for a `Method` struct (Issue #5125):
    /// `(module, file, line)`. Upstream `Method` exposes `.module::Module`,
    /// `.file::Symbol`, and `.line::Int32`; `show(::Method)` renders these as
    /// ` @ Module file:line`.
    ///
    /// SubsetJuliaVM models the defining module of a top-level user definition
    /// as `Main`. The line number is the `FunctionInfo.def_line` recovered from
    /// the IR `Function.span` at compile time; the original source path is not
    /// retained, so `file` falls back to the representative `:none` symbol
    /// (upstream uses the actual path, which this layer cannot reproduce — the
    /// #5125 fixtures therefore assert only the structure of the show output,
    /// never an exact path/line).
    fn method_source_location(&self, info: &FunctionInfo) -> (Value, Value, Value) {
        let module = Value::Module(Box::new(ModuleValue::new("Main")));
        let file = Value::Symbol(SymbolValue::new("none"));
        (module, file, Value::I32(info.def_line as i32))
    }

    fn method_return_julia_type(
        &self,
        info: &FunctionInfo,
        arg_types: Option<&[JuliaType]>,
        closure_captures: Option<&[(String, Value)]>,
    ) -> JuliaType {
        arg_types
            .and_then(|types| builtin_reflection_return_type(&info.name, types))
            .or_else(|| arg_types.and_then(|types| resolve_direct_typevar_return_type(info, types)))
            .or_else(|| {
                arg_types.and_then(|types| {
                    self.infer_specialized_reflection_return_type(info, types, closure_captures)
                        .and_then(non_any_julia_type)
                })
            })
            .or_else(|| {
                // Non-Any snapshots include reachability `Union{}` and precise
                // method return types; keep them ahead of the tiny bytecode
                // literal scan so dead tails cannot override Bottom (Issue #6258).
                info.return_julia_type
                    .clone()
                    .filter(|ty| !matches!(ty, JuliaType::Any))
                    .map(|ty| instantiate_return_julia_type(ty, info, arg_types))
            })
            .or_else(|| self.bytecode_literal_return_julia_type(info))
            .or_else(|| {
                info.return_julia_type
                    .clone()
                    .map(|ty| instantiate_return_julia_type(ty, info, arg_types))
            })
            .unwrap_or_else(|| value_type_to_julia_type(&info.return_type, &self.struct_defs))
    }

    fn bytecode_literal_return_julia_type(&self, info: &FunctionInfo) -> Option<JuliaType> {
        let code = self.code.get(info.code_start..info.code_end)?;
        if code.len() > 3 {
            return None;
        }
        let return_idx = code.iter().rposition(is_return_instr)?;
        if code
            .iter()
            .take(return_idx)
            .any(|instr| matches!(instr, Instr::Jump(_)))
        {
            return None;
        }
        match code.get(return_idx)? {
            Instr::ReturnI64 => Some(JuliaType::Int64),
            Instr::ReturnF64 => Some(JuliaType::Float64),
            Instr::ReturnF32 => Some(JuliaType::Float32),
            Instr::ReturnF16 => Some(JuliaType::Float16),
            Instr::ReturnNothing => Some(JuliaType::Nothing),
            Instr::ReturnAny => return_idx
                .checked_sub(1)
                .and_then(|idx| code.get(idx))
                .and_then(push_literal_julia_type),
            _ => None,
        }
    }

    fn return_types_by_ftype(
        &self,
        func_name: &str,
        arg_types: &[JuliaType],
        closure_captures: Option<&[(String, Value)]>,
    ) -> Result<Vec<Value>, VmError> {
        let has_user_methods = self
            .functions
            .iter()
            .any(|info| reflection_function_name_matches(info, func_name));

        if !has_user_methods {
            return Ok(builtin_reflection_return_type(func_name, arg_types)
                .map(|ty| vec![Value::DataType(Box::new(ty))])
                .unwrap_or_default());
        }

        if arg_types.iter().all(JuliaType::is_concrete) {
            let table = self.reflection_method_table(func_name);
            return match table.dispatch(arg_types) {
                Ok(method) => {
                    let info = self.functions.get(method.global_index).ok_or_else(|| {
                        VmError::ErrorException(format!(
                            "reflection method index {} out of bounds for {}",
                            method.global_index, func_name
                        ))
                    })?;
                    Ok(vec![Value::DataType(Box::new(
                        self.method_return_julia_type(info, Some(arg_types), closure_captures),
                    ))])
                }
                Err(
                    DispatchError::NoMethodFound { .. } | DispatchError::AmbiguousMethod { .. },
                ) => Ok(Vec::new()),
            };
        }

        let return_types = self
            .find_matching_methods(func_name, Some(arg_types))
            .unwrap_or_default()
            .iter()
            .map(|info| {
                Value::DataType(Box::new(self.method_return_julia_type(
                    info,
                    Some(arg_types),
                    closure_captures,
                )))
            })
            .collect();
        Ok(return_types)
    }

    fn reflection_method_table(&self, func_name: &str) -> MethodTable {
        let mut table = MethodTable::new(func_name.to_string());
        for (global_index, info) in self.functions.iter().enumerate() {
            if !reflection_function_name_matches(info, func_name) {
                continue;
            }
            table.add_method(MethodSig::from_julia_projections(
                table.methods.len(),
                global_index,
                info.params
                    .iter()
                    .enumerate()
                    .map(|(idx, (name, _))| {
                        (
                            name.clone(),
                            info.param_julia_types
                                .get(idx)
                                .cloned()
                                .unwrap_or(JuliaType::Any),
                        )
                    })
                    .collect(),
                info.return_type.clone(),
                info.return_julia_type.clone(),
                info.is_base_extension,
                info.type_params.clone(),
                info.vararg_param_index,
                info.vararg_fixed_count,
            ));
        }

        let mut struct_hierarchy = StructHierarchy::new();
        for def in &self.struct_defs {
            struct_hierarchy.insert(&def.name, def.parent_type.clone(), Vec::new());
        }
        for def in &self.abstract_types {
            struct_hierarchy.insert_if_absent(
                &def.name,
                def.parent.clone(),
                def.type_params.clone(),
            );
        }

        let concrete_struct_names = self
            .struct_defs
            .iter()
            .map(|def| def.name.clone())
            .collect::<Vec<_>>();
        let abstract_type_names = self
            .abstract_types
            .iter()
            .map(|def| def.name.clone())
            .collect::<Vec<_>>();
        let mut parametric_struct_names = Vec::new();
        if let Some(ctx) = &self.compile_context {
            for (name, parametric) in &ctx.parametric_structs {
                let type_params = parametric
                    .def
                    .type_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect();
                struct_hierarchy.insert_if_absent(
                    name,
                    parametric.def.parent_type.clone(),
                    type_params,
                );
                parametric_struct_names.push(name.clone());
            }
        }
        table.set_struct_hierarchy_projection(
            &struct_hierarchy,
            &concrete_struct_names,
            &parametric_struct_names,
            &abstract_type_names,
        );

        table
    }

    /// Create a Method struct value from FunctionInfo
    fn create_method_struct(
        &self,
        info: &FunctionInfo,
        arg_types: Option<&[JuliaType]>,
        closure_captures: Option<&[(String, Value)]>,
    ) -> Result<Value, VmError> {
        // Find Method struct type_id
        let method_type_id = self
            .struct_defs
            .iter()
            .position(|def| def.name == "Method")
            .ok_or_else(|| VmError::TypeError("Method struct not defined".into()))?;

        // Build signature tuple from param_julia_types
        let sig_values: Vec<Value> = info
            .param_julia_types
            .iter()
            .map(|jt| Value::DataType(Box::new(jt.clone())))
            .collect();
        let sig = Value::Tuple(TupleValue::new(sig_values));

        let return_julia_type = self.method_return_julia_type(info, arg_types, closure_captures);

        let (def_module, def_file, def_line) = self.method_source_location(info);
        let method_struct = StructInstance {
            type_id: method_type_id,
            struct_name: "Method".into(),
            values: vec![
                Value::Symbol(SymbolValue::new(&info.name)),
                sig,
                // Method.nargs includes the function object and is Int32 in
                // upstream Julia (Issue #4989).
                Value::I32((info.params.len() + 1) as i32),
                Value::DataType(Box::new(return_julia_type)),
                // Source-location reflection fields: `.module` / `.file` /
                // `.line` (Issue #5125). The `mod` placeholder name is renamed
                // to `module` in the struct-definition table at startup.
                def_module,
                def_file,
                def_line,
                // Representative @constprop / inline reflection metadata
                // (Issues #4977/#4978/#4980/#4981).
                Value::U8(info.constprop_meta),
                Value::U8(info.inlining_meta),
                // Representative @nospecialize bitmask (Issue #4984).
                Value::I32(info.nospecialize_meta),
                // isva mirrors whether the matched method is varargs
                // (Issue #4983).
                Value::Bool(info.vararg_param_index.is_some()),
                // Representative @propagate_inbounds / @nospecializeinfer /
                // @assume_effects metadata (Issues #4979/#4983).
                Value::Bool(info.propagate_inbounds_meta),
                Value::Bool(info.nospecializeinfer_meta),
                Value::U16(info.purity_meta),
            ],
        };

        Ok(Value::Struct(method_struct))
    }

    /// Build the function IRs the shared inference engine reasons over: one
    /// clone of each specializable function's IR, renamed to its fallback's
    /// public name so callee lookups resolve by source name. Reused by every
    /// reflection inference entry point that constructs an engine.
    fn build_specializable_inference_functions(&self) -> Vec<crate::ir::core::Function> {
        self.specializable_functions
            .iter()
            .map(|specializable| {
                let mut func = specializable.ir.clone();
                if let Some(fallback) = self.functions.get(specializable.fallback_index) {
                    func.name = fallback.name.clone();
                }
                func
            })
            .collect()
    }

    fn infer_specialized_reflection_return_type(
        &self,
        info: &FunctionInfo,
        arg_types: &[JuliaType],
        closure_captures: Option<&[(String, Value)]>,
    ) -> Option<JuliaType> {
        if info.name == "collect" || info.name == "Base.collect" {
            if let Some(return_type) =
                self.infer_generator_collect_reflection_return_type(arg_types)
            {
                return Some(return_type);
            }
        }

        // A parameter triggers re-inference if its declared type is not fixed at
        // method-definition time: either fully untyped (`Any`) or annotated with a
        // `where`-bound type variable (`x::T`, `::Type{T}`, `xs::Vector{T}`, ...).
        // For the latter, reflection must substitute the concrete argument type for
        // the TypeVar to recover a precise return type (Issue #5003). Without this
        // the snapshot return type (e.g. `Any` for `id(x::T) where T = x`) is used.
        // `where`-bound type variables can hide inside a parametric container that
        // is stored as a string-spelled `Struct` (e.g. `Tuple{Vararg{T,N}}` lowers
        // to `Struct("NTuple{N, T}")`), so consult the method's declared `where`
        // parameters in addition to the structured TypeVar nodes (Issue #4843).
        let where_param_names: Vec<&str> = info
            .type_params
            .iter()
            .map(|param| param.name.as_str())
            .collect();
        let has_untyped_param = info.param_julia_types.iter().any(|param| {
            matches!(param, JuliaType::Any)
                || julia_type_mentions_typevar(param)
                || julia_type_mentions_where_param(param, &where_param_names)
        });
        let has_unknown_return_snapshot = matches!(info.return_type, ValueType::Any)
            && matches!(info.return_julia_type.as_ref(), None | Some(JuliaType::Any));
        if !has_untyped_param && !has_unknown_return_snapshot {
            return None;
        }

        let compile_context = self.compile_context.as_ref()?;
        let target = self.specializable_functions.iter().find(|specializable| {
            self.functions
                .get(specializable.fallback_index)
                .is_some_and(|fallback| {
                    fallback.name == info.name
                        && specializable.ir.params.len() == info.param_julia_types.len()
                        && fallback.param_julia_types == info.param_julia_types
                        && fallback.vararg_param_index == info.vararg_param_index
                        && fallback.vararg_fixed_count == info.vararg_fixed_count
                })
        })?;

        let global_types: HashMap<String, ValueType> = HashMap::new();
        let inference_functions = self.build_specializable_inference_functions();
        let mut engine = build_shared_inference_engine(
            &compile_context.struct_table,
            &global_types,
            inference_functions.iter(),
        );
        // Parametric struct definitions live only by base name in the compile
        // context, so hand them to the engine to recover concrete instantiated
        // constructor returns and field facts (Issues #4849 / #4850 / #4851).
        engine.set_parametric_structs(compile_context.parametric_structs.clone());
        self.seed_reflection_return_snapshots(&mut engine);
        let mut target_ir = target.ir.clone();
        target_ir.name = info.name.clone();
        let lattice_args: Vec<LatticeType> = arg_types
            .iter()
            .map(reflection_julia_type_to_lattice)
            .collect();

        // Bind each `where` parameter to the concrete value it takes for this
        // signature: type parameters (`T`) become their `DataType`, value/length
        // parameters (`N`, `M`) become their `Int64` length. A method body that
        // returns those parameters directly — e.g. `(T, N)` for
        // `v(xs::Tuple{Vararg{T,N}})` — then recovers the precise return shape
        // instead of widening to `Any` (Issue #4843).
        let where_param_bindings = bind_where_params_from_arg_types(
            &info.param_julia_types,
            arg_types,
            &where_param_names,
        );

        let inferred = if closure_captures.is_some() || !where_param_bindings.is_empty() {
            let mut base_env = TypeEnv::new();
            if let Some(captures) = closure_captures {
                for (name, value) in captures {
                    base_env.set(name, reflection_value_to_lattice(value));
                }
            }
            for (name, lattice) in &where_param_bindings {
                base_env.set(name, lattice.clone());
            }
            engine.infer_function_with_arg_types_and_base_env(&target_ir, &lattice_args, &base_env)
        } else {
            engine.infer_function_with_arg_types(&target_ir, &lattice_args)
        };

        lattice_to_parametric_julia_type(&inferred).or_else(|| {
            let value_type = lattice_to_value_type(&inferred);
            Some(value_type_to_julia_type(&value_type, &self.struct_defs))
        })
    }

    /// Compose the interprocedural exception type for a matched user method
    /// (Issue #5600), mirroring the engine setup of
    /// `infer_specialized_reflection_return_type`. Returns the exception
    /// `JuliaType` (a known exception struct or a `Union` of them), or `None`
    /// when the body is proven not to throw or the type is unknown — in which
    /// case the pure-Julia caller keeps its `Union{}` default.
    fn compose_function_exception_type(
        &mut self,
        info: &FunctionInfo,
        arg_types: &[JuliaType],
        closure_captures: Option<&[(String, Value)]>,
    ) -> Option<JuliaType> {
        // Closure captures must be in scope for the body walk; binding them by
        // re-seeding the engine env is not exposed here, so the capture case
        // falls back to the default (no composition) — the common top-level
        // user function path (no captures) is the representative surface.
        if closure_captures.is_some() {
            return None;
        }

        // Build the inference engine + target IR using shared `&self` state. The
        // borrows are confined to this block so the `&mut self` classifier below
        // can re-enter the VM to consult the pure-Julia exception classification
        // (Issue #6272).
        let (mut engine, target_ir, lattice_args) = {
            let compile_context = self.compile_context.as_ref()?;
            let target = self.specializable_functions.iter().find(|specializable| {
                self.functions
                    .get(specializable.fallback_index)
                    .is_some_and(|fallback| {
                        fallback.name == info.name
                            && specializable.ir.params.len() == info.param_julia_types.len()
                            && fallback.param_julia_types == info.param_julia_types
                            && fallback.vararg_param_index == info.vararg_param_index
                            && fallback.vararg_fixed_count == info.vararg_fixed_count
                    })
            })?;

            let global_types: HashMap<String, ValueType> = HashMap::new();
            let inference_functions = self.build_specializable_inference_functions();
            // Names of pure-Julia Base callees the walker may encounter. Their
            // exception types are obtained from the pure-Julia reflection
            // classification rather than by walking their (e.g. self-recursive
            // `gcd`/`lcm`) bodies (Issue #6272).
            let base_function_names: HashSet<String> = self
                .specializable_functions
                .iter()
                .filter(|specializable| specializable.fallback_index < self.base_function_count)
                .filter_map(|specializable| {
                    self.functions
                        .get(specializable.fallback_index)
                        .map(|fallback| fallback.name.clone())
                })
                .collect();

            let mut engine = build_shared_inference_engine(
                &compile_context.struct_table,
                &global_types,
                inference_functions.iter(),
            );
            engine.set_parametric_structs(compile_context.parametric_structs.clone());
            engine.set_base_function_names(base_function_names);

            let mut target_ir = target.ir.clone();
            target_ir.name = info.name.clone();
            let lattice_args: Vec<LatticeType> = arg_types
                .iter()
                .map(reflection_julia_type_to_lattice)
                .collect();

            (engine, target_ir, lattice_args)
        };

        let mut classifier = VmBaseExceptionClassifier { vm: self };
        let exct = engine.infer_function_exception_type(&mut classifier, &target_ir, &lattice_args);
        exception_type_to_julia_type(&exct)
    }

    fn seed_reflection_return_snapshots(
        &self,
        engine: &mut crate::compile::abstract_interp::InferenceEngine,
    ) {
        for (global_index, info) in self.functions.iter().enumerate() {
            if !matches!(info.return_type, ValueType::Struct(_)) && info.return_julia_type.is_none()
            {
                continue;
            }
            let params: Vec<_> = info
                .params
                .iter()
                .zip(info.param_julia_types.iter())
                .map(|((name, _), ty)| (name.clone(), ty.clone()))
                .collect();
            if params.len() != info.params.len() {
                continue;
            }
            let sig = MethodSig::from_julia_projections(
                0,
                global_index,
                params,
                info.return_type.clone(),
                info.return_julia_type.clone().or_else(|| {
                    matches!(info.return_type, ValueType::Struct(_))
                        .then(|| value_type_to_julia_type(&info.return_type, &self.struct_defs))
                }),
                info.is_base_extension,
                info.type_params.clone(),
                info.vararg_param_index,
                info.vararg_fixed_count,
            );
            engine.add_initial_method(info.name.clone(), sig);
        }
    }

    fn infer_generator_collect_reflection_return_type(
        &self,
        arg_types: &[JuliaType],
    ) -> Option<JuliaType> {
        let [JuliaType::Struct(generator_type)] = arg_types else {
            return None;
        };
        let generator_args = top_level_generic_args(generator_type, "Base.Generator")
            .or_else(|| top_level_generic_args(generator_type, "Generator"))?;
        let iter_type = generator_args.first()?;
        let callable_type = generator_args.get(1)?;
        let element_type = generator_iter_element_type(iter_type)?;
        let callable_name = callable_type
            .strip_prefix("typeof(")
            .and_then(|name| name.strip_suffix(')'))?;

        let compile_context = self.compile_context.as_ref()?;
        let inference_functions = self.build_specializable_inference_functions();
        let target = inference_functions
            .iter()
            .find(|func| func.name == callable_name)?;
        let mut engine = build_shared_inference_engine(
            &compile_context.struct_table,
            &HashMap::new(),
            inference_functions.iter(),
        );
        let arg_lattice = reflection_julia_type_to_lattice(&element_type);
        let inferred = engine.infer_function_with_arg_types(target, &[arg_lattice]);
        let value_type = lattice_to_value_type(&inferred);
        Some(JuliaType::VectorOf(Box::new(value_type_to_julia_type(
            &value_type,
            &self.struct_defs,
        ))))
    }

    fn create_builtin_method_structs(
        &self,
        func_name: &str,
        arg_types: Option<&[JuliaType]>,
    ) -> Result<Vec<Value>, VmError> {
        let Some(types) = arg_types else {
            return Ok(vec![]);
        };
        let Some(return_type) = builtin_reflection_return_type(func_name, types) else {
            return Ok(vec![]);
        };

        let method_type_id = self
            .struct_defs
            .iter()
            .position(|def| def.name == "Method")
            .ok_or_else(|| VmError::TypeError("Method struct not defined".into()))?;
        let sig_values = types
            .iter()
            .cloned()
            .map(|t| Value::DataType(Box::new(t)))
            .collect::<Vec<_>>();
        let method_struct = StructInstance {
            type_id: method_type_id,
            struct_name: "Method".into(),
            values: vec![
                Value::Symbol(SymbolValue::new(func_name)),
                Value::Tuple(TupleValue::new(sig_values)),
                // Method.nargs includes the function object and is Int32 in
                // upstream Julia (Issue #4989).
                Value::I32((types.len() + 1) as i32),
                Value::DataType(Box::new(return_type)),
                // Source-location reflection fields (Issue #5125). Built-in
                // callables are modeled as living in `Base` with no recoverable
                // source line.
                Value::Module(Box::new(ModuleValue::new("Base"))),
                Value::Symbol(SymbolValue::new("none")),
                Value::I32(0),
                // Built-in callables carry no retained @constprop / inline
                // metadata, so report the upstream defaults.
                Value::U8(0),
                Value::U8(0),
                // No retained @nospecialize / vararg / @propagate_inbounds /
                // @nospecializeinfer / @assume_effects metadata for the covered
                // built-in reflection slice (Issues #4979/#4983/#4984).
                Value::I32(0),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::U16(0),
            ],
        };
        Ok(vec![Value::Struct(method_struct)])
    }
}

fn reflection_function_name_matches(info: &FunctionInfo, query: &str) -> bool {
    if info.name == query {
        return true;
    }
    if !info.is_base_extension {
        return false;
    }

    let query_name = query.strip_prefix("Base.").unwrap_or(query);
    let info_name = info.name.rsplit('.').next().unwrap_or(info.name.as_str());
    query_name == info_name
}

fn is_top_level_module_binding_scope(module_name: &str) -> bool {
    matches!(module_name, "Main" | "Base" | "Core")
}

/// Returns `true` for built-in type names that are bound in `Base`/`Core`.
///
/// Used by function-form `isdefined(::Module, ::Symbol)` so that e.g.
/// `isdefined(Base, :Int64)` and `isdefined(Core, :Vector)` resolve to `true`
/// (Issue #5002/#4958). Mirrors `compile::type_helpers::is_builtin_type_name`
/// but lives in the VM layer where that helper is not visible.
fn is_builtin_module_type_name(name: &str) -> bool {
    matches!(
        name,
        // Numeric types
        "Int64" | "Int32" | "Int16" | "Int8" | "Int128" | "Int" |
        "UInt64" | "UInt32" | "UInt16" | "UInt8" | "UInt128" | "UInt" |
        "Float64" | "Float32" | "Float16" |
        "BigInt" | "BigFloat" |
        "Complex" | "ComplexF64" | "ComplexF32" |
        "Rational" |
        // Abstract numeric types
        "Number" | "Real" | "Integer" | "Signed" | "Unsigned" | "AbstractFloat" |
        // String / char types
        "String" | "AbstractString" | "Char" |
        // Collection types
        "Array" | "Vector" | "Matrix" | "DenseArray" | "DenseVector" | "DenseMatrix" |
        "BitArray" | "BitVector" | "BitMatrix" |
        "AbstractArray" | "AbstractVector" | "AbstractMatrix" |
        "Tuple" | "NamedTuple" | "Dict" | "Set" | "Pair" |
        // Range types
        "AbstractRange" | "UnitRange" | "StepRange" | "StepRangeLen" | "LinRange" |
        // IO types
        "IO" | "IOBuffer" |
        // Other concrete/abstract types
        "Any" | "Nothing" | "Missing" | "Bool" | "Symbol" |
        "Function" | "Type" | "DataType" | "UnionAll" | "TypeVar" | "Module" |
        // Regex types
        "Regex" | "RegexMatch" |
        // Metaprogramming types
        "Expr" | "QuoteNode" | "LineNumberNode" | "GlobalRef" | "Method"
    )
}

/// Result type of an elementary arithmetic operator (`+`, `-`, `*`, `/`) applied
/// to two concrete numeric arguments, computed from the promotion lattice
/// (`Base.promote_type`). Returns `None` for anything else (non-numeric,
/// abstract, or unsupported arity/operator), leaving the caller's other
/// reflection layers to decide.
///
/// `+`, `-`, `*` yield `promote_type(A, B)`; `/` additionally widens the
/// promoted integer result to its floating-point counterpart (`float(...)`),
/// matching upstream (`/(Int,Int) == Float64`, `/(Float32,Int) == Float32`).
///
/// `Bool` operands are intentionally excluded: their arithmetic does not follow
/// `promote_type` (e.g. `+(Bool,Bool) == Int64` but `*(Bool,Bool) == Bool`), so
/// they defer to the broader reflection layers (a safe `Any` upper bound) rather
/// than risk a wrong concrete answer.
fn arithmetic_op_result_type(op: &str, arg_types: &[JuliaType]) -> Option<JuliaType> {
    if !matches!(op, "+" | "-" | "*" | "/") {
        return None;
    }
    let [a, b] = arg_types else {
        return None;
    };
    if matches!(a, JuliaType::Bool) || matches!(b, JuliaType::Bool) {
        return None;
    }
    if !(is_concrete_real_numeric(a) && is_concrete_real_numeric(b)) {
        return None;
    }
    let name_a = a.name();
    let name_b = b.name();
    let promoted = crate::compile::promotion::promote_type(&name_a, &name_b);
    if promoted == "Any" || promoted.is_empty() || promoted == "Union{}" {
        return None;
    }
    let result_name = if op == "/" {
        float_widen_type_name(&promoted)
    } else {
        promoted
    };
    let result = JuliaType::from_name_or_struct(&result_name);
    // Only return a concrete numeric answer; otherwise defer.
    if is_concrete_real_numeric(&result) {
        Some(result)
    } else {
        None
    }
}

/// `true` for concrete real numeric leaf types (integers, bools, floats) that
/// participate in the elementary arithmetic promotion lattice. Excludes
/// abstract numerics, bignums, complex/rational structs, and non-numbers.
fn is_concrete_real_numeric(ty: &JuliaType) -> bool {
    matches!(
        ty,
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
            | JuliaType::Bool
            | JuliaType::Float16
            | JuliaType::Float32
            | JuliaType::Float64
    )
}

/// Map a concrete real numeric type name to the type produced by `float(T)`:
/// integer/bool types widen to `Float64`, `Float16`/`Float32`/`Float64` are
/// preserved. Used for the `/` operator result.
fn float_widen_type_name(name: &str) -> String {
    match name {
        "Float16" => "Float16".to_string(),
        "Float32" => "Float32".to_string(),
        _ => "Float64".to_string(),
    }
}

fn builtin_reflection_return_type(func_name: &str, arg_types: &[JuliaType]) -> Option<JuliaType> {
    let short_name = func_name
        .rsplit_once('.')
        .map_or(func_name, |(_, name)| name);
    // Precise result type for the elementary arithmetic operators over concrete
    // numeric arguments. Builtin operators carry no per-signature inferred
    // return-type snapshot when passed as runtime function *values* (the snapshot
    // collapses to `Any`), so `Base.infer_return_type(+, Tuple{Int,Float64})`
    // would be imprecise. Computing the result directly from the promotion
    // lattice here gives `Base.promote_op` (Issue #5114) the upstream answers
    // (`+(Int,Float64)==Float64`, `*(Int,Int)==Int64`, `/(Int,Int)==Float64`).
    if let Some(result) = arithmetic_op_result_type(short_name, arg_types) {
        return Some(result);
    }
    match short_name {
        "string" if !arg_types.is_empty() => Some(JuliaType::String),
        "length" if matches!(arg_types, [JuliaType::VectorOf(_) | JuliaType::Array]) => {
            Some(JuliaType::Int64)
        }
        "getindex"
            if matches!(
                arg_types,
                [
                    JuliaType::VectorOf(_),
                    JuliaType::Int64 | JuliaType::Integer
                ]
            ) =>
        {
            if let JuliaType::VectorOf(element) = &arg_types[0] {
                Some((**element).clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn top_level_generic_args(type_name: &str, prefix: &str) -> Option<Vec<String>> {
    let inner = type_name
        .strip_prefix(prefix)?
        .strip_prefix('{')?
        .strip_suffix('}')?;
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' => depth = depth.saturating_add(1),
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(inner[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    Some(args)
}

fn generator_iter_element_type(iter_type: &str) -> Option<JuliaType> {
    let base_name = iter_type
        .split_once('{')
        .map_or(iter_type, |(base, _)| base)
        .rsplit_once('.')
        .map_or_else(
            || {
                iter_type
                    .split_once('{')
                    .map_or(iter_type, |(base, _)| base)
            },
            |(_, name)| name,
        );
    match base_name {
        "Vector" | "Memory" | "UnitRange" | "StepRange" | "StepRangeLen" => {
            top_level_generic_args(iter_type, base_name)
                .or_else(|| top_level_generic_args(iter_type, &format!("Base.{base_name}")))
                .and_then(|args| args.into_iter().next())
                .map(|name| JuliaType::from_name_or_struct(&name))
        }
        "Matrix" => top_level_generic_args(iter_type, "Matrix")
            .or_else(|| top_level_generic_args(iter_type, "Base.Matrix"))
            .and_then(|args| args.into_iter().next())
            .map(|name| JuliaType::from_name_or_struct(&name)),
        "Array" => top_level_generic_args(iter_type, "Array")
            .or_else(|| top_level_generic_args(iter_type, "Base.Array"))
            .and_then(|args| args.into_iter().next())
            .map(|name| JuliaType::from_name_or_struct(&name)),
        _ => None,
    }
}

/// Convert a composed `ExceptionType` to the exception `JuliaType` returned to
/// the pure-Julia `infer_exception_type` (Issue #5600). `Bottom` (proven
/// no-throw) returns `None` so the caller keeps its `Union{}` default; `Any`
/// (could throw anything, e.g. `gcd`/`lcm` over `BigInt`) surfaces `Any`
/// (Issue #6284).
fn exception_type_to_julia_type(exct: &ExceptionType) -> Option<JuliaType> {
    match exct {
        // No exception path proven — the caller keeps its `Union{}` default.
        ExceptionType::Bottom => None,
        // Could throw anything (e.g. a composed `gcd`/`lcm` over `BigInt`) —
        // surface `Any`, matching upstream (Issue #6284).
        ExceptionType::Any => Some(JuliaType::Any),
        ExceptionType::Known(name) => Some(JuliaType::Struct((*name).to_string())),
        ExceptionType::Union(set) => Some(JuliaType::Union(
            set.iter()
                .map(|n| JuliaType::Struct((*n).to_string()))
                .collect(),
        )),
    }
}

/// Union two exception `JuliaType`s (across multiple matched methods),
/// flattening nested `Union`s and de-duplicating (Issue #5600).
fn merge_exception_julia_types(a: JuliaType, b: JuliaType) -> JuliaType {
    fn collect(t: JuliaType, out: &mut Vec<JuliaType>) {
        match t {
            JuliaType::Union(inner) => {
                for m in inner {
                    collect(m, out);
                }
            }
            other => {
                if !out.contains(&other) {
                    out.push(other);
                }
            }
        }
    }
    let mut members = Vec::new();
    collect(a, &mut members);
    collect(b, &mut members);
    match members.len() {
        0 => JuliaType::Bottom,
        1 => {
            let Some(member) = members.pop() else {
                return JuliaType::Bottom;
            };
            member
        }
        _ => JuliaType::Union(members),
    }
}

/// Consults the pure-Julia reflection classification
/// (`Base._classified_exception_type`) for a Base callee's exception type by
/// re-entering the VM synchronously (Issue #6272). Used by the interprocedural
/// exception composer so the semantics of pure-Julia Base helpers such as
/// `gcd`/`lcm` stay owned by pure Julia rather than encoded as Rust name
/// special-cases. See [`BaseCalleeExceptionClassifier`].
struct VmBaseExceptionClassifier<'a, R: RngLike> {
    vm: &'a mut Vm<R>,
}

impl<R: RngLike> BaseCalleeExceptionClassifier for VmBaseExceptionClassifier<'_, R> {
    fn classify_base_callee(
        &mut self,
        name: &str,
        arg_types: &[LatticeType],
    ) -> Option<ExceptionType> {
        // Build the `(f, Tuple{argtypes...})` arguments the pure-Julia classifier
        // expects. `f` is the same `Value::Function` shape a bare function
        // reference evaluates to, so `nameof(f)` recovers `name`.
        let julia_arg_types: Vec<JuliaType> = arg_types
            .iter()
            .map(|lt| {
                lattice_to_parametric_julia_type(lt).unwrap_or_else(|| {
                    value_type_to_julia_type(&lattice_to_value_type(lt), &self.vm.struct_defs)
                })
            })
            .collect();
        let types_val = Value::DataType(Box::new(JuliaType::TupleOf(julia_arg_types)));
        let func_val = Value::Function(FunctionValue::new(name.to_string()));

        // Consult the pure-Julia classification synchronously — the single
        // source of truth for these callees' exception types. A nested VM error
        // (unexpected for this pure table lookup) degrades to "no
        // classification", i.e. the conservative no-throw default.
        let classified = self
            .vm
            .eval_dispatch_call("_classified_exception_type", vec![func_val, types_val])
            .ok()?;
        classified_value_to_exception_type(&classified)
    }
}

/// Convert the `Value` returned by `Base._classified_exception_type` (a known
/// exception `DataType`, a `Union{...}` of them, `Any`, or `nothing`) into an
/// `ExceptionType`. `Any` becomes [`ExceptionType::Any`] (Issue #6284); `nothing`
/// and unrecognized names yield `None` (Issue #6272).
fn classified_value_to_exception_type(val: &Value) -> Option<ExceptionType> {
    let Value::DataType(jt) = val else {
        return None;
    };
    // An `Any` classification (e.g. `gcd`/`lcm` over `BigInt`, which delegates to
    // GMP and cannot be proven `nothrow`) composes to `ExceptionType::Any`,
    // matching upstream (Issue #6284).
    if matches!(jt.as_ref(), JuliaType::Any) {
        return Some(ExceptionType::Any);
    }
    let mut names: Vec<&'static str> = Vec::new();
    collect_interned_exception_names(jt, &mut names);
    match names.len() {
        0 => None,
        1 => Some(ExceptionType::Known(names[0])),
        _ => Some(ExceptionType::Union(names.into_iter().collect())),
    }
}

/// Collect the interned names of the exception types named by `jt`, flattening
/// `Union`s. Names with no static interning are dropped (Issue #6272).
fn collect_interned_exception_names(jt: &JuliaType, out: &mut Vec<&'static str>) {
    // `_classified_exception_type` returns a concrete exception struct type or a
    // `Union` of them; recurse through unions and intern each named struct. Any
    // other shape is not an exception type and is ignored.
    let name = match jt {
        JuliaType::Union(members) => {
            for member in members {
                collect_interned_exception_names(member, out);
            }
            return;
        }
        JuliaType::Struct(name) => name.as_str(),
        _ => return,
    };
    if let Some(interned) = intern_known_exception_name(name) {
        if !out.contains(&interned) {
            out.push(interned);
        }
    }
}

/// Map a (possibly module-qualified) exception type name to a `'static` name
/// usable in [`ExceptionType::Known`]. Covers the exception types the pure-Julia
/// reflection classification can return; unknown names yield `None` so the
/// composer degrades conservatively rather than fabricating a name (Issue #6272).
fn intern_known_exception_name(name: &str) -> Option<&'static str> {
    // Strip a Julia module prefix (e.g. `Base.OverflowError` → `OverflowError`).
    let bare = name.rsplit('.').next().unwrap_or(name);
    Some(match bare {
        "DomainError" => "DomainError",
        "InexactError" => "InexactError",
        "DivideError" => "DivideError",
        "OverflowError" => "OverflowError",
        "BoundsError" => "BoundsError",
        "KeyError" => "KeyError",
        "ArgumentError" => "ArgumentError",
        "DimensionMismatch" => "DimensionMismatch",
        "MethodError" => "MethodError",
        "TypeError" => "TypeError",
        "UndefVarError" => "UndefVarError",
        "UndefKeywordError" => "UndefKeywordError",
        "StackOverflowError" => "StackOverflowError",
        "AssertionError" => "AssertionError",
        "ErrorException" => "ErrorException",
        "OutOfMemoryError" => "OutOfMemoryError",
        "InitError" => "InitError",
        _ => return None,
    })
}

/// Reflection-side `JuliaType → LatticeType` lift.
///
/// Issue #5916: delegates to the canonical
/// [`crate::compile::bridge::julia_type_to_lattice`] so reflection builtins
/// (`Base.infer_return_type`, exception-type inference, element-type
/// inference) feed the inference engine the same lattice spelling the
/// compiler uses. This also fixes the historical divergences of the local
/// copy: empty `Union{}` now lowers to `Bottom` (it produced
/// `LatticeType::Union(∅)`), a union containing `Any` widens to `Top`, and
/// `Real`/`Signed`/`Unsigned` keep their abstract numeric markers instead of
/// collapsing to `Top`.
fn reflection_julia_type_to_lattice(ty: &JuliaType) -> LatticeType {
    crate::compile::bridge::julia_type_to_lattice(ty)
}

fn reflection_value_to_lattice(value: &Value) -> LatticeType {
    match value {
        Value::I64(v) => LatticeType::Const(ConstValue::Int64(*v)),
        Value::F64(v) => LatticeType::Const(ConstValue::Float64(*v)),
        Value::Bool(v) => LatticeType::Const(ConstValue::Bool(*v)),
        Value::Str(v) => LatticeType::Const(ConstValue::String(v.clone())),
        Value::Symbol(v) => LatticeType::Const(ConstValue::Symbol(v.as_str().to_string())),
        Value::Nothing => LatticeType::Const(ConstValue::Nothing),
        Value::Char(_) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }
        Value::Tuple(tuple) => LatticeType::Concrete(ConcreteType::Tuple {
            elements: tuple
                .elements
                .iter()
                .map(|value| match reflection_value_to_lattice(value) {
                    LatticeType::Concrete(concrete) => concrete,
                    LatticeType::Const(value) => value.to_concrete_type(),
                    _ => ConcreteType::Core(CoreType::Any),
                })
                .collect(),
        }),
        _ => LatticeType::Top,
    }
}

fn split_reflection_union_arg_types(arg_types: &[JuliaType]) -> Option<Vec<Vec<JuliaType>>> {
    let mut saw_union = false;
    let mut total_variants = 1usize;
    let mut variants: Vec<Vec<JuliaType>> = vec![Vec::with_capacity(arg_types.len())];

    for arg_type in arg_types {
        let alternatives = match arg_type {
            JuliaType::Union(types) if !types.is_empty() => {
                saw_union = true;
                total_variants = total_variants.checked_mul(types.len())?;
                if total_variants > 4 {
                    return None;
                }
                types.clone()
            }
            _ => vec![arg_type.clone()],
        };

        let mut next = Vec::with_capacity(variants.len() * alternatives.len());
        for prefix in &variants {
            for alternative in &alternatives {
                let mut candidate = prefix.clone();
                candidate.push(alternative.clone());
                next.push(candidate);
            }
        }
        variants = next;
    }

    saw_union.then_some(variants)
}

fn accepts_kw_names(info: &FunctionInfo, kw_names: &[String]) -> bool {
    if kw_names.is_empty() {
        return true;
    }

    let accepts_any_kw = info.kwparams.iter().any(|kw| kw.is_varargs);
    kw_names.iter().all(|name| {
        info.kwparams
            .iter()
            .any(|kw| !kw.is_varargs && kw.name == *name)
            || accepts_any_kw
    })
}

fn instantiate_return_julia_type(
    return_type: JuliaType,
    info: &FunctionInfo,
    arg_types: Option<&[JuliaType]>,
) -> JuliaType {
    if !julia_type_mentions_type_params(&return_type, &info.type_params) {
        return return_type;
    }

    let Some(arg_types) = arg_types else {
        return JuliaType::Any;
    };
    if info.type_params.is_empty() || info.param_julia_types.len() != arg_types.len() {
        return JuliaType::Any;
    }

    let mut bindings: HashMap<String, JuliaType> = HashMap::new();
    for (arg_ty, param_ty) in arg_types.iter().zip(info.param_julia_types.iter()) {
        if let Some(extracted) = arg_ty.extract_type_bindings(param_ty, &info.type_params) {
            for (name, ty) in extracted {
                if julia_type_mentions_type_param_name(&return_type, &name) && !ty.is_concrete() {
                    return JuliaType::Any;
                }
                match bindings.entry(name) {
                    std::collections::hash_map::Entry::Occupied(existing) => {
                        if existing.get() != &ty {
                            return JuliaType::Any;
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(vacant) => {
                        vacant.insert(ty);
                    }
                }
            }
        }
    }

    let instantiated = bindings
        .into_iter()
        .fold(return_type, |ty, (name, replacement)| {
            ty.substitute(&name, &replacement)
        });

    if julia_type_mentions_type_params(&instantiated, &info.type_params) {
        JuliaType::Any
    } else {
        instantiated
    }
}

fn julia_type_mentions_type_params(
    ty: &JuliaType,
    type_params: &[crate::types::TypeParam],
) -> bool {
    type_params
        .iter()
        .any(|type_param| julia_type_mentions_type_param_name(ty, &type_param.name))
}

fn julia_type_mentions_type_param_name(ty: &JuliaType, name: &str) -> bool {
    match ty {
        JuliaType::TypeVar(type_name, _) => type_name == name,
        JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_mentions_type_param_name(inner, name)
        }
        JuliaType::TupleOf(types) | JuliaType::Union(types) => types
            .iter()
            .any(|ty| julia_type_mentions_type_param_name(ty, name)),
        JuliaType::UnionAll { body, .. } => julia_type_mentions_type_param_name(body, name),
        JuliaType::Struct(type_name) => type_name == name,
        _ => false,
    }
}

/// Returns true if the type mentions any `TypeVar` (a `where`-bound type
/// variable), recursing into parametric containers. Used to decide whether a
/// method parameter's declared type leaves the concrete type open at
/// definition time and therefore needs reflection-time re-inference
/// (Issue #5003).
fn julia_type_mentions_typevar(ty: &JuliaType) -> bool {
    match ty {
        JuliaType::TypeVar(_, _) => true,
        JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_mentions_typevar(inner)
        }
        JuliaType::TupleOf(types) | JuliaType::Union(types) => {
            types.iter().any(julia_type_mentions_typevar)
        }
        JuliaType::UnionAll { body, .. } => julia_type_mentions_typevar(body),
        _ => false,
    }
}

/// Resolve the precise return type for a method whose body directly returns a
/// `where`-bound type parameter, e.g. `f(::Val{N}) where N = N` or
/// `h(xs::NTuple{N,T}) where {N,T} = N` / `h2(...) = T` (Issue #4845).
///
/// `info.direct_return_type_param` names the returned type variable `R`. We bind
/// `R` from the concrete `arg_types` against the declared `param_julia_types`:
///   * if `R` occupies a *value* position (`Val{N}`, the length slot of
///     `NTuple{N,T}`), the body returns the bound *value*, so the inferred
///     return type is the value's carrier type (`Val{3}` -> `Int64`);
///   * if `R` occupies a *type* position (the element slot of `NTuple{N,T}`),
///     the body returns the bound *type object*, so the inferred return type is
///     `Type{R}` (`h2` over `Tuple{Int64,Int64}` -> `Type{Int64}`).
///
/// Returns `None` (falling through to the existing inference paths) whenever the
/// binding cannot be resolved unambiguously, so this only ever *adds* precision.
fn resolve_direct_typevar_return_type(
    info: &FunctionInfo,
    arg_types: &[JuliaType],
) -> Option<JuliaType> {
    let returned = info.direct_return_type_param.as_deref()?;
    if info.param_julia_types.len() != arg_types.len() {
        return None;
    }

    for (param_ty, arg_ty) in info.param_julia_types.iter().zip(arg_types.iter()) {
        if let Some(resolved) = bind_returned_type_param(returned, param_ty, arg_ty) {
            return Some(resolved);
        }
    }
    None
}

/// Whether function `a`'s fixed parameter list is strictly more specific than
/// `b`'s by the pairwise-subtype rule, mirroring
/// `method_table.rs::method_params_strictly_more_specific` (Issue #5068) but over
/// reflection's `FunctionInfo` slots (Issue #5937).
///
/// Returns true when both have the same fixed arity, no varargs, every slot of
/// `a` is a subtype of the matching slot of `b`, and at least one slot is a
/// *strict* subtype (`a_i <: b_i` but not `b_i <: a_i`). Used to detect whether a
/// set of tied candidates has a unique most-specific member; if none dominates,
/// the dispatch is ambiguous.
fn function_info_params_strictly_more_specific(a: &FunctionInfo, b: &FunctionInfo) -> bool {
    if a.vararg_param_index.is_some() || b.vararg_param_index.is_some() {
        return false;
    }
    if a.param_julia_types.len() != b.param_julia_types.len() {
        return false;
    }
    let mut has_strict = false;
    for (a_ty, b_ty) in a.param_julia_types.iter().zip(b.param_julia_types.iter()) {
        if !type_values_subtype(a_ty, b_ty) {
            return false;
        }
        if !type_values_subtype(b_ty, a_ty) {
            has_strict = true;
        }
    }
    has_strict
}

/// Bind the returned type parameter `returned` from one `(param, arg)` pair,
/// returning the resolved return type if `returned` appears in `param`.
fn bind_returned_type_param(
    returned: &str,
    param_ty: &JuliaType,
    arg_ty: &JuliaType,
) -> Option<JuliaType> {
    // A bare returned type variable `T` declared as the parameter's own type
    // (`f(x::T) where T = T`) matched against a *concrete* argument: the body
    // returns the type object of that argument, so the inferred return type is
    // the precise `Type{C}` (e.g. `Type{Int64}`), not the widened `DataType`
    // (Issue #5933). Only concrete arguments resolve here; an abstract argument
    // (`Tuple{Integer}`) leaves `T` open (`Type{T} where T<:Integer`) and falls
    // through to `None`, which the existing inference paths widen to `Any` — that
    // larger abstract slice is handled elsewhere.
    if is_returned_typevar(param_ty, returned) {
        if arg_ty.is_concrete() {
            return Some(JuliaType::TypeOf(Box::new(arg_ty.clone())));
        }
        return None;
    }

    // `::Type{T}` matched against a concrete `Type{C}` argument: the body
    // returns the *type object* `C`, so the inferred return type is the precise
    // `Type{C}` (e.g. `Type{Int64}`), not the widened `DataType` (Issue #4268).
    if let JuliaType::TypeOf(param_inner) = param_ty {
        if is_returned_typevar(param_inner, returned) {
            if let JuliaType::TypeOf(concrete) = arg_ty {
                return Some(JuliaType::TypeOf(concrete.clone()));
            }
        }
        return None;
    }

    let JuliaType::Struct(param_name) = param_ty else {
        return None;
    };
    let (base, params) = split_parametric_name(param_name);

    // `NTuple{N,T}` (an alias for `Tuple{Vararg{T,N}}`) matched against a
    // concrete tuple argument: `N` is the length value-parameter, `T` is the
    // element type-parameter.
    if base == "NTuple" && params.len() == 2 {
        if let JuliaType::TupleOf(elements) = arg_ty {
            if params[0] == returned {
                // Length value-parameter: a tuple length is always `Int64`.
                return Some(JuliaType::Int64);
            }
            if params[1] == returned {
                // Element type-parameter: requires a homogeneous tuple so the
                // element type `T` is unambiguous.
                let first = elements.first()?;
                if elements.iter().all(|elem| elem == first) {
                    return Some(JuliaType::TypeOf(Box::new(first.clone())));
                }
            }
        }
        return None;
    }

    // Generic single/multi-parameter struct, e.g. `Val{N}` matched against
    // `Val{3}`. Align positional parameters and bind `returned` to the
    // corresponding concrete slot.
    if let JuliaType::Struct(arg_name) = arg_ty {
        let (arg_base, arg_params) = split_parametric_name(arg_name);
        if arg_base != base || arg_params.len() != params.len() {
            return None;
        }
        for (decl, concrete) in params.iter().zip(arg_params.iter()) {
            if *decl == returned {
                return resolve_param_slot_return(concrete);
            }
        }
    }
    None
}

/// True when `ty` is exactly the `where`-bound type variable named `returned`,
/// represented either as `TypeVar(returned, _)` or as a bare `Struct(returned)`
/// placeholder (both forms the parser/lowering can produce for a type variable).
fn is_returned_typevar(ty: &JuliaType, returned: &str) -> bool {
    match ty {
        JuliaType::TypeVar(name, _) => name == returned,
        JuliaType::Struct(name) => name == returned,
        _ => false,
    }
}

/// Resolve the return type for a single concrete parameter slot token.
///
/// A value-literal token (`3`, `1.5`, `:x`, `true`) means the body returns that
/// *value*, so the return type is the value's carrier type. A type token
/// (`Int64`, ...) means the body returns the *type object*, so the return type
/// is `Type{token}`.
fn resolve_param_slot_return(token: &str) -> Option<JuliaType> {
    if let Some(carrier) = value_param_carrier_type(token) {
        return Some(carrier);
    }
    let ty = JuliaType::from_name(token)?;
    Some(JuliaType::TypeOf(Box::new(ty)))
}

/// Map a value-parameter literal token to the carrier type of its value.
fn value_param_carrier_type(token: &str) -> Option<JuliaType> {
    let token = token.trim();
    if let Some(carrier) = typed_signed_int_carrier_type(token) {
        return Some(carrier);
    }
    if let Some(carrier) = typed_unsigned_int_carrier_type(token) {
        return Some(carrier);
    }
    if token == "true" || token == "false" {
        return Some(JuliaType::Bool);
    }
    if let Some(symbol) = token.strip_prefix(':') {
        if !symbol.is_empty()
            && symbol
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '!')
        {
            return Some(JuliaType::Symbol);
        }
    }
    if token.parse::<i64>().is_ok() {
        return Some(JuliaType::Int64);
    }
    // Only treat as Float64 when the token is a plain decimal literal, not a
    // type name or other identifier.
    if token.chars().all(|ch| {
        ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E'
    }) && token.chars().any(|ch| ch == '.')
        && token.parse::<f64>().is_ok()
    {
        return Some(JuliaType::Float64);
    }
    None
}

fn typed_signed_int_carrier_type(token: &str) -> Option<JuliaType> {
    for (bits, carrier) in [
        (8_u16, JuliaType::Int8),
        (16, JuliaType::Int16),
        (32, JuliaType::Int32),
        (128, JuliaType::Int128),
    ] {
        let prefix = format!("Int{bits}(");
        let Some(inner) = token
            .strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix(')'))
        else {
            continue;
        };
        let value = inner.parse::<i128>().ok()?;
        let in_range = match bits {
            8 => i8::try_from(value).is_ok(),
            16 => i16::try_from(value).is_ok(),
            32 => i32::try_from(value).is_ok(),
            128 => true,
            _ => false,
        };
        if !in_range {
            return None;
        }
        return Some(carrier);
    }
    None
}

fn typed_unsigned_int_carrier_type(token: &str) -> Option<JuliaType> {
    if let Some(digits) = token.strip_prefix("0x") {
        let Ok(value) = u128::from_str_radix(digits, 16) else {
            return None;
        };
        return match digits.len() {
            2 if u8::try_from(value).is_ok() => Some(JuliaType::UInt8),
            4 if u16::try_from(value).is_ok() => Some(JuliaType::UInt16),
            8 if u32::try_from(value).is_ok() => Some(JuliaType::UInt32),
            16 if u64::try_from(value).is_ok() => Some(JuliaType::UInt64),
            32 => Some(JuliaType::UInt128),
            _ => None,
        };
    }

    for (bits, carrier) in [
        (8_u16, JuliaType::UInt8),
        (16, JuliaType::UInt16),
        (32, JuliaType::UInt32),
        (64, JuliaType::UInt64),
        (128, JuliaType::UInt128),
    ] {
        let prefix = format!("UInt{bits}(");
        let Some(inner) = token
            .strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix(')'))
        else {
            continue;
        };
        let value = inner.parse::<u128>().ok()?;
        let in_range = match bits {
            8 => u8::try_from(value).is_ok(),
            16 => u16::try_from(value).is_ok(),
            32 => u32::try_from(value).is_ok(),
            64 => u64::try_from(value).is_ok(),
            128 => true,
            _ => false,
        };
        if !in_range {
            return None;
        }
        return Some(carrier);
    }
    None
}

/// Split a parametric type name `Base{P1,P2,...}` into the base and its
/// top-level comma-separated parameter tokens. Returns an empty parameter list
/// for non-parametric names. Tracks brace and parenthesis depth so nested
/// commas (e.g. tuple value parameters) are not split.
fn split_parametric_name(name: &str) -> (&str, Vec<&str>) {
    let Some(open) = name.find('{') else {
        return (name, Vec::new());
    };
    if !name.ends_with('}') {
        return (name, Vec::new());
    }
    let base = &name[..open];
    let inner = &name[open + 1..name.len() - 1];
    let mut params = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                params.push(inner[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start <= inner.len() {
        let tail = inner[start..].trim();
        if !tail.is_empty() || !params.is_empty() {
            params.push(tail);
        }
    }
    (base, params)
}

fn non_any_julia_type(julia_type: JuliaType) -> Option<JuliaType> {
    (!matches!(julia_type, JuliaType::Any)).then_some(julia_type)
}

fn is_return_instr(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::ReturnI64
            | Instr::ReturnF64
            | Instr::ReturnF32
            | Instr::ReturnF16
            | Instr::ReturnArray
            | Instr::ReturnNothing
            | Instr::ReturnAny
            | Instr::ReturnRange
            | Instr::ReturnStruct
            | Instr::ReturnRng
            | Instr::ReturnTuple
            | Instr::ReturnNamedTuple
            | Instr::ReturnDict
            | Instr::ReturnSet
            | Instr::ReturnRef
            | Instr::ReturnMemory
    )
}

fn push_literal_julia_type(instr: &Instr) -> Option<JuliaType> {
    match instr {
        Instr::PushI64(_) => Some(JuliaType::Int64),
        Instr::PushI128(_) => Some(JuliaType::Int128),
        Instr::PushBigInt(_) => Some(JuliaType::BigInt),
        Instr::PushBigFloat(_) => Some(JuliaType::BigFloat),
        Instr::PushF64(_) => Some(JuliaType::Float64),
        Instr::PushF32(_) => Some(JuliaType::Float32),
        Instr::PushF16(_) => Some(JuliaType::Float16),
        Instr::PushBool(_) => Some(JuliaType::Bool),
        Instr::PushStr(_) => Some(JuliaType::String),
        Instr::PushChar(_) => Some(JuliaType::Char),
        Instr::PushNothing => Some(JuliaType::Nothing),
        Instr::PushMissing => Some(JuliaType::Missing),
        _ => None,
    }
}

/// Returns true if a parameter annotation references any of the method's
/// `where` parameters. Unlike [`julia_type_mentions_typevar`], this also catches
/// `where` parameters hidden inside a string-spelled parametric type — e.g.
/// `Tuple{Vararg{T,N}}` / `NTuple{N,NTuple{M,T}}` lower to
/// `JuliaType::Struct("NTuple{N, T}")` whose embedded `N`/`T` are not structured
/// `TypeVar` nodes (Issue #4843).
fn julia_type_mentions_where_param(ty: &JuliaType, where_params: &[&str]) -> bool {
    if where_params.is_empty() {
        return false;
    }
    match ty {
        JuliaType::TypeVar(name, _) => where_params.contains(&name.as_str()),
        JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
            julia_type_mentions_where_param(inner, where_params)
        }
        JuliaType::TupleOf(types) | JuliaType::Union(types) => types
            .iter()
            .any(|inner| julia_type_mentions_where_param(inner, where_params)),
        JuliaType::UnionAll { body, .. } => julia_type_mentions_where_param(body, where_params),
        JuliaType::Struct(name) => type_name_mentions_where_param(name, where_params),
        _ => false,
    }
}

/// True if a parametric type-name string mentions a `where` parameter as one of
/// its identifier tokens (e.g. `"NTuple{N, T}"` mentions `N` and `T`).
fn type_name_mentions_where_param(name: &str, where_params: &[&str]) -> bool {
    split_identifier_tokens(name)
        .iter()
        .any(|token| where_params.contains(&token.as_str()))
}

/// Splits a type-name string into identifier-like tokens, dropping braces,
/// commas and surrounding whitespace.
fn split_identifier_tokens(name: &str) -> Vec<String> {
    name.split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Unifies each parameter annotation against its concrete argument type to bind
/// the method's `where` parameters. Type parameters bind to their `DataType`
/// (a type object whose own type is `DataType`); value/length parameters bind to
/// their `Int64` length (Issue #4843).
fn bind_where_params_from_arg_types(
    param_annotations: &[JuliaType],
    arg_types: &[JuliaType],
    where_params: &[&str],
) -> Vec<(String, LatticeType)> {
    let mut bindings: HashMap<String, LatticeType> = HashMap::new();
    for (annotation, concrete) in param_annotations.iter().zip(arg_types.iter()) {
        unify_where_params(annotation, concrete, where_params, &mut bindings);
    }
    bindings.into_iter().collect()
}

/// Recursively unifies a parameter annotation pattern against a concrete type,
/// recording `where`-parameter bindings. `NTuple{LEN, ELEM}` patterns (whether
/// structured `TupleOf` or string-spelled `Struct`) bind their length parameter
/// to the matched tuple arity and recurse into the element type.
fn unify_where_params(
    annotation: &JuliaType,
    concrete: &JuliaType,
    where_params: &[&str],
    bindings: &mut HashMap<String, LatticeType>,
) {
    match annotation {
        // Bare type parameter `T`: binds to the concrete argument's `DataType`.
        JuliaType::TypeVar(name, _) if where_params.contains(&name.as_str()) => {
            bind_type_param(name, concrete, bindings);
        }
        JuliaType::VectorOf(elem) | JuliaType::MatrixOf(elem) => {
            if let JuliaType::VectorOf(concrete_elem) | JuliaType::MatrixOf(concrete_elem) =
                concrete
            {
                unify_where_params(elem, concrete_elem, where_params, bindings);
            }
        }
        JuliaType::TypeOf(inner) => {
            // `::Type{T}` matched against a `DataType` value: bind `T` directly.
            if let JuliaType::TypeVar(name, _) = inner.as_ref() {
                if where_params.contains(&name.as_str()) {
                    bindings.insert(
                        (*name).to_string(),
                        LatticeType::Concrete(ConcreteType::DataType {
                            name: concrete.to_string(),
                        }),
                    );
                }
            }
        }
        // Structured parametric tuple `Tuple{...}`.
        JuliaType::TupleOf(elements) => {
            if let JuliaType::TupleOf(concrete_elems) = concrete {
                for (elem, concrete_elem) in elements.iter().zip(concrete_elems.iter()) {
                    unify_where_params(elem, concrete_elem, where_params, bindings);
                }
            }
        }
        // String-spelled parametric type, e.g. `NTuple{N, T}` / `NTuple{N, NTuple{M, T}}`.
        JuliaType::Struct(name) => {
            unify_ntuple_pattern(name, concrete, where_params, bindings);
        }
        _ => {}
    }
}

/// Binds a `where` type parameter `name` to the `DataType` describing `concrete`.
fn bind_type_param(name: &str, concrete: &JuliaType, bindings: &mut HashMap<String, LatticeType>) {
    bindings.insert(
        name.to_string(),
        LatticeType::Concrete(ConcreteType::DataType {
            name: concrete.to_string(),
        }),
    );
}

/// Unifies an `NTuple{LEN}` / `NTuple{LEN, ELEM}` pattern string against a
/// concrete tuple type, binding the length parameter to the tuple arity and,
/// for the two-argument form, recursing into the element. The element may
/// itself be a nested `NTuple{...}` or a bare type parameter.
fn unify_ntuple_pattern(
    pattern: &str,
    concrete: &JuliaType,
    where_params: &[&str],
    bindings: &mut HashMap<String, LatticeType>,
) {
    let Some((len_token, elem_token)) = parse_ntuple_pattern(pattern) else {
        return;
    };
    let JuliaType::TupleOf(concrete_elems) = concrete else {
        return;
    };
    // Length parameter binds to the concrete arity.
    if where_params.contains(&len_token.as_str()) {
        bindings.insert(
            len_token,
            LatticeType::Const(ConstValue::Int64(concrete_elems.len() as i64)),
        );
    }
    let Some(elem_token) = elem_token else {
        return;
    };
    let Some(first_elem) = concrete_elems.first() else {
        return;
    };
    let elem_token = elem_token.trim();
    if elem_token.starts_with("NTuple{") {
        unify_ntuple_pattern(elem_token, first_elem, where_params, bindings);
    } else if where_params.contains(&elem_token) {
        // Bare element type parameter `T`.
        bind_type_param(elem_token, first_elem, bindings);
    }
}

/// Parses an `NTuple{LEN}` / `NTuple{LEN, ELEM}` pattern string into its length
/// and optional element tokens, respecting nested braces in the element.
/// Returns `None` for any other shape.
fn parse_ntuple_pattern(pattern: &str) -> Option<(String, Option<String>)> {
    let inner = pattern.strip_prefix("NTuple{")?.strip_suffix('}')?;
    let mut depth = 0usize;
    let mut split_at = None;
    for (index, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                split_at = Some(index);
                break;
            }
            _ => {}
        }
    }
    let Some(split_at) = split_at else {
        let len = inner.trim();
        return (!len.is_empty()).then(|| (len.to_string(), None));
    };
    let len = inner[..split_at].trim().to_string();
    let elem = inner[split_at + 1..].trim().to_string();
    if len.is_empty() || elem.is_empty() {
        return None;
    }
    Some((len, Some(elem)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    // `Base.promote_op` (Issue #5114) precision for the elementary arithmetic
    // operators, which is what `arithmetic_op_result_type` powers through the
    // reflection return-type path.
    #[test]
    fn arithmetic_op_result_type_matches_promote_op_issue_5114() {
        use JuliaType::*;
        // +, -, * promote their arguments.
        assert_eq!(
            arithmetic_op_result_type("+", &[Int64, Float64]),
            Some(Float64)
        );
        assert_eq!(
            arithmetic_op_result_type("-", &[Int64, Float64]),
            Some(Float64)
        );
        assert_eq!(arithmetic_op_result_type("*", &[Int64, Int64]), Some(Int64));
        assert_eq!(arithmetic_op_result_type("+", &[Int32, Int64]), Some(Int64));
        assert_eq!(
            arithmetic_op_result_type("+", &[Int64, Float32]),
            Some(Float32)
        );
        // `/` widens the promoted result to a float.
        assert_eq!(
            arithmetic_op_result_type("/", &[Int64, Int64]),
            Some(Float64)
        );
        assert_eq!(
            arithmetic_op_result_type("/", &[Float32, Float32]),
            Some(Float32)
        );
        assert_eq!(
            arithmetic_op_result_type("/", &[Float16, Float16]),
            Some(Float16)
        );
        // Module-qualified operator names resolve the same through the public
        // reflection entry point (which strips the module prefix first).
        assert_eq!(
            builtin_reflection_return_type("Base.+", &[Int64, Float64]),
            Some(Float64)
        );
        assert_eq!(
            builtin_reflection_return_type("+", &[Int64, Float64]),
            Some(Float64)
        );
        // Non-numeric, abstract, wrong arity, or unsupported operator -> defer.
        assert_eq!(arithmetic_op_result_type("+", &[Int64, String]), None);
        assert_eq!(arithmetic_op_result_type("+", &[Number, Int64]), None);
        assert_eq!(arithmetic_op_result_type("+", &[Int64]), None);
        assert_eq!(arithmetic_op_result_type("^", &[Int64, Int64]), None);
        // Bool arithmetic does not follow promote_type, so it defers.
        assert_eq!(arithmetic_op_result_type("+", &[Bool, Bool]), None);
        assert_eq!(arithmetic_op_result_type("*", &[Bool, Int64]), None);
    }

    #[test]
    fn split_parametric_name_handles_value_and_nested_params_issue_4845() {
        assert_eq!(split_parametric_name("Val{3}"), ("Val", vec!["3"]));
        assert_eq!(split_parametric_name("Val{:x}"), ("Val", vec![":x"]));
        assert_eq!(
            split_parametric_name("NTuple{N,T}"),
            ("NTuple", vec!["N", "T"])
        );
        // Nested commas (e.g. a tuple value parameter) must not be split.
        assert_eq!(
            split_parametric_name("Val{(1, 2)}"),
            ("Val", vec!["(1, 2)"])
        );
        assert_eq!(split_parametric_name("Int64"), ("Int64", Vec::new()));
    }

    #[test]
    fn value_param_carrier_type_classifies_literals_issue_4845() {
        assert_eq!(value_param_carrier_type("3"), Some(JuliaType::Int64));
        assert_eq!(value_param_carrier_type("1.5"), Some(JuliaType::Float64));
        assert_eq!(value_param_carrier_type(":x"), Some(JuliaType::Symbol));
        assert_eq!(value_param_carrier_type("true"), Some(JuliaType::Bool));
        assert_eq!(value_param_carrier_type("false"), Some(JuliaType::Bool));
        assert_eq!(value_param_carrier_type("0x01"), Some(JuliaType::UInt8));
        assert_eq!(value_param_carrier_type("UInt8(1)"), Some(JuliaType::UInt8));
        assert_eq!(value_param_carrier_type("Int32(2)"), Some(JuliaType::Int32));
        // A bare type name is not a value literal.
        assert_eq!(value_param_carrier_type("Int64"), None);
    }

    #[test]
    fn bind_returned_type_param_resolves_val_and_ntuple_issue_4845() {
        // `Val{N}` value-parameter: returns the value's carrier type.
        assert_eq!(
            bind_returned_type_param(
                "N",
                &JuliaType::Struct("Val{N}".into()),
                &JuliaType::Struct("Val{3}".into())
            ),
            Some(JuliaType::Int64)
        );
        // `NTuple{N,T}` length value-parameter `N` -> Int64.
        assert_eq!(
            bind_returned_type_param(
                "N",
                &JuliaType::Struct("NTuple{N,T}".into()),
                &JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])
            ),
            Some(JuliaType::Int64)
        );
        // `NTuple{N,T}` element type-parameter `T` -> Type{T} for homogeneous
        // tuples.
        assert_eq!(
            bind_returned_type_param(
                "T",
                &JuliaType::Struct("NTuple{N,T}".into()),
                &JuliaType::TupleOf(vec![JuliaType::Float64, JuliaType::Float64])
            ),
            Some(JuliaType::TypeOf(Box::new(JuliaType::Float64)))
        );
        // Heterogeneous tuple leaves `T` ambiguous -> fall through.
        assert_eq!(
            bind_returned_type_param(
                "T",
                &JuliaType::Struct("NTuple{N,T}".into()),
                &JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Float64])
            ),
            None
        );
    }

    #[test]
    fn bind_returned_type_param_resolves_type_t_return_issue_4268() {
        // `g(::Type{T}) where T = T` returns the type object bound by the
        // `::Type{T}` argument, so the inferred return type is the precise
        // `Type{C}` (here `Type{Int64}`), not the widened `DataType`.
        assert_eq!(
            bind_returned_type_param(
                "T",
                &JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".into(), None))),
                &JuliaType::TypeOf(Box::new(JuliaType::Int64))
            ),
            Some(JuliaType::TypeOf(Box::new(JuliaType::Int64)))
        );
        // The type variable may also appear as a bare `Struct` placeholder.
        assert_eq!(
            bind_returned_type_param(
                "T",
                &JuliaType::TypeOf(Box::new(JuliaType::Struct("T".into()))),
                &JuliaType::TypeOf(Box::new(JuliaType::Struct("String".into())))
            ),
            Some(JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "String".into()
            ))))
        );
        // A `Type{T}` parameter against a non-`Type` argument cannot resolve the
        // returned variable -> fall through to existing inference paths.
        assert_eq!(
            bind_returned_type_param(
                "T",
                &JuliaType::TypeOf(Box::new(JuliaType::TypeVar("T".into(), None))),
                &JuliaType::Int64
            ),
            None
        );
        // A `Type{...}` parameter whose inner variable is not the returned name
        // must not claim a binding for the returned variable.
        assert_eq!(
            bind_returned_type_param(
                "T",
                &JuliaType::TypeOf(Box::new(JuliaType::TypeVar("S".into(), None))),
                &JuliaType::TypeOf(Box::new(JuliaType::Int64))
            ),
            None
        );
    }

    #[test]
    fn test_issue_4270_reflection_type_conversion_preserves_nothing_in_union_array() {
        let ty = JuliaType::VectorOf(Box::new(JuliaType::Union(vec![
            JuliaType::Int64,
            JuliaType::Nothing,
        ])));

        let lattice = reflection_julia_type_to_lattice(&ty);
        let LatticeType::Concrete(ConcreteType::Array { element, .. }) = lattice else {
            panic!("expected array lattice type");
        };
        let ConcreteType::UnionOf(types) = *element else {
            panic!("expected union element type");
        };

        assert_eq!(
            types.into_iter().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Nothing))
            ])
        );
    }

    #[test]
    fn test_issue_4843_parse_ntuple_pattern_nested() {
        assert_eq!(
            parse_ntuple_pattern("NTuple{N, T}"),
            Some(("N".to_string(), Some("T".to_string())))
        );
        assert_eq!(
            parse_ntuple_pattern("NTuple{N}"),
            Some(("N".to_string(), None))
        );
        assert_eq!(
            parse_ntuple_pattern("NTuple{N,NTuple{M, T}}"),
            Some(("N".to_string(), Some("NTuple{M, T}".to_string())))
        );
        assert_eq!(parse_ntuple_pattern("Tuple{Int64, Int64}"), None);
    }

    #[test]
    fn test_issue_4843_mentions_where_param_in_struct_name() {
        let ntuple = JuliaType::Struct("NTuple{N, T}".to_string());
        assert!(julia_type_mentions_where_param(&ntuple, &["T", "N"]));
        assert!(!julia_type_mentions_where_param(&ntuple, &["S"]));
        // No `where` params declared -> never matches.
        assert!(!julia_type_mentions_where_param(&ntuple, &[]));
    }

    #[test]
    fn test_issue_4843_bind_vararg_pair() {
        // v(xs::Tuple{Vararg{T,N}}) called with Tuple{Int64,Int64}:
        // T -> DataType(Int64), N -> Int64 length 2.
        let bindings = bind_where_params_from_arg_types(
            &[JuliaType::Struct("NTuple{N, T}".to_string())],
            &[JuliaType::TupleOf(vec![JuliaType::Int64, JuliaType::Int64])],
            &["T", "N"],
        );
        let map: HashMap<_, _> = bindings.into_iter().collect();
        assert_eq!(
            map.get("N"),
            Some(&LatticeType::Const(ConstValue::Int64(2)))
        );
        assert_eq!(
            map.get("T"),
            Some(&LatticeType::Concrete(ConcreteType::DataType {
                name: "Int64".to_string()
            }))
        );
    }

    #[test]
    fn test_issue_8404_bind_ntuple_length_only() {
        let bindings = bind_where_params_from_arg_types(
            &[JuliaType::Struct("NTuple{N}".to_string())],
            &[JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::Float64,
            ])],
            &["N"],
        );
        let map: HashMap<_, _> = bindings.into_iter().collect();
        assert_eq!(
            map.get("N"),
            Some(&LatticeType::Const(ConstValue::Int64(2)))
        );
    }

    #[test]
    fn test_issue_4843_bind_nested_ntuple() {
        // hn(xs::NTuple{N,NTuple{M,T}}) called with NTuple{2,NTuple{3,Float64}}:
        // N -> 2, M -> 3, T -> DataType(Float64).
        let inner = JuliaType::TupleOf(vec![
            JuliaType::Float64,
            JuliaType::Float64,
            JuliaType::Float64,
        ]);
        let bindings = bind_where_params_from_arg_types(
            &[JuliaType::Struct("NTuple{N,NTuple{M,T}}".to_string())],
            &[JuliaType::TupleOf(vec![inner.clone(), inner])],
            &["N", "M", "T"],
        );
        let map: HashMap<_, _> = bindings.into_iter().collect();
        assert_eq!(
            map.get("N"),
            Some(&LatticeType::Const(ConstValue::Int64(2)))
        );
        assert_eq!(
            map.get("M"),
            Some(&LatticeType::Const(ConstValue::Int64(3)))
        );
        assert_eq!(
            map.get("T"),
            Some(&LatticeType::Concrete(ConcreteType::DataType {
                name: "Float64".to_string()
            }))
        );
    }
}
