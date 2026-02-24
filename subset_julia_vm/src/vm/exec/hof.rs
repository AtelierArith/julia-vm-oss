//! Higher-order function operations for the VM.
//!
//! This module handles HOF instructions:
//! - MapFunc: Apply function to each element
//! - MapFuncInPlace: Apply function in-place (map!)
//! - FilterFunc: Filter elements by predicate

// SAFETY: i64→usize cast for range lengths uses `r.length()` which returns ≥ 0;
// i64→usize for n-tuple count is from the instruction operand, always non-negative.
#![allow(clippy::cast_sign_loss)]
//! - FilterFuncInPlace: Filter elements in-place (filter!)
//! - ReduceFunc, ReduceFuncWithInit: Reduce array to single value
//! - MapFoldrFunc, MapFoldrFuncWithInit: Map then right-fold
//! - SumFunc: Sum with function applied
//! - AnyFunc, AllFunc: Check if any/all elements satisfy predicate
//! - CountFunc: Count elements satisfying predicate
//! - NtupleFunc: Create tuple by calling function for each index
//! - MakeGenerator: Create a generator from iterator and function
//!
//! Note: ForEachFunc removed - foreach is now Pure Julia (base/abstractarray.jl)

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::frame::HofOpKind;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::util::{pop_array_or_values, PopArrayResult};
use super::super::value::{new_array_ref, ArrayValue, GeneratorValue, TupleValue, Value};
use super::super::Vm;

/// Result of executing a HOF instruction.
pub(super) enum HofResult {
    /// Instruction not handled by this module
    NotHandled,
    /// Instruction handled successfully
    Handled,
    /// Error was raised and caught by handler, continue to next iteration
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute higher-order function instructions.
    /// Returns the execution result.
    #[inline]
    pub(super) fn execute_hof(&mut self, instr: &Instr) -> Result<HofResult, VmError> {
        match instr {
            // Note: MapFunc, FilterFunc removed - now Pure Julia (base/iterators.jl)
            Instr::FindAllFunc(func_index) => {
                let arr_result = pop_array_or_values(&mut self.stack);
                match self.try_or_handle(arr_result)? {
                    Some(PopArrayResult::F64Array(arr)) => {
                        if arr.borrow().data.is_empty() {
                            // Empty array returns empty Int64 array
                            self.stack
                                .push(Value::Array(new_array_ref(ArrayValue::from_i64(
                                    vec![],
                                    vec![0],
                                ))));
                        } else {
                            self.start_hof_call(*func_index, arr, HofOpKind::FindAll, None)?;
                        }
                    }
                    Some(PopArrayResult::Values { values, shape }) => {
                        // Use value-based HOF path for struct arrays
                        self.start_hof_call_values(*func_index, values, shape, HofOpKind::FindAll)?;
                    }
                    None => return Ok(HofResult::Continue),
                }
                Ok(HofResult::Handled)
            }

            Instr::FindFirstFunc(func_index) => {
                let arr_result = pop_array_or_values(&mut self.stack);
                match self.try_or_handle(arr_result)? {
                    Some(PopArrayResult::F64Array(arr)) => {
                        if arr.borrow().data.is_empty() {
                            // Empty array returns nothing
                            self.stack.push(Value::Nothing);
                        } else {
                            self.start_hof_call(*func_index, arr, HofOpKind::FindFirst, None)?;
                        }
                    }
                    Some(PopArrayResult::Values { values, shape }) => {
                        if values.is_empty() {
                            self.stack.push(Value::Nothing);
                        } else {
                            self.start_hof_call_values(
                                *func_index,
                                values,
                                shape,
                                HofOpKind::FindFirst,
                            )?;
                        }
                    }
                    None => return Ok(HofResult::Continue),
                }
                Ok(HofResult::Handled)
            }

            Instr::FindLastFunc(func_index) => {
                let arr_result = pop_array_or_values(&mut self.stack);
                match self.try_or_handle(arr_result)? {
                    Some(PopArrayResult::F64Array(arr)) => {
                        if arr.borrow().data.is_empty() {
                            // Empty array returns nothing
                            self.stack.push(Value::Nothing);
                        } else {
                            self.start_hof_call(*func_index, arr, HofOpKind::FindLast, None)?;
                        }
                    }
                    Some(PopArrayResult::Values { values, shape }) => {
                        if values.is_empty() {
                            self.stack.push(Value::Nothing);
                        } else {
                            self.start_hof_call_values(
                                *func_index,
                                values,
                                shape,
                                HofOpKind::FindLast,
                            )?;
                        }
                    }
                    None => return Ok(HofResult::Continue),
                }
                Ok(HofResult::Handled)
            }

            // Note: ReduceFunc, ReduceFuncWithInit, FoldrFunc, FoldrFuncWithInit removed
            // - now Pure Julia (base/iterators.jl)
            Instr::MapReduceFunc(map_func_index, reduce_func_index) => {
                // mapreduce(f, op, arr) - apply f to each element, then reduce with op
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                let arr_borrow = arr.borrow();
                if arr_borrow.data.is_empty() {
                    // User-visible: user can call mapreduce on an empty array without providing an init value
                    return Err(VmError::TypeError("mapreduce on empty array".to_string()));
                } else if arr_borrow.len() == 1 {
                    // Just apply the map function to single element
                    let data = arr_borrow.try_as_f64_vec()?;
                    drop(arr_borrow);
                    self.call_function_with_arg(*map_func_index, data[0])?;
                } else {
                    drop(arr_borrow);
                    self.start_mapreduce_call(*map_func_index, *reduce_func_index, arr, None)?;
                }
                Ok(HofResult::Handled)
            }

            Instr::MapReduceFuncWithInit(map_func_index, reduce_func_index) => {
                // mapreduce(f, op, arr, init) - apply f to each element, then reduce with op and init
                let init = self.pop_f64_or_i64()?;
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                if arr.borrow().data.is_empty() {
                    self.stack.push(Value::F64(init));
                } else {
                    self.start_mapreduce_call(
                        *map_func_index,
                        *reduce_func_index,
                        arr,
                        Some(init),
                    )?;
                }
                Ok(HofResult::Handled)
            }

            Instr::MapFoldrFunc(map_func_index, reduce_func_index) => {
                // mapfoldr(f, op, arr) - apply f to each element in reverse, then reduce with op (swapped args)
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                let arr_borrow = arr.borrow();
                if arr_borrow.data.is_empty() {
                    // User-visible: user can call mapfoldr on an empty array without providing an init value
                    return Err(VmError::TypeError("mapfoldr on empty array".to_string()));
                } else if arr_borrow.len() == 1 {
                    // Just apply the map function to single element
                    let data = arr_borrow.try_as_f64_vec()?;
                    drop(arr_borrow);
                    self.call_function_with_arg(*map_func_index, data[0])?;
                } else {
                    // Reverse the array for right-associative fold
                    let data = arr_borrow.try_as_f64_vec()?;
                    let reversed: Vec<f64> = data.iter().rev().cloned().collect();
                    let reversed_arr =
                        new_array_ref(ArrayValue::from_f64(reversed, vec![arr_borrow.len()]));
                    drop(arr_borrow);
                    self.start_mapfoldr_call(
                        *map_func_index,
                        *reduce_func_index,
                        reversed_arr,
                        None,
                    )?;
                }
                Ok(HofResult::Handled)
            }

            Instr::MapFoldrFuncWithInit(map_func_index, reduce_func_index) => {
                // mapfoldr(f, op, arr, init) - apply f to each element in reverse, then reduce with op (swapped args) and init
                let init = self.pop_f64_or_i64()?;
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                if arr.borrow().data.is_empty() {
                    self.stack.push(Value::F64(init));
                } else {
                    // Reverse the array for right-associative fold
                    let arr_borrow = arr.borrow();
                    let data = arr_borrow.try_as_f64_vec()?;
                    let reversed: Vec<f64> = data.iter().rev().cloned().collect();
                    let reversed_arr =
                        new_array_ref(ArrayValue::from_f64(reversed, vec![arr_borrow.len()]));
                    drop(arr_borrow);
                    self.start_mapfoldr_call(
                        *map_func_index,
                        *reduce_func_index,
                        reversed_arr,
                        Some(init),
                    )?;
                }
                Ok(HofResult::Handled)
            }

            Instr::MapFuncInPlace(func_index) => {
                // map!(f, dest, src) - apply f to each element of src and store in dest
                let src_result = self.stack.pop_array();
                let src = match self.try_or_handle(src_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                let dest_result = self.stack.pop_array();
                let dest = match self.try_or_handle(dest_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                let src_len = src.borrow().len();
                let dest_len = dest.borrow().len();
                if src_len != dest_len {
                    // User-visible: user can call map! with source and destination arrays of different lengths
                    return Err(VmError::TypeError(format!(
                        "map!: destination and source must have the same length (got {} and {})",
                        dest_len, src_len
                    )));
                }

                if src.borrow().data.is_empty() {
                    // Empty arrays, just return dest
                    self.stack.push(Value::Array(dest));
                } else {
                    self.start_map_inplace_call(*func_index, dest, src)?;
                }
                Ok(HofResult::Handled)
            }

            Instr::FilterFuncInPlace(func_index) => {
                // filter!(f, arr) - filter elements in-place
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                if arr.borrow().data.is_empty() {
                    // Empty array, just return it
                    self.stack.push(Value::Array(arr));
                } else {
                    self.start_filter_inplace_call(*func_index, arr)?;
                }
                Ok(HofResult::Handled)
            }

            // Note: ForEachFunc removed - foreach is now Pure Julia (base/abstractarray.jl)
            Instr::SumFunc(func_index) => {
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                if arr.borrow().data.is_empty() {
                    // Sum of empty array is 0
                    self.stack.push(Value::F64(0.0));
                } else {
                    self.start_sum_call(*func_index, arr)?;
                }
                Ok(HofResult::Handled)
            }

            Instr::AnyFunc(func_index) => {
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                if arr.borrow().data.is_empty() {
                    // any on empty array is false (Issue #2031)
                    self.stack.push(Value::Bool(false));
                } else {
                    self.start_hof_call(*func_index, arr, HofOpKind::Any, None)?;
                }
                Ok(HofResult::Handled)
            }

            Instr::AllFunc(func_index) => {
                let arr_result = self.stack.pop_array();
                let arr = match self.try_or_handle(arr_result)? {
                    Some(arr) => arr,
                    None => return Ok(HofResult::Continue),
                };

                if arr.borrow().data.is_empty() {
                    // all on empty array is true (vacuous truth, Issue #2031)
                    self.stack.push(Value::Bool(true));
                } else {
                    self.start_hof_call(*func_index, arr, HofOpKind::All, None)?;
                }
                Ok(HofResult::Handled)
            }

            Instr::CountFunc(func_index) => {
                // count(f, itr) - Array/Range only
                // Note: count(f, string) is handled by Pure Julia method dispatch
                // via call.rs routing, not by this HOF instruction (Issue #2081).
                let val = self.stack.pop().ok_or(VmError::StackUnderflow)?;
                let arr = match val {
                    Value::Array(a) => a,
                    Value::Range(r) => {
                        let len = r.length() as usize;
                        let mut data = Vec::with_capacity(len);
                        for i in 0..len {
                            data.push(r.start + (i as f64) * r.step);
                        }
                        new_array_ref(ArrayValue::from_f64(data, vec![len]))
                    }
                    // Memory → Array conversion (Issue #2764)
                    Value::Memory(mem) => super::super::util::memory_to_array_ref(&mem),
                    other => {
                        // User-visible: user can call count on a non-iterable type
                        return Err(VmError::TypeError(format!(
                            "count: expected Array, got {:?}",
                            super::super::util::value_type_name(&other)
                        )));
                    }
                };

                if arr.borrow().data.is_empty() {
                    self.stack.push(Value::I64(0));
                } else {
                    self.start_hof_call_with_accumulator(*func_index, arr, HofOpKind::Count, 0.0)?;
                }
                Ok(HofResult::Handled)
            }

            Instr::NtupleFunc(func_index) => {
                // ntuple(f, n) - Create tuple by calling f(i) for i in 1:n
                let n = self.stack.pop_i64()?;
                if n < 0 {
                    // User-visible: user can call ntuple with a negative n argument
                    return Err(VmError::TypeError(
                        "ntuple: n must be non-negative".to_string(),
                    ));
                }
                if n == 0 {
                    // Return empty tuple
                    self.stack.push(Value::Tuple(TupleValue::new(vec![])));
                } else {
                    // Create an array [1, 2, ..., n] and use HOF infrastructure
                    let indices: Vec<f64> = (1..=n).map(|i| i as f64).collect();
                    let arr = new_array_ref(ArrayValue::from_f64(indices, vec![n as usize]));
                    self.start_ntuple_call(*func_index, arr, n as usize)?;
                }
                Ok(HofResult::Handled)
            }

            Instr::MakeGenerator(func_index) => {
                // Pop the underlying iterator and create a Generator
                let iter = self.stack.pop_value()?;
                let generator = GeneratorValue::new(*func_index, iter);
                self.stack.push(Value::Generator(generator));
                Ok(HofResult::Handled)
            }

            Instr::WrapInGenerator => {
                // Pop an array and wrap it in a Generator for eager-evaluated generator expressions
                // This uses func_index = usize::MAX as a sentinel to indicate "pre-evaluated"
                let arr = self.stack.pop_value()?;
                let generator = GeneratorValue::new(usize::MAX, arr);
                self.stack.push(Value::Generator(generator));
                Ok(HofResult::Handled)
            }

            Instr::SprintFunc(func_index, arg_count) => {
                // sprint(f, args...) - Call f(io, args...) and return result as string
                // Pop args from stack (in reverse order since they were pushed left-to-right)
                let mut args = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count {
                    let arg = self.stack.pop_value()?;
                    args.push(arg);
                }
                args.reverse(); // Restore original order

                // Create an IOBuffer reference for interior mutability
                let io = super::super::value::IOValue::buffer_ref();

                // Start the sprint call: call f(io, args...)
                self.start_sprint_call(*func_index, io, args)?;
                Ok(HofResult::Handled)
            }

            _ => Ok(HofResult::NotHandled),
        }
    }
}
