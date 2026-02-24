//! HOF call entry points.

use crate::rng::RngLike;

use crate::vm::error::VmError;
use crate::vm::frame::{BroadcastInput, BroadcastResults, BroadcastState, HofOpKind};
use crate::vm::value::{ArrayRef, Value, ValueType};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    pub(in crate::vm) fn start_hof_call(
        &mut self,
        func_index: usize,
        arr: ArrayRef,
        op_kind: HofOpKind,
        _acc: Option<f64>,
    ) -> Result<(), VmError> {
        let arr_borrow = arr.borrow();
        let input_data = arr_borrow.try_as_f64_vec()?;
        let input_shape = arr_borrow.shape.clone();
        let first_val = input_data[0];
        let capacity = arr_borrow.len();
        drop(arr_borrow);

        // Get the return type of the function to determine the output array type
        let return_type = if func_index < self.functions.len() {
            self.functions[func_index].return_type.clone()
        } else {
            ValueType::F64 // Fallback to F64 if function not found
        };

        // For I64, Bool, and Any return types, use Value-based processing
        // I64/Bool to preserve exact types, Any to allow runtime type determination
        let use_value_mode = matches!(
            return_type,
            ValueType::I64 | ValueType::Bool | ValueType::Any
        );

        // Create BroadcastResults with the correct element type
        let results = match return_type {
            ValueType::I64 | ValueType::Bool | ValueType::Any => {
                BroadcastResults::new_values(capacity)
            }
            _ => BroadcastResults::new_f64(capacity), // Default to F64
        };

        self.broadcast_state = Some(BroadcastState {
            func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results,
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind,
            accumulator: None,
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1, // HOF function frame will be pushed
            is_value_mode: use_value_mode,
            reduce_func_index: None,
        });

        // Call the function with the appropriate method based on return type
        if use_value_mode {
            self.call_function_with_value(func_index, Value::F64(first_val))?;
        } else {
            self.call_function_with_arg(func_index, first_val)?;
        }
        Ok(())
    }

    // Note: start_reduce_call, start_foldr_call, start_reduce_call_values removed
    // - reduce/foldl/foldr are now Pure Julia (base/iterators.jl)

    /// Start a mapreduce call: apply map_func to each element, then reduce with reduce_func
    pub(in crate::vm) fn start_mapreduce_call(
        &mut self,
        map_func_index: usize,
        reduce_func_index: usize,
        arr: ArrayRef,
        init: Option<f64>,
    ) -> Result<(), VmError> {
        let arr_borrow = arr.borrow();
        let input_data = arr_borrow.try_as_f64_vec()?;
        let input_shape = arr_borrow.shape.clone();
        let first_val = input_data[0];
        drop(arr_borrow);

        self.broadcast_state = Some(BroadcastState {
            func_index: map_func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_f64(0),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::MapReduce,
            accumulator: init.map(Value::F64),
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: false,
            reduce_func_index: Some(reduce_func_index),
        });

        // Start by applying map function to the first element
        self.call_function_with_arg(map_func_index, first_val)
    }

    /// Start a mapfoldr call: apply map_func to each element (in reverse order), then reduce with reduce_func (swapped args)
    pub(in crate::vm) fn start_mapfoldr_call(
        &mut self,
        map_func_index: usize,
        reduce_func_index: usize,
        arr: ArrayRef,
        init: Option<f64>,
    ) -> Result<(), VmError> {
        let arr_borrow = arr.borrow();
        let input_data = arr_borrow.try_as_f64_vec()?;
        let input_shape = arr_borrow.shape.clone();
        let first_val = input_data[0];
        drop(arr_borrow);

        self.broadcast_state = Some(BroadcastState {
            func_index: map_func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_f64(0),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::MapFoldr,
            accumulator: init.map(Value::F64),
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: false,
            reduce_func_index: Some(reduce_func_index),
        });

        // Start by applying map function to the first element (which is the last in the original array)
        self.call_function_with_arg(map_func_index, first_val)
    }

    /// Start a map! call: apply func to each element of src and store in dest
    pub(in crate::vm) fn start_map_inplace_call(
        &mut self,
        func_index: usize,
        dest: ArrayRef,
        src: ArrayRef,
    ) -> Result<(), VmError> {
        let src_borrow = src.borrow();
        let input_data = src_borrow.try_as_f64_vec()?;
        let input_shape = src_borrow.shape.clone();
        let first_val = input_data[0];
        drop(src_borrow);

        self.broadcast_state = Some(BroadcastState {
            func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: Some(dest),
            results: BroadcastResults::new_f64(0),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::MapInPlace,
            accumulator: None,
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: false,
            reduce_func_index: None,
        });

        self.call_function_with_arg(func_index, first_val)
    }

    /// Start a filter! call: filter elements in-place
    pub(in crate::vm) fn start_filter_inplace_call(
        &mut self,
        func_index: usize,
        arr: ArrayRef,
    ) -> Result<(), VmError> {
        let arr_borrow = arr.borrow();
        let input_data = arr_borrow.try_as_f64_vec()?;
        let input_shape = arr_borrow.shape.clone();
        let first_val = input_data[0];
        drop(arr_borrow);

        self.broadcast_state = Some(BroadcastState {
            func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: Some(arr),
            results: BroadcastResults::new_values(0), // Will store indices of matching elements
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::FilterInPlace,
            accumulator: None,
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1,
            is_value_mode: false,
            reduce_func_index: None,
        });

        self.call_function_with_arg(func_index, first_val)
    }

    /// Start a sum call: apply f to each element and sum results
    pub(in crate::vm) fn start_sum_call(
        &mut self,
        func_index: usize,
        arr: ArrayRef,
    ) -> Result<(), VmError> {
        let arr_borrow = arr.borrow();
        let input_data = arr_borrow.try_as_f64_vec()?;
        let input_shape = arr_borrow.shape.clone();
        let first_val = input_data[0];
        drop(arr_borrow);

        self.broadcast_state = Some(BroadcastState {
            func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_f64(0),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind: HofOpKind::Sum,
            accumulator: Some(Value::F64(0.0)), // Start with sum = 0
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1, // HOF function frame will be pushed
            is_value_mode: false,
            reduce_func_index: None,
        });

        self.call_function_with_arg(func_index, first_val)
    }

    /// Start a HOF call with an accumulator (for count, etc.)
    pub(in crate::vm) fn start_hof_call_with_accumulator(
        &mut self,
        func_index: usize,
        arr: ArrayRef,
        op_kind: HofOpKind,
        init: f64,
    ) -> Result<(), VmError> {
        let arr_borrow = arr.borrow();
        let input_data = arr_borrow.try_as_f64_vec()?;
        let input_shape = arr_borrow.shape.clone();
        let first_val = input_data[0];
        drop(arr_borrow);

        self.broadcast_state = Some(BroadcastState {
            func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
            input2: None,
            input2_shape: None,
            result_shape: None,
            dest_array: None,
            results: BroadcastResults::new_f64(0),
            current_index: 0,
            return_ip_after_broadcast: self.ip,
            op_kind,
            accumulator: Some(Value::F64(init)),
            extra_args: Vec::new(),
            hof_frame_depth: self.frames.len() + 1, // HOF function frame will be pushed
            is_value_mode: false,
            reduce_func_index: None,
        });

        self.call_function_with_arg(func_index, first_val)
    }

    /// Start an ntuple call: apply f to 1..n and collect into tuple
    pub(in crate::vm) fn start_ntuple_call(
        &mut self,
        func_index: usize,
        arr: ArrayRef,
        n: usize,
    ) -> Result<(), VmError> {
        let arr_borrow = arr.borrow();
        let input_data = arr_borrow.try_as_f64_vec()?;
        let input_shape = arr_borrow.shape.clone();
        let first_val = input_data[0];
        drop(arr_borrow);

        self.broadcast_state = Some(BroadcastState {
            func_index,
            input: BroadcastInput::F64(input_data),
            input_shape,
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
            reduce_func_index: None,
        });

        self.call_function_with_arg(func_index, first_val)
    }
    // Note: tuple-map call paths are now handled in Pure Julia (base/iterators.jl).
}
