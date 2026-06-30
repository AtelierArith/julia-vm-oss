//! Conversion-related type builtins.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// This module implements Julia's `reinterpret` builtin which intentionally
// transmutes bits between signed/unsigned types.  All sign-losing casts here
// are correct by definition of the operation.
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::util::value_type_name;
use super::value::{StructInstance, TupleValue, Value};
use super::Vm;

impl<R: crate::rng::RngLike> Vm<R> {
    /// Convert a heap `Rational`/`Complex` struct to `Float64` for the `float`
    /// builtin (`FloatConv`). Rationals collapse to `num/den`; complex values
    /// reallocate a `Complex{Float64}`. Extracted from the `FloatConv` handler to
    /// keep it flat (Issue #6833).
    fn float_conv_struct_ref(&mut self, idx: usize) -> Result<Value, VmError> {
        let Some(s) = self.struct_heap.get(idx).cloned() else {
            return Err(VmError::TypeError(
                "float: invalid struct reference".to_string(),
            ));
        };
        if s.is_rational() {
            if s.values.len() < 2 {
                return Err(VmError::TypeError(
                    "float: Rational must have num and den fields".to_string(),
                ));
            }
            let num = match &s.values[0] {
                Value::I64(n) => *n as f64,
                Value::I32(n) => *n as f64,
                Value::I16(n) => *n as f64,
                Value::I8(n) => *n as f64,
                Value::Bool(b) => {
                    if *b {
                        1.0
                    } else {
                        0.0
                    }
                }
                Value::F64(f) => *f,
                _ => {
                    return Err(VmError::TypeError(
                        "float: Rational numerator must be numeric".to_string(),
                    ))
                }
            };
            let den = match &s.values[1] {
                Value::I64(d) => *d as f64,
                Value::I32(d) => *d as f64,
                Value::I16(d) => *d as f64,
                Value::I8(d) => *d as f64,
                Value::Bool(b) => {
                    if *b {
                        1.0
                    } else {
                        0.0
                    }
                }
                Value::F64(f) => *f,
                _ => {
                    return Err(VmError::TypeError(
                        "float: Rational denominator must be numeric".to_string(),
                    ))
                }
            };
            return Ok(Value::F64(num / den));
        }
        if s.is_complex() {
            if s.values.len() < 2 {
                return Err(VmError::TypeError(
                    "float: Complex must have re and im fields".to_string(),
                ));
            }
            let re = match &s.values[0] {
                Value::I64(n) => *n as f64,
                Value::F64(f) => *f,
                _ => {
                    return Err(VmError::TypeError(
                        "float: Complex re must be numeric".to_string(),
                    ))
                }
            };
            let im = match &s.values[1] {
                Value::I64(n) => *n as f64,
                Value::F64(f) => *f,
                _ => {
                    return Err(VmError::TypeError(
                        "float: Complex im must be numeric".to_string(),
                    ))
                }
            };
            let complex_struct = StructInstance::with_name(
                s.type_id,
                "Complex{Float64}".to_string(),
                vec![Value::F64(re), Value::F64(im)],
            );
            let new_idx = self.struct_heap.len();
            self.struct_heap.push(complex_struct);
            return Ok(Value::StructRef(new_idx));
        }
        Err(VmError::TypeError(format!(
            "float: cannot convert {} to Float64",
            s.struct_name
        )))
    }

    pub(super) fn execute_builtin_types_conversion(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        match builtin {
            BuiltinId::Convert => {
                let value = self.stack.pop_value()?;
                let target_type = self.stack.pop_value()?;
                let args = vec![target_type.clone(), value.clone()];

                if let Some(func_index) = self.find_best_method_index(&["Base.convert"], &args) {
                    self.start_function_call(func_index, args)?;
                    return Ok(Some(()));
                }

                let type_name_owned: String;
                let type_name: &str = match &target_type {
                    Value::DataType(jt) => {
                        type_name_owned = jt.name().to_string();
                        &type_name_owned
                    }
                    Value::Str(s) => s.as_str(),
                    _ => {
                        return Err(VmError::TypeError(
                            "convert first argument must be a type".to_string(),
                        ))
                    }
                };

                let converted = super::convert::convert_value(type_name, &value);
                match converted {
                    Ok(val) => self.stack.push(val),
                    // An `InexactError` is the final, correct result for a
                    // recognized target type whose value is out of range; do not
                    // fall back to a pure-Julia `convert` method (for `Bool` that
                    // would call the missing `Bool(x)` constructor and mask the
                    // error as "Function 'Bool' not found"). Issue #7970.
                    Err(err @ VmError::InexactError(_)) => return Err(err),
                    Err(err) => {
                        let args = vec![target_type, value];
                        if let Some(func_index) =
                            self.find_best_method_index(&["convert", "Base.convert"], &args)
                        {
                            self.start_function_call(func_index, args)?;
                            return Ok(Some(()));
                        }
                        return Err(err);
                    }
                }
                Ok(Some(()))
            }
            BuiltinId::Promote => {
                let mut values: Vec<Value> = Vec::with_capacity(argc);
                for _ in 0..argc {
                    values.push(self.stack.pop_value()?);
                }
                values.reverse();

                if let Some(func_index) =
                    self.find_best_method_index(&["promote", "Base.promote"], &values)
                {
                    self.start_function_call(func_index, values)?;
                    return Ok(Some(()));
                }
                self.stack
                    .push(Value::Tuple(TupleValue { elements: values }));
                Ok(Some(()))
            }
            BuiltinId::Signed => {
                let val = self.stack.pop_value()?;
                let result = match val {
                    Value::I8(v) => Value::I8(v),
                    Value::I16(v) => Value::I16(v),
                    Value::I32(v) => Value::I32(v),
                    Value::I64(v) => Value::I64(v),
                    Value::I128(v) => Value::I128(v),
                    Value::U8(v) => Value::I8(v as i8),
                    Value::U16(v) => Value::I16(v as i16),
                    Value::U32(v) => Value::I32(v as i32),
                    Value::U64(v) => Value::I64(v as i64),
                    Value::U128(v) => Value::I128(v as i128),
                    Value::F64(v) => Value::I64(v as i64),
                    Value::Bool(b) => Value::I64(if b { 1 } else { 0 }),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "signed: cannot convert {:?} to signed integer",
                            val
                        )))
                    }
                };
                self.stack.push(result);
                Ok(Some(()))
            }
            BuiltinId::Unsigned => {
                let val = self.stack.pop_value()?;
                let result = match val {
                    Value::I8(v) => Value::U8(v as u8),
                    Value::I16(v) => Value::U16(v as u16),
                    Value::I32(v) => Value::U32(v as u32),
                    Value::I64(v) => Value::U64(v as u64),
                    Value::I128(v) => Value::U128(v as u128),
                    Value::U8(v) => Value::U8(v),
                    Value::U16(v) => Value::U16(v),
                    Value::U32(v) => Value::U32(v),
                    Value::U64(v) => Value::U64(v),
                    Value::U128(v) => Value::U128(v),
                    Value::F64(v) => Value::I64(v as u64 as i64),
                    Value::Bool(b) => Value::U64(if b { 1 } else { 0 }),
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "unsigned: cannot convert {:?} to unsigned integer",
                            val
                        )))
                    }
                };
                self.stack.push(result);
                Ok(Some(()))
            }
            BuiltinId::FloatConv => {
                let val = self.stack.pop_value()?;
                let result = match &val {
                    Value::I64(v) => Value::F64(*v as f64),
                    Value::I32(v) => Value::F64(*v as f64),
                    Value::I16(v) => Value::F64(*v as f64),
                    Value::I8(v) => Value::F64(*v as f64),
                    Value::I128(v) => Value::F64(*v as f64),
                    Value::U8(v) => Value::F64(*v as f64),
                    Value::U16(v) => Value::F64(*v as f64),
                    Value::U32(v) => Value::F64(*v as f64),
                    Value::U64(v) => Value::F64(*v as f64),
                    Value::U128(v) => Value::F64(*v as f64),
                    Value::F64(v) => Value::F64(*v),
                    Value::F32(v) => Value::F32(*v),
                    Value::F16(v) => Value::F16(*v),
                    Value::Bool(b) => Value::F64(if *b { 1.0 } else { 0.0 }),
                    Value::StructRef(idx) => self.float_conv_struct_ref(*idx)?,
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "float: cannot convert {:?} to Float64",
                            val
                        )))
                    }
                };
                self.stack.push(result);
                Ok(Some(()))
            }
            // Widemul removed - now Pure Julia (base/number.jl: widen(x)*widen(y),
            // Issue #6737). The old I64-only handler errored on Int32 etc.
            BuiltinId::Reinterpret => {
                let value = self.stack.pop_value()?;
                let target_type = self.stack.pop_value()?;
                let type_name: &str = match &target_type {
                    Value::DataType(jt) => &jt.name(),
                    _ => {
                        return Err(VmError::TypeError(
                            "reinterpret: first argument must be a type".to_string(),
                        ))
                    }
                };

                let result = match type_name {
                    "Float64" => match &value {
                        Value::I64(v) => Value::F64(f64::from_bits(*v as u64)),
                        Value::U64(v) => Value::F64(f64::from_bits(*v)),
                        Value::F64(v) => Value::F64(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Float64, {}): size mismatch (8 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "Int64" => match &value {
                        Value::F64(v) => Value::I64(v.to_bits() as i64),
                        Value::U64(v) => Value::I64(*v as i64),
                        Value::I64(v) => Value::I64(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Int64, {}): size mismatch (8 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "UInt64" => match &value {
                        Value::F64(v) => Value::U64(v.to_bits()),
                        Value::I64(v) => Value::U64(*v as u64),
                        Value::U64(v) => Value::U64(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(UInt64, {}): size mismatch (8 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "Float32" => match &value {
                        Value::I32(v) => Value::F32(f32::from_bits(*v as u32)),
                        Value::U32(v) => Value::F32(f32::from_bits(*v)),
                        Value::F32(v) => Value::F32(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Float32, {}): size mismatch (4 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "Int32" => match &value {
                        Value::F32(v) => Value::I32(v.to_bits() as i32),
                        Value::U32(v) => Value::I32(*v as i32),
                        Value::I32(v) => Value::I32(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Int32, {}): size mismatch (4 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "UInt32" => match &value {
                        Value::F32(v) => Value::U32(v.to_bits()),
                        Value::I32(v) => Value::U32(*v as u32),
                        Value::U32(v) => Value::U32(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(UInt32, {}): size mismatch (4 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    // Float16 <-> UInt16/Int16 bit reinterpretation (Issue #6740):
                    // needed by the pure-Julia float decomposition for Float16.
                    "Float16" => match &value {
                        Value::U16(v) => Value::F16(half::f16::from_bits(*v)),
                        Value::I16(v) => Value::F16(half::f16::from_bits(*v as u16)),
                        Value::F16(v) => Value::F16(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Float16, {}): size mismatch (2 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "Int16" => match &value {
                        Value::U16(v) => Value::I16(*v as i16),
                        Value::I16(v) => Value::I16(*v),
                        Value::F16(v) => Value::I16(v.to_bits() as i16),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Int16, {}): size mismatch (2 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "UInt16" => match &value {
                        Value::I16(v) => Value::U16(*v as u16),
                        Value::U16(v) => Value::U16(*v),
                        Value::F16(v) => Value::U16(v.to_bits()),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(UInt16, {}): size mismatch (2 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "Int8" => match &value {
                        Value::U8(v) => Value::I8(*v as i8),
                        Value::I8(v) => Value::I8(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Int8, {}): size mismatch (1 byte required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "UInt8" => match &value {
                        Value::I8(v) => Value::U8(*v as u8),
                        Value::U8(v) => Value::U8(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(UInt8, {}): size mismatch (1 byte required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "Int128" => match &value {
                        Value::U128(v) => Value::I128(*v as i128),
                        Value::I128(v) => Value::I128(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(Int128, {}): size mismatch (16 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    "UInt128" => match &value {
                        Value::I128(v) => Value::U128(*v as u128),
                        Value::U128(v) => Value::U128(*v),
                        _ => {
                            return Err(VmError::TypeError(format!(
                                "reinterpret(UInt128, {}): size mismatch (16 bytes required)",
                                value_type_name(&value)
                            )))
                        }
                    },
                    _ => {
                        return Err(VmError::TypeError(format!(
                            "reinterpret: unsupported target type '{}'",
                            type_name
                        )))
                    }
                };

                self.stack.push(result);
                Ok(Some(()))
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::builtins::BuiltinId;
    use crate::rng::StableRng;
    use crate::vm::stack_ops::StackOps;
    use crate::vm::value::Value;
    use crate::vm::Vm;

    fn run_conversion_builtin(builtin: BuiltinId, input: Value) -> Value {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.stack.push(input);
        if let Err(err) = vm.execute_builtin_types_conversion(&builtin, 1) {
            panic!("conversion builtin failed: {err:?}");
        }
        match vm.stack.pop_value() {
            Ok(value) => value,
            Err(err) => panic!("conversion builtin left no result: {err:?}"),
        }
    }

    #[test]
    fn signed_builtin_reinterprets_integer_widths() {
        let cases = [
            (Value::I8(-1), Value::I8(-1)),
            (Value::I16(-1), Value::I16(-1)),
            (Value::I32(-1), Value::I32(-1)),
            (Value::I64(-1), Value::I64(-1)),
            (Value::I128(-1), Value::I128(-1)),
            (Value::U8(u8::MAX), Value::I8(-1)),
            (Value::U16(u16::MAX), Value::I16(-1)),
            (Value::U32(u32::MAX), Value::I32(-1)),
            (Value::U64(u64::MAX), Value::I64(-1)),
            (Value::U128(u128::MAX), Value::I128(-1)),
            (Value::Bool(true), Value::I64(1)),
        ];

        for (input, expected) in cases {
            let actual = run_conversion_builtin(BuiltinId::Signed, input.clone());
            assert!(
                matches_expected_value(&actual, &expected),
                "signed({input:?}) produced {actual:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn unsigned_builtin_reinterprets_integer_widths() {
        let cases = [
            (Value::I8(-1), Value::U8(u8::MAX)),
            (Value::I16(-1), Value::U16(u16::MAX)),
            (Value::I32(-1), Value::U32(u32::MAX)),
            (Value::I64(-1), Value::U64(u64::MAX)),
            (Value::I128(-1), Value::U128(u128::MAX)),
            (Value::U8(u8::MAX), Value::U8(u8::MAX)),
            (Value::U16(u16::MAX), Value::U16(u16::MAX)),
            (Value::U32(u32::MAX), Value::U32(u32::MAX)),
            (Value::U64(u64::MAX), Value::U64(u64::MAX)),
            (Value::U128(u128::MAX), Value::U128(u128::MAX)),
            (Value::Bool(true), Value::U64(1)),
        ];

        for (input, expected) in cases {
            let actual = run_conversion_builtin(BuiltinId::Unsigned, input.clone());
            assert!(
                matches_expected_value(&actual, &expected),
                "unsigned({input:?}) produced {actual:?}, expected {expected:?}"
            );
        }
    }

    fn matches_expected_value(actual: &Value, expected: &Value) -> bool {
        match (actual, expected) {
            (Value::I8(a), Value::I8(b)) => a == b,
            (Value::I16(a), Value::I16(b)) => a == b,
            (Value::I32(a), Value::I32(b)) => a == b,
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::I128(a), Value::I128(b)) => a == b,
            (Value::U8(a), Value::U8(b)) => a == b,
            (Value::U16(a), Value::U16(b)) => a == b,
            (Value::U32(a), Value::U32(b)) => a == b,
            (Value::U64(a), Value::U64(b)) => a == b,
            (Value::U128(a), Value::U128(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            _ => false,
        }
    }
}
