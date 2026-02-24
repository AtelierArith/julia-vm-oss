//! VM instruction profiler
//!
//! Tracks instruction execution frequency to identify optimization opportunities.

use super::instr::Instr;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

static PROFILING_ENABLED: AtomicBool = AtomicBool::new(false);

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

/// Record an instruction execution
#[inline]
pub fn record(instr: &Instr) {
    if !is_enabled() {
        return;
    }

    let instr_name = instruction_name(instr);
    INSTRUCTION_COUNTS.with(|counts| {
        let mut counts = counts.borrow_mut();
        *counts.entry(instr_name).or_insert(0) += 1;
    });
}

/// Get instruction name for profiling
fn instruction_name(instr: &Instr) -> String {
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
        Instr::DefineFunction(_) => "DefineFunction",
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
        Instr::LoadSlot(_) => "LoadSlot",
        Instr::StoreSlot(_) => "StoreSlot",
        Instr::LoadAny(_) => "LoadAny",
        Instr::StoreAny(_) => "StoreAny",
        Instr::LoadTypeBinding(_) => "LoadTypeBinding",

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
        Instr::LoadSubI64Slot(_) => "LoadSubI64Slot",
        Instr::LoadMulI64Slot(_) => "LoadMulI64Slot",
        Instr::LoadModI64Slot(_) => "LoadModI64Slot",
        Instr::IncVarI64(_) => "IncVarI64",
        Instr::DecVarI64(_) => "DecVarI64",
        Instr::IncVarI64Slot(_) => "IncVarI64Slot",
        Instr::DecVarI64Slot(_) => "DecVarI64Slot",
        Instr::JumpIfNeI64(_) => "JumpIfNeI64",
        Instr::JumpIfEqI64(_) => "JumpIfEqI64",
        Instr::JumpIfLtI64(_) => "JumpIfLtI64",
        Instr::JumpIfGtI64(_) => "JumpIfGtI64",
        Instr::JumpIfLeI64(_) => "JumpIfLeI64",
        Instr::JumpIfGeI64(_) => "JumpIfGeI64",

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
        Instr::CallWithKwargs(_, _, _) => "CallWithKwargs",
        Instr::CallWithKwargsSplat(_, _, _, _) => "CallWithKwargsSplat",
        Instr::CallWithSplat(_, _, _) => "CallWithSplat",
        Instr::CallIntrinsic(_) => "CallIntrinsic",
        Instr::CallBuiltin(_, _) => "CallBuiltin",
        Instr::CallDynamic(_, _, _) => "CallDynamic",
        Instr::CallDynamicBinary(_, _, _) => "CallDynamicBinary",
        Instr::CallDynamicBinaryBoth(_, _) => "CallDynamicBinaryBoth",
        Instr::CallDynamicBinaryNoFallback(_) => "CallDynamicBinaryNoFallback",
        Instr::CallDynamicOrBuiltin(_, _) => "CallDynamicOrBuiltin",
        Instr::IterateDynamic(_, _) => "IterateDynamic",
        Instr::CallTypedDispatch(_, _, _, _) => "CallTypedDispatch",
        Instr::CallTypeConstructor => "CallTypeConstructor",
        Instr::CallGlobalRef(_) => "CallGlobalRef",
        Instr::CallFunctionVariable(_) => "CallFunctionVariable",
        Instr::CallFunctionVariableWithSplat(_, _) => "CallFunctionVariableWithSplat",
        Instr::CallSpecialize(_, _) => "CallSpecialize",

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

        _ => "Other",
    }
    .to_string()
}

/// Clear profiling data
pub fn clear() {
    INSTRUCTION_COUNTS.with(|counts| {
        counts.borrow_mut().clear();
    });
}

/// Get profiling results sorted by frequency (descending)
pub fn get_results() -> Vec<(String, u64)> {
    INSTRUCTION_COUNTS.with(|counts| {
        let counts = counts.borrow();
        let mut results: Vec<_> = counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results
    })
}

/// Print profiling results
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

    let _ = writeln!(std::io::stderr(), "{}", "=".repeat(50));
}

#[cfg(test)]
mod tests {
    use super::instruction_name;
    use crate::vm::instr::Instr;

    #[test]
    fn test_instruction_name_classifies_typed_dispatch_and_specialize() {
        let typed = Instr::CallTypedDispatch("f".to_string(), 2, 10, vec![(10, vec!["Any".into()])]);
        let specialize = Instr::CallSpecialize(42, 2);

        assert_eq!(instruction_name(&typed), "CallTypedDispatch");
        assert_eq!(instruction_name(&specialize), "CallSpecialize");
    }
}
