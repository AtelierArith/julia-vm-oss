//! Array mutation instructions.
//!
//! Handles: Zero, ArrayPush, ArrayPop, ArrayPushFirst, ArrayPopFirst,
//!          ArrayInsert, ArrayDeleteAt

#![deny(clippy::unwrap_used)]
// SAFETY: i64→usize casts for insert/delete indices are VM-internal values
// that are valid array indices by construction.
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::stack_ops::StackOps;
use crate::rng::RngLike;

/// Result type for array mutation operations
pub(super) enum ArrayMutateResult {
    /// Instruction was not handled by this module
    NotHandled,
    /// Instruction was handled successfully
    Handled,
    /// Instruction handled, but need to continue (e.g., after raise)
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute array mutation instructions.
    ///
    /// Returns `ArrayMutateResult::NotHandled` if the instruction is not an array mutation operation.
    #[inline]
    pub(super) fn execute_array_mutate(
        &mut self,
        instr: &Instr,
    ) -> Result<ArrayMutateResult, VmError> {
        match instr {
            Instr::Zero => {
                // zero(x) - return zero of the same type as x
                // zero(T) - return zero of type T (Issue #2181)
                let val = self.stack.pop_value()?;
                // Check if value is Complex (inline or heap reference)
                let is_complex_val = match &val {
                    Value::Struct(s) => s.is_complex(),
                    Value::StructRef(idx) => {
                        self.struct_heap.get(*idx).is_some_and(|s| s.is_complex())
                    }
                    _ => false,
                };
                if is_complex_val {
                    // Return zero Complex as heap-allocated struct
                    let complex_val = self.create_complex(self.get_complex_type_id(), 0.0, 0.0);
                    self.stack.push(complex_val);
                } else {
                    match val {
                        // Value-based: zero(x) where x is a value
                        Value::F64(_) => self.stack.push(Value::F64(0.0)),
                        Value::F32(_) => self.stack.push(Value::F32(0.0)),
                        Value::F16(_) => self.stack.push(Value::F16(half::f16::ZERO)),
                        Value::I64(_) => self.stack.push(Value::I64(0)),
                        Value::I32(_) => self.stack.push(Value::I32(0)),
                        Value::I16(_) => self.stack.push(Value::I16(0)),
                        Value::I8(_) => self.stack.push(Value::I8(0)),
                        Value::I128(_) => self.stack.push(Value::I128(0)),
                        Value::U8(_) => self.stack.push(Value::U8(0)),
                        Value::U16(_) => self.stack.push(Value::U16(0)),
                        Value::U32(_) => self.stack.push(Value::U32(0)),
                        Value::U64(_) => self.stack.push(Value::U64(0)),
                        Value::U128(_) => self.stack.push(Value::U128(0)),
                        Value::Bool(_) => self.stack.push(Value::Bool(false)),
                        // Type-based: zero(Int64), zero(Float32), etc.
                        Value::DataType(ref jt) => {
                            use crate::types::JuliaType;
                            let result = match jt {
                                JuliaType::Int64 => Value::I64(0),
                                JuliaType::Int32 => Value::I32(0),
                                JuliaType::Int16 => Value::I16(0),
                                JuliaType::Int8 => Value::I8(0),
                                JuliaType::Int128 => Value::I128(0),
                                JuliaType::UInt8 => Value::U8(0),
                                JuliaType::UInt16 => Value::U16(0),
                                JuliaType::UInt32 => Value::U32(0),
                                JuliaType::UInt64 => Value::U64(0),
                                JuliaType::UInt128 => Value::U128(0),
                                JuliaType::Float64 => Value::F64(0.0),
                                JuliaType::Float32 => Value::F32(0.0),
                                JuliaType::Float16 => Value::F16(half::f16::ZERO),
                                JuliaType::Bool => Value::Bool(false),
                                _ => Value::F64(0.0), // Fallback for unknown types
                            };
                            self.stack.push(result);
                        }
                        Value::Array(arr) => {
                            // Return zero array of same shape and type
                            let arr = arr.borrow();
                            self.stack
                                .push(Value::Array(new_array_ref(ArrayValue::zeros(
                                    arr.shape.clone(),
                                ))));
                        }
                        // Memory → zero array of same shape (Issue #2764)
                        Value::Memory(mem) => {
                            let mem = mem.borrow();
                            self.stack
                                .push(Value::Array(new_array_ref(ArrayValue::zeros(
                                    vec![mem.len()],
                                ))));
                        }
                        _ => self.stack.push(Value::F64(0.0)), // Default to 0.0
                    }
                }
                Ok(ArrayMutateResult::Handled)
            }

            Instr::ArrayPush => {
                // Pop value first (as f64 for legacy Array, or keep as Value for TypedArray)
                let val = self.stack.pop_value()?;
                let arr_val = self.stack.pop_value()?;

                // Normalize Memory → Array (Issue #2764)
                let arr_val = match arr_val {
                    Value::Memory(mem) => Value::Array(util::memory_to_array_ref(&mem)),
                    other => other,
                };
                match arr_val {
                    Value::Array(arr) => {
                        // Push value in-place
                        let push_result = arr.borrow_mut().push(val);
                        if self.try_or_handle(push_result)?.is_none() {
                            return Ok(ArrayMutateResult::Continue);
                        }
                        self.stack.push(Value::Array(arr));
                    }
                    Value::Set(mut set) => {
                        // Runtime fallback: push! on Set adds element (Issue #1828)
                        // The compiler may have coerced integers to F64, so convert back
                        // Convert integer-valued F64 back to I64 for Set keys (Issue #1828)
                        // All other values pass through unchanged for Set element storage
                        let set_val = match &val {
                            Value::F64(f) if *f == (*f as i64) as f64 => Value::I64(*f as i64),
                            other => other.clone(),
                        };
                        let key = DictKey::from_value(&set_val).map_err(|_| {
                            VmError::TypeError(format!(
                                "invalid Set element type: {:?}",
                                util::value_type_name(&val)
                            ))
                        })?;
                        set.insert(key);
                        self.stack.push(Value::Set(set));
                    }
                    other => {
                        // User-visible: push! on a non-array/set is a runtime type mismatch — user can trigger via Any-typed dispatch
                        return Err(VmError::TypeError(format!(
                            "ArrayPush: expected Array or Set, got {:?}",
                            util::value_type_name(&other)
                        )))
                    }
                }
                Ok(ArrayMutateResult::Handled)
            }

            Instr::ArrayPop => {
                let arr_val = self.stack.pop_value()?;
                // Normalize Memory → Array (Issue #2764)
                let arr_val = match arr_val {
                    Value::Memory(mem) => Value::Array(util::memory_to_array_ref(&mem)),
                    other => other,
                };

                match arr_val {
                    Value::Array(arr) => {
                        let val = {
                            let pop_result = arr.borrow_mut().pop();
                            match self.try_or_handle(pop_result)? {
                                Some(val) => val,
                                None => return Ok(ArrayMutateResult::Continue),
                            }
                        };
                        self.stack.push(Value::Array(arr));
                        self.stack.push(val);
                    }
                    Value::Set(mut set) => {
                        // Runtime fallback: pop! on Set removes and returns an arbitrary element (Issue #1832)
                        if set.is_empty() {
                            // User-visible: pop!(empty_set) throws ArgumentError in Julia — catchable.
                            self.raise(VmError::TypeError(
                                "ArgumentError: Set must be non-empty".to_string(),
                            ))?;
                            return Ok(ArrayMutateResult::Continue);
                        }
                        // Remove an arbitrary element (first in iteration order)
                        // Safety: is_empty() check above guarantees at least one element
                        let key = match set.iter().next() {
                            Some(k) => k.clone(),
                            None => {
                                return Err(VmError::InternalError(
                                    "Set must be non-empty (unreachable: checked above)".to_string(),
                                ))
                            }
                        };
                        set.remove(&key);
                        let val = key.to_value();
                        self.stack.push(Value::Set(set));
                        self.stack.push(val);
                    }
                    other => {
                        // User-visible: pop! on a non-array/set is a runtime type mismatch — user can trigger via Any-typed dispatch
                        return Err(VmError::TypeError(format!(
                            "ArrayPop: expected Array or Set, got {:?}",
                            util::value_type_name(&other)
                        )))
                    }
                }
                Ok(ArrayMutateResult::Handled)
            }

            Instr::ArrayPushFirst => {
                let val = self.pop_f64_or_i64()?;
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(ArrayMutateResult::Continue),
                };
                {
                    let mut arr = arr.borrow_mut();
                    let result = arr.push_first(Value::F64(val));
                    if self.try_or_handle(result)?.is_none() {
                        return Ok(ArrayMutateResult::Continue);
                    }
                }
                self.stack.push(Value::Array(arr));
                Ok(ArrayMutateResult::Handled)
            }

            Instr::ArrayPopFirst => {
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(ArrayMutateResult::Continue),
                };
                let val = {
                    let mut arr = arr.borrow_mut();
                    let result = arr.pop_first();
                    match self.try_or_handle(result)? {
                        Some(val) => val,
                        None => return Ok(ArrayMutateResult::Continue),
                    }
                };
                self.stack.push(Value::Array(arr));
                self.stack.push(val);
                Ok(ArrayMutateResult::Handled)
            }

            Instr::ArrayInsert => {
                let val = self.pop_f64_or_i64()?;
                let index = self.stack.pop_i64()?;
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(ArrayMutateResult::Continue),
                };
                {
                    let mut arr = arr.borrow_mut();
                    let result = arr.insert_at(index as usize, Value::F64(val));
                    if self.try_or_handle(result)?.is_none() {
                        return Ok(ArrayMutateResult::Continue);
                    }
                }
                self.stack.push(Value::Array(arr));
                Ok(ArrayMutateResult::Handled)
            }

            Instr::ArrayDeleteAt => {
                let index = self.stack.pop_i64()?;
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(ArrayMutateResult::Continue),
                };
                {
                    let mut arr = arr.borrow_mut();
                    let result = arr.delete_at(index as usize);
                    if self.try_or_handle(result)?.is_none() {
                        return Ok(ArrayMutateResult::Continue);
                    }
                }
                self.stack.push(Value::Array(arr));
                Ok(ArrayMutateResult::Handled)
            }

            _ => Ok(ArrayMutateResult::NotHandled),
        }
    }
}
