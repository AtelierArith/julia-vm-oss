//! Handler for `CallDynamicBinaryBoth` instruction.
//!
//! Extracted from `call_dynamic_binary.rs` to reduce function length (Issue #2935).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// SAFETY: i64→u32 casts for Char arithmetic codepoints; values are used with
// char::from_u32 which safely handles invalid codepoints via unwrap_or.
#![allow(clippy::cast_sign_loss)]

use super::super::*;
use super::call_dynamic::CallDynamicResult;
#[cfg(debug_assertions)]
use super::call_dynamic::dispatch_debug_log;
use super::call_dynamic_binary::try_string_char_concat;
use super::util::{
    bind_value_to_slot, extract_base_type, is_rust_dict_parametric_mismatch, score_type_match,
};
use crate::rng::RngLike;

#[cfg(debug_assertions)]
use super::call_dynamic::dispatch_debug_enabled;

impl<R: RngLike> Vm<R> {
    /// Handle `CallDynamicBinaryBoth` dispatch.
    ///
    /// Runtime dispatch for binary operators when both operands are `Any`.
    /// Falls back to intrinsic operations for primitive numeric types.
    pub(super) fn execute_binary_both(
        &mut self,
        fallback_intrinsic: &Intrinsic,
        candidates: &[(usize, String, String)],
    ) -> Result<CallDynamicResult, VmError> {
        // Pop both arguments
        let right = self.stack.pop_value()?;
        let left = self.stack.pop_value()?;
        
        // Get type names for both operands
        let left_type_name = self.get_type_name(&left);
        let right_type_name = self.get_type_name(&right);
        
        #[cfg(debug_assertions)]
        if dispatch_debug_enabled() {
            dispatch_debug_log(format_args!(
                "[DISPATCH] BinaryBoth: ({}, {}) intrinsic={:?}, candidates={}",
                left_type_name,
                right_type_name,
                fallback_intrinsic,
                candidates.len()
            ));
        }
        
        // Issue #2437: Skip user-defined method dispatch when both operands
        // are builtin numeric types. This mirrors the `both_same_primitive`
        // guard in compile_binary_op (binary.rs) but at runtime, since
        // compile_binary_op_call doesn't know operand types at compile time.
        // Without this guard, +(::Number, ::Number) from promotion.jl would
        // shadow the builtin + for primitive operations (including mixed-type
        // cases like Float64 < Int64). The intrinsic fallback already handles
        // cross-type primitive promotion correctly.
        // Uses shared is_builtin_numeric_value() from vm/util.rs (Issue #2439).
        let both_primitive = super::super::util::is_builtin_numeric_value(&left)
            && super::super::util::is_builtin_numeric_value(&right);
        
        // Issue #2492: Also skip dispatch when BigInt/BigFloat intrinsics can
        // handle the operation. BigInt/BigFloat arithmetic (+-*÷ and comparisons)
        // is handled by intrinsics at the fallback path below. Without this guard,
        // +(::Number, ::Number) would match via subtype dispatch and cause
        // infinite recursion: +(BigInt,BigInt) -> promote -> +(BigInt,BigInt).
        // Issue #2498: Also covers BigFloat for same reason.
        let bigint_intrinsic_handles = matches!(
            (&left, &right),
            (
                Value::BigInt(_),
                Value::BigInt(_) | Value::I64(_) | Value::I128(_)
            ) | (Value::I64(_) | Value::I128(_), Value::BigInt(_))
                | (
                    Value::BigFloat(_),
                    Value::BigFloat(_) | Value::F64(_) | Value::I64(_)
                )
                | (Value::F64(_) | Value::I64(_), Value::BigFloat(_))
        );
        
        // Find the best matching method using scored dispatch (Issue #2517).
        // Uses shared score_type_match() for consistent scoring across all handlers.
        // Skip dispatch when both operands are builtin primitive numeric types
        // to preserve builtin/intrinsic precedence (Issue #2437).
        let left_base = extract_base_type(&left_type_name);
        let right_base = extract_base_type(&right_type_name);
        
        let matched = if both_primitive || bigint_intrinsic_handles {
            None
        } else {
            let mut best: Option<(&(usize, String, String), u32)> = None;
            for candidate in candidates {
                let (_, left_expected, right_expected) = candidate;
        
                // Value::Dict (Rust-backed) must not match parametric Dict{K,V}
                // Pure Julia methods that expect StructRef (Issue #2748).
                if is_rust_dict_parametric_mismatch(&left, left_expected)
                    || is_rust_dict_parametric_mismatch(&right, right_expected)
                {
                    continue;
                }
        
                let mut left_score =
                    score_type_match(left_expected, &left_type_name, left_base);
                if left_score == 0 && self.check_subtype(&left_type_name, left_expected) {
                    left_score = 1;
                }
        
                let mut right_score =
                    score_type_match(right_expected, &right_type_name, right_base);
                if right_score == 0 && self.check_subtype(&right_type_name, right_expected)
                {
                    right_score = 1;
                }
        
                if left_score > 0 && right_score > 0 {
                    let total_score = left_score + right_score;
                    if best.is_none_or(|b| total_score > b.1) {
                        best = Some((candidate, total_score));
                    }
                }
            }
            best.map(|(c, _)| c)
        };
        
        if let Some((func_index, ref left_exp, ref right_exp)) = matched {
            #[cfg(debug_assertions)]
            if dispatch_debug_enabled() {
                dispatch_debug_log(format_args!(
                    "[DISPATCH]   -> matched method #{}: ({}, {})",
                    func_index, left_exp, right_exp
                ));
            }
            // Call the user-defined method
            let func = match self.get_function_cloned_or_raise(*func_index)? {
                Some(f) => f,
                None => return Ok(CallDynamicResult::Continue),
            };
        
            let mut frame = Frame::new_with_slots(func.local_slot_count, Some(*func_index));

            // Bind type parameters from where clauses (Issue #2468).
            // Only clone args when type params exist (common case: no type params).
            if !func.type_params.is_empty() {
                let args = [left.clone(), right.clone()];
                self.bind_type_params(&func, &args, &mut frame);
            }

            // Bind arguments directly to frame slots (Issue #3373: avoid double clone)
            if let Some(&slot) = func.param_slots.first() {
                bind_value_to_slot(&mut frame, slot, left, &mut self.struct_heap);
            }
            if let Some(&slot) = func.param_slots.get(1) {
                bind_value_to_slot(&mut frame, slot, right, &mut self.struct_heap);
            }
        
            for kwparam in &func.kwparams {
                if kwparam.required {
                    return Err(VmError::UndefKeywordError(kwparam.name.clone()));
                }
                bind_value_to_slot(
                    &mut frame,
                    kwparam.slot,
                    kwparam.default.clone(),
                    &mut self.struct_heap,
                );
            }
        
            self.return_ips.push(self.ip);
            self.frames.push(frame);
            self.ip = func.entry;
        } else {
            // No matching method - try fallback to intrinsic if both are primitives
            // Bool is also considered primitive (promotes to Int64 for arithmetic)
            // Float32 is also primitive and participates in type promotion
            #[cfg(debug_assertions)]
            if dispatch_debug_enabled() {
                dispatch_debug_log(format_args!(
                    "[DISPATCH]   -> no method match, trying primitive fallback"
                ));
            }
            let left_is_primitive = matches!(
                &left,
                Value::I64(_)
                    | Value::I128(_)
                    | Value::F64(_)
                    | Value::F32(_)
                    | Value::F16(_)
                    | Value::Bool(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
            );
            let right_is_primitive = matches!(
                &right,
                Value::I64(_)
                    | Value::I128(_)
                    | Value::F64(_)
                    | Value::F32(_)
                    | Value::F16(_)
                    | Value::Bool(_)
                    | Value::U8(_)
                    | Value::U16(_)
                    | Value::U32(_)
                    | Value::U64(_)
                    | Value::U128(_)
                    | Value::I8(_)
                    | Value::I16(_)
                    | Value::I32(_)
            );
            let left_is_struct = matches!(&left, Value::Struct(_) | Value::StructRef(_));
            let right_is_struct = matches!(&right, Value::Struct(_) | Value::StructRef(_));
        
            // Normalize Memory → Array for binary dispatch (Issue #2764)
            let left = match left {
                Value::Memory(mem) => {
                    Value::Array(super::super::util::memory_to_array_ref(&mem))
                }
                other => other,
            };
            let right = match right {
                Value::Memory(mem) => {
                    Value::Array(super::super::util::memory_to_array_ref(&mem))
                }
                other => other,
            };
        
            // Convert Bool and small integer values to I64 for arithmetic operations
            // This enables mixed-type dispatch for UInt8/Int64 etc. (Issue #1853)
            // Convert Bool and small integer values to I64 for arithmetic operations
            let left = match left {
                Value::Bool(b) => Value::I64(if b { 1 } else { 0 }),
                Value::U8(v) => Value::I64(i64::from(v)),
                Value::U16(v) => Value::I64(i64::from(v)),
                Value::U32(v) => Value::I64(i64::from(v)),
                Value::U64(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt64 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::U128(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt128 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::I8(v) => Value::I64(v as i64),
                Value::I16(v) => Value::I64(v as i64),
                Value::I32(v) => Value::I64(v as i64),
                // Pass through I64, I128, F64, F32, F16, Complex, Rational, etc.
                // Guarded by left_is_primitive && right_is_primitive below
                other => other,
            };
            let right = match right {
                Value::Bool(b) => Value::I64(if b { 1 } else { 0 }),
                Value::U8(v) => Value::I64(i64::from(v)),
                Value::U16(v) => Value::I64(i64::from(v)),
                Value::U32(v) => Value::I64(i64::from(v)),
                Value::U64(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt64 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::U128(v) => {
                    let i = i64::try_from(v).map_err(|_| {
                        VmError::OverflowError(format!(
                            "cannot convert UInt128 value {} to Int64 without overflow",
                            v
                        ))
                    })?;
                    Value::I64(i)
                }
                Value::I8(v) => Value::I64(v as i64),
                Value::I16(v) => Value::I64(v as i64),
                Value::I32(v) => Value::I64(v as i64),
                // Pass through I64, I128, F64, F32, F16, Complex, Rational, etc.
                // Guarded by left_is_primitive && right_is_primitive below
                other => other,
            };
        
            if left_is_primitive && right_is_primitive {
                // Use the fallback intrinsic for primitive-only operations
                // Select Int version if both operands are I64, EXCEPT for:
                // - DivFloat: Julia's / always returns Float64, even for integers
                // - PowFloat: Power should use floating point for proper semantics
                let both_int = matches!((&left, &right), (Value::I64(_), Value::I64(_)));
                let has_i128 =
                    matches!(&left, Value::I128(_)) || matches!(&right, Value::I128(_));
                let both_f32 = matches!((&left, &right), (Value::F32(_), Value::F32(_)));
                let has_f32 =
                    matches!(&left, Value::F32(_)) || matches!(&right, Value::F32(_));
                let has_f64 =
                    matches!(&left, Value::F64(_)) || matches!(&right, Value::F64(_));
                let both_f16 = matches!((&left, &right), (Value::F16(_), Value::F16(_)));
                let has_f16 =
                    matches!(&left, Value::F16(_)) || matches!(&right, Value::F16(_));
        
                // Handle Int128 operations (Issue #1904)
                // I128 must be checked before float paths since I128+I64 should stay integer
                if has_i128 && !has_f64 && !has_f32 && !has_f16 {
                    // Both operands are integer (I128, I64, or Bool->I64)
                    // Promote both to i128
                    let a = match &left {
                        Value::I128(v) => *v,
                        Value::I64(v) => *v as i128,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "I128 path: unexpected left operand {:?}",
                                left
                            )))
                        }
                    };
                    let b = match &right {
                        Value::I128(v) => *v,
                        Value::I64(v) => *v as i128,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "I128 path: unexpected right operand {:?}",
                                right
                            )))
                        }
                    };
                    let result = match fallback_intrinsic {
                        Intrinsic::AddFloat => Value::I128(a.wrapping_add(b)),
                        Intrinsic::SubFloat => Value::I128(a.wrapping_sub(b)),
                        Intrinsic::MulFloat => Value::I128(a.wrapping_mul(b)),
                        Intrinsic::DivFloat => {
                            // Julia's / always returns Float64, even for integers
                            Value::F64(a as f64 / b as f64)
                        }
                        Intrinsic::PowFloat => Value::F64((a as f64).powf(b as f64)),
                        Intrinsic::SdivInt => {
                            // Integer division (÷)
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(CallDynamicResult::Continue);
                            }
                            Value::I128(a / b)
                        }
                        Intrinsic::SremInt => {
                            // Integer remainder (mod/rem)
                            if b == 0 {
                                self.raise(VmError::DivisionByZero)?;
                                return Ok(CallDynamicResult::Continue);
                            }
                            // Julia's mod: result = a - floor(a/b) * b
                            Value::I128(((a % b) + b) % b)
                        }
                        Intrinsic::EqFloat | Intrinsic::EqInt => Value::Bool(a == b),
                        Intrinsic::NeFloat | Intrinsic::NeInt => Value::Bool(a != b),
                        Intrinsic::LtFloat | Intrinsic::SltInt => Value::Bool(a < b),
                        Intrinsic::LeFloat | Intrinsic::SleInt => Value::Bool(a <= b),
                        Intrinsic::GtFloat | Intrinsic::SgtInt => Value::Bool(a > b),
                        Intrinsic::GeFloat | Intrinsic::SgeInt => Value::Bool(a >= b),
                        _ => {
                            self.raise(VmError::unsupported_op(
                                "Int128", &fallback_intrinsic,
                            ))?;
                            return Ok(CallDynamicResult::Continue);
                        }
                    };
                    self.stack.push(result);
                } else if has_f16 && !both_f16 {
                    // Float16 mixed with other type
                    if has_f64 {
                        // F16 <-> F64: promote both to F64
                        let a = match &left {
                            Value::F16(v) => v.to_f64(),
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Float64 path: unexpected left operand {:?}",
                                    left
                                )))
                            }
                        };
                        let b = match &right {
                            Value::F16(v) => v.to_f64(),
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Float64 path: unexpected right operand {:?}",
                                    right
                                )))
                            }
                        };
                        let result = match fallback_intrinsic {
                            Intrinsic::AddFloat => Value::F64(a + b),
                            Intrinsic::SubFloat => Value::F64(a - b),
                            Intrinsic::MulFloat => Value::F64(a * b),
                            Intrinsic::DivFloat => Value::F64(a / b),
                            Intrinsic::EqFloat => Value::Bool(a == b),
                            Intrinsic::NeFloat => Value::Bool(a != b),
                            Intrinsic::LtFloat => Value::Bool(a < b),
                            Intrinsic::LeFloat => Value::Bool(a <= b),
                            Intrinsic::GtFloat => Value::Bool(a > b),
                            Intrinsic::GeFloat => Value::Bool(a >= b),
                            Intrinsic::SremInt => {
                                let result = a - (a / b).floor() * b;
                                Value::F64(result)
                            }
                            Intrinsic::SdivInt => {
                                let result = (a / b).floor();
                                Value::F64(result)
                            }
                            _ => {
                                self.raise(VmError::unsupported_op(
                                    "Float16-Float64", &fallback_intrinsic,
                                ))?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        };
                        self.stack.push(result);
                    } else if has_f32 {
                        // F16 <-> F32: promote both to F32
                        let a = match &left {
                            Value::F16(v) => v.to_f32(),
                            Value::F32(v) => *v,
                            Value::I64(v) => *v as f32,
                            Value::I128(v) => *v as f32,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Float32 path: unexpected left operand {:?}",
                                    left
                                )))
                            }
                        };
                        let b = match &right {
                            Value::F16(v) => v.to_f32(),
                            Value::F32(v) => *v,
                            Value::I64(v) => *v as f32,
                            Value::I128(v) => *v as f32,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Float32 path: unexpected right operand {:?}",
                                    right
                                )))
                            }
                        };
                        let result = match fallback_intrinsic {
                            Intrinsic::AddFloat => Value::F32(a + b),
                            Intrinsic::SubFloat => Value::F32(a - b),
                            Intrinsic::MulFloat => Value::F32(a * b),
                            Intrinsic::DivFloat => Value::F32(a / b),
                            Intrinsic::EqFloat => Value::Bool(a == b),
                            Intrinsic::NeFloat => Value::Bool(a != b),
                            Intrinsic::LtFloat => Value::Bool(a < b),
                            Intrinsic::LeFloat => Value::Bool(a <= b),
                            Intrinsic::GtFloat => Value::Bool(a > b),
                            Intrinsic::GeFloat => Value::Bool(a >= b),
                            Intrinsic::SremInt => {
                                let result = a - (a / b).floor() * b;
                                Value::F32(result)
                            }
                            Intrinsic::SdivInt => {
                                let result = (a / b).floor();
                                Value::F32(result)
                            }
                            _ => {
                                self.raise(VmError::unsupported_op(
                                    "Float16-Float32", &fallback_intrinsic,
                                ))?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        };
                        self.stack.push(result);
                    } else {
                        // F16 <-> Int: result is F16 (Julia semantics: Float16 + Int -> Float16)
                        let a = match &left {
                            Value::F16(v) => v.to_f64(),
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Int path: unexpected left operand {:?}",
                                    left
                                )))
                            }
                        };
                        let b = match &right {
                            Value::F16(v) => v.to_f64(),
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float16-Int path: unexpected right operand {:?}",
                                    right
                                )))
                            }
                        };
                        let result = match fallback_intrinsic {
                            Intrinsic::AddFloat => Value::F16(half::f16::from_f64(a + b)),
                            Intrinsic::SubFloat => Value::F16(half::f16::from_f64(a - b)),
                            Intrinsic::MulFloat => Value::F16(half::f16::from_f64(a * b)),
                            Intrinsic::DivFloat => Value::F16(half::f16::from_f64(a / b)),
                            Intrinsic::EqFloat => Value::Bool(a == b),
                            Intrinsic::NeFloat => Value::Bool(a != b),
                            Intrinsic::LtFloat => Value::Bool(a < b),
                            Intrinsic::LeFloat => Value::Bool(a <= b),
                            Intrinsic::GtFloat => Value::Bool(a > b),
                            Intrinsic::GeFloat => Value::Bool(a >= b),
                            Intrinsic::SremInt => {
                                let result = a - (a / b).floor() * b;
                                Value::F16(half::f16::from_f64(result))
                            }
                            Intrinsic::SdivInt => {
                                let result = (a / b).floor();
                                Value::F16(half::f16::from_f64(result))
                            }
                            _ => {
                                self.raise(VmError::unsupported_op(
                                    "Float16-Int64", &fallback_intrinsic,
                                ))?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        };
                        self.stack.push(result);
                    }
                } else if has_f32 && !both_f32 {
                    // Handle Float32 mixed-type operations
                    // F32 + I64, F32 - I64, F32 * I64, F32 / I64 -> F32 result
                    // F32 + F64, F32 - F64, F32 * F64, F32 / F64 -> F64 result (promotion)
                    if has_f64 {
                        // F32 <-> F64: promote both to F64
                        #[cfg(debug_assertions)]
                        if dispatch_debug_enabled() {
                            dispatch_debug_log(format_args!(
                                "[DISPATCH]   -> F32-F64 mixed path: {:?}",
                                fallback_intrinsic
                            ));
                        }
                        let a = match &left {
                            Value::F32(v) => *v as f64,
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float32-Float64 path: unexpected left operand {:?}",
                                    left
                                )))
                            }
                        };
                        let b = match &right {
                            Value::F32(v) => *v as f64,
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            Value::I128(v) => *v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float32-Float64 path: unexpected right operand {:?}",
                                    right
                                )))
                            }
                        };
                        let result = match fallback_intrinsic {
                            Intrinsic::AddFloat => Value::F64(a + b),
                            Intrinsic::SubFloat => Value::F64(a - b),
                            Intrinsic::MulFloat => Value::F64(a * b),
                            Intrinsic::DivFloat => Value::F64(a / b),
                            Intrinsic::EqFloat => Value::Bool(a == b),
                            Intrinsic::NeFloat => Value::Bool(a != b),
                            Intrinsic::LtFloat => Value::Bool(a < b),
                            Intrinsic::LeFloat => Value::Bool(a <= b),
                            Intrinsic::GtFloat => Value::Bool(a > b),
                            Intrinsic::GeFloat => Value::Bool(a >= b),
                            // SremInt (mod/rem): use fmod semantics for F32-F64 mixed type
                            Intrinsic::SremInt => {
                                // Julia's mod: result = a - floor(a/b) * b
                                let result = a - (a / b).floor() * b;
                                Value::F64(result)
                            }
                            // SdivInt (div): floor division for F32-F64 mixed type (Issue #1849)
                            Intrinsic::SdivInt => {
                                let result = (a / b).floor();
                                Value::F64(result)
                            }
                            _ => {
                                self.raise(VmError::unsupported_op(
                                    "Float32-Float64", &fallback_intrinsic,
                                ))?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        };
                        self.stack.push(result);
                    } else {
                        // F32 <-> Int: keep result as F32 (Julia semantics for Float32 + Int)
                        #[cfg(debug_assertions)]
                        if dispatch_debug_enabled() {
                            dispatch_debug_log(format_args!(
                                "[DISPATCH]   -> F32-Int mixed path: {:?}",
                                fallback_intrinsic
                            ));
                        }
                        let a = match &left {
                            Value::F32(v) => *v,
                            Value::I64(v) => *v as f32,
                            Value::I128(v) => *v as f32,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float32-Int path: unexpected left operand {:?}",
                                    left
                                )))
                            }
                        };
                        let b = match &right {
                            Value::F32(v) => *v,
                            Value::I64(v) => *v as f32,
                            Value::I128(v) => *v as f32,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "Float32-Int path: unexpected right operand {:?}",
                                    right
                                )))
                            }
                        };
                        let result = match fallback_intrinsic {
                            Intrinsic::AddFloat => Value::F32(a + b),
                            Intrinsic::SubFloat => Value::F32(a - b),
                            Intrinsic::MulFloat => Value::F32(a * b),
                            Intrinsic::DivFloat => Value::F32(a / b),
                            Intrinsic::EqFloat => Value::Bool(a == b),
                            Intrinsic::NeFloat => Value::Bool(a != b),
                            Intrinsic::LtFloat => Value::Bool(a < b),
                            Intrinsic::LeFloat => Value::Bool(a <= b),
                            Intrinsic::GtFloat => Value::Bool(a > b),
                            Intrinsic::GeFloat => Value::Bool(a >= b),
                            // SremInt (mod/rem): use fmod semantics for F32-I64 mixed type
                            // Issue #1776: circshift uses mod(k, n) which triggers this
                            Intrinsic::SremInt => {
                                // Julia's mod: result = a - floor(a/b) * b
                                let result = a - (a / b).floor() * b;
                                Value::F32(result)
                            }
                            // SdivInt (div): floor division for F32-I64 mixed type (Issue #1849)
                            Intrinsic::SdivInt => {
                                let result = (a / b).floor();
                                Value::F32(result)
                            }
                            _ => {
                                self.raise(VmError::unsupported_op(
                                    "Float32-Int64", &fallback_intrinsic,
                                ))?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        };
                        self.stack.push(result);
                    }
                } else if *fallback_intrinsic == Intrinsic::SremInt && !both_int {
                    // Special case: SremInt (mod) with Float64/Float32/Float16 - use fmod semantics
                    // At least one operand is floating point, use floating point mod
                    let a = match &left {
                        Value::F64(v) => *v,
                        Value::F32(v) => *v as f64,
                        Value::F16(v) => v.to_f64(),
                        Value::I64(v) => *v as f64,
                        Value::I128(v) => *v as f64,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "SremInt float path: unexpected left operand {:?}",
                                left
                            )))
                        }
                    };
                    let b = match &right {
                        Value::F64(v) => *v,
                        Value::F32(v) => *v as f64,
                        Value::F16(v) => v.to_f64(),
                        Value::I64(v) => *v as f64,
                        Value::I128(v) => *v as f64,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "SremInt float path: unexpected right operand {:?}",
                                right
                            )))
                        }
                    };
                    // Julia's mod is always positive: mod(a, b) = a - floor(a/b) * b
                    let result = a - (a / b).floor() * b;
                    // Preserve F32 type when both operands are F32 (Issue #1762)
                    if both_f32 {
                        self.stack.push(Value::F32(result as f32));
                    } else if both_f16 {
                        self.stack.push(Value::F16(half::f16::from_f64(result)));
                    } else {
                        self.stack.push(Value::F64(result));
                    }
                } else {
                    let actual_intrinsic = if both_int {
                        match fallback_intrinsic {
                            Intrinsic::AddFloat => Intrinsic::AddInt,
                            Intrinsic::SubFloat => Intrinsic::SubInt,
                            Intrinsic::MulFloat => Intrinsic::MulInt,
                            // DivFloat stays as DivFloat - Julia's / always returns Float64
                            Intrinsic::DivFloat => Intrinsic::DivFloat,
                            // PowFloat stays as PowFloat for proper floating point semantics
                            Intrinsic::PowFloat => Intrinsic::PowFloat,
                            Intrinsic::LtFloat => Intrinsic::SltInt,
                            Intrinsic::LeFloat => Intrinsic::SleInt,
                            Intrinsic::GtFloat => Intrinsic::SgtInt,
                            Intrinsic::GeFloat => Intrinsic::SgeInt,
                            Intrinsic::EqFloat => Intrinsic::EqInt,
                            Intrinsic::NeFloat => Intrinsic::NeInt,
                            other => *other,
                        }
                    } else {
                        *fallback_intrinsic
                    };
                    // Convert operands to appropriate types
                    let (left_val, right_val) = if matches!(
                        actual_intrinsic,
                        Intrinsic::DivFloat | Intrinsic::PowFloat
                    ) && both_int
                    {
                        // For DivFloat and PowFloat with integers, convert both to F64
                        let l = match left {
                            Value::I64(v) => Value::F64(v as f64),
                            Value::I128(v) => Value::F64(v as f64),
                            Value::F32(v) => Value::F64(v as f64),
                            Value::F16(v) => Value::F64(v.to_f64()),
                            // F64 passes through unchanged; Complex/Rational handled elsewhere
                            other => other,
                        };
                        let r = match right {
                            Value::I64(v) => Value::F64(v as f64),
                            Value::I128(v) => Value::F64(v as f64),
                            Value::F32(v) => Value::F64(v as f64),
                            Value::F16(v) => Value::F64(v.to_f64()),
                            // F64 passes through unchanged; Complex/Rational handled elsewhere
                            other => other,
                        };
                        (l, r)
                    } else if matches!(
                        actual_intrinsic,
                        Intrinsic::EqFloat
                            | Intrinsic::NeFloat
                            | Intrinsic::LtFloat
                            | Intrinsic::LeFloat
                            | Intrinsic::GtFloat
                            | Intrinsic::GeFloat
                            | Intrinsic::AddFloat
                            | Intrinsic::SubFloat
                            | Intrinsic::MulFloat
                    ) && !both_int
                    {
                        // For Float comparisons/ops with mixed types, convert I64/I128/F32/F16 to F64
                        let l = match left {
                            Value::I64(v) => Value::F64(v as f64),
                            Value::I128(v) => Value::F64(v as f64),
                            Value::F32(v) => Value::F64(v as f64),
                            Value::F16(v) => Value::F64(v.to_f64()),
                            // F64 passes through unchanged; Complex/Rational handled elsewhere
                            other => other,
                        };
                        let r = match right {
                            Value::I64(v) => Value::F64(v as f64),
                            Value::I128(v) => Value::F64(v as f64),
                            Value::F32(v) => Value::F64(v as f64),
                            Value::F16(v) => Value::F64(v.to_f64()),
                            // F64 passes through unchanged; Complex/Rational handled elsewhere
                            other => other,
                        };
                        (l, r)
                    } else {
                        (left, right)
                    };
                    self.stack.push(left_val);
                    self.stack.push(right_val);
                    // Use raise() instead of ? to integrate with try-catch
                    if let Err(e) = self.execute_intrinsic(actual_intrinsic) {
                        self.raise(e)?;
                        return Ok(CallDynamicResult::Continue);
                    }
                }
            } else if matches!(fallback_intrinsic, Intrinsic::MulFloat)
                && matches!(
                    (&left, &right),
                    (Value::Str(_), Value::Str(_))
                        | (Value::Str(_), Value::Char(_))
                        | (Value::Char(_), Value::Str(_))
                        | (Value::Char(_), Value::Char(_))
                )
            {
                // String/Char concatenation: "a" * "b", "a" * 'b', 'a' * "b", 'a' * 'b' (Issue #2127)
                if let Some(result) = try_string_char_concat(&left, &right) {
                    self.stack.push(result);
                } else {
                    return Err(VmError::InternalError(
                        "string/char concat match but helper returned None".to_string(),
                    ));
                }
            } else if matches!((&left, &right), (Value::Str(_), Value::Str(_)))
                && matches!(
                    fallback_intrinsic,
                    Intrinsic::LtFloat
                        | Intrinsic::LeFloat
                        | Intrinsic::GtFloat
                        | Intrinsic::GeFloat
                        | Intrinsic::EqFloat
                        | Intrinsic::NeFloat
                )
            {
                // String comparison: lexicographic ordering (Issue #2025)
                let result = match (&left, &right) {
                    (Value::Str(a), Value::Str(b)) => match fallback_intrinsic {
                        Intrinsic::LtFloat => a < b,
                        Intrinsic::LeFloat => a <= b,
                        Intrinsic::GtFloat => a > b,
                        Intrinsic::GeFloat => a >= b,
                        Intrinsic::EqFloat => a == b,
                        Intrinsic::NeFloat => a != b,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "string comparison path: unexpected intrinsic {:?}",
                                fallback_intrinsic
                            )))
                        }
                    },
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "string comparison path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                self.stack.push(Value::Bool(result));
            } else if (left_is_struct && right_is_struct)
                && matches!(fallback_intrinsic, Intrinsic::EqFloat | Intrinsic::NeFloat)
            {
                // Struct-struct comparison: use field-by-field comparison
                let is_equal = self.compare_struct_fields(&left, &right);
                let result = if matches!(fallback_intrinsic, Intrinsic::EqFloat) {
                    is_equal
                } else {
                    !is_equal
                };
                self.stack.push(Value::Bool(result));
            } else if matches!((&left, &right), (Value::Symbol(_), Value::Symbol(_)))
                && matches!(fallback_intrinsic, Intrinsic::EqFloat | Intrinsic::NeFloat)
            {
                // Symbol-Symbol comparison: compare by name (interned)
                let is_equal = match (&left, &right) {
                    (Value::Symbol(a), Value::Symbol(b)) => a == b,
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "symbol comparison path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = if matches!(fallback_intrinsic, Intrinsic::EqFloat) {
                    is_equal
                } else {
                    !is_equal
                };
                self.stack.push(Value::Bool(result));
            } else if matches!((&left, &right), (Value::Bool(_), Value::Bool(_)))
                && matches!(fallback_intrinsic, Intrinsic::EqFloat | Intrinsic::NeFloat)
            {
                // Bool-Bool comparison
                let is_equal = match (&left, &right) {
                    (Value::Bool(a), Value::Bool(b)) => a == b,
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "bool comparison path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = if matches!(fallback_intrinsic, Intrinsic::EqFloat) {
                    is_equal
                } else {
                    !is_equal
                };
                self.stack.push(Value::Bool(result));
            } else if matches!((&left, &right), (Value::Char(_), Value::Char(_))) {
                // Char-Char operations (Issue #2122)
                let (a, b) = match (&left, &right) {
                    (Value::Char(a), Value::Char(b)) => (*a as i64, *b as i64),
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "char-char path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = match fallback_intrinsic {
                    // Char - Char → Int (difference of codepoints)
                    Intrinsic::SubFloat | Intrinsic::SubInt => Value::I64(a - b),
                    // Comparisons (both float and int intrinsic forms)
                    Intrinsic::EqFloat | Intrinsic::EqInt => Value::Bool(a == b),
                    Intrinsic::NeFloat | Intrinsic::NeInt => Value::Bool(a != b),
                    Intrinsic::LtFloat | Intrinsic::SltInt => Value::Bool(a < b),
                    Intrinsic::LeFloat | Intrinsic::SleInt => Value::Bool(a <= b),
                    Intrinsic::GtFloat | Intrinsic::SgtInt => Value::Bool(a > b),
                    Intrinsic::GeFloat | Intrinsic::SgeInt => Value::Bool(a >= b),
                    _ => {
                        return Err(VmError::unsupported_op(
                            "Char-Char", &fallback_intrinsic,
                        ));
                    }
                };
                self.stack.push(result);
            } else if (matches!(&left, Value::Char(_)) && matches!(&right, Value::I64(_)))
                || (matches!(&left, Value::I64(_)) && matches!(&right, Value::Char(_)))
            {
                // Issue #2122: Char+Int / Int+Char -> Char, Char-Int -> Char
                let (char_val, int_val) = match (&left, &right) {
                    (Value::Char(c), Value::I64(n)) => (*c as i64, *n),
                    (Value::I64(n), Value::Char(c)) => (*c as i64, *n),
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "char-int path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let left_is_char = matches!(&left, Value::Char(_));
                let result = match fallback_intrinsic {
                    Intrinsic::AddFloat | Intrinsic::AddInt => {
                        // Char + Int or Int + Char -> Char
                        let cp = char_val + int_val;
                        Value::Char(char::from_u32(cp as u32).unwrap_or('\0'))
                    }
                    Intrinsic::SubFloat | Intrinsic::SubInt if left_is_char => {
                        // Char - Int -> Char
                        let cp = char_val - int_val;
                        Value::Char(char::from_u32(cp as u32).unwrap_or('\0'))
                    }
                    Intrinsic::SubFloat | Intrinsic::SubInt => {
                        // Int - Char -> Int (unusual but handle it)
                        Value::I64(int_val - char_val)
                    }
                    Intrinsic::EqFloat | Intrinsic::EqInt => {
                        Value::Bool(char_val == int_val)
                    }
                    Intrinsic::NeFloat | Intrinsic::NeInt => {
                        Value::Bool(char_val != int_val)
                    }
                    Intrinsic::LtFloat | Intrinsic::SltInt => {
                        Value::Bool(if left_is_char {
                            char_val < int_val
                        } else {
                            int_val < char_val
                        })
                    }
                    Intrinsic::LeFloat | Intrinsic::SleInt => {
                        Value::Bool(if left_is_char {
                            char_val <= int_val
                        } else {
                            int_val <= char_val
                        })
                    }
                    Intrinsic::GtFloat | Intrinsic::SgtInt => {
                        Value::Bool(if left_is_char {
                            char_val > int_val
                        } else {
                            int_val > char_val
                        })
                    }
                    Intrinsic::GeFloat | Intrinsic::SgeInt => {
                        Value::Bool(if left_is_char {
                            char_val >= int_val
                        } else {
                            int_val >= char_val
                        })
                    }
                    _ => {
                        // INTERNAL: Char-Int comparison table covers all valid intrinsics; unsupported op is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "unsupported Char-Int operation: {:?}",
                            fallback_intrinsic
                        )));
                    }
                };
                self.stack.push(result);
            } else if matches!((&left, &right), (Value::F16(_), Value::F16(_))) {
                // Float16-Float16 operations: promote to F64 and use float intrinsics
                let (a, b) = match (&left, &right) {
                    (Value::F16(a), Value::F16(b)) => (a.to_f64(), b.to_f64()),
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "Float16 path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = match fallback_intrinsic {
                    Intrinsic::AddFloat => Value::F16(half::f16::from_f64(a + b)),
                    Intrinsic::SubFloat => Value::F16(half::f16::from_f64(a - b)),
                    Intrinsic::MulFloat => Value::F16(half::f16::from_f64(a * b)),
                    Intrinsic::DivFloat => Value::F16(half::f16::from_f64(a / b)),
                    Intrinsic::EqFloat => Value::Bool(a == b),
                    Intrinsic::NeFloat => Value::Bool(a != b),
                    Intrinsic::LtFloat => Value::Bool(a < b),
                    Intrinsic::LeFloat => Value::Bool(a <= b),
                    Intrinsic::GtFloat => Value::Bool(a > b),
                    Intrinsic::GeFloat => Value::Bool(a >= b),
                    Intrinsic::SremInt => {
                        let result = a - (a / b).floor() * b;
                        Value::F16(half::f16::from_f64(result))
                    }
                    Intrinsic::SdivInt => {
                        let result = (a / b).floor();
                        Value::F16(half::f16::from_f64(result))
                    }
                    _ => {
                        self.raise(VmError::unsupported_op(
                            "Float16", &fallback_intrinsic,
                        ))?;
                        return Ok(CallDynamicResult::Continue);
                    }
                };
                self.stack.push(result);
            } else if matches!((&left, &right), (Value::F32(_), Value::F32(_))) {
                // Float32-Float32 operations: perform in f32 and preserve type
                // This ensures Float32 + Float32 returns Float32, not Float64
                #[cfg(debug_assertions)]
                if dispatch_debug_enabled() {
                    dispatch_debug_log(format_args!(
                        "[DISPATCH]   -> F32-F32 same-type path: {:?}",
                        fallback_intrinsic
                    ));
                }
                let (a, b) = match (&left, &right) {
                    (Value::F32(a), Value::F32(b)) => (*a, *b),
                    _ => {
                        return Err(VmError::InternalError(format!(
                            "Float32 path: unexpected operands ({:?}, {:?})",
                            left, right
                        )))
                    }
                };
                let result = match fallback_intrinsic {
                    Intrinsic::AddFloat => Value::F32(a + b),
                    Intrinsic::SubFloat => Value::F32(a - b),
                    Intrinsic::MulFloat => Value::F32(a * b),
                    Intrinsic::DivFloat => Value::F32(a / b),
                    Intrinsic::EqFloat => Value::Bool(a == b),
                    Intrinsic::NeFloat => Value::Bool(a != b),
                    Intrinsic::LtFloat => Value::Bool(a < b),
                    Intrinsic::LeFloat => Value::Bool(a <= b),
                    Intrinsic::GtFloat => Value::Bool(a > b),
                    Intrinsic::GeFloat => Value::Bool(a >= b),
                    _ => {
                        self.raise(VmError::unsupported_op(
                            "Float32", &fallback_intrinsic,
                        ))?;
                        return Ok(CallDynamicResult::Continue);
                    }
                };
                self.stack.push(result);
            } else if matches!(&left, Value::BigInt(_) | Value::I64(_) | Value::I128(_))
                && matches!(&right, Value::BigInt(_) | Value::I64(_) | Value::I128(_))
                && (matches!(&left, Value::BigInt(_)) || matches!(&right, Value::BigInt(_)))
            {
                // BigInt operations: at least one operand is BigInt, other can be I64/I128
                // Use BigInt intrinsics which auto-promote I64/I128 to BigInt
                let bigint_intrinsic = match fallback_intrinsic {
                    Intrinsic::AddFloat => Intrinsic::AddBigInt,
                    Intrinsic::SubFloat => Intrinsic::SubBigInt,
                    Intrinsic::MulFloat => Intrinsic::MulBigInt,
                    Intrinsic::DivFloat | Intrinsic::SdivInt => Intrinsic::DivBigInt, // ÷ and / both use DivBigInt for BigInt
                    Intrinsic::LtFloat => Intrinsic::LtBigInt,
                    Intrinsic::LeFloat => Intrinsic::LeBigInt,
                    Intrinsic::GtFloat => Intrinsic::GtBigInt,
                    Intrinsic::GeFloat => Intrinsic::GeBigInt,
                    Intrinsic::EqFloat => Intrinsic::EqBigInt,
                    Intrinsic::NeFloat => Intrinsic::NeBigInt,
                    Intrinsic::SremInt => Intrinsic::RemBigInt,
                    other => *other, // Keep other intrinsics as-is
                };
                self.stack.push(left);
                self.stack.push(right);
                if let Err(e) = self.execute_intrinsic(bigint_intrinsic) {
                    self.raise(e)?;
                    return Ok(CallDynamicResult::Continue);
                }
            } else if matches!(&left, Value::BigFloat(_) | Value::F64(_) | Value::I64(_))
                && matches!(&right, Value::BigFloat(_) | Value::F64(_) | Value::I64(_))
                && (matches!(&left, Value::BigFloat(_))
                    || matches!(&right, Value::BigFloat(_)))
            {
                // BigFloat operations: at least one operand is BigFloat, other can be F64/I64
                // Use BigFloat intrinsics which auto-promote F64/I64 to BigFloat (Issue #2498)
                let bigfloat_intrinsic = match fallback_intrinsic {
                    Intrinsic::AddFloat => Intrinsic::AddBigFloat,
                    Intrinsic::SubFloat => Intrinsic::SubBigFloat,
                    Intrinsic::MulFloat => Intrinsic::MulBigFloat,
                    Intrinsic::DivFloat | Intrinsic::SdivInt => Intrinsic::DivBigFloat,
                    Intrinsic::LtFloat => Intrinsic::LtBigFloat,
                    Intrinsic::LeFloat => Intrinsic::LeBigFloat,
                    Intrinsic::GtFloat => Intrinsic::GtBigFloat,
                    Intrinsic::GeFloat => Intrinsic::GeBigFloat,
                    Intrinsic::EqFloat => Intrinsic::EqBigFloat,
                    Intrinsic::NeFloat => Intrinsic::NeBigFloat,
                    other => *other, // Keep other intrinsics as-is
                };
                self.stack.push(left);
                self.stack.push(right);
                if let Err(e) = self.execute_intrinsic(bigfloat_intrinsic) {
                    self.raise(e)?;
                    return Ok(CallDynamicResult::Continue);
                }
            } else if (left_is_primitive && right_is_struct)
                || (left_is_struct && right_is_primitive)
                || (left_is_struct && right_is_struct)
            {
                // Struct operations: handled by candidates-based dispatch above.
                // Complex arithmetic goes through Julia dispatch (Issue #2422 Phase 4).
                self.raise(VmError::no_method_matching_op(
                    &left_type_name, &right_type_name,
                ))?;
                return Ok(CallDynamicResult::Continue);
            } else if *fallback_intrinsic == Intrinsic::SremInt {
                // Modulo with Float64 - use fmod-style operation
                let a = match &left {
                    Value::F64(v) => *v,
                    Value::I64(v) => *v as f64,
                    _ => {
                        self.raise(VmError::TypeError(format!(
                            "mod expects numeric, got {} and {}",
                            left_type_name, right_type_name
                        )))?;
                        return Ok(CallDynamicResult::Continue);
                    }
                };
                let b = match &right {
                    Value::F64(v) => *v,
                    Value::I64(v) => *v as f64,
                    _ => {
                        self.raise(VmError::TypeError(format!(
                            "mod expects numeric, got {} and {}",
                            left_type_name, right_type_name
                        )))?;
                        return Ok(CallDynamicResult::Continue);
                    }
                };
                // Julia's mod is always positive: mod(a, b) = a - floor(a/b) * b
                let result = a - (a / b).floor() * b;
                self.stack.push(Value::F64(result));
            } else {
                // Mixed struct + primitive or other non-primitive types
                // Check for String/Char operations before raising MethodError (Issue #2127)
                let is_str_or_char =
                    |v: &Value| matches!(v, Value::Str(_) | Value::Char(_));
                if is_str_or_char(&left) && is_str_or_char(&right) {
                    // String/Char * concatenation
                    if matches!(fallback_intrinsic, Intrinsic::MulFloat) {
                        if let Some(result) = try_string_char_concat(&left, &right) {
                            self.stack.push(result);
                        } else {
                            return Err(VmError::InternalError(
                                "string/char concat match but helper returned None"
                                    .to_string(),
                            ));
                        }
                    } else if let (Value::Str(l), Value::Str(r)) = (&left, &right) {
                        // String comparison operations (only for Str-Str)
                        if matches!(fallback_intrinsic, Intrinsic::EqFloat) {
                            self.stack.push(Value::Bool(l == r));
                        } else if matches!(fallback_intrinsic, Intrinsic::NeFloat) {
                            self.stack.push(Value::Bool(l != r));
                        } else if matches!(fallback_intrinsic, Intrinsic::LtFloat) {
                            self.stack.push(Value::Bool(l < r));
                        } else if matches!(fallback_intrinsic, Intrinsic::LeFloat) {
                            self.stack.push(Value::Bool(l <= r));
                        } else if matches!(fallback_intrinsic, Intrinsic::GtFloat) {
                            self.stack.push(Value::Bool(l > r));
                        } else if matches!(fallback_intrinsic, Intrinsic::GeFloat) {
                            self.stack.push(Value::Bool(l >= r));
                        } else {
                            self.raise(VmError::MethodError(format!(
                                "no method matching operator(String, String) for {:?}",
                                fallback_intrinsic
                            )))?;
                            return Ok(CallDynamicResult::Continue);
                        }
                    } else {
                        // Char-involved comparison: not supported
                        self.raise(VmError::MethodError(format!(
                            "no method matching operator({}, {}) for {:?}",
                            left_type_name, right_type_name, fallback_intrinsic
                        )))?;
                        return Ok(CallDynamicResult::Continue);
                    }
                } else if let (Value::DataType(l), Value::DataType(r)) = (&left, &right) {
                    // DataType equality comparison (e.g., Int64 == Float64)
                    let result = match fallback_intrinsic {
                        Intrinsic::EqFloat => l == r,
                        Intrinsic::NeFloat => l != r,
                        _ => {
                            self.raise(VmError::MethodError(format!(
                                "comparison op {:?} not supported for DataType",
                                fallback_intrinsic
                            )))?;
                            return Ok(CallDynamicResult::Continue);
                        }
                    };
                    self.stack.push(Value::Bool(result));
                } else if matches!(
                    (&left, &right),
                    (Value::Struct(_) | Value::StructRef(_), Value::Array(_))
                        | (Value::Array(_), Value::Struct(_) | Value::StructRef(_))
                ) && matches!(fallback_intrinsic, Intrinsic::MulFloat)
                {
                    // Complex * Vector or Vector * Complex: scalar-vector multiplication
                    // Check which operand is the array and which is the scalar
                    let (scalar_val, arr_val) = match (&left, &right) {
                        (s @ (Value::Struct(_) | Value::StructRef(_)), Value::Array(a)) => {
                            (s.clone(), a.clone())
                        }
                        (Value::Array(a), s @ (Value::Struct(_) | Value::StructRef(_))) => {
                            (s.clone(), a.clone())
                        }
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "complex scalar-array path: unexpected operands ({:?}, {:?})",
                                left, right
                            )))
                        }
                    };
        
                    // Check if scalar is Complex
                    let scalar_struct = match &scalar_val {
                        Value::Struct(s) => Some(s.clone()),
                        Value::StructRef(idx) => self.struct_heap.get(*idx).cloned(),
                        _ => None,
                    };
        
                    if let Some(s) = &scalar_struct {
                        if self.is_complex(s) {
                            // Get complex scalar components
                            let (c_re, c_im) = s.as_complex_parts().unwrap_or((0.0, 0.0));
        
                            // Use matmul helper for scalar-vector multiplication
                            use crate::vm::matmul::{scalar_vector_mul_complex, Complex64};
                            let scalar = Complex64::new(c_re, c_im);
                            let arr = arr_val.borrow();
                            let mul_result =
                                scalar_vector_mul_complex(scalar, &arr, &self.struct_heap);
                            drop(arr);
        
                            match mul_result {
                                Ok(mut result) => {
                                    // Store correct Complex type_id (Issue #1804)
                                    if result
                                        .element_type_override
                                        .as_ref()
                                        .is_some_and(|e| e.is_complex())
                                    {
                                        result.struct_type_id =
                                            Some(self.get_complex_type_id());
                                    }
                                    self.stack.push(Value::Array(new_array_ref(result)));
                                }
                                Err(e) => {
                                    self.raise(e)?;
                                    return Ok(CallDynamicResult::Continue);
                                }
                            }
                        } else {
                            // Non-Complex struct * Vector - not supported
                            self.raise(VmError::no_method_matching_op(
                                &left_type_name, &right_type_name,
                            ))?;
                            return Ok(CallDynamicResult::Continue);
                        }
                    } else {
                        self.raise(VmError::no_method_matching_op(
                            &left_type_name, &right_type_name,
                        ))?;
                        return Ok(CallDynamicResult::Continue);
                    }
                } else if matches!(
                    (&left, &right),
                    (Value::I64(_) | Value::F64(_), Value::Array(_))
                        | (Value::Array(_), Value::I64(_) | Value::F64(_))
                ) && matches!(fallback_intrinsic, Intrinsic::MulFloat)
                {
                    // ============================================================
                    // Scalar-Array Multiplication Dispatch (Issue #1799)
                    // ============================================================
                    // This handles both `scalar * array` and `array * scalar` for
                    // all numeric types. The dispatch tree:
                    //
                    // 1. Real scalar × Complex array → scalar_vector_mul_complex
                    // 2. Real scalar × Real array    → scalar_vector_mul_real
                    //
                    // IMPORTANT: Always handle BOTH complex AND real arrays.
                    // Never make the else branch raise MethodError - that creates
                    // asymmetric dispatch where Complex works but Real doesn't.
                    //
                    // For a unified dispatcher, see: vm/matmul::scalar_vector_mul()
                    // ============================================================
                    use crate::vm::matmul::{
                        is_complex_array, scalar_vector_mul_complex, Complex64,
                    };
        
                    let (scalar_val, arr_ref) = match (&left, &right) {
                        (s @ (Value::I64(_) | Value::F64(_)), Value::Array(a)) => {
                            (s.clone(), a.clone())
                        }
                        (Value::Array(a), s @ (Value::I64(_) | Value::F64(_))) => {
                            (s.clone(), a.clone())
                        }
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "real scalar-array path: unexpected operands ({:?}, {:?})",
                                left, right
                            )))
                        }
                    };
        
                    let arr = arr_ref.borrow();
                    if is_complex_array(&arr) {
                        // Real scalar * Complex array: convert scalar to Complex(re, 0.0)
                        let scalar_f64 = match scalar_val {
                            Value::F64(v) => v,
                            Value::I64(v) => v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "real scalar-array path: unexpected scalar {:?}",
                                    scalar_val
                                )))
                            }
                        };
                        let scalar = Complex64::from_real(scalar_f64);
                        let mul_result =
                            scalar_vector_mul_complex(scalar, &arr, &self.struct_heap);
                        drop(arr);
        
                        match mul_result {
                            Ok(mut result) => {
                                // Store correct Complex type_id (Issue #1804)
                                if result
                                    .element_type_override
                                    .as_ref()
                                    .is_some_and(|e| e.is_complex())
                                {
                                    result.struct_type_id =
                                        Some(self.get_complex_type_id());
                                }
                                self.stack.push(Value::Array(new_array_ref(result)));
                            }
                            Err(e) => {
                                self.raise(e)?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        }
                    } else {
                        // Real scalar * Real array: element-wise multiplication
                        use crate::vm::matmul::scalar_vector_mul_real;
                        let scalar_f64 = match scalar_val {
                            Value::F64(v) => v,
                            Value::I64(v) => v as f64,
                            _ => {
                                return Err(VmError::InternalError(format!(
                                    "real scalar-array path: unexpected scalar {:?}",
                                    scalar_val
                                )))
                            }
                        };
                        let mul_result = scalar_vector_mul_real(scalar_f64, &arr);
                        drop(arr);
        
                        match mul_result {
                            Ok(result) => {
                                self.stack.push(Value::Array(new_array_ref(result)));
                            }
                            Err(e) => {
                                self.raise(e)?;
                                return Ok(CallDynamicResult::Continue);
                            }
                        }
                    }
                } else if matches!((&left, &right), (Value::Array(_), Value::Array(_)))
                    && matches!(fallback_intrinsic, Intrinsic::MulFloat)
                {
                    // Array * Array: use matrix multiplication (handles mixed real/complex)
                    use crate::vm::matmul::{is_complex_array, matmul, matmul_complex};
                    let a = match &left {
                        Value::Array(arr) => arr.borrow(),
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "array-array matmul path: unexpected left operand {:?}",
                                left
                            )))
                        }
                    };
                    let b = match &right {
                        Value::Array(arr) => arr.borrow(),
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "array-array matmul path: unexpected right operand {:?}",
                                right
                            )))
                        }
                    };
                    let a_is_complex = is_complex_array(&a);
                    let b_is_complex = is_complex_array(&b);
                    let mul_result = if a_is_complex || b_is_complex {
                        matmul_complex(&a, &b, &self.struct_heap)
                    } else {
                        matmul(&a, &b)
                    };
                    drop(a);
                    drop(b);
                    match mul_result {
                        Ok(mut result) => {
                            // Store correct Complex type_id (Issue #1804)
                            if result
                                .element_type_override
                                .as_ref()
                                .is_some_and(|e| e.is_complex())
                            {
                                result.struct_type_id = Some(self.get_complex_type_id());
                            }
                            self.stack.push(Value::Array(new_array_ref(result)));
                        }
                        Err(e) => {
                            self.raise(e)?;
                            return Ok(CallDynamicResult::Continue);
                        }
                    }
                } else if matches!(
                    (&left, &right),
                    (
                        Value::I64(_) | Value::F64(_) | Value::F32(_),
                        Value::Array(_)
                    ) | (
                        Value::Array(_),
                        Value::I64(_) | Value::F64(_) | Value::F32(_)
                    )
                ) && matches!(
                    fallback_intrinsic,
                    Intrinsic::DivFloat
                        | Intrinsic::AddFloat
                        | Intrinsic::SubFloat
                        | Intrinsic::MulFloat
                ) {
                    // Array / Scalar, Scalar / Array, and other element-wise ops (Issue #1929)
                    // Use dynamic_ops which already handles Array/Scalar operations
                    let result = match fallback_intrinsic {
                        Intrinsic::AddFloat => self.dynamic_add(&left, &right)?,
                        Intrinsic::SubFloat => self.dynamic_sub(&left, &right)?,
                        Intrinsic::MulFloat => self.dynamic_mul(&left, &right)?,
                        Intrinsic::DivFloat => self.dynamic_div(&left, &right)?,
                        _ => {
                            return Err(VmError::InternalError(format!(
                                "array-scalar path: unexpected intrinsic {:?}",
                                fallback_intrinsic
                            )))
                        }
                    };
                    self.stack.push(result);
                } else {
                    // No matching method found - raise MethodError
                    self.raise(VmError::no_method_matching_op(
                        &left_type_name, &right_type_name,
                    ))?;
                    return Ok(CallDynamicResult::Continue);
                }
            }
        }
        Ok(CallDynamicResult::Handled)
    }
}
