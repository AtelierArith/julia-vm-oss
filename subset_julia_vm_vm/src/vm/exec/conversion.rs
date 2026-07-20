//! Type conversion operations for the VM.
//!
//! This module handles type conversion instructions including:
//! - ToF64, ToI64: Basic numeric conversions
//! - BoolToI64, I64ToBool: Boolean/integer conversions
//! - NotBool: Boolean negation

// SAFETY: DynamicToU8/U16/U32/U64 cast i64 to unsigned types; these are
// reinterpretation casts matching Julia's unsigned type constructors (wrapping semantics).
#![allow(clippy::cast_sign_loss)]
//! - DynamicToBool, DynamicToF64, DynamicToI64: Dynamic type conversions

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::Value;
use super::super::Vm;
use super::DispatchAction;

// Issue #5160: Rational numerator/denominator extraction is centralized on
// `StructInstance` (`is_rational` / `as_rational_parts_i64` /
// `as_rational_parts_f64`), mirroring the `is_complex` / `as_complex_parts`
// helpers, instead of being duplicated inline in each conversion arm here.

impl<R: RngLike> Vm<R> {
    /// Execute type conversion instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_conversion(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::ToF64 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::F64(v as f64)),
                    Value::I128(v) => self.stack.push(Value::F64(v as f64)),
                    Value::I32(v) => self.stack.push(Value::F64(v as f64)),
                    Value::I16(v) => self.stack.push(Value::F64(v as f64)),
                    Value::I8(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U64(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U128(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U32(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U16(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U8(v) => self.stack.push(Value::F64(v as f64)),
                    Value::F64(v) => self.stack.push(Value::F64(v)),
                    Value::F32(v) => self.stack.push(Value::F64(v as f64)),
                    Value::F16(v) => self.stack.push(Value::F64(v.to_f64())),
                    Value::Bool(b) => self.stack.push(Value::F64(if b { 1.0 } else { 0.0 })),
                    Value::BigInt(ref b) => {
                        // Convert BigInt to F64 (may lose precision for large values)
                        use num_traits::ToPrimitive;
                        let f = b.to_f64().unwrap_or(f64::INFINITY);
                        self.stack.push(Value::F64(f));
                    }
                    Value::Struct(ref s) if s.is_rational() => {
                        if let Some((num, den)) = s.as_rational_parts_f64() {
                            self.stack.push(Value::F64(num / den));
                        } else {
                            return Err(VmError::type_error_expected(
                                "ToF64",
                                "numeric Rational fields",
                                &val,
                            ));
                        }
                    }
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(idx) {
                            if s.is_rational() {
                                if let Some((num, den)) = s.as_rational_parts_f64() {
                                    self.stack.push(Value::F64(num / den));
                                    return Ok(DispatchAction::Continue);
                                }
                            }
                        }
                        return Err(VmError::type_error_expected("ToF64", "numeric", &val));
                    }
                    other => {
                        return Err(VmError::type_error_expected("ToF64", "numeric", &other));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::ToI64 => {
                match self.stack.pop_value()? {
                    Value::F64(v) => self.stack.push(Value::I64(v as i64)),
                    Value::F32(v) => self.stack.push(Value::I64(v as i64)),
                    Value::F16(v) => self.stack.push(Value::I64(v.to_f64() as i64)),
                    Value::I64(v) => self.stack.push(Value::I64(v)),
                    Value::I128(v) => self.stack.push(Value::I64(v as i64)),
                    Value::I32(v) => self.stack.push(Value::I64(v as i64)),
                    Value::I16(v) => self.stack.push(Value::I64(v as i64)),
                    Value::I8(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U64(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U128(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U32(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U16(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U8(v) => self.stack.push(Value::I64(v as i64)),
                    Value::Bool(b) => self.stack.push(Value::I64(if b { 1 } else { 0 })),
                    // Char: convert codepoint to I64 (Issue #1875)
                    Value::Char(c) => self.stack.push(Value::I64(c as i64)),
                    other => {
                        return Err(VmError::type_error_expected("ToI64", "numeric", &other));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::BoolToI64 => {
                match self.stack.pop_value()? {
                    Value::Bool(b) => self.stack.push(Value::I64(if b { 1 } else { 0 })),
                    // If already I64, just pass through (type mismatch between inference and runtime)
                    Value::I64(v) => self.stack.push(Value::I64(v)),
                    other => {
                        return Err(VmError::type_error_expected(
                            "BoolToI64",
                            "Bool or I64",
                            &other,
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::I64ToBool => {
                match self.stack.pop_value()? {
                    Value::I64(v) => self.stack.push(Value::Bool(v != 0)),
                    // If already Bool, just pass through (type mismatch between inference and runtime)
                    Value::Bool(b) => self.stack.push(Value::Bool(b)),
                    other => {
                        return Err(VmError::type_error_expected(
                            "I64ToBool",
                            "I64 or Bool",
                            &other,
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::NotBool => {
                match self.stack.pop_value()? {
                    Value::Bool(b) => self.stack.push(Value::Bool(!b)),
                    // `!(missing) === missing` (Issue #5267). Tuple/named-tuple
                    // `!=` lowers to `==` followed by `NotBool`; a `missing`
                    // `==` result (e.g. `(1, missing) != (1, 2)`) must stay
                    // `missing` rather than raising a type error.
                    Value::Missing => self.stack.push(Value::Missing),
                    other => {
                        return Err(VmError::type_error_expected("NotBool", "Bool", &other));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToBool => {
                // Dynamic conversion from Any type to Bool
                // Only Bool values are accepted, others raise TypeError
                match self.stack.pop_value()? {
                    Value::Bool(b) => self.stack.push(Value::Bool(b)),
                    // Allow I64 as truthy (0 is false, non-zero is true) for backwards compat
                    Value::I64(v) => self.stack.push(Value::Bool(v != 0)),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToBool",
                            "Bool",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToF64 => {
                // Dynamic conversion from Any type to F64
                match self.stack.pop_value()? {
                    Value::I64(v) => self.stack.push(Value::F64(v as f64)),
                    Value::F64(v) => self.stack.push(Value::F64(v)),
                    // Bool: convert to 0.0 or 1.0 (Julia: Float64(true) == 1.0)
                    Value::Bool(b) => self.stack.push(Value::F64(if b { 1.0 } else { 0.0 })),
                    // New numeric types
                    Value::I8(v) => self.stack.push(Value::F64(v as f64)),
                    Value::I16(v) => self.stack.push(Value::F64(v as f64)),
                    Value::I32(v) => self.stack.push(Value::F64(v as f64)),
                    Value::I128(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U8(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U16(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U32(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U64(v) => self.stack.push(Value::F64(v as f64)),
                    Value::U128(v) => self.stack.push(Value::F64(v as f64)),
                    Value::F32(v) => self.stack.push(Value::F64(v as f64)),
                    Value::F16(v) => self.stack.push(Value::F64(v.to_f64())),
                    // BigInt/BigFloat: convert to F64
                    Value::BigInt(ref n) => {
                        let f = n.to_string().parse::<f64>().unwrap_or(f64::NAN);
                        self.stack.push(Value::F64(f));
                    }
                    Value::BigFloat(ref n) => {
                        let f = n.to_string().parse::<f64>().unwrap_or(f64::NAN);
                        self.stack.push(Value::F64(f));
                    }
                    // Complex: extract real part (like real() function)
                    // Handle both "Complex" and "Complex{Float64}" struct names
                    Value::Struct(s) if s.is_complex() => {
                        if let Some(Value::F64(re)) = s.values.first() {
                            self.stack.push(Value::F64(*re));
                        } else {
                            self.stack.push(Value::F64(0.0));
                        }
                    }
                    // Rational: convert to Float64 via num/den
                    Value::Struct(s) if s.is_rational() => {
                        if let Some((num, den)) = s.as_rational_parts_f64() {
                            self.stack.push(Value::F64(num / den));
                        } else {
                            // INTERNAL: Rational struct with wrong field types — structural invariant.
                            return Err(VmError::InternalError(
                                "DynamicToF64: invalid Rational struct fields".to_string(),
                            ));
                        }
                    }
                    // Irrational singleton: convert to Float64 (Issue #5133)
                    Value::Struct(s) if s.as_irrational_f64().is_some() => {
                        if let Some(v) = s.as_irrational_f64() {
                            self.stack.push(Value::F64(v));
                        }
                    }
                    // Dates module structs: convert value to F64
                    Value::Struct(s) if Self::is_dates_struct(&s.struct_name) => {
                        if let Some(Value::I64(v)) = s.values.first() {
                            self.stack.push(Value::F64(*v as f64));
                        } else {
                            // INTERNAL: Dates struct with wrong field type — structural invariant.
                            return Err(VmError::InternalError(format!(
                                "DynamicToF64: {} struct has no Int64 value field",
                                s.struct_name
                            )));
                        }
                    }
                    // StructRef: look up the struct on the heap and extract real part
                    Value::StructRef(idx) => {
                        let s = &self.struct_heap[idx];
                        if s.is_complex() {
                            if let Some(Value::F64(re)) = s.values.first() {
                                self.stack.push(Value::F64(*re));
                            } else {
                                self.stack.push(Value::F64(0.0));
                            }
                        } else if s.is_rational() {
                            if let Some((num, den)) = s.as_rational_parts_f64() {
                                self.stack.push(Value::F64(num / den));
                            } else {
                                // INTERNAL: Rational StructRef with wrong field types — structural invariant.
                                return Err(VmError::InternalError(
                                    "DynamicToF64: invalid Rational struct fields (StructRef)"
                                        .to_string(),
                                ));
                            }
                        } else if let Some(v) = s.as_irrational_f64() {
                            self.stack.push(Value::F64(v));
                        } else if Self::is_dates_struct(&s.struct_name) {
                            // Dates module structs: convert value to F64
                            if let Some(Value::I64(v)) = s.values.first() {
                                self.stack.push(Value::F64(*v as f64));
                            } else {
                                // INTERNAL: Dates StructRef with wrong field type — structural invariant.
                                return Err(VmError::InternalError(format!(
                                    "DynamicToF64: {} struct has no Int64 value field (StructRef)",
                                    s.struct_name
                                )));
                            }
                        } else {
                            // User-visible: DynamicToF64 on unsupported struct type — runtime TypeError.
                            return Err(VmError::TypeError(format!(
                                "DynamicToF64: expected numeric, got StructRef to {}",
                                s.struct_name
                            )));
                        }
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToF64",
                            "numeric",
                            &other,
                        ));
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToF32 => {
                // Dynamic conversion from Any type to F32
                match self.stack.pop_value()? {
                    Value::F32(v) => self.stack.push(Value::F32(v)),
                    Value::I64(v) => self.stack.push(Value::F32(v as f32)),
                    Value::F64(v) => self.stack.push(Value::F32(v as f32)),
                    // New numeric types
                    Value::I8(v) => self.stack.push(Value::F32(v as f32)),
                    Value::I16(v) => self.stack.push(Value::F32(v as f32)),
                    Value::I32(v) => self.stack.push(Value::F32(v as f32)),
                    Value::I128(v) => self.stack.push(Value::F32(v as f32)),
                    Value::U8(v) => self.stack.push(Value::F32(v as f32)),
                    Value::U16(v) => self.stack.push(Value::F32(v as f32)),
                    Value::U32(v) => self.stack.push(Value::F32(v as f32)),
                    Value::U64(v) => self.stack.push(Value::F32(v as f32)),
                    Value::U128(v) => self.stack.push(Value::F32(v as f32)),
                    Value::F16(v) => self.stack.push(Value::F32(v.to_f32())),
                    Value::Bool(b) => self.stack.push(Value::F32(if b { 1.0 } else { 0.0 })),
                    Value::Struct(s) if s.as_irrational_f64().is_some() => {
                        if let Some(v) = s.as_irrational_f64() {
                            self.stack.push(Value::F32(v as f32));
                        }
                    }
                    Value::StructRef(idx) => {
                        let s = &self.struct_heap[idx];
                        if let Some(v) = s.as_irrational_f64() {
                            self.stack.push(Value::F32(v as f32));
                        } else {
                            return Err(VmError::type_error_expected(
                                "DynamicToF32",
                                "numeric",
                                &Value::StructRef(idx),
                            ));
                        }
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToF32",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToF16 => {
                // Dynamic conversion from Any type to F16
                use half::f16;
                match self.stack.pop_value()? {
                    Value::F16(v) => self.stack.push(Value::F16(v)),
                    Value::I64(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::F64(v) => self.stack.push(Value::F16(f16::from_f64(v))),
                    Value::F32(v) => self.stack.push(Value::F16(f16::from_f32(v))),
                    // New numeric types
                    Value::I8(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::I16(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::I32(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::I128(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::U8(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::U16(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::U32(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::U64(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::U128(v) => self.stack.push(Value::F16(f16::from_f64(v as f64))),
                    Value::Bool(b) => {
                        self.stack
                            .push(Value::F16(f16::from_f64(if b { 1.0 } else { 0.0 })));
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToF16",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToI64 => {
                // Dynamic conversion from Any type to I64
                match self.stack.pop_value()? {
                    Value::I64(v) => self.stack.push(Value::I64(v)),
                    Value::F64(v) => self.stack.push(Value::I64(v as i64)),
                    // Bool: true -> 1, false -> 0 (Julia semantics: Int(true) == 1)
                    Value::Bool(b) => self.stack.push(Value::I64(if b { 1 } else { 0 })),
                    // New numeric types
                    Value::I8(v) => self.stack.push(Value::I64(v as i64)),
                    Value::I16(v) => self.stack.push(Value::I64(v as i64)),
                    Value::I32(v) => self.stack.push(Value::I64(v as i64)),
                    Value::I128(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U8(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U16(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U32(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U64(v) => self.stack.push(Value::I64(v as i64)),
                    Value::U128(v) => self.stack.push(Value::I64(v as i64)),
                    Value::F32(v) => self.stack.push(Value::I64(v as i64)),
                    Value::F16(v) => self.stack.push(Value::I64(v.to_f64() as i64)),
                    // Char: convert codepoint to I64 (Issue #1875)
                    Value::Char(c) => self.stack.push(Value::I64(c as i64)),
                    // BigInt/BigFloat: convert to I64
                    Value::BigInt(ref n) => {
                        let i = n.to_string().parse::<i64>().unwrap_or(0);
                        self.stack.push(Value::I64(i));
                    }
                    Value::BigFloat(ref n) => {
                        let i = n.to_string().parse::<f64>().unwrap_or(0.0) as i64;
                        self.stack.push(Value::I64(i));
                    }
                    // Complex: extract real part and convert to I64 (like Int(real(z)))
                    Value::Struct(s) if s.is_complex() => {
                        if let Some(Value::F64(re)) = s.values.first() {
                            self.stack.push(Value::I64(*re as i64));
                        } else if let Some(Value::I64(re)) = s.values.first() {
                            self.stack.push(Value::I64(*re));
                        } else {
                            self.stack.push(Value::I64(0));
                        }
                    }
                    // Rational: convert to I64 via num/den
                    Value::Struct(s) if s.is_rational() => {
                        if let Some((num, den)) = s.as_rational_parts_i64() {
                            self.stack.push(Value::I64(num / den));
                        } else {
                            // INTERNAL: Rational struct with wrong field types — structural invariant.
                            return Err(VmError::InternalError(
                                "DynamicToI64: invalid Rational struct fields".to_string(),
                            ));
                        }
                    }
                    // Dates module structs (Date, DateTime, Time, Year, Month, Day, etc.)
                    // all have a single `value::Int64` field
                    Value::Struct(s) if Self::is_dates_struct(&s.struct_name) => {
                        if let Some(Value::I64(v)) = s.values.first() {
                            self.stack.push(Value::I64(*v));
                        } else {
                            // INTERNAL: Dates struct with wrong field type — structural invariant.
                            return Err(VmError::InternalError(format!(
                                "DynamicToI64: {} struct has no Int64 value field",
                                s.struct_name
                            )));
                        }
                    }
                    // StructRef: look up the struct on the heap and convert
                    Value::StructRef(idx) => {
                        let s = &self.struct_heap[idx];
                        if s.is_complex() {
                            if let Some(Value::F64(re)) = s.values.first() {
                                self.stack.push(Value::I64(*re as i64));
                            } else if let Some(Value::I64(re)) = s.values.first() {
                                self.stack.push(Value::I64(*re));
                            } else {
                                self.stack.push(Value::I64(0));
                            }
                        } else if s.is_rational() {
                            if let Some((num, den)) = s.as_rational_parts_i64() {
                                self.stack.push(Value::I64(num / den));
                            } else {
                                // INTERNAL: Rational StructRef with wrong field types — structural invariant.
                                return Err(VmError::InternalError(
                                    "DynamicToI64: invalid Rational struct fields (StructRef)"
                                        .to_string(),
                                ));
                            }
                        } else if Self::is_dates_struct(&s.struct_name) {
                            // Dates module structs (Date, DateTime, Time, Year, Month, Day, etc.)
                            // all have a single `value::Int64` field
                            if let Some(Value::I64(v)) = s.values.first() {
                                self.stack.push(Value::I64(*v));
                            } else {
                                // INTERNAL: Dates StructRef with wrong field type — structural invariant.
                                return Err(VmError::InternalError(format!(
                                    "DynamicToI64: {} struct has no Int64 value field (StructRef)",
                                    s.struct_name
                                )));
                            }
                        } else {
                            // User-visible: DynamicToI64 on unsupported struct type — runtime TypeError.
                            return Err(VmError::TypeError(format!(
                                "DynamicToI64: cannot convert StructRef to I64, got StructRef to {}", s.struct_name
                            )));
                        }
                    }
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToI64",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            // Small integer back-conversion instructions (Issue #2278)
            // These truncate I64 results back to the original small integer type,
            // mirroring DynamicToF32/DynamicToF16 for float types.
            Instr::DynamicToI8 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::I8(v as i8)),
                    Value::I8(v) => self.stack.push(Value::I8(v)),
                    Value::Bool(b) => self.stack.push(Value::I8(if b { 1 } else { 0 })),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToI8",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToI16 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::I16(v as i16)),
                    Value::I16(v) => self.stack.push(Value::I16(v)),
                    Value::Bool(b) => self.stack.push(Value::I16(if b { 1 } else { 0 })),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToI16",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToI32 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::I32(v as i32)),
                    Value::I32(v) => self.stack.push(Value::I32(v)),
                    Value::Bool(b) => self.stack.push(Value::I32(if b { 1 } else { 0 })),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToI32",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToU8 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::U8(v as u8)),
                    Value::U8(v) => self.stack.push(Value::U8(v)),
                    Value::Bool(b) => self.stack.push(Value::U8(if b { 1 } else { 0 })),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToU8",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToU16 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::U16(v as u16)),
                    Value::U16(v) => self.stack.push(Value::U16(v)),
                    Value::Bool(b) => self.stack.push(Value::U16(if b { 1 } else { 0 })),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToU16",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToU32 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::U32(v as u32)),
                    Value::U32(v) => self.stack.push(Value::U32(v)),
                    Value::Bool(b) => self.stack.push(Value::U32(if b { 1 } else { 0 })),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToU32",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::DynamicToU64 => {
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(v) => self.stack.push(Value::U64(v as u64)),
                    Value::U64(v) => self.stack.push(Value::U64(v)),
                    Value::Bool(b) => self.stack.push(Value::U64(if b { 1 } else { 0 })),
                    other => {
                        return Err(VmError::type_error_expected(
                            "DynamicToU64",
                            "numeric",
                            &other,
                        ))
                    }
                }
                Ok(DispatchAction::Continue)
            }

            // Type checking
            Instr::IsNothing => {
                let val = self.stack.pop_value()?;
                let is_nothing = matches!(val, Value::Nothing);
                self.stack.push(Value::Bool(is_nothing));
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    /// Check if a struct name belongs to the Dates module
    /// These structs all have a single `value::Int64` field
    fn is_dates_struct(name: &str) -> bool {
        // Strip module prefix if present (e.g., "Dates.Date" -> "Date")
        let base_name = name.strip_prefix("Dates.").unwrap_or(name);
        matches!(
            base_name,
            "Date"
                | "DateTime"
                | "Time"
                | "Year"
                | "Quarter"
                | "Month"
                | "Week"
                | "Day"
                | "Hour"
                | "Minute"
                | "Second"
                | "Millisecond"
                | "Microsecond"
                | "Nanosecond"
        )
    }
}
