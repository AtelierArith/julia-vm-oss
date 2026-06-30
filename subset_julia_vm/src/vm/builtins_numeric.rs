//! Numeric type constructor builtin functions for the VM.
//!
//! BigInt, BigFloat, and numeric type conversions (Int8, Int16, etc.)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// SAFETY: i64→usize cast for BigFloat precision is guarded by `if n < 1`;
// i64→u8 cast for rounding mode is guarded by `if !(0..=5).contains(&n)`.
#![allow(clippy::cast_sign_loss)]

use crate::builtins::BuiltinId;
use crate::rng::RngLike;

use super::error::VmError;
use super::stack_ops::StackOps;
use super::value::{
    get_bigfloat_precision, get_bigfloat_rounding_mode, set_bigfloat_precision,
    set_bigfloat_rounding_mode, BigFloatRoundingMode, RustBigFloat, RustBigInt, Value,
};
use super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute numeric type constructor builtin functions.
    /// Returns `Ok(Some(()))` if handled, `Ok(None)` if not a numeric builtin.
    pub(super) fn execute_builtin_numeric(
        &mut self,
        builtin: &BuiltinId,
        argc: usize,
    ) -> Result<Option<()>, VmError> {
        if matches!(
            builtin,
            BuiltinId::Bool
                | BuiltinId::BigInt
                | BuiltinId::BigFloat
                | BuiltinId::Int8
                | BuiltinId::Int16
                | BuiltinId::Int32
                | BuiltinId::Int64
                | BuiltinId::Int128
                | BuiltinId::UInt8
                | BuiltinId::UInt16
                | BuiltinId::UInt32
                | BuiltinId::UInt64
                | BuiltinId::UInt128
                | BuiltinId::Float16
                | BuiltinId::Float32
                | BuiltinId::Float64
        ) && argc != 1
        {
            return Err(VmError::MethodError(format!(
                "numeric type constructor requires exactly 1 argument, got {}",
                argc
            )));
        }

        match builtin {
            // =========================================================================
            // BigInt Operations
            // =========================================================================
            BuiltinId::BigInt => {
                // BigInt(x) - convert to arbitrary precision integer
                let val = self.stack.pop_value()?;
                let bigint = match val {
                    Value::Bool(n) => RustBigInt::from(if n { 1u8 } else { 0u8 }),
                    Value::I8(n) => RustBigInt::from(n),
                    Value::I16(n) => RustBigInt::from(n),
                    Value::I32(n) => RustBigInt::from(n),
                    Value::I64(n) => RustBigInt::from(n),
                    Value::I128(n) => RustBigInt::from(n),
                    Value::U8(n) => RustBigInt::from(n),
                    Value::U16(n) => RustBigInt::from(n),
                    Value::U32(n) => RustBigInt::from(n),
                    Value::U64(n) => RustBigInt::from(n),
                    Value::U128(n) => RustBigInt::from(n),
                    Value::F64(n) => RustBigInt::from(n as i64),
                    Value::Str(s) => s.parse::<RustBigInt>().map_err(|_| {
                        VmError::TypeError(format!("Cannot parse '{}' as BigInt", s))
                    })?,
                    Value::BigInt(n) => n,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "Cannot convert {:?} to BigInt",
                            other
                        )));
                    }
                };
                self.stack.push(Value::BigInt(bigint));
            }

            // =========================================================================
            // BigFloat Operations
            // =========================================================================
            BuiltinId::BigFloat => {
                // BigFloat(x) - convert to arbitrary precision float
                let val = self.stack.pop_value()?;
                let precision = get_bigfloat_precision();
                let parse_bigfloat_decimal = |s: &str| -> Result<RustBigFloat, VmError> {
                    let mut consts = astro_float::Consts::new().map_err(|e| {
                        VmError::InternalError(format!(
                            "Failed to initialize BigFloat constants: {}",
                            e
                        ))
                    })?;
                    let bf = RustBigFloat::parse(
                        s,
                        astro_float::Radix::Dec,
                        precision,
                        BigFloatRoundingMode::ToEven,
                        &mut consts,
                    );
                    if bf.is_nan() && !s.to_lowercase().contains("nan") {
                        return Err(VmError::TypeError(format!(
                            "Cannot parse '{}' as BigFloat",
                            s
                        )));
                    }
                    Ok(bf)
                };
                let bigfloat = match val {
                    Value::I64(n) => RustBigFloat::from_f64(n as f64, precision),
                    Value::F64(n) => RustBigFloat::from_f64(n, precision),
                    Value::Str(s) => parse_bigfloat_decimal(&s)?,
                    Value::BigFloat(n) => n,
                    Value::BigInt(n) => {
                        // Convert BigInt to BigFloat via string
                        let s = n.to_string();
                        parse_bigfloat_decimal(&s)?
                    }
                    Value::Struct(s) => match s.irrational_decimal() {
                        Some(decimal) => parse_bigfloat_decimal(decimal)?,
                        None => {
                            return Err(VmError::TypeError(format!(
                                "Cannot convert {:?} to BigFloat",
                                Value::Struct(s)
                            )));
                        }
                    },
                    Value::StructRef(idx) => {
                        let decimal = self
                            .struct_heap
                            .get(idx)
                            .and_then(|s| s.irrational_decimal())
                            .ok_or_else(|| {
                                VmError::TypeError(format!(
                                    "Cannot convert StructRef({}) to BigFloat",
                                    idx
                                ))
                            })?;
                        parse_bigfloat_decimal(decimal)?
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "Cannot convert {:?} to BigFloat",
                            other
                        )));
                    }
                };
                self.stack.push(Value::BigFloat(bigfloat));
            }

            BuiltinId::BigFloatPrecision => {
                // _bigfloat_precision(x) - get the precision of a BigFloat value
                let val = self.stack.pop_value()?;
                match val {
                    Value::BigFloat(bf) => {
                        // astro_float::BigFloat::precision() returns Option<usize>
                        // For Inf/NaN it returns None, so we try mantissa_max_bit_len
                        // If both are None (shouldn't happen), use the default precision
                        let prec = bf
                            .precision()
                            .or_else(|| bf.mantissa_max_bit_len())
                            .unwrap_or(get_bigfloat_precision());
                        self.stack.push(Value::I64(prec as i64));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_bigfloat_precision requires BigFloat, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            BuiltinId::BigFloatDefaultPrecision => {
                // _bigfloat_default_precision() - get the default precision for new BigFloats
                let prec = get_bigfloat_precision();
                self.stack.push(Value::I64(prec as i64));
            }

            BuiltinId::SetBigFloatDefaultPrecision => {
                // _set_bigfloat_default_precision!(n) - set the default precision
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(n) => {
                        if n < 1 {
                            return Err(VmError::DomainError(
                                "precision must be at least 1".to_string(),
                            ));
                        }
                        let old_prec = set_bigfloat_precision(n as usize);
                        self.stack.push(Value::I64(old_prec as i64));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_set_bigfloat_default_precision! requires Int64, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            BuiltinId::BigFloatRounding => {
                // _bigfloat_rounding() - get the current rounding mode
                // Returns: 0=ToEven (RoundNearest), 1=ToZero, 2=Up, 3=Down, 4=FromZero, 5=ToOdd
                let mode = get_bigfloat_rounding_mode();
                self.stack.push(Value::I64(mode as i64));
            }

            BuiltinId::SetBigFloatRounding => {
                // _set_bigfloat_rounding!(mode) - set the rounding mode
                // mode: 0=ToEven (RoundNearest), 1=ToZero, 2=Up, 3=Down, 4=FromZero, 5=ToOdd
                let val = self.stack.pop_value()?;
                match val {
                    Value::I64(n) => {
                        if !(0..=5).contains(&n) {
                            return Err(VmError::DomainError(format!(
                                "invalid rounding mode: {}, must be 0-5",
                                n
                            )));
                        }
                        let old_mode = set_bigfloat_rounding_mode(n as u8);
                        self.stack.push(Value::I64(old_mode as i64));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "_set_bigfloat_rounding! requires Int64, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            // =========================================================================
            // Subnormal (Denormal) Float Control
            // =========================================================================
            BuiltinId::GetZeroSubnormals => {
                // get_zero_subnormals() - check if subnormals are flushed to zero
                // In SubsetJuliaVM, we always follow IEEE standard (subnormals preserved)
                // This returns false since we don't support flushing subnormals to zero
                self.stack.push(Value::Bool(false));
            }

            BuiltinId::SetZeroSubnormals => {
                // set_zero_subnormals(yes::Bool) - enable/disable flushing subnormals to zero
                // Returns true if successful, false if hardware doesn't support it
                // In SubsetJuliaVM, we cannot change the subnormal handling mode,
                // so we return false when yes=true (can't enable), true when yes=false (already disabled)
                let val = self.stack.pop_value()?;
                match val {
                    Value::Bool(yes) => {
                        // If yes=true, we can't enable flush-to-zero, so return false
                        // If yes=false, subnormals are already preserved, so return true
                        self.stack.push(Value::Bool(!yes));
                    }
                    other => {
                        return Err(VmError::TypeError(format!(
                            "set_zero_subnormals requires Bool, got {:?}",
                            other.value_type()
                        )));
                    }
                }
            }

            // =========================================================================
            // Numeric Type Constructors
            // =========================================================================
            // Bool(x) — only 0/1 are valid; routes through the range-checked
            // shared `convert(Bool, x)` so out-of-range values raise InexactError
            // (Issue #7971, building on #7970).
            BuiltinId::Bool => {
                let val = self.stack.pop_value()?;
                let result = crate::vm::convert::convert_value("Bool", &val)?;
                self.stack.push(result);
            }
            BuiltinId::Int8 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i8(&val)?;
                self.stack.push(Value::I8(result));
            }
            BuiltinId::Int16 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i16(&val)?;
                self.stack.push(Value::I16(result));
            }
            BuiltinId::Int32 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i32(&val)?;
                self.stack.push(Value::I32(result));
            }
            BuiltinId::Int64 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i64(&val)?;
                self.stack.push(Value::I64(result));
            }
            BuiltinId::Int128 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_i128(&val)?;
                self.stack.push(Value::I128(result));
            }
            BuiltinId::UInt8 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u8(&val)?;
                self.stack.push(Value::U8(result));
            }
            BuiltinId::UInt16 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u16(&val)?;
                self.stack.push(Value::U16(result));
            }
            BuiltinId::UInt32 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u32(&val)?;
                self.stack.push(Value::U32(result));
            }
            BuiltinId::UInt64 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u64(&val)?;
                self.stack.push(Value::U64(result));
            }
            BuiltinId::UInt128 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_u128(&val)?;
                self.stack.push(Value::U128(result));
            }
            BuiltinId::Float16 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_f16(&val)?;
                self.stack.push(Value::F16(result));
            }
            BuiltinId::Float32 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_f32(&val)?;
                self.stack.push(Value::F32(result));
            }
            BuiltinId::Float64 => {
                let val = self.stack.pop_value()?;
                let result = self.convert_to_f64(&val)?;
                self.stack.push(Value::F64(result));
            }

            _ => return Ok(None),
        }
        Ok(Some(()))
    }
}
