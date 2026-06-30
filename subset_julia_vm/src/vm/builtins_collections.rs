//! Collection-related builtin execution.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;
use crate::vm::value::is_native_array_value;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_utils::type_values_subtype;
use super::value::{
    array_wrapper_value_to_array_value, is_scalar_carrier, native_array_value_ref, MemoryRefValue,
    Value,
};
use super::Vm;

impl<R: RngLike> Vm<R> {
    pub(super) fn execute_builtin_collections(
        &mut self,
        builtin: &BuiltinId,
        _argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::Length => {
                let val = self.stack.pop_value()?;
                if matches!(val, Value::Struct(_) | Value::StructRef(_)) {
                    let args = vec![val];
                    if let Some(func_index) =
                        self.find_best_method_index(&["length", "Base.length"], &args)
                    {
                        self.start_function_call(func_index, args)?;
                        return Ok(Some(()));
                    }
                    // Native fallback for a MemoryRef-backed `Array{T,N}` wrapper
                    // when no user/Base `length` method is available (e.g. a bare
                    // VM without Base loaded): count elements directly from the
                    // wrapper storage. Gated on the dispatch miss so a user
                    // `length` override still wins when present, and a no-op for
                    // Base-loaded programs where `length(::AbstractArray)` exists
                    // (Issue #6807).
                    if let Some(arr) =
                        array_wrapper_value_to_array_value(&args[0], &self.struct_heap)?
                    {
                        self.stack.push(Value::I64(arr.element_count() as i64));
                        return Ok(Some(()));
                    }
                    let type_name = self.get_type_name(&args[0]);
                    return Err(VmError::MethodError(format!(
                        "no method matching length({})",
                        type_name
                    )));
                }
                let len = match &val {
                    _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                        Some(arr) => arr.borrow().element_count() as i64,
                        None => 0,
                    },
                    Value::Tuple(items) => items.len() as i64,
                    // Core.SimpleVector length (Issue #4722).
                    Value::SimpleVector(items) => items.len() as i64,
                    Value::NamedTuple(nt) => nt.values.len() as i64,
                    Value::Range(r) => r.length(),
                    Value::Str(s) => s.chars().count() as i64,
                    Value::Pairs(p) => p.data.values.len() as i64,
                    Value::Memory(mem) => mem.borrow().len() as i64,
                    // Issue #7964: flat static-array reps.
                    Value::StaticArray(sv) => sv.len() as i64,
                    Value::StaticArrayInline(sv) => sv.len() as i64,
                    Value::Generator(g) => match g.iter.as_ref() {
                        inner if is_native_array_value(inner) => {
                            match native_array_value_ref(inner) {
                                Some(arr) => arr.borrow().element_count() as i64,
                                None => 0,
                            }
                        }
                        Value::Range(r) => r.length(),
                        Value::Tuple(t) => t.len() as i64,
                        inner @ (Value::Struct(_) | Value::StructRef(_)) => {
                            if let Some(arr) =
                                array_wrapper_value_to_array_value(inner, &self.struct_heap)?
                            {
                                arr.element_count() as i64
                            } else {
                                return Err(VmError::TypeError(format!(
                                    "length not defined for Generator's underlying iterator {:?}",
                                    g.iter
                                )));
                            }
                        }
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "length not defined for Generator's underlying iterator {:?}",
                                g.iter
                            )))
                        }
                    },
                    // Issue #4814 / #4871 / #4875: scalars in upstream Julia
                    // behave as 0-dimensional collections — `length(x) == 1`
                    // for every `Number` and `AbstractChar` subtype. The
                    // carrier predicate lives in `vm/value/predicates.rs`
                    // so this stays in lock-step with the `IndexLoad`
                    // scalar arm and any future scalar-aware builtin.
                    other if is_scalar_carrier(other) => 1,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "length not defined for {:?}",
                            val
                        )))
                    }
                };
                self.stack.push(Value::I64(len));
                Ok(Some(()))
            }
            BuiltinId::Eltype | BuiltinId::_Eltype => {
                let val = self.stack.pop_value()?;
                if matches!(builtin, BuiltinId::Eltype) {
                    if let Value::DataType(jt) = &val {
                        let element_type = datatype_eltype(jt);
                        if !matches!(element_type, crate::types::JuliaType::Any) {
                            self.stack.push(Value::DataType(Box::new(element_type)));
                            return Ok(Some(()));
                        }
                    }
                }
                if matches!(builtin, BuiltinId::Eltype) {
                    let direct_args = if matches!(
                        val,
                        Value::Struct(_) | Value::StructRef(_) | Value::DataType(_)
                    ) {
                        Some(vec![val.clone()])
                    } else {
                        None
                    };
                    if let Some(args) = direct_args {
                        if let Some(func_index) =
                            self.find_best_method_index(&["eltype", "Base.eltype"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                    }
                    if matches!(val, Value::Struct(_) | Value::StructRef(_)) {
                        let args = vec![Value::DataType(Box::new(self.get_value_julia_type(&val)))];
                        if let Some(func_index) =
                            self.find_best_method_index(&["eltype", "Base.eltype"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                    }
                }
                if matches!(builtin, BuiltinId::Eltype)
                    && matches!(val, Value::Struct(_) | Value::StructRef(_))
                {
                    self.stack
                        .push(Value::DataType(Box::new(crate::types::JuliaType::Any)));
                    return Ok(Some(()));
                }
                let element_type = match &val {
                    _ if is_native_array_value(&val) => match native_array_value_ref(&val) {
                        Some(arr) => {
                            let arr_borrow = arr.borrow();
                            self.array_value_declared_element_julia_type(&arr_borrow)
                        }
                        None => crate::types::JuliaType::Any,
                    },
                    Value::DataType(jt) if matches!(builtin, BuiltinId::Eltype) => {
                        datatype_eltype(jt)
                    }
                    Value::Memory(mem) if matches!(builtin, BuiltinId::Eltype) => {
                        let elem_type_name = mem.borrow().element_type().julia_type_name();
                        crate::types::JuliaType::from_name_or_struct(&elem_type_name)
                    }
                    Value::Tuple(t) if matches!(builtin, BuiltinId::Eltype) => {
                        if t.elements.is_empty() {
                            crate::types::JuliaType::Any
                        } else {
                            let first_type = t.elements[0].runtime_type();
                            let all_same =
                                t.elements.iter().all(|e| e.runtime_type() == first_type);
                            if all_same {
                                first_type
                            } else {
                                crate::types::JuliaType::Any
                            }
                        }
                    }
                    // Issue #5196: upstream `eltype(::Core.SimpleVector)` is always
                    // `Any` (it does not narrow to the common element type the way
                    // a Tuple does). Returning `Any` here keeps the `collect` path
                    // building a `Vector{Any}` regardless of element homogeneity.
                    Value::SimpleVector(_) if matches!(builtin, BuiltinId::Eltype) => {
                        crate::types::JuliaType::Any
                    }
                    Value::Bool(_)
                    | Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
                    | Value::I64(_)
                    | Value::I128(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::F16(_)
                    | Value::F32(_)
                    | Value::F64(_)
                    | Value::BigInt(_)
                    | Value::BigFloat(_)
                        if matches!(builtin, BuiltinId::Eltype) =>
                    {
                        // Upstream Base defines eltype(::Type{T}) where T<:Number = T
                        // and the value fallback delegates through typeof(x). Generic
                        // sjulia call sites can reach this builtin fallback (Issue #4665).
                        val.runtime_type()
                    }
                    Value::Pairs(p)
                        if matches!(builtin, BuiltinId::Eltype | BuiltinId::_Eltype) =>
                    {
                        crate::types::JuliaType::Struct(format!(
                            "Pair{{Symbol, {}}}",
                            self.pairs_value_element_type_name(&p.data.values)
                        ))
                    }
                    Value::Range(r) if matches!(builtin, BuiltinId::Eltype) => {
                        // Issue #3550: respect the range's typed element tag.
                        match r.element_type {
                            crate::vm::value::RangeElementType::Default => {
                                if r.is_float {
                                    crate::types::JuliaType::Float64
                                } else {
                                    crate::types::JuliaType::Int64
                                }
                            }
                            crate::vm::value::RangeElementType::Int8 => {
                                crate::types::JuliaType::Int8
                            }
                            crate::vm::value::RangeElementType::Int16 => {
                                crate::types::JuliaType::Int16
                            }
                            crate::vm::value::RangeElementType::Int32 => {
                                crate::types::JuliaType::Int32
                            }
                            crate::vm::value::RangeElementType::Int64 => {
                                crate::types::JuliaType::Int64
                            }
                            crate::vm::value::RangeElementType::UInt8 => {
                                crate::types::JuliaType::UInt8
                            }
                            crate::vm::value::RangeElementType::UInt16 => {
                                crate::types::JuliaType::UInt16
                            }
                            crate::vm::value::RangeElementType::UInt32 => {
                                crate::types::JuliaType::UInt32
                            }
                            crate::vm::value::RangeElementType::UInt64 => {
                                crate::types::JuliaType::UInt64
                            }
                            crate::vm::value::RangeElementType::Float32 => {
                                crate::types::JuliaType::Float32
                            }
                            crate::vm::value::RangeElementType::Float64 => {
                                crate::types::JuliaType::Float64
                            }
                            crate::vm::value::RangeElementType::Char => {
                                crate::types::JuliaType::Char
                            }
                        }
                    }
                    // Static arrays (StaticArrays.jl SVector/SMatrix/SArray):
                    // report the concrete element type from the flat carrier's
                    // type tag instead of widening to `Any`. Without this, a
                    // statically-`Any` static-array value reaching this builtin
                    // (e.g. `eltype(itr)` inside the generic `_collect`) produced
                    // `Vector{Any}` for `collect(SVector(1,2,3))` (Issue #8131).
                    Value::StaticArrayInline(sv)
                        if matches!(builtin, BuiltinId::Eltype | BuiltinId::_Eltype) =>
                    {
                        crate::types::JuliaType::from_name_or_struct(sv.tag.julia_name())
                    }
                    Value::StaticArray(sv)
                        if matches!(builtin, BuiltinId::Eltype | BuiltinId::_Eltype) =>
                    {
                        crate::types::JuliaType::from_name_or_struct(sv.elems.element_type_name())
                    }
                    Value::Str(_) => crate::types::JuliaType::Char,
                    _ => crate::types::JuliaType::Any,
                };
                self.stack.push(Value::DataType(Box::new(element_type)));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefNew => {
                let _check_bounds = if _argc >= 3 {
                    matches!(self.stack.pop_value()?, Value::Bool(true))
                } else {
                    true
                };
                let index = if _argc >= 2 {
                    match self.stack.pop_value()? {
                        Value::I64(i) if i >= 1 => usize::try_from(i).map_err(|_| {
                            VmError::TypeError(format!(
                                "memoryref: index out of range for usize, got {}",
                                i
                            ))
                        })?,
                        Value::U64(i) if i >= 1 => usize::try_from(i).map_err(|_| {
                            VmError::TypeError(format!(
                                "memoryref: index out of range for usize, got {}",
                                i
                            ))
                        })?,
                        other => {
                            return Err(VmError::TypeError(format!(
                                "memoryref: expected positive integer index, got {:?}",
                                other
                            )))
                        }
                    }
                } else {
                    1
                };
                let target = self.stack.pop_value()?;
                let memref = match target {
                    Value::Memory(mem) => MemoryRefValue::new(mem, index)?,
                    Value::MemoryRef(memref) => {
                        let parent = memref.parent();
                        let new_index = memref.memory_index().saturating_add(index - 1);
                        MemoryRefValue::new(parent, new_index)?
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryref: expected Memory or MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(Value::MemoryRef(Box::new(memref)));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefGet => {
                for _ in 1.._argc {
                    let _ = self.stack.pop_value()?;
                }
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefget: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(memref.get(1)?);
                Ok(Some(()))
            }
            BuiltinId::MemoryRefSet => {
                for _ in 2.._argc {
                    let _ = self.stack.pop_value()?;
                }
                let value = self.stack.pop_value()?;
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefset!: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                memref.set(1, value)?;
                self.stack.push(Value::MemoryRef(memref));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefOffset => {
                if _argc != 1 {
                    return Err(VmError::TypeError(
                        "memoryrefoffset requires exactly 1 argument".to_string(),
                    ));
                }
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefoffset: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(Value::I64(memref.memory_index() as i64));
                Ok(Some(()))
            }
            BuiltinId::MemoryRefParent => {
                if _argc != 1 {
                    return Err(VmError::TypeError(
                        "memoryrefparent requires exactly 1 argument".to_string(),
                    ));
                }
                let memref = match self.stack.pop_value()? {
                    Value::MemoryRef(memref) => memref,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "memoryrefparent: expected MemoryRef, got {:?}",
                            other
                        )))
                    }
                };
                self.stack.push(Value::Memory(memref.parent()));
                Ok(Some(()))
            }
            _ => Ok(None),
        }
    }
}

pub(in crate::vm) fn datatype_eltype(jt: &crate::types::JuliaType) -> crate::types::JuliaType {
    use crate::types::JuliaType;

    match jt {
        JuliaType::VectorOf(elem) | JuliaType::MatrixOf(elem) => {
            resolve_typevar_bound(elem.as_ref(), jt)
        }
        JuliaType::UnionAll { body, .. } => datatype_eltype(body),
        JuliaType::String => JuliaType::Char,
        numeric if type_values_subtype(numeric, &JuliaType::Number) => numeric.clone(),
        JuliaType::Struct(name)
            if name.starts_with("Vector{")
                || name.starts_with("Matrix{")
                || name.starts_with("Array{")
                || name.starts_with("Set{") =>
        {
            parse_first_type_parameter(name).unwrap_or(JuliaType::Any)
        }
        _ => JuliaType::Any,
    }
}

fn resolve_typevar_bound(
    candidate: &crate::types::JuliaType,
    owner: &crate::types::JuliaType,
) -> crate::types::JuliaType {
    use crate::types::JuliaType;

    match (candidate, owner) {
        (
            JuliaType::TypeVar(name, candidate_bound),
            JuliaType::UnionAll {
                var,
                lower_bound: _,
                bound,
                body: _,
            },
        ) if name == var => bound
            .as_ref()
            .map(|b| b.as_str())
            .or(candidate_bound.as_deref())
            .map(JuliaType::from_name_or_struct)
            .unwrap_or(JuliaType::Any),
        (JuliaType::TypeVar(_, bound), _) => bound
            .as_deref()
            .map(JuliaType::from_name_or_struct)
            .unwrap_or(JuliaType::Any),
        _ => candidate.clone(),
    }
}

fn parse_first_type_parameter(type_name: &str) -> Option<crate::types::JuliaType> {
    let open = type_name.find('{')?;
    let close = type_name.rfind('}')?;
    if close <= open + 1 {
        return None;
    }

    let inner = &type_name[open + 1..close];
    let first = split_top_level_params(inner).into_iter().next()?;
    let trimmed = first.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(crate::types::JuliaType::from_name_or_struct(trimmed))
    }
}

fn split_top_level_params(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[start..idx].to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(s[start..].to_string());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::StableRng;
    use crate::types::JuliaType;
    use crate::vm::{FunctionInfo, StructInstance, ValueType};
    use std::rc::Rc;

    fn add_eltype_method(
        vm: &mut Vm<StableRng>,
        param_julia_type: JuliaType,
        entry: usize,
    ) -> usize {
        let idx = vm.functions.len();
        vm.functions.push(Rc::new(FunctionInfo {
            name: "Base.eltype".to_string(),
            params: vec![("x".to_string(), ValueType::DataType)],
            kwparams: vec![],
            entry,
            return_type: ValueType::DataType,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            min_world: 1,
            type_params: vec![],
            param_julia_types: vec![param_julia_type],
            code_start: entry,
            code_end: entry,
            slot_names: vec!["x".to_string()],
            slot_types: vec![Some(crate::vm::VarTypeTag::Any)],
            local_slot_count: 1,
            param_slots: vec![0],
            vararg_param_index: None,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            // Builtin stub FunctionInfo: no source line (Issue #5125).
            def_line: 0,
        }));
        vm.function_name_index
            .entry("Base.eltype".to_string())
            .or_default()
            .push(idx);
        idx
    }

    #[test]
    fn eltype_type_object_dispatches_before_datatype_fallback() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let method_idx = add_eltype_method(
            &mut vm,
            JuliaType::TypeOf(Box::new(JuliaType::Struct("MyIter".to_string()))),
            17,
        );

        vm.stack.push(Value::DataType(Box::new(JuliaType::Struct(
            "MyIter".to_string(),
        ))));

        assert!(matches!(
            vm.execute_builtin_collections(&BuiltinId::Eltype, 1),
            Ok(Some(()))
        ));
        assert_eq!(vm.ip, 17);
        assert_eq!(
            vm.frames.last().and_then(|frame| frame.func_index),
            Some(method_idx)
        );
        assert!(vm.stack.is_empty());
    }

    #[test]
    fn eltype_struct_value_falls_back_to_type_object_method() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let method_idx = add_eltype_method(
            &mut vm,
            JuliaType::TypeOf(Box::new(JuliaType::Struct("MyIter".to_string()))),
            23,
        );

        vm.stack.push(Value::Struct(StructInstance::with_name(
            0,
            "MyIter".to_string(),
            vec![],
        )));

        assert!(matches!(
            vm.execute_builtin_collections(&BuiltinId::Eltype, 1),
            Ok(Some(()))
        ));
        assert_eq!(vm.ip, 23);
        assert_eq!(
            vm.frames.last().and_then(|frame| frame.func_index),
            Some(method_idx)
        );
        assert!(vm.stack.is_empty());
    }
}
