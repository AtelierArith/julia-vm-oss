//! Handler for `CallDynamicBinaryNoFallback` instruction.
//!
//! Extracted from `call_dynamic_binary.rs` to reduce function length (Issue #2935).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::call_dynamic::CallDynamicResult;
use super::call_dynamic_binary::try_string_char_concat;
use super::util::{
    bind_value_to_slot, extract_base_type, is_rust_dict_parametric_mismatch, score_type_match,
};
use crate::rng::RngLike;

impl<R: RngLike> Vm<R> {
    /// Handle `CallDynamicBinaryNoFallback` dispatch.
    ///
    /// Runtime dispatch for binary operators WITHOUT builtin fallback.
    /// Used when user-defined methods shadow builtins completely.
    pub(super) fn execute_binary_no_fallback(
        &mut self,
        candidates: &[(usize, String, String)],
    ) -> Result<CallDynamicResult, VmError> {
        // Runtime dispatch for binary operators WITHOUT builtin fallback.
        // This is used when user-defined methods shadow builtins completely.
        let right = self.stack.pop_value()?;
        let left = self.stack.pop_value()?;
        
        // Get type names for both operands
        let left_type_name = self.get_type_name(&left);
        let right_type_name = self.get_type_name(&right);
        
        // Scored dispatch: find the most specific match (Issue #2517).
        // Previously used break-on-first-match which could select a less
        // specific method over a more specific one.
        let left_base = extract_base_type(&left_type_name);
        let right_base = extract_base_type(&right_type_name);
        
        let matched = {
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
        
        if let Some((func_index, _, _)) = matched {
            // Call the matched method
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
            // Check for String/Char concatenation via * before raising MethodError (Issue #2127)
            if let Some(result) = try_string_char_concat(&left, &right) {
                self.stack.push(result);
                return Ok(CallDynamicResult::Handled);
            }
            // No matching method - raise MethodError (no fallback)
            self.raise(VmError::no_method_matching_op(
                &left_type_name, &right_type_name,
            ))?;
            return Ok(CallDynamicResult::Continue);
        }
        Ok(CallDynamicResult::Handled)
    }
}
