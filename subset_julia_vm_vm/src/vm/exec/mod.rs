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
pub(in crate::vm) mod error_handling;
pub(in crate::vm) mod exception_payload;
// Handler-function-pointer dispatch experiment (Issue #8562); compiled only
// under the `vm-handler-table` cargo feature so default builds are unchanged.
#[cfg(feature = "vm-handler-table")]
pub(in crate::vm) mod handler_table;
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
#[deny(clippy::match_same_arms)]
pub(in crate::vm) mod return_ops;
pub(in crate::vm) mod rng;
mod set;
mod sleep;
mod stack;
pub(in crate::vm) mod string_index;
mod string_ops;
mod struct_ops;
mod tuple;

use super::executable::ExecutableBlockResult;
use super::*;
pub(in crate::vm) use call::bind_kwargs_defaults;
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
            | Instr::PushStrBytes(..)
            | Instr::PushChar(..)
            | Instr::PushCharMalformed(..)
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
            | Instr::CreateResolvedClosure(..)
            | Instr::LoadCaptured(..)
            | Instr::DefineFunction(..)
            | Instr::DefineEvalFunction(..)
            | Instr::ActivateUsing { .. }
            | Instr::ActivateModule(..)
            | Instr::DefineEvalStruct(..)
            | Instr::DefineEvalAbstractType(..)
            | Instr::DefineEvalPrimitiveType(..)
            | Instr::DefineRuntimeNominal(..)
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
            | Instr::TakeSlot(..)
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
            | Instr::AddF64Slots(..)
            | Instr::AddF64I64Slots(..)
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
            | Instr::ProbeRuntimeBinding(..)
            | Instr::LoadGlobalAny(..)
            | Instr::LoadTypeBinding(..)
            | Instr::LoadValBool(..)
            | Instr::LoadValSymbol(..)
            | Instr::StoreAny(..)
            | Instr::StoreGlobalAny(..)
            | Instr::LoadAddI64(..)
            | Instr::LoadAddI64Slot(..)
            | Instr::LoadAddConstI64Slot(..)
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
            | Instr::IsDefined(..)
            | Instr::ForgetLetLocals(..)
            | Instr::EnterLexicalScope(..)
            | Instr::LoadLexical(..)
            | Instr::StoreLexical(..)
            | Instr::IsLexicalDefined(..)
            | Instr::ExitLexicalScope => $self.execute_locals($instr),
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
            | Instr::JumpIfNotGeF64(..)
            | Instr::JumpIfCmpI64SlotConst(..) => $self.execute_jump($instr),
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
            | Instr::CallSpecializeF64Slots(..)
            | Instr::CallSpecializeInboundsF64Slots(..)
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
            | Instr::CallParametricConstructorDispatch(..)
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
            | Instr::MakeStepRangeLazy
            | Instr::CoerceRangeStopI64 => $self.execute_range($instr),
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
            | Instr::ThrowUndefVarError(..)
            | Instr::RaiseUndefVarErrorIfFunctionInvisible(..)
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
            | Instr::ApplyTypeDynamicSplat(..)
            | Instr::LoadStruct(..)
            | Instr::StoreStruct(..)
            | Instr::GetField(..)
            | Instr::GetFieldByName(..)
            | Instr::GetExprField(..)
            | Instr::GetLineNumberNodeField(..)
            | Instr::GetQuoteNodeValue
            | Instr::GetGlobalRefField(..)
            | Instr::SetField(..)
            | Instr::SetFieldByName(..) => $self.execute_struct($instr),
            Instr::NewTuple(..)
            | Instr::MakeSimpleVector(..)
            | Instr::LoadTuple(..)
            | Instr::StoreTuple(..)
            | Instr::TupleGet
            | Instr::TupleUnpack(..)
            | Instr::TupleFirst
            | Instr::TupleSecond => $self.execute_tuple($instr),
            Instr::NewNamedTuple(..)
            | Instr::LoadNamedTuple(..)
            | Instr::StoreNamedTuple(..)
            | Instr::NamedTupleGetField(..)
            | Instr::NamedTupleGetIndex
            | Instr::NamedTupleGetBySymbol => $self.execute_named_tuple($instr),
            Instr::NewPairs(..)
            | Instr::PairsGetBySymbol
            | Instr::PairsLength
            | Instr::PairsKeys
            | Instr::PairsValues => $self.execute_pairs($instr),
            Instr::DictSet | Instr::DictLen | Instr::LoadDict(..) | Instr::StoreDict(..) => {
                $self.execute_dict($instr)
            }
            Instr::NewSet
            | Instr::NewSetTyped(..)
            | Instr::SetAdd
            | Instr::StoreSet(..)
            | Instr::LoadSet(..) => $self.execute_set($instr),
            // Container returns (Issue #10102 follow-up): these route a
            // returned struct/tuple/named-tuple/dict/set through the caller /
            // HOF / generator continuation machinery, so — exactly like the
            // scalar `Return*` arms above — they carry the
            // `handle_pending_call_depth_overflow` postlude that also runs the
            // memory-waterline safepoint. Without it, a per-element HOF /
            // generator driver whose callee returns a container (which does NOT
            // pass through `jump_to` and whose per-element entry bypasses the
            // `Call` postlude) could grow the struct heap between safepoints far
            // longer than a scalar-returning callee. Splitting these out of
            // their group arms keeps container returns symmetric with scalar
            // returns.
            Instr::ReturnStruct => {
                let action = $self.execute_struct($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::ReturnTuple => {
                let action = $self.execute_tuple($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::ReturnNamedTuple => {
                let action = $self.execute_named_tuple($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::ReturnDict => {
                let action = $self.execute_dict($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::ReturnSet => {
                let action = $self.execute_set($instr)?;
                $self.handle_pending_call_depth_overflow()?;
                Ok(action)
            }
            Instr::NtupleFunc(..)
            | Instr::NtupleRuntime
            | Instr::MakeGenerator(..)
            | Instr::MakeGeneratorRuntime(..)
            | Instr::MakeGeneratorRuntimeFiltered(..)
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
            | Instr::RngRandArrayTyped(..)
            | Instr::RandArrayTyped(..)
            | Instr::RngRandnArrayF64(..)
            | Instr::PushGlobalRng
            | Instr::RandnArg(..)
            | Instr::RandArg(..)
            | Instr::RandScalarTyped(..)
            | Instr::RngRandScalarTyped(..)
            | Instr::RngRandArg
            | Instr::RandMaybeRng { .. } => $self.execute_rng($instr),
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
                        if let Some(metrics) = self.stack_metrics.as_deref_mut() {
                            metrics.executable_block_runs += 1;
                        }
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
                        let mut val = self.normalize_host_return_value(val);
                        self.compact_struct_heap_at_safe_point_with_return(Some(&mut val));
                        return Ok(val);
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

            // Issue #10514: hot `CallSpecializeI64Slots` sites run the cached
            // predecoded scalar block directly from the main loop, bypassing the
            // general `execute_call` dispatch path.
            if let Instr::CallSpecializeI64Slots(operands)
            | Instr::CallSpecializeInboundsI64Slots(operands) = instr
            {
                if self.try_execute_i64_slot_specialize_main_loop_fast_path(operands)? {
                    if self.ip != ip + 1 {
                        self.refresh_next_executable_ip_from(self.ip);
                    }
                    if self.frames.len() <= target_depth {
                        return self.stack.pop_value();
                    }
                    continue;
                }
            }

            // Opt-in stack VM dispatch/high-water counters (Issue #8559);
            // one never-taken branch per instruction when disabled.
            if let Some(metrics) = self.stack_metrics.as_deref_mut() {
                metrics.record_dispatch(self.stack.len(), self.frames.len());
            }
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
                    // Safepoint now at back-edges + Call/Return boundaries
                    // (Issue #10102); see `jump_to` and `run()`.
                    if self.ip != ip + 1 {
                        self.refresh_next_executable_ip_from(self.ip);
                    }
                }
                Ok(DispatchAction::Exit(val)) => {
                    let mut val = self.normalize_host_return_value(val);
                    self.compact_struct_heap_at_safe_point_with_return(Some(&mut val));
                    return Ok(val);
                }
                Err(err) => {
                    self.last_error_ip = Some(ip);
                    return Err(err);
                }
            }
        }
    }

    pub fn run(&mut self) -> Result<Value, VmError> {
        // Issue #9198 S4: keep the bytecode crate's `type_id -> struct name`
        // registry in sync with this VM's `struct_defs` so values reconstructed
        // from unboxed inline-struct array storage (`StructInlineF64`) recover
        // their concrete name for `show`/`typeof`. Cheap (one clear + fill) and
        // idempotent across re-entrant `run()`; struct_defs are fixed per program.
        crate::vm::value::set_struct_name_registry(self.struct_defs.iter().map(|d| d.name.clone()));
        // Issue #11365: (re)seed the Main-scope type-visibility registry from
        // frame-0 state so display can decide bare-vs-`Main.M.B` qualification.
        // Seeding covers REPL-persisted and cache-restored bindings; `using`
        // statements executing during this run update it incrementally through
        // the global-store choke points.
        self.seed_main_scope_visibility();
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
                        if let Some(metrics) = self.stack_metrics.as_deref_mut() {
                            metrics.executable_block_runs += 1;
                        }
                        self.refresh_next_executable_ip_from(self.ip);
                        continue;
                    }
                    Ok(ExecutableBlockResult::NotExecuted) => {
                        self.refresh_next_executable_ip_from(ip + 1)
                    }
                    Ok(ExecutableBlockResult::Exit(val)) => {
                        // Zero-task/main-task execution pays one predictable
                        // branch and never enters the scheduler (Issue #10349).
                        if self.current_task_id != 0 && self.finish_current_task_and_switch()? {
                            code = Rc::clone(&self.code);
                            self.refresh_next_executable_ip_from(self.ip);
                            continue;
                        }
                        let val = self.normalize_host_return_value(val);
                        self.run_exit_finalizers()?;
                        return Ok(val);
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

            // Issue #10514: hot `CallSpecializeI64Slots` sites run the cached
            // predecoded scalar block directly from the main loop, bypassing the
            // general `execute_call` dispatch path.
            if let Instr::CallSpecializeI64Slots(operands)
            | Instr::CallSpecializeInboundsI64Slots(operands) = instr
            {
                if self.try_execute_i64_slot_specialize_main_loop_fast_path(operands)? {
                    if self.ip != ip + 1 {
                        self.refresh_next_executable_ip_from(self.ip);
                    }
                    continue;
                }
            }

            // Debug: trace every instruction (comment out in production)
            #[cfg(debug_assertions)]
            if std::env::var("TRACE_INSTRS").is_ok() {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "VM: ip={}, instr={:?}", ip, instr);
            }

            // Opt-in stack VM dispatch/high-water counters (Issue #8559);
            // one never-taken branch per instruction when disabled.
            if let Some(metrics) = self.stack_metrics.as_deref_mut() {
                metrics.record_dispatch(self.stack.len(), self.frames.len());
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
                    // Memory-waterline safepoint moved off the per-instruction
                    // path (Issue #10102): it now fires at loop back-edges
                    // (`jump_to`) and at `Call`/`Return` boundaries (via
                    // `handle_pending_call_depth_overflow`), removing one branch
                    // per loop-body instruction.
                    if self.ip != ip + 1 {
                        self.refresh_next_executable_ip_from(self.ip);
                    }
                    continue;
                }
                Ok(DispatchAction::Exit(val)) => {
                    // Keep the zero-task fast path to one predictable branch;
                    // scheduler state is cold until a child task exits (#10349).
                    if self.current_task_id != 0 && self.finish_current_task_and_switch()? {
                        code = Rc::clone(&self.code);
                        self.refresh_next_executable_ip_from(self.ip);
                        continue;
                    }
                    let val = self.normalize_host_return_value(val);
                    self.run_exit_finalizers()?;
                    return Ok(val);
                }
                Err(err) => {
                    // Store the IP of the failing instruction for span lookup (Issue #2856)
                    self.last_error_ip = Some(ip);
                    // Issue #10406: some instruction handlers propagate a
                    // catchable error with a bare `?` instead of `self.raise()`
                    // (e.g. the numeric fast paths' `value_to_f64` type check,
                    // and the per-element callee failures surfaced by HOFs like
                    // `map`/`sum`). Such an error reaches here with the enclosing
                    // `try`'s handler still installed but never consulted, so it
                    // aborted the program instead of being caught. Route it
                    // through the same handler machinery the instruction-level
                    // raise sites use (mirroring the `CallBuiltin`/`CallIntrinsic`
                    // arms). This is monotonic: `raise` catches only when a
                    // handler is installed and returns `Err` unchanged otherwise,
                    // so programs without a surrounding `try` are unaffected.
                    // Internal/host errors (`Cancelled`, `InternalError`, ...)
                    // stay uncatchable via `is_catchable_vm_error`.
                    if Self::is_catchable_vm_error(&err) {
                        self.raise(err)?;
                        self.check_memory_waterline_safepoint()?;
                        self.refresh_next_executable_ip_from(self.ip);
                        continue;
                    }
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
        // Handler-table dispatch experiment (Issue #8562): when this build
        // carries the `vm-handler-table` feature AND the per-Vm gate is
        // armed (`SJULIA_HANDLER_TABLE=1` / `set_handler_table_forced`),
        // dispatch through the function-pointer table instead of the match.
        // Default builds compile this branch away entirely; feature builds
        // with the gate off pay one `is_some` check per dispatch.
        #[cfg(feature = "vm-handler-table")]
        if self.handler_table.is_some() {
            return self.dispatch_instr_handler_table(instr);
        }
        dispatch_instr_match!(self, instr)
    }

    /// The exhaustive `match` dispatch path, re-entered by the handler
    /// table's fallback row (Issue #8562). Expands the same
    /// `dispatch_instr_match!` macro as `dispatch_instr`, so instructions
    /// outside the table's hot subset execute byte-for-byte the default
    /// path and the two dispatch mechanisms cannot diverge semantically.
    #[cfg(feature = "vm-handler-table")]
    pub(super) fn dispatch_instr_match_path(
        &mut self,
        instr: &Instr,
    ) -> Result<DispatchAction, VmError> {
        dispatch_instr_match!(self, instr)
    }

    /// Fast-path handler for `CallSpecializeI64Slots` / `CallSpecializeInboundsI64Slots`
    /// executed directly from the main interpreter loop (Issue #10514).
    ///
    /// When the specialized callee has already been predecoded into an
    /// `I64FunctionBlock`, we can run it here without routing through the
    /// `execute_call` match and the `execute_call_specialize_i64_slots` method.
    /// On success the instruction pointer is advanced past the call and any
    /// following compare-branch sequence is fused. On failure the caller falls
    /// back to the normal dispatch path.
    #[inline(always)]
    fn try_execute_i64_slot_specialize_main_loop_fast_path(
        &mut self,
        operands: &CallSpecializeSlots,
    ) -> Result<bool, VmError> {
        let entry = match self
            .specialization_i64_fast_cache
            .get(operands.spec_func_index)
        {
            Some(Some(entry)) if entry.arity == operands.slots.len() => entry,
            _ => return Ok(false),
        };
        let block = match entry.predecoded.as_ref().and_then(|b| b.as_ref()) {
            Some(block) => block,
            None => return Ok(false),
        };

        let mut args_buf = [0_i64; 8];
        let slots = &operands.slots;
        let args = if slots.len() <= 8 {
            match self.load_i64_slot_specialize_values_into(slots, &mut args_buf)? {
                Some(()) => &args_buf[..slots.len()],
                None => return Ok(false),
            }
        } else {
            return Ok(false);
        };

        let Some(value) = Self::execute_i64_function_block(block, args) else {
            return Ok(false);
        };
        crate::vm::profiler::record_event("SpecializeI64DispatchCacheHit");

        if self.try_consume_i64_eq_branch(value) {
            return Ok(true);
        }
        self.stack.push(Value::I64(value));
        Ok(true)
    }

    /// Take a jump: cancellation and the memory-waterline safepoint are checked
    /// on backward jumps (loop back-edges) only.
    ///
    /// Cancellation is checked before the instruction pointer moves (Issue
    /// #6342). The struct-heap memory-waterline safepoint (Issue #10102) is
    /// checked *after* the pointer moves to `target`, mirroring the former
    /// per-instruction placement in `run()` — so if the safepoint raises
    /// `OutOfMemory` into a catch handler, the handler's `ip` (set by `raise`)
    /// wins over the loop back-edge target. Moving this check off the
    /// per-instruction path removes one branch per loop-body instruction; the
    /// waterline does not need immediate reaction, and lagging until the loop
    /// back-edge (or the next `Call`/`Return` boundary, which already carries
    /// the check via `handle_pending_call_depth_overflow`) is sufficient to
    /// trigger compaction before a struct-heavy loop blows the soft limit.
    #[inline(always)]
    pub(super) fn jump_to(&mut self, target: usize) -> Result<DispatchAction, VmError> {
        let is_back_edge = target < self.ip;
        if is_back_edge {
            self.check_cancel_boundary()?;
        }
        self.ip = target;
        if is_back_edge {
            self.check_memory_waterline_safepoint()?;
        }
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

/// Internal error for a compiler-generated slot index that is out of bounds.
///
/// Slot indices are produced by the compiler; an out-of-bounds index means the
/// compiler emitted an invalid slot, which is a VM-internal invariant violation
/// rather than a user error. Consolidates the ~50 byte-identical
/// `"<Instr>: slot out of bounds: {slot}"` `InternalError` sites across the
/// slot load/store/jump/call handlers (Issue #10260). `slot` is taken as
/// `impl Display` so the handlers can pass `slot`/`*slot`/`dst` unchanged and
/// the rendered message stays byte-identical to the previous inline `format!`.
#[cold]
pub(super) fn slot_out_of_bounds(instr: &str, slot: impl std::fmt::Display) -> VmError {
    VmError::InternalError(format!("{}: slot out of bounds: {}", instr, slot))
}
