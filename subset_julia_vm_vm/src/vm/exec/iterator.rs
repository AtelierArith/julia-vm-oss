//! Iterator operations for the VM.
//!
//! This module handles iteration instructions:
//! - IterateFirst: Get first element and state from collection
//! - IterateNext: Get next element given state

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::hof_exec::state::{
    GeneratorIterateKind, GeneratorIterateState, RuntimeCallableResult,
};
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{GeneratorCallable, GeneratorValue, Value};
use super::super::Vm;
use super::DispatchAction;

impl<R: RngLike> Vm<R> {
    pub(in crate::vm) fn start_lazy_generator_iterate_call(
        &mut self,
        generator: &GeneratorValue,
        state: Option<&Value>,
    ) -> Result<bool, VmError> {
        let (callable, iter) =
            self.fuse_lazy_generator_iter(generator.callable.clone(), generator.iter.as_ref());
        let generator = GeneratorValue::with_result_element_type(
            callable,
            iter,
            generator.result_element_type.clone(),
        );

        if let GeneratorCallable::FilteredFunctionIndex {
            map_func_index,
            predicate_func_index,
        } = generator.callable
        {
            let next = if let Some(state) = state {
                self.iterate_next(generator.iter.as_ref(), state)?
            } else {
                self.iterate_first(generator.iter.as_ref())?
            };
            let Value::Tuple(tuple) = next else {
                self.stack.push(next);
                return Ok(true);
            };
            if tuple.elements.len() != 2 {
                // User-visible: custom iterate for a filtered generator returned the wrong tuple arity.
                return Err(VmError::TypeError(format!(
                    "Generator iterate expected a 2-element tuple, got {} elements",
                    tuple.elements.len()
                )));
            }

            let input_value = tuple.elements[0].clone();
            let next_state = tuple.elements[1].clone();
            let return_ip = self.ip;
            self.call_function_with_value(predicate_func_index, input_value.clone())?;
            self.generator_iterate_state.push(GeneratorIterateState {
                next_state,
                return_ip,
                call_frame_depth: self.frames.len(),
                kind: GeneratorIterateKind::FilterPredicate {
                    map_func_index,
                    predicate_func_index,
                    iter: generator.iter.as_ref().clone(),
                    input_value,
                },
            });
            return Ok(true);
        }

        if let GeneratorCallable::FilteredRuntimeValue { map, predicate } = &generator.callable {
            // Issue #9271: runtime-callable analogue of the FilteredFunctionIndex
            // arm above. Iterate the base lazily, then drive predicate → (if
            // truthy) map via `call_runtime_callable_value` so a function-scope /
            // capturing lifted `__gen_body_N` / `__gen_pred_N` iterates lazily.
            let map = map.as_ref().clone();
            let predicate = predicate.as_ref().clone();
            let iter = generator.iter.as_ref().clone();
            let next = if let Some(state) = state {
                self.iterate_next(&iter, state)?
            } else {
                self.iterate_first(&iter)?
            };
            let Value::Tuple(tuple) = next else {
                self.stack.push(next);
                return Ok(true);
            };
            if tuple.elements.len() != 2 {
                // User-visible: custom iterate for a runtime-callable filtered generator returned the wrong tuple arity.
                return Err(VmError::TypeError(format!(
                    "Generator iterate expected a 2-element tuple, got {} elements",
                    tuple.elements.len()
                )));
            }
            let input_value = tuple.elements[0].clone();
            let next_state = tuple.elements[1].clone();
            let return_ip = self.ip;
            match self.call_runtime_callable_value(predicate.clone(), vec![input_value.clone()])? {
                RuntimeCallableResult::StartedFrame => {
                    self.generator_iterate_state.push(GeneratorIterateState {
                        next_state,
                        return_ip,
                        call_frame_depth: self.frames.len(),
                        kind: GeneratorIterateKind::FilterPredicateRuntime {
                            map,
                            predicate,
                            iter,
                            input_value,
                        },
                    });
                }
                RuntimeCallableResult::Immediate(pred_result) => {
                    return self.resolve_runtime_filter_predicate(
                        map,
                        predicate,
                        iter,
                        input_value,
                        next_state,
                        return_ip,
                        pred_result,
                    );
                }
                RuntimeCallableResult::Raised => {}
            }
            return Ok(true);
        }

        let (func_index, tuple_splat) = match generator.callable {
            GeneratorCallable::FunctionIndex(func_index) => (func_index, false),
            GeneratorCallable::TupleSplatFunctionIndex(func_index) => (func_index, true),
            GeneratorCallable::RuntimeValue(ref callable)
            | GeneratorCallable::TupleSplatRuntimeValue(ref callable) => {
                let tuple_splat = matches!(
                    generator.callable,
                    GeneratorCallable::TupleSplatRuntimeValue(_)
                );
                let next = if let Some(state) = state {
                    self.iterate_next(generator.iter.as_ref(), state)?
                } else {
                    self.iterate_first(generator.iter.as_ref())?
                };
                let Value::Tuple(tuple) = next else {
                    self.stack.push(next);
                    return Ok(true);
                };
                if tuple.elements.len() != 2 {
                    // User-visible: custom iterate for a generator returned the wrong tuple arity.
                    return Err(VmError::TypeError(format!(
                        "Generator iterate expected a 2-element tuple, got {} elements",
                        tuple.elements.len()
                    )));
                }
                let mapped_arg = tuple.elements[0].clone();
                let next_state = tuple.elements[1].clone();
                let args = if tuple_splat {
                    match mapped_arg {
                        Value::Tuple(tuple) => tuple.elements,
                        other => {
                            // User-visible: a splatting generator body can only splat tuple inputs.
                            return Err(VmError::TypeError(format!(
                                "Generator vararg callable expected tuple input, got {:?}",
                                other.runtime_type()
                            )));
                        }
                    }
                } else {
                    vec![mapped_arg]
                };
                let return_ip = self.ip;
                match self.call_runtime_callable_value(callable.as_ref().clone(), args)? {
                    RuntimeCallableResult::Immediate(value) => {
                        self.stack
                            .push(Value::Tuple(super::super::value::TupleValue {
                                elements: vec![value, next_state],
                            }));
                    }
                    RuntimeCallableResult::StartedFrame => {
                        self.generator_iterate_state.push(GeneratorIterateState {
                            next_state,
                            return_ip,
                            call_frame_depth: self.frames.len(),
                            kind: GeneratorIterateKind::Map,
                        });
                    }
                    RuntimeCallableResult::Raised => {}
                }
                return Ok(true);
            }
            _ => return Ok(false),
        };

        let next = if let Some(state) = state {
            self.iterate_next(generator.iter.as_ref(), state)?
        } else {
            self.iterate_first(generator.iter.as_ref())?
        };

        let Value::Tuple(tuple) = next else {
            self.stack.push(next);
            return Ok(true);
        };
        if tuple.elements.len() != 2 {
            // User-visible: custom iterate for a generator returned the wrong tuple arity.
            return Err(VmError::TypeError(format!(
                "Generator iterate expected a 2-element tuple, got {} elements",
                tuple.elements.len()
            )));
        }

        let mapped_arg = tuple.elements[0].clone();
        let next_state = tuple.elements[1].clone();
        let return_ip = self.ip;
        if tuple_splat {
            self.call_function_with_tuple_splat(func_index, mapped_arg)?;
        } else {
            self.call_function_with_value(func_index, mapped_arg)?;
        }
        self.generator_iterate_state.push(GeneratorIterateState {
            next_state,
            return_ip,
            call_frame_depth: self.frames.len(),
            kind: GeneratorIterateKind::Map,
        });
        Ok(true)
    }

    /// Continue a runtime-callable filtered generator step once the predicate's
    /// result is known (Issue #9271). If truthy, call the map on `input_value`;
    /// otherwise advance the base iterator and re-run the predicate. Loops over
    /// immediately-returning callables; when a call starts a frame it parks a
    /// continuation (`FilterPredicateRuntime` / `FilterMap`) and returns so the
    /// frame's return re-enters via `handle_generator_iterate_return`.
    pub(in crate::vm) fn resolve_runtime_filter_predicate(
        &mut self,
        map: Value,
        predicate: Value,
        iter: Value,
        mut input_value: Value,
        mut next_state: Value,
        return_ip: usize,
        mut pred_result: Value,
    ) -> Result<bool, VmError> {
        loop {
            let is_truthy = match &pred_result {
                Value::Bool(b) => *b,
                Value::I64(v) => *v != 0,
                Value::F64(v) => *v != 0.0,
                Value::Nothing => false,
                _ => true,
            };
            if is_truthy {
                match self.call_runtime_callable_value(map.clone(), vec![input_value.clone()])? {
                    RuntimeCallableResult::StartedFrame => {
                        self.generator_iterate_state.push(GeneratorIterateState {
                            next_state,
                            return_ip,
                            call_frame_depth: self.frames.len(),
                            kind: GeneratorIterateKind::FilterMap,
                        });
                        return Ok(true);
                    }
                    RuntimeCallableResult::Immediate(mapped) => {
                        self.ip = return_ip;
                        self.stack
                            .push(Value::Tuple(super::super::value::TupleValue {
                                elements: vec![mapped, next_state],
                            }));
                        return Ok(true);
                    }
                    RuntimeCallableResult::Raised => return Ok(true),
                }
            } else {
                let next = self.iterate_next(&iter, &next_state)?;
                let Value::Tuple(tuple) = next else {
                    self.ip = return_ip;
                    self.stack.push(next);
                    return Ok(true);
                };
                if tuple.elements.len() != 2 {
                    // User-visible: resumed custom iterate for a generator returned the wrong tuple arity.
                    return Err(VmError::TypeError(format!(
                        "Generator iterate expected a 2-element tuple, got {} elements",
                        tuple.elements.len()
                    )));
                }
                input_value = tuple.elements[0].clone();
                next_state = tuple.elements[1].clone();
                match self
                    .call_runtime_callable_value(predicate.clone(), vec![input_value.clone()])?
                {
                    RuntimeCallableResult::StartedFrame => {
                        self.generator_iterate_state.push(GeneratorIterateState {
                            next_state,
                            return_ip,
                            call_frame_depth: self.frames.len(),
                            kind: GeneratorIterateKind::FilterPredicateRuntime {
                                map,
                                predicate,
                                iter,
                                input_value,
                            },
                        });
                        return Ok(true);
                    }
                    RuntimeCallableResult::Immediate(v) => {
                        pred_result = v;
                        continue;
                    }
                    RuntimeCallableResult::Raised => return Ok(true),
                }
            }
        }
    }

    /// Execute iterator instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_iterator(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::IterateFirst => {
                let coll = self.stack.pop_value()?;
                if let Value::Generator(generator) = &coll {
                    if self.start_lazy_generator_iterate_call(generator, None)? {
                        return Ok(DispatchAction::Continue);
                    }
                }
                let result = self.iterate_first(&coll)?;
                self.stack.push(result);
                Ok(DispatchAction::Continue)
            }

            Instr::IterateNext => {
                let state = self.stack.pop_value()?;
                let coll = self.stack.pop_value()?;
                if let Value::Generator(generator) = &coll {
                    if self.start_lazy_generator_iterate_call(generator, Some(&state))? {
                        return Ok(DispatchAction::Continue);
                    }
                }
                let result = self.iterate_next(&coll, &state)?;
                self.stack.push(result);
                Ok(DispatchAction::Continue)
            }

            // Issue #5168: tuple-free ForEach. Pop the iterable; on builtin
            // fast-path collections push `[state, element]` plus a `Bool(true)`
            // flag (or just `Bool(false)` when exhausted) instead of allocating a
            // `(element, state)` tuple. Generators and any non-fast collection
            // fall back to the tuple-returning `iterate_first` and the resulting
            // tuple/Nothing is unpacked onto the stack.
            Instr::IterateFirstSplit => {
                let coll = self.stack.pop_value()?;
                if let Value::Generator(_) = &coll {
                    // Generators have their own (possibly frame-suspending) protocol;
                    // route through IterateFirst's logic and split the tuple result.
                    if let Value::Generator(generator) = &coll {
                        if self.start_lazy_generator_iterate_call(generator, None)? {
                            self.split_pending_iterate_tuple()?;
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                match self.iterate_first_fast(&coll)? {
                    Some(next) => self.push_split_iterate(next),
                    None => {
                        let result = self.iterate_first(&coll)?;
                        self.push_split_from_tuple(result)?;
                    }
                }
                Ok(DispatchAction::Continue)
            }

            Instr::IterateNextSplit => {
                let state = self.stack.pop_value()?;
                let coll = self.stack.pop_value()?;
                if let Value::Generator(generator) = &coll {
                    if self.start_lazy_generator_iterate_call(generator, Some(&state))? {
                        self.split_pending_iterate_tuple()?;
                        return Ok(DispatchAction::Continue);
                    }
                }
                match self.iterate_next_fast(&coll, &state)? {
                    Some(next) => self.push_split_iterate(next),
                    None => {
                        let result = self.iterate_next(&coll, &state)?;
                        self.push_split_from_tuple(result)?;
                    }
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }

    /// Push the result of a tuple-free fast-path iterate onto the stack using the
    /// split layout consumed by the ForEach lowering (Issue #5168):
    ///   - `Some((element, state))` → push `state`, push `element`, push `Bool(true)`
    ///   - `None`                   → push `Bool(false)`
    #[inline]
    fn push_split_iterate(&mut self, next: Option<(Value, Value)>) {
        match next {
            Some((element, state)) => {
                self.stack.push(state);
                self.stack.push(element);
                self.stack.push(Value::Bool(true));
            }
            None => {
                self.stack.push(Value::Bool(false));
            }
        }
    }

    /// Convert a tuple-returning `iterate_first`/`iterate_next` result into the
    /// split stack layout. `Value::Nothing` means exhausted; a 2-element tuple
    /// carries `(element, state)`.
    fn push_split_from_tuple(&mut self, result: Value) -> Result<(), VmError> {
        match result {
            Value::Nothing => {
                self.stack.push(Value::Bool(false));
                Ok(())
            }
            Value::Tuple(tuple) => {
                if tuple.elements.len() != 2 {
                    // User-visible: iterate methods must return nothing or a 2-tuple.
                    return Err(VmError::TypeError(format!(
                        "iterate expected a 2-element (element, state) tuple, got {} elements",
                        tuple.elements.len()
                    )));
                }
                let mut it = tuple.elements.into_iter();
                let element = it.next().ok_or_else(|| {
                    VmError::TypeError("iterate result missing element".to_string())
                })?;
                let state = it.next().ok_or_else(|| {
                    VmError::TypeError("iterate result missing state".to_string())
                })?;
                self.stack.push(state);
                self.stack.push(element);
                self.stack.push(Value::Bool(true));
                Ok(())
            }
            other => Err(VmError::TypeError(format!(
                "iterate expected a 2-element tuple or nothing, got {:?}",
                other.runtime_type()
            ))),
        }
    }

    /// A generator iterate call may have pushed its `(element, state)` tuple (or
    /// `Nothing`) onto the stack synchronously. Pop it and re-push in the split
    /// layout so the ForEach lowering sees a uniform shape. If the call instead
    /// suspended into a frame (no synchronous result), leave the stack alone — the
    /// resuming logic in the generator machinery pushes the tuple, which is
    /// consumed by the original tuple-based IterateFirst/IterateNext path only;
    /// the split path is never emitted for generator-typed iterables, so this is
    /// a defensive no-op for that case.
    fn split_pending_iterate_tuple(&mut self) -> Result<(), VmError> {
        if let Some(top) = self.stack.last().cloned() {
            if matches!(top, Value::Tuple(_) | Value::Nothing) {
                let result = self.stack.pop_value()?;
                self.push_split_from_tuple(result)?;
            }
        }
        Ok(())
    }
}
