//! Value-mode HOF execution helpers.

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::frame::Frame;
use super::super::types::FunctionInfo;
use super::super::util::bind_value_to_slot;
use super::super::value::{
    native_array_value_from_array as array_value, native_array_value_ref, ArrayElementType,
    ArrayValue, TupleValue, Value,
};
use super::super::Vm;
use super::state::{
    BroadcastInput, BroadcastResults, BroadcastState, HofOpKind, RuntimeCallableResult,
};

impl<R: RngLike> Vm<R> {
    /// Start a HOF call with value-based input (for struct arrays)
    pub(crate) fn start_hof_call_values_with_array_result(
        &mut self,
        func_index: usize,
        values: Vec<Value>,
        shape: Vec<usize>,
        op_kind: HofOpKind,
        wrap_array_result: bool,
    ) -> Result<(), VmError> {
        if values.is_empty() {
            // Empty array - return empty result immediately
            match op_kind {
                // Note: Map, Filter removed - now Pure Julia (base/iterators.jl)
                HofOpKind::Broadcast | HofOpKind::BroadcastTupleSplat => {
                    let result =
                        self.create_value_mode_result_array(Vec::new(), shape, wrap_array_result)?;
                    self.stack.push(result);
                }
                HofOpKind::FindAll => {
                    // Empty array returns empty Int64 array
                    self.stack
                        .push(array_value(ArrayValue::memory_first_from_i64(
                            vec![],
                            vec![0],
                        )));
                }
                _ => {
                    self.stack.push(Value::Nothing);
                }
            }
            return Ok(());
        }

        let first_val = values[0].clone();
        let capacity = values.len();

        self.push_broadcast_state(BroadcastState {
            func_index,
            runtime_callable: None,
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
            wrap_array_result,
            reduce_func_index: None,
        });

        if matches!(op_kind, HofOpKind::BroadcastTupleSplat) {
            self.call_function_with_tuple_splat(func_index, first_val)
        } else {
            self.call_function_with_value(func_index, first_val)
        }
    }

    pub(crate) fn start_hof_filter_map_values_with_array_result(
        &mut self,
        predicate_func_index: usize,
        map_func_index: usize,
        values: Vec<Value>,
        result_element_type: Option<ArrayElementType>,
        wrap_array_result: bool,
    ) -> Result<(), VmError> {
        if values.is_empty() {
            let element_type =
                result_element_type.unwrap_or_else(|| ArrayElementType::UnionOf(Vec::new()));
            let arr = ArrayValue::memory_first_with_capacity(element_type, 0);
            let result = if wrap_array_result {
                self.array_wrapper_value(arr)?
            } else {
                array_value(arr)
            };
            self.stack.push(result);
            return Ok(());
        }

        let first_val = values[0].clone();
        let value_count = values.len();
        self.push_broadcast_state(BroadcastState {
            func_index: predicate_func_index,
            runtime_callable: None,
            input: BroadcastInput::Values(values),
            input_shape: vec![value_count],
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_values(value_count),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::FilterMap,
            accumulator: None,
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: true,
            wrap_array_result,
            reduce_func_index: Some(map_func_index),
        });

        self.call_function_with_value(predicate_func_index, first_val)
    }

    pub(crate) fn start_hof_runtime_call_values_with_array_result(
        &mut self,
        callable: Value,
        values: Vec<Value>,
        shape: Vec<usize>,
        op_kind: HofOpKind,
        wrap_array_result: bool,
    ) -> Result<(), VmError> {
        if values.is_empty() {
            let mut arr = ArrayValue::memory_first_with_capacity(ArrayElementType::Any, 0);
            arr.shape = shape;
            let result = if wrap_array_result {
                self.array_wrapper_value(arr)?
            } else {
                array_value(arr)
            };
            self.stack.push(result);
            return Ok(());
        }

        let first_val = values[0].clone();
        let capacity = values.len();
        self.push_broadcast_state(BroadcastState {
            func_index: 0,
            runtime_callable: Some(callable.clone()),
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
            wrap_array_result,
            reduce_func_index: None,
        });

        let args = if matches!(op_kind, HofOpKind::BroadcastTupleSplat) {
            match first_val {
                Value::Tuple(tuple) => tuple.elements,
                other => {
                    return Err(VmError::TypeError(format!(
                        "Generator vararg callable expected tuple input, got {:?}",
                        other.runtime_type()
                    )))
                }
            }
        } else {
            vec![first_val]
        };
        match self.call_runtime_callable_value(callable, args)? {
            RuntimeCallableResult::StartedFrame => Ok(()),
            RuntimeCallableResult::Immediate(result) => {
                self.handle_runtime_hof_immediate_value(result)
            }
            RuntimeCallableResult::Raised => {
                self.clear_broadcast_state();
                Ok(())
            }
        }
    }

    /// Call a function with any Value argument (for struct array HOF)
    pub(crate) fn call_function_with_value(
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
        let func = self.get_function_checked(func_index)?.clone();
        let local_slot_count = func.local_slot_count;
        let param_slots: Vec<usize> = func.param_slots.clone();
        let args = std::iter::once(arg.clone())
            .chain(extra_args.iter().cloned())
            .collect::<Vec<_>>();

        let mut frame = self.acquire_frame(local_slot_count, Some(func_index));

        if let Some(&slot) = param_slots.first() {
            bind_value_to_slot(&mut frame, slot, arg, &mut self.struct_heap);
        }

        for (i, extra_arg) in extra_args.iter().enumerate() {
            if let Some(&slot) = param_slots.get(i + 1) {
                bind_value_to_slot(&mut frame, slot, extra_arg.clone(), &mut self.struct_heap);
            }
        }

        let target_entry = self
            .try_specialized_entry_for_runtime_call(func_index, &args)
            .unwrap_or(func.entry);
        self.push_hof_call_frame_with_generated(func_index, &func, &args, frame, target_entry)
    }

    pub(crate) fn call_function_with_tuple_splat(
        &mut self,
        func_index: usize,
        arg: Value,
    ) -> Result<(), VmError> {
        let args = match arg {
            Value::Tuple(tuple) => tuple.elements,
            other => {
                return Err(VmError::TypeError(format!(
                    "Generator vararg callable expected tuple input, got {:?}",
                    other.runtime_type()
                )));
            }
        };

        let func = self.get_function_checked(func_index)?.clone();
        let mut frame = self.acquire_frame(func.local_slot_count, Some(func_index));

        if let Some(vararg_idx) = func.vararg_param_index {
            for idx in 0..vararg_idx {
                if let (Some(val), Some(slot)) = (args.get(idx), func.param_slots.get(idx)) {
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
            let vararg_values = args[vararg_idx..].to_vec();
            let vararg_tuple = Value::Tuple(TupleValue {
                elements: vararg_values,
            });
            if let Some(slot) = func.param_slots.get(vararg_idx) {
                bind_value_to_slot(&mut frame, *slot, vararg_tuple, &mut self.struct_heap);
            }
        } else {
            for (idx, slot) in func.param_slots.iter().enumerate() {
                if let Some(val) = args.get(idx) {
                    bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
                }
            }
        }

        let target_entry = self
            .try_specialized_entry_for_runtime_call(func_index, &args)
            .unwrap_or(func.entry);
        self.push_hof_call_frame_with_generated(func_index, &func, &args, frame, target_entry)
    }

    pub(in crate::vm) fn push_hof_call_frame_with_generated(
        &mut self,
        func_index: usize,
        func: &FunctionInfo,
        args: &[Value],
        mut frame: Frame,
        target_entry: usize,
    ) -> Result<(), VmError> {
        let generated_eval_frame = func.is_generated.then(|| frame.clone());
        self.bind_generated_body_arg_types(func, args, &mut frame);
        self.return_ips.push(self.ip);
        self.try_push_call_frame(frame)?;
        self.remember_current_generated_expr_cache_key(
            func,
            func_index,
            args,
            generated_eval_frame,
        );
        self.ip = target_entry;
        Ok(())
    }

    /// Handle return value from HOF function call in value mode
    /// Called when is_value_mode is true
    pub(crate) fn handle_hof_return_value(&mut self, result: Value) -> Result<(), VmError> {
        // Direct field access keeps the borrow scoped to `broadcast_states` so
        // the body can still mutate the disjoint frame/stack fields (Issue #5229).
        let bc_state = self.broadcast_states.last_mut().ok_or_else(|| {
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
        if let Some(frame) = self.frames.pop() {
            self.stack.truncate(frame.stack_base);
        }
        self.return_ips.pop();

        match op_kind {
            HofOpKind::Broadcast | HofOpKind::BroadcastTupleSplat => {
                // Collect result into results array
                bc_state.results.push_value(result);
                bc_state.current_index += 1;

                if bc_state.current_index < element_count {
                    // More elements to process
                    if let Some(next_val) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        let runtime_callable = bc_state.runtime_callable.clone();
                        let extra_args = bc_state.extra_args.clone();
                        if let Some(callable) = runtime_callable {
                            let args = if op_kind == HofOpKind::BroadcastTupleSplat {
                                match next_val {
                                    Value::Tuple(tuple) => tuple.elements,
                                    other => {
                                        return Err(VmError::TypeError(format!(
                                        "Generator vararg callable expected tuple input, got {:?}",
                                        other.runtime_type()
                                    )))
                                    }
                                }
                            } else {
                                vec![next_val]
                            };
                            match self.call_runtime_callable_value(callable, args)? {
                                RuntimeCallableResult::StartedFrame => {}
                                RuntimeCallableResult::Raised => {
                                    self.clear_broadcast_state();
                                    return Ok(());
                                }
                                RuntimeCallableResult::Immediate(value) => {
                                    self.handle_runtime_hof_immediate_value(value)?;
                                }
                            }
                        } else if op_kind == HofOpKind::BroadcastTupleSplat {
                            self.call_function_with_tuple_splat(func_index, next_val)?;
                        } else if extra_args.is_empty() {
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
                    let wrap_array_result = bc_state.wrap_array_result;
                    self.clear_broadcast_state();

                    // Create appropriate result array based on result types
                    let result_array = self.create_value_mode_result_array(
                        result_values,
                        result_shape,
                        wrap_array_result,
                    )?;
                    self.stack.push(result_array);
                    self.ip = return_ip;
                }
            }

            HofOpKind::FilterMap => {
                if bc_state.accumulator.take().is_some() {
                    bc_state.results.push_value(result);
                    bc_state.current_index += 1;
                    let next_index = bc_state.current_index;
                    let predicate_func_index = bc_state.func_index;

                    if next_index < element_count {
                        if let Some(next_val) = bc_state.input.get(next_index) {
                            self.call_function_with_value(predicate_func_index, next_val)?;
                        }
                    } else {
                        let mut result_values = bc_state.results.take_values();
                        if result_values.is_empty() {
                            let return_ip = bc_state.return_ip_after_broadcast;
                            let wrap_array_result = bc_state.wrap_array_result;
                            self.clear_broadcast_state();
                            let arr = ArrayValue::memory_first_with_capacity(
                                ArrayElementType::UnionOf(Vec::new()),
                                0,
                            );
                            let result = if wrap_array_result {
                                self.array_wrapper_value(arr)?
                            } else {
                                array_value(arr)
                            };
                            self.stack.push(result);
                            self.ip = return_ip;
                        } else {
                            let result_shape = vec![result_values.len()];
                            let return_ip = bc_state.return_ip_after_broadcast;
                            let wrap_array_result = bc_state.wrap_array_result;
                            self.clear_broadcast_state();
                            let result_array = self.create_value_mode_result_array(
                                std::mem::take(&mut result_values),
                                result_shape,
                                wrap_array_result,
                            )?;
                            self.stack.push(result_array);
                            self.ip = return_ip;
                        }
                    }
                } else {
                    let is_truthy = match &result {
                        Value::Bool(b) => *b,
                        Value::I64(v) => *v != 0,
                        Value::F64(v) => *v != 0.0,
                        Value::Nothing => false,
                        _ => true,
                    };
                    if is_truthy {
                        let map_func_index = bc_state.reduce_func_index.ok_or_else(|| {
                            VmError::InternalError(
                                "FilterMap missing map function index".to_string(),
                            )
                        })?;
                        let input_val = bc_state.input.get(current_idx).ok_or_else(|| {
                            VmError::InternalError(
                                "FilterMap missing current input value".to_string(),
                            )
                        })?;
                        bc_state.accumulator = Some(input_val.clone());
                        self.call_function_with_value(map_func_index, input_val)?;
                    } else {
                        bc_state.current_index += 1;
                        let next_index = bc_state.current_index;
                        let predicate_func_index = bc_state.func_index;
                        if next_index < element_count {
                            if let Some(next_val) = bc_state.input.get(next_index) {
                                self.call_function_with_value(predicate_func_index, next_val)?;
                            }
                        } else {
                            let mut result_values = bc_state.results.take_values();
                            let return_ip = bc_state.return_ip_after_broadcast;
                            let wrap_array_result = bc_state.wrap_array_result;
                            self.clear_broadcast_state();
                            if result_values.is_empty() {
                                let arr = ArrayValue::memory_first_with_capacity(
                                    ArrayElementType::UnionOf(Vec::new()),
                                    0,
                                );
                                let result = if wrap_array_result {
                                    self.array_wrapper_value(arr)?
                                } else {
                                    array_value(arr)
                                };
                                self.stack.push(result);
                            } else {
                                let result_shape = vec![result_values.len()];
                                let result_array = self.create_value_mode_result_array(
                                    std::mem::take(&mut result_values),
                                    result_shape,
                                    wrap_array_result,
                                )?;
                                self.stack.push(result_array);
                            }
                            self.ip = return_ip;
                        }
                    }
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
                    self.clear_broadcast_state();
                    self.stack
                        .push(array_value(ArrayValue::memory_first_from_i64(
                            result_data,
                            vec![len],
                        )));
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
                    self.clear_broadcast_state();
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
                        self.clear_broadcast_state();
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
                    self.clear_broadcast_state();
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
                    if let Some(next_idx) = bc_state.input.get(bc_state.current_index) {
                        let func_index = bc_state.func_index;
                        if let Some(callable) = bc_state.runtime_callable.clone() {
                            match self.call_runtime_callable_value(callable, vec![next_idx])? {
                                RuntimeCallableResult::StartedFrame => {}
                                RuntimeCallableResult::Raised => {
                                    self.clear_broadcast_state();
                                    return Ok(());
                                }
                                RuntimeCallableResult::Immediate(result) => {
                                    self.handle_runtime_hof_immediate_value(result)?;
                                }
                            }
                        } else {
                            self.call_function_with_value(func_index, next_idx)?;
                        }
                    }
                } else {
                    // All elements processed - create tuple from results
                    let result_values = bc_state.results.take_values();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    self.clear_broadcast_state();
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
                    self.clear_broadcast_state();
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
                    Value::Bool(true) => 1.0,
                    Value::Bool(false) => 0.0,
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
                            self.clear_broadcast_state();
                            self.stack.push(Value::F64(final_sum));
                            self.ip = return_ip;
                        }
                    }
                    HofOpKind::Any => {
                        // Short-circuit: if result is truthy, we're done
                        if result_f64 != 0.0 {
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.clear_broadcast_state();
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
                                self.clear_broadcast_state();
                                self.stack.push(Value::Bool(false));
                                self.ip = return_ip;
                            }
                        }
                    }
                    HofOpKind::All => {
                        // Short-circuit: if result is falsy, we're done
                        if result_f64 == 0.0 {
                            let return_ip = bc_state.return_ip_after_broadcast;
                            self.clear_broadcast_state();
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
                                self.clear_broadcast_state();
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
                            self.clear_broadcast_state();
                            self.stack.push(Value::I64(final_count));
                            self.ip = return_ip;
                        }
                    }
                    // Note: ForEach removed - foreach is now Pure Julia (base/abstractarray.jl)
                    _ => {
                        // Default: just finish
                        let return_ip = bc_state.return_ip_after_broadcast;
                        self.clear_broadcast_state();
                        self.stack.push(result);
                        self.ip = return_ip;
                    }
                }
            }
        }
        Ok(())
    }

    /// Create a TypedArray from a vector of Values
    pub(crate) fn create_typed_array_from_values(
        &mut self,
        values: Vec<Value>,
        shape: Vec<usize>,
    ) -> Result<Value, VmError> {
        if values.is_empty() {
            let mut arr = ArrayValue::memory_first_with_capacity(ArrayElementType::Any, 0);
            arr.shape = shape;
            return self.array_value_to_wrapper(arr);
        }

        let mut values = values;
        for v in values.iter_mut() {
            if let Value::Struct(s) = v {
                let idx = self.struct_heap.len();
                self.struct_heap.push(s.clone());
                *v = Value::StructRef(idx);
            }
        }

        // A nested array element (e.g. the inner result of
        // `map(x -> map(...), v)`) stays a Memory-first `Array{T,N}` *wrapper*
        // `StructRef`. The display/index consumers resolve it deep
        // (`resolve_struct_refs_for_format`) and the typeinfo-prefix formatter
        // treats array-wrapper elements identically to the native carrier
        // (Issue #6882), so it no longer needs to be materialized to a native
        // carrier to avoid the #5229 leak. The value-mode result is therefore a
        // clean `Vector` of wrapper elements (Issue #6807).

        let all_struct_ref = values.iter().all(|v| matches!(v, Value::StructRef(_)));

        if all_struct_ref {
            let struct_type_id = values.iter().find_map(|v| {
                if let Value::StructRef(idx) = v {
                    self.struct_heap.get(*idx).map(|s| s.type_id)
                } else {
                    None
                }
            });
            let mut arr = ArrayValue::memory_first_with_capacity(
                struct_type_id
                    .map(ArrayElementType::StructOf)
                    .unwrap_or(ArrayElementType::Any),
                values.len(),
            );
            for value in values {
                arr.push(value)?;
            }
            arr.shape = shape;
            arr.struct_type_id = struct_type_id;
            self.array_value_to_wrapper(arr)
        } else {
            let mut arr =
                ArrayValue::memory_first_collect_typejoin_values(values, ArrayElementType::Any)?;
            arr.shape = shape;
            self.array_value_to_wrapper(arr)
        }
    }

    fn create_value_mode_result_array(
        &mut self,
        values: Vec<Value>,
        shape: Vec<usize>,
        wrap_array_result: bool,
    ) -> Result<Value, VmError> {
        let result = self.create_typed_array_from_values(values, shape)?;
        if !wrap_array_result {
            return Ok(result);
        }
        let Some(arr_ref) = native_array_value_ref(&result) else {
            return Ok(result);
        };
        let arr = {
            let borrowed = arr_ref.borrow();
            borrowed.clone()
        };
        self.array_wrapper_value(arr)
    }

    pub(in crate::vm) fn handle_runtime_hof_immediate_value(
        &mut self,
        mut result: Value,
    ) -> Result<(), VmError> {
        loop {
            let (
                next_call,
                return_ip,
                completed_values,
                completed_shape,
                completed_op_kind,
                completed_wrap_array_result,
            ) = {
                let bc_state = self.broadcast_states.last_mut().ok_or_else(|| {
                    VmError::InternalError(
                        "handle_runtime_hof_immediate_value called without broadcast_state"
                            .to_string(),
                    )
                })?;
                bc_state.results.push_value(result);
                bc_state.current_index += 1;

                let element_count: usize = bc_state.input_shape.iter().product();
                if bc_state.current_index >= element_count {
                    let values = bc_state.results.take_values();
                    let shape = bc_state.input_shape.clone();
                    let return_ip = bc_state.return_ip_after_broadcast;
                    let op_kind = bc_state.op_kind;
                    let wrap_array_result = bc_state.wrap_array_result;
                    (
                        None,
                        return_ip,
                        Some(values),
                        Some(shape),
                        Some(op_kind),
                        wrap_array_result,
                    )
                } else {
                    let callable = bc_state.runtime_callable.clone().ok_or_else(|| {
                        VmError::InternalError(
                            "runtime HOF immediate path missing callable".to_string(),
                        )
                    })?;
                    let op_kind = bc_state.op_kind;
                    let next_val = bc_state.input.get(bc_state.current_index).ok_or_else(|| {
                        VmError::InternalError(
                            "runtime HOF immediate path missing input".to_string(),
                        )
                    })?;
                    let args = if op_kind == HofOpKind::BroadcastTupleSplat {
                        match next_val {
                            Value::Tuple(tuple) => tuple.elements,
                            other => {
                                return Err(VmError::TypeError(format!(
                                    "Generator vararg callable expected tuple input, got {:?}",
                                    other.runtime_type()
                                )))
                            }
                        }
                    } else {
                        vec![next_val]
                    };
                    (Some((callable, args)), 0, None, None, None, false)
                }
            };

            if let (Some(values), Some(shape)) = (completed_values, completed_shape) {
                self.clear_broadcast_state();
                if completed_op_kind == Some(HofOpKind::Ntuple) {
                    self.stack.push(Value::Tuple(TupleValue::new(values)));
                } else {
                    let result_array = self.create_value_mode_result_array(
                        values,
                        shape,
                        completed_wrap_array_result,
                    )?;
                    self.stack.push(result_array);
                }
                self.ip = return_ip;
                return Ok(());
            }

            let Some((callable, args)) = next_call else {
                return Ok(());
            };
            match self.call_runtime_callable_value(callable, args)? {
                RuntimeCallableResult::StartedFrame => return Ok(()),
                RuntimeCallableResult::Immediate(next_result) => {
                    result = next_result;
                }
                RuntimeCallableResult::Raised => {
                    self.clear_broadcast_state();
                    return Ok(());
                }
            }
        }
    }
}
