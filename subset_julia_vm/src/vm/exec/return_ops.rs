//! Return operations for the VM.
//!
//! This module handles return instructions:
//! - ReturnF64, ReturnI64: Return typed numeric values
//! - ReturnArray: Return array values
//! - ReturnAny: Return any value type
//! - ReturnNothing: Return nothing/unit
//! - ReturnRng, ReturnRange, ReturnRef: Return special types
//!
//! Note: ReturnTuple, ReturnNamedTuple, ReturnDict, ReturnStruct are handled
//! by their respective modules (tuple.rs, named_tuple.rs, dict.rs, struct_ops.rs).

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::util;
use super::super::value::Value;
use super::super::Vm;

/// Result of executing a return instruction.
pub(super) enum ReturnResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled, continue execution (internal return)
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
    /// Exit run loop with this value (final return)
    Exit(Value),
}

impl<R: RngLike> Vm<R> {
    /// Execute return instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_return(&mut self, instr: &Instr) -> Result<ReturnResult, VmError> {
        match instr {
            Instr::ReturnF64 => {
                let x = self.pop_f64_or_i64()?;

                // Check if we're in HOF/broadcast mode AND this is the HOF function returning
                // (not a nested function call within the HOF function body)
                let (is_hof_return, is_value_mode) = self
                    .broadcast_state
                    .as_ref()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if is_hof_return {
                    if is_value_mode {
                        self.handle_hof_return_value(Value::F64(x))?;
                    } else {
                        self.handle_hof_return(x)?;
                    }
                    Ok(ReturnResult::Handled)
                } else if self.handle_composed_call_return(Value::F64(x))? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(Value::F64(x));
                    Ok(ReturnResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(Value::F64(x)))
                }
            }

            Instr::ReturnF32 | Instr::ReturnF16 => {
                let val = self.stack.pop_value()?;

                let (is_hof_return, is_value_mode) = self
                    .broadcast_state
                    .as_ref()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if is_hof_return {
                    if is_value_mode {
                        self.handle_hof_return_value(val)?;
                    } else {
                        let f = match &val {
                            Value::F32(x) => *x as f64,
                            Value::F16(x) => f64::from(*x),
                            _ => 0.0,
                        };
                        self.handle_hof_return(f)?;
                    }
                    Ok(ReturnResult::Handled)
                } else if self.handle_composed_call_return(val.clone())? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(val);
                    Ok(ReturnResult::Handled)
                } else {
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(val))
                }
            }

            Instr::ReturnI64 => {
                // Pop the value, preserving narrow integer types (I8, I16, I32, etc.)
                // The compiler may emit ReturnI64 for functions that return narrow integers
                // because julia_type_to_value_type maps all integer types to ValueType::I64.
                // We preserve the original type to maintain correct runtime semantics.
                let val = self.stack.pop_value()?;
                let (x, preserved_val) = match &val {
                    Value::I64(v) => (*v, val),
                    Value::Bool(v) => (if *v { 1 } else { 0 }, val),
                    Value::I32(v) => (*v as i64, val),
                    Value::I16(v) => (*v as i64, val),
                    Value::I8(v) => (*v as i64, val),
                    Value::I128(v) => (*v as i64, val),
                    Value::U8(v) => (*v as i64, val),
                    Value::U16(v) => (*v as i64, val),
                    Value::U32(v) => (*v as i64, val),
                    Value::U64(v) => (*v as i64, val),
                    Value::U128(v) => (*v as i64, val),
                    // BigInt is a valid integer type — may reach ReturnI64 when a function
                    // compiled for Int64 is called with BigInt via runtime dispatch (Issue #2508)
                    Value::BigInt(v) => {
                        use num_traits::ToPrimitive;
                        (v.to_i64().unwrap_or(0), val)
                    }
                    _ => {
                        // INTERNAL: ReturnI64 is emitted only for integer-returning functions; wrong return type is a compiler bug
                        return Err(VmError::InternalError(format!(
                            "ReturnI64: expected integer, got {:?}",
                            val
                        )));
                    }
                };

                // Check if we're in HOF/broadcast mode AND this is the HOF function returning
                // (not a nested function call within the HOF function body)
                let (is_hof_return, is_value_mode) = self
                    .broadcast_state
                    .as_ref()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if is_hof_return {
                    if is_value_mode {
                        self.handle_hof_return_value(preserved_val)?;
                    } else {
                        self.handle_hof_return(x as f64)?;
                    }
                    Ok(ReturnResult::Handled)
                } else if self.handle_composed_call_return(preserved_val.clone())? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(preserved_val);
                    Ok(ReturnResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(preserved_val))
                }
            }

            Instr::ReturnArray => {
                // Return array value
                let val = self.stack.pop_value()?;
                match val {
                    // Memory is also a valid array-like return type (Issue #2764)
                    Value::Array(_) | Value::Memory(_) => {
                        if let Some(return_ip) = self.return_ips.pop() {
                            // Pop any exception handlers from try blocks in this function
                            self.pop_handlers_for_return();
                            self.frames.pop();
                            self.ip = return_ip;
                            self.stack.push(val);
                            Ok(ReturnResult::Handled)
                        } else {
                            // Final return - also pop handlers
                            self.pop_handlers_for_return();
                            Ok(ReturnResult::Exit(val))
                        }
                    }
                    other => {
                        let err = VmError::TypeError(format!(
                            "ReturnArray: expected Array or TypedArray, got {:?}",
                            util::value_type_name(&other)
                        ));
                        match self.try_or_handle::<()>(Err(err))? {
                            Some(_) => {
                                Err(VmError::InternalError(
                                    "ReturnArray error handling returned unexpected value"
                                        .to_string(),
                                ))
                            }
                            None => Ok(ReturnResult::Continue),
                        }
                    }
                }
            }

            Instr::ReturnAny => {
                // Dynamic return - pops and returns whatever is on stack
                let val = self.stack.pop_value()?;

                // Check if we're in HOF/broadcast mode AND this is the HOF function returning
                // (not a nested function call within the HOF function body)
                let (is_hof_return, is_value_mode) = self
                    .broadcast_state
                    .as_ref()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if is_hof_return {
                    if is_value_mode {
                        // Value mode: handle any value type
                        self.handle_hof_return_value(val)?;
                    } else {
                        // Legacy f64 mode
                        match &val {
                            Value::I64(x) => self.handle_hof_return(*x as f64)?,
                            Value::F64(x) => self.handle_hof_return(*x)?,
                            Value::Bool(b) => self.handle_hof_return(if *b { 1.0 } else { 0.0 })?,
                            _ => {} // Non-scalar values don't participate in legacy HOF
                        }
                    }
                    Ok(ReturnResult::Handled)
                } else if self.handle_sprint_return()? {
                    // Sprint function call just returned - string is already pushed
                    Ok(ReturnResult::Handled)
                } else if self.handle_composed_call_return(val.clone())? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(val);
                    Ok(ReturnResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(val))
                }
            }

            Instr::ReturnNothing => {
                // Check for sprint return first
                if self.handle_sprint_return()? {
                    // Sprint function call just returned - string is already pushed
                    return Ok(ReturnResult::Handled);
                }

                // Check if we're in HOF/broadcast mode AND this is the HOF function returning
                let (is_hof_return, is_value_mode) = self
                    .broadcast_state
                    .as_ref()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if is_hof_return {
                    if is_value_mode {
                        self.handle_hof_return_value(Value::Nothing)?;
                    } else {
                        // For f64 path, Nothing is treated as 0.0
                        self.handle_hof_return(0.0)?;
                    }
                    Ok(ReturnResult::Handled)
                } else if self.handle_composed_call_return(Value::Nothing)? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(Value::Nothing);
                    Ok(ReturnResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(Value::Nothing))
                }
            }

            Instr::ReturnRng => {
                let val = self.stack.pop().unwrap_or(Value::Nothing);
                if self.handle_composed_call_return(val.clone())? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(val);
                    Ok(ReturnResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(val))
                }
            }

            Instr::ReturnRange => {
                let val = self.stack.pop().unwrap_or(Value::Nothing);
                if self.handle_composed_call_return(val.clone())? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(val);
                    Ok(ReturnResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(val))
                }
            }

            Instr::ReturnRef => {
                let val = self.stack.pop().unwrap_or(Value::Nothing);
                if self.handle_composed_call_return(val.clone())? {
                    Ok(ReturnResult::Handled)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.frames.pop();
                    self.ip = return_ip;
                    self.stack.push(val);
                    Ok(ReturnResult::Handled)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(ReturnResult::Exit(val))
                }
            }

            _ => Ok(ReturnResult::NotHandled),
        }
    }

    /// Handle return from a composed function call.
    /// If we're in a composed call and the inner function just returned,
    /// call the next outer function with the result.
    /// Returns true if this was a composed call return that was handled.
    fn handle_composed_call_return(&mut self, result: Value) -> Result<bool, VmError> {
        // Check if we're in a composed call and at the right frame depth
        // Note: The inner function's frame is still on the stack, so we compare to call_frame_depth + 1
        let should_call_next = self
            .composed_call_state
            .as_ref()
            .map(|cs| self.frames.len() == cs.call_frame_depth + 1)
            .unwrap_or(false);

        if !should_call_next {
            return Ok(false);
        }

        // Take the composed call state mutably
        let mut state = self.composed_call_state.take().ok_or_else(|| {
            VmError::TypeError("Expected composed call state but found None".to_string())
        })?;

        // Pop the current function's frame and return IP
        self.return_ips.pop();
        self.frames.pop();

        // Pop the next function to call from the pending stack
        let next_func = state.pending_outers.pop().ok_or_else(|| {
            VmError::TypeError("Empty pending_outers in composed call".to_string())
        })?;

        // Check if there are more functions pending after this one
        let has_more_pending = !state.pending_outers.is_empty();

        // Resolve function name and optional captures from the callable value
        let (func_name, captures) = match &next_func {
            Value::Function(fv) => (fv.name.clone(), vec![]),
            Value::Closure(cv) => (cv.name.clone(), cv.captures.clone()),
            _ => {
                // INTERNAL: composed call pending_outers can only contain Function/Closure; other type is a compiler bug
                return Err(VmError::InternalError(format!(
                    "Expected Function or Closure in composed call, got {:?}",
                    next_func
                )));
            }
        };

        // Find function by name and call it
        let func_index = self
            .functions
            .iter()
            .position(|f| f.name == func_name)
            .ok_or_else(|| {
                VmError::TypeError(format!(
                    "Function '{}' not found in composed call",
                    func_name
                ))
            })?;

        let func = self.get_function_checked(func_index)?.clone();

        // Create frame with captures (empty vec for regular functions)
        let mut frame = super::super::frame::Frame::new_with_captures(
            func.local_slot_count,
            Some(func_index),
            captures,
        );

        // Bind result to first parameter slot
        if let Some(slot) = func.param_slots.first() {
            super::util::bind_value_to_slot(&mut frame, *slot, result, &mut self.struct_heap);
        }

        if has_more_pending {
            // More functions to call - restore state and continue
            self.composed_call_state = Some(state);
            self.return_ips.push(self.ip); // Will be handled by next composed return
        } else {
            // This is the last function - use original return_ip
            self.return_ips.push(state.return_ip);
        }

        self.frames.push(frame);
        self.ip = func.entry;
        Ok(true)
    }
}
