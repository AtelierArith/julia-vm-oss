//! HOF call entry points.

use crate::rng::RngLike;

use super::state::{
    BroadcastInput, BroadcastResults, BroadcastState, HofOpKind, RuntimeCallableResult,
};
use crate::vm::error::VmError;
use crate::vm::value::{TupleValue, Value};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    /// Start an ntuple call: apply f to 1..n and collect into tuple
    pub(in crate::vm) fn start_ntuple_call(
        &mut self,
        func_index: usize,
        input_data: Vec<Value>,
    ) -> Result<(), VmError> {
        let n = input_data.len();
        let first_val = input_data[0].clone();

        self.push_broadcast_state(BroadcastState {
            func_index,
            runtime_callable: None,
            input: BroadcastInput::Values(input_data),
            input_shape: vec![n],
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_values(n),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::Ntuple,
            accumulator: None,
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: true,
            wrap_array_result: false,
            reduce_func_index: None,
        });

        self.call_function_with_value(func_index, first_val)
    }

    /// Start an ntuple call with a runtime callable value.
    pub(in crate::vm) fn start_ntuple_runtime_call(
        &mut self,
        callable: Value,
        input_data: Vec<Value>,
    ) -> Result<(), VmError> {
        let n = input_data.len();
        if n == 0 {
            self.stack.push(Value::Tuple(TupleValue::new(vec![])));
            return Ok(());
        }

        let first_val = input_data[0].clone();
        self.push_broadcast_state(BroadcastState {
            func_index: 0,
            runtime_callable: Some(callable.clone()),
            input: BroadcastInput::Values(input_data),
            input_shape: vec![n],
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_values(n),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::Ntuple,
            accumulator: None,
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: true,
            wrap_array_result: false,
            reduce_func_index: None,
        });

        match self.call_runtime_callable_value(callable, vec![first_val])? {
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
    // Note: tuple-map call paths are now handled in Pure Julia (base/iterators.jl).
}
