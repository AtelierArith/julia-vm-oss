//! Value-mode HOF execution helpers.

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::frame::{BroadcastInput, BroadcastResults, BroadcastState, Frame, HofOpKind};
use super::super::util::bind_value_to_slot;
use super::super::value::{
    new_array_ref, new_typed_array_ref, ArrayData, ArrayValue, TupleValue, TypedArrayValue, Value,
};
use super::super::Vm;
impl<R: RngLike> Vm<R> {

    /// Start a HOF call with value-based input (for struct arrays)
    pub(crate) fn start_hof_call_values(
        &mut self,
        func_index: usize,
        values: Vec<Value>,
        shape: Vec<usize>,
        op_kind: HofOpKind,
    ) -> Result<(), VmError> {
        if values.is_empty() {
            // Empty array - return empty result immediately
            match op_kind {
                // Note: Map, Filter removed - now Pure Julia (base/iterators.jl)
                HofOpKind::Broadcast => {
                    self.stack
                        .push(Value::Array(new_typed_array_ref(TypedArrayValue::new(
                            ArrayData::Any(vec![]),
                            shape,
                        ))));
                }
                HofOpKind::FindAll => {
                    // Empty array returns empty Int64 array
                    self.stack
                        .push(Value::Array(new_array_ref(ArrayValue::from_i64(
                            vec![],
                            vec![0],
                        ))));
                }
                _ => {
                    self.stack.push(Value::Nothing);
                }
            }
            return Ok(());
        }

        let first_val = values[0].clone();
        let capacity = values.len();

        self.broadcast_state = Some(BroadcastState {
            func_index,
            input: BroadcastInput::Values(values),
            input_shape: shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_values(capacity),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind,
            accumulator: None,
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: true,
            reduce_func_index: None,
        });

        self.call_function_with_value(func_index, first_val)
    }

    /// Call a function with any Value argument (for struct array HOF)
    pub(super) fn call_function_with_value(
        &mut self,
        func_index: usize,
        arg: Value,
    ) -> Result<(), VmError> {
        self.call_function_with_value_and_extra_args(func_index, arg, &[])
    }

    /// Call a function with any Value argument plus extra args (for broadcast with Ref)
    pub(super) fn call_function_with_value_and_extra_args(
        &mut self,
        func_index: usize,
        arg: Value,
        extra_args: &[Value],
    ) -> Result<(), VmError> {
        let func = self.get_function_checked(func_index)?;
        let local_slot_count = func.local_slot_count;
        let param_slots: Vec<usize> = func.param_slots.clone();
        let entry = func.entry;

        let mut frame = Frame::new_with_slots(local_slot_count, Some(func_index));

        if let Some(&slot) = param_slots.first() {
            bind_value_to_slot(&mut frame, slot, arg, &mut self.struct_heap);
        }

        for (i, extra_arg) in extra_args.iter().enumerate() {
            if let Some(&slot) = param_slots.get(i + 1) {
                bind_value_to_slot(&mut frame, slot, extra_arg.clone(), &mut self.struct_heap);
            }
        }

        self.return_ips.push(self.ip);
        self.frames.push(frame);
        self.ip = entry;
        Ok(())
    }

    /// Handle return value from HOF function call in value mode
    /// Called when is_value_mode is true
    pub(crate) fn handle_hof_return_value(&mut self, result: Value) -> Result<(), VmError> {
        let bc_state = self.broadcast_state.as_mut().ok_or_else(|| {
            VmError::InternalError(
                "handle_hof_return_value called without broadcast_state".to_string(),
            )
        })?;
        let op_kind = bc_state.op_kind;
        let current_idx = bc_state.current_index;

        // Calculate element count
        let element_count: usize = bc_state.input_shape.iter().product();

        // Note: input_val was used for Filter which is now Pure Julia

        // Pop the current frame
        self.frames.pop();
        self.return_ips.pop();

        match op_kind {
            HofOpKind::Broadcast => {
                // Collect result into results array
                bc_state.results.push_value(result);
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    // More elements to process
                    if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        let extra_args = bc_state.extra_args.clone();
                        if extra_args.is_empty() {
                            self.call_function_with_value(func_index, next_val)?;
                        } else {
                            self.call_function_with_value_and_extra_args(
                                func_index,
                                next_val,
                                &extra_args,
                            )?;
                        }
                    }
                } else {
                    // All elements processed - create result array
                    let result_values = bc_state.results.take_values();
                    let result_shape = bc_state.input_shape.clone();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;

                    // Create appropriate result array based on result types
                    let result_array =
                        self.create_typed_array_from_values(result_values, result_shape);
                    self.stack.push(result_array);
                    self.ip = return_ip;
                }
            }

            // Note: HofOpKind::Filter removed - filter is now Pure Julia (base/iterators.jl)
            HofOpKind::FindAll => {
                // Collect 1-based index if result is truthy
                let is_truthy = match &result {
                    Value::Bool(b) => *b,
                    Value::I64(v) => *v != 0,
                    Value::F64(v) => *v != 0.0,
                    Value::Nothing => false,
                    _ => true, // Non-nothing values are truthy
                };
                if is_truthy {
                    // Push 1-based index as i64
                    bc_state
                        .results
                        .push_i64((bc_state.current_index + 1) as i64);
                }
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        let extra_args = bc_state.extra_args.clone();
                        if extra_args.is_empty() {
                            self.call_function_with_value(func_index, next_val)?;
                        } else {
                            self.call_function_with_value_and_extra_args(
                                func_index,
                                next_val,
                                &extra_args,
                            )?;
                        }
                    }
                } else {
                    // All elements processed - create result array of Int64 indices
                    let result_data = bc_state.results.take_i64();
                    let len = result_data.len();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack
                        .push(Value::Array(new_array_ref(ArrayValue::from_i64(
                            result_data,
                            vec![len],
                        ))));
                    self.ip = return_ip;
                }
            }

            HofOpKind::FindFirst => {
                // Short-circuit: if result is truthy, return 1-based index immediately
                let is_truthy = match &result {
                    Value::Bool(b) => *b,
                    Value::I64(v) => *v != 0,
                    Value::F64(v) => *v != 0.0,
                    Value::Nothing => false,
                    _ => true,
                };
                if is_truthy {
                    let index = (bc_state.current_index + 1) as i64;
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::I64(index));
                    self.ip = return_ip;
                } else {
                    bc_state.current_index += 1;
                    if bc_state.current_index < element_count {
                        if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                            let func_index = bc_state.func_index;
                            let extra_args = bc_state.extra_args.clone();
                            if extra_args.is_empty() {
                                self.call_function_with_value(func_index, next_val)?;
                            } else {
                                self.call_function_with_value_and_extra_args(
                                    func_index,
                                    next_val,
                                    &extra_args,
                                )?;
                            }
                        }
                    } else {
                        // All elements checked, none matched - return nothing
                        let return_ip = bc_state.return_ip_after_broadcast;
                        self.broadcast_state = None;
                        self.stack.push(Value::Nothing);
                        self.ip = return_ip;
                    }
                }
            }

            HofOpKind::FindLast => {
                // Track last matching index using accumulator
                let is_truthy = match &result {
                    Value::Bool(b) => *b,
                    Value::I64(v) => *v != 0,
                    Value::F64(v) => *v != 0.0,
                    Value::Nothing => false,
                    _ => true,
                };
                if is_truthy {
                    bc_state.accumulator = Some(Value::I64((bc_state.current_index + 1) as i64));
                }
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        let extra_args = bc_state.extra_args.clone();
                        if extra_args.is_empty() {
                            self.call_function_with_value(func_index, next_val)?;
                        } else {
                            self.call_function_with_value_and_extra_args(
                                func_index,
                                next_val,
                                &extra_args,
                            )?;
                        }
                    }
                } else {
                    // All elements processed - return last matching index or nothing
                    let result_val = match bc_state.accumulator {
                        Some(Value::I64(idx)) => Value::I64(idx),
                        _ => Value::Nothing,
                    };
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(result_val);
                    self.ip = return_ip;
                }
            }

            HofOpKind::Ntuple => {
                // Collect result into tuple
                bc_state.results.push_value(result);
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    // More elements to process - call f with next index
                    if let Some(Value::F64(next_idx)) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        self.call_function_with_arg(func_index, next_idx)?;
                    }
                } else {
                    // All elements processed - create tuple from results
                    let result_values = bc_state.results.take_values();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack
                        .push(Value::Tuple(TupleValue::new(result_values)));
                    self.ip = return_ip;
                }
            }

            HofOpKind::TupleMap => {
                // Collect result into tuple (same as Ntuple but uses Value input)
                bc_state.results.push_value(result);
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    // More elements to process - call f with next value and extra args
                    if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        let extra_args = bc_state.extra_args.clone();
                        if extra_args.is_empty() {
                            self.call_function_with_value(func_index, next_val)?;
                        } else {
                            self.call_function_with_value_and_extra_args(
                                func_index,
                                next_val,
                                &extra_args,
                            )?;
                        }
                    }
                } else {
                    // All elements processed - create tuple from results
                    let result_values = bc_state.results.take_values();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack
                        .push(Value::Tuple(TupleValue::new(result_values)));
                    self.ip = return_ip;
                }
            }

            // Note: HofOpKind::Reduce, ForEach removed - now Pure Julia
            _ => {
                // Other operations (Sum, Any, All, Count) can use the f64 path
                // by converting the result to f64
                let result_f64 = match result {
                    Value::F64(v) => v,
                    Value::I64(v) => v as f64,
                    Value::Bool(b) => {
                        if b {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    _ => 0.0,
                };
                // Temporarily set is_value_mode to false to use the f64 path
                bc_state.is_value_mode = false;
                // Re-increment index since we'll be handling it in handle_hof_return
                bc_state.current_index = current_idx;
                // Re-push frames that were popped
                // Actually, we already popped them, so we need to handle this differently
                // For now, just handle these cases directly
                match op_kind {
                    HofOpKind::Sum => {
                        let current_sum = match bc_state.accumulator {
                            Some(Value::F64(v)) => v,
                            _ => 0.0,
                        };
                        bc_state.accumulator = Some(Value::F64(current_sum + result_f64));
                        bc_state.current_index += 1;

                        if bc_state.current_index < element_count {
                            if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                                let func_index = bc_state.func_index;
                                self.call_function_with_value(func_index, next_val)?;
                            }
                        } else {
                            let final_sum = match bc_state.accumulator {
                                Some(Value::F64(v)) => v,
                                _ => 0.0,
                            };
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.broadcast_state = None;
                            self.stack.push(Value::F64(final_sum));
                            self.ip = return_ip;
                        }
                    }
                    HofOpKind::Any => {
                        // Short-circuit: if result is truthy, we're done
                        if result_f64 != 0.0 {
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.broadcast_state = None;
                            self.stack.push(Value::Bool(true));
                            self.ip = return_ip;
                        } else {
                            bc_state.current_index += 1;
                            if bc_state.current_index < element_count {
                                if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                                    let func_index = bc_state.func_index;
                                    self.call_function_with_value(func_index, next_val)?;
                                }
                            } else {
                                // All elements processed, no truthy value found
                                let return_ip = bc_state.return_ip_after_broadcast;
                                self.broadcast_state = None;
                                self.stack.push(Value::Bool(false));
                                self.ip = return_ip;
                            }
                        }
                    }
                    HofOpKind::All => {
                        // Short-circuit: if result is falsy, we're done
                        if result_f64 == 0.0 {
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.broadcast_state = None;
                            self.stack.push(Value::Bool(false));
                            self.ip = return_ip;
                        } else {
                            bc_state.current_index += 1;
                            if bc_state.current_index < element_count {
                                if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                                    let func_index = bc_state.func_index;
                                    self.call_function_with_value(func_index, next_val)?;
                                }
                            } else {
                                // All elements processed, all truthy
                                let return_ip = bc_state.return_ip_after_broadcast;
                                self.broadcast_state = None;
                                self.stack.push(Value::Bool(true));
                                self.ip = return_ip;
                            }
                        }
                    }
                    HofOpKind::Count => {
                        // Increment count if result is truthy
                        let current_count = match bc_state.accumulator {
                            Some(Value::I64(v)) => v,
                            _ => 0,
                        };
                        if result_f64 != 0.0 {
                            bc_state.accumulator = Some(Value::I64(current_count + 1));
                        }
                        bc_state.current_index += 1;
                        if bc_state.current_index < element_count {
                            if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                                let func_index = bc_state.func_index;
                                self.call_function_with_value(func_index, next_val)?;
                            }
                        } else {
                            let final_count = match bc_state.accumulator {
                                Some(Value::I64(v)) => v,
                                _ => 0,
                            };
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.broadcast_state = None;
                            self.stack.push(Value::I64(final_count));
                            self.ip = return_ip;
                        }
                    }
                    // Note: ForEach removed - foreach is now Pure Julia (base/abstractarray.jl)
                    _ => {
                        // Default: just finish
                        let return_ip = bc_state.return_ip_after_broadcast;
                        self.broadcast_state = None;
                        self.stack.push(result);
                        self.ip = return_ip;
                    }
                }
            }
        }
        Ok(())
    }

    /// Create a TypedArray from a vector of Values
    fn create_typed_array_from_values(&mut self, values: Vec<Value>, shape: Vec<usize>) -> Value {
        if values.is_empty() {
            return Value::Array(new_typed_array_ref(TypedArrayValue::new(
                ArrayData::Any(vec![]),
                shape,
            )));
        }

        let mut values = values;
        for v in values.iter_mut() {
            if let Value::Struct(s) = v {
                let idx = self.struct_heap.len();
                self.struct_heap.push(s.clone());
                *v = Value::StructRef(idx);
            }
        }

        // Check if all values are the same type
        let _first = &values[0];
        let all_f64 = values.iter().all(|v| matches!(v, Value::F64(_)));
        let all_i64 = values.iter().all(|v| matches!(v, Value::I64(_)));
        let all_struct_ref = values.iter().all(|v| matches!(v, Value::StructRef(_)));

        if all_f64 {
            let data: Vec<f64> = values
                .iter()
                .map(|v| if let Value::F64(f) = v { *f } else { 0.0 })
                .collect();
            Value::Array(new_typed_array_ref(TypedArrayValue::new(
                ArrayData::F64(data),
                shape,
            )))
        } else if all_i64 {
            let data: Vec<i64> = values
                .iter()
                .map(|v| if let Value::I64(i) = v { *i } else { 0 })
                .collect();
            Value::Array(new_typed_array_ref(TypedArrayValue::new(
                ArrayData::I64(data),
                shape,
            )))
        } else if all_struct_ref {
            let data: Vec<usize> = values
                .iter()
                .map(|v| {
                    if let Value::StructRef(idx) = v {
                        *idx
                    } else {
                        0
                    }
                })
                .collect();
            let struct_type_id = values.iter().find_map(|v| {
                if let Value::StructRef(idx) = v {
                    self.struct_heap.get(*idx).map(|s| s.type_id)
                } else {
                    None
                }
            });
            let mut arr = TypedArrayValue::new(ArrayData::StructRefs(data), shape);
            arr.struct_type_id = struct_type_id;
            Value::Array(new_typed_array_ref(arr))
        } else {
            // Mixed types - use Any array
            Value::Array(new_typed_array_ref(TypedArrayValue::new(
                ArrayData::Any(values),
                shape,
            )))
        }
    }

}
