//! Array creation and storage instructions.
//!
//! Handles: NewArray, PushElem, FinalizeArray, PushArrayValue,
//!          NewArrayTyped, PushElemTyped, FinalizeArrayTyped,
//!          AllocUndefTyped, LoadArray, StoreArray

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use crate::rng::RngLike;

/// Result type for array basic operations
pub(super) enum ArrayBasicResult {
    /// Instruction was not handled by this module
    NotHandled,
    /// Instruction was handled successfully
    Handled,
    /// Instruction handled, but need to continue (e.g., after raise)
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute array creation and storage instructions.
    ///
    /// Returns `ArrayBasicResult::NotHandled` if the instruction is not an array basic operation.
    #[inline]
    pub(super) fn execute_array_basic(
        &mut self,
        instr: &Instr,
    ) -> Result<ArrayBasicResult, VmError> {
        match instr {
            Instr::NewArray(capacity) => {
                let data = Vec::with_capacity(*capacity);
                let arr = ArrayValue::from_f64(data, vec![0]);
                self.stack.push(Value::Array(new_array_ref(arr)));
                Ok(ArrayBasicResult::Handled)
            }

            Instr::PushArrayValue(arr) => {
                self.stack.push(Value::Array(new_array_ref(arr.clone())));
                Ok(ArrayBasicResult::Handled)
            }

            Instr::PushElem => {
                let val = self.pop_f64_or_i64()?;
                match self.stack.last_mut() {
                    Some(Value::Array(arr)) => {
                        let mut arr_mut = arr.borrow_mut();
                        arr_mut.try_data_f64_mut()?.push(val);
                        arr_mut.shape[0] = arr_mut.len();
                    }
                    _ => {
                        // INTERNAL: compiler always emits NewArray before PushElem.
                        return Err(VmError::InternalError(
                            "PushElem: expected Array on stack (compiler invariant)".to_string(),
                        ));
                    }
                }
                Ok(ArrayBasicResult::Handled)
            }

            Instr::FinalizeArray(shape) => {
                match self.stack.last_mut() {
                    Some(Value::Array(arr)) => {
                        arr.borrow_mut().shape = shape.clone();
                    }
                    _ => {
                        // INTERNAL: compiler always emits NewArray before FinalizeArray.
                        return Err(VmError::InternalError(
                            "FinalizeArray: expected Array on stack (compiler invariant)".to_string(),
                        ));
                    }
                }
                Ok(ArrayBasicResult::Handled)
            }

            Instr::NewArrayTyped(ref elem_type, capacity) => {
                let data = match elem_type {
                    ArrayElementType::F32 => ArrayData::F32(Vec::with_capacity(*capacity)),
                    ArrayElementType::F64 => ArrayData::F64(Vec::with_capacity(*capacity)),
                    // Complex types use interleaved storage: [re1, im1, re2, im2, ...]
                    ArrayElementType::ComplexF32 => {
                        ArrayData::F32(Vec::with_capacity(*capacity * 2))
                    }
                    ArrayElementType::ComplexF64 => {
                        ArrayData::F64(Vec::with_capacity(*capacity * 2))
                    }
                    ArrayElementType::I8 => ArrayData::I8(Vec::with_capacity(*capacity)),
                    ArrayElementType::I16 => ArrayData::I16(Vec::with_capacity(*capacity)),
                    ArrayElementType::I32 => ArrayData::I32(Vec::with_capacity(*capacity)),
                    ArrayElementType::I64 => ArrayData::I64(Vec::with_capacity(*capacity)),
                    ArrayElementType::U8 => ArrayData::U8(Vec::with_capacity(*capacity)),
                    ArrayElementType::U16 => ArrayData::U16(Vec::with_capacity(*capacity)),
                    ArrayElementType::U32 => ArrayData::U32(Vec::with_capacity(*capacity)),
                    ArrayElementType::U64 => ArrayData::U64(Vec::with_capacity(*capacity)),
                    ArrayElementType::Bool => ArrayData::Bool(Vec::with_capacity(*capacity)),
                    ArrayElementType::String => ArrayData::String(Vec::with_capacity(*capacity)),
                    ArrayElementType::Char => ArrayData::Char(Vec::with_capacity(*capacity)),
                    ArrayElementType::StructOf(_) => {
                        ArrayData::StructRefs(Vec::with_capacity(*capacity))
                    }
                    // isbits struct inline storage: AoS format [f1, f2, f1, f2, ...]
                    ArrayElementType::StructInlineOf(_, field_count) => {
                        ArrayData::Any(Vec::with_capacity(*capacity * field_count))
                    }
                    ArrayElementType::Struct | ArrayElementType::Any => {
                        ArrayData::Any(Vec::with_capacity(*capacity))
                    }
                    // Tuple arrays use AoS storage in ArrayData::Any
                    ArrayElementType::TupleOf(ref field_types) => {
                        ArrayData::Any(Vec::with_capacity(*capacity * field_types.len()))
                    }
                };
                let mut arr = TypedArrayValue::new(data, vec![0]);
                if let ArrayElementType::StructOf(type_id) = elem_type {
                    arr.struct_type_id = Some(*type_id);
                }
                // Set struct_type_id and element_type_override for isbits struct arrays
                if let ArrayElementType::StructInlineOf(type_id, field_count) = elem_type {
                    arr.struct_type_id = Some(*type_id);
                    arr.element_type_override =
                        Some(ArrayElementType::StructInlineOf(*type_id, *field_count));
                }
                self.stack.push(Value::Array(new_typed_array_ref(arr)));
                Ok(ArrayBasicResult::Handled)
            }

            Instr::PushElemTyped => {
                let val = self.stack.pop_value()?;
                match self.stack.last_mut() {
                    Some(Value::Array(arr)) => {
                        let mut arr_mut = arr.borrow_mut();
                        let struct_type_id = arr_mut.struct_type_id;
                        // Special handling for struct arrays: convert Value::Struct to StructRef
                        match (&mut arr_mut.data, &val) {
                            (ArrayData::StructRefs(refs), Value::Struct(s)) => {
                                let idx = self.struct_heap.len();
                                self.struct_heap.push(s.clone());
                                refs.push(idx);
                                arr_mut.shape[0] += 1;
                            }
                            (ArrayData::StructRefs(refs), Value::StructRef(idx)) => {
                                refs.push(*idx);
                                arr_mut.shape[0] += 1;
                            }
                            (ArrayData::StructRefs(refs), Value::I64(i)) => {
                                let mut handled = false;
                                if let Some(type_id) = struct_type_id {
                                    if let Some(def) = self.struct_defs.get(type_id) {
                                        if def.name == "Rational"
                                            || def.name.starts_with("Rational{")
                                        {
                                            let struct_name = def.name.clone();
                                            let idx = self.struct_heap.len();
                                            self.struct_heap.push(StructInstance::with_name(
                                                type_id,
                                                struct_name,
                                                vec![Value::I64(*i), Value::I64(1)],
                                            ));
                                            refs.push(idx);
                                            arr_mut.shape[0] += 1;
                                            handled = true;
                                        }
                                    }
                                }
                                if !handled {
                                    arr_mut.push(val)?
                                }
                            }
                            _ => {
                                // Regular push for other types
                                arr_mut.push(val)?
                            }
                        }
                    }
                    _ => {
                        // INTERNAL: compiler always emits NewArrayTyped before PushElemTyped.
                        return Err(VmError::InternalError(
                            "PushElemTyped: expected TypedArray on stack (compiler invariant)".to_string(),
                        ));
                    }
                }
                Ok(ArrayBasicResult::Handled)
            }

            Instr::FinalizeArrayTyped(shape) => {
                match self.stack.last_mut() {
                    Some(Value::Array(arr)) => {
                        arr.borrow_mut().shape = shape.clone();
                    }
                    _ => {
                        // INTERNAL: compiler always emits NewArrayTyped before FinalizeArrayTyped.
                        return Err(VmError::InternalError(
                            "FinalizeArrayTyped: expected TypedArray on stack (compiler invariant)".to_string(),
                        ));
                    }
                }
                Ok(ArrayBasicResult::Handled)
            }

            Instr::AllocUndefTyped(ref elem_type, argc) => {
                // Generic Array{T}(undef, dims...) for all element types (Issue #2218)
                let mut dims = Vec::with_capacity(*argc);
                for _ in 0..*argc {
                    dims.push(self.stack.pop_usize()?);
                }
                dims.reverse();
                let mut arr = ArrayValue::undef_typed(elem_type, dims);
                // For Complex types, store the correct type_id from struct_defs
                if matches!(
                    elem_type,
                    ArrayElementType::ComplexF64 | ArrayElementType::ComplexF32
                ) {
                    arr.struct_type_id = Some(self.get_complex_type_id());
                }
                self.stack.push(Value::Array(new_array_ref(arr)));
                Ok(ArrayBasicResult::Handled)
            }

            Instr::LoadArray(name) => {
                // First check slots and locals in current frame
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Array(arr)) = self.load_slot_value_by_name(frame, name) {
                        self.stack.push(Value::Array(arr));
                        return Ok(ArrayBasicResult::Continue);
                    }
                    // Check if it's a TypedArray in locals_any
                    if let Some(Value::Array(arr)) = frame.locals_any.get(name).cloned() {
                        self.stack.push(Value::Array(arr));
                        return Ok(ArrayBasicResult::Continue);
                    }
                    // Runtime fallback: Set stored in locals_any (Issue #1828)
                    if let Some(val @ Value::Set(_)) = frame.locals_any.get(name).cloned() {
                        self.stack.push(val);
                        return Ok(ArrayBasicResult::Continue);
                    }
                    if let Some(arr) = frame.locals_array.get(name).cloned() {
                        self.stack.push(Value::Array(arr));
                        return Ok(ArrayBasicResult::Continue);
                    }
                }
                // Fall back to global frame if present
                if self.frames.len() > 1 {
                    if let Some(frame) = self.frames.first() {
                        if let Some(Value::Array(arr)) = self.load_slot_value_by_name(frame, name) {
                            self.stack.push(Value::Array(arr));
                            return Ok(ArrayBasicResult::Continue);
                        }
                        if let Some(Value::Array(arr)) = frame.locals_any.get(name).cloned() {
                            self.stack.push(Value::Array(arr));
                            return Ok(ArrayBasicResult::Continue);
                        }
                        // Runtime fallback: Set in global frame (Issue #1828)
                        if let Some(val @ Value::Set(_)) = frame.locals_any.get(name).cloned() {
                            self.stack.push(val);
                            return Ok(ArrayBasicResult::Continue);
                        }
                        if let Some(arr) = frame.locals_array.get(name).cloned() {
                            self.stack.push(Value::Array(arr));
                            return Ok(ArrayBasicResult::Continue);
                        }
                    }
                }
                // Variable not found - raise error instead of creating empty array
                self.raise(VmError::UndefVarError(name.clone()))?;
                Ok(ArrayBasicResult::Continue)
            }

            Instr::StoreArray(name) => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::Array(arr) => {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.locals_array.insert(name.clone(), arr);
                            frame.var_types.insert(name.clone(), frame::VarTypeTag::Array);
                        }
                    }
                    Value::Set(_) | Value::StructRef(_) | Value::Dict(_) => {
                        // Runtime fallback: store Set/StructRef/Dict in locals_any
                        // (Issue #1828, Issue #2748)
                        if let Some(frame) = self.frames.last_mut() {
                            frame.locals_any.insert(name.clone(), val);
                            frame.var_types.insert(name.clone(), frame::VarTypeTag::Any);
                        }
                    }
                    other => {
                        // INTERNAL: StoreArray is emitted only when the compiler typed the variable as Array; wrong type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "StoreArray: expected Array or Set, got {:?}",
                            util::value_type_name(&other)
                        )));
                    }
                }
                Ok(ArrayBasicResult::Handled)
            }

            _ => Ok(ArrayBasicResult::NotHandled),
        }
    }
}
