//! Type builtin functions for the VM.
//!
//! Type operations: typeof, isa, convert, promote, subtype checks.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::builtins::BuiltinId;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::type_utils::normalize_type_for_isa;
use super::value::{ArrayData, DictKey, TupleValue, Value};
use super::{new_array_ref, Vm};

impl<R: crate::rng::RngLike> Vm<R> {
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
            BuiltinId::TypeOf => {
                let val = self.stack.pop_value()?;

                // Check if this is a type name literal (from lowered ParametrizedTypeExpression)
                if let Value::Str(type_name_str) = &val {
                    if let Some(parsed_type) = crate::types::JuliaType::from_name(type_name_str) {
                        self.stack.push(Value::DataType(parsed_type));
                        return Ok(Some(()));
                    }
                    if type_name_str.contains('{')
                        && type_name_str
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                    {
                        let parsed_type =
                            crate::types::JuliaType::from_name_or_struct(type_name_str);
                        self.stack.push(Value::DataType(parsed_type));
                        return Ok(Some(()));
                    }
                }

                let julia_type = match &val {
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            // Special case: Generator struct maps to Base.Generator type
                            if s.struct_name == "Generator" {
                                crate::types::JuliaType::Generator
                            } else {
                                crate::types::JuliaType::Struct(s.struct_name.clone())
                            }
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                    Value::Struct(s) => {
                        if s.struct_name.is_empty() {
                            crate::types::JuliaType::Any
                        } else if s.struct_name == "Generator" {
                            // Special case: Generator struct maps to Base.Generator type
                            crate::types::JuliaType::Generator
                        } else {
                            crate::types::JuliaType::Struct(s.struct_name.clone())
                        }
                    }
                    Value::Array(arr) => {
                        let arr_borrow = arr.borrow();
                        let elem_type = match arr_borrow.data.element_type() {
                            super::value::ArrayElementType::F32 => crate::types::JuliaType::Float32,
                            super::value::ArrayElementType::F64 => crate::types::JuliaType::Float64,
                            super::value::ArrayElementType::ComplexF32 => {
                                crate::types::JuliaType::Struct("Complex{Float32}".to_string())
                            }
                            super::value::ArrayElementType::ComplexF64 => {
                                crate::types::JuliaType::Struct("Complex{Float64}".to_string())
                            }
                            super::value::ArrayElementType::I8 => crate::types::JuliaType::Int8,
                            super::value::ArrayElementType::I16 => crate::types::JuliaType::Int16,
                            super::value::ArrayElementType::I32 => crate::types::JuliaType::Int32,
                            super::value::ArrayElementType::I64 => crate::types::JuliaType::Int64,
                            super::value::ArrayElementType::U8 => crate::types::JuliaType::UInt8,
                            super::value::ArrayElementType::U16 => crate::types::JuliaType::UInt16,
                            super::value::ArrayElementType::U32 => crate::types::JuliaType::UInt32,
                            super::value::ArrayElementType::U64 => crate::types::JuliaType::UInt64,
                            super::value::ArrayElementType::Bool => crate::types::JuliaType::Bool,
                            super::value::ArrayElementType::String => {
                                crate::types::JuliaType::String
                            }
                            super::value::ArrayElementType::Char => crate::types::JuliaType::Char,
                            super::value::ArrayElementType::StructOf(type_id) => {
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            super::value::ArrayElementType::StructInlineOf(type_id, _) => {
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            super::value::ArrayElementType::Struct => {
                                if let ArrayData::StructRefs(ref struct_refs) = arr_borrow.data {
                                    if let Some(&first_ref_idx) = struct_refs.first() {
                                        if let Some(struct_instance) =
                                            self.struct_heap.get(first_ref_idx)
                                        {
                                            crate::types::JuliaType::Struct(
                                                struct_instance.struct_name.clone(),
                                            )
                                        } else {
                                            crate::types::JuliaType::Any
                                        }
                                    } else {
                                        crate::types::JuliaType::Any
                                    }
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            super::value::ArrayElementType::Any => crate::types::JuliaType::Any,
                            super::value::ArrayElementType::TupleOf(_) => {
                                crate::types::JuliaType::Any
                            }
                        };
                        match arr_borrow.shape.len() {
                            1 => crate::types::JuliaType::VectorOf(Box::new(elem_type)),
                            2 => crate::types::JuliaType::MatrixOf(Box::new(elem_type)),
                            _ => crate::types::JuliaType::Array,
                        }
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = crate::vm::util::memory_to_array_ref(mem);
                        let arr_borrow = arr.borrow();
                        let elem_type = match arr_borrow.data.element_type() {
                            super::value::ArrayElementType::F32 => crate::types::JuliaType::Float32,
                            super::value::ArrayElementType::F64 => crate::types::JuliaType::Float64,
                            super::value::ArrayElementType::ComplexF32 => {
                                crate::types::JuliaType::Struct("Complex{Float32}".to_string())
                            }
                            super::value::ArrayElementType::ComplexF64 => {
                                crate::types::JuliaType::Struct("Complex{Float64}".to_string())
                            }
                            super::value::ArrayElementType::I8 => crate::types::JuliaType::Int8,
                            super::value::ArrayElementType::I16 => crate::types::JuliaType::Int16,
                            super::value::ArrayElementType::I32 => crate::types::JuliaType::Int32,
                            super::value::ArrayElementType::I64 => crate::types::JuliaType::Int64,
                            super::value::ArrayElementType::U8 => crate::types::JuliaType::UInt8,
                            super::value::ArrayElementType::U16 => crate::types::JuliaType::UInt16,
                            super::value::ArrayElementType::U32 => crate::types::JuliaType::UInt32,
                            super::value::ArrayElementType::U64 => crate::types::JuliaType::UInt64,
                            super::value::ArrayElementType::Bool => crate::types::JuliaType::Bool,
                            super::value::ArrayElementType::String => {
                                crate::types::JuliaType::String
                            }
                            super::value::ArrayElementType::Char => crate::types::JuliaType::Char,
                            super::value::ArrayElementType::StructOf(type_id) => {
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            super::value::ArrayElementType::StructInlineOf(type_id, _) => {
                                if let Some(def) = self.struct_defs.get(type_id) {
                                    crate::types::JuliaType::Struct(def.name.clone())
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            super::value::ArrayElementType::Struct => {
                                if let ArrayData::StructRefs(ref struct_refs) = arr_borrow.data {
                                    if let Some(&first_ref_idx) = struct_refs.first() {
                                        if let Some(struct_instance) =
                                            self.struct_heap.get(first_ref_idx)
                                        {
                                            crate::types::JuliaType::Struct(
                                                struct_instance.struct_name.clone(),
                                            )
                                        } else {
                                            crate::types::JuliaType::Any
                                        }
                                    } else {
                                        crate::types::JuliaType::Any
                                    }
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            super::value::ArrayElementType::Any => crate::types::JuliaType::Any,
                            super::value::ArrayElementType::TupleOf(_) => {
                                crate::types::JuliaType::Any
                            }
                        };
                        match arr_borrow.shape.len() {
                            1 => crate::types::JuliaType::VectorOf(Box::new(elem_type)),
                            2 => crate::types::JuliaType::MatrixOf(Box::new(elem_type)),
                            _ => crate::types::JuliaType::Array,
                        }
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
                    Value::Dict(d) => {
                        // Return Dict{K, V} with type parameters (Issue #2742)
                        let k = d.key_type.as_deref().unwrap_or("Any");
                        let v = d.value_type.as_deref().unwrap_or("Any");
                        crate::types::JuliaType::Struct(format!("Dict{{{}, {}}}", k, v))
                    }
                    _ => val.runtime_type(),
                };
                self.stack.push(Value::DataType(julia_type));
            }

            BuiltinId::Isa => {
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

                let (struct_name_opt, resolved_val_type) = match &val {
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            (
                                Some(si.struct_name.clone()),
                                crate::types::JuliaType::Struct(si.struct_name.clone()),
                            )
                        } else {
                            (None, crate::types::JuliaType::Any)
                        }
                    }
                    Value::Struct(si) => (
                        Some(si.struct_name.clone()),
                        crate::types::JuliaType::Struct(si.struct_name.clone()),
                    ),
                    Value::Array(arr) => {
                        let arr_borrow = arr.borrow();
                        let elem_type = match &arr_borrow.data {
                            ArrayData::F32(_) => crate::types::JuliaType::Float32,
                            ArrayData::F64(_) => crate::types::JuliaType::Float64,
                            ArrayData::I8(_) => crate::types::JuliaType::Int8,
                            ArrayData::I16(_) => crate::types::JuliaType::Int16,
                            ArrayData::I32(_) => crate::types::JuliaType::Int32,
                            ArrayData::I64(_) => crate::types::JuliaType::Int64,
                            ArrayData::U8(_) => crate::types::JuliaType::UInt8,
                            ArrayData::U16(_) => crate::types::JuliaType::UInt16,
                            ArrayData::U32(_) => crate::types::JuliaType::UInt32,
                            ArrayData::U64(_) => crate::types::JuliaType::UInt64,
                            ArrayData::Bool(_) => crate::types::JuliaType::Bool,
                            ArrayData::String(_) => crate::types::JuliaType::String,
                            ArrayData::Char(_) => crate::types::JuliaType::Char,
                            ArrayData::StructRefs(refs) => {
                                if let Some(type_id) = arr_borrow.struct_type_id {
                                    self.struct_defs
                                        .get(type_id)
                                        .map(|def| {
                                            crate::types::JuliaType::Struct(def.name.clone())
                                        })
                                        .unwrap_or(crate::types::JuliaType::Any)
                                } else if let Some(&first_ref_idx) = refs.first() {
                                    self.struct_heap
                                        .get(first_ref_idx)
                                        .map(|s| {
                                            crate::types::JuliaType::Struct(s.struct_name.clone())
                                        })
                                        .unwrap_or(crate::types::JuliaType::Any)
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            ArrayData::Any(values) => {
                                if let Some(first) = values.first() {
                                    match first {
                                        Value::StructRef(idx) => self
                                            .struct_heap
                                            .get(*idx)
                                            .map(|s| {
                                                crate::types::JuliaType::Struct(
                                                    s.struct_name.clone(),
                                                )
                                            })
                                            .unwrap_or(crate::types::JuliaType::Any),
                                        Value::Struct(s) => {
                                            crate::types::JuliaType::Struct(s.struct_name.clone())
                                        }
                                        _ => crate::types::JuliaType::Any,
                                    }
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                        };

                        let array_type = match arr_borrow.shape.len() {
                            1 => crate::types::JuliaType::VectorOf(Box::new(elem_type)),
                            2 => crate::types::JuliaType::MatrixOf(Box::new(elem_type)),
                            _ => crate::types::JuliaType::Array,
                        };
                        (None, array_type)
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = crate::vm::util::memory_to_array_ref(mem);
                        let arr_borrow = arr.borrow();
                        let elem_type = match &arr_borrow.data {
                            ArrayData::F32(_) => crate::types::JuliaType::Float32,
                            ArrayData::F64(_) => crate::types::JuliaType::Float64,
                            ArrayData::I8(_) => crate::types::JuliaType::Int8,
                            ArrayData::I16(_) => crate::types::JuliaType::Int16,
                            ArrayData::I32(_) => crate::types::JuliaType::Int32,
                            ArrayData::I64(_) => crate::types::JuliaType::Int64,
                            ArrayData::U8(_) => crate::types::JuliaType::UInt8,
                            ArrayData::U16(_) => crate::types::JuliaType::UInt16,
                            ArrayData::U32(_) => crate::types::JuliaType::UInt32,
                            ArrayData::U64(_) => crate::types::JuliaType::UInt64,
                            ArrayData::Bool(_) => crate::types::JuliaType::Bool,
                            ArrayData::String(_) => crate::types::JuliaType::String,
                            ArrayData::Char(_) => crate::types::JuliaType::Char,
                            ArrayData::StructRefs(refs) => {
                                if let Some(type_id) = arr_borrow.struct_type_id {
                                    self.struct_defs
                                        .get(type_id)
                                        .map(|def| {
                                            crate::types::JuliaType::Struct(def.name.clone())
                                        })
                                        .unwrap_or(crate::types::JuliaType::Any)
                                } else if let Some(&first_ref_idx) = refs.first() {
                                    self.struct_heap
                                        .get(first_ref_idx)
                                        .map(|s| {
                                            crate::types::JuliaType::Struct(s.struct_name.clone())
                                        })
                                        .unwrap_or(crate::types::JuliaType::Any)
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                            ArrayData::Any(values) => {
                                if let Some(first) = values.first() {
                                    match first {
                                        Value::StructRef(idx) => self
                                            .struct_heap
                                            .get(*idx)
                                            .map(|s| {
                                                crate::types::JuliaType::Struct(
                                                    s.struct_name.clone(),
                                                )
                                            })
                                            .unwrap_or(crate::types::JuliaType::Any),
                                        Value::Struct(s) => {
                                            crate::types::JuliaType::Struct(s.struct_name.clone())
                                        }
                                        _ => crate::types::JuliaType::Any,
                                    }
                                } else {
                                    crate::types::JuliaType::Any
                                }
                            }
                        };

                        let array_type = match arr_borrow.shape.len() {
                            1 => crate::types::JuliaType::VectorOf(Box::new(elem_type)),
                            2 => crate::types::JuliaType::MatrixOf(Box::new(elem_type)),
                            _ => crate::types::JuliaType::Array,
                        };
                        (None, array_type)
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
                    // Ref: isa(Ref(x), Ref) should be true (Issue #2687)
                    // runtime_type() returns inner type, so we need special handling
                    Value::Ref(_) => (None, crate::types::JuliaType::Struct("Ref".to_string())),
                    _ => (None, val.runtime_type()),
                };

                let normalized_target = normalize_type_for_isa(&target_type_name);
                let is_match = if let Some(ref struct_name) = struct_name_opt {
                    let normalized_struct = normalize_type_for_isa(struct_name);
                    if normalized_struct == normalized_target {
                        true
                    } else {
                        let is_abstract_type = self
                            .abstract_type_name_index
                            .contains_key(&target_type_name);

                        if is_abstract_type {
                            self.check_isa_with_abstract_resolved(
                                &struct_name_opt,
                                &target_type_name,
                            )
                        } else {
                            let target_type = crate::types::JuliaType::from_name(&target_type_name)
                                .unwrap_or_else(|| {
                                    crate::types::JuliaType::Struct(target_type_name.clone())
                                });
                            resolved_val_type.is_subtype_of(&target_type)
                        }
                    }
                } else {
                    let target_type = crate::types::JuliaType::from_name(&target_type_name)
                        .unwrap_or_else(|| {
                            crate::types::JuliaType::Struct(target_type_name.clone())
                        });
                    resolved_val_type.is_subtype_of(&target_type)
                };
                self.stack.push(Value::Bool(is_match));
            }

            BuiltinId::Subtype => {
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                let left_type = match &left {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    Value::Struct(s) => s.struct_name.clone(),
                    _ => format!("{:?}", left),
                };
                let right_type = match &right {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    Value::Struct(s) => s.struct_name.clone(),
                    _ => format!("{:?}", right),
                };

                let is_subtype = self.check_subtype(&left_type, &right_type);
                self.stack.push(Value::Bool(is_subtype));
            }
            BuiltinId::Sizeof => {
                // sizeof(x) - return size of value in bytes
                // For primitive types, return their actual byte size
                // For composite types (structs, arrays), return approximate size
                let val = self.stack.pop_value()?;
                let size: i64 = match &val {
                    Value::Bool(_) => 1,
                    Value::I64(_) => 8,
                    Value::F64(_) => 8,
                    Value::Char(_) => 4, // Julia Char is 32-bit Unicode
                    Value::Str(s) => s.len() as i64, // Number of bytes in UTF-8 encoding
                    Value::Array(arr) => {
                        let arr_ref = arr.borrow();
                        let elem_size: i64 = match &arr_ref.data {
                            ArrayData::F64(_) => 8,
                            ArrayData::I64(_) => 8,
                            ArrayData::F32(_) => 4,
                            ArrayData::I32(_) => 4,
                            ArrayData::I16(_) => 2,
                            ArrayData::I8(_) => 1,
                            ArrayData::U64(_) => 8,
                            ArrayData::U32(_) => 4,
                            ArrayData::U16(_) => 2,
                            ArrayData::U8(_) => 1,
                            ArrayData::Bool(_) => 1,
                            ArrayData::Char(_) => 4,
                            ArrayData::String(_) => 8,     // Pointer size
                            ArrayData::StructRefs(_) => 8, // Pointer size
                            ArrayData::Any(_) => 8,        // Pointer size
                        };
                        let total_elems: i64 = arr_ref.shape.iter().map(|&d| d as i64).product();
                        elem_size * total_elems
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = crate::vm::util::memory_to_array_ref(mem);
                        let arr_ref = arr.borrow();
                        let elem_size: i64 = match &arr_ref.data {
                            ArrayData::F64(_) => 8,
                            ArrayData::I64(_) => 8,
                            ArrayData::F32(_) => 4,
                            ArrayData::I32(_) => 4,
                            ArrayData::I16(_) => 2,
                            ArrayData::I8(_) => 1,
                            ArrayData::U64(_) => 8,
                            ArrayData::U32(_) => 4,
                            ArrayData::U16(_) => 2,
                            ArrayData::U8(_) => 1,
                            ArrayData::Bool(_) => 1,
                            ArrayData::Char(_) => 4,
                            ArrayData::String(_) => 8,     // Pointer size
                            ArrayData::StructRefs(_) => 8, // Pointer size
                            ArrayData::Any(_) => 8,        // Pointer size
                        };
                        let total_elems: i64 = arr_ref.shape.iter().map(|&d| d as i64).product();
                        elem_size * total_elems
                    }
                    Value::Tuple(t) => {
                        // Sum of sizes of all elements
                        t.elements.len() as i64 * 8 // Approximate: 8 bytes per element pointer
                    }
                    Value::Nothing => 0,
                    Value::Missing => 0,
                    Value::DataType(_) => 8, // Type pointer
                    Value::StructRef(_) | Value::Struct(_) => 8, // Pointer/reference size
                    _ => 8,                  // Default to pointer size for other types
                };
                self.stack.push(Value::I64(size));
            }

            BuiltinId::Isbits => {
                // isbits(x) - check if x is an instance of a bits type
                // Bits types: Bool, Int*, UInt*, Float*, Char
                // Non-bits types: String, Array, Struct, etc.
                let val = self.stack.pop_value()?;
                let is_bits = matches!(
                    &val,
                    Value::Bool(_)
                        | Value::I64(_)
                        | Value::F64(_)
                        | Value::Char(_)
                        | Value::Nothing
                );
                self.stack.push(Value::Bool(is_bits));
            }

            BuiltinId::Isbitstype => {
                // isbitstype(T) - check if T is a bits type
                // Bits types are immutable and contain no references
                let type_val = self.stack.pop_value()?;
                let type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };

                let is_bits = matches!(
                    type_name.as_str(),
                    "Bool"
                        | "Int8"
                        | "Int16"
                        | "Int32"
                        | "Int64"
                        | "Int128"
                        | "Int"
                        | "UInt8"
                        | "UInt16"
                        | "UInt32"
                        | "UInt64"
                        | "UInt128"
                        | "UInt"
                        | "Float16"
                        | "Float32"
                        | "Float64"
                        | "Char"
                        | "Nothing"
                );
                self.stack.push(Value::Bool(is_bits));
            }

            BuiltinId::Supertype => {
                // supertype(T) - get parent type
                let type_val = self.stack.pop_value()?;
                let type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    _ => {
                        // Return Any for non-type values
                        self.stack
                            .push(Value::DataType(crate::types::JuliaType::Any));
                        return Ok(Some(()));
                    }
                };

                // Get supertype from the type hierarchy
                let supertype = match type_name.as_str() {
                    // Primitive types
                    "Int8" | "Int16" | "Int32" | "Int64" | "Int128" => "Signed",
                    "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128" => "Unsigned",
                    "Signed" | "Unsigned" => "Integer",
                    "Integer" => "Real",
                    "Float16" | "Float32" | "Float64" => "AbstractFloat",
                    "AbstractFloat" | "Real" => "Number",
                    "Number" => "Any",
                    "Bool" => "Integer",
                    "Char" => "AbstractChar",
                    "AbstractChar" => "Any",
                    "String" => "AbstractString",
                    "AbstractString" => "Any",
                    "Array" | "Vector" | "Matrix" => "DenseArray",
                    "DenseArray" => "AbstractArray",
                    "AbstractArray" => "Any",
                    "Tuple" => "Any",
                    "Nothing" => "Any",
                    "Any" => "Any", // Any is its own supertype
                    // User-defined structs - check abstract types
                    _ => {
                        // Look up in struct definitions for parent type
                        if let Some(def) = self
                            .struct_defs
                            .iter()
                            .find(|d| d.name == type_name.as_ref())
                        {
                            if let Some(ref parent) = def.parent_type {
                                parent.as_str()
                            } else {
                                "Any"
                            }
                        } else if let Some(at) = self
                            .abstract_type_name_index
                            .get(&*type_name)
                            .and_then(|&idx| self.abstract_types.get(idx))
                        {
                            if let Some(ref parent) = at.parent {
                                parent.as_str()
                            } else {
                                "Any"
                            }
                        } else {
                            "Any"
                        }
                    }
                };

                let super_jt = crate::types::JuliaType::from_name(supertype)
                    .unwrap_or_else(|| crate::types::JuliaType::Struct(supertype.to_string()));
                self.stack.push(Value::DataType(super_jt));
            }

            BuiltinId::Supertypes => {
                // supertypes(T) - tuple of all supertypes from T to Any
                let type_val = self.stack.pop_value()?;
                let mut type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    _ => {
                        // For non-type values, return empty tuple
                        self.stack
                            .push(Value::Tuple(TupleValue { elements: vec![] }));
                        return Ok(Some(()));
                    }
                };

                // Build chain of supertypes by repeatedly calling supertype logic
                let mut supertypes = vec![];

                // Add the type itself
                let jt = crate::types::JuliaType::from_name(&type_name)
                    .unwrap_or_else(|| crate::types::JuliaType::Struct(type_name.clone()));
                supertypes.push(Value::DataType(jt));

                // Walk up the type hierarchy until we reach Any
                loop {
                    if type_name == "Any" {
                        break;
                    }

                    // Get supertype
                    let supertype = match type_name.as_str() {
                        // Primitive types
                        "Int8" | "Int16" | "Int32" | "Int64" | "Int128" => "Signed",
                        "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128" => "Unsigned",
                        "Signed" | "Unsigned" => "Integer",
                        "Integer" => "Real",
                        "Float16" | "Float32" | "Float64" => "AbstractFloat",
                        "AbstractFloat" | "Real" => "Number",
                        "Number" => "Any",
                        "Bool" => "Integer",
                        "Char" => "AbstractChar",
                        "AbstractChar" => "Any",
                        "String" => "AbstractString",
                        "AbstractString" => "Any",
                        "Array" | "Vector" | "Matrix" => "DenseArray",
                        "DenseArray" => "AbstractArray",
                        "AbstractArray" => "Any",
                        "Tuple" => "Any",
                        "Nothing" => "Any",
                        "Any" => break, // Already at top
                        // User-defined structs - check abstract types
                        _ => {
                            // Look up in struct definitions for parent type
                            if let Some(def) = self
                                .struct_defs
                                .iter()
                                .find(|d| d.name == type_name.as_ref())
                            {
                                if let Some(ref parent) = def.parent_type {
                                    parent.as_str()
                                } else {
                                    "Any"
                                }
                            } else if let Some(at) = self
                                .abstract_type_name_index
                                .get(&*type_name)
                                .and_then(|&idx| self.abstract_types.get(idx))
                            {
                                if let Some(ref parent) = at.parent {
                                    parent.as_str()
                                } else {
                                    "Any"
                                }
                            } else {
                                "Any"
                            }
                        }
                    };

                    type_name = supertype.to_string();
                    let super_jt = crate::types::JuliaType::from_name(supertype)
                        .unwrap_or_else(|| crate::types::JuliaType::Struct(supertype.to_string()));
                    supertypes.push(Value::DataType(super_jt));
                }

                self.stack.push(Value::Tuple(TupleValue {
                    elements: supertypes,
                }));
            }

            BuiltinId::Subtypes => {
                // subtypes(T) - vector of direct subtypes
                let type_val = self.stack.pop_value()?;
                let type_name = match &type_val {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    _ => {
                        // For non-type values, return empty array
                        let empty_array = ArrayData::Any(vec![]);
                        let array_val = Value::Array(new_array_ref(super::value::ArrayValue {
                            data: empty_array,
                            shape: vec![0],
                            struct_type_id: None,
                            element_type_override: None,
                        }));
                        self.stack.push(array_val);
                        return Ok(Some(()));
                    }
                };

                let mut subtypes = vec![];

                // Check all known types to find direct children
                // Built-in type hierarchy
                match type_name.as_str() {
                    "Any" => {
                        // Direct subtypes of Any
                        subtypes.extend(vec![
                            Value::DataType(crate::types::JuliaType::Number),
                            Value::DataType(crate::types::JuliaType::AbstractString),
                            Value::DataType(crate::types::JuliaType::AbstractChar),
                            Value::DataType(crate::types::JuliaType::AbstractArray),
                            Value::DataType(crate::types::JuliaType::Tuple),
                            Value::DataType(crate::types::JuliaType::Nothing),
                        ]);
                    }
                    "Number" => {
                        subtypes.extend(vec![Value::DataType(crate::types::JuliaType::Real)]);
                    }
                    "Real" => {
                        subtypes.extend(vec![
                            Value::DataType(crate::types::JuliaType::AbstractFloat),
                            Value::DataType(crate::types::JuliaType::Integer),
                        ]);
                    }
                    "Integer" => {
                        subtypes.extend(vec![
                            Value::DataType(crate::types::JuliaType::Bool),
                            Value::DataType(crate::types::JuliaType::Signed),
                            Value::DataType(crate::types::JuliaType::Unsigned),
                        ]);
                    }
                    "Signed" => {
                        subtypes.extend(vec![
                            Value::DataType(crate::types::JuliaType::Int8),
                            Value::DataType(crate::types::JuliaType::Int16),
                            Value::DataType(crate::types::JuliaType::Int32),
                            Value::DataType(crate::types::JuliaType::Int64),
                            Value::DataType(crate::types::JuliaType::Int128),
                        ]);
                    }
                    "Unsigned" => {
                        subtypes.extend(vec![
                            Value::DataType(crate::types::JuliaType::UInt8),
                            Value::DataType(crate::types::JuliaType::UInt16),
                            Value::DataType(crate::types::JuliaType::UInt32),
                            Value::DataType(crate::types::JuliaType::UInt64),
                            Value::DataType(crate::types::JuliaType::UInt128),
                        ]);
                    }
                    "AbstractFloat" => {
                        subtypes.extend(vec![
                            Value::DataType(crate::types::JuliaType::Float16),
                            Value::DataType(crate::types::JuliaType::Float32),
                            Value::DataType(crate::types::JuliaType::Float64),
                        ]);
                    }
                    "AbstractString" => {
                        subtypes.push(Value::DataType(crate::types::JuliaType::String));
                    }
                    "AbstractChar" => {
                        subtypes.push(Value::DataType(crate::types::JuliaType::Char));
                    }
                    "AbstractArray" => {
                        subtypes.push(Value::DataType(crate::types::JuliaType::Array));
                    }
                    _ => {
                        // Check user-defined types
                        // Find all struct defs that have this type as parent
                        for def in &self.struct_defs {
                            if let Some(ref parent) = def.parent_type {
                                if parent == &type_name {
                                    subtypes.push(Value::DataType(
                                        crate::types::JuliaType::Struct(def.name.clone()),
                                    ));
                                }
                            }
                        }
                        // Find all abstract types that have this type as parent
                        for at in &self.abstract_types {
                            if let Some(ref parent) = at.parent {
                                if parent == &type_name {
                                    subtypes.push(Value::DataType(
                                        crate::types::JuliaType::Struct(at.name.clone()),
                                    ));
                                }
                            }
                        }
                    }
                }

                let len = subtypes.len();
                let array_data = ArrayData::Any(subtypes);
                let array_val = Value::Array(new_array_ref(super::value::ArrayValue {
                    data: array_data,
                    shape: vec![len],
                    struct_type_id: None,
                    element_type_override: None,
                }));
                self.stack.push(array_val);
            }

            BuiltinId::Typeintersect => {
                // typeintersect(A, B) - compute type intersection
                let type_b = self.stack.pop_value()?;
                let type_a = self.stack.pop_value()?;

                let type_a_name = match &type_a {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    _ => {
                        // For non-type values, return Union{} (bottom type)
                        self.stack
                            .push(Value::DataType(crate::types::JuliaType::Union(vec![])));
                        return Ok(Some(()));
                    }
                };

                let type_b_name = match &type_b {
                    Value::DataType(jt) => jt.name().to_string(),
                    Value::Str(s) => s.clone(),
                    _ => {
                        // For non-type values, return Union{} (bottom type)
                        self.stack
                            .push(Value::DataType(crate::types::JuliaType::Union(vec![])));
                        return Ok(Some(()));
                    }
                };

                // Helper function to check if type_a is a subtype of type_b
                let is_subtype_of = |a: &str, b: &str| -> bool {
                    if a == b {
                        return true;
                    }

                    // Walk up the type hierarchy from a to see if we reach b
                    let mut current = a.to_string();
                    loop {
                        if current == b {
                            return true;
                        }
                        if current == "Any" {
                            return false;
                        }

                        let supertype = match current.as_str() {
                            "Int8" | "Int16" | "Int32" | "Int64" | "Int128" => "Signed",
                            "UInt8" | "UInt16" | "UInt32" | "UInt64" | "UInt128" => "Unsigned",
                            "Signed" | "Unsigned" => "Integer",
                            "Integer" => "Real",
                            "Float16" | "Float32" | "Float64" => "AbstractFloat",
                            "AbstractFloat" | "Real" => "Number",
                            "Number" => "Any",
                            "Bool" => "Integer",
                            "Char" => "AbstractChar",
                            "AbstractChar" => "Any",
                            "String" => "AbstractString",
                            "AbstractString" => "Any",
                            "Array" | "Vector" | "Matrix" => "DenseArray",
                            "DenseArray" => "AbstractArray",
                            "AbstractArray" => "Any",
                            "Tuple" => "Any",
                            "Nothing" => "Any",
                            "Any" => return false,
                            _ => "Any",
                        };
                        current = supertype.to_string();
                    }
                };

                // Compute intersection
                let result_type = if type_a_name == type_b_name {
                    // Same type - intersection is the type itself
                    crate::types::JuliaType::from_name(&type_a_name)
                        .unwrap_or_else(|| crate::types::JuliaType::Struct(type_a_name.clone()))
                } else if is_subtype_of(&type_a_name, &type_b_name) {
                    // A <: B, so intersection is A (the more specific type)
                    crate::types::JuliaType::from_name(&type_a_name)
                        .unwrap_or_else(|| crate::types::JuliaType::Struct(type_a_name.clone()))
                } else if is_subtype_of(&type_b_name, &type_a_name) {
                    // B <: A, so intersection is B (the more specific type)
                    crate::types::JuliaType::from_name(&type_b_name)
                        .unwrap_or_else(|| crate::types::JuliaType::Struct(type_b_name.clone()))
                } else {
                    // No subtype relationship - return Union{} (bottom type/empty union)
                    crate::types::JuliaType::Union(vec![])
                };

                self.stack.push(Value::DataType(result_type));
            }

            // BuiltinId::Typejoin removed - now Pure Julia (base/reflection.jl)
            // BuiltinId::Fieldcount removed - now Pure Julia (base/reflection.jl)
            BuiltinId::Hasfield => {
                // hasfield(T, name) - check if field exists
                let name_val = self.stack.pop_value()?;
                let type_val = self.stack.pop_value()?;

                let field_name = match &name_val {
                    Value::Str(s) => s.clone(),
                    Value::Symbol(s) => s.as_str().to_string(),
                    _ => {
                        self.stack.push(Value::Bool(false));
                        return Ok(Some(()));
                    }
                };

                let has_field = match &type_val {
                    Value::DataType(jt) => {
                        let type_name = jt.name();
                        self.struct_defs
                            .iter()
                            .find(|def| def.name == type_name.as_ref())
                            .map(|def| def.fields.iter().any(|(name, _)| name == &field_name))
                            .unwrap_or(false)
                    }
                    Value::Str(type_name) => self
                        .struct_defs
                        .iter()
                        .find(|def| &def.name == type_name)
                        .map(|def| def.fields.iter().any(|(name, _)| name == &field_name))
                        .unwrap_or(false),
                    Value::StructRef(idx) => {
                        if let Some(si) = self.struct_heap.get(*idx) {
                            self.struct_defs
                                .iter()
                                .find(|def| def.name == si.struct_name)
                                .map(|def| def.fields.iter().any(|(name, _)| name == &field_name))
                                .unwrap_or(false)
                        } else {
                            false
                        }
                    }
                    Value::Struct(si) => self
                        .struct_defs
                        .iter()
                        .find(|def| def.name == si.struct_name)
                        .map(|def| def.fields.iter().any(|(name, _)| name == &field_name))
                        .unwrap_or(false),
                    Value::NamedTuple(nt) => nt.names.contains(&field_name),
                    _ => false,
                };
                self.stack.push(Value::Bool(has_field));
            }

            // BuiltinId::Isconcretetype, Isabstracttype, Isprimitivetype, Isstructtype
            // removed - now Pure Julia (base/reflection.jl) with internal intrinsics
            BuiltinId::_Isabstracttype => {
                // _isabstracttype(T) - internal intrinsic: check if T is an abstract type
                let type_val = self.stack.pop_value()?;
                let is_abstract = match &type_val {
                    Value::DataType(jt) => {
                        let type_name = jt.name();
                        let type_name_ref = type_name.as_ref();
                        self.abstract_type_name_index.contains_key(type_name_ref)
                            || matches!(
                                type_name_ref,
                                "Any"
                                    | "Number"
                                    | "Real"
                                    | "Integer"
                                    | "Signed"
                                    | "Unsigned"
                                    | "AbstractFloat"
                                    | "AbstractString"
                                    | "AbstractChar"
                                    | "AbstractArray"
                                    | "AbstractVector"
                                    | "AbstractMatrix"
                                    | "AbstractDict"
                                    | "AbstractSet"
                                    | "AbstractRange"
                                    | "AbstractUnitRange"
                            )
                    }
                    _ => false,
                };
                self.stack.push(Value::Bool(is_abstract));
            }

            BuiltinId::_Isconcretetype => {
                // _isconcretetype(T) - internal intrinsic: check if T is a concrete type
                let type_val = self.stack.pop_value()?;
                let is_concrete = match &type_val {
                    Value::DataType(jt) => {
                        let type_name = jt.name();
                        let type_name_ref = type_name.as_ref();
                        let is_abstract = self.abstract_type_name_index.contains_key(type_name_ref);
                        if is_abstract {
                            false
                        } else {
                            matches!(
                                type_name_ref,
                                "Bool"
                                    | "Int8"
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
                                    | "Char"
                                    | "String"
                                    | "Nothing"
                                    | "Missing"
                            ) || self.struct_defs.iter().any(|def| def.name == type_name_ref)
                        }
                    }
                    _ => false,
                };
                self.stack.push(Value::Bool(is_concrete));
            }

            BuiltinId::_Ismutabletype => {
                // _ismutabletype(T) - internal intrinsic: check if T is a mutable type
                let type_val = self.stack.pop_value()?;
                let is_mutable_type = match &type_val {
                    Value::DataType(jt) => {
                        let type_name = jt.name();
                        let type_name_ref = type_name.as_ref();
                        matches!(type_name_ref, "Array" | "Vector" | "Matrix" | "Dict")
                            || self
                                .struct_defs
                                .iter()
                                .find(|def| def.name == type_name_ref)
                                .map(|def| def.is_mutable)
                                .unwrap_or(false)
                    }
                    _ => false,
                };
                self.stack.push(Value::Bool(is_mutable_type));
            }

            BuiltinId::Ismutable => {
                // ismutable(x) - check if x is mutable
                // In Julia: Arrays, Dicts, mutable structs are mutable
                let val = self.stack.pop_value()?;
                let is_mutable = match &val {
                    Value::Array(_) | Value::Memory(_) | Value::Dict(_) => true, // Memory → Array (Issue #2764)
                    Value::StructRef(idx) => {
                        // Look up the struct from the heap and check if it's mutable
                        self.struct_heap
                            .get(*idx)
                            .and_then(|s| {
                                self.struct_defs
                                    .iter()
                                    .find(|def| def.name == s.struct_name)
                                    .map(|def| def.is_mutable)
                            })
                            .unwrap_or(false)
                    }
                    Value::Struct(s) => {
                        // Check if the struct is defined as mutable
                        self.struct_defs
                            .iter()
                            .find(|def| def.name == s.struct_name)
                            .map(|def| def.is_mutable)
                            .unwrap_or(false)
                    }
                    // Primitive types are immutable
                    _ => false,
                };
                self.stack.push(Value::Bool(is_mutable));
            }

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
                    Value::Array(arr) => {
                        "Array".hash(&mut hasher);
                        // Use pointer-like identity for arrays (mutable objects)
                        (arr.as_ptr() as usize).hash(&mut hasher);
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
                    Value::Dict(d) => {
                        "Dict".hash(&mut hasher);
                        d.identity_ptr().hash(&mut hasher);
                    }
                    Value::Tuple(t) => {
                        "Tuple".hash(&mut hasher);
                        t.len().hash(&mut hasher);
                    }
                    Value::DataType(jt) => {
                        "DataType".hash(&mut hasher);
                        jt.name().hash(&mut hasher);
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
                    Value::Struct(s) if s.struct_name == "Complex" => s
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

                // Helper function to compare two values for equality (like Julia's ==)
                fn values_equal(a: &Value, b: &Value) -> bool {
                    match (a, b) {
                        (Value::I64(x), Value::I64(y)) => x == y,
                        (Value::F64(x), Value::F64(y)) => x == y,
                        (Value::I64(x), Value::F64(y)) => (*x as f64) == *y,
                        (Value::F64(x), Value::I64(y)) => *x == (*y as f64),
                        (Value::Bool(x), Value::Bool(y)) => x == y,
                        (Value::Str(x), Value::Str(y)) => x == y,
                        (Value::Char(x), Value::Char(y)) => x == y,
                        (Value::Symbol(x), Value::Symbol(y)) => x == y,
                        (Value::Nothing, Value::Nothing) => true,
                        (Value::Missing, Value::Missing) => true,
                        _ => false,
                    }
                }

                let found = match &collection {
                    Value::Array(arr) => {
                        let arr_ref = arr.borrow();
                        let len = arr_ref.len();
                        let mut found = false;
                        for i in 0..len {
                            if let Some(v) = arr_ref.data.get_value(i) {
                                if values_equal(&element, &v) {
                                    found = true;
                                    break;
                                }
                            }
                        }
                        found
                    }
                    // Memory → Array (Issue #2764)
                    Value::Memory(mem) => {
                        let arr = crate::vm::util::memory_to_array_ref(mem);
                        let arr_ref = arr.borrow();
                        let len = arr_ref.len();
                        let mut found = false;
                        for i in 0..len {
                            if let Some(v) = arr_ref.data.get_value(i) {
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
                    Value::Dict(d) => {
                        // Check if element is a key in the dict
                        d.iter().any(|(k, _)| match (&element, k) {
                            (Value::Str(s), DictKey::Str(ks)) => s == ks,
                            (Value::I64(n), DictKey::I64(kn)) => n == kn,
                            (Value::Symbol(sym), DictKey::Symbol(ks)) => sym.as_str() == ks,
                            _ => false,
                        })
                    }
                    Value::Set(s) => {
                        // Check if element is in the set (elements are stored as DictKey)
                        s.elements.iter().any(|k| match (&element, k) {
                            (Value::I64(n), DictKey::I64(kn)) => n == kn,
                            (Value::F64(n), DictKey::I64(kn)) => *n == (*kn as f64),
                            (Value::Str(st), DictKey::Str(ks)) => st == ks,
                            (Value::Symbol(sym), DictKey::Symbol(ks)) => sym.as_str() == ks,
                            _ => false,
                        })
                    }
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
                        match jt {
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

                self.stack.push(Value::DataType(result_type));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
