//! Sprint function support.

use crate::rng::RngLike;

use crate::vm::error::VmError;
use crate::vm::frame::{Frame, SprintState};
use crate::vm::util::bind_value_to_slot;
use crate::vm::value::{IORef, Value};
use crate::vm::Vm;

impl<R: RngLike> Vm<R> {
    // ============ Sprint function support ============

    /// Start a sprint call: create IOBuffer, call f(io, args...), return string
    pub(in crate::vm) fn start_sprint_call(
        &mut self,
        func_index: usize,
        io: IORef,
        args: Vec<Value>,
    ) -> Result<(), VmError> {
        // Save the sprint state
        self.sprint_state = Some(SprintState {
            io: io.clone(),
            return_ip: self.ip,
            call_frame_depth: self.frames.len(),
        });

        // Get the function info
        let func = self.get_function_checked(func_index)?;
        let local_slot_count = func.local_slot_count;
        let param_slots: Vec<usize> = func.param_slots.clone();
        let entry = func.entry;

        // Create a new frame for the function
        let mut frame = Frame::new_with_slots(local_slot_count, Some(func_index));

        // Bind the IOBuffer as the first parameter (same IORef for interior mutability)
        if let Some(&slot) = param_slots.first() {
            let io_val = Value::IO(io.clone());
            bind_value_to_slot(&mut frame, slot, io_val, &mut self.struct_heap);
        }

        // Bind remaining arguments
        for (i, arg) in args.iter().enumerate() {
            if let Some(&slot) = param_slots.get(i + 1) {
                bind_value_to_slot(&mut frame, slot, arg.clone(), &mut self.struct_heap);
            }
        }

        // Push return IP and frame
        self.return_ips.push(self.ip);
        self.frames.push(frame);
        self.ip = entry;
        Ok(())
    }

    /// Handle return from sprint function call.
    /// Extracts the string from the IOBuffer and pushes it onto the stack.
    /// Returns Ok(true) if this was a sprint return that was handled.
    pub(in crate::vm) fn handle_sprint_return(&mut self) -> Result<bool, VmError> {
        // Check if we're in a sprint call and at the right frame depth
        let should_finish = self
            .sprint_state
            .as_ref()
            .map(|ss| self.frames.len() == ss.call_frame_depth + 1)
            .unwrap_or(false);

        if !should_finish {
            return Ok(false);
        }

        // Take the sprint state
        let state = self.sprint_state.take().ok_or_else(|| {
            VmError::TypeError("Expected sprint state but found None".to_string())
        })?;

        // Pop the function's frame and return IP
        self.frames.pop();
        self.return_ips.pop();

        // Extract the string from the IOBuffer (using interior mutability)
        let result = state.io.borrow().buffer.clone();
        self.stack.push(Value::Str(result));
        self.ip = state.return_ip;

        Ok(true)
    }
}
