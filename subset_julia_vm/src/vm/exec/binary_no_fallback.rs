//! Handler for `CallDynamicBinaryNoFallback` instruction.
//!
//! Extracted from `call_dynamic_binary.rs` to reduce function length (Issue #2935).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::*;
use super::call_dynamic_binary::try_string_char_concat;
use super::util::{
    bind_value_to_slot, is_rust_dict_parametric_mismatch, is_struct_dict_bare_mismatch,
};
use super::DispatchAction;
use crate::inference_core::dispatch_resolver::{
    resolve_runtime_core_signature_candidates, RuntimeCoreCandidate,
};
use crate::rng::RngLike;

impl<R: RngLike> Vm<R> {
    /// Handle `CallDynamicBinaryNoFallback` dispatch.
    ///
    /// Runtime dispatch for binary operators WITHOUT builtin fallback.
    /// Used when user-defined methods shadow builtins completely.
    pub(super) fn execute_binary_no_fallback(
        &mut self,
        candidates: &[usize],
    ) -> Result<DispatchAction, VmError> {
        // Runtime dispatch for binary operators WITHOUT builtin fallback.
        // This is used when user-defined methods shadow builtins completely.
        let right = self.stack.pop_value()?;
        let left = self.stack.pop_value()?;

        // Get type names for both operands
        let left_type_name = self.get_type_name(&left);
        let right_type_name = self.get_type_name(&right);

        // Issue #6496: the payload carries only candidate function indices;
        // the expected signatures are derived from `FunctionInfo` and
        // memoized per function index. Issue #6502 slice 2: matching runs on
        // the structured `core_signature` projection.
        self.ensure_binary_candidate_signatures(candidates);

        // Scored dispatch: find the most specific match (Issue #2517).
        // Previously used break-on-first-match which could select a less
        // specific method over a more specific one.
        let actual_cores = [
            crate::vm::dispatch_binding::runtime_actual_core_type(
                &self.dispatch_julia_type_for_value(&left),
            ),
            crate::vm::dispatch_binding::runtime_actual_core_type(
                &self.dispatch_julia_type_for_value(&right),
            ),
        ];
        let matched = resolve_runtime_core_signature_candidates(
            &self.struct_hierarchy,
            candidates
                .iter()
                .enumerate()
                .filter_map(|(pos, &func_index)| {
                    let sig = self.binary_candidate_core_signature(func_index)?;
                    let (left_expected, right_expected) =
                        (sig.rendered[0].as_str(), sig.rendered[1].as_str());
                    // Value::Dict (Rust-backed) must not match parametric Dict{K,V}
                    // Pure Julia methods that expect StructRef (Issue #2748).
                    if is_rust_dict_parametric_mismatch(&left, left_expected)
                        || is_rust_dict_parametric_mismatch(&right, right_expected)
                        || is_struct_dict_bare_mismatch(&left, left_expected, &self.struct_heap)
                        || is_struct_dict_bare_mismatch(&right, right_expected, &self.struct_heap)
                    {
                        return None;
                    }

                    Some(RuntimeCoreCandidate {
                        idx: pos,
                        slots: [&sig.slots[0], &sig.slots[1]],
                        signature: sig.signature.as_ref(),
                    })
                }),
            &actual_cores,
            |actual, expected| self.check_subtype_core(actual, expected),
        )
        .map(|(pos, _)| candidates[pos]);

        if let Some(func_index) = matched {
            // Call the matched method
            let func = match self.get_function_cloned_or_raise(func_index)? {
                Some(f) => f,
                None => return Ok(DispatchAction::Continue),
            };

            let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

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
            self.try_push_call_frame(frame)?;
            self.ip = func.entry;
        } else {
            // Check for String/Char concatenation via * before raising MethodError (Issue #2127)
            if let Some(result) = try_string_char_concat(&left, &right) {
                self.stack.push(result);
                return Ok(DispatchAction::Continue);
            }
            // No matching method - raise MethodError (no fallback)
            self.raise(VmError::no_method_matching_op(
                &left_type_name,
                &right_type_name,
            ))?;
            return Ok(DispatchAction::Continue);
        }
        Ok(DispatchAction::Continue)
    }
}
