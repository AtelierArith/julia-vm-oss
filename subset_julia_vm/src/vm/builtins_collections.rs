//! Collection-related builtin execution.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{ArrayElementType, Value};
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
                    let type_name = self.get_type_name(&args[0]);
                    return Err(VmError::MethodError(format!(
                        "no method matching length({})",
                        type_name
                    )));
                }
                let len = match &val {
                    Value::Array(arr) => arr.borrow().element_count() as i64,
                    Value::Tuple(items) => items.len() as i64,
                    Value::NamedTuple(nt) => nt.values.len() as i64,
                    Value::Dict(dict) => dict.len() as i64,
                    Value::Set(set) => set.len() as i64,
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
                    Value::Str(s) => s.chars().count() as i64,
                    Value::Pairs(p) => p.data.values.len() as i64,
                    Value::Memory(mem) => mem.borrow().len() as i64,
                    Value::Generator(g) => match g.iter.as_ref() {
                        Value::Array(arr) => arr.borrow().element_count() as i64,
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
                        Value::Tuple(t) => t.len() as i64,
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "length not defined for Generator's underlying iterator {:?}",
                                g.iter
                            )))
                        }
                    },
                    Value::I64(_)
                    | Value::I32(_)
                    | Value::I16(_)
                    | Value::I8(_)
                    | Value::I128(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::F64(_)
                    | Value::F32(_)
                    | Value::F16(_)
                    | Value::Bool(_) => 1,
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
                let element_type = match &val {
                    Value::Array(arr) => {
                        let arr_borrow = arr.borrow();
                        match arr_borrow.data.element_type() {
                            ArrayElementType::F32 => crate::types::JuliaType::Float32,
                            ArrayElementType::F64 => crate::types::JuliaType::Float64,
                            ArrayElementType::ComplexF32 => {
                                crate::types::JuliaType::Struct("Complex{Float32}".to_string())
                            }
                            ArrayElementType::ComplexF64 => {
                                crate::types::JuliaType::Struct("Complex{Float64}".to_string())
                            }
                            ArrayElementType::I8 => crate::types::JuliaType::Int8,
                            ArrayElementType::I16 => crate::types::JuliaType::Int16,
                            ArrayElementType::I32 => crate::types::JuliaType::Int32,
                            ArrayElementType::I64 => crate::types::JuliaType::Int64,
                            ArrayElementType::U8 => crate::types::JuliaType::UInt8,
                            ArrayElementType::U16 => crate::types::JuliaType::UInt16,
                            ArrayElementType::U32 => crate::types::JuliaType::UInt32,
                            ArrayElementType::U64 => crate::types::JuliaType::UInt64,
                            ArrayElementType::Bool => crate::types::JuliaType::Bool,
                            ArrayElementType::String => crate::types::JuliaType::String,
                            ArrayElementType::Char => crate::types::JuliaType::Char,
                            ArrayElementType::Struct => crate::types::JuliaType::Any,
                            ArrayElementType::StructOf(type_id) => {
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            ArrayElementType::StructInlineOf(type_id, _) => {
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            ArrayElementType::Any => crate::types::JuliaType::Any,
                            ArrayElementType::TupleOf(ref field_types) => {
                                let type_names: Vec<std::string::String> = field_types
                                    .iter()
                                    .map(|ft| match ft {
                                        ArrayElementType::I64 => "Int64".to_string(),
                                        ArrayElementType::F64 => "Float64".to_string(),
                                        ArrayElementType::Bool => "Bool".to_string(),
                                        ArrayElementType::String => "String".to_string(),
                                        _ => "Any".to_string(),
                                    })
                                    .collect();
                                crate::types::JuliaType::Struct(format!(
                                    "Tuple{{{}}}",
                                    type_names.join(", ")
                                ))
                            }
                        }
                    }
                    Value::Tuple(t) if matches!(builtin, BuiltinId::Eltype) => {
                        if t.elements.is_empty() {
                            crate::types::JuliaType::Any
                        } else {
                            let first_type = t.elements[0].runtime_type();
                            let all_same = t.elements.iter().all(|e| e.runtime_type() == first_type);
                            if all_same {
                                first_type
                            } else {
                                crate::types::JuliaType::Any
                            }
                        }
                    }
                    Value::Range(_) if matches!(builtin, BuiltinId::Eltype) => {
                        crate::types::JuliaType::Float64
                    }
                    Value::Dict(_) if matches!(builtin, BuiltinId::Eltype) => {
                        crate::types::JuliaType::Any
                    }
                    Value::Str(_) => crate::types::JuliaType::Char,
                    _ => crate::types::JuliaType::Any,
                };
                self.stack.push(Value::DataType(element_type));
                Ok(Some(()))
            }
            _ => Ok(None),
        }
    }
}
