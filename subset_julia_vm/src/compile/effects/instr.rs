//! VM instruction effect attribution for local optimizers.
//!
//! The table is intentionally conservative: unknown, allocating, dispatching,
//! mutating, time/RNG, and IO instructions are optimization barriers. Arithmetic
//! and simple projections are marked precisely enough for LICM/CSE candidates.

use super::Effects;
use crate::builtins::BuiltinId;
use crate::vm::Instr;

/// Return the conservative effect summary for one VM instruction.
pub fn instruction_effects(instr: &Instr) -> Effects {
    if is_pure_total_instruction(instr) {
        return Effects::pure_arithmetic();
    }

    if is_effect_free_may_throw_instruction(instr) {
        return Effects::effect_free_may_throw();
    }

    if is_index_read_instruction(instr) {
        return Effects::array_getindex();
    }

    if is_mutating_instruction(instr) {
        return Effects::with_side_effects();
    }

    match instr {
        Instr::CallBuiltin(builtin, _) => builtin_effects(*builtin),
        Instr::CallIntrinsic(_) => Effects::effect_free_may_throw(),
        Instr::Nop => Effects::total(),
        instr if is_control_transfer_instruction(instr) => Effects::total(),
        _ => Effects::arbitrary(),
    }
}

/// Return true when an instruction can be hoisted out of a loop without changing
/// exception timing, state, or observable results.
pub fn instruction_can_hoist(instr: &Instr) -> bool {
    instruction_effects(instr).is_pure() && !instruction_is_cse_barrier(instr)
}

/// Return true when an instruction is a conservative CSE candidate.
pub fn instruction_can_cse(instr: &Instr) -> bool {
    let effects = instruction_effects(instr);
    effects.is_pure() && !instruction_is_cse_barrier(instr)
}

/// Return true when CSE value-numbering should stop at this instruction.
pub fn instruction_is_cse_barrier(instr: &Instr) -> bool {
    is_control_transfer_instruction(instr)
        || is_mutating_instruction(instr)
        || is_allocating_instruction(instr)
        || matches!(
            instr,
            Instr::Call(_, _)
                | Instr::CallInbounds(_, _)
                | Instr::CallWithKwargs(_, _, _)
                | Instr::CallWithKwargsSplat(_, _, _, _)
                | Instr::CallWithSplat(_, _, _)
                | Instr::CallDynamic(_, _, _)
                | Instr::CallDynamicBinary(_, _, _)
                | Instr::CallDynamicBinaryBoth(_, _)
                | Instr::CallDynamicBinaryNoFallback(_)
                | Instr::CallDynamicOrBuiltin(_, _)
                | Instr::CallTypedDispatchOrBuiltin(_, _, _, _)
                | Instr::CallTypedDispatchOrBuiltinResult(_, _, _, _)
                | Instr::CallTypedDispatchOrBuiltinStoreDict(_)
                | Instr::CallTypedDispatchOrBuiltinStoreDictResult(_)
                | Instr::IterateDynamic(_, _)
                | Instr::CallTypedDispatch(_, _, _, _)
                | Instr::CallTypeConstructor
                | Instr::CallGlobalRef(_)
                | Instr::CallFunctionVariable(_)
                | Instr::InvokeFunctionVariable(_, _)
                | Instr::InvokeFunctionVariableWithKwargs(_)
                | Instr::InvokeFunctionVariableDynamicSignature(_)
                | Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(_, _, _)
                | Instr::CallFunctionVariableWithSplat(_, _)
                | Instr::CallFunctionVariableWithKwargsSplat(_)
                | Instr::CallSpecialize(_, _)
                | Instr::CallSpecializeInbounds(_, _)
                | Instr::CallSpecializeI64Slots(_)
                | Instr::CallSpecializeInboundsI64Slots(_)
                | Instr::CallResolvedI64Slots(_)
                | Instr::CallInboundsI64Slots(_)
                | Instr::NtupleFunc(_)
                | Instr::NtupleRuntime
                | Instr::SprintFunc(_, _)
                | Instr::CallBuiltin(_, _)
                | Instr::CallIntrinsic(_)
        )
        || !instruction_effects(instr).effect_free.is_always_true()
}

fn builtin_effects(builtin: BuiltinId) -> Effects {
    if builtin.is_pure_math() {
        Effects::pure_arithmetic()
    } else if builtin.has_side_effects() {
        Effects::with_side_effects()
    } else {
        Effects::arbitrary()
    }
}

fn is_pure_total_instruction(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::PushI64(_)
            | Instr::PushI128(_)
            | Instr::PushF64(_)
            | Instr::PushF32(_)
            | Instr::PushF16(_)
            | Instr::PushBool(_)
            | Instr::PushNothing
            | Instr::PushMissing
            | Instr::PushUndef
            | Instr::PushCNull
            | Instr::PushBoundsCheckEnabled
            | Instr::PushChar(_)
            | Instr::AddI64
            | Instr::SubI64
            | Instr::MulI64
            | Instr::IncI64
            | Instr::NegI64
            | Instr::AddF64
            | Instr::SubF64
            | Instr::MulF64
            | Instr::DivF64
            | Instr::SqrtF64
            | Instr::FloorF64
            | Instr::CeilF64
            | Instr::AbsF64
            | Instr::Abs2F64
            | Instr::PowF64
            | Instr::NegF64
            | Instr::GtI64
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
            | Instr::EqStr
            | Instr::LtStr
            | Instr::LeStr
            | Instr::GtStr
            | Instr::GeStr
            | Instr::BoolToI64
            | Instr::I64ToBool
            | Instr::NotBool
            | Instr::ToF64
            | Instr::ToI64
            | Instr::DynamicToI8
            | Instr::DynamicToI16
            | Instr::DynamicToI32
            | Instr::DynamicToU8
            | Instr::DynamicToU16
            | Instr::DynamicToU32
            | Instr::DynamicToU64
            | Instr::SelectI64
            | Instr::SelectF64
            | Instr::IsNothing
            | Instr::TupleFirst
            | Instr::TupleSecond
            | Instr::PairsLength
            | Instr::Zero
            | Instr::SliceAll
            | Instr::DupI64
            | Instr::DupF64
            | Instr::Dup
            | Instr::Swap
    )
}

fn is_effect_free_may_throw_instruction(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::PushBigInt(_)
            | Instr::PushBigFloat(_)
            | Instr::PushStr(_)
            | Instr::PushSymbol(_)
            | Instr::PushDataType(_)
            | Instr::PushFunction(_)
            | Instr::PushResolvedFunction(_)
            | Instr::PushStdout
            | Instr::PushStderr
            | Instr::PushStdin
            | Instr::PushDevnull
            | Instr::LoadStr(_)
            | Instr::LoadI64(_)
            | Instr::LoadF64(_)
            | Instr::LoadF32(_)
            | Instr::LoadF16(_)
            | Instr::LoadBool(_)
            | Instr::LoadSlot(_)
            | Instr::LoadSlotI64(_)
            | Instr::LoadSlotI64ToF64(_)
            | Instr::LoadSlotF64(_)
            | Instr::LoadSlotBool(_)
            | Instr::LoadAny(_)
            | Instr::LoadGlobalAny(_)
            | Instr::LoadTypeBinding(_)
            | Instr::LoadValBool(_)
            | Instr::LoadValSymbol(_)
            | Instr::LoadCaptured(_)
            | Instr::LoadAddI64(_)
            | Instr::LoadSubI64(_)
            | Instr::LoadMulI64(_)
            | Instr::LoadModI64(_)
            | Instr::LoadAddI64Slot(_)
            | Instr::LoadSubI64Slot(_)
            | Instr::LoadMulI64Slot(_)
            | Instr::LoadModI64Slot(_)
            | Instr::LoadSquareF64Slot(_)
            | Instr::LoadAddF64Slot(_)
            | Instr::LoadSubF64Slot(_)
            | Instr::LoadMulF64Slot(_)
            | Instr::LoadDivF64Slot(_)
            | Instr::LoadArray(_)
            | Instr::LoadRange(_)
            | Instr::LoadStruct(_)
            | Instr::LoadTuple(_)
            | Instr::LoadNamedTuple(_)
            | Instr::LoadDict(_)
            | Instr::LoadSet(_)
            | Instr::LoadRng(_)
            | Instr::LoadMemory(_)
            | Instr::DynamicToF64
            | Instr::DynamicToF32
            | Instr::DynamicToF16
            | Instr::DynamicToI64
            | Instr::DynamicToBool
            | Instr::ModI64
            | Instr::DynamicAdd
            | Instr::DynamicSub
            | Instr::DynamicMul
            | Instr::DynamicDiv
            | Instr::DynamicMod
            | Instr::DynamicIntDiv
            | Instr::DynamicNeg
            | Instr::DynamicPow
            | Instr::EqStruct
            | Instr::GetField(_)
            | Instr::GetFieldByName(_)
            | Instr::GetExprField(_)
            | Instr::GetLineNumberNodeField(_)
            | Instr::GetQuoteNodeValue
            | Instr::GetGlobalRefField(_)
            | Instr::TupleGet
            | Instr::NamedTupleGetField(_)
            | Instr::NamedTupleGetIndex
            | Instr::NamedTupleGetBySymbol
            | Instr::PairsGetBySymbol
            | Instr::PairsKeys
            | Instr::PairsValues
            | Instr::DictLen
            | Instr::MemoryLength
            | Instr::IsDefined(_)
            | Instr::ToString
            | Instr::ToStr
            | Instr::StringConcat(_)
            | Instr::ConcatStrings(_)
            | Instr::UnwrapRef
    )
}

fn is_index_read_instruction(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::IndexLoad(_)
            | Instr::IndexLoadTyped(_)
            | Instr::IndexSlice(_)
            | Instr::RangeFirst
            | Instr::RangeLast
            | Instr::RangeGetIndex
            | Instr::MemoryGet
            | Instr::IterateFirst
            | Instr::IterateNext
            | Instr::IterateFirstSplit
            | Instr::IterateNextSplit
    )
}

fn is_mutating_instruction(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::StoreStr(_)
            | Instr::StoreI64(_)
            | Instr::StoreF64(_)
            | Instr::StoreF32(_)
            | Instr::StoreF16(_)
            | Instr::StoreBool(_)
            | Instr::StoreSlot(_)
            | Instr::StoreSlotI64(_)
            | Instr::StoreSlotF64(_)
            | Instr::StoreSlotBool(_)
            | Instr::StoreAny(_)
            | Instr::IncVarI64(_)
            | Instr::DecVarI64(_)
            | Instr::IncVarI64Slot(_)
            | Instr::DecVarI64Slot(_)
            | Instr::AddConstI64Slot(_, _)
            | Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _)
            | Instr::IndexStore(_)
            | Instr::IndexStoreTyped(_)
            | Instr::ArrayPush
            | Instr::ArrayPushTypejoin
            | Instr::ReserveArray
            | Instr::ArrayPop
            | Instr::ArrayPushFirst
            | Instr::ArrayPopFirst
            | Instr::ArrayInsert
            | Instr::ArrayDeleteAt
            | Instr::ArrayDeleteAtIndices
            | Instr::PushElem
            | Instr::PushElemTyped
            | Instr::DictSet
            | Instr::SetAdd
            | Instr::StoreArray(_)
            | Instr::StoreRange(_)
            | Instr::StoreStruct(_)
            | Instr::StoreTuple(_)
            | Instr::StoreNamedTuple(_)
            | Instr::StoreDict(_)
            | Instr::StoreSet(_)
            | Instr::StoreRng(_)
            | Instr::StoreMemory(_)
            | Instr::SetField(_)
            | Instr::SetFieldByName(_)
            | Instr::MemorySet
            | Instr::RngRandF64
            | Instr::RngRandArrayF64(_)
            | Instr::RngRandArrayI64(_)
            | Instr::RngRandnF64
            | Instr::RngRandnArrayF64(_)
            | Instr::SeedGlobalRng
            | Instr::PrintStr
            | Instr::PrintI64
            | Instr::PrintF64
            | Instr::PrintStrNoNewline
            | Instr::PrintI64NoNewline
            | Instr::PrintF64NoNewline
            | Instr::PrintAny
            | Instr::PrintAnyNoNewline
            | Instr::PrintNewline
            | Instr::IOPrintlnNewline
            | Instr::SleepF64
            | Instr::SleepI64
            | Instr::ThrowError
            | Instr::ThrowValue
            | Instr::Rethrow
            | Instr::RethrowCurrent
            | Instr::RethrowOther
            | Instr::Test(_)
            | Instr::TestSetBegin(_)
            | Instr::TestSetEnd
            | Instr::TestThrowsBegin(_)
            | Instr::TestThrowsEnd
            | Instr::PushHandler(_, _)
            | Instr::PopHandler
            | Instr::ClearError
            | Instr::PushExceptionValue
            | Instr::PushErrorCode
            | Instr::PushErrorMessage
    )
}

fn is_allocating_instruction(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::PushEnv
            | Instr::PushModule(_)
            | Instr::CreateClosure { .. }
            | Instr::DefineFunction(_)
            | Instr::DefineEvalFunction(_)
            | Instr::CreateExpr { .. }
            | Instr::CreateQuoteNode
            | Instr::PushLineNumberNode { .. }
            | Instr::PushRegex { .. }
            | Instr::PushEnum { .. }
            | Instr::NewArray(_)
            | Instr::FinalizeArray(_)
            | Instr::PushArrayValue(_)
            | Instr::NewArrayTyped(_, _)
            | Instr::FinalizeArrayTyped(_)
            | Instr::AllocUndefTyped(_, _)
            | Instr::AllocUndefTypedFromTuple(_)
            | Instr::AllocUndefDynamicTyped(_)
            | Instr::AllocUndefDynamicTypedFromTuple
            | Instr::MatMul
            | Instr::MakeRange
            | Instr::MakeRangeF64
            | Instr::MakeRangeLazy
            | Instr::MakeStepRangeLazy
            | Instr::RangeCollect
            | Instr::NewStruct(_, _)
            | Instr::NewStructSplat(_)
            | Instr::NewParametricStruct(_, _)
            | Instr::NewDynamicParametricStruct(_, _, _)
            | Instr::ConstructParametricType(_, _)
            | Instr::NewStableRng
            | Instr::NewXoshiro
            | Instr::NewMersenne
            | Instr::MakeSimpleVector(_)
            | Instr::NewTuple(_)
            | Instr::TupleUnpack(_)
            | Instr::NewNamedTuple(_)
            | Instr::NewPairs(_)
            | Instr::NewSet
            | Instr::NewSetTyped(_)
            | Instr::MakeRef
            | Instr::NewMemory(_, _)
            | Instr::NewMemoryDynamic(_)
            | Instr::NewMemoryDynamicTyped
            | Instr::MakeGenerator(_)
            | Instr::MakeGeneratorRuntime(_, _)
            | Instr::WrapInGenerator
            | Instr::RandF64
            | Instr::RandArray(_)
            | Instr::RandIntArray(_)
            | Instr::RandnF64
            | Instr::RandnArray(_)
            | Instr::TimeNs
    )
}

fn is_control_transfer_instruction(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::Jump(_)
            | Instr::JumpIfZero(_)
            | Instr::JumpIfNeI64(_)
            | Instr::JumpIfEqI64(_)
            | Instr::JumpIfLtI64(_)
            | Instr::JumpIfGtI64(_)
            | Instr::JumpIfGtI64Slots(_, _, _)
            | Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _)
            | Instr::JumpIfLeI64(_)
            | Instr::JumpIfGeI64(_)
            | Instr::JumpIfEqF64(_)
            | Instr::JumpIfNeF64(_)
            | Instr::JumpIfNotLtF64(_)
            | Instr::JumpIfNotGtF64(_)
            | Instr::JumpIfNotLeF64(_)
            | Instr::JumpIfNotGeF64(_)
            | Instr::ReturnI64
            | Instr::ReturnF64
            | Instr::ReturnF32
            | Instr::ReturnF16
            | Instr::ReturnArray
            | Instr::ReturnNothing
            | Instr::ReturnAny
            | Instr::ReturnRange
            | Instr::ReturnStruct
            | Instr::ReturnRng
            | Instr::ReturnTuple
            | Instr::ReturnNamedTuple
            | Instr::ReturnDict
            | Instr::ReturnSet
            | Instr::ReturnRef
            | Instr::ReturnMemory
    )
}

#[cfg(test)]
mod tests {
    use super::super::EffectBit;
    use super::*;

    #[test]
    fn instruction_effects_marks_arithmetic_hoistable_issue_5185() {
        let effects = instruction_effects(&Instr::AddI64);

        assert!(effects.is_pure());
        assert!(instruction_can_hoist(&Instr::AddI64));
        assert!(instruction_can_cse(&Instr::AddI64));
        assert!(!instruction_is_cse_barrier(&Instr::AddI64));
    }

    #[test]
    fn instruction_effects_keeps_bounds_checked_reads_non_hoistable_issue_5185() {
        let effects = instruction_effects(&Instr::IndexLoad(1));

        assert!(effects.consistent.is_always_true());
        assert!(effects.effect_free.is_always_true());
        assert!(!effects.nothrow);
        assert!(!instruction_can_hoist(&Instr::IndexLoad(1)));
        assert!(!instruction_can_cse(&Instr::IndexLoad(1)));
        assert!(!instruction_is_cse_barrier(&Instr::IndexLoad(1)));
    }

    #[test]
    fn instruction_effects_marks_stores_and_io_as_barriers_issue_5185() {
        for instr in [Instr::StoreI64("x".to_string()), Instr::PrintStr] {
            let effects = instruction_effects(&instr);

            assert!(effects.effect_free.is_always_false());
            assert!(instruction_is_cse_barrier(&instr));
            assert!(!instruction_can_hoist(&instr));
            assert!(!instruction_can_cse(&instr));
        }
    }

    #[test]
    fn instruction_effects_uses_builtin_purity_issue_5185() {
        let pure = instruction_effects(&Instr::CallBuiltin(BuiltinId::Round, 1));
        let side_effect = instruction_effects(&Instr::CallBuiltin(BuiltinId::Println, 1));

        assert!(pure.is_pure());
        assert!(side_effect.effect_free.is_always_false());
        assert!(instruction_is_cse_barrier(&Instr::CallBuiltin(
            BuiltinId::Println,
            1
        )));
    }

    #[test]
    fn instruction_effects_marks_dispatch_and_control_as_barriers_issue_5185() {
        assert!(instruction_effects(&Instr::CallDynamic(0, 1, Vec::new()))
            .effect_free
            .is_always_false());
        assert!(instruction_is_cse_barrier(&Instr::CallDynamic(
            0,
            1,
            Vec::new()
        )));
        assert!(instruction_is_cse_barrier(&Instr::Jump(3)));
        assert!(!instruction_effects(&Instr::Jump(3))
            .effect_free
            .is_always_false());
    }

    #[test]
    fn instruction_effects_marks_allocations_as_barriers_issue_5185() {
        let instr = Instr::NewArray(2);

        assert!(instruction_is_cse_barrier(&instr));
        assert!(!instruction_can_hoist(&instr));
        assert!(!instruction_can_cse(&instr));
    }

    #[test]
    fn instruction_effects_exposes_effect_bits_issue_5185() {
        assert_eq!(
            instruction_effects(&Instr::IndexStore(1)).effect_free,
            EffectBit::AlwaysFalse
        );
    }
}
