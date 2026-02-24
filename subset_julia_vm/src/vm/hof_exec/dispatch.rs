//! F64-mode HOF return dispatch.

use crate::rng::RngLike;

use crate::vm::broadcast::{broadcast_get_index, compute_strides, expand_shapes_for_julia};
use crate::vm::error::VmError;
use crate::vm::frame::{BroadcastInput, HofOpKind};
use crate::vm::value::{new_array_ref, ArrayData, ArrayValue, Value};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Handle return value from HOF function call
    /// Called by ReturnF64/ReturnI64 when in HOF mode
    pub(in crate::vm) fn handle_hof_return(&mut self, result: f64) -> Result<(), VmError> {
        let bc_state = self.broadcast_state.as_mut().ok_or_else(|| {
            VmError::InternalError("handle_hof_return called without broadcast_state".to_string())
        })?;
        let op_kind = bc_state.op_kind;
        let current_idx = bc_state.current_index;

        // Calculate element count
        let element_count: usize = bc_state.input_shape.iter().product();

        // Get input value at current index (for filter)
        let input_val = bc_state.input.get(current_idx).unwrap_or(Value::F64(0.0));

        // Pop the current frame
        self.frames.pop();
        self.return_ips.pop();

        match op_kind {
            HofOpKind::Broadcast => {
                // Collect result into results array
                bc_state.results.push_f64(result);
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    // More elements to process
                    let next_val = bc_state.input.get(bc_state.current_index);
                    let func_index = bc_state.func_index;
                    let extra_args = bc_state.extra_args.clone();

                    if let Some(Value::F64(v)) = next_val {
                        if extra_args.is_empty() {
                            self.call_function_with_arg(func_index, v)?;
                        } else {
                            self.call_function_with_extra_args(
                                func_index,
                                Value::F64(v),
                                &extra_args,
                            )?;
                        }
                    }
                } else {
                    // All elements processed - create result array
                    let result_data = bc_state.results.take_f64();
                    let result_shape = bc_state.input_shape.clone();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack
                        .push(Value::Array(new_array_ref(ArrayValue::from_f64(
                            result_data,
                            result_shape,
                        ))));
                    self.ip = return_ip;
                }
            }

            // Note: HofOpKind::Filter removed - filter is now Pure Julia (base/iterators.jl)
            HofOpKind::FindAll => {
                // Collect 1-based index if result is truthy (non-zero)
                if result != 0.0 {
                    // Push 1-based index as i64
                    bc_state
                        .results
                        .push_i64((bc_state.current_index + 1) as i64);
                }
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    let next_val = bc_state.input.get(bc_state.current_index);
                    let func_index = bc_state.func_index;
                    let extra_args = bc_state.extra_args.clone();

                    if let Some(Value::F64(v)) = next_val {
                        if extra_args.is_empty() {
                            self.call_function_with_arg(func_index, v)?;
                        } else {
                            self.call_function_with_extra_args(
                                func_index,
                                Value::F64(v),
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
                if result != 0.0 {
                    let index = (bc_state.current_index + 1) as i64;
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::I64(index));
                    self.ip = return_ip;
                } else {
                    bc_state.current_index += 1;
                    if bc_state.current_index < element_count {
                        let next_val = bc_state.input.get(bc_state.current_index);
                        let func_index = bc_state.func_index;
                        let extra_args = bc_state.extra_args.clone();

                        if let Some(Value::F64(v)) = next_val {
                            if extra_args.is_empty() {
                                self.call_function_with_arg(func_index, v)?;
                            } else {
                                self.call_function_with_extra_args(
                                    func_index,
                                    Value::F64(v),
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
                if result != 0.0 {
                    bc_state.accumulator = Some(Value::I64((bc_state.current_index + 1) as i64));
                }
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    let next_val = bc_state.input.get(bc_state.current_index);
                    let func_index = bc_state.func_index;
                    let extra_args = bc_state.extra_args.clone();

                    if let Some(Value::F64(v)) = next_val {
                        if extra_args.is_empty() {
                            self.call_function_with_arg(func_index, v)?;
                        } else {
                            self.call_function_with_extra_args(
                                func_index,
                                Value::F64(v),
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

            // Note: HofOpKind::Reduce, ForEach removed - now Pure Julia
            HofOpKind::Sum => {
                // Accumulate sum of results
                let current_sum = match bc_state.accumulator {
                    Some(Value::F64(v)) => v,
                    _ => 0.0,
                };
                bc_state.accumulator = Some(Value::F64(current_sum + result));
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    let next_val = bc_state.input.get(bc_state.current_index);
                    let func_index = bc_state.func_index;
                    let extra_args = bc_state.extra_args.clone();

                    if let Some(Value::F64(v)) = next_val {
                        if extra_args.is_empty() {
                            self.call_function_with_arg(func_index, v)?;
                        } else {
                            self.call_function_with_extra_args(
                                func_index,
                                Value::F64(v),
                                &extra_args,
                            )?;
                        }
                    }
                } else {
                    // All elements processed - return final sum
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
                // Short-circuit: if result is truthy, we're done (Issue #2031)
                if result != 0.0 {
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::Bool(true));
                    self.ip = return_ip;
                } else {
                    bc_state.current_index += 1;
                    if bc_state.current_index < element_count {
                        let next_val = bc_state.input.get(bc_state.current_index);
                        let func_index = bc_state.func_index;
                        let extra_args = bc_state.extra_args.clone();

                        if let Some(Value::F64(v)) = next_val {
                            if extra_args.is_empty() {
                                self.call_function_with_arg(func_index, v)?;
                            } else {
                                self.call_function_with_extra_args(
                                    func_index,
                                    Value::F64(v),
                                    &extra_args,
                                )?;
                            }
                        }
                    } else {
                        // All elements checked, none were true (Issue #2031)
                        let return_ip = bc_state.return_ip_after_broadcast;
                        self.broadcast_state = None;
                        self.stack.push(Value::Bool(false));
                        self.ip = return_ip;
                    }
                }
            }

            HofOpKind::All => {
                // Short-circuit: if result is falsy, we're done (Issue #2031)
                if result == 0.0 {
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::Bool(false));
                    self.ip = return_ip;
                } else {
                    bc_state.current_index += 1;
                    if bc_state.current_index < element_count {
                        let next_val = bc_state.input.get(bc_state.current_index);
                        let func_index = bc_state.func_index;
                        let extra_args = bc_state.extra_args.clone();

                        if let Some(Value::F64(v)) = next_val {
                            if extra_args.is_empty() {
                                self.call_function_with_arg(func_index, v)?;
                            } else {
                                self.call_function_with_extra_args(
                                    func_index,
                                    Value::F64(v),
                                    &extra_args,
                                )?;
                            }
                        }
                    } else {
                        // All elements checked, all were true (Issue #2031)
                        let return_ip = bc_state.return_ip_after_broadcast;
                        self.broadcast_state = None;
                        self.stack.push(Value::Bool(true));
                        self.ip = return_ip;
                    }
                }
            }

            HofOpKind::Count => {
                // Accumulate count of truthy results
                let current_count = match bc_state.accumulator {
                    Some(Value::F64(v)) => v,
                    Some(Value::I64(v)) => v as f64,
                    _ => 0.0,
                };
                if result != 0.0 {
                    bc_state.accumulator = Some(Value::F64(current_count + 1.0));
                }
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    let next_val = bc_state.input.get(bc_state.current_index);
                    let func_index = bc_state.func_index;
                    let extra_args = bc_state.extra_args.clone();

                    if let Some(Value::F64(v)) = next_val {
                        if extra_args.is_empty() {
                            self.call_function_with_arg(func_index, v)?;
                        } else {
                            self.call_function_with_extra_args(
                                func_index,
                                Value::F64(v),
                                &extra_args,
                            )?;
                        }
                    }
                } else {
                    // All elements processed - return final count as Int64
                    let final_count = match bc_state.accumulator {
                        Some(Value::F64(v)) => v as i64,
                        Some(Value::I64(v)) => v,
                        _ => 0,
                    };
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::I64(final_count));
                    self.ip = return_ip;
                }
            }

            HofOpKind::Broadcast2 => {
                // broadcast(f, A, B) - two-input broadcast with shape broadcasting
                bc_state.results.push_f64(result);
                bc_state.current_index += 1;

                // Get result shape (from the stored broadcast result_shape)
                let result_shape = bc_state.result_shape.clone().ok_or_else(|| {
                    VmError::InternalError("Broadcast2 requires result_shape".to_string())
                })?;
                let element_count: usize = result_shape.iter().product();

                if bc_state.current_index < element_count {
                    // More elements to process
                    // Compute broadcast indices for both inputs
                    let a_shape = &bc_state.input_shape;
                    let b_shape = bc_state.input2_shape.as_ref().ok_or_else(|| {
                        VmError::InternalError("Broadcast2 requires input2_shape".to_string())
                    })?;

                    let (a_expanded, b_expanded) = expand_shapes_for_julia(a_shape, b_shape);
                    let result_strides = compute_strides(&result_shape);
                    let a_strides = compute_strides(&a_expanded);
                    let b_strides = compute_strides(&b_expanded);
                    let a_ndims_diff = result_shape.len() - a_expanded.len();
                    let b_ndims_diff = result_shape.len() - b_expanded.len();

                    let idx = bc_state.current_index;
                    let a_idx = broadcast_get_index(
                        idx,
                        &result_shape,
                        &result_strides,
                        &a_expanded,
                        &a_strides,
                        a_ndims_diff,
                    );
                    let b_idx = broadcast_get_index(
                        idx,
                        &result_shape,
                        &result_strides,
                        &b_expanded,
                        &b_strides,
                        b_ndims_diff,
                    );

                    let next_a = match &bc_state.input {
                        BroadcastInput::F64(data) => data[a_idx],
                        BroadcastInput::Values(vals) => match &vals[a_idx] {
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            _ => 0.0,
                        },
                    };
                    let input2_ref = bc_state.input2.as_ref().ok_or_else(|| {
                        VmError::InternalError("Broadcast2 requires input2".to_string())
                    })?;
                    let next_b = match input2_ref {
                        BroadcastInput::F64(data) => data[b_idx],
                        BroadcastInput::Values(vals) => match &vals[b_idx] {
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            _ => 0.0,
                        },
                    };

                    let func_index = bc_state.func_index;
                    self.call_function_with_two_args(func_index, next_a, next_b)?;
                } else {
                    // All elements processed - create result array
                    let result_data = bc_state.results.take_f64();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack
                        .push(Value::Array(new_array_ref(ArrayValue::from_f64(
                            result_data,
                            result_shape,
                        ))));
                    self.ip = return_ip;
                }
            }

            HofOpKind::Broadcast2InPlace => {
                // broadcast!(f, dest, A, B) - in-place two-input broadcast
                // Store result directly into dest array at current_index
                let dest_ref = bc_state.dest_array.clone().ok_or_else(|| {
                    VmError::InternalError("Broadcast2InPlace requires dest_array".to_string())
                })?;
                {
                    let mut dest_borrow = dest_ref.borrow_mut();
                    dest_borrow.try_data_f64_mut()?[current_idx] = result;
                }
                bc_state.current_index += 1;

                // Get result shape
                let result_shape = bc_state.result_shape.clone().ok_or_else(|| {
                    VmError::InternalError("Broadcast2InPlace requires result_shape".to_string())
                })?;
                let element_count: usize = result_shape.iter().product();

                if bc_state.current_index < element_count {
                    // More elements to process
                    let a_shape = &bc_state.input_shape;
                    let b_shape = bc_state.input2_shape.as_ref().ok_or_else(|| {
                        VmError::InternalError(
                            "Broadcast2InPlace requires input2_shape".to_string(),
                        )
                    })?;

                    let (a_expanded, b_expanded) = expand_shapes_for_julia(a_shape, b_shape);
                    let result_strides = compute_strides(&result_shape);
                    let a_strides = compute_strides(&a_expanded);
                    let b_strides = compute_strides(&b_expanded);
                    let a_ndims_diff = result_shape.len() - a_expanded.len();
                    let b_ndims_diff = result_shape.len() - b_expanded.len();

                    let idx = bc_state.current_index;
                    let a_idx = broadcast_get_index(
                        idx,
                        &result_shape,
                        &result_strides,
                        &a_expanded,
                        &a_strides,
                        a_ndims_diff,
                    );
                    let b_idx = broadcast_get_index(
                        idx,
                        &result_shape,
                        &result_strides,
                        &b_expanded,
                        &b_strides,
                        b_ndims_diff,
                    );

                    let next_a = match &bc_state.input {
                        BroadcastInput::F64(data) => data[a_idx],
                        BroadcastInput::Values(vals) => match &vals[a_idx] {
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            _ => 0.0,
                        },
                    };
                    let input2_ref = bc_state.input2.as_ref().ok_or_else(|| {
                        VmError::InternalError("Broadcast2InPlace requires input2".to_string())
                    })?;
                    let next_b = match input2_ref {
                        BroadcastInput::F64(data) => data[b_idx],
                        BroadcastInput::Values(vals) => match &vals[b_idx] {
                            Value::F64(v) => *v,
                            Value::I64(v) => *v as f64,
                            _ => 0.0,
                        },
                    };

                    let func_index = bc_state.func_index;
                    self.call_function_with_two_args(func_index, next_a, next_b)?;
                } else {
                    // All elements processed - return dest array
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::Array(dest_ref));
                    self.ip = return_ip;
                }
            }

            // Note: HofOpKind::Foldr removed - foldr is now Pure Julia (base/iterators.jl)
            HofOpKind::MapReduce => {
                // MapReduce uses results field as a phase marker:
                // - Empty results: just returned from map
                // - Non-empty results: just returned from reduce
                if bc_state.results.is_empty() {
                    // Just returned from map function
                    if bc_state.accumulator.is_none() {
                        // First element mapped - use as initial accumulator
                        bc_state.accumulator = Some(Value::F64(result));
                        bc_state.current_index += 1;

                        if bc_state.current_index < element_count {
                            // Map next element
                            if let Some(Value::F64(next_val)) =
                                bc_state.input.get(bc_state.current_index)
                            {
                                let func_index = bc_state.func_index;
                                self.call_function_with_arg(func_index, next_val)?;
                            }
                        } else {
                            // Only one element - return the mapped result
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.broadcast_state = None;
                            self.stack.push(Value::F64(result));
                            self.ip = return_ip;
                        }
                    } else {
                        // Have accumulator - call reduce with (acc, mapped_value)
                        // Mark that we're entering reduce phase by pushing to results
                        bc_state.results.push_f64(1.0); // Marker value
                        let acc = match &bc_state.accumulator {
                            Some(Value::F64(v)) => *v,
                            _ => 0.0,
                        };
                        let reduce_func_index = bc_state.reduce_func_index.ok_or_else(|| {
                            VmError::InternalError(
                                "MapReduce requires reduce_func_index".to_string(),
                            )
                        })?;
                        self.call_function_with_two_args(reduce_func_index, acc, result)?;
                    }
                } else {
                    // Just returned from reduce function
                    bc_state.results.clear(); // Clear phase marker
                    bc_state.accumulator = Some(Value::F64(result)); // Update accumulator
                    bc_state.current_index += 1;

                    if bc_state.current_index < element_count {
                        // Map next element
                        if let Some(Value::F64(next_val)) =
                            bc_state.input.get(bc_state.current_index)
                        {
                            let func_index = bc_state.func_index;
                            self.call_function_with_arg(func_index, next_val)?;
                        }
                    } else {
                        // All done - return final accumulator
                        let return_ip = bc_state.return_ip_after_broadcast;
                        self.broadcast_state = None;
                        self.stack.push(Value::F64(result));
                        self.ip = return_ip;
                    }
                }
            }

            HofOpKind::MapFoldr => {
                // MapFoldr uses results field as a phase marker (same as MapReduce):
                // - Empty results: just returned from map
                // - Non-empty results: just returned from reduce
                // The difference is: reduce function args are swapped (mapped_value, acc)
                if bc_state.results.is_empty() {
                    // Just returned from map function
                    if bc_state.accumulator.is_none() {
                        // First element mapped - use as initial accumulator
                        bc_state.accumulator = Some(Value::F64(result));
                        bc_state.current_index += 1;

                        if bc_state.current_index < element_count {
                            // Map next element
                            if let Some(Value::F64(next_val)) =
                                bc_state.input.get(bc_state.current_index)
                            {
                                let func_index = bc_state.func_index;
                                self.call_function_with_arg(func_index, next_val)?;
                            }
                        } else {
                            // Only one element - return the mapped result
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.broadcast_state = None;
                            self.stack.push(Value::F64(result));
                            self.ip = return_ip;
                        }
                    } else {
                        // Have accumulator - call reduce with (mapped_value, acc) - SWAPPED!
                        // Mark that we're entering reduce phase by pushing to results
                        bc_state.results.push_f64(1.0); // Marker value
                        let acc = match &bc_state.accumulator {
                            Some(Value::F64(v)) => *v,
                            _ => 0.0,
                        };
                        let reduce_func_index = bc_state.reduce_func_index.ok_or_else(|| {
                            VmError::InternalError(
                                "MapFoldr requires reduce_func_index".to_string(),
                            )
                        })?;
                        // Note: arguments are swapped for right-associative fold
                        self.call_function_with_two_args(reduce_func_index, result, acc)?;
                    }
                } else {
                    // Just returned from reduce function
                    bc_state.results.clear(); // Clear phase marker
                    bc_state.accumulator = Some(Value::F64(result)); // Update accumulator
                    bc_state.current_index += 1;

                    if bc_state.current_index < element_count {
                        // Map next element
                        if let Some(Value::F64(next_val)) =
                            bc_state.input.get(bc_state.current_index)
                        {
                            let func_index = bc_state.func_index;
                            self.call_function_with_arg(func_index, next_val)?;
                        }
                    } else {
                        // All done - return final accumulator
                        let return_ip = bc_state.return_ip_after_broadcast;
                        self.broadcast_state = None;
                        self.stack.push(Value::F64(result));
                        self.ip = return_ip;
                    }
                }
            }

            HofOpKind::MapInPlace => {
                // Store result directly into dest array at current_index
                let dest_ref = bc_state.dest_array.clone().ok_or_else(|| {
                    VmError::InternalError("MapInPlace requires dest_array".to_string())
                })?;
                {
                    let mut dest_borrow = dest_ref.borrow_mut();
                    dest_borrow.try_data_f64_mut()?[current_idx] = result;
                }
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    // More elements to process
                    if let Some(Value::F64(next_val)) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        self.call_function_with_arg(func_index, next_val)?;
                    }
                } else {
                    // All elements processed - return dest array
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::Array(dest_ref));
                    self.ip = return_ip;
                }
            }

            HofOpKind::FilterInPlace => {
                // Keep track of elements to keep
                if result != 0.0 {
                    bc_state.results.push_value(input_val);
                }
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    // More elements to process
                    if let Some(Value::F64(next_val)) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        self.call_function_with_arg(func_index, next_val)?;
                    }
                } else {
                    // All elements processed - update array in-place
                    let kept_values = bc_state.results.take_values();
                    let new_len = kept_values.len();
                    let dest_ref = bc_state.dest_array.clone().ok_or_else(|| {
                        VmError::InternalError("FilterInPlace requires dest_array".to_string())
                    })?;
                    {
                        let mut dest_borrow = dest_ref.borrow_mut();
                        // Rebuild the data with only kept values
                        let new_data: Vec<f64> = kept_values
                            .iter()
                            .map(|v| match v {
                                Value::F64(f) => *f,
                                Value::I64(i) => *i as f64,
                                _ => 0.0,
                            })
                            .collect();
                        dest_borrow.data = ArrayData::F64(new_data);
                        dest_borrow.shape = vec![new_len];
                    }
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.broadcast_state = None;
                    self.stack.push(Value::Array(dest_ref));
                    self.ip = return_ip;
                }
            }

            HofOpKind::Ntuple => {
                // Ntuple always uses value mode, so this path should never be taken
                return Err(VmError::InternalError(
                    "Ntuple should use value mode".to_string(),
                ));
            }
            HofOpKind::TupleMap => {
                // TupleMap always uses value mode, so this path should never be taken
                return Err(VmError::InternalError(
                    "TupleMap should use value mode".to_string(),
                ));
            }
        }
        Ok(())
    }
}
