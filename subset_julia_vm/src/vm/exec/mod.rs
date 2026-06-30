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
pub(crate) mod array_basic;
mod array_index;
mod array_index_slice;
mod array_mutate;
mod binary_both;
mod binary_no_fallback;
mod call;
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
pub(in crate::vm) mod return_ops;
mod rng;
mod set;
mod sleep;
mod stack;
mod string_ops;
mod struct_ops;
mod tuple;

use super::executable::ExecutableBlockResult;
use super::*;
use util::value_to_string;

use crate::rng::{randn, RngLike};
use std::rc::Rc;

/// Result of dispatching a single instruction (Issue #2939).
pub(super) enum DispatchAction {
    /// Continue to next instruction
    Continue,
    /// Exit the VM with a value
    Exit(Value),
}

/// Generates the exhaustive `Instr` -> handler dispatch `match` (Issue #6827).
///
/// The match is wrapped in a macro so `dispatch_instr` stays a one-line shim
/// while the variant->handler table lives here. Expansion is byte-for-byte the
/// original `match instr { ... }`, so codegen (and the optimized jump table)
/// is unchanged — no benchmark regression — and the match is still exhaustive,
/// so adding an `Instr` variant without a handler is a compile error.
macro_rules! dispatch_instr_match {
    ($self:ident, $instr:ident) => {
        match $instr {
            Instr::PushI64(..)
            | Instr::PushI128(..)
            | Instr::PushBigInt(..)
            | Instr::PushBigFloat(..)
            | Instr::PushF64(..)
            | Instr::PushF32(..)
            | Instr::PushF16(..)
            | Instr::PushBool(..)
            | Instr::PushBoundsCheckEnabled
            | Instr::PushStr(..)
            | Instr::PushChar(..)
            | Instr::PushNothing
            | Instr::PushMissing
            | Instr::PushUndef
            | Instr::PushStdout
            | Instr::PushStderr
            | Instr::PushStdin
            | Instr::PushDevnull
            | Instr::PushCNull
            | Instr::PushEnv
            | Instr::PushModule(..)
            | Instr::PushDataType(..)
            | Instr::PushFunction(..)
            | Instr::PushResolvedFunction(..)
            | Instr::CreateClosure { .. }
            | Instr::LoadCaptured(..)
            | Instr::DefineFunction(..)
            | Instr::DefineEvalFunction(..)
            | Instr::Pop
            | Instr::PopIfIO
            | Instr::Swap
            | Instr::MakeRef
            | Instr::UnwrapRef
            | Instr::PushSymbol(..)
            | Instr::CreateExpr { .. }
            | Instr::CreateQuoteNode
            | Instr::PushLineNumberNode { .. }
            | Instr::PushRegex { .. }
            | Instr::PushEnum { .. }
            | Instr::RegisterEnum(..)
            | Instr::ConstructEnum(..) => $self.execute_stack($instr),
            Instr::LoadStr(..)
            | Instr::StoreStr(..)
            | Instr::LoadI64(..)
            | Instr::StoreI64(..)
            | Instr::LoadF64(..)
            | Instr::StoreF64(..)
            | Instr::LoadF32(..)
            | Instr::StoreF32(..)
            | Instr::LoadF16(..)
            | Instr::StoreF16(..)
            | Instr::LoadBool(..)
            | Instr::StoreBool(..)
            | Instr::LoadSlot(..)
            | Instr::StoreSlot(..)
            | Instr::LoadSlotArray(..)
            | Instr::StoreSlotArray(..)
            | Instr::LoadSlotTuple(..)
            | Instr::StoreSlotTuple(..)
            | Instr::LoadSlotNamedTuple(..)
            | Instr::StoreSlotNamedTuple(..)
            | Instr::LoadSlotDict(..)
            | Instr::StoreSlotDict(..)
            | Instr::LoadSlotSet(..)
            | Instr::StoreSlotSet(..)
            | Instr::LoadSlotStruct(..)
            | Instr::StoreSlotStruct(..)
            | Instr::LoadSlotRange(..)
            | Instr::StoreSlotRange(..)
            | Instr::LoadSlotRng(..)
            | Instr::StoreSlotRng(..)
            | Instr::LoadSlotGenerator(..)
            | Instr::StoreSlotGenerator(..)
            | Instr::LoadSlotI64(..)
            | Instr::LoadSlotI64ToF64(..)
            | Instr::StoreSlotI64(..)
            | Instr::LoadSlotF64(..)
            | Instr::StoreSlotF64(..)
            | Instr::LoadSquareF64Slot(..)
            | Instr::LoadAddF64Slot(..)
            | Instr::LoadSubF64Slot(..)
            | Instr::LoadMulF64Slot(..)
            | Instr::LoadDivF64Slot(..)
            | Instr::LoadSlotBool(..)
            | Instr::StoreSlotBool(..)
            | Instr::LoadSlotF32(..)
            | Instr::StoreSlotF32(..)
            | Instr::LoadSlotF16(..)
            | Instr::StoreSlotF16(..)
            | Instr::LoadSlotStr(..)
            | Instr::StoreSlotStr(..)
            | Instr::LoadSlotChar(..)
            | Instr::StoreSlotChar(..)
            | Instr::LoadSlotSymbol(..)
            | Instr::StoreSlotSymbol(..)
            | Instr::LoadSlotNarrowInt(..)
            | Instr::StoreSlotNarrowInt(..)
            | Instr::LoadSlotNothing(..)
            | Instr::StoreSlotNothing(..)
            | Instr::LoadAny(..)
            | Instr::LoadGlobalAny(..)
            | Instr::LoadTypeBinding(..)
            | Instr::LoadValBool(..)
            | Instr::LoadValSymbol(..)
            | Instr::StoreAny(..)
            | Instr::StoreGlobalAny(..)
            | Instr::LoadAddI64(..)
            | Instr::LoadAddI64Slot(..)
            | Instr::LoadSubI64(..)
            | Instr::LoadSubI64Slot(..)
            | Instr::LoadMulI64(..)
            | Instr::LoadMulI64Slot(..)
            | Instr::LoadModI64(..)
            | Instr::LoadModI64Slot(..)
            | Instr::IncVarI64(..)
            | Instr::IncVarI64Slot(..)
            | Instr::AddConstI64Slot(..)
            | Instr::DecVarI64(..)
            | Instr::DecVarI64Slot(..)
            | Instr::IsDefined(..) => $self.execute_locals($instr),
            Instr::DynamicAdd
            | Instr::DynamicSub
            | Instr::DynamicMul
            | Instr::DynamicDiv
            | Instr::DynamicMod
            | Instr::DynamicIntDiv
            | Instr::DynamicNeg
            | Instr::DynamicPow
            | Instr::AddI64
            | Instr::SubI64
            | Instr::MulI64
            | Instr::ModI64
            | Instr::IncI64
            | Instr::NegI64
            | Instr::DupI64
            | Instr::DupF64
            | Instr::Dup
            | Instr::AddF64
            | Instr::SubF64
            | Instr::MulF64
            | Instr::DivF64
            | Instr::NegF64
            | Instr::PowF64
            | Instr::SqrtF64
            | Instr::FloorF64
            | Instr::CeilF64
            | Instr::AbsF64
            | Instr::Abs2F64 => $self.execute_arithmetic($instr),
            Instr::GtI64
            | Instr::LtI64
            | Instr::LeI64
            | Instr::GeI64
            | Instr::EqI64
            | Instr::NeI64
            | Instr::LtF64
            | Instr::GtF64
            | Instr::LeF64
            | Instr::GeF64
            | Instr::EqF64
            | Instr::NeF64
            | Instr::EqStruct
            | Instr::EqStr
            | Instr::LtStr
            | Instr::LeStr
            | Instr::GtStr
            | Instr::GeStr
            | Instr::SelectI64
            | Instr::SelectF64 => $self.execute_comparison($instr),
            Instr::Jump(..)
            | Instr::JumpIfZero(..)
            | Instr::JumpIfNeI64(..)
            | Instr::JumpIfEqI64(..)
            | Instr::JumpIfLtI64(..)
            | Instr::JumpIfGtI64(..)
            | Instr::JumpIfGtI64Slots(..)
            | Instr::AddConstI64SlotAndJumpIfLe(..)
            | Instr::JumpIfLeI64(..)
            | Instr::JumpIfGeI64(..)
            | Instr::JumpIfEqF64(..)
            | Instr::JumpIfNeF64(..)
            | Instr::JumpIfNotLtF64(..)
            | Instr::JumpIfNotGtF64(..)
            | Instr::JumpIfNotLeF64(..)
            | Instr::JumpIfNotGeF64(..) => $self.execute_jump($instr),
            Instr::ReturnF64
            | Instr::ReturnF32
            | Instr::ReturnF16
            | Instr::ReturnI64
            | Instr::ReturnArray
            | Instr::ReturnAny
            | Instr::ReturnNothing
            | Instr::ReturnRng
            | Instr::ReturnRange
            | Instr::ReturnRef => {
                let action = $self.execute_return($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::Call(..)
            | Instr::CallInbounds(..)
            | Instr::CallResolved(..)
            | Instr::CallResolvedI64Slots(..)
            | Instr::CallInboundsI64Slots(..)
            | Instr::CallStaticParametric(..)
            | Instr::CallWithKwargs(..)
            | Instr::CallWithKwargsSplat(..)
            | Instr::CallWithSplat(..)
            | Instr::CallSpecialize(..)
            | Instr::CallSpecializeInbounds(..)
            | Instr::CallSpecializeI64Slots(..)
            | Instr::CallSpecializeInboundsI64Slots(..)
            | Instr::CallIntrinsic(..)
            | Instr::CallBuiltin(..) => {
                let action = $self.execute_call($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::CallDynamicBinary(..)
            | Instr::CallDynamicBinaryBoth(..)
            | Instr::CallDynamicBinaryNoFallback(..)
            | Instr::CallTypedDispatch(..)
            | Instr::CallTypedDispatchOrBuiltin(..)
            | Instr::CallTypedDispatchOrBuiltinResult(..)
            | Instr::CallTypedDispatchOrBuiltinStoreDict(..)
            | Instr::CallTypedDispatchOrBuiltinStoreDictResult(..)
            | Instr::CallTypeConstructor
            | Instr::CallGlobalRef(..)
            | Instr::CallFunctionVariable(..)
            | Instr::InvokeFunctionVariable(..)
            | Instr::InvokeFunctionVariableWithKwargs(..)
            | Instr::InvokeFunctionVariableDynamicSignature(..)
            | Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(..)
            | Instr::CallFunctionVariableWithSplat(..)
            | Instr::CallFunctionVariableWithKwargsSplat(..)
            | Instr::CallDynamic(..)
            | Instr::CallDynamicOrBuiltin(..)
            | Instr::IterateDynamic(..) => {
                let action = $self.execute_call_dynamic($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::RandF64
            | Instr::RandArray(..)
            | Instr::RandIntArray(..)
            | Instr::RandnF64
            | Instr::RandnArray(..)
            | Instr::SeedGlobalRng => $self.execute_rng($instr),
            Instr::ToF64
            | Instr::ToI64
            | Instr::BoolToI64
            | Instr::I64ToBool
            | Instr::NotBool
            | Instr::DynamicToBool
            | Instr::DynamicToF64
            | Instr::DynamicToF32
            | Instr::DynamicToF16
            | Instr::DynamicToI64
            | Instr::DynamicToI8
            | Instr::DynamicToI16
            | Instr::DynamicToI32
            | Instr::DynamicToU8
            | Instr::DynamicToU16
            | Instr::DynamicToU32
            | Instr::DynamicToU64
            | Instr::IsNothing => $self.execute_conversion($instr),
            Instr::ToString | Instr::StringConcat(..) | Instr::ConcatStrings(..) | Instr::ToStr => {
                $self.execute_string_ops($instr)
            }
            Instr::MakeRange
            | Instr::MakeRangeF64
            | Instr::MakeRangeLazy
            | Instr::MakeStepRangeLazy => $self.execute_range($instr),
            Instr::IterateFirst
            | Instr::IterateNext
            | Instr::IterateFirstSplit
            | Instr::IterateNextSplit => $self.execute_iterator($instr),
            Instr::SleepF64 | Instr::SleepI64 => $self.execute_sleep($instr),
            Instr::PrintStr
            | Instr::PrintI64
            | Instr::PrintF64
            | Instr::PrintStrNoNewline
            | Instr::PrintI64NoNewline
            | Instr::PrintF64NoNewline
            | Instr::PrintAny
            | Instr::PrintAnyNoNewline
            | Instr::PrintNewline
            | Instr::IOPrintlnNewline => {
                let action = $self.execute_print($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::ThrowError
            | Instr::ThrowMethodError(..)
            | Instr::ThrowValue
            | Instr::PushExceptionValue
            | Instr::Test(..)
            | Instr::TestSetBegin(..)
            | Instr::TestSetEnd
            | Instr::TestThrowsBegin(..)
            | Instr::TestThrowsEnd
            | Instr::PushHandler(..)
            | Instr::PopHandler
            | Instr::ClearError
            | Instr::PushErrorCode
            | Instr::PushErrorMessage
            | Instr::Rethrow
            | Instr::RethrowCurrent
            | Instr::RethrowOther
            | Instr::PopCaughtException => $self.execute_error_handling($instr),
            Instr::NewStruct(..)
            | Instr::NewStructSplat(..)
            | Instr::NewParametricStruct(..)
            | Instr::NewDynamicParametricStruct(..)
            | Instr::ConstructParametricType(..)
            | Instr::ConstructParametricTypeSplat(..)
            | Instr::ApplyTypeDynamic(..)
            | Instr::LoadStruct(..)
            | Instr::StoreStruct(..)
            | Instr::GetField(..)
            | Instr::GetFieldByName(..)
            | Instr::GetExprField(..)
            | Instr::GetLineNumberNodeField(..)
            | Instr::GetQuoteNodeValue
            | Instr::GetGlobalRefField(..)
            | Instr::SetField(..)
            | Instr::SetFieldByName(..)
            | Instr::ReturnStruct => $self.execute_struct($instr),
            Instr::NewTuple(..)
            | Instr::MakeSimpleVector(..)
            | Instr::LoadTuple(..)
            | Instr::StoreTuple(..)
            | Instr::TupleGet
            | Instr::TupleUnpack(..)
            | Instr::TupleFirst
            | Instr::TupleSecond
            | Instr::ReturnTuple => $self.execute_tuple($instr),
            Instr::NewNamedTuple(..)
            | Instr::LoadNamedTuple(..)
            | Instr::StoreNamedTuple(..)
            | Instr::NamedTupleGetField(..)
            | Instr::NamedTupleGetIndex
            | Instr::NamedTupleGetBySymbol
            | Instr::ReturnNamedTuple => $self.execute_named_tuple($instr),
            Instr::NewPairs(..)
            | Instr::PairsGetBySymbol
            | Instr::PairsLength
            | Instr::PairsKeys
            | Instr::PairsValues => $self.execute_pairs($instr),
            Instr::DictSet
            | Instr::DictLen
            | Instr::LoadDict(..)
            | Instr::StoreDict(..)
            | Instr::ReturnDict => $self.execute_dict($instr),
            Instr::NewSet
            | Instr::NewSetTyped(..)
            | Instr::SetAdd
            | Instr::StoreSet(..)
            | Instr::LoadSet(..)
            | Instr::ReturnSet => $self.execute_set($instr),
            Instr::NtupleFunc(..)
            | Instr::NtupleRuntime
            | Instr::MakeGenerator(..)
            | Instr::MakeGeneratorRuntime(..)
            | Instr::WrapInGenerator
            | Instr::SprintFunc(..) => {
                let action = $self.execute_hof($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::NewArray(..)
            | Instr::PushArrayValue(..)
            | Instr::ReserveArray
            | Instr::PushElem
            | Instr::FinalizeArray(..)
            | Instr::NewArrayTyped(..)
            | Instr::PushElemTyped
            | Instr::FinalizeArrayTyped(..)
            | Instr::AllocUndefTyped(..)
            | Instr::AllocUndefTypedFromTuple(..)
            | Instr::AllocUndefDynamicTyped(..)
            | Instr::AllocUndefDynamicTypedFromTuple
            | Instr::LoadArray(..)
            | Instr::StoreArray(..) => $self.execute_array_basic($instr),
            Instr::IndexLoadTypedInbounds(..)
            | Instr::IndexLoadTyped(..)
            | Instr::IndexStoreTyped(..)
            | Instr::IndexLoad(..)
            | Instr::IndexSlice(..)
            | Instr::IndexLoadInbounds(..)
            | Instr::IndexStoreInbounds(..)
            | Instr::IndexStore(..) => $self.execute_array_index($instr),
            Instr::Zero
            | Instr::ArrayPush
            | Instr::ArrayPushTypejoin
            | Instr::ArrayPop
            | Instr::ArrayPushFirst
            | Instr::ArrayPopFirst
            | Instr::ArrayInsert
            | Instr::ArrayDeleteAt
            | Instr::ArrayDeleteAtIndices => $self.execute_array_mutate($instr),
            Instr::MatMul => $self.execute_matrix($instr),
            Instr::NewMemory(..)
            | Instr::NewMemoryDynamic(..)
            | Instr::NewMemoryDynamicTyped
            | Instr::MemoryGet
            | Instr::MemorySet
            | Instr::MemoryLength
            | Instr::LoadMemory(..)
            | Instr::StoreMemory(..)
            | Instr::ReturnMemory => $self.execute_memory($instr),
            Instr::TimeNs
            | Instr::SliceAll
            | Instr::NewStableRng
            | Instr::NewXoshiro
            | Instr::NewMersenne
            | Instr::LoadRng(..)
            | Instr::StoreRng(..)
            | Instr::RngRandF64
            | Instr::RngRandnF64
            | Instr::LoadRange(..)
            | Instr::StoreRange(..) => $self.execute_misc($instr),
            Instr::RngRandArrayF64(..)
            | Instr::RngRandArrayI64(..)
            | Instr::RngRandnArrayF64(..)
            | Instr::PushGlobalRng
            | Instr::RandnArg(..)
            | Instr::RandArg(..) => $self.execute_rng($instr),
            Instr::RangeCollect
            | Instr::RangeFirst
            | Instr::RangeLast
            | Instr::RangeGetIndex
            | Instr::Nop => Err(unhandled($instr)),
        }
    };
}

impl<R: RngLike> Vm<R> {
    /// Drive the interpreter loop synchronously until the current call frame
    /// returns to `target_depth` (the frame count that existed *before* the
    /// call being awaited was pushed). Returns the value the returning frame
    /// left on the stack.
    ///
    /// This is used to evaluate a function/constructor call started from a
    /// builtin handler (e.g. `eval`) that must produce a value synchronously
    /// rather than yielding to the top-level loop (Issue #4976). Unlike `run()`,
    /// it stops exactly when the awaited frame returns instead of continuing
    /// into whatever instruction the return jumped back to.
    pub(crate) fn run_until_frame_return(&mut self, target_depth: usize) -> Result<Value, VmError> {
        // Install the ancestor-handler floor (Issue #5972) for the lifetime of
        // this nested dispatch, restoring the enclosing dispatch's floor (or
        // `None` at the top level) on *every* exit path — including the `?`
        // early-returns inside the loop below. Saving/restoring (rather than
        // clearing to `None`) keeps nested `eval`s correct: each sees its own
        // deeper floor; an outer `eval` resumes with its shallower one.
        let saved_floor = self.eval_dispatch_floor;
        self.eval_dispatch_floor = Some(target_depth);
        let result = self.run_until_frame_return_inner(target_depth);
        self.eval_dispatch_floor = saved_floor;
        result
    }

    /// Inner driver loop for [`Self::run_until_frame_return`]; the public wrapper
    /// installs/restores the Issue #5972 ancestor-handler floor around it.
    fn run_until_frame_return_inner(&mut self, target_depth: usize) -> Result<Value, VmError> {
        // Snapshot the shared instruction slice (Issue #5177). As in `run()`,
        // the `Rc` clone lets us hold an immutable `&Instr` across the
        // `&mut self` dispatch without cloning each instruction. A re-entrant
        // `eval` dispatch (eval -> eval_dispatch_call -> run_until_frame_return)
        // on the same code slot reads the real instruction from the snapshot.
        let mut code = Rc::clone(&self.code);
        loop {
            let ip = self.ip;
            if self.next_executable_ip == ip {
                match self.try_execute_executable_block(ip) {
                    Ok(ExecutableBlockResult::Continue) => {
                        self.refresh_next_executable_ip_from(self.ip);
                        if self.frames.len() <= target_depth {
                            return self.stack.pop_value();
                        }
                        continue;
                    }
                    Ok(ExecutableBlockResult::NotExecuted) => {
                        self.refresh_next_executable_ip_from(ip + 1)
                    }
                    Ok(ExecutableBlockResult::Exit(val)) => {
                        return Ok(self.normalize_host_return_value(val));
                    }
                    Err(err) => {
                        self.last_error_ip = Some(ip);
                        return Err(err);
                    }
                }
            }
            self.ip += 1;

            let instr = &code[ip];
            super::profiler::record(instr);
            let result = self.dispatch_instr(instr);

            // Follow a `CallSpecialize` copy-on-write append (see `run()`).
            if !Rc::ptr_eq(&code, &self.code) {
                code = Rc::clone(&self.code);
            }

            match result {
                Ok(DispatchAction::Continue) => {
                    // The awaited frame returned once we are back at the target
                    // depth; the return instruction pushed its value on the
                    // stack and rewound `ip` to the caller.
                    if self.frames.len() <= target_depth {
                        return self.stack.pop_value();
                    }
                    if self.ip != ip + 1 {
                        self.refresh_next_executable_ip_from(self.ip);
                    }
                }
                Ok(DispatchAction::Exit(val)) => return Ok(self.normalize_host_return_value(val)),
                Err(err) => {
                    self.last_error_ip = Some(ip);
                    return Err(err);
                }
            }
        }
    }

    pub fn run(&mut self) -> Result<Value, VmError> {
        // Hold a cheap snapshot clone of the shared instruction slice (Issue
        // #5177). `Rc::clone` is a refcount bump, not a copy. Because this keeps
        // an immutable borrow that is independent of `&mut self`, the dispatch
        // loop can read `&code[ip]` and pass it to `dispatch_instr(&mut self,
        // ..)` without the per-cycle `mem::replace(.., Nop)` swap/restore the
        // old loop used purely to satisfy the borrow checker.
        //
        // The snapshot also makes the `eval` re-entrancy guard (Issue #5014)
        // unnecessary: a re-entrant `eval` dispatch executing on the same code
        // slot reads the real instruction from a never-blanked vector.
        let mut code = Rc::clone(&self.code);
        loop {
            let ip = self.ip;
            if self.next_executable_ip == ip {
                match self.try_execute_executable_block(ip) {
                    Ok(ExecutableBlockResult::Continue) => {
                        self.refresh_next_executable_ip_from(self.ip);
                        continue;
                    }
                    Ok(ExecutableBlockResult::NotExecuted) => {
                        self.refresh_next_executable_ip_from(ip + 1)
                    }
                    Ok(ExecutableBlockResult::Exit(val)) => {
                        return Ok(self.normalize_host_return_value(val));
                    }
                    Err(err) => {
                        self.last_error_ip = Some(ip);
                        return Err(err);
                    }
                }
            }
            self.ip += 1;

            // Immutable borrow of the snapshot; valid for the whole dispatch
            // even though `dispatch_instr` takes `&mut self`.
            let instr = &code[ip];

            // Profile instruction execution
            super::profiler::record(instr);

            // Debug: trace every instruction (comment out in production)
            #[cfg(debug_assertions)]
            if std::env::var("TRACE_INSTRS").is_ok() {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "VM: ip={}, instr={:?}", ip, instr);
            }

            // Dispatch instruction; all handler logic is in a separate method.
            let result = self.dispatch_instr(instr);

            // `CallSpecialize` may have appended bytecode to `self.code` via a
            // copy-on-write `Rc::make_mut`, leaving `self.code` pointing at a
            // fresh allocation while our snapshot still points at the old one.
            // Refresh the snapshot so the next fetch reads the live vector (the
            // jump into the specialized entry point lands in the new vector).
            // The common case (no append) is a single pointer comparison.
            if !Rc::ptr_eq(&code, &self.code) {
                code = Rc::clone(&self.code);
            }

            match result {
                Ok(DispatchAction::Continue) => {
                    if self.ip != ip + 1 {
                        self.refresh_next_executable_ip_from(self.ip);
                    }
                    continue;
                }
                Ok(DispatchAction::Exit(val)) => return Ok(self.normalize_host_return_value(val)),
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
    ///
    /// Single exhaustive match (Issue #6343): every `Instr` variant has an
    /// explicit arm routing to the handler module that owns it, so the
    /// compiler emits one jump table instead of the former 28-stage
    /// linear `NotHandled`-falls-through handler chain (Issue #5175 ordering).
    /// No wildcard arm: adding an `Instr` variant is a compile error here
    /// until it is routed.
    #[inline(always)]
    fn dispatch_instr(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        dispatch_instr_match!(self, instr)
    }

    /// Take a jump: cancellation is checked on backward jumps (loop
    /// back-edges, Issue #6342) before the instruction pointer moves.
    #[inline(always)]
    pub(super) fn jump_to(&mut self, target: usize) -> Result<DispatchAction, VmError> {
        if target < self.ip {
            self.check_cancel_boundary()?;
        }
        self.ip = target;
        Ok(DispatchAction::Continue)
    }

    /// Miscellaneous instructions formerly handled inline at the tail of the
    /// dispatch chain (RNG scalars, ranges-by-name, timing; Issue #6343).
    fn execute_misc(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
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
                self.stack.push(Value::Rng(RngInstance::stable(seed)));
            }
            Instr::NewXoshiro => {
                let seed = match self.stack.pop() {
                    Some(Value::I64(s)) => s as u64,
                    Some(Value::F64(s)) => s as u64,
                    _ => 0,
                };
                self.stack.push(Value::Rng(RngInstance::xoshiro(seed)));
            }
            // MersenneTwister(seed): constructible, deterministic MT19937-64
            // engine (NOT bit-identical to upstream's dSFMT — Issue #7306).
            Instr::NewMersenne => {
                let seed = match self.stack.pop() {
                    Some(Value::I64(s)) => s as u64,
                    Some(Value::F64(s)) => s as u64,
                    _ => 0,
                };
                self.stack.push(Value::Rng(RngInstance::mersenne(seed)));
            }
            Instr::LoadRng(name) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(Value::Rng(rng)) = self.load_slot_value_by_name(frame, name) {
                        self.stack.push(Value::Rng(rng));
                        return Ok(DispatchAction::Continue);
                    }
                    if let Some(Value::Rng(rng)) = frame.locals_any.get(name).cloned() {
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
                        if let Some(Value::Rng(rng)) = frame.locals_any.get(name).cloned() {
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
                        frame.locals_any.insert(name.clone(), Value::Rng(rng));
                        frame.var_types.insert(name.clone(), frame::VarTypeTag::Rng);
                    }
                }
            }
            Instr::RngRandF64 => {
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    // The global handle draws from the VM's own RNG so the
                    // stream is shared with bare rand() (Issue #7230).
                    let val = if matches!(rng, RngInstance::Global) {
                        self.rng.next_f64()
                    } else {
                        rng.next_f64()
                    };
                    self.stack.push(Value::F64(val));
                    self.stack.push(Value::Rng(rng));
                }
            }
            Instr::RngRandnF64 => {
                if let Some(Value::Rng(mut rng)) = self.stack.pop() {
                    let val = if matches!(rng, RngInstance::Global) {
                        randn(&mut self.rng)
                    } else {
                        randn(&mut rng)
                    };
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
                    if let Some(Value::Range(range)) = frame.locals_any.get(name).cloned() {
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
                        if let Some(Value::Range(range)) = frame.locals_any.get(name).cloned() {
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
                        frame.locals_any.insert(name.clone(), Value::Range(range));
                        frame
                            .var_types
                            .insert(name.clone(), frame::VarTypeTag::Range);
                    }
                }
            }
            _ => return Err(unhandled(instr)),
        }
        Ok(DispatchAction::Continue)
    }
}

/// Error for an `Instr` that reached a dispatch arm whose handler did not
/// recognize it (Issue #6343). Matches the former chain-tail catch-all.
#[cold]
pub(super) fn unhandled(instr: &Instr) -> VmError {
    VmError::NotImplemented(format!("Instruction not yet implemented: {:?}", instr))
}
