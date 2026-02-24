//! Local variable load/store operations.
//!
//! This module handles Load*/Store* instructions for local variables,
//! as well as fused load+arithmetic operations.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::frame::VarTypeTag;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{SymbolValue, Value};
use super::super::Vm;

/// Result of executing a local variable instruction.
pub(super) enum LocalsResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute local variable load/store instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_locals(&mut self, instr: &Instr) -> Result<LocalsResult, VmError> {
        match instr {
            Instr::LoadStr(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        self.load_slot_value_by_name(frame, name)
                            .and_then(|val| match val {
                                Value::Str(v) => Some(v),
                                _ => None,
                            })
                            .or_else(|| frame.locals_str.get(name).cloned())
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                self.load_slot_value_by_name(frame, name)
                                    .and_then(|val| match val {
                                        Value::Str(v) => Some(v),
                                        _ => None,
                                    })
                                    .or_else(|| frame.locals_str.get(name).cloned())
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                self.stack.push(Value::Str(v));
                Ok(LocalsResult::Handled)
            }
            Instr::StoreStr(name) => {
                let v = self.stack.pop_str()?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_str.insert(name.clone(), v);
                    frame.var_types.insert(name.clone(), VarTypeTag::Str);
                }
                Ok(LocalsResult::Handled)
            }

            Instr::LoadI64(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        frame.locals_i64.get(name).copied().or_else(|| {
                            match self.load_slot_value_by_name(frame, name) {
                                Some(Value::I64(v)) => Some(v),
                                _ => None,
                            }
                        })
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                frame.locals_i64.get(name).copied().or_else(|| {
                                    match self.load_slot_value_by_name(frame, name) {
                                        Some(Value::I64(v)) => Some(v),
                                        _ => None,
                                    }
                                })
                            })
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::I64(val));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::StoreI64(name) => {
                let v = self.stack.pop_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_i64.insert(name.clone(), v);
                    frame.var_types.insert(name.clone(), VarTypeTag::I64);
                }
                Ok(LocalsResult::Handled)
            }

            Instr::LoadF64(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| {
                        frame.locals_f64.get(name).copied().or_else(|| {
                            match self.load_slot_value_by_name(frame, name) {
                                Some(Value::F64(v)) => Some(v),
                                _ => None,
                            }
                        })
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                frame.locals_f64.get(name).copied().or_else(|| {
                                    match self.load_slot_value_by_name(frame, name) {
                                        Some(Value::F64(v)) => Some(v),
                                        _ => None,
                                    }
                                })
                            })
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::F64(val));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::StoreF64(name) => {
                let v = self.pop_f64_or_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_f64.insert(name.clone(), v);
                    frame.var_types.insert(name.clone(), VarTypeTag::F64);
                }
                Ok(LocalsResult::Handled)
            }

            Instr::LoadF32(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals_f32.get(name).copied())
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| frame.locals_f32.get(name).copied())
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::F32(val));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::StoreF32(name) => {
                let val = self.stack.pop_value()?;
                let v = match val {
                    Value::F32(f) => f,
                    Value::F64(f) => f as f32,
                    Value::I64(i) => i as f32,
                    _ => 0.0f32,
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_f32.insert(name.clone(), v);
                    frame.var_types.insert(name.clone(), VarTypeTag::F32);
                }
                Ok(LocalsResult::Handled)
            }

            Instr::LoadF16(name) => {
                let v = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals_f16.get(name).copied())
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| frame.locals_f16.get(name).copied())
                        } else {
                            None
                        }
                    });
                match v {
                    Some(val) => {
                        self.stack.push(Value::F16(val));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::StoreF16(name) => {
                let val = self.stack.pop_value()?;
                let v = match val {
                    Value::F16(f) => f,
                    Value::F32(f) => half::f16::from_f32(f),
                    Value::F64(f) => half::f16::from_f64(f),
                    Value::I64(i) => half::f16::from_f64(i as f64),
                    _ => half::f16::from_f64(0.0),
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_f16.insert(name.clone(), v);
                    frame.var_types.insert(name.clone(), VarTypeTag::F16);
                }
                Ok(LocalsResult::Handled)
            }

            Instr::LoadSlot(slot) => {
                if let Some(frame) = self.frames.last() {
                    let val = frame.locals_slots.get(*slot).and_then(|v| v.clone());
                    match val {
                        Some(v) => {
                            self.stack.push(v);
                            Ok(LocalsResult::Handled)
                        }
                        None => {
                            let name = self.slot_name_for_frame(frame, *slot);
                            self.raise(VmError::UndefVarError(name))?;
                            Ok(LocalsResult::Continue)
                        }
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                    Ok(LocalsResult::Continue)
                }
            }
            Instr::StoreSlot(slot) => {
                let val = self.stack.pop_value()?;
                // Convert Struct to StructRef for heap storage; all other values pass through
                let val = match val {
                    Value::Struct(s) => {
                        let idx = self.struct_heap.len();
                        self.struct_heap.push(s);
                        Value::StructRef(idx)
                    }
                    Value::StructRef(idx) => Value::StructRef(idx),
                    // Pass through all other Value variants unchanged (e.g., I64, F64, Bool, etc.)
                    other => other,
                };
                if let Some(frame) = self.frames.last_mut() {
                    if let Some(slot_ref) = frame.locals_slots.get_mut(*slot) {
                        *slot_ref = Some(val);
                    } else {
                        // INTERNAL: slot index is compiler-generated; out-of-bounds means compiler produced an invalid slot
                        return Err(VmError::InternalError(format!(
                            "StoreSlot: slot out of bounds: {}",
                            slot
                        )));
                    }
                }
                Ok(LocalsResult::Handled)
            }

            Instr::LoadAny(name) => {
                let found = self.try_load_from_frame(name, self.frames.len().saturating_sub(1));
                if !found {
                    // Check type_bindings from all frames (for where clause type parameters)
                    // Type parameters like T from `where T` should be accessible from nested function calls
                    let mut type_binding_found = false;
                    for frame_idx in (0..self.frames.len()).rev() {
                        if let Some(frame) = self.frames.get(frame_idx) {
                            if let Some(julia_type) = frame.type_bindings.get(name) {
                                self.stack.push(Value::DataType(julia_type.clone()));
                                type_binding_found = true;
                                break;
                            }
                        }
                    }
                    if !type_binding_found {
                        if self.frames.len() > 1 {
                            let global_found = self.try_load_from_frame(name, 0);
                            if !global_found {
                                self.raise(VmError::UndefVarError(name.clone()))?;
                                return Ok(LocalsResult::Continue);
                            }
                        } else {
                            self.raise(VmError::UndefVarError(name.clone()))?;
                            return Ok(LocalsResult::Continue);
                        }
                    }
                }
                Ok(LocalsResult::Handled)
            }
            Instr::LoadTypeBinding(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(julia_type) = frame.type_bindings.get(name) {
                        self.stack.push(Value::DataType(julia_type.clone()));
                        Ok(LocalsResult::Handled)
                    } else {
                        self.raise(VmError::UndefVarError(format!(
                            "Unbound type parameter: {}",
                            name
                        )))?;
                        Ok(LocalsResult::Continue)
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!(
                        "No frame for type binding: {}",
                        name
                    )))?;
                    Ok(LocalsResult::Continue)
                }
            }
            Instr::LoadValBool(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(val) = frame.locals_bool.get(name) {
                        self.stack.push(Value::Bool(*val));
                        Ok(LocalsResult::Handled)
                    } else {
                        self.raise(VmError::UndefVarError(format!(
                            "Unbound Val boolean parameter: {}",
                            name
                        )))?;
                        Ok(LocalsResult::Continue)
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!(
                        "No frame for Val boolean: {}",
                        name
                    )))?;
                    Ok(LocalsResult::Continue)
                }
            }
            Instr::LoadValSymbol(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(symbol_name) = frame.locals_val_symbol.get(name) {
                        self.stack
                            .push(Value::Symbol(SymbolValue::new(symbol_name)));
                        Ok(LocalsResult::Handled)
                    } else {
                        self.raise(VmError::UndefVarError(format!(
                            "Unbound Val symbol parameter: {}",
                            name
                        )))?;
                        Ok(LocalsResult::Continue)
                    }
                } else {
                    self.raise(VmError::UndefVarError(format!(
                        "No frame for Val symbol: {}",
                        name
                    )))?;
                    Ok(LocalsResult::Continue)
                }
            }
            Instr::StoreAny(name) => {
                let val = self.stack.pop_value()?;
                if let Some(frame) = self.frames.last_mut() {
                    // O(1) removal via tag instead of clearing all 18 maps
                    frame.remove_var(name);

                    let tag = match val {
                        Value::I64(v) => {
                            frame.locals_i64.insert(name.clone(), v);
                            VarTypeTag::I64
                        }
                        Value::F64(v) => {
                            frame.locals_f64.insert(name.clone(), v);
                            VarTypeTag::F64
                        }
                        Value::Str(v) => {
                            frame.locals_str.insert(name.clone(), v);
                            VarTypeTag::Str
                        }
                        Value::Char(v) => {
                            frame.locals_char.insert(name.clone(), v);
                            VarTypeTag::Char
                        }
                        Value::Array(v) => {
                            frame.locals_array.insert(name.clone(), v);
                            VarTypeTag::Array
                        }
                        Value::Tuple(v) => {
                            frame.locals_tuple.insert(name.clone(), v);
                            VarTypeTag::Tuple
                        }
                        Value::NamedTuple(v) => {
                            frame.locals_named_tuple.insert(name.clone(), v);
                            VarTypeTag::NamedTuple
                        }
                        Value::Dict(v) => {
                            frame.locals_dict.insert(name.clone(), v);
                            VarTypeTag::Dict
                        }
                        Value::Set(_) => {
                            frame.locals_any.insert(name.clone(), val);
                            VarTypeTag::Any
                        }
                        Value::Struct(v) => {
                            let idx = self.struct_heap.len();
                            self.struct_heap.push(v);
                            frame.locals_struct.insert(name.clone(), idx);
                            VarTypeTag::Struct
                        }
                        Value::F32(v) => {
                            frame.locals_f32.insert(name.clone(), v);
                            VarTypeTag::F32
                        }
                        Value::F16(v) => {
                            frame.locals_f16.insert(name.clone(), v);
                            VarTypeTag::F16
                        }
                        Value::Bool(b) => {
                            frame.locals_bool.insert(name.clone(), b);
                            VarTypeTag::Bool
                        }
                        Value::I8(_)
                        | Value::I16(_)
                        | Value::I32(_)
                        | Value::I128(_)
                        | Value::U8(_)
                        | Value::U16(_)
                        | Value::U32(_)
                        | Value::U64(_)
                        | Value::U128(_) => {
                            frame.locals_narrow_int.insert(name.clone(), val);
                            VarTypeTag::NarrowInt
                        }
                        Value::StructRef(idx) => {
                            frame.locals_struct.insert(name.clone(), idx);
                            VarTypeTag::Struct
                        }
                        Value::Range(v) => {
                            frame.locals_range.insert(name.clone(), v);
                            VarTypeTag::Range
                        }
                        Value::Rng(v) => {
                            frame.locals_rng.insert(name.clone(), v);
                            VarTypeTag::Rng
                        }
                        Value::Generator(v) => {
                            frame.locals_generator.insert(name.clone(), v);
                            VarTypeTag::Generator
                        }
                        Value::Nothing => {
                            frame.locals_nothing.insert(name.clone());
                            VarTypeTag::Nothing
                        }
                        Value::Missing
                        | Value::BigInt(_)
                        | Value::BigFloat(_)
                        | Value::SliceAll
                        | Value::Ref(_)
                        | Value::DataType(_)
                        | Value::Module(_)
                        | Value::Function(_)
                        | Value::Closure(_)
                        | Value::ComposedFunction(_)
                        | Value::Undef
                        | Value::IO(_)
                        | Value::Symbol(_)
                        | Value::Expr(_)
                        | Value::QuoteNode(_)
                        | Value::LineNumberNode(_)
                        | Value::GlobalRef(_)
                        | Value::Pairs(_)
                        | Value::Regex(_)
                        | Value::RegexMatch(_)
                        | Value::Enum { .. }
                        | Value::Memory(_) => {
                            frame.locals_any.insert(name.clone(), val);
                            VarTypeTag::Any
                        }
                    };
                    frame.var_types.insert(name.clone(), tag);
                }
                Ok(LocalsResult::Handled)
            }

            // === Fused load+arithmetic instructions ===
            Instr::LoadAddI64(name) => {
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals_i64.get(name).copied())
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| frame.locals_i64.get(name).copied())
                        } else {
                            None
                        }
                    });
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(var + stack_val));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::LoadAddI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(LocalsResult::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let var_val = frame.locals_slots.get(*slot).and_then(|v| v.as_ref());
                match var_val {
                    Some(Value::I64(var)) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(var + stack_val));
                        Ok(LocalsResult::Handled)
                    }
                    Some(_) => {
                        Err(VmError::TypeError(
                            "LoadAddI64Slot: expected I64".to_string(),
                        ))
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }

            Instr::LoadSubI64(name) => {
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals_i64.get(name).copied())
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| frame.locals_i64.get(name).copied())
                        } else {
                            None
                        }
                    });
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(stack_val - var));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::LoadSubI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(LocalsResult::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let var_val = frame.locals_slots.get(*slot).and_then(|v| v.as_ref());
                match var_val {
                    Some(Value::I64(var)) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(stack_val - var));
                        Ok(LocalsResult::Handled)
                    }
                    Some(_) => {
                        Err(VmError::TypeError(
                            "LoadSubI64Slot: expected I64".to_string(),
                        ))
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }

            Instr::LoadMulI64(name) => {
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals_i64.get(name).copied())
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| frame.locals_i64.get(name).copied())
                        } else {
                            None
                        }
                    });
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(var * stack_val));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::LoadMulI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(LocalsResult::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let var_val = frame.locals_slots.get(*slot).and_then(|v| v.as_ref());
                match var_val {
                    Some(Value::I64(var)) => {
                        let stack_val = self.stack.pop_i64()?;
                        self.stack.push(Value::I64(var * stack_val));
                        Ok(LocalsResult::Handled)
                    }
                    Some(_) => {
                        Err(VmError::TypeError(
                            "LoadMulI64Slot: expected I64".to_string(),
                        ))
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }

            Instr::LoadModI64(name) => {
                let var_val = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals_i64.get(name).copied())
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames
                                .first()
                                .and_then(|frame| frame.locals_i64.get(name).copied())
                        } else {
                            None
                        }
                    });
                match var_val {
                    Some(var) => {
                        let stack_val = self.stack.pop_i64()?;
                        if var == 0 {
                            self.raise(VmError::DivisionByZero)?;
                            return Ok(LocalsResult::Continue);
                        }
                        self.stack.push(Value::I64(stack_val % var));
                        Ok(LocalsResult::Handled)
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name.clone()))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }
            Instr::LoadModI64Slot(slot) => {
                let frame = match self.frames.last() {
                    Some(frame) => frame,
                    None => {
                        self.raise(VmError::UndefVarError(format!("slot {}", slot)))?;
                        return Ok(LocalsResult::Continue);
                    }
                };
                let name = self.slot_name_for_frame(frame, *slot);
                let var_val = frame.locals_slots.get(*slot).and_then(|v| v.as_ref());
                match var_val {
                    Some(Value::I64(var)) => {
                        let stack_val = self.stack.pop_i64()?;
                        if *var == 0 {
                            self.raise(VmError::DivisionByZero)?;
                            return Ok(LocalsResult::Continue);
                        }
                        self.stack.push(Value::I64(stack_val % var));
                        Ok(LocalsResult::Handled)
                    }
                    Some(_) => {
                        Err(VmError::TypeError(
                            "LoadModI64Slot: expected I64".to_string(),
                        ))
                    }
                    None => {
                        self.raise(VmError::UndefVarError(name))?;
                        Ok(LocalsResult::Continue)
                    }
                }
            }

            Instr::IncVarI64(name) => {
                let increment = self.stack.pop_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    if let Some(val) = frame.locals_i64.get_mut(name) {
                        *val += increment;
                        return Ok(LocalsResult::Handled);
                    }
                    // Try global frame
                    if self.frames.len() > 1 {
                        if let Some(val) = self
                            .frames
                            .first_mut()
                            .and_then(|f| f.locals_i64.get_mut(name))
                        {
                            *val += increment;
                            return Ok(LocalsResult::Handled);
                        }
                    }
                }
                self.raise(VmError::UndefVarError(name.clone()))?;
                Ok(LocalsResult::Continue)
            }
            Instr::IncVarI64Slot(slot) => {
                let increment = self.stack.pop_i64()?;
                let name = self
                    .frames
                    .last()
                    .map(|frame| self.slot_name_for_frame(frame, *slot))
                    .unwrap_or_else(|| format!("slot {}", slot));
                if let Some(frame) = self.frames.last_mut() {
                    match frame.locals_slots.get_mut(*slot) {
                        Some(Some(Value::I64(val))) => {
                            *val += increment;
                            return Ok(LocalsResult::Handled);
                        }
                        Some(Some(_)) => {
                            // INTERNAL: IncVarI64Slot is emitted only for I64-typed slots; wrong type is a compiler bug
                            return Err(VmError::InternalError(
                                "IncVarI64Slot: expected I64".to_string(),
                            ));
                        }
                        Some(None) => {
                            self.raise(VmError::UndefVarError(name))?;
                            return Ok(LocalsResult::Continue);
                        }
                        None => {
                            // INTERNAL: slot index is compiler-generated; out-of-bounds means compiler produced an invalid slot
                            return Err(VmError::InternalError(format!(
                                "IncVarI64Slot: slot out of bounds: {}",
                                slot
                            )));
                        }
                    }
                }
                Ok(LocalsResult::Handled)
            }

            Instr::DecVarI64(name) => {
                let decrement = self.stack.pop_i64()?;
                if let Some(frame) = self.frames.last_mut() {
                    if let Some(val) = frame.locals_i64.get_mut(name) {
                        *val -= decrement;
                        return Ok(LocalsResult::Handled);
                    }
                    // Try global frame
                    if self.frames.len() > 1 {
                        if let Some(val) = self
                            .frames
                            .first_mut()
                            .and_then(|f| f.locals_i64.get_mut(name))
                        {
                            *val -= decrement;
                            return Ok(LocalsResult::Handled);
                        }
                    }
                }
                self.raise(VmError::UndefVarError(name.clone()))?;
                Ok(LocalsResult::Continue)
            }
            Instr::DecVarI64Slot(slot) => {
                let decrement = self.stack.pop_i64()?;
                let name = self
                    .frames
                    .last()
                    .map(|frame| self.slot_name_for_frame(frame, *slot))
                    .unwrap_or_else(|| format!("slot {}", slot));
                if let Some(frame) = self.frames.last_mut() {
                    match frame.locals_slots.get_mut(*slot) {
                        Some(Some(Value::I64(val))) => {
                            *val -= decrement;
                            return Ok(LocalsResult::Handled);
                        }
                        Some(Some(_)) => {
                            // INTERNAL: DecVarI64Slot is emitted only for I64-typed slots; wrong type is a compiler bug
                            return Err(VmError::InternalError(
                                "DecVarI64Slot: expected I64".to_string(),
                            ));
                        }
                        Some(None) => {
                            self.raise(VmError::UndefVarError(name))?;
                            return Ok(LocalsResult::Continue);
                        }
                        None => {
                            // INTERNAL: slot index is compiler-generated; out-of-bounds means compiler produced an invalid slot
                            return Err(VmError::InternalError(format!(
                                "DecVarI64Slot: slot out of bounds: {}",
                                slot
                            )));
                        }
                    }
                }
                Ok(LocalsResult::Handled)
            }

            // Variable reflection: check if a variable is defined
            Instr::IsDefined(name) => {
                // Check current frame first
                let current_frame_idx = self.frames.len().saturating_sub(1);
                let defined_in_current = self.is_var_defined_in_frame(name, current_frame_idx);

                if defined_in_current {
                    self.stack.push(Value::Bool(true));
                    return Ok(LocalsResult::Handled);
                }

                // Check type bindings from all frames (for where clause type parameters)
                for frame_idx in (0..self.frames.len()).rev() {
                    if let Some(frame) = self.frames.get(frame_idx) {
                        if frame.type_bindings.contains_key(name) {
                            self.stack.push(Value::Bool(true));
                            return Ok(LocalsResult::Handled);
                        }
                    }
                }

                // Check global frame
                if self.frames.len() > 1 {
                    let defined_in_global = self.is_var_defined_in_frame(name, 0);
                    self.stack.push(Value::Bool(defined_in_global));
                } else {
                    self.stack.push(Value::Bool(false));
                }

                Ok(LocalsResult::Handled)
            }

            _ => Ok(LocalsResult::NotHandled),
        }
    }
}
