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

use super::DispatchAction;
use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::hof_exec::state::{
    GeneratorIterateKind, GeneratorIterateState, RuntimeCallableResult,
};
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{TupleValue, Value};
use super::super::Vm;

/// Result of routing a generic return value through the shared continuation
/// machinery (HOF value mode, generator-iterate continuations, composed calls,
/// and the normal caller-frame return). Used by the typed-return handlers
/// (`ReturnTuple`, `ReturnDict`, `ReturnNamedTuple`, ...) so a non-scalar value
/// returned from a `map`/`filter`/generator closure is collected correctly
/// instead of leaking past the HOF driver (Issue #5231).
pub(in crate::vm) enum ValueReturnRouting {
    /// Value was consumed by a continuation (HOF/generator/composed/normal
    /// return); keep running.
    Handled,
    /// No continuation matched and there is no caller frame: this is the final
    /// return value of the program.
    Exit(Value),
}

impl<R: RngLike> Vm<R> {
    pub(in crate::vm) fn handle_generator_iterate_return(
        &mut self,
        result: Value,
    ) -> Result<bool, VmError> {
        // Only the innermost (top-of-stack) pending continuation can match the
        // current frame depth: a generator's mapping function may itself iterate
        // another generator, so the continuations form a nested stack (Issue
        // #5229). We pop the top entry iff it belongs to the frame that is
        // returning right now.
        let should_handle = self
            .generator_iterate_state
            .last()
            .map(|state| self.frames.len() == state.call_frame_depth)
            .unwrap_or(false);
        if !should_handle {
            return Ok(false);
        }

        let state = self.generator_iterate_state.pop().ok_or_else(|| {
            VmError::InternalError(
                "generator iterate return state disappeared during handling".to_string(),
            )
        })?;

        match state.kind {
            GeneratorIterateKind::Map | GeneratorIterateKind::FilterMap => {
                self.return_ips.pop();
                self.pop_call_frame();
                self.ip = state.return_ip;
                self.stack.push(Value::Tuple(TupleValue {
                    elements: vec![result, state.next_state],
                }));
            }
            GeneratorIterateKind::FilterPredicate {
                map_func_index,
                predicate_func_index,
                iter,
                input_value,
            } => {
                self.return_ips.pop();
                self.pop_call_frame();
                let is_truthy = match &result {
                    Value::Bool(b) => *b,
                    Value::I64(v) => *v != 0,
                    Value::F64(v) => *v != 0.0,
                    Value::Nothing => false,
                    _ => true,
                };

                if is_truthy {
                    self.call_function_with_value(map_func_index, input_value)?;
                    self.generator_iterate_state.push(GeneratorIterateState {
                        next_state: state.next_state,
                        return_ip: state.return_ip,
                        call_frame_depth: self.frames.len(),
                        kind: GeneratorIterateKind::FilterMap,
                    });
                } else {
                    let next = self.iterate_next(&iter, &state.next_state)?;
                    let Value::Tuple(tuple) = next else {
                        self.ip = state.return_ip;
                        self.stack.push(next);
                        return Ok(true);
                    };
                    if tuple.elements.len() != 2 {
                        return Err(VmError::TypeError(format!(
                            "Generator iterate expected a 2-element tuple, got {} elements",
                            tuple.elements.len()
                        )));
                    }

                    let input_value = tuple.elements[0].clone();
                    let next_state = tuple.elements[1].clone();
                    self.call_function_with_value(predicate_func_index, input_value.clone())?;
                    self.generator_iterate_state.push(GeneratorIterateState {
                        next_state,
                        return_ip: state.return_ip,
                        call_frame_depth: self.frames.len(),
                        kind: GeneratorIterateKind::FilterPredicate {
                            map_func_index,
                            predicate_func_index,
                            iter,
                            input_value,
                        },
                    });
                }
            }
            GeneratorIterateKind::FilterPredicateRuntime {
                map,
                predicate,
                iter,
                input_value,
            } => {
                // Issue #9271: runtime-callable analogue of `FilterPredicate`.
                // The predicate frame just returned `result`; fold it (call the
                // map on match, else advance the base and re-run the predicate).
                self.return_ips.pop();
                self.pop_call_frame();
                self.resolve_runtime_filter_predicate(
                    map,
                    predicate,
                    iter,
                    input_value,
                    state.next_state,
                    state.return_ip,
                    result,
                )?;
            }
        }
        Ok(true)
    }

    /// Execute return instructions.
    /// Returns the execution result.
    // Hot dispatch handler: front-loaded in `dispatch_instr` (Issue #5175).
    #[inline(always)]
    pub(super) fn execute_return(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::ReturnF64 => {
                let x = self.pop_f64_or_i64()?;

                // Check if we're in HOF/broadcast mode AND this is the HOF function returning
                // (not a nested function call within the HOF function body)
                let (is_hof_return, is_value_mode) = self
                    .broadcast_state()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if self.handle_composed_call_return(Value::F64(x))? {
                    Ok(DispatchAction::Continue)
                } else if is_hof_return {
                    if is_value_mode {
                        self.handle_hof_return_value(Value::F64(x))?;
                    } else {
                        self.handle_hof_return(x)?;
                    }
                    Ok(DispatchAction::Continue)
                } else if self.handle_generator_iterate_return(Value::F64(x))? {
                    Ok(DispatchAction::Continue)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.pop_call_frame();
                    self.ip = return_ip;
                    self.stack.push(Value::F64(x));
                    Ok(DispatchAction::Continue)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(DispatchAction::Exit(Value::F64(x)))
                }
            }

            Instr::ReturnF32 | Instr::ReturnF16 => {
                let val = self.stack.pop_value()?;

                let (is_hof_return, is_value_mode) = self
                    .broadcast_state()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if self.handle_composed_call_return(val.clone())? {
                    Ok(DispatchAction::Continue)
                } else if is_hof_return {
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
                    Ok(DispatchAction::Continue)
                } else if self.handle_generator_iterate_return(val.clone())? {
                    Ok(DispatchAction::Continue)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    self.pop_handlers_for_return();
                    self.pop_call_frame();
                    self.ip = return_ip;
                    self.stack.push(val);
                    Ok(DispatchAction::Continue)
                } else {
                    self.pop_handlers_for_return();
                    Ok(DispatchAction::Exit(val))
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
                    Value::BigInt(v) => (v.to_i64().unwrap_or(0), val),
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
                    .broadcast_state()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if self.handle_composed_call_return(preserved_val.clone())? {
                    Ok(DispatchAction::Continue)
                } else if is_hof_return {
                    if is_value_mode {
                        self.handle_hof_return_value(preserved_val)?;
                    } else {
                        self.handle_hof_return(x as f64)?;
                    }
                    Ok(DispatchAction::Continue)
                } else if self.handle_generator_iterate_return(preserved_val.clone())? {
                    Ok(DispatchAction::Continue)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.pop_call_frame();
                    self.ip = return_ip;
                    self.stack.push(preserved_val);
                    Ok(DispatchAction::Continue)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(DispatchAction::Exit(preserved_val))
                }
            }

            Instr::ReturnArray => {
                // Return array value
                let val = self.stack.pop_value()?;
                match val {
                    // Memory is also a valid array-like return type (Issue #2764)
                    val if super::super::value::is_native_array_value(&val)
                        || matches!(val, Value::Memory(_)) =>
                    {
                        let (is_hof_return, is_value_mode) = self
                            .broadcast_state()
                            .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                            .unwrap_or((false, false));
                        if is_hof_return && is_value_mode {
                            self.handle_hof_return_value(val)?;
                            Ok(DispatchAction::Continue)
                        } else if self.handle_generator_iterate_return(val.clone())? {
                            Ok(DispatchAction::Continue)
                        } else if let Some(return_ip) = self.return_ips.pop() {
                            // Pop any exception handlers from try blocks in this function
                            self.pop_handlers_for_return();
                            self.pop_call_frame();
                            self.ip = return_ip;
                            self.stack.push(val);
                            Ok(DispatchAction::Continue)
                        } else {
                            // Final return - also pop handlers
                            self.pop_handlers_for_return();
                            Ok(DispatchAction::Exit(val))
                        }
                    }
                    Value::StructRef(idx)
                        if self.struct_heap.get(idx).is_some_and(|s| {
                            &*s.struct_name == "Array" || s.struct_name.starts_with("Array{")
                        }) =>
                    {
                        let (is_hof_return, is_value_mode) = self
                            .broadcast_state()
                            .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                            .unwrap_or((false, false));
                        if is_hof_return && is_value_mode {
                            self.handle_hof_return_value(Value::StructRef(idx))?;
                            Ok(DispatchAction::Continue)
                        } else if self.handle_generator_iterate_return(Value::StructRef(idx))? {
                            Ok(DispatchAction::Continue)
                        } else if let Some(return_ip) = self.return_ips.pop() {
                            self.pop_handlers_for_return();
                            self.pop_call_frame();
                            self.ip = return_ip;
                            self.stack.push(Value::StructRef(idx));
                            Ok(DispatchAction::Continue)
                        } else {
                            self.pop_handlers_for_return();
                            Ok(DispatchAction::Exit(Value::StructRef(idx)))
                        }
                    }
                    Value::Struct(s)
                        if &*s.struct_name == "Array" || s.struct_name.starts_with("Array{") =>
                    {
                        let (is_hof_return, is_value_mode) = self
                            .broadcast_state()
                            .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                            .unwrap_or((false, false));
                        if is_hof_return && is_value_mode {
                            self.handle_hof_return_value(Value::Struct(s))?;
                            Ok(DispatchAction::Continue)
                        } else if self.handle_generator_iterate_return(Value::Struct(s.clone()))? {
                            Ok(DispatchAction::Continue)
                        } else if let Some(return_ip) = self.return_ips.pop() {
                            self.pop_handlers_for_return();
                            self.pop_call_frame();
                            self.ip = return_ip;
                            self.stack.push(Value::Struct(s));
                            Ok(DispatchAction::Continue)
                        } else {
                            self.pop_handlers_for_return();
                            Ok(DispatchAction::Exit(Value::Struct(s)))
                        }
                    }
                    other => {
                        // `ReturnArray` is a compile-time return-type hint, not a
                        // runtime guarantee: a `map`/`filter`/generator closure
                        // inferred to return an `Array` may at runtime yield a
                        // non-array value (e.g. a Dict, Set, Tuple, or struct).
                        // Rather than hard-erroring, route the value through the
                        // shared continuation machinery so HOF/generator drivers
                        // collect it and ordinary returns propagate it, matching
                        // the dynamic `ReturnAny` path (Issue #5231).
                        match self.route_value_return(other)? {
                            ValueReturnRouting::Handled => Ok(DispatchAction::Continue),
                            ValueReturnRouting::Exit(v) => Ok(DispatchAction::Exit(v)),
                        }
                    }
                }
            }

            Instr::ReturnAny => {
                // Dynamic return - pops and returns whatever is on stack
                let val = self.stack.pop_value()?;
                match self.route_value_return(val)? {
                    ValueReturnRouting::Handled => Ok(DispatchAction::Continue),
                    ValueReturnRouting::Exit(v) => Ok(DispatchAction::Exit(v)),
                }
            }

            Instr::ReturnNothing => {
                // Check for sprint return first
                if self.handle_sprint_return()? {
                    // Sprint function call just returned - string is already pushed
                    return Ok(DispatchAction::Continue);
                }
                if self.handle_redirect_return(Value::Nothing)? {
                    return Ok(DispatchAction::Continue);
                }

                // Check if we're in HOF/broadcast mode AND this is the HOF function returning
                let (is_hof_return, is_value_mode) = self
                    .broadcast_state()
                    .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
                    .unwrap_or((false, false));

                if self.handle_composed_call_return(Value::Nothing)? {
                    Ok(DispatchAction::Continue)
                } else if is_hof_return {
                    if is_value_mode {
                        self.handle_hof_return_value(Value::Nothing)?;
                    } else {
                        // For f64 path, Nothing is treated as 0.0
                        self.handle_hof_return(0.0)?;
                    }
                    Ok(DispatchAction::Continue)
                } else if self.handle_generator_iterate_return(Value::Nothing)? {
                    Ok(DispatchAction::Continue)
                } else if let Some(return_ip) = self.return_ips.pop() {
                    // Pop any exception handlers from try blocks in this function
                    self.pop_handlers_for_return();
                    self.pop_call_frame();
                    self.ip = return_ip;
                    self.stack.push(Value::Nothing);
                    Ok(DispatchAction::Continue)
                } else {
                    // Final return - also pop handlers
                    self.pop_handlers_for_return();
                    Ok(DispatchAction::Exit(Value::Nothing))
                }
            }

            Instr::ReturnRng | Instr::ReturnRange | Instr::ReturnRef => {
                self.handle_special_value_return()
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    fn handle_special_value_return(&mut self) -> Result<DispatchAction, VmError> {
        let val = self.stack.pop().unwrap_or(Value::Nothing);
        if self.handle_generator_iterate_return(val.clone())?
            || self.handle_composed_call_return(val.clone())?
        {
            Ok(DispatchAction::Continue)
        } else if let Some(return_ip) = self.return_ips.pop() {
            // Pop any exception handlers from try blocks in this function
            self.pop_handlers_for_return();
            self.pop_call_frame();
            self.ip = return_ip;
            self.stack.push(val);
            Ok(DispatchAction::Continue)
        } else {
            // Final return - also pop handlers
            self.pop_handlers_for_return();
            Ok(DispatchAction::Exit(val))
        }
    }

    /// Route a generic return `val` through every continuation the dynamic
    /// `ReturnAny` path honours, in the same order:
    ///   1. composed-call (`∘`) chaining,
    ///   2. HOF/broadcast value mode (and the legacy f64 mode),
    ///   3. generator-iterate continuations (`map`/`filter` over generators),
    ///   4. `sprint` returns,
    ///   5. the normal caller-frame return,
    ///   6. otherwise the program's final return value.
    ///
    /// The typed-return handlers (`ReturnAny`, `ReturnArray` non-array fallback,
    /// `ReturnTuple`, `ReturnDict`, `ReturnNamedTuple`) all funnel through here
    /// so a non-scalar value returned from a HOF/generator closure is collected
    /// by the driver instead of leaking past it (Issue #5231).
    pub(in crate::vm) fn route_value_return(
        &mut self,
        val: Value,
    ) -> Result<ValueReturnRouting, VmError> {
        // Check if we're in HOF/broadcast mode AND this is the HOF function
        // returning (not a nested call within the HOF function body).
        let (is_hof_return, is_value_mode) = self
            .broadcast_state()
            .map(|bc| (self.frames.len() == bc.hof_frame_depth, bc.is_value_mode))
            .unwrap_or((false, false));

        if self.handle_composed_call_return(val.clone())? {
            Ok(ValueReturnRouting::Handled)
        } else if is_hof_return {
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
            Ok(ValueReturnRouting::Handled)
        } else if self.handle_generator_iterate_return(val.clone())? {
            Ok(ValueReturnRouting::Handled)
        } else if self.handle_sprint_return()? {
            // Sprint function call just returned - string is already pushed
            Ok(ValueReturnRouting::Handled)
        } else if self.handle_redirect_return(val.clone())? {
            Ok(ValueReturnRouting::Handled)
        } else if let Some(return_ip) = self.return_ips.pop() {
            // Pop any exception handlers from try blocks in this function
            self.pop_handlers_for_return();
            self.pop_call_frame();
            self.ip = return_ip;
            self.stack.push(val);
            Ok(ValueReturnRouting::Handled)
        } else {
            // Final return - also pop handlers
            self.pop_handlers_for_return();
            Ok(ValueReturnRouting::Exit(val))
        }
    }

    /// Handle return from a composed function call.
    /// If we're in a composed call and the inner function just returned,
    /// call the next outer function with the result.
    /// Returns true if this was a composed call return that was handled.
    pub(in crate::vm) fn handle_composed_call_return(
        &mut self,
        result: Value,
    ) -> Result<bool, VmError> {
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
        self.pop_call_frame();

        let mut result = result;
        loop {
            // Pop the next function to call from the pending stack.
            let next_func = state.pending_outers.pop().ok_or_else(|| {
                VmError::TypeError("Empty pending_outers in composed call".to_string())
            })?;

            let has_more_pending = !state.pending_outers.is_empty();

            if let Value::Function(fv) = &next_func {
                if fv.name == "!" {
                    result = match result {
                        Value::Bool(b) => Value::Bool(!b),
                        Value::Missing => Value::Missing,
                        other => return Err(VmError::type_error_expected("!", "Bool", &other)),
                    };

                    if has_more_pending {
                        continue;
                    }

                    let route_to_runtime_hof = self
                        .broadcast_state()
                        .map(|bc| {
                            bc.runtime_callable.is_some()
                                && bc.hof_frame_depth == state.call_frame_depth + 1
                        })
                        .unwrap_or(false);
                    if route_to_runtime_hof {
                        self.handle_runtime_hof_immediate_value(result)?;
                    } else {
                        self.ip = state.return_ip;
                        self.stack.push(result);
                    }
                    return Ok(true);
                }
            }

            // Use the ordinary runtime-callable path for every outer. It
            // preserves frozen source/helper provenance and routes dispatch
            // misses/ambiguities through Julia's catchable exception machinery
            // instead of collapsing them to an Option/TypeError (Issue #9784).
            if !has_more_pending {
                // `call_runtime_callable_value` records the current IP as the
                // callee return address. The final outer returns directly to the
                // original composed-call continuation.
                self.ip = state.return_ip;
            }
            match self.call_runtime_callable_value(next_func, vec![result])? {
                RuntimeCallableResult::Immediate(value) => {
                    result = value;
                    if has_more_pending {
                        continue;
                    }
                    let route_to_runtime_hof = self
                        .broadcast_state()
                        .map(|bc| {
                            bc.runtime_callable.is_some()
                                && bc.hof_frame_depth == state.call_frame_depth + 1
                        })
                        .unwrap_or(false);
                    if route_to_runtime_hof {
                        self.handle_runtime_hof_immediate_value(result)?;
                    } else {
                        self.stack.push(result);
                    }
                    return Ok(true);
                }
                RuntimeCallableResult::StartedFrame => {
                    if has_more_pending {
                        self.composed_call_state = Some(state);
                    }
                    return Ok(true);
                }
                RuntimeCallableResult::Raised => return Ok(true),
            }
        }
    }
}
