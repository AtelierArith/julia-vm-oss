//! Value-mode HOF execution helpers.

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::exec::bind_kwargs_defaults;
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

/// One step of the alternating `FilterMap` state machine (Issue #9271): call
/// the predicate, or (once a value passed the predicate) call the map body.
enum FilterMapCall {
    Predicate(Value),
    Map(Value),
}

/// Resolved callable for a `FilterMap` step: either a runtime callable `Value`
/// (function/closure, possibly capturing) or a bare function-table index.
enum FilterMapTarget {
    Runtime(Value),
    Index(usize),
}

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
            runtime_reduce_callable: None,
            result_element_type: None,
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
            let element_type = result_element_type
                .clone()
                .unwrap_or_else(|| ArrayElementType::UnionOf(Vec::new()));
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
            runtime_reduce_callable: None,
            result_element_type,
        });

        self.call_function_with_value(predicate_func_index, first_val)
    }

    /// Runtime-callable analogue of
    /// [`Self::start_hof_filter_map_values_with_array_result`] (Issue #9271):
    /// the predicate and map are callable `Value`s (function/closure that may
    /// capture) rather than function-table indices. Drives the same alternating
    /// predicate → (if truthy) map state machine, but issues each call through
    /// `call_runtime_callable_value` so captured environments are honored.
    pub(crate) fn start_hof_filter_map_runtime_values_with_array_result(
        &mut self,
        predicate: Value,
        map: Value,
        values: Vec<Value>,
        result_element_type: Option<ArrayElementType>,
        wrap_array_result: bool,
    ) -> Result<(), VmError> {
        // The compiler only supplies an empty-result eltype when the filtered
        // generator predicate is transparent enough to use that hint. Runtime
        // callables may still capture a local function value (for example a
        // lifted generator body inside `let` / `@testset`), but that alone does
        // not invalidate the already-computed map result type.
        let empty_result_element_type = result_element_type;
        if values.is_empty() {
            let element_type =
                empty_result_element_type.unwrap_or_else(|| ArrayElementType::UnionOf(Vec::new()));
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
            func_index: 0,
            runtime_callable: Some(predicate),
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
            reduce_func_index: None,
            runtime_reduce_callable: Some(map),
            result_element_type: empty_result_element_type,
        });

        self.filter_map_drive(FilterMapCall::Predicate(first_val))
    }

    /// Issue one `FilterMap` step (predicate or map) against the current
    /// broadcast state, using the runtime callable when present (Issue #9271)
    /// and falling back to the function-table index otherwise. Returns whether
    /// a frame was started (drive returns to the interpreter loop) or the call
    /// produced an immediate value / raised.
    fn filter_map_call(
        &mut self,
        is_map: bool,
        value: Value,
    ) -> Result<RuntimeCallableResult, VmError> {
        let target = {
            let bc = self.broadcast_states.last().ok_or_else(|| {
                VmError::InternalError("filter_map_call without broadcast_state".to_string())
            })?;
            if is_map {
                match bc.runtime_reduce_callable.clone() {
                    Some(map) => FilterMapTarget::Runtime(map),
                    None => FilterMapTarget::Index(bc.reduce_func_index.ok_or_else(|| {
                        VmError::InternalError("FilterMap missing map function index".to_string())
                    })?),
                }
            } else {
                match bc.runtime_callable.clone() {
                    Some(pred) => FilterMapTarget::Runtime(pred),
                    None => FilterMapTarget::Index(bc.func_index),
                }
            }
        };
        match target {
            FilterMapTarget::Runtime(callable) => {
                self.call_runtime_callable_value(callable, vec![value])
            }
            FilterMapTarget::Index(func_index) => {
                self.call_function_with_value(func_index, value)?;
                Ok(RuntimeCallableResult::StartedFrame)
            }
        }
    }

    /// Drive the `FilterMap` state machine forward from a pending call. Loops
    /// over immediately-returning callables; returns as soon as a call starts a
    /// frame (the frame's return re-enters via `handle_hof_return_value`) or the
    /// machine finalizes its result array.
    fn filter_map_drive(&mut self, mut call: FilterMapCall) -> Result<(), VmError> {
        loop {
            let (is_map, value) = match call {
                FilterMapCall::Predicate(v) => (false, v),
                FilterMapCall::Map(v) => (true, v),
            };
            match self.filter_map_call(is_map, value)? {
                RuntimeCallableResult::StartedFrame => return Ok(()),
                RuntimeCallableResult::Raised => {
                    self.clear_broadcast_state();
                    return Ok(());
                }
                RuntimeCallableResult::Immediate(result) => match self.filter_map_fold(result)? {
                    Some(next) => call = next,
                    None => return Ok(()),
                },
            }
        }
    }

    /// Fold one `FilterMap` return value into the broadcast state and decide the
    /// next call. `None` means the machine finalized (result array pushed, `ip`
    /// restored, state cleared). Shared by the frame-return path
    /// (`handle_hof_return_value`) and the immediate-value driver
    /// (`filter_map_drive`).
    fn filter_map_fold(&mut self, result: Value) -> Result<Option<FilterMapCall>, VmError> {
        enum Step {
            Call(FilterMapCall),
            Finalize,
        }
        let step = {
            let bc = self.broadcast_states.last_mut().ok_or_else(|| {
                VmError::InternalError("filter_map_fold without broadcast_state".to_string())
            })?;
            let element_count: usize = bc.input_shape.iter().product();
            if bc.accumulator.take().is_some() {
                // Just returned from the map body: record the mapped value.
                bc.results.push_value(result);
                bc.current_index += 1;
                let next_index = bc.current_index;
                if next_index < element_count {
                    let next_val = bc.input.get(next_index).ok_or_else(|| {
                        VmError::InternalError("FilterMap missing next input value".to_string())
                    })?;
                    Step::Call(FilterMapCall::Predicate(next_val))
                } else {
                    Step::Finalize
                }
            } else {
                // Just returned from the predicate.
                let is_truthy = match &result {
                    Value::Bool(b) => *b,
                    Value::I64(v) => *v != 0,
                    Value::F64(v) => *v != 0.0,
                    Value::Nothing => false,
                    _ => true,
                };
                if is_truthy {
                    let current_idx = bc.current_index;
                    let input_val = bc.input.get(current_idx).ok_or_else(|| {
                        VmError::InternalError("FilterMap missing current input value".to_string())
                    })?;
                    // Mark the map phase by parking the input value in the
                    // accumulator, then map it.
                    bc.accumulator = Some(input_val.clone());
                    Step::Call(FilterMapCall::Map(input_val))
                } else {
                    bc.current_index += 1;
                    let next_index = bc.current_index;
                    if next_index < element_count {
                        let next_val = bc.input.get(next_index).ok_or_else(|| {
                            VmError::InternalError("FilterMap missing next input value".to_string())
                        })?;
                        Step::Call(FilterMapCall::Predicate(next_val))
                    } else {
                        Step::Finalize
                    }
                }
            }
        };
        match step {
            Step::Call(call) => Ok(Some(call)),
            Step::Finalize => {
                self.filter_map_finalize()?;
                Ok(None)
            }
        }
    }

    /// Finalize a `FilterMap`: build the collected result array, restore `ip`,
    /// and clear the broadcast state. For all-filtered-out generators the
    /// compiler supplies an eltype only when the predicate is statically
    /// transparent; otherwise the empty result falls back to `Union{}[]`.
    fn filter_map_finalize(&mut self) -> Result<(), VmError> {
        let (
            mut result_values,
            result_element_type,
            return_ip,
            wrap_array_result,
            predicate_uses_inlined_call,
        ) = {
            let bc = self.broadcast_states.last_mut().ok_or_else(|| {
                VmError::InternalError("filter_map_finalize without broadcast_state".to_string())
            })?;
            let predicate_uses_inlined_call =
                self.functions.get(bc.func_index).is_some_and(|func| {
                    func.slot_names
                        .iter()
                        .any(|name| name.starts_with("__sjulia_inline_arg_"))
                });
            (
                bc.results.take_values(),
                bc.result_element_type.clone(),
                bc.return_ip_after_broadcast,
                bc.wrap_array_result,
                predicate_uses_inlined_call,
            )
        };
        self.clear_broadcast_state();
        let result = if result_values.is_empty() {
            let element_type = if predicate_uses_inlined_call {
                ArrayElementType::UnionOf(Vec::new())
            } else {
                result_element_type.unwrap_or_else(|| ArrayElementType::UnionOf(Vec::new()))
            };
            let arr = ArrayValue::memory_first_with_capacity(element_type, 0);
            if wrap_array_result {
                self.array_wrapper_value(arr)?
            } else {
                array_value(arr)
            }
        } else {
            let result_shape = vec![result_values.len()];
            self.create_value_mode_result_array(
                std::mem::take(&mut result_values),
                result_shape,
                wrap_array_result,
            )?
        };
        self.stack.push(result);
        self.ip = return_ip;
        Ok(())
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
            runtime_reduce_callable: None,
            result_element_type: None,
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

        self.bind_type_params(&func, &args, &mut frame);

        if let Some(&slot) = param_slots.first() {
            bind_value_to_slot(&mut frame, slot, arg, &mut self.struct_heap);
        }

        for (i, extra_arg) in extra_args.iter().enumerate() {
            if let Some(&slot) = param_slots.get(i + 1) {
                bind_value_to_slot(&mut frame, slot, extra_arg.clone(), &mut self.struct_heap);
            }
        }

        bind_kwargs_defaults(
            &func,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        let (target_entry, slot_count) = self
            .try_specialized_entry_for_runtime_call(func_index, &args)
            .unwrap_or((func.entry, func.local_slot_count));
        if slot_count > frame.locals_slots.len() {
            frame.locals_slots.resize(slot_count, None);
        }
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

        self.bind_type_params(&func, &args, &mut frame);

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

        bind_kwargs_defaults(
            &func,
            &mut frame,
            &mut self.struct_heap,
            &self.code,
            &self.functions,
            self.frames.first(),
            &self.global_slot_map,
        )?;

        let (target_entry, slot_count) = self
            .try_specialized_entry_for_runtime_call(func_index, &args)
            .unwrap_or((func.entry, func.local_slot_count));
        if slot_count > frame.locals_slots.len() {
            frame.locals_slots.resize(slot_count, None);
        }
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

        // Pop the current frame
        if let Some(frame) = self.frames.pop() {
            self.stack.truncate(frame.stack_base);
        }
        self.return_ips.pop();

        match op_kind {
            HofOpKind::Broadcast | HofOpKind::BroadcastTupleSplat => {
                // Fold + advance loop (Issues #9693/#8797). A function-index
                // element call whose callee predecodes to a frame-less typed
                // scalar function block executes immediately — no frame, no
                // per-element specialization probe — so this loop drives all
                // remaining elements without leaving Rust. Anything else
                // (runtime callables, tuple splats, block bails) takes the
                // original per-element frame path and exits the loop; the
                // frame's return re-enters here.
                let mut result = result;
                loop {
                    enum Advance {
                        Call(Value, usize, Option<Value>, Vec<Value>),
                        Finalize(Vec<Value>, Vec<usize>, usize, bool),
                    }
                    let advance = {
                        let bc_state = self.broadcast_states.last_mut().ok_or_else(|| {
                            VmError::InternalError(
                                "hof broadcast fold without broadcast_state".to_string(),
                            )
                        })?;
                        bc_state.results.push_value(result);
                        bc_state.current_index += 1;
                        if bc_state.current_index < element_count {
                            let next_val =
                                bc_state.input.get(bc_state.current_index).ok_or_else(|| {
                                    VmError::InternalError(
                                        "hof broadcast missing next input value".to_string(),
                                    )
                                })?;
                            Advance::Call(
                                next_val,
                                bc_state.func_index,
                                bc_state.runtime_callable.clone(),
                                bc_state.extra_args.clone(),
                            )
                        } else {
                            Advance::Finalize(
                                bc_state.results.take_values(),
                                bc_state.input_shape.clone(),
                                bc_state.return_ip_after_broadcast,
                                bc_state.wrap_array_result,
                            )
                        }
                    };
                    match advance {
                        Advance::Call(next_val, func_index, runtime_callable, extra_args) => {
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
                                return Ok(());
                            }
                            if op_kind == HofOpKind::BroadcastTupleSplat {
                                self.call_function_with_tuple_splat(func_index, next_val)?;
                                return Ok(());
                            }
                            // Frame-less typed scalar function block attempt
                            // (Issue #9693): on a hit, fold the value and keep
                            // driving in this loop.
                            if let Some((entry, end)) = self
                                .functions
                                .get(func_index)
                                .map(|f| (f.entry, f.code_end))
                            {
                                if let Some(value) = self.try_run_typed_scalar_function_with_args(
                                    func_index,
                                    entry,
                                    end,
                                    &next_val,
                                    &extra_args,
                                ) {
                                    result = value;
                                    continue;
                                }
                            }
                            if extra_args.is_empty() {
                                self.call_function_with_value(func_index, next_val)?;
                            } else {
                                self.call_function_with_value_and_extra_args(
                                    func_index,
                                    next_val,
                                    &extra_args,
                                )?;
                            }
                            return Ok(());
                        }
                        Advance::Finalize(
                            result_values,
                            result_shape,
                            return_ip,
                            wrap_array_result,
                        ) => {
                            self.clear_broadcast_state();
                            let result_array = self.create_value_mode_result_array(
                                result_values,
                                result_shape,
                                wrap_array_result,
                            )?;
                            self.stack.push(result_array);
                            self.ip = return_ip;
                            return Ok(());
                        }
                    }
                }
            }

            HofOpKind::FilterMap => {
                // Issue #9127 / #9271: the alternating predicate → (if truthy)
                // map state machine is shared by both the function-index and the
                // runtime-callable FilterMap. `filter_map_fold` records this
                // return and decides the next call (or finalizes); the frame is
                // already popped above, so no `bc_state` field access remains in
                // this arm (the helpers re-borrow `broadcast_states`).
                if let Some(next) = self.filter_map_fold(result)? {
                    self.filter_map_drive(next)?;
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

    /// When every value is an `Array{T,N}` wrapper `StructRef` sharing one
    /// concrete parametric type (`Vector{Int64}`, `Matrix{Float64}`, ...),
    /// return that type as the outer array's element tag (boxed `Abstract`
    /// storage). Returns `None` for non-array-wrapper structs (plain user
    /// structs), heterogeneous element types, or a non-concrete result — the
    /// caller then keeps the generic `StructOf` / `Any` behavior (Issues
    /// #10187, #10272).
    fn value_mode_nested_array_element_type(&self, values: &[Value]) -> Option<ArrayElementType> {
        let mut joined: Option<crate::types::JuliaType> = None;
        for value in values {
            let Value::StructRef(idx) = value else {
                return None;
            };
            let instance = self.struct_heap.get(*idx)?;
            let jt = self.array_wrapper_julia_type_resolved(instance)?;
            joined = Some(match joined {
                None => jt,
                Some(prev) if prev == jt => prev,
                Some(_) => return None,
            });
        }
        let jt = joined?;
        if !jt.is_concrete() {
            return None;
        }
        Some(ArrayElementType::Abstract(jt.name().into_owned()))
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
            // A vector-of-arrays result (`map(Vector, ::Vector{Vector{Int64}})`,
            // `collect(Vector(x) for x in xs)`, `map(x -> [x], xs)`, ...): every
            // element is itself an `Array{T,N}` wrapper `StructRef`. Its concrete
            // parametric type (`Vector{Int64}`) must become the OUTER element tag
            // instead of the generic `Array` struct id, which renders as
            // `Array{Any, Any}` and loses the precise `Vector{Vector{Int64}}`
            // outer type (Issues #10187, #10272). Falls back to the user-struct
            // `StructOf` path below when the elements are not array wrappers or
            // are heterogeneous.
            if let Some(nested_elem) = self.value_mode_nested_array_element_type(&values) {
                let mut arr = ArrayValue::memory_first_with_capacity(nested_elem, values.len());
                for value in values {
                    arr.push(value)?;
                }
                arr.shape = shape;
                return self.array_value_to_wrapper(arr);
            }
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
