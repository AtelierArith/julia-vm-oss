//! Reflection builtin functions for the VM.
//!
//! Internal introspection operations: _fieldnames, _fieldtypes, deepcopy, methods, hasmethod, which.
//! These are internal VM builtins that are wrapped by Pure Julia functions
//! in subset_julia_vm/src/julia/base/reflection.jl.

// SAFETY: i64/i32→usize casts for field index access are guarded by `if index == 0`
// checks that reject non-positive values before the cast.
#![allow(clippy::cast_sign_loss)]

pub(super) mod primitives;

use crate::builtins::{BuiltinBindingAuthority, BuiltinId};
use crate::inference_core::{CorePrimitive, CoreType, CoreTypeVar};
use crate::ir::core::{Block, Expr, Function, NumericConvertTarget, Stmt};
use crate::rng::RngLike;
use crate::runtime_types::{
    build_reflection_inference_session, infer_function_effects, julia_type_to_lattice,
    lattice_to_julia_type, lattice_to_parametric_julia_type, lattice_to_value_type,
    BaseCalleeExceptionClassifier, ConcreteType, ConstValue, EffectBit, Effects, ExceptionType,
    LatticeType, MethodSig, MethodTable, ParametricStructDef, ReflectionInferenceSession, TypeEnv,
};
use crate::types::{
    builtin_type_binding_authority, BuiltinTypeBindingAuthority, DispatchError, JuliaType,
    StructHierarchy,
};
use std::collections::{HashMap, HashSet};

use super::error::VmError;
use super::instr::Instr;
use super::stack_ops::StackOps;
use super::type_objects::{ReflectionParameter, RuntimeTypeObjectKind, RuntimeTypeRegistry};
use super::type_utils::type_values_subtype;
use super::util;
use super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, ArrayData, ArrayElementType,
    ArrayValue, BindingFieldAccess, BindingValue, CallableSingletonIdentity, ExprValue,
    FunctionValue, GlobalRefValue, LineNumberNodeValue, ModuleValue, RuntimeTypeNameValue,
    RuntimeTypeVarValue, StructInstance, SymbolValue, TupleValue, Value, ValueType,
};
use super::{FunctionInfo, TypeVarProjectionKey, Vm};
use primitives::{
    extract_func_name, extract_kw_names_from_value, extract_signature_tuple_from_value,
    extract_types_from_value, value_type_to_julia_type,
};
use subset_julia_vm_bytecode::{DynamicCallCandidate, ReplMethodIdentity};

fn runtime_julia_type_to_value(ty: JuliaType) -> Value {
    Value::type_object(ty)
}

fn is_single_char_typevar_name(name: &str) -> bool {
    name.len() == 1
        && name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
}

fn qualify_module_local_type_alias_target(
    module_name: &str,
    target: &str,
    owner_defines: impl FnOnce(&str) -> bool,
) -> String {
    let target_head = target.split_once('{').map_or(target, |(head, _)| head);
    if target_head.contains('.') {
        return target.to_string();
    }
    let qualified_head = format!("{module_name}.{target_head}");
    if owner_defines(&qualified_head) {
        format!("{module_name}.{target}")
    } else {
        target.to_string()
    }
}

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

fn parametric_struct_field_index(
    ctx: Option<&crate::bytecode::RuntimeCompileContext>,
    struct_name: &str,
    field_name: &str,
) -> Option<usize> {
    let base = struct_name.split('{').next().unwrap_or(struct_name);
    ctx.and_then(|ctx| {
        ctx.parametric_structs
            .get(base)
            .or_else(|| {
                base.rsplit_once('.')
                    .and_then(|(_, short)| ctx.parametric_structs.get(short))
            })
            .and_then(|ps| {
                ps.def
                    .fields
                    .iter()
                    .position(|field| field.name == field_name)
            })
    })
}

fn line_number_node_file_value(line: &LineNumberNodeValue) -> Value {
    line.file
        .as_ref()
        .map_or(Value::Nothing, |file| Value::Symbol(SymbolValue::new(file)))
}

fn line_number_node_field_by_index(line: &LineNumberNodeValue, field_idx: usize) -> Option<Value> {
    match field_idx {
        0 => Some(Value::I64(line.line)),
        1 => Some(line_number_node_file_value(line)),
        _ => None,
    }
}

fn line_number_node_field_by_name(line: &LineNumberNodeValue, field_name: &str) -> Option<Value> {
    match field_name {
        "line" => Some(Value::I64(line.line)),
        "file" => Some(line_number_node_file_value(line)),
        _ => None,
    }
}

fn global_ref_field_by_index(global_ref: &GlobalRefValue, field_idx: usize) -> Option<Value> {
    match field_idx {
        0 => Some(Value::Module(Box::new(ModuleValue::new(
            &global_ref.module,
        )))),
        1 => Some(Value::Symbol(global_ref.name.clone())),
        2 => Some(Value::Binding(Box::new(BindingValue::new(
            global_ref.clone(),
        )))),
        _ => None,
    }
}

fn global_ref_field_by_name(global_ref: &GlobalRefValue, field_name: &str) -> Option<Value> {
    match field_name {
        "mod" => Some(Value::Module(Box::new(ModuleValue::new(
            &global_ref.module,
        )))),
        "name" => Some(Value::Symbol(global_ref.name.clone())),
        "binding" => Some(Value::Binding(Box::new(BindingValue::new(
            global_ref.clone(),
        )))),
        _ => None,
    }
}

/// Issue #10606: a `Union` type value exposes ONLY the two branch fields
/// `a`/`b`. When the compiler statically narrows the receiver to
/// `ValueType::DataType`, dot access routes `parameters`, `var`, `body`, `lb`,
/// `ub` to dedicated reflection builtins (`_TypeParameters`, `_UnionAllVar`,
/// `_UnionAllBody`, `_TypeVarLowerBound`, `_TypeVarUpperBound`) rather than
/// through the `getfield`/`GetFieldByName` field-match that already rejects
/// them for a Union receiver. If the runtime value turns out to be a Union,
/// those names are NOT fields upstream — every one raises `FieldError(Union,
/// :field)` — so each builtin funnels through this helper first. Returns
/// `Some(err)` only for a `Value::DataType` carrying a Union-kind type; all
/// other carriers fall through unchanged.
fn union_reflection_field_error(
    registry: &RuntimeTypeRegistry<'_>,
    val: &Value,
    field: &str,
) -> Option<VmError> {
    if let Value::DataType(jt) = val {
        if registry.object(jt).kind() == RuntimeTypeObjectKind::Union {
            return Some(VmError::FieldError {
                type_name: "Union".to_string(),
                field: field.to_string(),
            });
        }
    }
    None
}

fn same_reflection_method_signature(left: &FunctionInfo, right: &FunctionInfo) -> bool {
    reflection_method_identity(left) == reflection_method_identity(right)
}

fn reflection_candidate_is_at_least_as_new(
    existing: &FunctionInfo,
    candidate: &FunctionInfo,
) -> bool {
    candidate.definition_order == 0
        || existing.definition_order == 0
        || existing.definition_order <= candidate.definition_order
}

fn reflection_method_identity(info: &FunctionInfo) -> ReplMethodIdentity {
    let signature = MethodSig::from_julia_projections(
        0,
        0,
        info.params
            .iter()
            .enumerate()
            .map(|(index, (name, _))| {
                (
                    name.clone(),
                    info.param_julia_types
                        .get(index)
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
    );
    ReplMethodIdentity::from_method_sig(&info.name, &signature)
}

fn same_reflection_function_identity(left: &FunctionInfo, right: &FunctionInfo) -> bool {
    same_reflection_method_signature(left, right)
        && left.is_lowering_helper == right.is_lowering_helper
        && left.entry == right.entry
        && left.code_start == right.code_start
        && left.code_end == right.code_end
        && left.min_world == right.min_world
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
                let candidate_indices =
                    self.reflection_callable_candidate_indices(&func_val, &func_name)?;
                let composed = {
                    let mut acc: Option<JuliaType> = None;
                    for (info_index, info) in self.find_composable_methods_for_candidates(
                        &func_name,
                        &arg_types,
                        candidate_indices.as_deref(),
                    ) {
                        if let Some(jt) = self.compose_function_exception_type(
                            info_index,
                            &info,
                            &arg_types,
                            closure_captures,
                        ) {
                            acc = Some(match acc {
                                None => jt,
                                Some(prev) => merge_exception_julia_types(prev, jt),
                            });
                        }
                    }
                    acc
                };
                match composed {
                    Some(jt) => self.stack.push(Value::DataType(Box::new(jt))),
                    None => self.stack.push(Value::Nothing),
                }
            }
            BuiltinId::ComposeEffects => {
                // _compose_effects(f, types) — body-derived effect summary for
                // matched user methods (Issue #8441). Returns a tuple matching
                // Base.Effects fields, or `nothing` when no body can be walked.
                let types_val = self.stack.pop_value()?;
                let arg_types = extract_types_from_value(&types_val, &self.struct_heap)?;
                let func_val = self.stack.pop_value()?;
                let closure_captures = match &func_val {
                    Value::Closure(cv) => Some(cv.captures.as_slice()),
                    _ => None,
                };
                let func_name = extract_func_name(&func_val)?;
                let candidate_indices =
                    self.reflection_callable_candidate_indices(&func_val, &func_name)?;
                let composed = {
                    let mut acc: Option<Effects> = None;
                    for (info_index, info) in self.find_composable_methods_for_candidates(
                        &func_name,
                        &arg_types,
                        candidate_indices.as_deref(),
                    ) {
                        if let Some(effects) = self.compose_function_effects(
                            info_index,
                            &info,
                            &arg_types,
                            closure_captures,
                        ) {
                            acc = Some(match acc {
                                None => effects,
                                Some(prev) => prev.merge(&effects),
                            });
                        }
                    }
                    acc
                };
                match composed {
                    Some(effects) => self.stack.push(effects_to_value_tuple(effects)),
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
                let candidate_indices =
                    self.reflection_callable_candidate_indices(&func_val, &func_name)?;
                // A DataType callee is a constructor: its return type is
                // structural (the applied parametric spelling constructs
                // exactly itself; a bare family instantiates from the
                // argument types), so resolve it before ordinary
                // function-name reflection widens it to Any (Issue #11402).
                let return_types = if matches!(&func_val, Value::DataType(_)) {
                    match self.constructor_reflection_return_types(&func_name, &arg_types) {
                        Some(types) => types,
                        None => self.return_types_by_ftype(
                            &func_name,
                            &arg_types,
                            closure_captures,
                            candidate_indices.as_deref(),
                        )?,
                    }
                } else {
                    self.return_types_by_ftype(
                        &func_name,
                        &arg_types,
                        closure_captures,
                        candidate_indices.as_deref(),
                    )?
                };
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
                                    .map(|(name, _)| Value::str_new(name.clone()))
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
                                .map(|(name, _)| Value::str_new(name.clone()))
                                .collect()
                        } else {
                            vec![]
                        }
                    }
                    Value::DataType(jt) => {
                        if let JuliaType::TupleOf(elements) = jt.as_ref() {
                            let mut names = Vec::with_capacity(elements.len());
                            for index in 1..=elements.len() {
                                let index = i64::try_from(index).map_err(|_| {
                                    VmError::TypeError(
                                        "tuple field index does not fit in Int64".into(),
                                    )
                                })?;
                                names.push(Value::I64(index));
                            }
                            names
                        } else {
                            let registry = RuntimeTypeRegistry::new(
                                self.compile_context.as_ref(),
                                &self.abstract_types,
                            );
                            let object = registry.object(jt);
                            if object.kind() == RuntimeTypeObjectKind::Union
                                || object.typename_symbol() == "Union"
                            {
                                vec![
                                    Value::Symbol(SymbolValue::new("a")),
                                    Value::Symbol(SymbolValue::new("b")),
                                ]
                            } else {
                                object
                                    .field_names()
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|name| Value::Symbol(SymbolValue::new(&name)))
                                    .collect()
                            }
                        }
                    }
                    Value::NamedTuple(nt) => {
                        nt.names.iter().map(|n| Value::str_new(n.clone())).collect()
                    }
                    // Handle type name passed as string (e.g., fieldnames(Person))
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        if let JuliaType::TupleOf(elements) = &jt {
                            let mut names = Vec::with_capacity(elements.len());
                            for index in 1..=elements.len() {
                                let index = i64::try_from(index).map_err(|_| {
                                    VmError::TypeError(
                                        "tuple field index does not fit in Int64".into(),
                                    )
                                })?;
                                names.push(Value::I64(index));
                            }
                            names
                        } else {
                            let registry = RuntimeTypeRegistry::new(
                                self.compile_context.as_ref(),
                                &self.abstract_types,
                            );
                            let object = registry.object(&jt);
                            if object.kind() == RuntimeTypeObjectKind::Union
                                || object.typename_symbol() == "Union"
                            {
                                vec![
                                    Value::Symbol(SymbolValue::new("a")),
                                    Value::Symbol(SymbolValue::new("b")),
                                ]
                            } else {
                                object
                                    .field_names()
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|name| Value::Symbol(SymbolValue::new(&name)))
                                    .collect()
                            }
                        }
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
                // Issue #10606: `Union{...}.parameters` is a FieldError, not an
                // svec of the branch types.
                if let Some(e) = union_reflection_field_error(&registry, &val, "parameters") {
                    return Err(e);
                }
                let (owner, params) = match &val {
                    Value::DataType(jt) => (
                        Some((**jt).clone()),
                        registry.object(jt).parameters_with_values(),
                    ),
                    Value::RuntimeTypeVar(_) => (None, vec![]),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        (
                            Some(jt.clone()),
                            registry.object(&jt).parameters_with_values(),
                        )
                    }
                    _ => (None, vec![]),
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
                    .map(|param| match &owner {
                        Some(owner) => self.reflection_parameter_to_value_for_owner(owner, param),
                        None => self.reflection_parameter_to_value(param),
                    })
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
                // Issue #10606: `Union{...}.var` is a FieldError(Union, :var),
                // not FieldError(DataType, :var).
                if let Some(e) = union_reflection_field_error(&registry, &val, "var") {
                    return Err(e);
                }
                let (owner, var) = match &val {
                    Value::DataType(jt) => {
                        (Some((**jt).clone()), registry.object(jt).unionall_var())
                    }
                    Value::RuntimeTypeVar(_) => (None, None),
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        (Some(jt.clone()), registry.object(&jt).unionall_var())
                    }
                    _ => (None, None),
                };
                let var = var.ok_or_else(|| {
                    // Issue #10212: nonexistent field -> FieldError, matching upstream.
                    VmError::FieldError {
                        type_name: crate::vm::util::value_type_name(&val).to_string(),
                        field: "var".to_string(),
                    }
                })?;
                let value = match owner {
                    Some(_) if matches!(var, JuliaType::RuntimeTypeVar { .. }) => {
                        runtime_julia_type_to_value(var)
                    }
                    Some(owner) => self.runtime_typevar_value_for_unionall_projection(&owner, var),
                    None => self.fresh_runtime_typevar_value_for_projection(var),
                };
                self.stack.push(value);
            }

            BuiltinId::_UnionAllBody => {
                // _unionall_body(T) - body of UnionAll-like parametric type T.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                // Issue #10606: `Union{...}.body` is a FieldError(Union, :body).
                if let Some(e) = union_reflection_field_error(&registry, &val, "body") {
                    return Err(e);
                }
                let body = match &val {
                    Value::DataType(jt) => {
                        let object = registry.object(jt);
                        match (object.unionall_body(), object.unionall_var()) {
                            (Some(body), Some(var)) => {
                                Some(self.project_unionall_body_with_identity(jt, body, var))
                            }
                            (body, _) => body,
                        }
                    }
                    Value::RuntimeTypeVar(_) => None,
                    Value::Str(type_name) => {
                        let jt = JuliaType::from_name_or_struct(type_name);
                        let object = registry.object(&jt);
                        match (object.unionall_body(), object.unionall_var()) {
                            (Some(body), Some(var)) => {
                                Some(self.project_unionall_body_with_identity(&jt, body, var))
                            }
                            (body, _) => body,
                        }
                    }
                    _ => None,
                }
                .ok_or_else(|| {
                    // Issue #10212: nonexistent field -> FieldError, matching upstream.
                    VmError::FieldError {
                        type_name: crate::vm::util::value_type_name(&val).to_string(),
                        field: "body".to_string(),
                    }
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
                    // Issue #10212: nonexistent field -> FieldError, matching upstream.
                    VmError::FieldError {
                        type_name: crate::vm::util::value_type_name(&val).to_string(),
                        field: "name".to_string(),
                    }
                })?;
                self.stack.push(Value::Symbol(SymbolValue::new(&name)));
            }

            BuiltinId::_TypeVarLowerBound => {
                // _type_var_lower_bound(T) - TypeVar.lb. Julia's default is Union{}.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                // Issue #10606: `Union{...}.lb` is a FieldError(Union, :lb).
                if let Some(e) = union_reflection_field_error(&registry, &val, "lb") {
                    return Err(e);
                }
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
                    // Issue #10212: nonexistent field -> FieldError, matching upstream.
                    VmError::FieldError {
                        type_name: crate::vm::util::value_type_name(&val).to_string(),
                        field: "lb".to_string(),
                    }
                })?;
                self.stack.push(runtime_julia_type_to_value(lb));
            }

            BuiltinId::_TypeVarUpperBound => {
                // _type_var_upper_bound(T) - TypeVar.ub. Unbounded TypeVars use Any.
                let val = self.stack.pop_value()?;
                let registry =
                    RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
                // Issue #10606: `Union{...}.ub` is a FieldError(Union, :ub).
                if let Some(e) = union_reflection_field_error(&registry, &val, "ub") {
                    return Err(e);
                }
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
                    // Issue #10212: nonexistent field -> FieldError, matching upstream.
                    VmError::FieldError {
                        type_name: crate::vm::util::value_type_name(&val).to_string(),
                        field: "ub".to_string(),
                    }
                })?;
                self.stack.push(runtime_julia_type_to_value(ub));
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
                    // Issue #11509 (transactional fix): park the receiver
                    // only atomically with this raise via
                    // `field_index_out_of_bounds_with_receiver`, not ahead of
                    // the lookup below — see that helper's doc comment for
                    // why (a successful getfield must never leave a stale
                    // receiver parked for a later, unrelated raise).
                    return Err(self.field_index_out_of_bounds_with_receiver(0, 0, obj_val));
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
                    Value::QuoteNode(inner) if field_idx == 0 => Some((**inner).clone()),
                    Value::QuoteNode(_) => None,
                    Value::Generator(generator) => {
                        self.generator_projected_field_by_index(generator, field_idx)?
                    }
                    // Issue #11382: projected through the shared
                    // `RegexMatchValue::field_by_index` authority so this
                    // path, `getfield`, and dot-access cannot drift apart.
                    Value::RegexMatch(m) => m.field_by_index(field_idx)?,
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
                            Value::QuoteNode(_) => 1,
                            Value::Generator(_) => 2,
                            Value::RegexMatch(_) => 5,
                            _ => 0,
                        };
                        // Issue #11509: report the caller's original 1-based
                        // `index`, not the internal 0-based `field_idx`.
                        // Parked atomically with this raise (see comment
                        // above the index==0 arm).
                        return Err(self.field_index_out_of_bounds_with_receiver(
                            index,
                            field_count,
                            obj_val,
                        ));
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
                                    // Look up field index by name from struct definition.
                                    let type_id = si.type_id;
                                    let field_idx = self
                                        .struct_defs
                                        .get(type_id)
                                        .and_then(|def| {
                                            def.fields
                                                .iter()
                                                .position(|(name, _)| name == field_name)
                                        })
                                        .or_else(|| {
                                            parametric_struct_field_index(
                                                self.compile_context.as_ref(),
                                                &si.struct_name,
                                                field_name,
                                            )
                                        });
                                    if let Some(field_idx) = field_idx {
                                        si.get_field(field_idx).cloned()
                                    } else if self.struct_defs.get(type_id).is_none() {
                                        return Err(VmError::TypeError(format!(
                                            "struct definition not found for type_id {}",
                                            type_id
                                        )));
                                    } else {
                                        // Issue #10212: FieldError, matching upstream.
                                        return Err(VmError::FieldError {
                                            type_name: si.struct_name.to_string(),
                                            field: field_name.to_string(),
                                        });
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
                                let field_idx = self
                                    .struct_defs
                                    .get(type_id)
                                    .and_then(|def| {
                                        def.fields.iter().position(|(name, _)| name == field_name)
                                    })
                                    .or_else(|| {
                                        parametric_struct_field_index(
                                            self.compile_context.as_ref(),
                                            &si.struct_name,
                                            field_name,
                                        )
                                    });
                                if let Some(field_idx) = field_idx {
                                    si.get_field(field_idx).cloned()
                                } else if self.struct_defs.get(type_id).is_none() {
                                    return Err(VmError::TypeError(format!(
                                        "struct definition not found for type_id {}",
                                        type_id
                                    )));
                                } else {
                                    // Issue #10212: FieldError, matching upstream.
                                    return Err(VmError::FieldError {
                                        type_name: si.struct_name.to_string(),
                                        field: field_name.to_string(),
                                    });
                                }
                            }
                            Value::NamedTuple(nt) => nt.get_by_name(field_name).ok().cloned(),
                            // Issue #7614: explicit `getfield(ex, :head/:args)` and
                            // `getproperty(ex, ...)` can reach this generic path when
                            // the receiver is carried through an Any-typed parameter.
                            Value::Expr(expr) => match expr_value_field_by_name(expr, field_name) {
                                Some(value) => Some(value),
                                None => {
                                    // Issue #10212: FieldError, matching upstream 1.12.
                                    return Err(VmError::FieldError {
                                        type_name: "Expr".to_string(),
                                        field: field_name.to_string(),
                                    });
                                }
                            },
                            Value::QuoteNode(inner) => {
                                if field_name == "value" {
                                    Some((**inner).clone())
                                } else {
                                    // Issue #10212: FieldError, matching upstream 1.12.
                                    return Err(VmError::FieldError {
                                        type_name: "QuoteNode".to_string(),
                                        field: field_name.to_string(),
                                    });
                                }
                            }
                            Value::LineNumberNode(line) => {
                                match line_number_node_field_by_name(line, field_name) {
                                    Some(value) => Some(value),
                                    None => {
                                        // Issue #10212: FieldError, matching upstream 1.12.
                                        return Err(VmError::FieldError {
                                            type_name: "LineNumberNode".to_string(),
                                            field: field_name.to_string(),
                                        });
                                    }
                                }
                            }
                            Value::GlobalRef(global_ref) => {
                                match global_ref_field_by_name(global_ref, field_name) {
                                    Some(value) => Some(value),
                                    None => {
                                        // Issue #10212: FieldError, matching upstream 1.12.
                                        return Err(VmError::FieldError {
                                            type_name: "GlobalRef".to_string(),
                                            field: field_name.to_string(),
                                        });
                                    }
                                }
                            }
                            // Issue #10067: distinguish a modeled field
                            // (`:globalref`, `:flags`) from an upstream field
                            // that exists but is unset (`:value`,
                            // `:partitions`, `:backedges` -> UndefRefError)
                            // from a name that is not a Core.Binding field at
                            // all (-> FieldError). Shared with
                            // exec/struct_ops.rs so the two sites cannot drift.
                            Value::Binding(binding) => match binding.field_by_name(field_name) {
                                BindingFieldAccess::Value(value) => Some(value),
                                BindingFieldAccess::Undef => {
                                    return Err(VmError::UndefRefError);
                                }
                                BindingFieldAccess::NoField => {
                                    return Err(VmError::FieldError {
                                        type_name: "Core.Binding".to_string(),
                                        field: field_name.to_string(),
                                    });
                                }
                            },
                            Value::Generator(generator) => {
                                Some(self.generator_projected_field(generator, field_name)?)
                            }
                            // Issue #11382: shared `RegexMatchValue::field_by_name`
                            // authority — the FieldError fallback below handles
                            // `None` (unknown field name).
                            Value::RegexMatch(m) => m.field_by_name(field_name)?,
                            Value::Module(module) => {
                                match self.get_module_binding(&module.name, field_name) {
                                    Some(value) => Some(value),
                                    None => {
                                        // Issue #10318: a missing module binding is an
                                        // UndefVarError upstream, not a field error.
                                        // Carry the module scope so the message keeps
                                        // the module name (`not defined in
                                        // `Main.<Module>``), matching upstream 1.12.
                                        return Err(VmError::UndefVarErrorInModule {
                                            var: field_name.to_string(),
                                            scope: crate::vm::util::module_scope_string(
                                                &module.name,
                                            ),
                                        });
                                    }
                                }
                            }
                            Value::DataType(jt) => {
                                let registry = RuntimeTypeRegistry::new(
                                    self.compile_context.as_ref(),
                                    &self.abstract_types,
                                );
                                let object = registry.object(jt);
                                // Hoisted before the field match: the
                                // FieldError arm below needs the receiver
                                // kind after arms whose closures borrow
                                // `self` mutably (Issue #10313).
                                let receiver_is_union =
                                    object.kind() == RuntimeTypeObjectKind::Union;
                                match field_name {
                                    // A `Union` type value exposes its two
                                    // branch types as the fields `a`/`b`,
                                    // matching upstream (Issue #10313).
                                    "a" | "b" => object
                                        .union_branch_field(field_name)
                                        .map(runtime_julia_type_to_value),
                                    // Issue #4722: getfield(T, :parameters) is a
                                    // Core.SimpleVector (svec), matching upstream.
                                    // Issue #5162: include integer/value params
                                    // (array dim `N`, `Val{5}`, ...).
                                    // Issue #10606: Union-kind type values only expose
                                    // `a`/`b`; reflection fields are FieldError upstream.
                                    "parameters" if !receiver_is_union => {
                                        let params = object.parameters_with_values();
                                        let elements = params
                                            .into_iter()
                                            .map(|p| {
                                                self.reflection_parameter_to_value_for_owner(jt, p)
                                            })
                                            .collect();
                                        Some(Value::SimpleVector(TupleValue { elements }))
                                    }
                                    "var" if !receiver_is_union => object.unionall_var().map(|t| {
                                        if matches!(t, JuliaType::RuntimeTypeVar { .. }) {
                                            runtime_julia_type_to_value(t)
                                        } else {
                                            self.runtime_typevar_value_for_unionall_projection(
                                                jt, t,
                                            )
                                        }
                                    }),
                                    "body" if !receiver_is_union => {
                                        match (object.unionall_body(), object.unionall_var()) {
                                            (Some(body), Some(var)) => Some(Value::DataType(
                                                Box::new(self.project_unionall_body_with_identity(
                                                    jt, body, var,
                                                )),
                                            )),
                                            (Some(body), None) => {
                                                Some(Value::DataType(Box::new(body)))
                                            }
                                            _ => None,
                                        }
                                    }
                                    "name" if object.kind() == RuntimeTypeObjectKind::TypeVar => {
                                        object
                                            .typevar_name()
                                            .map(|name| Value::Symbol(SymbolValue::new(&name)))
                                    }
                                    "name" if !receiver_is_union => Some(Value::RuntimeTypeName(
                                        Box::new(RuntimeTypeNameValue {
                                            name: object.typename_symbol(),
                                            identity: object.typename_identity(),
                                        }),
                                    )),
                                    "lb" if !receiver_is_union => object
                                        .typevar_lower_bound()
                                        .map(runtime_julia_type_to_value),
                                    "ub" if !receiver_is_union => object
                                        .typevar_upper_bound()
                                        .map(runtime_julia_type_to_value),
                                    _ => {
                                        // Issue #10313: a Union-kind receiver
                                        // reports `Union` as its type name
                                        // (`FieldError(Union, :c)` upstream),
                                        // not the generic `DataType` that the
                                        // shared fall-through below derives
                                        // from the `Value` carrier.
                                        if receiver_is_union {
                                            return Err(VmError::FieldError {
                                                type_name: "Union".to_string(),
                                                field: field_name.to_string(),
                                            });
                                        }
                                        None
                                    }
                                }
                            }
                            Value::RuntimeTypeVar(tv) => match field_name {
                                // Issue #4722: empty parameters svec for a TypeVar.
                                "parameters" => {
                                    Some(Value::SimpleVector(TupleValue { elements: vec![] }))
                                }
                                "name" => Some(Value::Symbol(SymbolValue::new(&tv.name))),
                                "lb" => Some(runtime_julia_type_to_value(tv.lower_bound.clone())),
                                "ub" => Some(runtime_julia_type_to_value(tv.upper_bound.clone())),
                                _ => None,
                            },
                            Value::RuntimeTypeName(type_name) => match field_name {
                                "name" => Some(Value::Symbol(SymbolValue::new(&type_name.name))),
                                "wrapper" => self
                                    .runtime_type_wrapper(&type_name.identity)
                                    .map(|wrapper| Value::DataType(Box::new(wrapper))),
                                _ => None,
                            },
                            // Rust-backed Base.Pairs values still expose Julia's
                            // physical `(data, itr)` layout through getfield. The
                            // current native carrier is only produced for
                            // pairs(::NamedTuple), whose iterator field is
                            // `nothing` upstream (Issue #11380).
                            Value::Pairs(pairs) => match field_name {
                                "data" => Some(Value::NamedTuple(pairs.data.clone())),
                                "itr" => Some(Value::Nothing),
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
                                    // Issue #10212: FieldError, matching upstream 1.12.
                                    return Err(VmError::FieldError {
                                        type_name: "Base.RefValue".to_string(),
                                        field: field_name.to_string(),
                                    });
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
                                // Issue #10212: nonexistent field (DataType, TypeVar,
                                // Core.TypeName, NamedTuple, ... fall through to this
                                // shared arm) -> FieldError with the receiver's
                                // Julia-visible type name, matching upstream 1.12.
                                return Err(VmError::FieldError {
                                    type_name: crate::vm::util::value_type_name(&obj_val)
                                        .to_string(),
                                    field: field_name.to_string(),
                                });
                            }
                        }
                    }
                    Value::I64(i) => {
                        // Access by integer index (1-based)
                        let index = *i as usize;
                        if index == 0 {
                            // Issue #11509 (transactional fix): park the
                            // receiver only atomically with this raise via
                            // `field_index_out_of_bounds_with_receiver`, not
                            // ahead of the lookup below — see that helper's
                            // doc comment for why (a successful getfield must
                            // never leave a stale receiver parked for a
                            // later, unrelated raise).
                            return Err(self.field_index_out_of_bounds_with_receiver(0, 0, obj_val));
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
                            Value::QuoteNode(inner) if field_idx == 0 => Some((**inner).clone()),
                            Value::QuoteNode(_) => None,
                            Value::LineNumberNode(line) => {
                                line_number_node_field_by_index(line, field_idx)
                            }
                            Value::GlobalRef(global_ref) => {
                                global_ref_field_by_index(global_ref, field_idx)
                            }
                            // Issue #10067: NoField (index outside 0..=4)
                            // falls through to None -> the shared
                            // FieldIndexOutOfBounds/BoundsError path below,
                            // matching pre-existing behavior for truly
                            // out-of-range indices; Undef is a distinct
                            // UndefRefError, not a bounds error.
                            Value::Binding(binding) => match binding.field_by_index(field_idx) {
                                BindingFieldAccess::Value(value) => Some(value),
                                BindingFieldAccess::Undef => return Err(VmError::UndefRefError),
                                BindingFieldAccess::NoField => None,
                            },
                            Value::Generator(generator) => {
                                self.generator_projected_field_by_index(generator, field_idx)?
                            }
                            // Issue #11382: shared `RegexMatchValue::field_by_index`
                            // authority — the FieldIndexOutOfBounds fallback below
                            // handles `None` (out-of-range index).
                            Value::RegexMatch(m) => m.field_by_index(field_idx)?,
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
                                    Value::RegexMatch(_) => 5,
                                    Value::Expr(_) => 2,
                                    Value::QuoteNode(_) => 1,
                                    Value::LineNumberNode(_) => 2,
                                    Value::GlobalRef(_) => 3,
                                    Value::Binding(_) => 5,
                                    _ => 0,
                                };
                                // Issue #11509: report the caller's original
                                // 1-based `index`, not the internal 0-based
                                // `field_idx`. Parked atomically with this
                                // raise (see comment above the index==0 arm).
                                return Err(self.field_index_out_of_bounds_with_receiver(
                                    index,
                                    field_count,
                                    obj_val,
                                ));
                            }
                        }
                    }
                    Value::I32(i) => {
                        // Handle I32 index as well
                        let index = *i as usize;
                        if index == 0 {
                            // Issue #11509 (transactional fix): park the
                            // receiver only atomically with this raise (see
                            // comment on the I64 branch's index==0 arm above).
                            return Err(self.field_index_out_of_bounds_with_receiver(0, 0, obj_val));
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
                            Value::QuoteNode(inner) if field_idx == 0 => Some((**inner).clone()),
                            Value::QuoteNode(_) => None,
                            Value::LineNumberNode(line) => {
                                line_number_node_field_by_index(line, field_idx)
                            }
                            Value::GlobalRef(global_ref) => {
                                global_ref_field_by_index(global_ref, field_idx)
                            }
                            // Issue #10067: NoField (index outside 0..=4)
                            // falls through to None -> the shared
                            // FieldIndexOutOfBounds/BoundsError path below,
                            // matching pre-existing behavior for truly
                            // out-of-range indices; Undef is a distinct
                            // UndefRefError, not a bounds error.
                            Value::Binding(binding) => match binding.field_by_index(field_idx) {
                                BindingFieldAccess::Value(value) => Some(value),
                                BindingFieldAccess::Undef => return Err(VmError::UndefRefError),
                                BindingFieldAccess::NoField => None,
                            },
                            Value::Generator(generator) => {
                                self.generator_projected_field_by_index(generator, field_idx)?
                            }
                            // Issue #11382: shared `RegexMatchValue::field_by_index`
                            // authority — the FieldIndexOutOfBounds fallback below
                            // handles `None` (out-of-range index).
                            Value::RegexMatch(m) => m.field_by_index(field_idx)?,
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
                                    Value::RegexMatch(_) => 5,
                                    Value::Expr(_) => 2,
                                    Value::QuoteNode(_) => 1,
                                    Value::LineNumberNode(_) => 2,
                                    Value::GlobalRef(_) => 3,
                                    Value::Binding(_) => 5,
                                    _ => 0,
                                };
                                // Issue #11509: report the caller's original
                                // 1-based `index`, not the internal 0-based
                                // `field_idx`. Parked atomically with this
                                // raise (see comment above the index==0 arm).
                                return Err(self.field_index_out_of_bounds_with_receiver(
                                    index,
                                    field_count,
                                    obj_val,
                                ));
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
                                        // Issue #10212: setfield! on a nonexistent
                                        // field is also a FieldError upstream.
                                        VmError::FieldError {
                                            type_name: def.name.to_string(),
                                            field: field_name.to_string(),
                                        }
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
                let candidate_indices =
                    self.reflection_callable_candidate_indices(&func_val, &func_name)?;

                let has_match = self
                    .find_matching_methods_for_candidates(
                        &func_name,
                        Some(&arg_types),
                        candidate_indices.as_deref(),
                    )
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
                let candidate_indices =
                    self.reflection_callable_candidate_indices(&func_val, &func_name)?;

                match self.find_matching_methods_for_candidates(
                    &func_name,
                    Some(&arg_types),
                    candidate_indices.as_deref(),
                ) {
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
                let candidate_indices =
                    self.reflection_callable_candidate_indices(&func_val, &func_name)?;

                let methods = self.find_matching_methods_for_candidates(
                    &func_name,
                    arg_types.as_deref(),
                    candidate_indices.as_deref(),
                );
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

            BuiltinId::_ModuleName => {
                // _module_name(m::Module) -> Symbol
                // Backs Pure Julia nameof(::Module) (Issue #11171). A module
                // value's `name` field carries any enclosing user-module
                // qualification (`"P.S"` for a nested `module S` defined
                // inside `module P`), mirroring how `names(m::Module)` above
                // recovers the module's own unqualified binding name;
                // `Main`/`Base`/stdlib roots are already bare.
                if argc != 1 {
                    return Err(VmError::TypeError(format!(
                        "_module_name: expected 1 argument, got {}",
                        argc
                    )));
                }
                let module = self.stack.pop_value()?;
                let Value::Module(m) = &module else {
                    return Err(VmError::TypeError(format!(
                        "_module_name: expected Module, got {}",
                        super::util::value_type_name(&module)
                    )));
                };
                let self_name = m.name.rsplit('.').next().unwrap_or(&m.name).to_string();
                self.stack.push(Value::Symbol(SymbolValue::new(self_name)));
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
                        let defined = self.module_binding_is_defined(m, s.as_str());
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

            BuiltinId::IsdefinedBindingField => {
                // _isdefined_binding_field(b::Core.Binding, s::Symbol) -> Bool
                // _isdefined_binding_field(b::Core.Binding, i::Integer) -> Bool
                // Backs function-form isdefined(::Core.Binding, ::Symbol) and
                // isdefined(::Core.Binding, ::Integer) (Issue #10067): reuses
                // the same field classification as getfield
                // (`BindingValue::field_by_name`/`field_by_index`) so the two
                // cannot drift — a field is "defined" iff it resolves to a
                // concrete value (`:globalref`, `:flags`), not merely a known
                // name/in-range index. Upstream returns `false` (not an
                // error) for an out-of-range integer index.
                let key = self.stack.pop_value()?;
                let binding = self.stack.pop_value()?;
                match (&binding, &key) {
                    (Value::Binding(b), Value::Symbol(s)) => {
                        self.stack.push(Value::Bool(b.is_field_defined(s.as_str())));
                    }
                    (Value::Binding(b), Value::I64(i)) => {
                        let defined = *i >= 1 && b.is_field_defined_by_index((*i - 1) as usize);
                        self.stack.push(Value::Bool(defined));
                    }
                    (Value::Binding(b), Value::I32(i)) => {
                        let defined = *i >= 1 && b.is_field_defined_by_index((*i - 1) as usize);
                        self.stack.push(Value::Bool(defined));
                    }
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "_isdefined_binding_field: expected (Core.Binding, Symbol/Integer), got ({}, {})",
                            super::util::value_type_name(&binding),
                            super::util::value_type_name(&key)
                        )));
                    }
                }
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }

    /// Project a `UnionAll.var` TypeVar as a fresh runtime TypeVar.
    ///
    /// This always allocates inside the caller's owner-scoped projection
    /// domain. Constructed parametric arguments already carry their identity in
    /// `JuliaType::RuntimeTypeVar`, so no name-keyed bridge is needed (Issues
    /// #10049/#10252).
    pub(crate) fn fresh_runtime_typevar_value_for_projection(&mut self, jt: JuliaType) -> Value {
        let JuliaType::TypeVar(name, bounds) = jt else {
            return Value::DataType(Box::new(jt));
        };
        let id = self.runtime_typevar_counter;
        self.runtime_typevar_counter += 1;
        let (lower_bound, upper_bound) = reflected_typevar_bounds(&name, bounds.as_deref());
        Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
            id,
            name,
            lower_bound,
            upper_bound,
        }))
    }

    /// Project a `UnionAll` TypeVar with identity scoped to one owner wrapper.
    ///
    /// This keeps upstream reflection identity within a wrapper chain, e.g.
    /// `Vector.var === Vector.body.parameters[1]`, while avoiding the old
    /// `(name, upper)` cache leak between unrelated wrappers that reuse TypeVar
    /// names (Issue #10252).
    pub(crate) fn runtime_typevar_value_for_unionall_projection(
        &mut self,
        owner: &JuliaType,
        jt: JuliaType,
    ) -> Value {
        let owner_key = self.runtime_typevar_projection_owner_key(owner);
        let registry =
            RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
        let mut current = owner.clone();
        let mut declared_chain = Vec::new();
        let mut visited = Vec::new();
        loop {
            if visited.contains(&current) {
                break;
            }
            visited.push(current.clone());
            let object = registry.object(&current);
            let (Some(var), Some(body)) = (object.unionall_var(), object.unionall_body()) else {
                break;
            };
            declared_chain.push(var);
            if body == current {
                break;
            }
            current = body;
        }
        // Key binders by their depth from the final body, rather than by an
        // index relative to the wrapper view passed by the caller. The suffix
        // `outer.body` therefore assigns its first binder the same depth that
        // the full `outer` chain assigns it, even when that suffix is reflected
        // before the outer binder (Issue #10261).
        let requested_depth = declared_chain
            .iter()
            .position(|declared| declared == &jt)
            .map(|position| declared_chain.len() - position - 1)
            .unwrap_or(0);
        // Allocate the complete owner chain outer-to-inner before answering a
        // single projection request. A caller may request `C` first through a
        // synthesized owner; eager allocation still makes `C.ub` reference the
        // same cached `B`, and `B.ub` the same `A` (Issue #10261).
        let chain_len = declared_chain.len();
        for (position, declared) in declared_chain.into_iter().enumerate() {
            if matches!(declared, JuliaType::TypeVar(..)) {
                self.runtime_typevar_value_for_projection_key(
                    owner_key.clone(),
                    chain_len - position - 1,
                    declared,
                );
            }
        }
        self.runtime_typevar_value_for_projection_key(owner_key, requested_depth, jt)
    }

    fn runtime_typevar_value_for_projection_key(
        &mut self,
        owner_key: CoreType,
        binder_depth: usize,
        jt: JuliaType,
    ) -> Value {
        let JuliaType::TypeVar(name, upper) = jt else {
            return Value::DataType(Box::new(jt));
        };
        let (declared_lower, declared_upper) =
            declared_projection_bounds_key(&name, upper.as_deref());
        let key = TypeVarProjectionKey {
            owner: owner_key,
            binder_depth,
            declared_lower,
            declared_upper,
        };
        if let Some(tv) = self
            .runtime_typevar_projection_identities
            .get(&key)
            .cloned()
        {
            return Value::RuntimeTypeVar(Box::new(tv));
        }
        let Value::RuntimeTypeVar(tv) =
            self.fresh_runtime_typevar_value_for_projection(JuliaType::TypeVar(name, upper))
        else {
            unreachable!("TypeVar input must project to RuntimeTypeVar")
        };
        let mut tv = *tv;
        tv.lower_bound =
            self.rebind_owner_projection_bound(&key.owner, binder_depth, tv.lower_bound);
        tv.upper_bound =
            self.rebind_owner_projection_bound(&key.owner, binder_depth, tv.upper_bound);
        self.runtime_typevar_projection_identities
            .insert(key, tv.clone());
        Value::RuntimeTypeVar(Box::new(tv))
    }

    pub(crate) fn project_unionall_body_with_identity(
        &mut self,
        owner: &JuliaType,
        body: JuliaType,
        var: JuliaType,
    ) -> JuliaType {
        let JuliaType::TypeVar(name, _) = &var else {
            return body;
        };
        let name = name.clone();
        let projected = self.runtime_typevar_value_for_unionall_projection(owner, var);
        let Value::RuntimeTypeVar(projected) = projected else {
            return body;
        };
        if let JuliaType::RuntimeUnionAll {
            body: promoted_body,
            ..
        } = self.project_unionall_binders_for_owner(owner, owner)
        {
            return crate::vm::type_objects::unwrap_array_alias_body(*promoted_body);
        }
        crate::vm::type_objects::unwrap_array_alias_body(
            body.substitute(&name, &projected.projection()),
        )
    }

    pub(crate) fn runtime_typevar_projection_owner_key(&self, owner: &JuliaType) -> CoreType {
        fn normalize_owner_typevars(ty: CoreType, projected_ids: &HashSet<u64>) -> CoreType {
            match ty {
                CoreType::TypeVar(var)
                    if !var.is_rigid()
                        || var
                            .rigid_identity
                            .is_some_and(|id| projected_ids.contains(&id)) =>
                {
                    CoreType::Named(var.name)
                }
                CoreType::TypeVar(mut var) => {
                    var.lower_bound = var
                        .lower_bound
                        .map(|bound| Box::new(normalize_owner_typevars(*bound, projected_ids)));
                    var.upper_bound = var
                        .upper_bound
                        .map(|bound| Box::new(normalize_owner_typevars(*bound, projected_ids)));
                    CoreType::TypeVar(var)
                }
                CoreType::Struct { name, params } => CoreType::Struct {
                    name,
                    params: params
                        .into_iter()
                        .map(|ty| normalize_owner_typevars(ty, projected_ids))
                        .collect(),
                },
                CoreType::Tuple(types) => CoreType::Tuple(
                    types
                        .into_iter()
                        .map(|ty| normalize_owner_typevars(ty, projected_ids))
                        .collect(),
                ),
                CoreType::Vararg(inner) => {
                    CoreType::Vararg(Box::new(normalize_owner_typevars(*inner, projected_ids)))
                }
                CoreType::VarargLen { element, len } => CoreType::VarargLen {
                    element: Box::new(normalize_owner_typevars(*element, projected_ids)),
                    len: Box::new(normalize_owner_typevars(*len, projected_ids)),
                },
                CoreType::NamedTuple(fields) => CoreType::NamedTuple(
                    fields
                        .into_iter()
                        .map(|(name, ty)| (name, normalize_owner_typevars(ty, projected_ids)))
                        .collect(),
                ),
                CoreType::Union(types) => CoreType::Union(
                    types
                        .into_iter()
                        .map(|ty| normalize_owner_typevars(ty, projected_ids))
                        .collect(),
                ),
                CoreType::TypeOf(inner) => {
                    CoreType::TypeOf(Box::new(normalize_owner_typevars(*inner, projected_ids)))
                }
                // Preserve NESTED UnionAll binders structurally (Issue
                // #10987): only the top-level wrapper chain is unwrapped (by
                // the caller's walk, before this normalization runs). A
                // binder that survives to here sits in a parameter position
                // of the final body, and its BOUND may hold the only
                // occurrence of an outer binder's name
                // (`Tuple{Vector{S} where S<:T} where T`) -- stripping to
                // the body erased that occurrence and collapsed
                // distinct-upstream wrappers into one owner domain. The var
                // is rebuilt through `CoreTypeVar::with_bounds` so
                // parse-allocated `scope_id`s never leak into key equality.
                CoreType::UnionAll { var, body } => CoreType::UnionAll {
                    var: CoreTypeVar::with_bounds(
                        var.name,
                        var.lower_bound
                            .map(|bound| Box::new(normalize_owner_typevars(*bound, projected_ids))),
                        var.upper_bound
                            .map(|bound| Box::new(normalize_owner_typevars(*bound, projected_ids))),
                    ),
                    body: Box::new(normalize_owner_typevars(*body, projected_ids)),
                },
                other => other,
            }
        }

        let registry =
            RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
        let mut current = owner.clone();
        let mut visited = Vec::new();
        loop {
            if visited.contains(&current) {
                break;
            }
            visited.push(current.clone());
            let next = match &current {
                JuliaType::UnionAll { body, .. } | JuliaType::RuntimeUnionAll { body, .. } => {
                    body.as_ref().clone()
                }
                _ => match registry.object(&current).unionall_body() {
                    Some(body) => body,
                    None => break,
                },
            };
            if next == current {
                break;
            }
            current = next;
        }
        let projected_ids: HashSet<u64> = self
            .runtime_typevar_projection_identities
            .values()
            .map(|typevar| typevar.id)
            .collect();
        normalize_owner_typevars(
            CoreType::from_julia_type_preserving_owner(&current),
            &projected_ids,
        )
    }

    fn runtime_typevar_projection_identity_for_owner_binder(
        &self,
        owner: &CoreType,
        binder_depth: usize,
        lower_bound: Option<&str>,
        upper_bound: Option<&str>,
    ) -> Option<RuntimeTypeVarValue> {
        let (declared_lower, declared_upper) =
            declared_projection_bounds_key_from_parts(lower_bound, upper_bound);
        self.runtime_typevar_projection_identities
            .get(&TypeVarProjectionKey {
                owner: owner.clone(),
                binder_depth,
                declared_lower,
                declared_upper,
            })
            .cloned()
    }

    /// Find the unique cached projection for `owner` whose *display* name and
    /// as-declared bounds match, when the caller only has a rendered
    /// reflection parameter (not a binder depth) to search by -- e.g. a bare
    /// `TypeVar(name, upper)` `.parameters` entry. The map's own key is the
    /// structural [`TypeVarProjectionKey`] (Issue #10987): the bounds filter
    /// compares the PARSED bounds against the key's structural
    /// `declared_lower`/`declared_upper` (never a rendered-string compare,
    /// and never the stored value's bounds -- those are rebound to id-bearing
    /// TypeVars at insertion time by `rebind_owner_projection_bound`), while
    /// `name` is read off each candidate's stored `RuntimeTypeVarValue.name`
    /// display metadata. Shadowed same-name binders at different depths
    /// remain ambiguous (both match) and correctly fall through to the
    /// caller's slower depth-computing recomputation.
    fn unique_runtime_typevar_projection_for_owner_parameter(
        &self,
        owner: &JuliaType,
        name: &str,
        upper: &Option<String>,
    ) -> Option<RuntimeTypeVarValue> {
        let owner = self.runtime_typevar_projection_owner_key(owner);
        let (declared_lower, declared_upper) =
            declared_projection_bounds_key(name, upper.as_deref());
        let mut matches = self
            .runtime_typevar_projection_identities
            .iter()
            .filter(|(key, typevar)| {
                key.owner == owner
                    && key.declared_lower == declared_lower
                    && key.declared_upper == declared_upper
                    && typevar.name == name
            })
            .map(|(_, typevar)| typevar);
        let first = matches.next()?;
        matches.next().is_none().then(|| first.clone())
    }

    pub(crate) fn project_unionall_binders_for_owner(
        &self,
        owner: &JuliaType,
        ty: &JuliaType,
    ) -> JuliaType {
        let materialized = if matches!(
            ty,
            JuliaType::UnionAll { .. } | JuliaType::RuntimeUnionAll { .. }
        ) {
            ty.clone()
        } else {
            let registry =
                RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
            let mut current = ty.clone();
            let mut binders = Vec::new();
            let mut visited = Vec::new();
            loop {
                if visited.contains(&current) {
                    break;
                }
                visited.push(current.clone());
                let object = registry.object(&current);
                let (Some(var), Some(body)) = (object.unionall_var(), object.unionall_body())
                else {
                    break;
                };
                let JuliaType::TypeVar(name, bounds) = var else {
                    break;
                };
                let (lower, upper) = reflected_typevar_bounds(&name, bounds.as_deref());
                binders.push((
                    name,
                    (!matches!(lower, JuliaType::Bottom))
                        .then(|| Box::new(lower.name().into_owned())),
                    (!matches!(upper, JuliaType::Any)).then(|| Box::new(upper.name().into_owned())),
                ));
                if body == current {
                    break;
                }
                current = body;
            }
            binders
                .into_iter()
                .rev()
                .fold(current, |body, (var, lower_bound, bound)| {
                    JuliaType::UnionAll {
                        var,
                        lower_bound,
                        bound,
                        body: Box::new(body),
                    }
                })
        };
        fn explicit_binder_count(ty: &JuliaType) -> usize {
            match ty {
                JuliaType::UnionAll { body, .. } | JuliaType::RuntimeUnionAll { body, .. } => {
                    1 + explicit_binder_count(body)
                }
                _ => 0,
            }
        }

        let owner_key = self.runtime_typevar_projection_owner_key(owner);
        let binder_depth = explicit_binder_count(&materialized).checked_sub(1);
        self.project_unionall_binders_for_owner_at(&owner_key, &materialized, binder_depth)
    }

    /// Materialize a partial application whose remaining binder bounds depend
    /// on an already-applied parameter. Upstream allocates fresh trailing
    /// TypeVars for each such application (`Q{Int}.var !== Q{Int}.var` when
    /// `Q{T,N<:T}`), while constant trailing binders remain declaration-shared.
    pub(crate) fn project_unionall_binders_with_fresh_owner(
        &mut self,
        ty: &JuliaType,
        applied_substitutions: &[(String, JuliaType)],
    ) -> JuliaType {
        fn explicit_binder_count(ty: &JuliaType) -> usize {
            match ty {
                JuliaType::UnionAll { body, .. } => 1 + explicit_binder_count(body),
                _ => 0,
            }
        }

        let discriminator = self.runtime_typevar_counter;
        self.runtime_typevar_counter += 1;
        let owner = CoreType::Named(format!("#partial-unionall-{discriminator}"));
        let mut current = ty;
        let mut depth = explicit_binder_count(ty).checked_sub(1);
        while let JuliaType::UnionAll {
            var,
            lower_bound,
            bound,
            body,
        } = current
        {
            let current_depth = depth.unwrap_or(0);
            let bounds = match (lower_bound.as_deref(), bound.as_deref()) {
                (Some(lower), Some(upper)) => Some(format!("{lower}<:{var}<:{upper}")),
                (Some(lower), None) => Some(format!(">:{lower}")),
                (None, upper) => upper.cloned(),
            };
            let (declared_lower, declared_upper) =
                declared_projection_bounds_key(var, bounds.as_deref());
            let key = TypeVarProjectionKey {
                owner: owner.clone(),
                binder_depth: current_depth,
                declared_lower,
                declared_upper,
            };
            let Value::RuntimeTypeVar(projected) = self.fresh_runtime_typevar_value_for_projection(
                JuliaType::TypeVar(var.clone(), bounds),
            ) else {
                unreachable!("source UnionAll binder must project to RuntimeTypeVar")
            };
            let mut projected = *projected;
            for (name, replacement) in applied_substitutions {
                projected.lower_bound = projected.lower_bound.substitute(name, replacement);
                projected.upper_bound = projected.upper_bound.substitute(name, replacement);
                if let JuliaType::RuntimeTypeVar {
                    name: replacement_name,
                    ..
                } = replacement
                {
                    projected.lower_bound = projected
                        .lower_bound
                        .substitute(replacement_name, replacement);
                    projected.upper_bound = projected
                        .upper_bound
                        .substitute(replacement_name, replacement);
                }
            }
            projected.lower_bound = self.rebind_owner_projection_bound(
                &key.owner,
                current_depth,
                projected.lower_bound,
            );
            projected.upper_bound = self.rebind_owner_projection_bound(
                &key.owner,
                current_depth,
                projected.upper_bound,
            );
            self.runtime_typevar_projection_identities
                .insert(key, projected);
            current = body;
            depth = current_depth.checked_sub(1);
        }
        self.project_unionall_binders_for_owner_at(
            &owner,
            ty,
            explicit_binder_count(ty).checked_sub(1),
        )
    }

    fn project_unionall_binders_for_owner_at(
        &self,
        owner: &CoreType,
        ty: &JuliaType,
        binder_depth: Option<usize>,
    ) -> JuliaType {
        match ty {
            JuliaType::UnionAll {
                var,
                lower_bound,
                bound,
                body,
            } => {
                let depth = binder_depth.unwrap_or(0);
                let next_depth = depth.checked_sub(1);
                match self.runtime_typevar_projection_identity_for_owner_binder(
                    owner,
                    depth,
                    lower_bound.as_deref().map(String::as_str),
                    bound.as_deref().map(String::as_str),
                ) {
                    Some(projected) => {
                        let projected_var = projected.projection();
                        let body = substitute_projected_unionall_body(body, var, &projected_var);
                        JuliaType::RuntimeUnionAll {
                            var: Box::new(projected.projection()),
                            body: Box::new(
                                self.project_unionall_binders_for_owner_at(
                                    owner, &body, next_depth,
                                ),
                            ),
                        }
                    }
                    None => JuliaType::UnionAll {
                        var: var.clone(),
                        lower_bound: lower_bound.clone(),
                        bound: bound.clone(),
                        body: Box::new(
                            self.project_unionall_binders_for_owner_at(owner, body, next_depth),
                        ),
                    },
                }
            }
            JuliaType::RuntimeUnionAll { var, body } => {
                let next_depth = binder_depth.and_then(|depth| depth.checked_sub(1));
                JuliaType::RuntimeUnionAll {
                    var: var.clone(),
                    body: Box::new(
                        self.project_unionall_binders_for_owner_at(owner, body, next_depth),
                    ),
                }
            }
            _ => ty.clone(),
        }
    }

    /// Rebind legacy name-only references inside one projected TypeVar bound
    /// to the id-bearing TypeVar already allocated in the same owner domain.
    /// A shadowed name resolves to the lexically nearest outer binder: with
    /// depth measured from the final body, that is the smallest candidate
    /// depth greater than the current binder's depth (Issue #10261).
    fn rebind_owner_projection_bound(
        &self,
        owner: &CoreType,
        binder_depth: usize,
        mut bound: JuliaType,
    ) -> JuliaType {
        let mut candidates: HashMap<String, (usize, &RuntimeTypeVarValue)> = HashMap::new();
        for (key, typevar) in &self.runtime_typevar_projection_identities {
            if &key.owner != owner || key.binder_depth <= binder_depth {
                continue;
            }
            candidates
                .entry(typevar.name.clone())
                .and_modify(|candidate| {
                    if key.binder_depth < candidate.0 {
                        *candidate = (key.binder_depth, typevar);
                    }
                })
                .or_insert((key.binder_depth, typevar));
        }
        for (name, (_, typevar)) in candidates {
            bound = bound.substitute(&name, &typevar.projection());
        }
        bound
    }

    /// Map a single `.parameters` reflection entry to its `Value`, matching
    /// upstream Julia (Issue #5162). Type parameters become a `DataType` (or an
    /// id-bearing `RuntimeTypeVar` when the structured type parameter carries
    /// one, so `Vector{T}.parameters[1] === T` holds -- Issue #4698); integer/value parameters become the concrete value they denote
    /// (`Array{T,N}.parameters == svec(T, N)`, `Val{5}.parameters == svec(5)`).
    pub(crate) fn reflection_parameter_to_value(&self, param: ReflectionParameter) -> Value {
        match param {
            ReflectionParameter::Type(JuliaType::RuntimeTypeVar {
                id,
                name,
                lower_bound,
                upper_bound,
            }) => Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
                id,
                name,
                lower_bound: *lower_bound,
                upper_bound: *upper_bound,
            })),
            ReflectionParameter::Type(JuliaType::TypeVar(name, upper)) => {
                Value::DataType(Box::new(JuliaType::TypeVar(name, upper)))
            }
            ReflectionParameter::Type(JuliaType::Struct(name)) => name
                .parse::<i64>()
                .map(Value::I64)
                .unwrap_or(Value::DataType(Box::new(JuliaType::Struct(name)))),
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
            ReflectionParameter::Str(s) => Value::str_new(s),
        }
    }

    pub(crate) fn reflection_parameter_to_value_for_owner(
        &mut self,
        owner: &JuliaType,
        param: ReflectionParameter,
    ) -> Value {
        match param {
            ReflectionParameter::Type(JuliaType::RuntimeTypeVar {
                id,
                name,
                lower_bound,
                upper_bound,
            }) => Value::RuntimeTypeVar(Box::new(RuntimeTypeVarValue {
                id,
                name,
                lower_bound: *lower_bound,
                upper_bound: *upper_bound,
            })),
            ReflectionParameter::Type(JuliaType::TypeVar(name, upper)) => {
                if let Some(tv) =
                    self.unique_runtime_typevar_projection_for_owner_parameter(owner, &name, &upper)
                {
                    return Value::RuntimeTypeVar(Box::new(tv));
                }
                self.runtime_typevar_value_for_unionall_projection(
                    owner,
                    JuliaType::TypeVar(name, upper),
                )
            }
            ReflectionParameter::Type(JuliaType::Struct(name)) => {
                if let Ok(value) = name.parse::<i64>() {
                    return Value::I64(value);
                }
                if let Some(tv) =
                    self.unique_runtime_typevar_projection_for_owner_parameter(owner, &name, &None)
                {
                    return Value::RuntimeTypeVar(Box::new(tv));
                }
                let owner_key = self.runtime_typevar_projection_owner_key(owner);
                if is_single_char_typevar_name(&name)
                    && owner_key
                        .to_julia_name()
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|token| token == name)
                {
                    return self.runtime_typevar_value_for_unionall_projection(
                        owner,
                        JuliaType::TypeVar(name, None),
                    );
                }
                Value::DataType(Box::new(JuliaType::Struct(name)))
            }
            other => self.reflection_parameter_to_value(other),
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
    fn module_binding_is_defined(&mut self, module: &ModuleValue, field_name: &str) -> bool {
        let module_name = module.name.as_str();
        if self.eval_struct_binding_is_pending(module_name, field_name) {
            return false;
        }
        // Macro bindings (`@name`) are erased during lowering, so they never reach
        // the global/function registry consulted below. Consult the per-module
        // macro binding table the compiler recorded instead (Issue #7948).
        // `macro_bindings` is keyed by `ModuleId` (Issue #10988 Phase 2a); resolve
        // the module-name string through `module_registry` first. An unregistered
        // module name simply has no macro bindings (`None` short-circuits below),
        // matching the pre-#10988 `HashMap::get` miss behavior.
        if field_name.starts_with('@')
            && self
                .module_registry
                .lookup(module_name)
                .and_then(|module_id| self.macro_bindings.get(&module_id))
                .is_some_and(|names| names.contains(field_name))
        {
            return true;
        }

        // Values and functions resolvable via the existing getfield(::Module)
        // path. Main needs the owner-aware definition lookup: the runtime
        // function index intentionally contains short aliases for every module
        // method, and the broader getfield path may use those aliases for
        // dispatch. A short alias alone is not a binding in Main (#11749).
        let runtime_binding_is_defined =
            if crate::module_names::classify_builtin_module(module_name)
                == crate::module_names::BuiltinModule::Main
            {
                self.get_global(field_name).is_some()
                    || self.get_global_definition_value(field_name).is_some()
                    || self
                        .resolve_live_import_binding(&format!("Main.{field_name}"))
                        .is_some()
            } else {
                self.get_module_binding(module_name, field_name).is_some()
            };
        if runtime_binding_is_defined {
            return true;
        }

        // Base/Core pure-Julia functions are stored under their unqualified
        // name in the function registry. Ordinary modules see only Base's
        // exports implicitly; baremodules do not unless they explicitly
        // `using Base`. Prefer declaration metadata from the compile context:
        // a nested module value may have been reconstructed from its registry
        // path and therefore cannot itself retain `module` vs `baremodule`
        // provenance (Issue #11410).
        let context = self.compile_context.as_ref();
        let base_exports_visible = module_name != "Core"
            && context
                .and_then(|ctx| ctx.module_base_exports_visibility.get(module_name).copied())
                .unwrap_or(module.base_exports_visible);
        let implicit_standard_bindings = context
            .and_then(|ctx| {
                ctx.module_implicit_standard_bindings
                    .get(module_name)
                    .copied()
            })
            .unwrap_or(module.implicit_standard_bindings);
        let base_exported_name =
            context.is_some_and(|ctx| ctx.base_exported_names.contains(field_name));
        if matches!(field_name, "eval" | "include")
            && (matches!(module_name, "Core" | "Base") || implicit_standard_bindings)
        {
            return true;
        }
        // Type names precede BuiltinId lookup because several constructor
        // builtins share source spellings with type bindings (`Int`, `Int64`,
        // `TypeVar`, ...). The canonical type registry owns their upstream
        // Core/Base namespace authority (Issue #11410).
        if let Some(authority) = builtin_type_binding_authority(field_name) {
            return match authority {
                BuiltinTypeBindingAuthority::Core => true,
                BuiltinTypeBindingAuthority::Base => {
                    module_name == "Base"
                        || (base_exports_visible && (base_exported_name || context.is_none()))
                }
            };
        }

        // Core exports are visible in every source-declared module, including
        // `baremodule`. This registry also contains Core names that have no
        // BuiltinId implementation alias (`applicable`, `fieldtype`, `throw`,
        // ...), so it must be consulted independently and before BuiltinId.
        if subset_julia_vm_types::inference_core::type_core::is_core_builtin_function_name(
            field_name,
        ) {
            return true;
        }
        if matches!(module_name, "Core" | "Base")
            && subset_julia_vm_types::inference_core::type_core::is_core_function_binding_name(
                field_name,
            )
        {
            return true;
        }

        let has_base_origin_method = self
            .get_function_indices_by_name(field_name)
            .iter()
            .any(|index| *index < self.base_function_count);
        if has_base_origin_method
            && (module_name == "Base"
                || (base_exports_visible && (base_exported_name || context.is_none())))
        {
            return true;
        }

        // A VM-executable BuiltinId alias is not automatically a Julia module
        // binding. Core-owned but non-exported names are qualified-only;
        // Base-private names exist only in Base; remaining aliases are visible
        // only if the bundled Base export table says so. VM-internal aliases
        // therefore stay undefined instead of leaking through Base (#11410).
        if BuiltinId::from_name(field_name).is_some() {
            return match BuiltinId::binding_authority(field_name) {
                Some(BuiltinBindingAuthority::Core) => {
                    matches!(module_name, "Core" | "Base")
                        || (base_exports_visible && base_exported_name)
                }
                Some(BuiltinBindingAuthority::BasePrivate) => module_name == "Base",
                None => (module_name == "Base" || base_exports_visible) && base_exported_name,
            };
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
        let top_level_lookup = util::is_top_level_module_binding_scope(module_name);
        if self
            .struct_defs
            .iter()
            .any(|d| d.name == qualified_name || (top_level_lookup && d.name == field_name))
            || self
                .abstract_types
                .iter()
                .any(|d| d.name == qualified_name || (top_level_lookup && d.name == field_name))
            || self
                .active_enum_name_index
                .keys()
                .any(|name| name == &qualified_name || (top_level_lookup && name == field_name))
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

        false
    }

    pub(in crate::vm) fn get_module_binding(
        &mut self,
        module_name: &str,
        field_name: &str,
    ) -> Option<Value> {
        // A concrete type's compiler-generated default constructors already
        // occupy the function table before its source declaration executes.
        // They are methods of the future type binding, not an independently
        // visible generic function. Keep the entire binding hidden while its
        // definition is reserved; `DefineEvalStruct` removes it from this queue
        // and the ordinary active struct/function paths take over (#11546).
        if self.eval_struct_binding_is_pending(module_name, field_name) {
            return None;
        }

        if module_name == "Sys" && field_name == "WORD_SIZE" {
            return Some(Value::I64(i64::from(usize::BITS)));
        }
        let qualified_name = format!("{}.{}", module_name, field_name);
        let top_level_lookup = util::is_top_level_module_binding_scope(module_name);

        // Imported names are live aliases of the source module's binding.
        // Follow the compiler-recorded binding chain before consulting any
        // stale assignment/global snapshot in the destination module
        // (`import P as Q`, `import P: x as y`, Issue #11176).
        if let Some(target) = self.resolve_live_import_binding(&qualified_name) {
            if self.module_registry.lookup(&target).is_some() {
                return Some(Value::Module(Box::new(ModuleValue::new(target))));
            }
            if let Some((owner, binding)) = target.rsplit_once('.') {
                return self.get_module_binding(owner, binding);
            }
        }

        if module_name == "Main" {
            if let Some(value) = self.get_global(field_name) {
                return Some(value);
            }
        }

        if let Some(value) = self.get_global(&qualified_name) {
            // Issue #10077: a plain function-singleton global bound under its
            // internal module-qualified registry key (`"Pkg9992B.transform9992b"`,
            // see `PipelineCtx`'s "`FunctionInfo.name` is *always*
            // module-qualified" note) must not leak that internal spelling as
            // the captured value's runtime type identity. Upstream Julia's
            // generic function has ONE canonical name (`nameof(f)`) regardless
            // of which module path reached it, so a module-qualified access
            // (`Module.func`) and a bare/imported access (`func`) of the SAME
            // function must produce `typeof`-identical, `isa Function`-true
            // values. Use the bare field spelling only when that binding reaches
            // the same generic; otherwise retain the qualified spelling and
            // distinct declaration owner. In either case keep resolved candidate
            // indices so calls bind to exactly this module's function.
            if let Value::Function(fv) = &value {
                if fv.name == qualified_name {
                    let candidates = fv.candidate_indices.clone().unwrap_or_else(|| {
                        self.get_function_indices_by_name(&qualified_name).to_vec()
                    });
                    return Some(Value::Function(self.module_function_value_with_candidates(
                        &qualified_name,
                        field_name,
                        candidates,
                    )));
                }
            }
            return Some(value);
        }

        // A Module value exposes its nested modules and type bindings through
        // ordinary `getfield`, just like functions and constants. This path is
        // required when the concrete module identity is available only at
        // runtime (`const MA = Mod1; MA.S`, `m::Module; m.Inner`) rather than
        // statically rewritten by the compiler (Issues #8113/#8114/#11176).
        if self.module_registry.lookup(&qualified_name).is_some() {
            return Some(Value::Module(Box::new(ModuleValue::new(qualified_name))));
        }

        if let Some(ctx) = &self.compile_context {
            if let Some(target) = ctx.type_aliases.get(&qualified_name) {
                let target = qualify_module_local_type_alias_target(
                    module_name,
                    target,
                    |qualified_target| {
                        ctx.type_aliases.contains_key(qualified_target)
                            || ctx.parametric_structs.contains_key(qualified_target)
                            || ctx
                                .primitive_types
                                .iter()
                                .any(|definition| definition.name == qualified_target)
                            || self
                                .struct_defs
                                .iter()
                                .any(|definition| definition.name == qualified_target)
                            || self
                                .abstract_types
                                .iter()
                                .any(|definition| definition.name == qualified_target)
                            || self.active_enum_name_index.contains_key(qualified_target)
                    },
                );
                let ty = self.runtime_type_wrapper(&target)?;
                return Some(Value::DataType(Box::new(ty)));
            }
        }

        if top_level_lookup && self.active_enum_name_index.contains_key(field_name) {
            return Some(Value::DataType(Box::new(JuliaType::Enum(
                field_name.to_string(),
            ))));
        }

        let is_qualified_type = self
            .struct_defs
            .iter()
            .any(|definition| definition.name == qualified_name)
            || self
                .abstract_types
                .iter()
                .any(|definition| definition.name == qualified_name)
            || self.compile_context.as_ref().is_some_and(|ctx| {
                ctx.parametric_structs.contains_key(&qualified_name)
                    || ctx
                        .primitive_types
                        .iter()
                        .any(|definition| definition.name == qualified_name)
            })
            || self.active_enum_name_index.contains_key(&qualified_name);
        if is_qualified_type {
            let ty = self.runtime_type_wrapper(&qualified_name)?;
            return Some(Value::DataType(Box::new(ty)));
        }

        let world = self.current_dispatch_world();
        let qualified_candidates: Vec<_> = self
            .get_function_indices_by_name(&qualified_name)
            .iter()
            .copied()
            .filter(|&index| self.function_visible_in_world(index, world))
            .collect();
        if !qualified_candidates.is_empty() {
            return Some(Value::Function(self.module_function_value_with_candidates(
                &qualified_name,
                field_name,
                qualified_candidates,
            )));
        }
        if module_name == "Main" {
            let candidates: Vec<_> = self
                .get_function_indices_by_name(field_name)
                .iter()
                .copied()
                .filter(|&index| self.function_visible_in_world(index, world))
                .collect();
            if !candidates.is_empty() {
                return Some(Value::Function(
                    self.function_value_with_candidates(field_name.to_string(), candidates),
                ));
            }
        }

        None
    }

    fn resolve_live_import_binding(&self, qualified_name: &str) -> Option<String> {
        let bindings = &self.compile_context.as_ref()?.module_imported_bindings;
        let mut current = qualified_name.to_string();
        let mut seen = std::collections::HashSet::new();
        while let Some(next) = bindings.get(&current) {
            if !seen.insert(current.clone()) {
                return None;
            }
            current = next.clone();
        }
        (current != qualified_name).then_some(current)
    }

    /// Julia-visible reflection observes the current world and excludes private
    /// lowering helpers even when their internal spelling matches a source
    /// generic. Keep every reflection/inference enumerator on this one gate.
    fn reflection_function_visible(&self, index: usize) -> bool {
        self.function_visible_in_world(index, self.current_dispatch_world())
            && self
                .functions
                .get(index)
                .is_some_and(|function| !function.is_lowering_helper)
    }

    /// Resolve the concrete callable family carried by a function/closure
    /// value. `None` means ordinary public name lookup; `Some` is an authority
    /// boundary that may intentionally include private lowering helpers.
    fn reflection_callable_candidate_indices(
        &self,
        value: &Value,
        func_name: &str,
    ) -> Result<Option<Vec<usize>>, VmError> {
        if !matches!(value, Value::Function(_) | Value::Closure(_)) {
            return Ok(None);
        }
        self.collect_runtime_callable_candidates(value, func_name)
            .map(|candidates| Some(candidates.into_iter().map(|(index, _)| index).collect()))
    }

    fn reflection_function_selected(
        &self,
        index: usize,
        candidate_indices: Option<&[usize]>,
    ) -> bool {
        match candidate_indices {
            Some(candidates) => {
                candidates.contains(&index)
                    && self.function_visible_in_world(index, self.current_dispatch_world())
            }
            None => self.reflection_function_visible(index),
        }
    }

    /// Find methods matching the given function name and optionally argument types.
    /// Returns None if no methods found, otherwise returns a vector of matching FunctionInfo
    /// sorted by specificity (most specific first).
    fn find_matching_methods_for_candidates(
        &self,
        func_name: &str,
        arg_types: Option<&[JuliaType]>,
        candidate_indices: Option<&[usize]>,
    ) -> Option<Vec<FunctionInfo>> {
        if let Some(types) = arg_types {
            if let Some(variants) = split_reflection_union_arg_types(types) {
                let mut split_matches: Vec<FunctionInfo> = Vec::new();
                for variant in variants {
                    if let Some(matches) = self.find_matching_methods_for_candidates(
                        func_name,
                        Some(&variant),
                        candidate_indices,
                    ) {
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

        for (index, info) in self.functions.iter().enumerate() {
            if !self.reflection_function_selected(index, candidate_indices) {
                continue;
            }
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
                .score
                    .saturating_add(
                        crate::inference_core::dispatch_resolver::typed_vararg_where_bonus_julia(
                            &info.param_julia_types,
                            &info.type_params,
                            info.vararg_param_index,
                        ),
                    );

                matches.push((info.as_ref().clone(), score));
            }
        }

        if matches.is_empty() {
            return None;
        }

        // Equal-signature redefinitions are replacement, not ambiguity. REPL
        // full rebuild may store current IR before older retained definitions,
        // so use source chronology rather than vector position (#9784).
        let mut deduplicated = Vec::with_capacity(matches.len());
        for candidate in matches {
            if let Some(position) =
                deduplicated
                    .iter()
                    .position(|(existing, _): &(FunctionInfo, u32)| {
                        same_reflection_method_signature(existing, &candidate.0)
                    })
            {
                if reflection_candidate_is_at_least_as_new(&deduplicated[position].0, &candidate.0)
                {
                    deduplicated[position] = candidate;
                }
            } else {
                deduplicated.push(candidate);
            }
        }
        let mut matches = deduplicated;

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

    /// Find methods whose bodies can be composed for reflection-time exception
    /// and effect inference. Concrete signatures must use the same method-table
    /// dispatch winner as runtime calls so Base extension methods (for example
    /// `function Base.:+(...)`) are selected ahead of generic Base fallbacks.
    fn find_composable_methods(
        &self,
        func_name: &str,
        arg_types: &[JuliaType],
    ) -> Vec<(usize, FunctionInfo)> {
        self.find_composable_methods_for_candidates(func_name, arg_types, None)
    }

    fn find_composable_methods_for_candidates(
        &self,
        func_name: &str,
        arg_types: &[JuliaType],
        candidate_indices: Option<&[usize]>,
    ) -> Vec<(usize, FunctionInfo)> {
        if arg_types.iter().all(JuliaType::is_concrete) {
            let table = self.reflection_method_table(func_name, candidate_indices);
            return match table.dispatch(arg_types) {
                Ok(method) => self
                    .functions
                    .get(method.global_index)
                    .map(|info| vec![(method.global_index, info.as_ref().clone())])
                    .unwrap_or_default(),
                Err(
                    DispatchError::NoMethodFound { .. } | DispatchError::AmbiguousMethod { .. },
                ) => Vec::new(),
            };
        }

        self.find_matching_methods_for_candidates(func_name, Some(arg_types), candidate_indices)
            .map(|infos| {
                infos
                    .into_iter()
                    .filter_map(|info| {
                        self.functions
                            .iter()
                            .enumerate()
                            .rev()
                            .find(|(index, candidate)| {
                                self.reflection_function_selected(*index, candidate_indices)
                                    && same_reflection_method_signature(candidate, &info)
                            })
                            .map(|(index, _)| (index, info))
                    })
                    .collect()
            })
            .unwrap_or_default()
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
            .or_else(|| self.bytecode_direct_call_return_julia_type(info))
            .or_else(|| {
                arg_types.and_then(|types| {
                    self.bytecode_dynamic_tail_call_return_julia_type(info, types)
                })
            })
            .or_else(|| self.bytecode_typed_tail_return_julia_type(info))
            .or_else(|| self.bytecode_literal_return_julia_type(info))
            .or_else(|| {
                info.return_julia_type
                    .clone()
                    .map(|ty| instantiate_return_julia_type(ty, info, arg_types))
            })
            .unwrap_or_else(|| value_type_to_julia_type(&info.return_type, &self.struct_defs))
    }

    fn bytecode_direct_call_return_julia_type(&self, info: &FunctionInfo) -> Option<JuliaType> {
        let code = self.code.get(info.code_start..info.code_end)?;
        let [call, ret] = code else {
            return None;
        };
        if !is_return_instr(ret) {
            return None;
        }
        let func_index = match call {
            Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) => {
                operands.func_index
            }
            _ => return None,
        };
        let callee = self.functions.get(func_index)?;
        if callee.code_start == info.code_start && callee.code_end == info.code_end {
            return None;
        }
        let callee_arg_types = callee.param_julia_types.clone();
        non_any_julia_type(self.method_return_julia_type(callee, Some(&callee_arg_types), None))
    }

    fn bytecode_dynamic_tail_call_return_julia_type(
        &self,
        info: &FunctionInfo,
        arg_types: &[JuliaType],
    ) -> Option<JuliaType> {
        let code = self.code.get(info.code_start..info.code_end)?;
        let return_idx = code.iter().rposition(is_return_instr)?;
        if !matches!(code.get(return_idx), Some(Instr::ReturnAny)) {
            return None;
        }
        if code
            .iter()
            .take(return_idx)
            .any(|instr| matches!(instr, Instr::Jump(_)))
        {
            return None;
        }

        let call_idx = return_idx.checked_sub(1)?;
        let Instr::CallDynamic(operands) = code.get(call_idx)? else {
            return None;
        };
        let argc = operands.arg_count;
        let candidates = &operands.candidates;
        let arg_start = call_idx.checked_sub(argc)?;
        let loaded_arg_types = code
            .get(arg_start..call_idx)?
            .iter()
            .map(|instr| self.bytecode_stack_arg_julia_type(instr, arg_types))
            .collect::<Option<Vec<_>>>()?;

        let mut table = MethodTable::new(info.name.clone());
        for candidate in candidates {
            let DynamicCallCandidate::Method(global_index) = candidate else {
                continue;
            };
            let Some(candidate_info) = self.functions.get(*global_index) else {
                continue;
            };
            let params: Vec<_> = candidate_info
                .params
                .iter()
                .zip(candidate_info.param_julia_types.iter())
                .map(|((name, _), ty)| (name.clone(), ty.clone()))
                .collect();
            if params.len() != candidate_info.params.len() {
                continue;
            }
            table.add_method(MethodSig::from_julia_projections(
                table.methods.len(),
                *global_index,
                params,
                candidate_info.return_type.clone(),
                candidate_info.return_julia_type.clone(),
                candidate_info.is_base_extension,
                candidate_info.type_params.clone(),
                candidate_info.vararg_param_index,
                candidate_info.vararg_fixed_count,
            ));
        }

        let method = table.dispatch(&loaded_arg_types).ok()?;
        let callee = self.functions.get(method.global_index)?;
        if callee.code_start == info.code_start && callee.code_end == info.code_end {
            return None;
        }
        non_any_julia_type(self.method_return_julia_type(callee, Some(&loaded_arg_types), None))
    }

    fn bytecode_stack_arg_julia_type(
        &self,
        instr: &Instr,
        arg_types: &[JuliaType],
    ) -> Option<JuliaType> {
        match instr {
            Instr::LoadSlot(slot)
            | Instr::LoadSlotI64(slot)
            | Instr::LoadSlotF64(slot)
            | Instr::LoadSlotF32(slot)
            | Instr::LoadSlotF16(slot)
            | Instr::LoadSlotBool(slot)
            | Instr::LoadSlotArray(slot)
            | Instr::LoadSlotTuple(slot)
            | Instr::LoadSlotStruct(slot) => arg_types.get(*slot).cloned(),
            Instr::PushI64(_) => Some(JuliaType::Int64),
            Instr::PushF64(_) => Some(JuliaType::Float64),
            Instr::PushF32(_) => Some(JuliaType::Float32),
            Instr::PushF16(_) => Some(JuliaType::Float16),
            Instr::PushBool(_) => Some(JuliaType::Bool),
            Instr::PushStr(_) => Some(JuliaType::String),
            _ => None,
        }
    }

    fn bytecode_typed_tail_return_julia_type(&self, info: &FunctionInfo) -> Option<JuliaType> {
        let code = self.code.get(info.code_start..info.code_end)?;
        let return_idx = code.iter().rposition(is_return_instr)?;
        if !matches!(code.get(return_idx), Some(Instr::ReturnAny)) {
            return None;
        }
        if code
            .iter()
            .take(return_idx)
            .any(|instr| matches!(instr, Instr::Jump(_)))
        {
            return None;
        }
        if self.bytecode_reads_widened_global(code, return_idx) {
            return None;
        }
        let tail = return_idx.checked_sub(1).and_then(|idx| code.get(idx))?;
        match tail {
            Instr::AddI64
            | Instr::SubI64
            | Instr::MulI64
            | Instr::ModI64
            | Instr::IncI64
            | Instr::NegI64
            | Instr::LoadAddI64(_)
            | Instr::LoadSubI64(_)
            | Instr::LoadMulI64(_)
            | Instr::LoadModI64(_)
            | Instr::LoadAddI64Slot(_)
            | Instr::LoadSubI64Slot(_)
            | Instr::LoadMulI64Slot(_)
            | Instr::LoadModI64Slot(_)
            | Instr::ToI64
            | Instr::BoolToI64
            | Instr::DynamicToI64 => Some(JuliaType::Int64),
            Instr::AddF64
            | Instr::SubF64
            | Instr::MulF64
            | Instr::DivF64
            | Instr::SqrtF64
            | Instr::FloorF64
            | Instr::CeilF64
            | Instr::AbsF64
            | Instr::Abs2F64
            | Instr::PowF64
            | Instr::NegF64
            | Instr::LoadDivF64Slot(_)
            | Instr::ToF64
            | Instr::DynamicToF64 => Some(JuliaType::Float64),
            Instr::DynamicToF32 => Some(JuliaType::Float32),
            Instr::DynamicToF16 => Some(JuliaType::Float16),
            Instr::I64ToBool | Instr::DynamicToBool | Instr::NotBool => Some(JuliaType::Bool),
            _ => None,
        }
    }

    fn bytecode_reads_widened_global(&self, code: &[Instr], return_idx: usize) -> bool {
        let Some(ctx) = self.compile_context.as_ref() else {
            return false;
        };
        code.iter().take(return_idx).any(|instr| {
            let name = match instr {
                Instr::LoadStr(name)
                | Instr::LoadI64(name)
                | Instr::LoadF64(name)
                | Instr::LoadF32(name)
                | Instr::LoadF16(name)
                | Instr::LoadBool(name)
                | Instr::LoadAny(name)
                | Instr::LoadGlobalAny(name)
                | Instr::LoadAddI64(name)
                | Instr::LoadSubI64(name)
                | Instr::LoadMulI64(name)
                | Instr::LoadModI64(name) => name,
                _ => return false,
            };
            matches!(ctx.inference_global_types.get(name), Some(ValueType::Any))
        })
    }

    fn bytecode_literal_return_julia_type(&self, info: &FunctionInfo) -> Option<JuliaType> {
        let code = self.code.get(info.code_start..info.code_end)?;
        if let Some(return_type) = self.bytecode_newstruct_return_julia_type(code) {
            return Some(return_type);
        }
        Self::bytecode_non_struct_literal_return_julia_type(code)
    }

    fn bytecode_non_struct_literal_return_julia_type(code: &[Instr]) -> Option<JuliaType> {
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

    fn bytecode_newstruct_return_julia_type(&self, code: &[Instr]) -> Option<JuliaType> {
        let return_idx = code.iter().rposition(is_return_instr)?;
        if !matches!(code.get(return_idx), Some(Instr::ReturnAny)) {
            return None;
        }
        let newstruct_idx = return_idx.checked_sub(1)?;
        let Instr::NewStruct(type_id, _) = code.get(newstruct_idx)? else {
            return None;
        };
        self.struct_defs
            .get(*type_id)
            .map(|def| JuliaType::Struct(def.name.clone()))
    }

    /// Structural return-type reflection for a DataType callee (Issue
    /// #11402). Constructors of a registered struct family return the
    /// concrete instantiated type upstream:
    /// - an applied parametric spelling (`S{Int64}`) constructs exactly
    ///   itself whenever a constructor of that arity exists (the synthesized
    ///   default's arity is the field count; explicit inners/outers register
    ///   as function rows), and reports `Union{}` for an arity nothing
    ///   accepts;
    /// - a bare family name (`S`) with concrete argument types infers the
    ///   type parameters from the field patterns (the same unifier the
    ///   runtime dynamic constructor uses).
    /// Returns `None` to fall back to ordinary function-name reflection when
    /// the callee is not a registered struct family or nothing resolves
    /// structurally.
    fn constructor_reflection_return_types(
        &self,
        type_name: &str,
        arg_types: &[JuliaType],
    ) -> Option<Vec<Value>> {
        let struct_result = |name: &str| {
            vec![Value::DataType(Box::new(JuliaType::Struct(
                name.to_string(),
            )))]
        };
        let method_accepts_arity = |query: &str| {
            self.functions.iter().enumerate().any(|(index, info)| {
                self.reflection_function_visible(index)
                    && reflection_function_name_matches(info, query)
                    && (info.params.len() == arg_types.len()
                        || info
                            .vararg_param_index
                            .is_some_and(|idx| arg_types.len() >= idx))
            })
        };
        if let Some(brace_idx) = type_name.find('{') {
            // Applied spelling: resolve the substituted field layout through
            // the runtime type registry (materialization-independent).
            let registry =
                RuntimeTypeRegistry::new(self.compile_context.as_ref(), &self.abstract_types);
            let field_types = registry
                .object(&JuliaType::Struct(type_name.to_string()))
                .field_types()?;
            let base_name = &type_name[..brace_idx];
            if field_types.len() == arg_types.len()
                || method_accepts_arity(type_name)
                || method_accepts_arity(base_name)
            {
                return Some(struct_result(type_name));
            }
            // A registered family whose constructors all reject this arity
            // dispatches to no method: Union{}.
            return Some(Vec::new());
        }
        if !arg_types.iter().all(JuliaType::is_concrete) {
            return None;
        }
        // Bare parametric family: infer the type parameters from the concrete
        // argument types, exactly like the runtime dynamic constructor.
        if let Some((base_name, def)) = self.resolve_runtime_parametric_def(type_name) {
            if def.inner_constructors.is_empty() && def.fields.len() == arg_types.len() {
                let inferred =
                    crate::runtime_types::infer_parametric_type_args(&def, &base_name, arg_types)
                        .ok()?;
                let type_arg_names: Vec<String> =
                    inferred.iter().map(|ty| ty.name().to_string()).collect();
                let applied = format!("{}{{{}}}", base_name, type_arg_names.join(", "));
                return Some(struct_result(&applied));
            }
            return None;
        }
        // Bare non-parametric struct: the default constructor takes one
        // argument per field.
        let def = self
            .struct_def_name_index
            .get(type_name)
            .and_then(|&idx| self.struct_defs.get(idx))?;
        if !def.name.contains('{') && def.fields.len() == arg_types.len() {
            return Some(struct_result(&def.name));
        }
        None
    }

    fn return_types_by_ftype(
        &self,
        func_name: &str,
        arg_types: &[JuliaType],
        closure_captures: Option<&[(String, Value)]>,
        candidate_indices: Option<&[usize]>,
    ) -> Result<Vec<Value>, VmError> {
        // A concrete builtin numeric `DataType` used as a constructor/
        // conversion callable (`Int64`, `Float64`, `UInt8`, `BigInt`, ...)
        // called with a single concrete numeric argument (real, arbitrary-
        // precision, or complex) constructs exactly that type upstream,
        // mirroring the generic
        // `(::Type{T})(x::Number) where T<:Number = convert(T, x)::T` — the
        // `::T` return-type assertion holds regardless of whether the
        // conversion throws `InexactError` at a given runtime value. Several
        // of these conversions (`Int8`, `UInt8`, `Float16`, ...) are
        // implemented as thin pure-Julia wrapper functions over an internal
        // intrinsic (e.g. `UInt8(x) = _to_uint8(x)` in base/int.jl), whose
        // own inferred return type is the imprecise `Any`; this structural
        // override runs before ordinary user-method dispatch so it wins
        // regardless of whether the callee resolves to a `FunctionInfo`
        // wrapper or has none at all (`Int64`/`Float64`/`BigInt`/`BigFloat`
        // have no pure-Julia wrapper, so previously fell through
        // `builtin_reflection_return_type` to `Union{}`) (Issue #11507).
        if let [arg] = arg_types {
            if is_concrete_numeric_constructor_arg(arg) {
                let short_name = func_name
                    .rsplit_once('.')
                    .map_or(func_name, |(_, name)| name);
                let callee = JuliaType::from_name_or_struct(short_name);
                if is_concrete_builtin_numeric_type(&callee) {
                    return Ok(vec![Value::DataType(Box::new(callee))]);
                }
            }
        }
        let has_user_methods = self.functions.iter().enumerate().any(|(index, info)| {
            self.reflection_function_selected(index, candidate_indices)
                && reflection_function_name_matches(info, func_name)
        });

        if !has_user_methods {
            return Ok(builtin_reflection_return_type(func_name, arg_types)
                .map(|ty| vec![Value::DataType(Box::new(ty))])
                .unwrap_or_default());
        }

        if arg_types.iter().all(JuliaType::is_concrete) {
            let table = self.reflection_method_table(func_name, candidate_indices);
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
            .find_matching_methods_for_candidates(func_name, Some(arg_types), candidate_indices)
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

    fn reflection_method_table(
        &self,
        func_name: &str,
        candidate_indices: Option<&[usize]>,
    ) -> MethodTable {
        let mut table = MethodTable::new(func_name.to_string());
        for (global_index, info) in self.functions.iter().enumerate() {
            if !self.reflection_function_selected(global_index, candidate_indices)
                || !reflection_function_name_matches(info, func_name)
            {
                continue;
            }
            let sig = MethodSig::from_julia_projections(
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
            );
            let keep = table
                .ordinary_method_with_same_signature(&sig)
                .and_then(|existing_index| self.functions.get(existing_index))
                .is_none_or(|existing| reflection_candidate_is_at_least_as_new(existing, info));
            if keep {
                table.add_method(sig);
            }
        }

        let mut struct_hierarchy = StructHierarchy::new();
        for def in &self.struct_defs {
            struct_hierarchy.insert(&def.name, def.parent_type.clone(), Vec::new());
        }
        for def in &self.abstract_types {
            struct_hierarchy.insert_if_absent(
                &def.name,
                def.parent.clone(),
                def.type_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect(),
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
        let world = self.current_dispatch_world();
        let source_names: HashSet<&str> = self
            .specializable_functions
            .iter()
            .filter_map(|specializable| {
                let fallback = self.functions.get(specializable.fallback_index)?;
                (self.function_visible_in_world(specializable.fallback_index, world)
                    && !fallback.is_lowering_helper)
                    .then_some(fallback.name.as_str())
            })
            .collect();

        self.specializable_functions
            .iter()
            .filter_map(|specializable| {
                // This is the inference dependency graph, not a Julia-visible
                // reflection listing. A selected source method may call a
                // private lowering helper, so retain every world-visible IR
                // dependency while public candidate selection continues to use
                // `reflection_function_visible` (Issue #11685).
                if !self.function_visible_in_world(specializable.fallback_index, world) {
                    return None;
                }
                // Deref Arc to get an owned clone of the Function for mutation.
                let mut func = (*specializable.ir).clone();
                if let Some(fallback) = self.functions.get(specializable.fallback_index) {
                    // A private helper is an implementation dependency, never
                    // an overload of a same-spelled source generic. Exclude
                    // only that colliding dependency; uniquely named helpers
                    // remain available for interprocedural inference, including
                    // nested #4268 callables (Issue #11685).
                    if fallback.is_lowering_helper && source_names.contains(fallback.name.as_str())
                    {
                        return None;
                    }
                    func.name = fallback.name.clone();
                }
                Some(func)
            })
            .collect()
    }

    fn seed_reflection_effect_callees(&self, target_ir: &Function) -> HashMap<String, Effects> {
        let mut call_arities = HashMap::new();
        collect_call_arities_from_block(&target_ir.body, &mut call_arities);

        let mut seeded = HashMap::new();
        for (callee_name, arities) in call_arities {
            if callee_name == target_ir.name {
                continue;
            }
            for arity in arities {
                if let Some(effects) =
                    self.compose_arity_matched_callee_effects(&callee_name, arity)
                {
                    seeded
                        .entry(callee_name.clone())
                        .and_modify(|existing: &mut Effects| *existing = existing.merge(&effects))
                        .or_insert(effects);
                }
            }
        }
        seeded
    }

    fn compose_arity_matched_callee_effects(
        &self,
        callee_name: &str,
        arity: usize,
    ) -> Option<Effects> {
        let mut exact_source = Vec::new();
        let mut exact_helper = Vec::new();
        let mut vararg_source = Vec::new();
        let mut vararg_helper = Vec::new();
        let mut has_source_name = false;
        for specializable in &self.specializable_functions {
            if !self.function_visible_in_world(
                specializable.fallback_index,
                self.current_dispatch_world(),
            ) {
                continue;
            }
            let Some(fallback) = self.functions.get(specializable.fallback_index) else {
                continue;
            };
            if fallback.name != callee_name {
                continue;
            }
            let is_helper = specializable.ir.is_lowering_helper();
            has_source_name |= !is_helper;
            if fallback.vararg_param_index.is_none() && fallback.param_julia_types.len() == arity {
                // Deref Arc to get an owned clone needed for name mutation below.
                let candidates = if is_helper {
                    &mut exact_helper
                } else {
                    &mut exact_source
                };
                candidates.push((fallback.name.clone(), (*specializable.ir).clone()));
            } else if fallback.vararg_param_index.is_some()
                && fallback.vararg_fixed_count.unwrap_or(0) <= arity
            {
                let candidates = if is_helper {
                    &mut vararg_helper
                } else {
                    &mut vararg_source
                };
                candidates.push((fallback.name.clone(), (*specializable.ir).clone()));
            }
        }

        // A private lowering helper is never an overload of a same-spelled
        // source generic. Prefer source candidates at each dispatch tier, just
        // like the reflection inference function table (Issue #11685).
        let candidates = if has_source_name {
            if exact_source.is_empty() {
                vararg_source
            } else {
                exact_source
            }
        } else if exact_helper.is_empty() {
            vararg_helper
        } else {
            exact_helper
        };
        let mut effects: Option<Effects> = None;
        for (fallback_name, mut ir) in candidates {
            ir.name = fallback_name;
            let callee_effects = infer_function_effects(&ir, &HashMap::new());
            effects = Some(match effects {
                None => callee_effects,
                Some(existing) => existing.merge(&callee_effects),
            });
        }
        effects
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
        let compile_context = self.compile_context.as_ref()?;
        let target = self.specializable_functions.iter().find(|specializable| {
            self.functions
                .get(specializable.fallback_index)
                .is_some_and(|fallback| {
                    specializable.ir.params.len() == info.param_julia_types.len()
                        && same_reflection_function_identity(fallback, info)
                })
        })?;
        let mentions_parametric_constructor = block_mentions_parametric_constructor(
            &target.ir.body,
            &compile_context.parametric_structs,
        );

        let has_unknown_return_snapshot = matches!(info.return_type, ValueType::Any)
            && matches!(info.return_julia_type.as_ref(), None | Some(JuliaType::Any));
        if !has_untyped_param && !has_unknown_return_snapshot && !mentions_parametric_constructor {
            return None;
        }
        let force_body_inference = mentions_parametric_constructor;

        let inference_functions = self.build_specializable_inference_functions();
        let mut engine = build_reflection_inference_session(
            &compile_context.struct_table,
            &compile_context.inference_global_types,
            inference_functions.iter(),
        )?;
        // Parametric struct definitions live only by base name in the compile
        // context, so hand them to the engine to recover concrete instantiated
        // constructor returns and field facts (Issues #4849 / #4850 / #4851).
        engine.set_parametric_structs(compile_context.parametric_structs.clone());
        self.seed_reflection_return_snapshots(engine.as_mut());
        let mut target_ir = (*target.ir).clone();
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

        // The seeded cache can contain the exact stale `Any` snapshot for a
        // fully typed method. Body-walk the target directly when the matched IR
        // contains a parametric default constructor so reflection recovers its
        // concrete type arguments (Issue #8638).
        let inferred = if closure_captures.is_some()
            || !where_param_bindings.is_empty()
            || force_body_inference
        {
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
        info_index: usize,
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
                if !self.function_visible_in_world(
                    specializable.fallback_index,
                    self.current_dispatch_world(),
                ) {
                    return false;
                }
                if specializable.fallback_index == info_index {
                    return true;
                }
                self.functions
                    .get(specializable.fallback_index)
                    .is_some_and(|fallback| {
                        specializable.ir.params.len() == info.param_julia_types.len()
                            && same_reflection_function_identity(fallback, info)
                    })
            })?;

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

            let mut engine = build_reflection_inference_session(
                &compile_context.struct_table,
                &compile_context.inference_global_types,
                inference_functions.iter(),
            )?;
            engine.set_parametric_structs(compile_context.parametric_structs.clone());
            engine.set_base_function_names(base_function_names);

            let mut target_ir = (*target.ir).clone();
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

    /// Compose body-derived effects for a matched user method (Issue #8441).
    fn compose_function_effects(
        &mut self,
        info_index: usize,
        info: &FunctionInfo,
        arg_types: &[JuliaType],
        closure_captures: Option<&[(String, Value)]>,
    ) -> Option<Effects> {
        // Capture-aware body walking needs the same env seeding as return-type
        // inference; keep this first slice to top-level user methods.
        if closure_captures.is_some() {
            return None;
        }

        let target = self.specializable_functions.iter().find(|specializable| {
            if !self.function_visible_in_world(
                specializable.fallback_index,
                self.current_dispatch_world(),
            ) {
                return false;
            }
            if specializable.fallback_index == info_index {
                return true;
            }
            self.functions
                .get(specializable.fallback_index)
                .is_some_and(|fallback| {
                    specializable.ir.params.len() == info.param_julia_types.len()
                        && same_reflection_function_identity(fallback, info)
                })
        })?;

        let mut target_ir = (*target.ir).clone();
        target_ir.name = info.name.clone();
        let callee_effects = self.seed_reflection_effect_callees(&target_ir);
        let mut effects = infer_function_effects(&target_ir, &callee_effects);
        if self
            .compose_function_exception_type(info_index, info, arg_types, closure_captures)
            .is_some()
        {
            effects.nothrow = false;
        }
        Some(effects)
    }

    fn seed_reflection_return_snapshots(&self, engine: &mut dyn ReflectionInferenceSession) {
        for (global_index, info) in self.functions.iter().enumerate() {
            if !self.reflection_function_visible(global_index) {
                continue;
            }
            let seeded_return_julia_type = info
                .return_julia_type
                .clone()
                .or_else(|| {
                    matches!(info.return_type, ValueType::Struct(_))
                        .then(|| value_type_to_julia_type(&info.return_type, &self.struct_defs))
                })
                .or_else(|| {
                    self.code
                        .get(info.code_start..info.code_end)
                        .and_then(Self::bytecode_non_struct_literal_return_julia_type)
                });
            if seeded_return_julia_type.is_none() {
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
                seeded_return_julia_type,
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
        // The callable type carries the source/helper provenance encoding.
        // Select its exact retained IR before constructing the name-keyed
        // dependency engine; a private generator helper may legally share its
        // surface spelling with a source generic (Issue #11685).
        let target = self.specializable_functions.iter().find(|specializable| {
            self.function_visible_in_world(
                specializable.fallback_index,
                self.current_dispatch_world(),
            ) && self
                .functions
                .get(specializable.fallback_index)
                .is_some_and(|fallback| {
                    CallableSingletonIdentity::from_provenance(
                        fallback.name.clone(),
                        fallback.is_lowering_helper,
                    )
                    .encoded_name()
                        == callable_name
                })
        })?;
        let target_fallback = self.functions.get(target.fallback_index)?;
        let mut target_ir = (*target.ir).clone();
        // Match the qualified name used by dependency-engine registration so
        // nested/module helpers retain their lexical owner during the direct
        // body walk.
        target_ir.name = target_fallback.name.clone();
        let inference_functions = self.build_specializable_inference_functions();
        let mut engine = build_reflection_inference_session(
            &compile_context.struct_table,
            &compile_context.inference_global_types,
            inference_functions.iter(),
        )?;
        let arg_lattice = reflection_julia_type_to_lattice(&element_type);
        let inferred = engine.infer_function_with_arg_types(&target_ir, &[arg_lattice]);
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

/// Substitute uses of a promoted binder in a UnionAll body without rendering
/// that identity back into the string-only bounds of nested legacy binders.
/// Those binders are promoted from their own cached RuntimeTypeVar nodes on the
/// next recursion step; rewriting `C<:B` to the display string `C<:B<:A` would
/// make its exact cache key unrecognizable (Issue #10261).
fn substitute_projected_unionall_body(
    body: &JuliaType,
    var_name: &str,
    replacement: &JuliaType,
) -> JuliaType {
    match body {
        JuliaType::UnionAll {
            var,
            lower_bound,
            bound,
            body,
        } => JuliaType::UnionAll {
            var: var.clone(),
            lower_bound: lower_bound.clone(),
            bound: bound.clone(),
            body: if var == var_name {
                body.clone()
            } else {
                Box::new(substitute_projected_unionall_body(
                    body,
                    var_name,
                    replacement,
                ))
            },
        },
        JuliaType::RuntimeUnionAll { var, body } => JuliaType::RuntimeUnionAll {
            var: var.clone(),
            body: Box::new(substitute_projected_unionall_body(
                body,
                var_name,
                replacement,
            )),
        },
        _ => body.substitute(var_name, replacement),
    }
}

/// Parse a binder's as-declared bounds into the structural
/// [`TypeVarProjectionKey`] components (Issue #10987): the legacy rendered
/// interval carried by `JuliaType::TypeVar` is parsed once through
/// [`reflected_typevar_bounds`]' grammar, with absent bounds defaulting to
/// `Bottom`/`Any`. Keying on the PARSED `JuliaType`s (not the rendered
/// string, the pre-#10987 shape) makes cache identity insensitive to
/// spelling drift (`"Int"` vs `"Int64"`, interval-format reconstruction,
/// whitespace) while keeping module-qualified spellings distinct (the
/// `CoreType` bridge would strip qualification -- see
/// `TypeVarProjectionKey`'s doc).
fn declared_projection_bounds_key(name: &str, bounds: Option<&str>) -> (JuliaType, JuliaType) {
    reflected_typevar_bounds(name, bounds)
}

/// Sibling of [`declared_projection_bounds_key`] for a legacy
/// `JuliaType::UnionAll` node, whose lower/upper bound names are already
/// separate fields (no interval string to parse). Must produce exactly what
/// [`declared_projection_bounds_key`] produces for the interval
/// `unionall_var()` renders from the same fields, so node-walk lookups hit
/// the entries the projection path inserted (kept in lockstep with
/// [`reflected_typevar_bounds`]' three-part arm, which splits the interval
/// at TOP-LEVEL `<:` only for exactly this reason).
fn declared_projection_bounds_key_from_parts(
    lower: Option<&str>,
    upper: Option<&str>,
) -> (JuliaType, JuliaType) {
    let lower = lower
        .map(|name| JuliaType::from_name_or_struct(name.trim()))
        .unwrap_or(JuliaType::Bottom);
    let upper = upper
        .map(|name| JuliaType::from_name_or_struct(name.trim()))
        .unwrap_or(JuliaType::Any);
    (lower, upper)
}

/// Split a legacy TypeVar bounds interval at TOP-LEVEL `<:` occurrences
/// only, so a bound type that itself contains `<:` inside brackets
/// (`Vector{Int64}<:T<:Vector{<:Real}`) still parses as the three-part
/// two-sided form instead of falling into the whole-string fallback
/// (Issue #11020; found by adversarial codex review of Issue #10987 -- a
/// naive `split("<:")` here disagreed with the node-walk key derivation in
/// `declared_projection_bounds_key_from_parts` and produced garbage bound
/// values for such intervals).
fn split_top_level_subtype(bounds: &str) -> Vec<&str> {
    let bytes = bounds.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth = depth.saturating_sub(1),
            b'<' if depth == 0 && bytes.get(i + 1) == Some(&b':') => {
                parts.push(&bounds[start..i]);
                start = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&bounds[start..]);
    parts
}

fn reflected_typevar_bounds(name: &str, bounds: Option<&str>) -> (JuliaType, JuliaType) {
    let Some(bounds) = bounds else {
        return (JuliaType::Bottom, JuliaType::Any);
    };
    if let Some(lower) = bounds.strip_prefix(">:") {
        return (JuliaType::from_name_or_struct(lower.trim()), JuliaType::Any);
    }
    let parts: Vec<_> = split_top_level_subtype(bounds)
        .into_iter()
        .map(str::trim)
        .collect();
    if let [lower, middle, upper] = parts.as_slice() {
        if *middle == name {
            return (
                JuliaType::from_name_or_struct(lower),
                JuliaType::from_name_or_struct(upper),
            );
        }
    }
    (
        JuliaType::Bottom,
        JuliaType::from_name_or_struct(bounds.trim()),
    )
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
    if !(is_concrete_numeric_or_complex(a) && is_concrete_numeric_or_complex(b)) {
        return None;
    }
    let name_a = a.name();
    let name_b = b.name();
    let promoted = if is_complex_type_name(&name_a) || is_complex_type_name(&name_b) {
        crate::promotion::promote_complex(&name_a, &name_b)
    } else {
        crate::promotion::promote_type(&name_a, &name_b)
    };
    if promoted == "Any" || promoted.is_empty() || promoted == "Union{}" {
        return None;
    }
    let result_name = if op == "/" {
        float_widen_result_type_name(&promoted)
    } else {
        promoted
    };
    let result = JuliaType::from_name_or_struct(&result_name);
    // Only return a concrete numeric answer; otherwise defer.
    if is_concrete_numeric_or_complex(&result) {
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

fn is_complex_type_name(name: &str) -> bool {
    name.starts_with("Complex{") && name.ends_with('}')
}

fn is_concrete_complex_numeric(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::Struct(name) if is_complex_type_name(name))
}

fn is_concrete_numeric_or_complex(ty: &JuliaType) -> bool {
    is_concrete_real_numeric(ty) || is_concrete_complex_numeric(ty)
}

/// `true` for every concrete builtin numeric leaf `DataType` that can appear
/// as a constructor/conversion callable (`Int64(x)`, `Float64(x)`, ...),
/// including the arbitrary-precision types excluded from
/// [`is_concrete_real_numeric`] (Issue #11507).
fn is_concrete_builtin_numeric_type(ty: &JuliaType) -> bool {
    is_concrete_real_numeric(ty) || matches!(ty, JuliaType::BigInt | JuliaType::BigFloat)
}

/// `true` for every concrete numeric (real, arbitrary-precision, or complex)
/// argument type that a builtin numeric constructor/conversion accepts —
/// `T(x)` constructs exactly `T` for any such `x`, including `BigInt`/
/// `BigFloat` arguments (Issue #11507).
fn is_concrete_numeric_constructor_arg(ty: &JuliaType) -> bool {
    is_concrete_builtin_numeric_type(ty) || is_concrete_complex_numeric(ty)
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

fn float_widen_result_type_name(name: &str) -> String {
    if let Some(inner) = name
        .strip_prefix("Complex{")
        .and_then(|inner| inner.strip_suffix('}'))
    {
        return format!("Complex{{{}}}", float_widen_type_name(inner));
    }
    float_widen_type_name(name)
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
        "abs" if matches!(arg_types, [ty] if is_concrete_real_numeric(ty)) => {
            Some(arg_types[0].clone())
        }
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

fn effects_to_value_tuple(effects: Effects) -> Value {
    Value::Tuple(TupleValue::new(vec![
        Value::U8(effect_bit_to_julia_u8(effects.consistent)),
        Value::U8(effect_bit_to_julia_u8(effects.effect_free)),
        Value::Bool(effects.nothrow),
        Value::Bool(effects.terminates),
        Value::Bool(effects.notaskstate),
        Value::U8(bool_effect_to_julia_u8(effects.inaccessiblememonly)),
        // `noub` is tri-stated (Issue #9496): reuse the same AlwaysTrue(0x00)
        // / AlwaysFalse(0x01) / Conditional(0x02) encoding as consistent /
        // effect_free above, matching upstream `noub::UInt8`'s
        // ALWAYS_TRUE/ALWAYS_FALSE/NOUB_IF_NOINBOUNDS encoding exactly.
        Value::U8(effect_bit_to_julia_u8(effects.noub)),
        Value::U8(bool_effect_to_julia_u8(effects.nonoverlayed)),
        Value::Bool(effects.nortcall),
    ]))
}

fn effect_bit_to_julia_u8(bit: EffectBit) -> u8 {
    match bit {
        EffectBit::AlwaysTrue => 0x00,
        EffectBit::AlwaysFalse => 0x01,
        EffectBit::Conditional => 0x02,
    }
}

fn bool_effect_to_julia_u8(value: bool) -> u8 {
    if value {
        0x00
    } else {
        0x01
    }
}

fn block_mentions_parametric_constructor(
    block: &Block,
    parametric_structs: &HashMap<String, ParametricStructDef>,
) -> bool {
    let mut call_arities = HashMap::new();
    collect_call_arities_from_block(block, &mut call_arities);
    call_arities.keys().any(|name| {
        parametric_structs.contains_key(name)
            || name
                .split_once('{')
                .is_some_and(|(base, _)| parametric_structs.contains_key(base))
    })
}

fn collect_call_arities_from_block(block: &Block, out: &mut HashMap<String, HashSet<usize>>) {
    for stmt in &block.stmts {
        collect_call_arities_from_stmt(stmt, out);
    }
}

fn collect_call_arities_from_stmt(stmt: &Stmt, out: &mut HashMap<String, HashSet<usize>>) {
    match stmt {
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => collect_call_arities_from_block(block, out),
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::Expr { expr: value, .. }
        | Stmt::Test {
            condition: value, ..
        }
        | Stmt::IndexAssign { value, .. }
        | Stmt::FieldAssign { value, .. }
        | Stmt::DestructuringAssign { value, .. } => collect_call_arities_from_expr(value, out),
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            collect_call_arities_from_expr(start, out);
            collect_call_arities_from_expr(end, out);
            if let Some(step) = step {
                collect_call_arities_from_expr(step, out);
            }
            collect_call_arities_from_block(body, out);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            collect_call_arities_from_expr(iterable, out);
            collect_call_arities_from_block(body, out);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_call_arities_from_expr(condition, out);
            collect_call_arities_from_block(body, out);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_call_arities_from_expr(condition, out);
            collect_call_arities_from_block(then_branch, out);
            if let Some(else_branch) = else_branch {
                collect_call_arities_from_block(else_branch, out);
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            collect_call_arities_from_block(try_block, out);
            if let Some(catch_block) = catch_block {
                collect_call_arities_from_block(catch_block, out);
            }
            if let Some(else_block) = else_block {
                collect_call_arities_from_block(else_block, out);
            }
            if let Some(finally_block) = finally_block {
                collect_call_arities_from_block(finally_block, out);
            }
        }
        Stmt::TestThrows { expr, .. } => collect_call_arities_from_expr(expr, out),
        Stmt::DictAssign { key, value, .. } => {
            collect_call_arities_from_expr(key, out);
            collect_call_arities_from_expr(value, out);
        }
        Stmt::FunctionDef { .. }
        | Stmt::EvalFunctionDef { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::RuntimeNominalDef { .. }
        | Stmt::Global { .. }
        | Stmt::Return { value: None, .. } => {}
    }
}

fn collect_call_arities_from_expr(expr: &Expr, out: &mut HashMap<String, HashSet<usize>>) {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            out.entry(function.to_string())
                .or_default()
                .insert(args.len());
            for arg in args {
                collect_call_arities_from_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_call_arities_from_expr(value, out);
            }
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            out.entry(function.to_string())
                .or_default()
                .insert(args.len());
            for arg in args {
                collect_call_arities_from_expr(arg, out);
            }
            for (_, value) in kwargs {
                collect_call_arities_from_expr(value, out);
            }
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::Pair {
            key: left,
            value: right,
            ..
        } => {
            collect_call_arities_from_expr(left, out);
            collect_call_arities_from_expr(right, out);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::QuoteLiteral {
            constructor: operand,
            ..
        }
        | Expr::AssignExpr { value: operand, .. } => collect_call_arities_from_expr(operand, out),
        // Structural counterpart of a bare `Int64(x)` / `Float64(x)` call
        // (Issue #9803): record the same (name, arity) a plain `Expr::Call`
        // would have, so arity-driven reflection sees the same call set.
        Expr::Convert {
            target, operand, ..
        } => {
            let name = match target {
                NumericConvertTarget::Int64 => "Int64",
                NumericConvertTarget::Float64 => "Float64",
            };
            out.entry(name.to_string()).or_default().insert(1);
            collect_call_arities_from_expr(operand, out);
        }
        Expr::ReturnExpr {
            value: Some(value), ..
        } => collect_call_arities_from_expr(value, out),
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                collect_call_arities_from_expr(element, out);
            }
        }
        Expr::Builtin { args, .. } | Expr::New { args, .. } => {
            for arg in args {
                collect_call_arities_from_expr(arg, out);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_call_arities_from_expr(array, out);
            for index in indices {
                collect_call_arities_from_expr(index, out);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_call_arities_from_expr(start, out);
            if let Some(step) = step {
                collect_call_arities_from_expr(step, out);
            }
            collect_call_arities_from_expr(stop, out);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            collect_call_arities_from_expr(body, out);
            collect_call_arities_from_expr(iter, out);
            if let Some(filter) = filter {
                collect_call_arities_from_expr(filter, out);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            collect_call_arities_from_expr(body, out);
            for (_, iter) in iterations {
                collect_call_arities_from_expr(iter, out);
            }
            if let Some(filter) = filter {
                collect_call_arities_from_expr(filter, out);
            }
        }
        Expr::FieldAccess { object, .. } => collect_call_arities_from_expr(object, out),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                collect_call_arities_from_expr(value, out);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                collect_call_arities_from_expr(key, out);
                collect_call_arities_from_expr(value, out);
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                collect_call_arities_from_expr(value, out);
            }
            collect_call_arities_from_block(body, out);
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                collect_call_arities_from_expr(part, out);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            collect_call_arities_from_expr(condition, out);
            collect_call_arities_from_expr(then_expr, out);
            collect_call_arities_from_expr(else_expr, out);
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                collect_call_arities_from_expr(base_expr, out);
            }
            for type_arg in type_args {
                collect_call_arities_from_expr(type_arg, out);
            }
        }
        Expr::Literal(_, _)
        | Expr::Var(_, _)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. }
        | Expr::ReturnExpr { value: None, .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
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
        let julia_arg_types: Vec<JuliaType> = arg_types.iter().map(lattice_to_julia_type).collect();
        let types_val = Value::DataType(Box::new(JuliaType::TupleOf(julia_arg_types.clone())));
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

    fn compose_base_extension_callee(
        &mut self,
        name: &str,
        arg_types: &[LatticeType],
    ) -> Option<ExceptionType> {
        let julia_arg_types: Vec<JuliaType> = arg_types.iter().map(lattice_to_julia_type).collect();
        let (info_index, info) = self
            .vm
            .find_composable_methods(name, &julia_arg_types)
            .into_iter()
            .find(|(_, info)| info.is_base_extension)?;
        self.vm
            .compose_function_exception_type(info_index, &info, &julia_arg_types, None)
            .and_then(|jt| classified_value_to_exception_type(&Value::DataType(Box::new(jt))))
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
/// [`crate::runtime_types::julia_type_to_lattice`] (the facade re-export of
/// the compiler's bridge, Issue #8449) so reflection builtins
/// (`Base.infer_return_type`, exception-type inference, element-type
/// inference) feed the inference engine the same lattice spelling the
/// compiler uses. This also fixes the historical divergences of the local
/// copy: empty `Union{}` now lowers to `Bottom` (it produced
/// `LatticeType::Union(∅)`), a union containing `Any` widens to `Top`, and
/// `Real`/`Signed`/`Unsigned` keep their abstract numeric markers instead of
/// collapsing to `Top`.
fn reflection_julia_type_to_lattice(ty: &JuliaType) -> LatticeType {
    julia_type_to_lattice(ty)
}

fn reflection_value_to_lattice(value: &Value) -> LatticeType {
    match value {
        Value::I64(v) => LatticeType::Const(ConstValue::Int64(*v)),
        Value::F64(v) => LatticeType::Const(ConstValue::Float64(*v)),
        Value::Bool(v) => LatticeType::Const(ConstValue::Bool(*v)),
        Value::Str(v) => LatticeType::Const(ConstValue::String(v.to_string())),
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
        if info.is_generated && returned_type_param_is_mentioned(returned, param_ty) {
            return Some(JuliaType::Any);
        }
    }
    None
}

fn returned_type_param_is_mentioned(returned: &str, param_ty: &JuliaType) -> bool {
    is_returned_typevar(param_ty, returned)
        || match param_ty {
            JuliaType::TypeOf(inner) | JuliaType::VectorOf(inner) | JuliaType::MatrixOf(inner) => {
                returned_type_param_is_mentioned(returned, inner)
            }
            JuliaType::TupleOf(types) | JuliaType::Union(types) => types
                .iter()
                .any(|ty| returned_type_param_is_mentioned(returned, ty)),
            JuliaType::UnionAll { body, .. } => returned_type_param_is_mentioned(returned, body),
            JuliaType::Struct(name) => split_parametric_name(name)
                .1
                .iter()
                .any(|param| param.trim() == returned),
            _ => false,
        }
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
            // `::Type{T}` matched against a concrete `Type{C}` argument: bind
            // `T` to `C` (unwrap *both* sides' `TypeOf` layer). `concrete` is
            // itself `Type{C}` (the type of the passed-in type object `C`),
            // so using its own `to_string()` here would bind `T` to the
            // bogus doubly-wrapped name `"Type{C}"` instead of `"C"` (Issue
            // #10133) — the same unwrap `bind_returned_type_param` already
            // does for the direct-return case just below.
            if let JuliaType::TypeVar(name, _) = inner.as_ref() {
                if where_params.contains(&name.as_str()) {
                    if let JuliaType::TypeOf(concrete_inner) = concrete {
                        bindings.insert(
                            (*name).to_string(),
                            LatticeType::Concrete(ConcreteType::DataType {
                                name: concrete_inner.to_string(),
                            }),
                        );
                    }
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
mod module_alias_reflection_tests {
    #[test]
    fn module_local_type_alias_fallback_preserves_owner_11176() {
        assert_eq!(
            super::qualify_module_local_type_alias_target("Outer.P", "T", |name| {
                name == "Outer.P.T"
            }),
            "Outer.P.T"
        );
        assert_eq!(
            super::qualify_module_local_type_alias_target("Outer.P", "T{Int64}", |name| {
                name == "Outer.P.T"
            }),
            "Outer.P.T{Int64}"
        );
        assert_eq!(
            super::qualify_module_local_type_alias_target("Outer.P", "Base.Int", |_| true),
            "Base.Int"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;
    use std::collections::{BTreeSet, HashMap, HashSet};

    fn test_span() -> Span {
        Span::new(0, 0, 1, 1, 0, 0)
    }

    // `UnionAll.var` and body `.parameters` are one owner-scoped identity
    // domain. Constructed parametric arguments carry their runtime identity in
    // `JuliaType::RuntimeTypeVar`; no name-keyed fallback may join the two
    // domains (Issues #10049/#10252/#10420).
    #[test]
    fn reflection_parameter_to_value_for_owner_prefers_owner_scoped_projection_issue_10420() {
        use crate::rng::StableRng;

        let mut vm = Vm::new(vec![], StableRng::new(0));

        // Simulate an unrelated user `T = TypeVar(:T)`.
        let user_tv = RuntimeTypeVarValue {
            id: 424_242,
            name: "T".to_string(),
            lower_bound: JuliaType::Bottom,
            upper_bound: JuliaType::Any,
        };

        // `Vector` (the UnionAll wrapper) and its body `Array{T, 1}` resolve
        // to the same owner key: one owner-scoped identity domain.
        let vector = JuliaType::Struct("Vector".to_string());
        let matrix = JuliaType::Struct("Matrix".to_string());
        let body = JuliaType::Struct("Array{T, 1}".to_string());
        assert_eq!(
            vm.runtime_typevar_projection_owner_key(&vector),
            vm.runtime_typevar_projection_owner_key(&body),
            "UnionAll wrapper and its body must share one projection owner key"
        );
        assert_ne!(
            vm.runtime_typevar_projection_owner_key(&vector),
            vm.runtime_typevar_projection_owner_key(&matrix),
            "Vector and Matrix projection domains must retain the Array rank"
        );

        // `Vector.var` projects an owner-scoped TypeVar (never the user's
        // constructed TypeVar, Issue #10252).
        let Value::RuntimeTypeVar(var_tv) = vm.runtime_typevar_value_for_unionall_projection(
            &vector,
            JuliaType::TypeVar("T".to_string(), None),
        ) else {
            panic!("Vector.var must project to a RuntimeTypeVar");
        };
        assert_ne!(
            var_tv.id, user_tv.id,
            "wrapper projection must not reuse the constructed-TypeVar identity"
        );

        // Body `.parameters` projection: the owner-local projection cache must
        // beat the global constructed-TypeVar cache. The built-in body string
        // re-parses the bare `T` as `JuliaType::Struct("T")` (Issue #10412),
        // so exercise that arm first...
        let Value::RuntimeTypeVar(param_tv) = vm.reflection_parameter_to_value_for_owner(
            &body,
            ReflectionParameter::Type(JuliaType::Struct("T".to_string())),
        ) else {
            panic!("owner-scoped `.parameters` entry must project to a RuntimeTypeVar");
        };
        assert_eq!(
            param_tv.id, var_tv.id,
            "Vector.var === Vector.body.parameters[1] must hold"
        );
        assert_ne!(
            param_tv.id, user_tv.id,
            "owner-scoped projection must not leak the user TypeVar identity"
        );

        // ...and the genuine `JuliaType::TypeVar` arm (user parametric type
        // bodies) must resolve through the same owner-scoped identity.
        let Value::RuntimeTypeVar(tv_arm) = vm.reflection_parameter_to_value_for_owner(
            &body,
            ReflectionParameter::Type(JuliaType::TypeVar("T".to_string(), None)),
        ) else {
            panic!("owner-scoped TypeVar `.parameters` entry must project to a RuntimeTypeVar");
        };
        assert_eq!(
            tv_arm.id, var_tv.id,
            "TypeVar-arm projection must share the owner-scoped identity"
        );

        // Constructed parameters carry identity structurally, without a global
        // name lookup: `Vector{T}.parameters[1] === T` stays true.
        let Value::RuntimeTypeVar(constructed_tv) =
            vm.reflection_parameter_to_value(ReflectionParameter::Type(user_tv.projection()))
        else {
            panic!("constructed `.parameters` entry must project to a RuntimeTypeVar");
        };
        assert_eq!(
            constructed_tv.id, user_tv.id,
            "Vector{{T}}.parameters[1] === T must keep the constructed identity"
        );
    }

    #[test]
    fn dependent_unionall_bounds_share_owner_scoped_runtime_ids_issue_10261() {
        use crate::rng::StableRng;

        let mut vm = Vm::new(vec![], StableRng::new(0));
        let body = JuliaType::Struct("Dep3{A, B, C}".to_string());
        let wrapper = JuliaType::UnionAll {
            var: "A".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                var: "B".to_string(),
                lower_bound: None,
                bound: Some(Box::new("A".to_string())),
                body: Box::new(JuliaType::UnionAll {
                    var: "C".to_string(),
                    lower_bound: None,
                    bound: Some(Box::new("B".to_string())),
                    body: Box::new(body.clone()),
                }),
            }),
        };
        assert_eq!(
            vm.runtime_typevar_projection_owner_key(&wrapper),
            vm.runtime_typevar_projection_owner_key(&body),
            "explicit UnionAll and final body must share one owner domain"
        );

        // Request the innermost binder first: projection must not depend on
        // the caller having populated A/B in declaration order.
        let Value::RuntimeTypeVar(c) = vm.runtime_typevar_value_for_unionall_projection(
            &wrapper,
            JuliaType::TypeVar("C".to_string(), Some("B".to_string())),
        ) else {
            panic!("C must project to a runtime TypeVar");
        };
        let Value::RuntimeTypeVar(b) = vm.runtime_typevar_value_for_unionall_projection(
            &wrapper,
            JuliaType::TypeVar("B".to_string(), Some("A".to_string())),
        ) else {
            panic!("B must project to a runtime TypeVar");
        };
        let Value::RuntimeTypeVar(a) = vm.runtime_typevar_value_for_unionall_projection(
            &wrapper,
            JuliaType::TypeVar("A".to_string(), None),
        ) else {
            panic!("A must project to a runtime TypeVar");
        };

        assert!(matches!(
            &b.upper_bound,
            JuliaType::RuntimeTypeVar { id, .. } if *id == a.id
        ));
        assert!(matches!(
            &c.upper_bound,
            JuliaType::RuntimeTypeVar { id, .. } if *id == b.id
        ));
        assert_eq!(c.upper_bound, b.projection());

        let promoted = vm.project_unionall_binders_for_owner(&wrapper, &wrapper);
        let JuliaType::RuntimeUnionAll {
            var: promoted_a,
            body,
        } = promoted
        else {
            panic!("outer A binder must promote to RuntimeUnionAll");
        };
        let JuliaType::RuntimeUnionAll {
            var: promoted_b,
            body,
        } = *body
        else {
            panic!("middle B binder must promote to RuntimeUnionAll");
        };
        let JuliaType::RuntimeUnionAll {
            var: promoted_c,
            body,
        } = *body
        else {
            panic!("inner C binder must promote to RuntimeUnionAll");
        };
        assert!(matches!(promoted_a.as_ref(), JuliaType::RuntimeTypeVar { id, .. } if *id == a.id));
        assert!(matches!(promoted_b.as_ref(), JuliaType::RuntimeTypeVar { id, .. } if *id == b.id));
        assert!(matches!(promoted_c.as_ref(), JuliaType::RuntimeTypeVar { id, .. } if *id == c.id));
        let JuliaType::RuntimeTypeVar {
            upper_bound: promoted_c_upper,
            ..
        } = promoted_c.as_ref()
        else {
            unreachable!()
        };
        assert_eq!(promoted_c_upper.as_ref(), &b.projection());
        let JuliaType::RuntimeParametric { params, .. } = *body else {
            panic!("promoted body must retain structured runtime parameters");
        };
        assert!(matches!(&params[0], JuliaType::RuntimeTypeVar { id, .. } if *id == a.id));
        assert!(matches!(&params[1], JuliaType::RuntimeTypeVar { id, .. } if *id == b.id));
        assert!(matches!(&params[2], JuliaType::RuntimeTypeVar { id, .. } if *id == c.id));
    }

    #[test]
    fn projection_owner_preserves_external_ids_and_nested_shadow_depths_issue_10261() {
        use crate::rng::StableRng;

        let mut vm = Vm::new(vec![], StableRng::new(0));
        let external = |id| JuliaType::RuntimeTypeVar {
            id,
            name: "F".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let partial = |free: JuliaType| JuliaType::UnionAll {
            var: "B".to_string(),
            lower_bound: None,
            bound: Some(Box::new("F".to_string())),
            body: Box::new(JuliaType::RuntimeParametric {
                base: "PartialDep".to_string(),
                params: vec![free, JuliaType::Struct("B".to_string())],
            }),
        };
        assert_ne!(
            vm.runtime_typevar_projection_owner_key(&partial(external(90_001))),
            vm.runtime_typevar_projection_owner_key(&partial(external(90_002))),
            "distinct external free TypeVars must remain distinct owner domains"
        );

        let nested_body = JuliaType::Struct("ShadowPair{T, T}".to_string());
        let nested = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                var: "T".to_string(),
                lower_bound: None,
                bound: Some(Box::new("Real".to_string())),
                body: Box::new(nested_body),
            }),
        };
        let JuliaType::UnionAll {
            body: inner_suffix, ..
        } = &nested
        else {
            unreachable!()
        };
        let Value::RuntimeTypeVar(inner_first) = vm.runtime_typevar_value_for_unionall_projection(
            inner_suffix,
            JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
        ) else {
            panic!("inner T must project before its outer wrapper is observed");
        };
        let Value::RuntimeTypeVar(outer) = vm.runtime_typevar_value_for_unionall_projection(
            &nested,
            JuliaType::TypeVar("T".to_string(), None),
        ) else {
            panic!("outer T must project to a RuntimeTypeVar");
        };
        let JuliaType::RuntimeUnionAll { var: inner, .. } = vm.project_unionall_body_with_identity(
            &nested,
            nested.clone().instantiate(&outer.projection()),
            JuliaType::TypeVar("T".to_string(), None),
        ) else {
            panic!("outer body must retain the inner RuntimeUnionAll binder");
        };
        let JuliaType::RuntimeTypeVar { id: inner_id, .. } = inner.as_ref() else {
            panic!("inner T must carry a runtime identity");
        };
        assert_ne!(
            outer.id, inner_first.id,
            "same-name nested binders must not share IDs"
        );
        assert_eq!(
            *inner_id, inner_first.id,
            "inner-first and full-wrapper views must reuse the inner binder ID"
        );

        let nearest = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::UnionAll {
                var: "T".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::UnionAll {
                    var: "T".to_string(),
                    lower_bound: None,
                    bound: Some(Box::new("T".to_string())),
                    body: Box::new(JuliaType::Struct("ShadowTriple{T,T,T}".to_string())),
                }),
            }),
        };
        vm.runtime_typevar_value_for_unionall_projection(
            &nearest,
            JuliaType::TypeVar("T".to_string(), Some("T".to_string())),
        );
        let JuliaType::RuntimeUnionAll {
            var: nearest_outer,
            body,
        } = vm.project_unionall_binders_for_owner(&nearest, &nearest)
        else {
            panic!("outer shadow binder must promote");
        };
        let JuliaType::RuntimeUnionAll {
            var: nearest_middle,
            body,
        } = *body
        else {
            panic!("middle shadow binder must promote");
        };
        let JuliaType::RuntimeUnionAll {
            var: nearest_inner, ..
        } = *body
        else {
            panic!("inner shadow binder must promote");
        };
        let JuliaType::RuntimeTypeVar { id: outer_id, .. } = nearest_outer.as_ref() else {
            unreachable!()
        };
        let JuliaType::RuntimeTypeVar { id: middle_id, .. } = nearest_middle.as_ref() else {
            unreachable!()
        };
        let JuliaType::RuntimeTypeVar {
            upper_bound: inner_upper,
            ..
        } = nearest_inner.as_ref()
        else {
            unreachable!()
        };
        assert_ne!(outer_id, middle_id);
        assert!(
            matches!(inner_upper.as_ref(), JuliaType::RuntimeTypeVar { id, .. } if id == middle_id)
        );
    }

    /// `runtime_typevar_projection_identities`'s key must be the structural
    /// `TypeVarProjectionKey` (Issue #10987): the rendered TypeVar NAME is
    /// display metadata carried on the stored `RuntimeTypeVarValue`, not a
    /// key component. Two owner wrappers that share the exact same final
    /// body -- so the same `CoreType` owner, the same single-binder depth
    /// (0), and the same (absent) declared bounds -- but spell their
    /// (otherwise unused, phantom) outer binder differently, as a
    /// `where`-binder rename would, must resolve to ONE shared runtime
    /// identity. Before this fix, the key's trailing `String` name component
    /// ("Z1" vs "Z2") made these two requests land in separate map entries,
    /// minting two distinct ids for what upstream treats as the same object.
    #[test]
    fn projection_identity_ignores_binder_rename_spelling_issue_10987() {
        use crate::rng::StableRng;

        let mut vm = Vm::new(vec![], StableRng::new(0));
        let body = JuliaType::Struct("PhantomOwner10987".to_string());
        let wrapper_first_spelling = JuliaType::UnionAll {
            var: "Z1".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(body.clone()),
        };
        let wrapper_renamed = JuliaType::UnionAll {
            var: "Z2".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(body.clone()),
        };
        assert_eq!(
            vm.runtime_typevar_projection_owner_key(&wrapper_first_spelling),
            vm.runtime_typevar_projection_owner_key(&wrapper_renamed),
            "identical final body must yield one owner domain regardless of the outer binder's name"
        );

        let Value::RuntimeTypeVar(first) = vm.runtime_typevar_value_for_unionall_projection(
            &wrapper_first_spelling,
            JuliaType::TypeVar("Z1".to_string(), None),
        ) else {
            panic!("Z1 must project to a runtime TypeVar");
        };
        let Value::RuntimeTypeVar(renamed) = vm.runtime_typevar_value_for_unionall_projection(
            &wrapper_renamed,
            JuliaType::TypeVar("Z2".to_string(), None),
        ) else {
            panic!("Z2 must project to a runtime TypeVar");
        };
        assert_eq!(
            first.id, renamed.id,
            "same structural owner+depth+bounds position must share cache identity across a binder rename"
        );
    }

    /// The declared bounds participate in `TypeVarProjectionKey` as PARSED
    /// structural types, not rendered strings (Issue #10987): two spellings
    /// of the same bound (`"Int"` vs `"Int64"`) must hit one cache entry.
    /// Under the pre-#10987 string-keyed shape, the second projection missed
    /// the cache (`"Int" != "Int64"`) and minted a duplicate identity.
    #[test]
    fn projection_identity_parses_bound_spelling_issue_10987() {
        use crate::rng::StableRng;

        let mut vm = Vm::new(vec![], StableRng::new(0));
        let body = JuliaType::Struct("BoundSpell10987{T}".to_string());
        let spelled_int = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: Some(Box::new("Int".to_string())),
            body: Box::new(body.clone()),
        };
        let spelled_int64 = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: Some(Box::new("Int64".to_string())),
            body: Box::new(body),
        };
        let Value::RuntimeTypeVar(first) = vm.runtime_typevar_value_for_unionall_projection(
            &spelled_int,
            JuliaType::TypeVar("T".to_string(), Some("Int".to_string())),
        ) else {
            panic!("T<:Int must project to a runtime TypeVar");
        };
        let Value::RuntimeTypeVar(second) = vm.runtime_typevar_value_for_unionall_projection(
            &spelled_int64,
            JuliaType::TypeVar("T".to_string(), Some("Int64".to_string())),
        ) else {
            panic!("T<:Int64 must project to a runtime TypeVar");
        };
        assert_eq!(
            first.id, second.id,
            "differently-spelled but structurally identical bounds must share one identity"
        );
    }

    /// Adversarial guard for the #10987 key narrowing: the owner key is
    /// derived from the wrapper's FINAL BODY, so two DISTINCT wrappers with
    /// different declared bounds can share `(owner, binder_depth)` -- e.g.
    /// `Vector{Int64} where Int64>:Signed` vs
    /// `Vector{Int64} where Signed<:Int64<:Real` (the
    /// `where_binder_shadow_scope_10100.jl` shapes). Their binders are
    /// distinct objects upstream; collapsing them (as a bounds-free
    /// `(owner, depth)` key would) makes the second wrapper's `.var` report
    /// the first wrapper's bounds. The structural `declared_lower`/
    /// `declared_upper` key components must keep them distinct.
    #[test]
    fn projection_identity_distinguishes_declared_bounds_issue_10987() {
        use crate::rng::StableRng;

        let mut vm = Vm::new(vec![], StableRng::new(0));
        let body = JuliaType::Struct("DistinctBounds10987{T}".to_string());
        let upper_only = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: Some(Box::new("Real".to_string())),
            body: Box::new(body.clone()),
        };
        let two_sided = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: Some(Box::new("Signed".to_string())),
            bound: Some(Box::new("Real".to_string())),
            body: Box::new(body),
        };
        let Value::RuntimeTypeVar(upper_only_tv) = vm
            .runtime_typevar_value_for_unionall_projection(
                &upper_only,
                JuliaType::TypeVar("T".to_string(), Some("Real".to_string())),
            )
        else {
            panic!("T<:Real must project to a runtime TypeVar");
        };
        let Value::RuntimeTypeVar(two_sided_tv) = vm.runtime_typevar_value_for_unionall_projection(
            &two_sided,
            JuliaType::TypeVar("T".to_string(), Some("Signed<:T<:Real".to_string())),
        ) else {
            panic!("Signed<:T<:Real must project to a runtime TypeVar");
        };
        assert_ne!(
            upper_only_tv.id, two_sided_tv.id,
            "same rendered body but different declared bounds are distinct binder objects"
        );
        assert_eq!(upper_only_tv.lower_bound, JuliaType::Bottom);
        assert_eq!(
            two_sided_tv.lower_bound,
            JuliaType::from_name_or_struct("Signed"),
            "the two-sided binder must keep its own declared lower bound"
        );
    }

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
    fn collect_call_arities_follows_direct_call_keyword_values_issue_8441() {
        let span = test_span();
        let expr = Expr::Call {
            function: "outer_kw_8441".to_string().into(),
            args: vec![Expr::Var("x".to_string().into(), span)],
            kwargs: vec![(
                "y".to_string().into(),
                Expr::Call {
                    function: "inner_kw_8441".to_string().into(),
                    args: vec![Expr::Var("z".to_string().into(), span)],
                    kwargs: vec![],
                    splat_mask: vec![false],
                    kwargs_splat_mask: vec![],
                    span,
                },
            )],
            splat_mask: vec![false],
            kwargs_splat_mask: vec![false],
            span,
        };
        let mut arities = HashMap::new();

        collect_call_arities_from_expr(&expr, &mut arities);

        assert_eq!(arities.get("outer_kw_8441"), Some(&HashSet::from([1usize])));
        assert_eq!(arities.get("inner_kw_8441"), Some(&HashSet::from([1usize])));
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

    #[test]
    fn single_character_value_parameter_is_not_a_typevar_10613() {
        assert!(is_single_char_typevar_name("T"));
        assert!(is_single_char_typevar_name("_"));
        assert!(!is_single_char_typevar_name("2"));
    }

    #[test]
    fn reflected_source_typevar_bounds_preserve_intervals_10613() {
        assert_eq!(
            reflected_typevar_bounds("T", Some(">:Signed")),
            (JuliaType::Signed, JuliaType::Any)
        );
        assert_eq!(
            reflected_typevar_bounds("T", Some("Signed<:T<:Real")),
            (JuliaType::Signed, JuliaType::Real)
        );
        assert_eq!(
            reflected_typevar_bounds("T", Some("Real")),
            (JuliaType::Bottom, JuliaType::Real)
        );
    }
}
