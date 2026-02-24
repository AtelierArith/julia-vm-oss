//! VM execution loop.
//!
//! This module contains the main `run()` method for the VM.
//! The run loop fetches and executes instructions until completion or error.

#![deny(clippy::unwrap_used)]
// SAFETY: i64/f64→u64 casts in NewStableRng/NewXoshiro seed initialization;
// negative seeds are reinterpreted as unsigned bit patterns, which is intentional.
#![allow(clippy::cast_sign_loss)]
#![deny(clippy::expect_used)]

mod arithmetic;
mod array_basic;
mod array_index;
mod array_index_slice;
mod array_mutate;
mod call;
mod binary_both;
mod binary_no_fallback;
mod call_dynamic;
mod call_dynamic_binary;
mod call_dynamic_typed;
mod call_function_variable;
mod comparison;
mod conversion;
mod dict;
mod error_handling;
mod hof;
mod iterator;
mod jump;
mod locals;
mod matrix;
mod memory;
mod named_tuple;
mod pairs;
mod print;
mod range;
mod return_ops;
mod rng;
mod set;
mod sleep;
mod stack;
mod string_ops;
mod struct_ops;
mod tuple;

use super::*;
use arithmetic::ArithmeticResult;
use array_basic::ArrayBasicResult;
use array_index::ArrayIndexResult;
use array_mutate::ArrayMutateResult;
use call::CallResult;
use call_dynamic::CallDynamicResult;
use dict::DictResult;
use error_handling::ErrorResult;
use hof::HofResult;
use jump::JumpResult;
use locals::LocalsResult;
use matrix::MatrixResult;
use memory::MemoryResult;
use named_tuple::NamedTupleResult;
use pairs::PairsResult;
use print::PrintResult;
use return_ops::ReturnResult;
use set::SetResult;
use sleep::SleepResult;
use struct_ops::StructResult;
use tuple::TupleResult;
use util::{format_value, value_to_string};

use crate::cancel;
use crate::rng::{randn, RngLike};

/// Result of dispatching a single instruction (Issue #2939).
enum DispatchAction {
    /// Continue to next instruction
    Continue,
    /// Exit the VM with a value
    Exit(Value),
}

impl<R: RngLike> Vm<R> {
    pub fn run(&mut self) -> Result<Value, VmError> {
        loop {
            if cancel::is_requested() {
                return Err(VmError::Cancelled);
            }
            let ip = self.ip;
            self.ip += 1;

            // Temporarily extract instruction to avoid clone (Issue #2939).
            // std::mem::replace swaps the instruction out with a Nop placeholder
            // (O(1) memcpy of the enum, no heap allocation).
            // The instruction is always put back after dispatch completes.
            let instr = std::mem::replace(&mut self.code[ip], Instr::Nop);

            // Profile instruction execution
            super::profiler::record(&instr);

            // Debug: trace every instruction (comment out in production)
            #[cfg(debug_assertions)]
            if std::env::var("TRACE_INSTRS").is_ok() {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "VM: ip={}, instr={:?}", ip, instr);
            }

            // Dispatch instruction; all handler logic is in a separate method
            // so that the instruction is always restored regardless of errors.
            let result = self.dispatch_instr(&instr);

            // Always put instruction back before handling the result.
            // This ensures the code vector remains intact even on errors,
            // and loop-back jumps find the correct instruction.
            self.code[ip] = instr;

            match result {
                Ok(DispatchAction::Continue) => continue,
                Ok(DispatchAction::Exit(val)) => return Ok(val),
                Err(err) => {
                    // Store the IP of the failing instruction for span lookup (Issue #2856)
                    self.last_error_ip = Some(ip);
                    return Err(err);
                }
            }
        }
    }

    /// Dispatch a single instruction to the appropriate handler.
    ///
    /// Takes `&Instr` (a reference to a local variable in `run()`), avoiding
    /// the need to clone every instruction on every cycle. The borrow checker
    /// is satisfied because `instr` is not borrowed from `self`.
    #[inline(always)]
    fn dispatch_instr(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        // Delegated handlers: try specialized modules first
        if self.execute_stack(instr)?.is_some() {
            return Ok(DispatchAction::Continue);
        }
        match self.execute_locals(instr)? {
            LocalsResult::Handled | LocalsResult::Continue => return Ok(DispatchAction::Continue),
            LocalsResult::NotHandled => {}
        }
        match self.execute_arithmetic(instr)? {
            ArithmeticResult::Handled | ArithmeticResult::Continue => {
                return Ok(DispatchAction::Continue)
            }
            ArithmeticResult::NotHandled => {}
        }
        if self.execute_comparison(instr)?.is_some() {
            return Ok(DispatchAction::Continue);
        }
        if self.execute_rng(instr)?.is_some() {
            return Ok(DispatchAction::Continue);
        }
        if self.execute_conversion(instr)?.is_some() {
            return Ok(DispatchAction::Continue);
        }
        if self.execute_string_ops(instr)?.is_some() {
            return Ok(DispatchAction::Continue);
        }
        if self.execute_range(instr)?.is_some() {
            return Ok(DispatchAction::Continue);
        }
        if self.execute_iterator(instr)?.is_some() {
            return Ok(DispatchAction::Continue);
        }
        match self.execute_sleep(instr)? {
            SleepResult::Handled | SleepResult::Continue => return Ok(DispatchAction::Continue),
            SleepResult::NotHandled => {}
        }
        match self.execute_print(instr)? {
            PrintResult::Handled | PrintResult::Continue => return Ok(DispatchAction::Continue),
            PrintResult::NotHandled => {}
        }
        match self.execute_error_handling(instr)? {
            ErrorResult::Handled | ErrorResult::Continue => return Ok(DispatchAction::Continue),
            ErrorResult::NotHandled => {}
        }
        match self.execute_jump(instr)? {
            JumpResult::Jump(target) => {
                self.ip = target;
                return Ok(DispatchAction::Continue);
            }
            JumpResult::NoJump => return Ok(DispatchAction::Continue),
            JumpResult::NotHandled => {}
        }
        match self.execute_struct(instr)? {
            StructResult::Handled | StructResult::Continue => return Ok(DispatchAction::Continue),
            StructResult::Return(val) => return Ok(DispatchAction::Exit(val)),
            StructResult::NotHandled => {}
        }
        match self.execute_tuple(instr)? {
            TupleResult::Handled | TupleResult::Continue => return Ok(DispatchAction::Continue),
            TupleResult::Return(val) => return Ok(DispatchAction::Exit(val)),
            TupleResult::NotHandled => {}
        }
        match self.execute_named_tuple(instr)? {
            NamedTupleResult::Handled | NamedTupleResult::Continue => {
                return Ok(DispatchAction::Continue)
            }
            NamedTupleResult::Return(val) => return Ok(DispatchAction::Exit(val)),
            NamedTupleResult::NotHandled => {}
        }
        match self.execute_pairs(instr)? {
            PairsResult::Handled | PairsResult::Continue => return Ok(DispatchAction::Continue),
            PairsResult::NotHandled => {}
        }
        match self.execute_dict(instr)? {
            DictResult::Handled => return Ok(DispatchAction::Continue),
            DictResult::Return(val) => return Ok(DispatchAction::Exit(val)),
            DictResult::NotHandled => {}
        }
        match self.execute_set(instr)? {
            SetResult::Handled => return Ok(DispatchAction::Continue),
            SetResult::Return(val) => return Ok(DispatchAction::Exit(val)),
            SetResult::NotHandled => {}
        }
        match self.execute_hof(instr)? {
            HofResult::Handled | HofResult::Continue => return Ok(DispatchAction::Continue),
            HofResult::NotHandled => {}
        }
        match self.execute_array_basic(instr)? {
            ArrayBasicResult::Handled | ArrayBasicResult::Continue => {
                return Ok(DispatchAction::Continue)
            }
            ArrayBasicResult::NotHandled => {}
        }
        match self.execute_array_index(instr)? {
            ArrayIndexResult::Handled | ArrayIndexResult::Continue => {
                return Ok(DispatchAction::Continue)
            }
            ArrayIndexResult::NotHandled => {}
        }
        match self.execute_array_mutate(instr)? {
            ArrayMutateResult::Handled | ArrayMutateResult::Continue => {
                return Ok(DispatchAction::Continue)
            }
            ArrayMutateResult::NotHandled => {}
        }
        match self.execute_matrix(instr)? {
            MatrixResult::Handled | MatrixResult::Continue => return Ok(DispatchAction::Continue),
            MatrixResult::NotHandled => {}
        }
        match self.execute_memory(instr)? {
            MemoryResult::Handled => return Ok(DispatchAction::Continue),
            MemoryResult::Return(val) => return Ok(DispatchAction::Exit(val)),
            MemoryResult::NotHandled => {}
        }
        match self.execute_return(instr)? {
            ReturnResult::Handled | ReturnResult::Continue => return Ok(DispatchAction::Continue),
            ReturnResult::Exit(val) => return Ok(DispatchAction::Exit(val)),
            ReturnResult::NotHandled => {}
        }
        match self.execute_call(instr)? {
            CallResult::Handled | CallResult::Continue => return Ok(DispatchAction::Continue),
            CallResult::NotHandled => {}
        }
        match self.execute_call_dynamic(instr)? {
            CallDynamicResult::Handled | CallDynamicResult::Continue => {
                return Ok(DispatchAction::Continue)
            }
            CallDynamicResult::NotHandled => {}
        }
        match instr {
            Instr::TimeNs => {
                // Use WASM timing only when both feature is enabled AND target is wasm32
                // This prevents js_sys calls on native targets during workspace builds
                #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
                {
                    // In WASM, use js_sys::Date::now() which returns milliseconds
                    // Convert to nanoseconds for consistency
                    let now_ms = js_sys::Date::now();
                    let now_ns = (now_ms * 1_000_000.0) as i64;
                    self.stack.push(Value::I64(now_ns));
                }
                #[cfg(not(all(feature = "wasm", target_arch = "wasm32")))]
                {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    #[allow(clippy::expect_used)]
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Time went backwards")
                        .as_nanos() as i64;
                    self.stack.push(Value::I64(now));
                }
            }

            // Array operations delegated to array_basic, array_index, array_mutate, matrix modules

            // NOTE: ArrayLen, ArraySum, ArrayShape, ArrayToSizeTuple, Zeros, Ones, Trues, Falses, Fill
            //       have been moved to CallBuiltin (Layer 2 Builtins)
            Instr::SliceAll => {
                self.stack.push(Value::SliceAll);
            }

            // RNG instance operations
            Instr::NewStableRng => {
                let seed = match self.stack.pop() {
                    Some(Value::I64(s)) => s as u64,
                    Some(Value::F64(s)) => s as u64,
                    _ => 0,
                };
                self.stack
                    .push(Value::Rng(RngInstance::Stable(StableRng::new(seed))));
            }
            Instr::NewXoshiro => {
                let seed = match self.stack.pop() {
                    Some(Value::I64(s)) => s as u64,
                    Some(Value::F64(s)) => s as u64,
                    _ => 0,
                };
                self.stack
                    .push(Value::Rng(RngInstance::Xoshiro(Xoshiro::new(seed))));
            }
            Instr::LoadRng(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Rng(rng)) = self.load_slot_value_by_name(frame, name) {
                        self.stack.push(Value::Rng(rng));
                        return Ok(DispatchAction::Continue);
                    }
                    if let Some(rng) = frame.locals_rng.get(name).cloned() {
                        self.stack.push(Value::Rng(rng));
                        return Ok(DispatchAction::Continue);
                    }
                }
                if self.frames.len() > 1 {
                    if let Some(frame) = self.frames.first() {
                        if let Some(Value::Rng(rng)) = self.load_slot_value_by_name(frame, name) {
                            self.stack.push(Value::Rng(rng));
                            return Ok(DispatchAction::Continue);
                        }
                        if let Some(rng) = frame.locals_rng.get(name).cloned() {
                            self.stack.push(Value::Rng(rng));
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                // INTERNAL: LoadRng is emitted only for RNG-typed variables; variable not found is a compiler bug
                return Err(VmError::InternalError(format!(
                    "RNG variable not found: {}",
                    name
                )));
            }
            Instr::StoreRng(name) => {
                if let Some(Value::Rng(rng)) = self.stack.pop() {
                    if let Some(frame) = self.frames.last_mut() {
                        frame.locals_rng.insert(name.clone(), rng);
                        frame.var_types.insert(name.clone(), frame::VarTypeTag::Rng);
                    }
                }
            }
            Instr::RngRandF64 => {
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let val = rng.next_f64();
                    self.stack.push(Value::F64(val));
                    self.stack.push(Value::Rng(rng));
                }
            }
            Instr::RngRandnF64 => {
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let val = randn(&mut rng);
                    self.stack.push(Value::F64(val));
                    self.stack.push(Value::Rng(rng));
                }
            }
            // NOTE: ReturnRng delegated to return_ops module
            Instr::LoadRange(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Range(range)) = self.load_slot_value_by_name(frame, name) {
                        self.stack.push(Value::Range(range));
                        return Ok(DispatchAction::Continue);
                    }
                    if let Some(range) = frame.locals_range.get(name).cloned() {
                        self.stack.push(Value::Range(range));
                        return Ok(DispatchAction::Continue);
                    }
                }
                if self.frames.len() > 1 {
                    if let Some(frame) = self.frames.first() {
                        if let Some(Value::Range(range)) = self.load_slot_value_by_name(frame, name)
                        {
                            self.stack.push(Value::Range(range));
                            return Ok(DispatchAction::Continue);
                        }
                        if let Some(range) = frame.locals_range.get(name).cloned() {
                            self.stack.push(Value::Range(range));
                            return Ok(DispatchAction::Continue);
                        }
                    }
                }
                // INTERNAL: LoadRange is emitted only for Range-typed variables; variable not found is a compiler bug
                return Err(VmError::InternalError(format!(
                    "Range variable not found: {}",
                    name
                )));
            }
            Instr::StoreRange(name) => {
                if let Some(Value::Range(range)) = self.stack.pop() {
                    if let Some(frame) = self.frames.last_mut() {
                        frame.locals_range.insert(name.clone(), range);
                        frame.var_types.insert(name.clone(), frame::VarTypeTag::Range);
                    }
                }
            }
            // NOTE: ReturnRange delegated to return_ops module

            // Catch-all for other unimplemented instructions
            _ => {
                return Err(VmError::NotImplemented(format!(
                    "Instruction not yet implemented: {:?}",
                    instr
                )));
            }
        }
        Ok(DispatchAction::Continue)
    }
}
