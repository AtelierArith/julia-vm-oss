//! VM instruction profiler
//!
//! Tracks instruction execution frequency to identify optimization opportunities.

use super::instr::Instr;
#[cfg(feature = "profiling")]
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

static PROFILING_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "profiling")]
thread_local! {
    static INSTRUCTION_COUNTS: std::cell::RefCell<HashMap<String, u64>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Enable instruction profiling
pub fn enable() {
    PROFILING_ENABLED.store(true, Ordering::Relaxed);
}

/// Disable instruction profiling
pub fn disable() {
    PROFILING_ENABLED.store(false, Ordering::Relaxed);
}

/// Check if profiling is enabled
pub fn is_enabled() -> bool {
    PROFILING_ENABLED.load(Ordering::Relaxed)
}

/// Record an instruction execution.
///
/// Gated on the `profiling` feature (Issue #5090): in the default build this
/// compiles to an empty body, so the call in the per-instruction dispatch loop
/// (`exec/mod.rs`) is fully eliminated and the hot path pays zero overhead.
#[cfg(feature = "profiling")]
#[inline]
pub fn record(instr: &Instr) {
    if !is_enabled() {
        return;
    }

    let instr_name = instruction_name(instr);
    record_named(&instr_name);
}

/// Zero-overhead no-op when the `profiling` feature is disabled (Issue #5090).
///
/// `#[inline(always)]` plus an empty body lets the optimizer drop the call
/// site (and the `std::mem::replace`'d `&instr` argument evaluation) entirely.
#[cfg(not(feature = "profiling"))]
#[inline(always)]
pub fn record(_instr: &Instr) {}

/// Record a non-instruction VM profiling event.
///
/// Gated on the `profiling` feature (Issue #5090); see [`record`].
#[cfg(feature = "profiling")]
#[inline]
pub fn record_event(event_name: &str) {
    if !is_enabled() {
        return;
    }
    record_named(event_name);
}

/// Zero-overhead no-op when the `profiling` feature is disabled (Issue #5090).
#[cfg(not(feature = "profiling"))]
#[inline(always)]
pub fn record_event(_event_name: &str) {}

#[cfg(feature = "profiling")]
#[inline]
fn record_named(name: &str) {
    INSTRUCTION_COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        *counts.entry(name.to_string()).or_insert(0) += 1;
    });
}

/// Get instruction name for profiling
#[cfg(feature = "profiling")]
fn instruction_name(instr: &Instr) -> String {
    if let Instr::CallIntrinsic(intrinsic) = instr {
        return format!("CallIntrinsic::{:?}", intrinsic);
    }
    if let Instr::CallDynamicBinaryBoth(intrinsic, candidates) = instr {
        return format!(
            "CallDynamicBinaryBoth::{:?}/{}",
            intrinsic,
            candidates.len()
        );
    }

    match instr {
        Instr::PushI64(_) => "PushI64",
        Instr::PushI128(_) => "PushI128",
        Instr::PushBigInt(_) => "PushBigInt",
        Instr::PushBigFloat(_) => "PushBigFloat",
        Instr::PushF64(_) => "PushF64",
        Instr::PushF32(_) => "PushF32",
        Instr::PushBool(_) => "PushBool",
        Instr::PushStr(_) => "PushStr",
        Instr::PushChar(_) => "PushChar",
        Instr::PushNothing => "PushNothing",
        Instr::PushMissing => "PushMissing",
        Instr::PushUndef => "PushUndef",
        Instr::PushStdout => "PushStdout",
        Instr::PushStderr => "PushStderr",
        Instr::PushStdin => "PushStdin",
        Instr::PushDevnull => "PushDevnull",
        Instr::PushCNull => "PushCNull",
        Instr::PushDataType(_) => "PushDataType",
        Instr::PushFunction(_) => "PushFunction",
        Instr::PushResolvedFunction(_) => "PushResolvedFunction",
        Instr::DefineFunction(_) => "DefineFunction",
        Instr::DefineEvalFunction(_) => "DefineEvalFunction",
        Instr::ActivateUsing { .. } => "ActivateUsing",
        Instr::ActivateModule(..) => "ActivateModule",
        Instr::DefineEvalStruct(_) => "DefineEvalStruct",
        Instr::DefineEvalAbstractType(_) => "DefineEvalAbstractType",
        Instr::DefineEvalPrimitiveType(_) => "DefineEvalPrimitiveType",
        Instr::DefineRuntimeNominal(_) => "DefineRuntimeNominal",
        Instr::LoadStr(_) => "LoadStr",
        Instr::StoreStr(_) => "StoreStr",
        Instr::LoadI64(_) => "LoadI64",
        Instr::StoreI64(_) => "StoreI64",
        Instr::LoadF64(_) => "LoadF64",
        Instr::StoreF64(_) => "StoreF64",
        Instr::LoadF32(_) => "LoadF32",
        Instr::StoreF32(_) => "StoreF32",
        Instr::LoadF16(_) => "LoadF16",
        Instr::StoreF16(_) => "StoreF16",
        Instr::LoadBool(_) => "LoadBool",
        Instr::StoreBool(_) => "StoreBool",
        Instr::LoadSlot(_) => "LoadSlot",
        Instr::TakeSlot(_) => "TakeSlot",
        Instr::StoreSlot(_) => "StoreSlot",
        Instr::LoadSlotI64(_) => "LoadSlotI64",
        Instr::StoreSlotI64(_) => "StoreSlotI64",
        Instr::LoadSlotF64(_) => "LoadSlotF64",
        Instr::StoreSlotF64(_) => "StoreSlotF64",
        Instr::LoadSlotBool(_) => "LoadSlotBool",
        Instr::StoreSlotBool(_) => "StoreSlotBool",
        Instr::LoadSlotF32(_) => "LoadSlotF32",
        Instr::StoreSlotF32(_) => "StoreSlotF32",
        Instr::LoadSlotF16(_) => "LoadSlotF16",
        Instr::StoreSlotF16(_) => "StoreSlotF16",
        Instr::LoadSlotStr(_) => "LoadSlotStr",
        Instr::StoreSlotStr(_) => "StoreSlotStr",
        Instr::LoadSlotChar(_) => "LoadSlotChar",
        Instr::StoreSlotChar(_) => "StoreSlotChar",
        Instr::LoadSlotSymbol(_) => "LoadSlotSymbol",
        Instr::StoreSlotSymbol(_) => "StoreSlotSymbol",
        Instr::LoadSlotNarrowInt(_) => "LoadSlotNarrowInt",
        Instr::StoreSlotNarrowInt(_) => "StoreSlotNarrowInt",
        Instr::LoadSlotNothing(_) => "LoadSlotNothing",
        Instr::StoreSlotNothing(_) => "StoreSlotNothing",
        Instr::LoadSlotArray(_) => "LoadSlotArray",
        Instr::StoreSlotArray(_) => "StoreSlotArray",
        Instr::LoadSlotTuple(_) => "LoadSlotTuple",
        Instr::StoreSlotTuple(_) => "StoreSlotTuple",
        Instr::LoadSlotNamedTuple(_) => "LoadSlotNamedTuple",
        Instr::StoreSlotNamedTuple(_) => "StoreSlotNamedTuple",
        Instr::LoadSlotDict(_) => "LoadSlotDict",
        Instr::StoreSlotDict(_) => "StoreSlotDict",
        Instr::LoadSlotSet(_) => "LoadSlotSet",
        Instr::StoreSlotSet(_) => "StoreSlotSet",
        Instr::LoadSlotStruct(_) => "LoadSlotStruct",
        Instr::StoreSlotStruct(_) => "StoreSlotStruct",
        Instr::LoadSlotRange(_) => "LoadSlotRange",
        Instr::StoreSlotRange(_) => "StoreSlotRange",
        Instr::LoadSlotRng(_) => "LoadSlotRng",
        Instr::StoreSlotRng(_) => "StoreSlotRng",
        Instr::LoadSlotGenerator(_) => "LoadSlotGenerator",
        Instr::StoreSlotGenerator(_) => "StoreSlotGenerator",
        Instr::LoadAny(_) => "LoadAny",
        Instr::ProbeRuntimeBinding(_) => "ProbeRuntimeBinding",
        Instr::LoadGlobalAny(_) => "LoadGlobalAny",
        Instr::StoreAny(_) => "StoreAny",
        Instr::LoadTypeBinding(_) => "LoadTypeBinding",
        Instr::CallStaticParametric(_) => "CallStaticParametric",

        Instr::DynamicAdd => "DynamicAdd",
        Instr::DynamicSub => "DynamicSub",
        Instr::DynamicMul => "DynamicMul",
        Instr::DynamicDiv => "DynamicDiv",
        Instr::DynamicMod => "DynamicMod",
        Instr::DynamicIntDiv => "DynamicIntDiv",
        Instr::DynamicNeg => "DynamicNeg",
        Instr::DynamicPow => "DynamicPow",

        Instr::AddI64 => "AddI64",
        Instr::SubI64 => "SubI64",
        Instr::MulI64 => "MulI64",
        Instr::ModI64 => "ModI64",
        Instr::IncI64 => "IncI64",
        Instr::DupI64 => "DupI64",
        Instr::Dup => "Dup",
        Instr::NegI64 => "NegI64",

        // Fused instructions
        Instr::LoadAddI64(_) => "LoadAddI64",
        Instr::LoadSubI64(_) => "LoadSubI64",
        Instr::LoadMulI64(_) => "LoadMulI64",
        Instr::LoadModI64(_) => "LoadModI64",
        Instr::LoadAddI64Slot(_) => "LoadAddI64Slot",
        Instr::LoadAddConstI64Slot(_, _) => "LoadAddConstI64Slot",
        Instr::LoadSubI64Slot(_) => "LoadSubI64Slot",
        Instr::LoadMulI64Slot(_) => "LoadMulI64Slot",
        Instr::LoadModI64Slot(_) => "LoadModI64Slot",
        Instr::LoadSlotI64ToF64(_) => "LoadSlotI64ToF64",
        Instr::LoadSquareF64Slot(_) => "LoadSquareF64Slot",
        Instr::LoadAddF64Slot(_) => "LoadAddF64Slot",
        Instr::AddF64Slots(_, _, _) => "AddF64Slots",
        Instr::AddF64I64Slots(_, _, _) => "AddF64I64Slots",
        Instr::LoadSubF64Slot(_) => "LoadSubF64Slot",
        Instr::LoadMulF64Slot(_) => "LoadMulF64Slot",
        Instr::LoadDivF64Slot(_) => "LoadDivF64Slot",
        Instr::IncVarI64(_) => "IncVarI64",
        Instr::DecVarI64(_) => "DecVarI64",
        Instr::IncVarI64Slot(_) => "IncVarI64Slot",
        Instr::DecVarI64Slot(_) => "DecVarI64Slot",
        Instr::AddConstI64Slot(_, _) => "AddConstI64Slot",
        Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _) => "AddConstI64SlotAndJumpIfLe",
        Instr::JumpIfNeI64(_) => "JumpIfNeI64",
        Instr::JumpIfEqI64(_) => "JumpIfEqI64",
        Instr::JumpIfLtI64(_) => "JumpIfLtI64",
        Instr::JumpIfGtI64(_) => "JumpIfGtI64",
        Instr::JumpIfGtI64Slots(_, _, _) => "JumpIfGtI64Slots",
        Instr::JumpIfCmpI64SlotConst(_, _, _, _) => "JumpIfCmpI64SlotConst",
        Instr::JumpIfLeI64(_) => "JumpIfLeI64",
        Instr::JumpIfGeI64(_) => "JumpIfGeI64",
        Instr::JumpIfEqF64(_) => "JumpIfEqF64",
        Instr::JumpIfNeF64(_) => "JumpIfNeF64",
        Instr::JumpIfNotLtF64(_) => "JumpIfNotLtF64",
        Instr::JumpIfNotGtF64(_) => "JumpIfNotGtF64",
        Instr::JumpIfNotLeF64(_) => "JumpIfNotLeF64",
        Instr::JumpIfNotGeF64(_) => "JumpIfNotGeF64",

        Instr::GtI64 => "GtI64",
        Instr::LtI64 => "LtI64",
        Instr::LeI64 => "LeI64",
        Instr::GeI64 => "GeI64",
        Instr::EqI64 => "EqI64",
        Instr::NeI64 => "NeI64",

        Instr::AddF64 => "AddF64",
        Instr::SubF64 => "SubF64",
        Instr::MulF64 => "MulF64",
        Instr::DivF64 => "DivF64",
        Instr::DupF64 => "DupF64",
        Instr::SqrtF64 => "SqrtF64",
        Instr::NegF64 => "NegF64",
        Instr::PowF64 => "PowF64",

        Instr::LtF64 => "LtF64",
        Instr::GtF64 => "GtF64",
        Instr::LeF64 => "LeF64",
        Instr::GeF64 => "GeF64",
        Instr::EqF64 => "EqF64",
        Instr::NeF64 => "NeF64",

        Instr::ToF64 => "ToF64",
        Instr::ToI64 => "ToI64",
        Instr::BoolToI64 => "BoolToI64",
        Instr::I64ToBool => "I64ToBool",
        Instr::NotBool => "NotBool",

        Instr::Jump(_) => "Jump",
        Instr::JumpIfZero(_) => "JumpIfZero",
        Instr::Call(_, _) => "Call",
        Instr::CallResolved(_, _) => "CallResolved",
        Instr::CallWithKwargs(_, _, _) => "CallWithKwargs",
        Instr::CallWithKwargsSplat(_, _, _, _) => "CallWithKwargsSplat",
        Instr::CallWithSplat(_, _, _) => "CallWithSplat",
        Instr::CallIntrinsic(_) => unreachable!("handled above"),
        Instr::CallBuiltin(_, _) => "CallBuiltin",
        Instr::CallDynamic(_) => "CallDynamic",
        Instr::CallDynamicBinary(_, _, _) => "CallDynamicBinary",
        Instr::CallDynamicBinaryBoth(_, _) => unreachable!("handled above"),
        Instr::CallDynamicBinaryNoFallback(_) => "CallDynamicBinaryNoFallback",
        Instr::CallDynamicOrBuiltin(_, _) => "CallDynamicOrBuiltin",
        Instr::CallTypedDispatchOrBuiltin(_, _, _, _) => "CallTypedDispatchOrBuiltin",
        Instr::CallTypedDispatchOrBuiltinResult(_, _, _, _) => "CallTypedDispatchOrBuiltinResult",
        Instr::CallTypedDispatchOrBuiltinStoreDict(_) => "CallTypedDispatchOrBuiltinStoreDict",
        Instr::CallTypedDispatchOrBuiltinStoreDictResult(_) => {
            "CallTypedDispatchOrBuiltinStoreDictResult"
        }
        Instr::IterateDynamic(_, _) => "IterateDynamic",
        Instr::CallTypedDispatch(_, _, _, _) => "CallTypedDispatch",
        Instr::CallTypeConstructor => "CallTypeConstructor",
        Instr::CallGlobalRef(_) => "CallGlobalRef",
        Instr::CallFunctionVariable(_) => "CallFunctionVariable",
        Instr::InvokeFunctionVariable(_, _) => "InvokeFunctionVariable",
        Instr::InvokeFunctionVariableWithKwargs(_) => "InvokeFunctionVariableWithKwargs",
        Instr::InvokeFunctionVariableDynamicSignature(_) => {
            "InvokeFunctionVariableDynamicSignature"
        }
        Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(_, _, _) => {
            "InvokeFunctionVariableDynamicSignatureWithKwargs"
        }
        Instr::CallFunctionVariableWithSplat(_, _) => "CallFunctionVariableWithSplat",
        Instr::CallFunctionVariableWithKwargsSplat(_) => "CallFunctionVariableWithKwargsSplat",
        Instr::CallSpecialize(_, _) => "CallSpecialize",
        Instr::CallSpecializeInbounds(_, _) => "CallSpecializeInbounds",
        Instr::CallSpecializeI64Slots(_) => "CallSpecializeI64Slots",
        Instr::CallSpecializeInboundsI64Slots(_) => "CallSpecializeInboundsI64Slots",
        Instr::CallSpecializeF64Slots(_) => "CallSpecializeF64Slots",
        Instr::CallSpecializeInboundsF64Slots(_) => "CallSpecializeInboundsF64Slots",
        Instr::CallResolvedI64Slots(_) => "CallResolvedI64Slots",
        Instr::CallInboundsI64Slots(_) => "CallInboundsI64Slots",

        Instr::ReturnI64 => "ReturnI64",
        Instr::ReturnF64 => "ReturnF64",
        Instr::ReturnF32 => "ReturnF32",
        Instr::ReturnF16 => "ReturnF16",
        Instr::ReturnArray => "ReturnArray",
        Instr::ReturnNothing => "ReturnNothing",
        Instr::ReturnAny => "ReturnAny",
        Instr::ReturnRange => "ReturnRange",
        Instr::ReturnStruct => "ReturnStruct",
        Instr::ReturnRng => "ReturnRng",
        Instr::ReturnTuple => "ReturnTuple",
        Instr::ReturnNamedTuple => "ReturnNamedTuple",
        Instr::ReturnDict => "ReturnDict",
        Instr::ReturnRef => "ReturnRef",
        Instr::Pop => "Pop",
        Instr::PopIfIO => "PopIfIO",
        Instr::ConstructParametricType(_, _) => "ConstructParametricType",
        Instr::ConstructParametricTypeSplat(_, _) => "ConstructParametricTypeSplat",
        Instr::ApplyTypeDynamic(_) => "ApplyTypeDynamic",
        Instr::ApplyTypeDynamicSplat(_) => "ApplyTypeDynamicSplat",

        // Struct field access. The index-based forms are the statically
        // resolved fast paths (including lazy specialization, Issue #6346); the
        // by-name forms resolve the field at runtime and indicate the generic
        // fallback was taken.
        Instr::GetField(_) => "GetField",
        Instr::SetField(_) => "SetField",
        Instr::GetFieldByName(_) => "GetFieldByName",
        Instr::SetFieldByName(_) => "SetFieldByName",

        Instr::EnterLexicalScope(_) => "EnterLexicalScope",
        Instr::LoadLexical(_) => "LoadLexical",
        Instr::StoreLexical(_) => "StoreLexical",
        Instr::IsLexicalDefined(_) => "IsLexicalDefined",
        Instr::ExitLexicalScope => "ExitLexicalScope",

        _ => "Other",
    }
    .to_string()
}

/// Clear profiling data
#[cfg(feature = "profiling")]
pub fn clear() {
    INSTRUCTION_COUNTS.with(|counts| {
        counts.borrow_mut().clear();
    });
}

/// No-op when the `profiling` feature is disabled (Issue #5090).
#[cfg(not(feature = "profiling"))]
pub fn clear() {}

/// Get profiling results sorted by frequency (descending)
#[cfg(feature = "profiling")]
pub fn get_results() -> Vec<(String, u64)> {
    INSTRUCTION_COUNTS.with(|counts| {
        let counts = counts.borrow();
        let mut results: Vec<_> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        results.sort_by_key(|b| std::cmp::Reverse(b.1));
        results
    })
}

/// Always-empty when the `profiling` feature is disabled (Issue #5090).
#[cfg(not(feature = "profiling"))]
pub fn get_results() -> Vec<(String, u64)> {
    Vec::new()
}

/// VM specialization counters derived from profiling instruction/event counts
/// (Issue #5095).
///
/// These are intentionally produced only from the existing `profiling` feature
/// data. The default build keeps zero runtime overhead while profiling benches
/// can compute boxing, dispatch, devirtualization, and typed-arithmetic ratios
/// from a single VM run.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SpecializationCounters {
    /// Dynamic-value traffic through generic frame/lexical loads and stores,
    /// plus `ReturnAny`.
    pub boxed_value_accesses: u64,
    /// Typed local/slot loads, stores, and returns used as the denominator for
    /// the boxed-value access rate.
    pub typed_value_accesses: u64,
    /// Dynamic arithmetic instructions (`DynamicAdd`, `DynamicSub`, ...).
    pub dynamic_arithmetic_ops: u64,
    /// Concrete/fused arithmetic instructions (`AddI64`, `AddF64`, ...).
    pub specialized_arithmetic_ops: u64,
    /// Dynamic dispatch instructions/events (`CallDynamic*`, function-variable
    /// calls, typed-dispatch fallbacks, and binary resolver lookups).
    pub dynamic_dispatches: u64,
    /// Statically resolved/devirtualized call instructions (`Call`,
    /// `CallBuiltin`, `CallIntrinsic`, `CallSpecialize`).
    pub devirtualized_calls: u64,
    /// Dispatch cache hits recorded by dynamic dispatch hot paths.
    pub dispatch_cache_hits: u64,
    /// Dispatch cache misses recorded by dynamic dispatch hot paths.
    pub dispatch_cache_misses: u64,
}

impl SpecializationCounters {
    /// Fraction of dynamic `Any` traffic among tracked value accesses.
    pub fn boxing_rate(self) -> f64 {
        ratio(
            self.boxed_value_accesses,
            self.boxed_value_accesses + self.typed_value_accesses,
        )
    }

    /// Fraction of dispatch cache lookups that missed.
    pub fn dispatch_miss_rate(self) -> f64 {
        ratio(
            self.dispatch_cache_misses,
            self.dispatch_cache_hits + self.dispatch_cache_misses,
        )
    }

    /// Fraction of calls that were statically resolved instead of dynamic.
    pub fn devirtualization_rate(self) -> f64 {
        ratio(
            self.devirtualized_calls,
            self.devirtualized_calls + self.dynamic_dispatches,
        )
    }

    /// Fraction of arithmetic instructions that used concrete/fused opcodes.
    pub fn specialized_arithmetic_rate(self) -> f64 {
        ratio(
            self.specialized_arithmetic_ops,
            self.specialized_arithmetic_ops + self.dynamic_arithmetic_ops,
        )
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

/// Return the current specialization counters.
pub fn specialization_counters() -> SpecializationCounters {
    specialization_counters_from_results(&get_results())
}

/// Derive Issue #5095 counters from a profiler result snapshot.
pub fn specialization_counters_from_results(results: &[(String, u64)]) -> SpecializationCounters {
    SpecializationCounters {
        boxed_value_accesses: sum_exact(
            results,
            &[
                "LoadAny",
                "StoreAny",
                "LoadLexical",
                "StoreLexical",
                "ReturnAny",
            ],
        ),
        typed_value_accesses: sum_exact(
            results,
            &[
                "LoadI64",
                "StoreI64",
                "LoadF64",
                "StoreF64",
                "LoadF32",
                "StoreF32",
                "LoadF16",
                "StoreF16",
                "LoadBool",
                "StoreBool",
                "LoadStr",
                "StoreStr",
                "LoadSlot",
                "StoreSlot",
                "LoadSlotI64",
                "StoreSlotI64",
                "LoadSlotF64",
                "StoreSlotF64",
                "LoadSlotBool",
                "StoreSlotBool",
                "LoadSlotF32",
                "StoreSlotF32",
                "LoadSlotF16",
                "StoreSlotF16",
                "LoadSlotStr",
                "StoreSlotStr",
                "LoadSlotChar",
                "StoreSlotChar",
                "LoadSlotSymbol",
                "StoreSlotSymbol",
                "LoadSlotNarrowInt",
                "StoreSlotNarrowInt",
                "LoadSlotNothing",
                "StoreSlotNothing",
                "LoadSlotArray",
                "StoreSlotArray",
                "LoadSlotTuple",
                "StoreSlotTuple",
                "LoadSlotNamedTuple",
                "StoreSlotNamedTuple",
                "LoadSlotDict",
                "StoreSlotDict",
                "LoadSlotSet",
                "StoreSlotSet",
                "LoadSlotStruct",
                "StoreSlotStruct",
                "LoadSlotRange",
                "StoreSlotRange",
                "LoadSlotRng",
                "StoreSlotRng",
                "LoadSlotGenerator",
                "StoreSlotGenerator",
                "ReturnI64",
                "ReturnF64",
                "ReturnF32",
                "ReturnF16",
                "ReturnArray",
                "ReturnNothing",
                "ReturnRange",
                "ReturnStruct",
                "ReturnRng",
                "ReturnTuple",
                "ReturnNamedTuple",
                "ReturnDict",
                "ReturnRef",
            ],
        ),
        dynamic_arithmetic_ops: sum_exact(
            results,
            &[
                "DynamicAdd",
                "DynamicSub",
                "DynamicMul",
                "DynamicDiv",
                "DynamicMod",
                "DynamicIntDiv",
                "DynamicNeg",
                "DynamicPow",
            ],
        ),
        specialized_arithmetic_ops: sum_exact(
            results,
            &[
                "AddI64",
                "SubI64",
                "MulI64",
                "ModI64",
                "IncI64",
                "NegI64",
                "AddF64",
                "SubF64",
                "MulF64",
                "DivF64",
                "NegF64",
                "PowF64",
                "LoadAddI64",
                "LoadSubI64",
                "LoadMulI64",
                "LoadModI64",
                "LoadAddI64Slot",
                "LoadSubI64Slot",
                "LoadMulI64Slot",
                "LoadModI64Slot",
                "LoadSquareF64Slot",
                "LoadAddF64Slot",
                "LoadSubF64Slot",
                "LoadMulF64Slot",
                "LoadDivF64Slot",
                "IncVarI64",
                "DecVarI64",
                "IncVarI64Slot",
                "DecVarI64Slot",
                "AddConstI64Slot",
                "AddConstI64SlotAndJumpIfLe",
                "AddF64Slots",
                "AddF64I64Slots",
            ],
        ),
        dynamic_dispatches: sum_prefixes(
            results,
            &[
                "CallDynamic",
                "CallTypedDispatch",
                "CallFunctionVariable",
                "InvokeFunctionVariable",
                "IterateDynamic",
                "BinaryBothResolverLookup",
            ],
        ),
        devirtualized_calls: sum_exact(
            results,
            &[
                "Call",
                "CallResolved",
                "CallBuiltin",
                "CallIntrinsic",
                "CallSpecialize",
                "CallSpecializeInbounds",
                "CallSpecializeI64Slots",
                "CallSpecializeInboundsI64Slots",
                "CallSpecializeF64Slots",
                "CallSpecializeInboundsF64Slots",
                "CallResolvedI64Slots",
                "CallInboundsI64Slots",
            ],
        ) + sum_prefixes(results, &["CallIntrinsic::"]),
        dispatch_cache_hits: sum_exact(
            results,
            &[
                "BinaryMethodCacheHit",
                "MethodDispatchCacheHit",
                "MethodDispatchNegativeCacheHit",
                "CallSiteDispatchCacheHit",
                "CallSiteDispatchNegativeCacheHit",
                "BinaryBothPrimitiveFastHit",
                "BinaryBothResolverMatch",
                "CallDirectFastHit",
                "CallDirectFastI64FunctionHit",
            ],
        ),
        dispatch_cache_misses: sum_exact(
            results,
            &[
                "BinaryMethodCacheMiss",
                "MethodDispatchCacheMiss",
                "CallSiteDispatchCacheMiss",
                "BinaryBothPrimitiveFastMiss",
                "BinaryBothResolverMiss",
                "CallDirectFastMiss",
            ],
        ),
    }
}

fn sum_exact(results: &[(String, u64)], names: &[&str]) -> u64 {
    results
        .iter()
        .filter(|(name, _)| names.contains(&name.as_str()))
        .map(|(_, count)| *count)
        .sum()
}

fn sum_prefixes(results: &[(String, u64)], prefixes: &[&str]) -> u64 {
    results
        .iter()
        .filter(|(name, _)| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .map(|(_, count)| *count)
        .sum()
}

/// Print profiling results
#[cfg(feature = "profiling")]
pub fn print_results() {
    use std::io::Write;
    let results = get_results();
    let total: u64 = results.iter().map(|(_, count)| count).sum();

    let _ = writeln!(std::io::stderr(), "\n=== VM Instruction Profile ===");
    let _ = writeln!(std::io::stderr(), "Total instructions executed: {}", total);
    let _ = writeln!(std::io::stderr(), "\nTop instructions by frequency:");
    let _ = writeln!(
        std::io::stderr(),
        "{:<25} {:>12} {:>10}",
        "Instruction",
        "Count",
        "Percent"
    );
    let _ = writeln!(std::io::stderr(), "{}", "-".repeat(50));

    for (i, (name, count)) in results.iter().take(20).enumerate() {
        let percent = (*count as f64 / total as f64) * 100.0;
        let _ = writeln!(
            std::io::stderr(),
            "{:2}. {:<22} {:>12} {:>9.2}%",
            i + 1,
            name,
            count,
            percent
        );
    }

    let binary_both_events: Vec<_> = results
        .iter()
        .filter(|(name, _)| name.starts_with("BinaryBoth"))
        .collect();
    if !binary_both_events.is_empty() {
        let _ = writeln!(std::io::stderr(), "\nBinaryBoth dispatch events:");
        for (name, count) in binary_both_events {
            let percent = (*count as f64 / total as f64) * 100.0;
            let _ = writeln!(
                std::io::stderr(),
                "{:<25} {:>12} {:>9.2}%",
                name,
                count,
                percent
            );
        }
    }

    let executable_block_events: Vec<_> = results
        .iter()
        .filter(|(name, _)| name.starts_with("ExecutableBlock::"))
        .collect();
    if !executable_block_events.is_empty() {
        let _ = writeln!(std::io::stderr(), "\nExecutable block events:");
        for (name, count) in executable_block_events {
            let percent = (*count as f64 / total as f64) * 100.0;
            let _ = writeln!(
                std::io::stderr(),
                "{:<25} {:>12} {:>9.2}%",
                name,
                count,
                percent
            );
        }
    }

    let call_direct_events: Vec<_> = results
        .iter()
        .filter(|(name, _)| name.starts_with("CallDirectFast"))
        .collect();
    if !call_direct_events.is_empty() {
        let _ = writeln!(std::io::stderr(), "\nDirect call fast-path events:");
        for (name, count) in call_direct_events {
            let percent = (*count as f64 / total as f64) * 100.0;
            let _ = writeln!(
                std::io::stderr(),
                "{:<25} {:>12} {:>9.2}%",
                name,
                count,
                percent
            );
        }
    }

    let _ = writeln!(std::io::stderr(), "{}", "=".repeat(50));
}

/// No-op when the `profiling` feature is disabled (Issue #5090).
#[cfg(not(feature = "profiling"))]
pub fn print_results() {}

#[cfg(all(test, feature = "profiling"))]
mod tests {
    use super::instruction_name;
    use crate::intrinsics::Intrinsic;
    use crate::vm::instr::Instr;

    #[test]
    fn test_instruction_name_classifies_typed_dispatch_and_specialize() {
        let typed = Instr::CallTypedDispatch("f".to_string(), 2, 10, vec![10]);
        let specialize = Instr::CallSpecialize(42, 2);

        assert_eq!(instruction_name(&typed), "CallTypedDispatch");
        assert_eq!(instruction_name(&specialize), "CallSpecialize");
    }

    #[test]
    fn test_instruction_name_splits_call_intrinsic_by_kind() {
        assert_eq!(
            instruction_name(&Instr::CallIntrinsic(Intrinsic::SdivInt)),
            "CallIntrinsic::SdivInt"
        );
    }

    #[test]
    fn test_instruction_name_splits_call_dynamic_binary_both_by_kind() {
        assert_eq!(
            instruction_name(&Instr::CallDynamicBinaryBoth(
                Intrinsic::DynamicAdd,
                vec![1]
            )),
            "CallDynamicBinaryBoth::DynamicAdd/1"
        );
    }

    #[test]
    fn test_instruction_name_classifies_lexical_environment_ops_11569() {
        for (instr, expected) in [
            (
                Instr::EnterLexicalScope(vec!["x".to_string()]),
                "EnterLexicalScope",
            ),
            (Instr::LoadLexical("x".to_string()), "LoadLexical"),
            (Instr::StoreLexical("x".to_string()), "StoreLexical"),
            (Instr::IsLexicalDefined("x".to_string()), "IsLexicalDefined"),
            (Instr::ExitLexicalScope, "ExitLexicalScope"),
        ] {
            assert_eq!(instruction_name(&instr), expected);
        }
    }

    // When the `profiling` feature is enabled, `record` after `enable()` must
    // actually accumulate counts (Issue #5090).
    #[test]
    fn test_record_counts_when_profiling_feature_enabled() {
        super::clear();
        super::enable();
        super::record(&Instr::AddI64);
        super::record(&Instr::AddI64);
        super::record(&Instr::Pop);
        super::disable();

        let counts: std::collections::HashMap<String, u64> =
            super::get_results().into_iter().collect();
        assert_eq!(counts.get("AddI64").copied().unwrap_or(0), 2);
        assert_eq!(counts.get("Pop").copied().unwrap_or(0), 1);
        super::clear();
    }
}

#[cfg(test)]
mod specialization_counter_tests {
    #[test]
    fn specialization_counters_derive_issue_5095_ratios() {
        let results = vec![
            ("LoadAny".to_string(), 2),
            ("StoreAny".to_string(), 1),
            ("LoadLexical".to_string(), 1),
            ("StoreLexical".to_string(), 1),
            ("LoadI64".to_string(), 7),
            ("StoreI64".to_string(), 2),
            ("DynamicAdd".to_string(), 3),
            ("AddI64".to_string(), 6),
            ("CallDynamic".to_string(), 4),
            ("Call".to_string(), 12),
            ("CallResolved".to_string(), 2),
            ("BinaryMethodCacheHit".to_string(), 8),
            ("BinaryMethodCacheMiss".to_string(), 2),
            ("MethodDispatchNegativeCacheHit".to_string(), 1),
            ("MethodDispatchCacheMiss".to_string(), 1),
            ("CallSiteDispatchCacheHit".to_string(), 2),
            ("CallSiteDispatchCacheMiss".to_string(), 1),
            ("BinaryBothPrimitiveFastHit".to_string(), 1),
            ("BinaryBothResolverMiss".to_string(), 1),
        ];

        let counters = super::specialization_counters_from_results(&results);

        assert_eq!(counters.boxed_value_accesses, 5);
        assert_eq!(counters.typed_value_accesses, 9);
        assert_eq!(counters.dynamic_arithmetic_ops, 3);
        assert_eq!(counters.specialized_arithmetic_ops, 6);
        assert_eq!(counters.dynamic_dispatches, 4);
        assert_eq!(counters.devirtualized_calls, 14);
        assert_eq!(counters.dispatch_cache_hits, 12);
        assert_eq!(counters.dispatch_cache_misses, 5);
        assert_eq!(counters.boxing_rate(), 5.0 / 14.0);
        assert_eq!(counters.dispatch_miss_rate(), 5.0 / 17.0);
        assert_eq!(counters.devirtualization_rate(), 14.0 / 18.0);
        assert_eq!(counters.specialized_arithmetic_rate(), 2.0 / 3.0);
    }
}

/// Zero-overhead contract for the default build (Issue #5090): with the
/// `profiling` feature disabled, `record`/`record_event`/`get_results` are
/// compiled-out no-ops, so profiling never accumulates data even after
/// `enable()`. This guards against accidentally re-introducing per-instruction
/// profiler bookkeeping into the hot interpreter loop.
#[cfg(all(test, not(feature = "profiling")))]
mod tests_no_profiling {
    use crate::vm::instr::Instr;

    #[test]
    fn test_record_is_noop_when_profiling_feature_disabled() {
        super::clear();
        super::enable();
        super::record(&Instr::AddI64);
        super::record(&Instr::Pop);
        super::record_event("CustomEvent");
        super::disable();

        assert!(
            super::get_results().is_empty(),
            "profiler must record nothing when the `profiling` feature is off"
        );
    }
}
