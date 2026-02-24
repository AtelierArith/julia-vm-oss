//! Binary operator dynamic dispatch handlers.
//!
//! Handles: CallDynamicBinary, CallDynamicBinaryBoth, CallDynamicBinaryNoFallback
//!
//! These instructions handle runtime method dispatch for binary operators
//! when one or both operand types are unknown at compile time.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::call_dynamic::CallDynamicResult;
#[cfg(debug_assertions)]
use super::call_dynamic::dispatch_debug_log;
use super::util::{
    bind_value_to_slot, extract_base_type, is_rust_dict_parametric_mismatch, score_type_match,
};
use crate::rng::RngLike;

#[cfg(debug_assertions)]
use super::call_dynamic::dispatch_debug_enabled;

/// Try to perform String/Char concatenation via `*` operator (Issue #2127).
/// Julia's `*` operator concatenates strings and chars: "a" * 'b' == "ab".
/// Returns Some(Value::Str) if both operands are String or Char, None otherwise.
pub(super) fn try_string_char_concat(left: &Value, right: &Value) -> Option<Value> {
    let l = match left {
        Value::Str(s) => s.as_str(),
        Value::Char(c) => return try_string_char_concat_with_char(*c, right, true),
        _ => return None,
    };
    match right {
        Value::Str(r) => Some(Value::Str(format!("{}{}", l, r))),
        Value::Char(c) => {
            let mut result = l.to_string();
            result.push(*c);
            Some(Value::Str(result))
        }
        _ => None,
    }
}

/// Helper for Char on the left side.
pub(super) fn try_string_char_concat_with_char(c: char, right: &Value, _left_is_char: bool) -> Option<Value> {
    match right {
        Value::Str(r) => {
            let mut result = String::with_capacity(r.len() + c.len_utf8());
            result.push(c);
            result.push_str(r);
            Some(Value::Str(result))
        }
        Value::Char(r) => {
            let mut result = String::with_capacity(c.len_utf8() + r.len_utf8());
            result.push(c);
            result.push(*r);
            Some(Value::Str(result))
        }
        _ => None,
    }
}

impl<R: RngLike> Vm<R> {
    /// Execute binary operator dynamic dispatch instructions.
    ///
    /// Returns `CallDynamicResult::NotHandled` if the instruction is not a binary dispatch operation.
    #[inline]
    pub(super) fn execute_call_dynamic_binary(
        &mut self,
        instr: &Instr,
    ) -> Result<CallDynamicResult, VmError> {
        match instr {
            Instr::CallDynamicBinary(_fallback_func_index, check_position, ref candidates) => {
                // Runtime method dispatch for binary operators with one Any-typed operand.
                // Pop both arguments
                let right = self.stack.pop_value()?;
                let left = self.stack.pop_value()?;

                // Check the type of the argument at check_position
                let checked_arg = if *check_position == 0 { &left } else { &right };
                let arg_type_name = self.get_type_name(checked_arg);

                #[cfg(debug_assertions)]
                if dispatch_debug_enabled() {
                    let left_type = self.get_type_name(&left);
                    let right_type = self.get_type_name(&right);
                    dispatch_debug_log(format_args!(
                        "[DISPATCH] Binary: ({}, {}) check_pos={}, candidates={}",
                        left_type,
                        right_type,
                        check_position,
                        candidates.len()
                    ));
                }

                // Check dispatch cache first (Issue #2943, #3355)
                let call_site_ip = self.ip - 1;
                let type_hash = hash_type_name(&arg_type_name);
                let selected_func_index = if let Some(&cached) = self
                    .dispatch_cache
                    .get(&call_site_ip)
                    .and_then(|m| m.get(&type_hash))
                {
                    if cached == usize::MAX {
                        None
                    } else {
                        Some(cached)
                    }
                } else {
                    // Find best matching candidate using scored dispatch (Issue #2511, #2517).
                    // Uses shared score_type_match() for consistent scoring across all handlers.
                    let arg_base = extract_base_type(&arg_type_name);
                    let mut best: Option<(usize, u32)> = None;
                    for (idx, expected_type) in candidates.iter() {
                        // Value::Dict (Rust-backed) must not match parametric Dict{K,V}
                        // Pure Julia methods that expect StructRef (Issue #2748).
                        if is_rust_dict_parametric_mismatch(checked_arg, expected_type) {
                            continue;
                        }
                        let mut score =
                            score_type_match(expected_type, &arg_type_name, arg_base);
                        if score == 0 && self.check_subtype(&arg_type_name, expected_type) {
                            score = 1;
                        }
                        if score > 0 && best.is_none_or(|(_, best_score)| score > best_score) {
                            best = Some((*idx, score));
                        }
                    }
                    let result = best.map(|(idx, _)| idx);
                    // Store in cache using hashed key (Issue #3355)
                    let cache_val = result.unwrap_or(usize::MAX);
                    self.dispatch_cache
                        .entry(call_site_ip)
                        .or_default()
                        .insert(type_hash, cache_val);
                    result
                };

                // If no matching candidate found, check for string concatenation fallback
                let selected_func_index = match selected_func_index {
                    Some(idx) => idx,
                    None => {
                        // Check for String/Char concatenation via * before raising MethodError (Issue #2127)
                        if let Some(result) = try_string_char_concat(&left, &right) {
                            self.stack.push(result);
                            return Ok(CallDynamicResult::Handled);
                        }
                        // No matching method - raise MethodError
                        let other_arg = if *check_position == 0 { &right } else { &left };
                        let other_type_name = self.get_type_name(other_arg);
                        let (left_type, right_type) = if *check_position == 0 {
                            (arg_type_name.clone(), other_type_name)
                        } else {
                            (other_type_name, arg_type_name.clone())
                        };
                        self.raise(VmError::no_method_matching_op(
                            &left_type, &right_type,
                        ))?;
                        return Ok(CallDynamicResult::Continue);
                    }
                };

                let func = match self.get_function_cloned_or_raise(selected_func_index)? {
                    Some(f) => f,
                    None => return Ok(CallDynamicResult::Continue),
                };

                let mut frame =
                    Frame::new_with_slots(func.local_slot_count, Some(selected_func_index));

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
                Ok(CallDynamicResult::Handled)
            }

            Instr::CallDynamicBinaryBoth(ref fallback_intrinsic, ref candidates) => {
                self.execute_binary_both(fallback_intrinsic, candidates)
            }

            Instr::CallDynamicBinaryNoFallback(ref candidates) => {
                self.execute_binary_no_fallback(candidates)
            }

            _ => Ok(CallDynamicResult::NotHandled),
        }
    }
}
