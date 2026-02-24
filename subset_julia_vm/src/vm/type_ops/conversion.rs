//! Numeric type conversion helpers.
//!
// All `as` casts in this module are intentional numeric coercions (Julia
// explicit type constructor calls, e.g. `UInt8(x)`).  Sign loss / truncation
// is detected by the InexactError pattern: `(x as TargetType) as f64 != x`.
#![allow(clippy::cast_sign_loss)]

use half::f16;
use num_traits::ToPrimitive;

use crate::rng::RngLike;
use crate::vm::error::VmError;
use crate::vm::value::Value;
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    // =========================================================================
    // Numeric Type Conversion Helpers
    // =========================================================================

    pub(in crate::vm) fn convert_to_i8(&self, val: &Value) -> Result<i8, VmError> {
        match val {
            Value::I8(n) => Ok(*n),
            Value::I16(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::I32(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::I64(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::I128(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U8(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U16(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U32(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U64(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::U128(n) => {
                i8::try_from(*n).map_err(|_| VmError::InexactError(format!("Int8({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as i8) as f32;
                if truncated == *n {
                    Ok(*n as i8)
                } else {
                    Err(VmError::InexactError(format!("Int8({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as i8) as f64;
                if truncated == *n {
                    Ok(*n as i8)
                } else {
                    Err(VmError::InexactError(format!("Int8({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Int8",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_i16(&self, val: &Value) -> Result<i16, VmError> {
        match val {
            Value::I8(n) => Ok(*n as i16),
            Value::I16(n) => Ok(*n),
            Value::I32(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::I64(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::I128(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U8(n) => Ok(*n as i16),
            Value::U16(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U32(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U64(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::U128(n) => {
                i16::try_from(*n).map_err(|_| VmError::InexactError(format!("Int16({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as i16) as f32;
                if truncated == *n {
                    Ok(*n as i16)
                } else {
                    Err(VmError::InexactError(format!("Int16({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as i16) as f64;
                if truncated == *n {
                    Ok(*n as i16)
                } else {
                    Err(VmError::InexactError(format!("Int16({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Int16",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_i32(&self, val: &Value) -> Result<i32, VmError> {
        match val {
            Value::I8(n) => Ok(*n as i32),
            Value::I16(n) => Ok(*n as i32),
            Value::I32(n) => Ok(*n),
            Value::I64(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::I128(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::U8(n) => Ok(*n as i32),
            Value::U16(n) => Ok(*n as i32),
            Value::U32(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::U64(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::U128(n) => {
                i32::try_from(*n).map_err(|_| VmError::InexactError(format!("Int32({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as i32) as f32;
                if truncated == *n {
                    Ok(*n as i32)
                } else {
                    Err(VmError::InexactError(format!("Int32({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as i32) as f64;
                if truncated == *n {
                    Ok(*n as i32)
                } else {
                    Err(VmError::InexactError(format!("Int32({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Int32",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_i64(&self, val: &Value) -> Result<i64, VmError> {
        match val {
            Value::Bool(b) => Ok(if *b { 1 } else { 0 }),
            Value::I8(n) => Ok(*n as i64),
            Value::I16(n) => Ok(*n as i64),
            Value::I32(n) => Ok(*n as i64),
            Value::I64(n) => Ok(*n),
            Value::I128(n) => {
                i64::try_from(*n).map_err(|_| VmError::InexactError(format!("Int64({})", n)))
            }
            Value::U8(n) => Ok(*n as i64),
            Value::U16(n) => Ok(*n as i64),
            Value::U32(n) => Ok(*n as i64),
            Value::U64(n) => {
                i64::try_from(*n).map_err(|_| VmError::InexactError(format!("Int64({})", n)))
            }
            Value::U128(n) => {
                i64::try_from(*n).map_err(|_| VmError::InexactError(format!("Int64({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as i64) as f32;
                if truncated == *n {
                    Ok(*n as i64)
                } else {
                    Err(VmError::InexactError(format!("Int64({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as i64) as f64;
                if truncated == *n {
                    Ok(*n as i64)
                } else {
                    Err(VmError::InexactError(format!("Int64({})", n)))
                }
            }
            Value::Char(c) => Ok(*c as i64),
            Value::BigInt(n) => n.to_i64().ok_or_else(|| {
                VmError::TypeError(format!("BigInt value {} too large to convert to Int64", n))
            }),
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Int64",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_i128(&self, val: &Value) -> Result<i128, VmError> {
        match val {
            Value::I8(n) => Ok(*n as i128),
            Value::I16(n) => Ok(*n as i128),
            Value::I32(n) => Ok(*n as i128),
            Value::I64(n) => Ok(*n as i128),
            Value::I128(n) => Ok(*n),
            Value::U8(n) => Ok(*n as i128),
            Value::U16(n) => Ok(*n as i128),
            Value::U32(n) => Ok(*n as i128),
            Value::U64(n) => Ok(*n as i128),
            Value::U128(n) => {
                i128::try_from(*n).map_err(|_| VmError::InexactError(format!("Int128({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as i128) as f32;
                if truncated == *n {
                    Ok(*n as i128)
                } else {
                    Err(VmError::InexactError(format!("Int128({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as i128) as f64;
                if truncated == *n {
                    Ok(*n as i128)
                } else {
                    Err(VmError::InexactError(format!("Int128({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Int128",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_u8(&self, val: &Value) -> Result<u8, VmError> {
        match val {
            Value::I8(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I16(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I32(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I64(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::I128(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U8(n) => Ok(*n),
            Value::U16(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U32(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U64(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::U128(n) => {
                u8::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt8({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as u8) as f32;
                if truncated == *n {
                    Ok(*n as u8)
                } else {
                    Err(VmError::InexactError(format!("UInt8({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as u8) as f64;
                if truncated == *n {
                    Ok(*n as u8)
                } else {
                    Err(VmError::InexactError(format!("UInt8({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to UInt8",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_u16(&self, val: &Value) -> Result<u16, VmError> {
        match val {
            Value::I8(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I16(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I32(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I64(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::I128(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::U8(n) => Ok(*n as u16),
            Value::U16(n) => Ok(*n),
            Value::U32(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::U64(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::U128(n) => {
                u16::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt16({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as u16) as f32;
                if truncated == *n {
                    Ok(*n as u16)
                } else {
                    Err(VmError::InexactError(format!("UInt16({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as u16) as f64;
                if truncated == *n {
                    Ok(*n as u16)
                } else {
                    Err(VmError::InexactError(format!("UInt16({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to UInt16",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_u32(&self, val: &Value) -> Result<u32, VmError> {
        match val {
            Value::I8(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I16(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I32(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I64(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::I128(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::U8(n) => Ok(*n as u32),
            Value::U16(n) => Ok(*n as u32),
            Value::U32(n) => Ok(*n),
            Value::U64(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::U128(n) => {
                u32::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt32({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as u32) as f32;
                if truncated == *n {
                    Ok(*n as u32)
                } else {
                    Err(VmError::InexactError(format!("UInt32({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as u32) as f64;
                if truncated == *n {
                    Ok(*n as u32)
                } else {
                    Err(VmError::InexactError(format!("UInt32({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to UInt32",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_u64(&self, val: &Value) -> Result<u64, VmError> {
        match val {
            Value::I8(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I16(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I32(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I64(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::I128(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::U8(n) => Ok(*n as u64),
            Value::U16(n) => Ok(*n as u64),
            Value::U32(n) => Ok(*n as u64),
            Value::U64(n) => Ok(*n),
            Value::U128(n) => {
                u64::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt64({})", n)))
            }
            Value::F32(n) => {
                let truncated = (*n as u64) as f32;
                if truncated == *n {
                    Ok(*n as u64)
                } else {
                    Err(VmError::InexactError(format!("UInt64({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as u64) as f64;
                if truncated == *n {
                    Ok(*n as u64)
                } else {
                    Err(VmError::InexactError(format!("UInt64({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to UInt64",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_u128(&self, val: &Value) -> Result<u128, VmError> {
        match val {
            Value::I8(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I16(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I32(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I64(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::I128(n) => {
                u128::try_from(*n).map_err(|_| VmError::InexactError(format!("UInt128({})", n)))
            }
            Value::U8(n) => Ok(*n as u128),
            Value::U16(n) => Ok(*n as u128),
            Value::U32(n) => Ok(*n as u128),
            Value::U64(n) => Ok(*n as u128),
            Value::U128(n) => Ok(*n),
            Value::F32(n) => {
                let truncated = (*n as u128) as f32;
                if truncated == *n {
                    Ok(*n as u128)
                } else {
                    Err(VmError::InexactError(format!("UInt128({})", n)))
                }
            }
            Value::F64(n) => {
                let truncated = (*n as u128) as f64;
                if truncated == *n {
                    Ok(*n as u128)
                } else {
                    Err(VmError::InexactError(format!("UInt128({})", n)))
                }
            }
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to UInt128",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_f16(&self, val: &Value) -> Result<f16, VmError> {
        match val {
            Value::I8(n) => Ok(f16::from_f32(*n as f32)),
            Value::I16(n) => Ok(f16::from_f32(*n as f32)),
            Value::I32(n) => Ok(f16::from_f32(*n as f32)),
            Value::I64(n) => Ok(f16::from_f32(*n as f32)),
            Value::I128(n) => Ok(f16::from_f64(*n as f64)),
            Value::U8(n) => Ok(f16::from_f32(*n as f32)),
            Value::U16(n) => Ok(f16::from_f32(*n as f32)),
            Value::U32(n) => Ok(f16::from_f32(*n as f32)),
            Value::U64(n) => Ok(f16::from_f64(*n as f64)),
            Value::U128(n) => Ok(f16::from_f64(*n as f64)),
            Value::F16(n) => Ok(*n),
            Value::F32(n) => Ok(f16::from_f32(*n)),
            Value::F64(n) => Ok(f16::from_f64(*n)),
            Value::Bool(b) => Ok(f16::from_f32(if *b { 1.0 } else { 0.0 })),
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Float16",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_f32(&self, val: &Value) -> Result<f32, VmError> {
        match val {
            Value::I8(n) => Ok(*n as f32),
            Value::I16(n) => Ok(*n as f32),
            Value::I32(n) => Ok(*n as f32),
            Value::I64(n) => Ok(*n as f32),
            Value::I128(n) => Ok(*n as f32),
            Value::U8(n) => Ok(*n as f32),
            Value::U16(n) => Ok(*n as f32),
            Value::U32(n) => Ok(*n as f32),
            Value::U64(n) => Ok(*n as f32),
            Value::U128(n) => Ok(*n as f32),
            Value::F16(n) => Ok(n.to_f32()),
            Value::F32(n) => Ok(*n),
            Value::F64(n) => Ok(*n as f32),
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Float32",
                val
            ))),
        }
    }

    pub(in crate::vm) fn convert_to_f64(&self, val: &Value) -> Result<f64, VmError> {
        match val {
            Value::I8(n) => Ok(*n as f64),
            Value::I16(n) => Ok(*n as f64),
            Value::I32(n) => Ok(*n as f64),
            Value::I64(n) => Ok(*n as f64),
            Value::I128(n) => Ok(*n as f64),
            Value::U8(n) => Ok(*n as f64),
            Value::U16(n) => Ok(*n as f64),
            Value::U32(n) => Ok(*n as f64),
            Value::U64(n) => Ok(*n as f64),
            Value::U128(n) => Ok(*n as f64),
            Value::F16(n) => Ok(n.to_f64()),
            Value::F32(n) => Ok(*n as f64),
            Value::F64(n) => Ok(*n),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            _ => Err(VmError::TypeError(format!(
                "Cannot convert {:?} to Float64",
                val
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::rng::StableRng;
    use crate::vm::error::VmError;
    use crate::vm::value::Value;
    use crate::vm::Vm;

    fn make_vm() -> Vm<StableRng> {
        Vm::new(vec![], StableRng::new(0))
    }

    // =========================================================================
    // UInt8 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u8_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u8(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::I8(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::I16(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u8_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u8(&Value::I64(256)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::U16(256)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u8(&Value::I64(1000)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u8_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u8(&Value::I64(0)), Ok(0));
        assert_eq!(vm.convert_to_u8(&Value::I64(255)), Ok(255));
        assert_eq!(vm.convert_to_u8(&Value::U8(42)), Ok(42));
        assert_eq!(vm.convert_to_u8(&Value::I64(128)), Ok(128));
    }

    // =========================================================================
    // UInt16 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u16_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u16(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u16(&Value::I8(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u16_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u16(&Value::I64(65536)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u16(&Value::U32(65536)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u16_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u16(&Value::I64(0)), Ok(0));
        assert_eq!(vm.convert_to_u16(&Value::I64(65535)), Ok(65535));
        assert_eq!(vm.convert_to_u16(&Value::U8(200)), Ok(200));
    }

    // =========================================================================
    // UInt32 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u32_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u32(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u32(&Value::I32(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u32_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u32(&Value::I64(4_294_967_296)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u32(&Value::U64(4_294_967_296)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u32_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u32(&Value::I64(0)), Ok(0));
        assert_eq!(vm.convert_to_u32(&Value::I64(4_294_967_295)), Ok(4_294_967_295));
        assert_eq!(vm.convert_to_u32(&Value::U16(1000)), Ok(1000));
    }

    // =========================================================================
    // UInt64 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u64_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u64(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u64(&Value::I8(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u64_rejects_overflow() {
        let vm = make_vm();
        // U128 values larger than u64::MAX overflow
        assert!(matches!(
            vm.convert_to_u64(&Value::U128(u64::MAX as u128 + 1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u64_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u64(&Value::I64(0)), Ok(0));
        assert_eq!(vm.convert_to_u64(&Value::I64(i64::MAX)), Ok(i64::MAX as u64));
        assert_eq!(vm.convert_to_u64(&Value::U8(255)), Ok(255));
    }

    // =========================================================================
    // UInt128 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_u128_rejects_negative() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_u128(&Value::I64(-1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_u128(&Value::I128(-1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_u128_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_u128(&Value::I64(0)), Ok(0));
        assert_eq!(vm.convert_to_u128(&Value::U64(u64::MAX)), Ok(u64::MAX as u128));
        assert_eq!(vm.convert_to_u128(&Value::U128(u128::MAX)), Ok(u128::MAX));
    }

    // =========================================================================
    // Int8 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i8_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_i8(&Value::I64(128)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i8(&Value::I64(-129)),
            Err(VmError::InexactError(_))
        ));
        // U8 values > 127 don't fit in i8
        assert!(matches!(
            vm.convert_to_i8(&Value::U8(128)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i8(&Value::U8(255)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i8_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i8(&Value::I64(127)), Ok(127));
        assert_eq!(vm.convert_to_i8(&Value::I64(-128)), Ok(-128));
        assert_eq!(vm.convert_to_i8(&Value::I8(0)), Ok(0));
        assert_eq!(vm.convert_to_i8(&Value::U8(0)), Ok(0));
        assert_eq!(vm.convert_to_i8(&Value::U8(127)), Ok(127));
    }

    // =========================================================================
    // Int16 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i16_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_i16(&Value::I64(32768)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i16(&Value::I64(-32769)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i16(&Value::U16(32768)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i16_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i16(&Value::I64(32767)), Ok(32767));
        assert_eq!(vm.convert_to_i16(&Value::I64(-32768)), Ok(-32768));
        assert_eq!(vm.convert_to_i16(&Value::I8(-128)), Ok(-128));
        assert_eq!(vm.convert_to_i16(&Value::U8(255)), Ok(255));
    }

    // =========================================================================
    // Int32 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i32_rejects_overflow() {
        let vm = make_vm();
        assert!(matches!(
            vm.convert_to_i32(&Value::I64(2_147_483_648)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i32(&Value::I64(-2_147_483_649)),
            Err(VmError::InexactError(_))
        ));
        // U32 values > i32::MAX overflow
        assert!(matches!(
            vm.convert_to_i32(&Value::U32(2_147_483_648)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i32_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i32(&Value::I64(2_147_483_647)), Ok(2_147_483_647));
        assert_eq!(vm.convert_to_i32(&Value::I64(-2_147_483_648)), Ok(-2_147_483_648));
        assert_eq!(vm.convert_to_i32(&Value::U16(65535)), Ok(65535));
    }

    // =========================================================================
    // Int64 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i64_rejects_overflow() {
        let vm = make_vm();
        // U64 values > i64::MAX don't fit
        assert!(matches!(
            vm.convert_to_i64(&Value::U64(u64::MAX)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i64(&Value::U64(i64::MAX as u64 + 1)),
            Err(VmError::InexactError(_))
        ));
        // I128 values outside i64 range
        assert!(matches!(
            vm.convert_to_i64(&Value::I128(i64::MAX as i128 + 1)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i64(&Value::I128(i64::MIN as i128 - 1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i64_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i64(&Value::I64(i64::MAX)), Ok(i64::MAX));
        assert_eq!(vm.convert_to_i64(&Value::I64(i64::MIN)), Ok(i64::MIN));
        assert_eq!(vm.convert_to_i64(&Value::U64(0)), Ok(0));
        assert_eq!(vm.convert_to_i64(&Value::U64(i64::MAX as u64)), Ok(i64::MAX));
        assert_eq!(vm.convert_to_i64(&Value::U8(255)), Ok(255));
    }

    // =========================================================================
    // Int128 range checks
    // =========================================================================

    #[test]
    fn test_convert_to_i128_rejects_overflow() {
        let vm = make_vm();
        // U128 values > i128::MAX don't fit
        assert!(matches!(
            vm.convert_to_i128(&Value::U128(u128::MAX)),
            Err(VmError::InexactError(_))
        ));
        assert!(matches!(
            vm.convert_to_i128(&Value::U128(i128::MAX as u128 + 1)),
            Err(VmError::InexactError(_))
        ));
    }

    #[test]
    fn test_convert_to_i128_accepts_valid() {
        let vm = make_vm();
        assert_eq!(vm.convert_to_i128(&Value::I64(i64::MIN)), Ok(i64::MIN as i128));
        assert_eq!(vm.convert_to_i128(&Value::U64(u64::MAX)), Ok(u64::MAX as i128));
        assert_eq!(vm.convert_to_i128(&Value::U128(i128::MAX as u128)), Ok(i128::MAX));
        assert_eq!(vm.convert_to_i128(&Value::I128(i128::MIN)), Ok(i128::MIN));
    }
}
