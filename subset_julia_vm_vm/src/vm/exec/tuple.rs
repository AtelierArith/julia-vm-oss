//! Tuple operations for the VM.
//!
//! This module handles tuple instructions:
//! - NewTuple: Create a tuple from stack values
//! - LoadTuple, StoreTuple: Load/store tuple variables
//! - TupleGet: Get element by index

// SAFETY: i64→usize casts for tuple element access are guarded by bounds checks
// that ensure the index is in [1, len].
#![allow(clippy::cast_sign_loss)]
//! - TupleUnpack: Destructure tuple into stack values
//! - TupleFirst, TupleSecond: Get first/second element
//! - ReturnTuple: Return tuple from function

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::DispatchAction;
use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::frame::VarTypeTag;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{TupleValue, Value};
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute tuple instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_tuple(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::NewTuple(n) => {
                let mut elements = Vec::with_capacity(*n);
                for _ in 0..*n {
                    elements.push(self.stack.pop_value()?);
                }
                elements.reverse();
                self.stack.push(Value::Tuple(TupleValue::new(elements)));
                Ok(DispatchAction::Continue)
            }

            // Issue #4722: Core.svec(...) builds a Core.SimpleVector value.
            Instr::MakeSimpleVector(n) => {
                let mut elements = Vec::with_capacity(*n);
                for _ in 0..*n {
                    elements.push(self.stack.pop_value()?);
                }
                elements.reverse();
                self.stack
                    .push(Value::SimpleVector(TupleValue::new(elements)));
                Ok(DispatchAction::Continue)
            }

            Instr::LoadTuple(name) => {
                let tuple = self
                    .frames
                    .last()
                    .and_then(|frame| match self.load_slot_value_by_name(frame, name) {
                        Some(Value::Tuple(t)) => Some(t),
                        _ => frame.locals_any.get(name).and_then(|value| match value {
                            Value::Tuple(tuple) => Some(tuple.clone()),
                            _ => None,
                        }),
                    })
                    .or_else(|| {
                        if self.frames.len() > 1 {
                            self.frames.first().and_then(|frame| {
                                match self.load_slot_value_by_name(frame, name) {
                                    Some(Value::Tuple(t)) => Some(t),
                                    _ => frame.locals_any.get(name).and_then(|value| match value {
                                        Value::Tuple(tuple) => Some(tuple.clone()),
                                        _ => None,
                                    }),
                                }
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| TupleValue::new(Vec::new()));
                self.stack.push(Value::Tuple(tuple));
                Ok(DispatchAction::Continue)
            }

            Instr::StoreTuple(name) => {
                let tuple = match self.stack.pop_value()? {
                    Value::Tuple(t) => t,
                    other => {
                        // INTERNAL: StoreTuple is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )));
                    }
                };
                if let Some(frame) = self.frames.last_mut() {
                    frame.locals_any.insert(name.clone(), Value::Tuple(tuple));
                    frame.var_types.insert(name.clone(), VarTypeTag::Tuple);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::TupleGet => {
                let index = self.stack.pop_i64()?;
                let val = self.stack.pop_value()?;

                let value = match &val {
                    // Issue #7964: flat StaticArray acts as its own .data tuple.
                    Value::StaticArray(sv) => match self.try_or_handle(sv.get_1d(index))? {
                        Some(v) => v,
                        None => return Ok(DispatchAction::Continue),
                    },
                    // Issue #7964 Phase 3: inline variant — same interface, no allocation.
                    Value::StaticArrayInline(sv) => match self.try_or_handle(sv.get_1d(index))? {
                        Some(v) => v,
                        None => return Ok(DispatchAction::Continue),
                    },
                    // Tuple and Core.SimpleVector share linear indexing (Issue #4722).
                    Value::Tuple(t) | Value::SimpleVector(t) => {
                        match self.try_or_handle(t.get(index).cloned())? {
                            Some(v) => v,
                            None => return Ok(DispatchAction::Continue),
                        }
                    }
                    // A value-carried NamedTuple is tuple-indexable: `nt[i]` yields
                    // the i-th field value in declaration order, with the same
                    // 1-based bounds as a Tuple (Issue #9786). This is reachable when
                    // a NamedTuple carried across REPL evals (Persistent model) hits a
                    // call site the compiler typed as Tuple.
                    Value::NamedTuple(nt) => {
                        match self.try_or_handle(nt.get_by_index(index).cloned())? {
                            Some(v) => v,
                            None => return Ok(DispatchAction::Continue),
                        }
                    }
                    // Handle Pair struct as a 2-element tuple (for Dict iteration)
                    Value::Struct(s) if &*s.struct_name == "Pair" && s.values.len() == 2 => {
                        // Julia uses 1-based indexing, so index 1 = first element, index 2 = second element
                        if !(1..=2).contains(&index) {
                            self.raise(VmError::IndexOutOfBounds {
                                indices: vec![index],
                                shape: vec![2],
                            })?;
                            return Ok(DispatchAction::Continue);
                        }
                        s.values[(index - 1) as usize].clone()
                    }
                    // Handle StructRef - dereference it first
                    Value::StructRef(idx) => {
                        // Build result while heap is borrowed, then release borrow before
                        // calling try_or_handle (which needs &mut self).
                        let heap_idx = *idx;
                        let result = {
                            let s = self.struct_heap.get(heap_idx);
                            match s {
                                Some(s) if &*s.struct_name == "Pair" && s.values.len() == 2 => {
                                    if (1..=2).contains(&index) {
                                        Ok(s.values[(index - 1) as usize].clone())
                                    } else {
                                        Err(VmError::IndexOutOfBounds {
                                            indices: vec![index],
                                            shape: vec![2],
                                        })
                                    }
                                }
                                Some(s) => Err(VmError::TypeError(format!(
                                    "TupleGet: Expected Tuple or Pair, got struct {}",
                                    s.struct_name
                                ))),
                                None => Err(VmError::TypeError(format!(
                                    "Invalid struct reference: {}",
                                    heap_idx
                                ))),
                            }
                        };
                        match self.try_or_handle(result)? {
                            Some(v) => v,
                            None => return Ok(DispatchAction::Continue),
                        }
                    }
                    other => {
                        // INTERNAL: TupleGet is emitted only when the compiler typed the collection as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )));
                    }
                };

                self.stack.push(value);
                Ok(DispatchAction::Continue)
            }

            Instr::TupleUnpack(n) => {
                let val = self.stack.pop_value()?;
                let elements = match &val {
                    // Tuple and Core.SimpleVector unpack identically (Issue #4722).
                    Value::Tuple(t) | Value::SimpleVector(t) => t.elements.clone(),
                    // A NamedTuple destructures to its field values in declaration
                    // order — `x, y, z = nt` binds x = nt[1], y = nt[2], … (Issue
                    // #9786). Reachable when a value-carried NamedTuple global (the
                    // Persistent REPL model) reaches a `TupleUnpack` the compiler
                    // emitted for an opaque RHS. Arity/BoundsError semantics below are
                    // shared with the Tuple arm.
                    Value::NamedTuple(nt) => nt.values.clone(),
                    // Handle Pair struct as a 2-element tuple (for Dict iteration destructuring)
                    Value::Struct(s) if &*s.struct_name == "Pair" && s.values.len() == 2 => {
                        s.values.clone()
                    }
                    // Handle StructRef - dereference it first
                    Value::StructRef(idx) => {
                        if let Some(s) = self.struct_heap.get(*idx) {
                            if &*s.struct_name == "Pair" && s.values.len() == 2 {
                                s.values.clone()
                            } else {
                                // INTERNAL: TupleUnpack non-Pair struct is a compiler bug; only Pair structs can be treated as 2-tuples
                                return Err(VmError::InternalError(format!(
                                    "Expected Tuple or Pair, got struct {}",
                                    s.struct_name
                                )));
                            }
                        } else {
                            // INTERNAL: TupleUnpack StructRef index is compiler-generated; invalid ref means heap corruption
                            return Err(VmError::InternalError(format!(
                                "Invalid struct reference: {}",
                                idx
                            )));
                        }
                    }
                    other => {
                        // INTERNAL: TupleUnpack is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )));
                    }
                };
                if elements.len() != *n {
                    self.raise(VmError::TupleDestructuringMismatch {
                        expected: *n,
                        got: elements.len(),
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                for elem in elements {
                    self.stack.push(elem);
                }
                Ok(DispatchAction::Continue)
            }

            Instr::TupleFirst => {
                let val = self.stack.pop_value()?;
                // A value-carried NamedTuple iterates its field values in order, so
                // `first(nt)` yields nt[1] (Issue #9786) — same as a Tuple.
                let elements: &[Value] = match &val {
                    Value::Tuple(t) | Value::SimpleVector(t) => &t.elements,
                    Value::NamedTuple(nt) => &nt.values,
                    other => {
                        // INTERNAL: TupleFirst is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "TupleFirst: expected Tuple, got {:?}",
                            other
                        )));
                    }
                };
                if elements.is_empty() {
                    // User-visible: first(()) throws BoundsError in Julia — must be catchable.
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![0],
                        shape: vec![0],
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                let first = elements[0].clone();
                self.stack.push(first);
                Ok(DispatchAction::Continue)
            }

            Instr::TupleSecond => {
                let val = self.stack.pop_value()?;
                // A value-carried NamedTuple iterates its field values in order, so
                // its second element is nt[2] (Issue #9786) — same as a Tuple.
                let elements: &[Value] = match &val {
                    Value::Tuple(t) | Value::SimpleVector(t) => &t.elements,
                    Value::NamedTuple(nt) => &nt.values,
                    other => {
                        // INTERNAL: TupleSecond is emitted only when the compiler typed the value as Tuple
                        return Err(VmError::InternalError(format!(
                            "TupleSecond: expected Tuple, got {:?}",
                            other
                        )));
                    }
                };
                if elements.len() < 2 {
                    // User-visible: tuple[2] on single-element tuple throws BoundsError — catchable.
                    self.raise(VmError::IndexOutOfBounds {
                        indices: vec![1],
                        shape: vec![elements.len()],
                    })?;
                    return Ok(DispatchAction::Continue);
                }
                let second = elements[1].clone();
                self.stack.push(second);
                Ok(DispatchAction::Continue)
            }

            Instr::ReturnTuple => {
                let tuple = match self.stack.pop_value()? {
                    Value::Tuple(t) => t,
                    other => {
                        // INTERNAL: ReturnTuple is emitted only when the compiler typed the return value as Tuple
                        return Err(VmError::InternalError(format!(
                            "Expected Tuple, got {:?}",
                            other
                        )));
                    }
                };
                // Route through the shared continuation machinery so a tuple
                // returned from a `map`/`filter`/generator closure is collected
                // by the HOF/generator driver instead of leaking past it
                // (Issue #5231).
                match self.route_value_return(Value::Tuple(tuple))? {
                    super::return_ops::ValueReturnRouting::Handled => Ok(DispatchAction::Continue),
                    super::return_ops::ValueReturnRouting::Exit(v) => Ok(DispatchAction::Exit(v)),
                }
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}
