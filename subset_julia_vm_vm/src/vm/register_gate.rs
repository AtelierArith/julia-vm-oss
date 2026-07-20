//! `SJULIA_REGISTER_VM=1` gate for the register VM prototype (Issue #8558).
//!
//! When the env var is set at `Vm` construction time, eligible direct calls
//! (`Call` / `CallInbounds` / `CallResolved`) execute their function body on
//! the side-by-side register VM (`crate::register_vm`); everything else stays
//! on the production stack VM. Per-function translations are memoized (an
//! untranslatable function is cached as ineligible and never retried), and
//! `SJULIA_REGISTER_VM_LOG=1` logs which path each function took.
//!
//! # Call boundary
//!
//! Calls made *inside* a register-executed body first ask the host for a
//! register-native callee frame. Translatable callees run on the register
//! interpreter's explicit frame stack, so recursive register calls do not
//! recurse on the host Rust stack. Calls that cannot be translated still use
//! `RegisterVmHost::call_function`, which starts a regular stack VM frame and
//! drives `run_until_frame_return` until the callee returns, using the same
//! nested-dispatch discipline as `eval_dispatch_call` (Issue #4976 / #5972 /
//! #7687): the `eval_dispatch_floor` keeps errors from being caught by ancestor
//! handlers mid-nested-loop, and error paths unwind leftover frames/stack/
//! handler state.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::intrinsics::Intrinsic;
use crate::register_vm::{
    execute_register_program, RegisterCallFrame, RegisterProgram, RegisterVmHost,
};
use crate::rng::RngLike;

use super::error::VmError;
use super::exec::DispatchAction;
use super::stack_ops::{StackOps, StackOpsExt};
use super::value::Value;
use super::Vm;
use subset_julia_vm_bytecode::{Instr, ValueType, VarTypeTag};

/// `SJULIA_REGISTER_VM_LOG` diagnostics sink. The crate denies
/// `clippy::print_stderr`; like the `TRACE_INSTRS` tracing in `exec/mod.rs`,
/// explicit opt-in debug logging writes through `std::io::stderr()` directly.
macro_rules! gate_log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

/// Maximum nesting of register VM executions live on the host Rust stack.
///
/// Native register-to-register calls use an explicit heap frame stack and do
/// not consume this budget. The cap still protects re-entrant register
/// executions that are reached through stack-VM fallback trampolines.
pub(crate) const MAX_REGISTER_VM_NESTING: usize = 64;

/// Process-wide gate override for hosts where environment variables are not
/// practical: wasm32-unknown-unknown has no real environment, and the iOS
/// measurement harness prefers an API call over `simctl` env plumbing
/// (Issue #8559). OR-ed with `SJULIA_REGISTER_VM` at `Vm` construction time.
static FORCED: AtomicBool = AtomicBool::new(false);

/// Force the register VM gate on/off for subsequently constructed `Vm`s,
/// regardless of the `SJULIA_REGISTER_VM` environment variable (Issue #8559).
pub fn set_register_vm_forced(enabled: bool) {
    FORCED.store(enabled, Ordering::Relaxed);
}

fn register_supported_value_type(ty: &ValueType) -> bool {
    matches!(
        ty,
        ValueType::I64 | ValueType::F64 | ValueType::Bool | ValueType::Nothing | ValueType::Any
    )
}

fn register_supported_slot_type(tag: VarTypeTag) -> bool {
    matches!(
        tag,
        VarTypeTag::I64
            | VarTypeTag::F64
            | VarTypeTag::Bool
            | VarTypeTag::Nothing
            | VarTypeTag::Any
    )
}

/// Register-gate support-matrix classifier for stack `Instr` variants
/// (Issue #10060, prevention for #10047/#10054).
///
/// `function_has_register_unsupported_stack_markers` scans a function's stack
/// bytecode for remnants whose semantics the register interpreter does not
/// implement (dynamic dispatch, builtin/specialized calls, dynamic numeric
/// conversion boxing, global load/store). This match is deliberately
/// **exhaustive — never add a `_` arm**: adding a new `Instr` variant is a
/// compile error here until it is explicitly classified, so the register
/// gate cannot silently widen its accepted subset when new dynamic call /
/// conversion / global marker variants land. When classifying a new variant,
/// follow the "RegisterVM Feature Work Checklist" in `docs/vm/CHECKLISTS.md`
/// (positive register execution test + negative stack-only test).
fn instr_is_register_unsupported_stack_marker(instr: &Instr) -> bool {
    match instr {
        // Register-unsupported stack markers: the function still relies on
        // stack-VM-owned semantics, so the whole function stays on the stack
        // VM (`register_program_for` rejects it before translation).
        Instr::DynamicToF64
        | Instr::LoadGlobalAny(..)
        | Instr::StoreGlobalAny(..)
        | Instr::Call(..)
        | Instr::CallBuiltin(..)
        | Instr::CallSpecialize(..)
        | Instr::CallSpecializeInbounds(..)
        | Instr::DynamicToF32
        | Instr::DynamicToF16
        | Instr::DynamicToI64
        | Instr::DynamicToBool
        | Instr::DynamicToI8
        | Instr::DynamicToI16
        | Instr::DynamicToI32
        | Instr::DynamicToU8
        | Instr::DynamicToU16
        | Instr::DynamicToU32
        | Instr::DynamicToU64
        | Instr::CallDynamic(..)
        | Instr::CallDynamicBinary(..)
        | Instr::CallDynamicBinaryBoth(..)
        | Instr::CallDynamicBinaryNoFallback(..)
        | Instr::CallDynamicOrBuiltin(..)
        // Module/main lexical environments are owned by the stack VM. Keep
        // every marker fail-closed until the register VM has an explicit
        // task-local lexical-environment model (Issues #11569/#9784).
        | Instr::EnterLexicalScope(..)
        | Instr::LoadLexical(..)
        | Instr::StoreLexical(..)
        | Instr::IsLexicalDefined(..)
        | Instr::ExitLexicalScope
        // Destructive slot load (Issue #10107) is a stack-VM-only rewrite; the
        // register VM has no `TakeSlot` translation, so a function containing
        // one stays on the stack VM. These appear only in trivial
        // `LoadSlot*; Return*` leaf accessors, where the register VM offers no
        // benefit — register support is deferred.
        | Instr::TakeSlot(..) => true,

        // Gate-neutral variants: either translatable from the shared SSA plan,
        // rejected later by `RegisterProgram::from_shared_plan_with_context`,
        // or unreachable in gate-eligible function bodies. Their presence
        // alone does not prove register-unsupported semantics.
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
        | Instr::PushSymbol(..)
        | Instr::CreateExpr { .. }
        | Instr::CreateQuoteNode
        | Instr::PushLineNumberNode { .. }
        | Instr::PushRegex { .. }
        | Instr::PushEnum { .. }
        | Instr::LoadStr(..)
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
        | Instr::LoadSlotI64(..)
        | Instr::StoreSlotI64(..)
        | Instr::LoadSlotF64(..)
        | Instr::StoreSlotF64(..)
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
        | Instr::LoadSlotNarrowInt(..)
        | Instr::StoreSlotNarrowInt(..)
        | Instr::LoadSlotNothing(..)
        | Instr::StoreSlotNothing(..)
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
        | Instr::LoadAny(..)
        | Instr::ProbeRuntimeBinding(..)
        | Instr::StoreAny(..)
        | Instr::LoadTypeBinding(..)
        | Instr::LoadValBool(..)
        | Instr::LoadValSymbol(..)
        | Instr::DynamicAdd
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
        | Instr::DupI64
        | Instr::Dup
        | Instr::NegI64
        | Instr::LoadAddI64(..)
        | Instr::LoadSubI64(..)
        | Instr::LoadMulI64(..)
        | Instr::LoadModI64(..)
        | Instr::LoadAddI64Slot(..)
        | Instr::LoadSubI64Slot(..)
        | Instr::LoadMulI64Slot(..)
        | Instr::LoadModI64Slot(..)
        | Instr::IncVarI64(..)
        | Instr::DecVarI64(..)
        | Instr::IncVarI64Slot(..)
        | Instr::DecVarI64Slot(..)
        | Instr::JumpIfNeI64(..)
        | Instr::JumpIfEqI64(..)
        | Instr::JumpIfLtI64(..)
        | Instr::JumpIfGtI64(..)
        | Instr::JumpIfLeI64(..)
        | Instr::JumpIfGeI64(..)
        | Instr::GtI64
        | Instr::LtI64
        | Instr::LeI64
        | Instr::GeI64
        | Instr::EqI64
        | Instr::NeI64
        | Instr::ToF64
        | Instr::ToI64
        | Instr::BoolToI64
        | Instr::I64ToBool
        | Instr::NotBool
        | Instr::AddF64
        | Instr::SubF64
        | Instr::MulF64
        | Instr::DupF64
        | Instr::DivF64
        | Instr::SqrtF64
        | Instr::FloorF64
        | Instr::CeilF64
        | Instr::AbsF64
        | Instr::Abs2F64
        | Instr::SleepF64
        | Instr::SleepI64
        | Instr::PowF64
        | Instr::NegF64
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
        | Instr::SelectF64
        | Instr::RandF64
        | Instr::RandArray(..)
        | Instr::RandIntArray(..)
        | Instr::RandnF64
        | Instr::RandnArray(..)
        | Instr::SeedGlobalRng
        | Instr::Jump(..)
        | Instr::JumpIfZero(..)
        | Instr::CallInbounds(..)
        | Instr::CallWithKwargs(..)
        | Instr::CallWithKwargsSplat(..)
        | Instr::CallWithSplat(..)
        | Instr::CallIntrinsic(..)
        | Instr::CallTypedDispatchOrBuiltin(..)
        | Instr::CallTypedDispatchOrBuiltinResult(..)
        | Instr::CallTypedDispatchOrBuiltinStoreDict(..)
        | Instr::CallTypedDispatchOrBuiltinStoreDictResult(..)
        | Instr::IterateDynamic(..)
        | Instr::CallTypedDispatch(..)
        | Instr::CallStaticParametric(..)
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
        | Instr::ReturnI64
        | Instr::ReturnF64
        | Instr::ReturnF32
        | Instr::ReturnF16
        | Instr::ReturnArray
        | Instr::ReturnNothing
        | Instr::ReturnAny
        | Instr::Pop
        | Instr::PopIfIO
        | Instr::Swap
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
        | Instr::ToString
        | Instr::StringConcat(..)
        | Instr::ThrowError
        | Instr::ThrowValue
        | Instr::PushExceptionValue
        | Instr::Test(..)
        | Instr::TestSetBegin(..)
        | Instr::TestSetEnd
        | Instr::TestThrowsBegin(..)
        | Instr::TestThrowsEnd
        | Instr::TimeNs
        | Instr::NewArray(..)
        | Instr::PushElem
        | Instr::FinalizeArray(..)
        | Instr::PushArrayValue(..)
        | Instr::LoadArray(..)
        | Instr::StoreArray(..)
        | Instr::IndexLoad(..)
        | Instr::IndexLoadInbounds(..)
        | Instr::IndexSlice(..)
        | Instr::IndexStore(..)
        | Instr::IndexStoreInbounds(..)
        | Instr::Zero
        | Instr::ArrayPush
        | Instr::ArrayPushTypejoin
        | Instr::ReserveArray
        | Instr::ArrayPop
        | Instr::ArrayPushFirst
        | Instr::ArrayPopFirst
        | Instr::ArrayInsert
        | Instr::ArrayDeleteAt
        | Instr::NewArrayTyped(..)
        | Instr::PushElemTyped
        | Instr::IndexLoadTyped(..)
        | Instr::IndexLoadTypedInbounds(..)
        | Instr::IndexStoreTyped(..)
        | Instr::FinalizeArrayTyped(..)
        | Instr::AllocUndefTyped(..)
        | Instr::AllocUndefTypedFromTuple(..)
        | Instr::AllocUndefDynamicTyped(..)
        | Instr::AllocUndefDynamicTypedFromTuple
        | Instr::MatMul
        | Instr::MakeRange
        | Instr::MakeRangeF64
        | Instr::MakeRangeLazy
        | Instr::LoadRange(..)
        | Instr::StoreRange(..)
        | Instr::ReturnRange
        | Instr::RangeCollect
        | Instr::RangeFirst
        | Instr::RangeLast
        | Instr::RangeGetIndex
        | Instr::ConcatStrings(..)
        | Instr::ToStr
        | Instr::PushHandler(..)
        | Instr::PopHandler
        | Instr::ClearError
        | Instr::PushErrorCode
        | Instr::PushErrorMessage
        | Instr::Rethrow
        | Instr::RethrowCurrent
        | Instr::RethrowOther
        | Instr::SliceAll
        | Instr::NewStruct(..)
        | Instr::NewStructSplat(..)
        | Instr::NewParametricStruct(..)
        | Instr::NewDynamicParametricStruct(..)
        | Instr::ConstructParametricType(..)
        | Instr::LoadStruct(..)
        | Instr::StoreStruct(..)
        | Instr::GetField(..)
        | Instr::GetFieldByName(..)
        | Instr::SetField(..)
        | Instr::SetFieldByName(..)
        | Instr::GetExprField(..)
        | Instr::GetLineNumberNodeField(..)
        | Instr::GetQuoteNodeValue
        | Instr::GetGlobalRefField(..)
        | Instr::ReturnStruct
        | Instr::NtupleFunc(..)
        | Instr::NtupleRuntime
        | Instr::MakeGenerator(..)
        | Instr::MakeGeneratorRuntime(..)
        | Instr::WrapInGenerator
        | Instr::SprintFunc(..)
        | Instr::NewStableRng
        | Instr::NewXoshiro
        | Instr::NewMersenne
        | Instr::LoadRng(..)
        | Instr::StoreRng(..)
        | Instr::RngRandF64
        | Instr::RngRandArrayF64(..)
        | Instr::RngRandArrayI64(..)
        | Instr::RngRandnF64
        | Instr::RngRandnArrayF64(..)
        | Instr::ReturnRng
        | Instr::PushGlobalRng
        | Instr::RandnArg(..)
        | Instr::RandArg(..)
        | Instr::MakeSimpleVector(..)
        | Instr::NewTuple(..)
        | Instr::LoadTuple(..)
        | Instr::StoreTuple(..)
        | Instr::TupleGet
        | Instr::TupleUnpack(..)
        | Instr::ReturnTuple
        | Instr::NewNamedTuple(..)
        | Instr::LoadNamedTuple(..)
        | Instr::StoreNamedTuple(..)
        | Instr::NamedTupleGetField(..)
        | Instr::NamedTupleGetIndex
        | Instr::NamedTupleGetBySymbol
        | Instr::ReturnNamedTuple
        | Instr::NewPairs(..)
        | Instr::PairsGetBySymbol
        | Instr::PairsLength
        | Instr::PairsKeys
        | Instr::PairsValues
        | Instr::LoadDict(..)
        | Instr::StoreDict(..)
        | Instr::DictSet
        | Instr::DictLen
        | Instr::ReturnDict
        | Instr::NewSet
        | Instr::NewSetTyped(..)
        | Instr::SetAdd
        | Instr::StoreSet(..)
        | Instr::LoadSet(..)
        | Instr::ReturnSet
        | Instr::MakeRef
        | Instr::UnwrapRef
        | Instr::ReturnRef
        | Instr::IterateFirst
        | Instr::IterateNext
        | Instr::IsNothing
        | Instr::TupleFirst
        | Instr::TupleSecond
        | Instr::IterateFirstSplit
        | Instr::IterateNextSplit
        | Instr::NewMemory(..)
        | Instr::NewMemoryDynamic(..)
        | Instr::NewMemoryDynamicTyped
        | Instr::MemoryGet
        | Instr::MemorySet
        | Instr::MemoryLength
        | Instr::LoadMemory(..)
        | Instr::StoreMemory(..)
        | Instr::ReturnMemory
        | Instr::IsDefined(..)
        | Instr::CallResolved(..)
        | Instr::LoadSlotSymbol(..)
        | Instr::StoreSlotSymbol(..)
        | Instr::Nop
        | Instr::ThrowMethodError(..)
        | Instr::ThrowUndefVarError(..)
        | Instr::RaiseUndefVarErrorIfFunctionInvisible(..)
        | Instr::ConstructParametricTypeSplat(..)
        | Instr::ApplyTypeDynamic(..)
        | Instr::RegisterEnum(..)
        | Instr::ConstructEnum(..)
        | Instr::MakeStepRangeLazy
        | Instr::ArrayDeleteAtIndices
        | Instr::LoadSquareF64Slot(..)
        | Instr::LoadAddF64Slot(..)
        | Instr::LoadSubF64Slot(..)
        | Instr::LoadMulF64Slot(..)
        | Instr::LoadDivF64Slot(..)
        | Instr::JumpIfEqF64(..)
        | Instr::JumpIfNeF64(..)
        | Instr::JumpIfNotLtF64(..)
        | Instr::JumpIfNotGtF64(..)
        | Instr::JumpIfNotLeF64(..)
        | Instr::JumpIfNotGeF64(..)
        | Instr::AddConstI64Slot(..)
        | Instr::JumpIfGtI64Slots(..)
        | Instr::CallSpecializeI64Slots(..)
        | Instr::CallSpecializeInboundsI64Slots(..)
        | Instr::LoadSlotI64ToF64(..)
        | Instr::AddConstI64SlotAndJumpIfLe(..)
        | Instr::CallResolvedI64Slots(..)
        | Instr::CallInboundsI64Slots(..)
        | Instr::PopCaughtException
        | Instr::LoadAddConstI64Slot(..)
        | Instr::AddF64Slots(..)
        | Instr::AddF64I64Slots(..)
        | Instr::RandScalarTyped(..)
        | Instr::RngRandScalarTyped(..)
        | Instr::RngRandArg
        | Instr::MakeGeneratorRuntimeFiltered(..)
        | Instr::RandMaybeRng { .. }
        | Instr::ForgetLetLocals(..)
        | Instr::RngRandArrayTyped(..)
        | Instr::RandArrayTyped(..)
        | Instr::CoerceRangeStopI64
        | Instr::JumpIfCmpI64SlotConst(..)
        | Instr::CallSpecializeF64Slots(..)
        | Instr::CallSpecializeInboundsF64Slots(..)
        | Instr::ApplyTypeDynamicSplat(..) => false,
    }
}

/// Per-`Vm` state for the register VM gate.
pub(crate) struct RegisterGateState {
    /// Memoized per-function translations; `None` = ineligible/untranslatable.
    programs: HashMap<usize, Option<Rc<RegisterProgram>>>,
    /// Current nesting of register executions on the host Rust stack.
    /// Native register frames inside one execution do not increment this.
    nesting: usize,
    /// Calls executed to completion on the register VM.
    executed_calls: u64,
    /// Eligible-checked calls that stayed on the stack VM (untranslatable
    /// function, ineligible shape, or re-entrant nesting cap).
    fallback_calls: u64,
    /// Total dynamic register instruction dispatches (for #8559 metrics).
    dispatch_total: u64,
    /// `SJULIA_REGISTER_VM_LOG` debug logging.
    log: bool,
}

impl RegisterGateState {
    /// Read the gate configuration from the environment. Returns `None`
    /// (gate disabled, zero overhead beyond one `Option` check) unless
    /// `SJULIA_REGISTER_VM` is `1`/`true` or [`set_register_vm_forced`] is
    /// active.
    pub(crate) fn from_env() -> Option<Self> {
        let enabled = FORCED.load(Ordering::Relaxed)
            || std::env::var("SJULIA_REGISTER_VM")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
        if !enabled {
            return None;
        }
        Some(Self {
            programs: HashMap::new(),
            nesting: 0,
            executed_calls: 0,
            fallback_calls: 0,
            dispatch_total: 0,
            log: std::env::var_os("SJULIA_REGISTER_VM_LOG").is_some(),
        })
    }
}

impl<R: RngLike> Vm<R> {
    /// Whether the `SJULIA_REGISTER_VM=1` gate is active for this VM.
    #[inline]
    pub(crate) fn register_gate_enabled(&self) -> bool {
        self.register_gate.is_some()
    }

    /// Number of function invocations executed end-to-end on the register VM.
    pub fn register_vm_executed_calls(&self) -> u64 {
        self.register_gate
            .as_ref()
            .map_or(0, |gate| gate.executed_calls)
    }

    /// Number of gate-checked invocations that stayed on the stack VM.
    pub fn register_vm_fallback_calls(&self) -> u64 {
        self.register_gate
            .as_ref()
            .map_or(0, |gate| gate.fallback_calls)
    }

    /// Total dynamic register instruction dispatches across all register VM
    /// executions (feeds the Issue #8559 measurement matrix).
    pub fn register_vm_dispatch_total(&self) -> u64 {
        self.register_gate
            .as_ref()
            .map_or(0, |gate| gate.dispatch_total)
    }

    /// Try to execute a direct call on the register VM. Returns `Ok(None)`
    /// when the call must stay on the stack VM (gate off, ineligible or
    /// untranslatable function, re-entrant nesting cap); the caller then falls through
    /// to the normal stack call path with the operand stack untouched.
    /// `_inbounds` (the caller-side `CallInbounds` context) is currently
    /// unobservable on the register VM: no translatable instruction consults
    /// `frame.inbounds_context` (`PushBoundsCheckEnabled` and the `Index*`
    /// family fail translation), and plain calls made *inside* the body never
    /// inherit the caller's inbounds context on the stack VM either.
    pub(in crate::vm) fn try_register_vm_call(
        &mut self,
        func_index: usize,
        arg_count: usize,
        _inbounds: bool,
    ) -> Result<Option<DispatchAction>, VmError> {
        {
            let Some(gate) = self.register_gate.as_mut() else {
                return Ok(None);
            };
            if gate.nesting >= MAX_REGISTER_VM_NESTING {
                gate.fallback_calls += 1;
                if gate.log {
                    gate_log!(
                        "[register-vm] func {func_index}: stack path (nesting cap {MAX_REGISTER_VM_NESTING})"
                    );
                }
                return Ok(None);
            }
        }

        let Some(program) = self.register_program_for(func_index) else {
            if let Some(gate) = self.register_gate.as_mut() {
                gate.fallback_calls += 1;
            }
            return Ok(None);
        };

        // Per-call shape check (the memoized translation is per function).
        let param_slots = {
            let Some(func) = self.functions.get(func_index) else {
                return Ok(None);
            };
            if func.params.len() != arg_count || func.param_slots.len() != arg_count {
                if let Some(gate) = self.register_gate.as_mut() {
                    gate.fallback_calls += 1;
                }
                return Ok(None);
            }
            func.param_slots.clone()
        };

        // Bind the call arguments into a fresh register-frame slot store,
        // mirroring `bind_value_to_slot` (structs move to the heap; all other
        // values — including heap references — pass through unchanged).
        let mut slots: Vec<Option<Value>> = vec![None; program.slot_count()];
        for idx in (0..arg_count).rev() {
            let val = self.stack.pop_value()?;
            let val = match val {
                Value::Struct(s) => {
                    let heap_idx = self.struct_heap.len();
                    self.struct_heap.push(s);
                    Value::StructRef(heap_idx)
                }
                other => other,
            };
            if let Some(entry) = slots.get_mut(param_slots[idx]) {
                *entry = Some(val);
            }
        }

        if let Some(gate) = self.register_gate.as_mut() {
            gate.nesting += 1;
        }
        let outcome = execute_register_program(&program, &mut slots, self);
        if let Some(gate) = self.register_gate.as_mut() {
            gate.nesting = gate.nesting.saturating_sub(1);
        }

        match outcome {
            Ok(outcome) => {
                if let Some(gate) = self.register_gate.as_mut() {
                    gate.executed_calls += outcome.register_call_count as u64;
                    gate.dispatch_total += outcome.dispatch_count as u64;
                    if gate.log {
                        gate_log!(
                            "[register-vm] {} (func {func_index}): register path, {} calls, {} dispatches",
                            program.name(),
                            outcome.register_call_count,
                            outcome.dispatch_count
                        );
                    }
                }
                self.stack.push(outcome.value);
                Ok(Some(DispatchAction::Continue))
            }
            Err(err) => {
                if self.register_gate.as_ref().is_some_and(|gate| gate.log) {
                    gate_log!(
                        "[register-vm] {} (func {func_index}): register path raised {err:?}",
                        program.name()
                    );
                }
                // Route the error through the stack VM's handler machinery at
                // the call site, exactly like a raising builtin.
                self.raise(err)?;
                Ok(Some(DispatchAction::Continue))
            }
        }
    }

    /// Memoized per-function translation lookup. Logs (once) which path a
    /// function will take when `SJULIA_REGISTER_VM_LOG` is set.
    fn register_program_for(&mut self, func_index: usize) -> Option<Rc<RegisterProgram>> {
        if let Some(gate) = self.register_gate.as_ref() {
            if let Some(entry) = gate.programs.get(&func_index) {
                return entry.clone();
            }
        }

        let func = self.functions.get(func_index)?.clone();
        let log = self.register_gate.as_ref().is_some_and(|gate| gate.log);

        let ineligible_reason = if func.is_generated {
            Some("@generated body")
        } else if func.vararg_param_index.is_some() {
            Some("varargs parameters")
        } else if !func.kwparams.is_empty() {
            Some("keyword parameters")
        } else if !func.type_params.is_empty() {
            Some("where-clause type parameters")
        } else if func.params.len() != func.param_slots.len() {
            Some("parameter/slot layout mismatch")
        } else if func.shared_plan.is_none() {
            Some("missing shared SSA plan")
        } else if self.function_has_register_unsupported_stack_markers(func.entry, func.code_end) {
            Some("unsupported stack-bytecode marker")
        } else if !register_supported_value_type(&func.return_type) {
            Some("unsupported return type")
        } else if func
            .params
            .iter()
            .any(|(_, ty)| !register_supported_value_type(ty))
        {
            Some("unsupported parameter type")
        } else if func
            .slot_types
            .iter()
            .flatten()
            .any(|tag| !register_supported_slot_type(*tag))
        {
            Some("unsupported slot type")
        } else {
            None
        };

        let translated = if let Some(reason) = ineligible_reason {
            if log {
                gate_log!(
                    "[register-vm] {} (func {func_index}): stack path ({reason})",
                    func.name
                );
            }
            None
        } else {
            let plan = func.shared_plan.as_ref()?;
            match RegisterProgram::from_shared_plan_with_context(
                plan,
                func.local_slot_count,
                Rc::new(func.slot_names.clone()),
                &func.slot_types,
                func.name.clone(),
                &self.function_name_index,
            ) {
                Ok(program) => {
                    if log {
                        let metrics = program.metrics();
                        gate_log!(
                            "[register-vm] {} (func {func_index}): translated, {} instrs, {} bytes, {} registers, {} slots",
                            func.name,
                            metrics.dispatch_count,
                            metrics.bytecode_bytes,
                            metrics.frame_registers,
                            metrics.frame_slots
                        );
                    }
                    Some(Rc::new(program))
                }
                Err(reason) => {
                    if log {
                        gate_log!(
                            "[register-vm] {} (func {func_index}): stack path ({reason})",
                            func.name
                        );
                    }
                    None
                }
            }
        };

        if let Some(gate) = self.register_gate.as_mut() {
            gate.programs.insert(func_index, translated.clone());
        }
        translated
    }

    fn function_has_register_unsupported_stack_markers(&self, entry: usize, end: usize) -> bool {
        let Some(code) = self.code.get(entry..end) else {
            return true;
        };
        code.iter().any(instr_is_register_unsupported_stack_marker)
    }

    /// Run a stack VM function call to completion for the register VM
    /// trampoline (see module docs). Mirrors `eval_dispatch_call`'s nested
    /// dispatch discipline (Issues #4976, #5972, #7687).
    fn register_trampoline_call(
        &mut self,
        func_index: usize,
        args: Vec<Value>,
        inbounds: bool,
    ) -> Result<Value, VmError> {
        let target_depth = self.frames.len();
        let saved_ip = self.ip;
        let saved_stack_len = self.stack.len();
        let saved_return_ips_len = self.return_ips.len();
        let saved_handlers_len = self.handlers.len();

        // Ancestor-handler floor (Issue #5972): an error whose handler lives
        // *outside* this nested dispatch must propagate as `Err` (the gate
        // re-raises it at the call site) instead of truncating frames below
        // the awaited depth mid-loop.
        let saved_floor = self.eval_dispatch_floor;
        self.eval_dispatch_floor = Some(target_depth);

        let result = self.register_trampoline_inner(func_index, args, inbounds, target_depth);

        self.eval_dispatch_floor = saved_floor;
        self.ip = saved_ip;

        if result.is_err() {
            // The error surfaced as a Rust `Err` rather than unwinding via a
            // bytecode handler, so the failing callee's frames, operand-stack
            // residue and return addresses are still live (Issue #7687).
            while self.frames.len() > target_depth {
                self.pop_call_frame();
            }
            if self.stack.len() > saved_stack_len {
                self.stack.truncate(saved_stack_len);
            }
            // Same class of leak as the bytecode-handler path (Issue #9319): a
            // higher-order / broadcast / generator driver parked inside this
            // nested trampoline is now orphaned above `target_depth` and would
            // be re-entered by a later frame return at that depth. Drop it.
            self.unwind_driven_callable_state(target_depth);
        }
        self.return_ips.truncate(saved_return_ips_len);
        if self.handlers.len() > saved_handlers_len {
            self.handlers.truncate(saved_handlers_len);
        }

        result
    }

    fn register_trampoline_inner(
        &mut self,
        func_index: usize,
        args: Vec<Value>,
        inbounds: bool,
        target_depth: usize,
    ) -> Result<Value, VmError> {
        let func = self.functions.get(func_index).cloned().ok_or_else(|| {
            VmError::InternalError(format!(
                "register VM call: function index {func_index} out of bounds (have {} functions)",
                self.functions.len()
            ))
        })?;
        let action = self.execute_direct_call_with_func_args(func_index, func, &args, inbounds)?;
        // Issue #10103: reclaim the owned scratch vector into the pool.
        self.release_arg_vec(args);
        self.handle_pending_call_depth_overflow()?;
        match action {
            DispatchAction::Exit(value) => Ok(value),
            DispatchAction::Continue => {
                if self.frames.len() > target_depth {
                    self.run_until_frame_return(target_depth)
                } else {
                    // The call produced its value without a frame (e.g. the
                    // cached `@generated` expression path pushes directly).
                    self.stack.pop_value()
                }
            }
        }
    }

    fn register_call_slots_for_args(
        &mut self,
        func_index: usize,
        arg_count: usize,
        args: &[Value],
    ) -> Option<(Rc<RegisterProgram>, Vec<Option<Value>>)> {
        let program = self.register_program_for(func_index)?;
        let param_slots = {
            let func = self.functions.get(func_index)?;
            if func.params.len() != arg_count || func.param_slots.len() != arg_count {
                return None;
            }
            func.param_slots.clone()
        };

        let mut slots: Vec<Option<Value>> = vec![None; program.slot_count()];
        for (idx, val) in args.iter().cloned().enumerate() {
            let val = match val {
                Value::Struct(s) => {
                    let heap_idx = self.struct_heap.len();
                    self.struct_heap.push(s);
                    Value::StructRef(heap_idx)
                }
                other => other,
            };
            {
                let entry = slots.get_mut(param_slots[idx])?;
                *entry = Some(val);
            }
        }
        Some((program, slots))
    }
}

impl<R: RngLike> RegisterVmHost for Vm<R> {
    fn prepare_register_call_frame(
        &mut self,
        func_index: usize,
        args: &[Value],
        _inbounds: bool,
    ) -> Result<Option<RegisterCallFrame>, VmError> {
        Ok(self
            .register_call_slots_for_args(func_index, args.len(), args)
            .map(|(program, slots)| RegisterCallFrame { program, slots }))
    }

    fn call_function(
        &mut self,
        func_index: usize,
        args: Vec<Value>,
        inbounds: bool,
    ) -> Result<Value, VmError> {
        self.register_trampoline_call(func_index, args, inbounds)
    }

    fn call_intrinsic(&mut self, intrinsic: Intrinsic, args: Vec<Value>) -> Result<Value, VmError> {
        // Intrinsics are pure operand-stack operations: push the arguments,
        // execute, pop the result. The stack length is restored on both
        // paths so a raising intrinsic leaves no residue.
        let base = self.stack.len();
        for arg in args {
            self.stack.push(arg);
        }
        match self.execute_intrinsic(intrinsic) {
            Ok(()) => {
                let value = self.stack.pop_value();
                self.stack.truncate(base);
                value
            }
            Err(err) => {
                self.stack.truncate(base);
                Err(err)
            }
        }
    }

    fn value_to_f64_slow(&mut self, value: &Value) -> Result<f64, VmError> {
        // Exact `pop_f64_or_i64` parity (BigInt, Rational/Irrational structs,
        // ...) without duplicating the conversion table.
        let mut tmp = vec![value.clone()];
        StackOpsExt::pop_f64_or_i64(&mut tmp, &self.struct_heap)
    }

    fn normalize_for_slot_storage(&mut self, value: Value) -> Value {
        self.value_for_slot_storage(value)
    }

    fn bool_context_type_name(&self, value: &Value) -> String {
        self.get_type_name(value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;
    use crate::ir::core::Expr;
    use crate::register_vm::RegisterInstr;
    use crate::rng::StableRng;
    use crate::span::Span;
    use crate::vm::{FunctionInfo, Instr, ValueType};
    use subset_julia_vm_bytecode::{SharedBlockPlan, SharedFunctionPlan, SharedTermPlan};

    fn gate_for_tests() -> RegisterGateState {
        RegisterGateState {
            programs: HashMap::new(),
            nesting: 0,
            executed_calls: 0,
            fallback_calls: 0,
            dispatch_total: 0,
            log: false,
        }
    }

    #[test]
    fn register_gate_builds_program_from_shared_plan_not_stack_instr_issue_9089() {
        let span = Span::new(0, 0, 0, 0, 0, 0);
        let plan = SharedFunctionPlan::new(vec![SharedBlockPlan::new(
            Vec::new(),
            SharedTermPlan::Return {
                expr: Some(Expr::Var("x".to_string().into(), span)),
            },
        )]);

        let mut vm = Vm::new(
            vec![Instr::PushStr("stack-only".to_string()), Instr::ReturnAny],
            StableRng::new(0),
        );
        vm.register_gate = Some(gate_for_tests());
        vm.functions.push(Rc::new(FunctionInfo {
            name: "gate_shared_plan_identity_9089".to_string(),
            params: vec![("x".to_string(), ValueType::Any)],
            kwparams: Vec::new(),
            entry: 0,
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            is_lowering_helper: false,
            definition_order: 0,
            min_world: 1,
            type_params: Vec::new(),
            param_julia_types: Vec::new(),
            code_start: 0,
            code_end: 2,
            slot_names: vec!["x".to_string()],
            slot_types: Vec::new(),
            local_slot_count: 1,
            param_slots: vec![0],
            vararg_param_index: None,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            def_line: 0,
            suppress_short_name_alias: false,
            shared_plan: Some(plan),
        }));

        let program = vm
            .register_program_for(0)
            .expect("shared plan should translate even when stack code cannot");
        assert_eq!(program.name(), "gate_shared_plan_identity_9089");
        assert!(
            !program
                .instructions()
                .iter()
                .any(|instr| matches!(instr, RegisterInstr::CallStack { .. })),
            "shared-plan gate path must not translate via stack bytecode"
        );
    }

    /// Issue #10060 prevention: the register-gate support matrix is an
    /// exhaustive `match` over `Instr` (`instr_is_register_unsupported_stack_
    /// marker`), so a new dynamic call / conversion / global marker variant is
    /// a compile error until explicitly classified. This test pins the
    /// classification of representative variants on both sides so a variant
    /// cannot be silently moved between the marker and gate-neutral arms.
    #[test]
    fn register_gate_marker_classification_pinned_issue_10060() {
        // Dynamic-conversion boxing, dynamic dispatch, and global access
        // remain stack-VM-owned semantics.
        for marker in [
            Instr::DynamicToF64,
            Instr::DynamicToF32,
            Instr::DynamicToI64,
            Instr::DynamicToBool,
            Instr::LoadGlobalAny("g".to_string()),
            Instr::StoreGlobalAny("g".to_string()),
            Instr::EnterLexicalScope(vec!["x".to_string()]),
            Instr::LoadLexical("x".to_string()),
            Instr::StoreLexical("x".to_string()),
            Instr::IsLexicalDefined("x".to_string()),
            Instr::ExitLexicalScope,
        ] {
            assert!(
                instr_is_register_unsupported_stack_marker(&marker),
                "{marker:?} must stay classified as a register-unsupported stack marker"
            );
        }
        // Translatable / gate-neutral instructions must not trip the marker
        // scan (they are handled by shared-plan translation or later checks).
        for neutral in [
            Instr::PushI64(1),
            Instr::PushF64(1.5),
            Instr::PushBool(true),
            Instr::PushNothing,
            Instr::ActivateUsing {
                owner_module: String::new(),
                program_index: 0,
            },
            Instr::ActivateModule(String::new()),
            Instr::ReturnAny,
        ] {
            assert!(
                !instr_is_register_unsupported_stack_marker(&neutral),
                "{neutral:?} must stay gate-neutral for the register gate"
            );
        }
    }
}
