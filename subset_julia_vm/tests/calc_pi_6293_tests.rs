#[cfg(feature = "profiling")]
use std::collections::HashMap;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
#[cfg(feature = "profiling")]
use subset_julia_vm::vm::profiler;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr, Value, Vm};

const CALC_PI_SOURCE: &str = r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

calc_pi(10)
"#;

const BASE_GCD_CALC_PI_SOURCE: &str = r#"
function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if gcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

calc_pi(10)
"#;

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    compile_with_cache(&program).expect("compile source")
}

fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
    compiled
        .functions
        .iter()
        .find(|func| func.name == name)
        .unwrap_or_else(|| panic!("function '{name}' not found"))
}

fn function_body<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a [Instr] {
    let func = get_function(compiled, name);
    &compiled.code[func.entry..func.code_end]
}

fn is_gcd_function(func: &FunctionInfo) -> bool {
    func.name == "gcd" || func.name.ends_with(".gcd")
}

fn run_calc_pi_10() -> Value {
    let mut vm = Vm::new_program(compile_source(CALC_PI_SOURCE), StableRng::new(0));
    vm.run().expect("run calc_pi(10)")
}

#[test]
fn calc_pi_original_source_preserves_result_6293() {
    let result = run_calc_pi_10();
    match result {
        Value::F64(value) => assert!(
            (value - 3.0860669992418384).abs() < 1.0e-12,
            "unexpected calc_pi(10) result: {value}"
        ),
        other => panic!("expected Float64 calc_pi result, got {other:?}"),
    }
}

#[test]
fn base_gcd_calc_pi_uses_slot_resolved_call_6315() {
    let compiled = compile_source(BASE_GCD_CALC_PI_SOURCE);
    let body = function_body(&compiled, "calc_pi");
    let gcd_slot_calls = body
        .iter()
        .filter(|instr| match instr {
            Instr::CallResolvedI64Slots(operands) if operands.slots.len() == 2 => compiled
                .functions
                .get(operands.func_index)
                .map(is_gcd_function)
                .unwrap_or(false),
            _ => false,
        })
        .count();

    assert!(
        gcd_slot_calls > 0,
        "Base gcd calls should load arguments directly from I64 slots: {body:?}"
    );
    assert!(
        !body.windows(3).any(|window| {
            matches!(
                window,
                [
                    Instr::LoadSlotI64(_),
                    Instr::LoadSlotI64(_),
                    Instr::CallResolved(func_index, 2)
                ] if compiled
                    .functions
                    .get(*func_index)
                    .map(is_gcd_function)
                    .unwrap_or(false)
            )
        }),
        "Base gcd should not keep the old LoadSlotI64/LoadSlotI64/CallResolved sequence: {body:?}"
    );
}

#[cfg(feature = "profiling")]
#[test]
fn untyped_calc_pi_uses_specialize_i64_dispatch_cache_8167() {
    // Issue #8167: an untyped callee (`mygcd`) invoked from a
    // `CallSpecializeI64Slots` site must resolve its all-`I64` specialization
    // once and then take the cheap `(spec_func_index, arity)` dispatch cache,
    // rather than rebuilding/hashing a `Vec`-keyed `SpecializationKey` per call.
    profiler::clear();
    profiler::enable();
    let result = run_calc_pi_10();
    profiler::disable();

    match result {
        Value::F64(value) => assert!(
            (value - 3.0860669992418384).abs() < 1.0e-12,
            "unexpected calc_pi(10) result: {value}"
        ),
        other => panic!("expected Float64 calc_pi result, got {other:?}"),
    }

    let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
    assert!(
        counts
            .get("SpecializeI64DispatchCacheHit")
            .copied()
            .unwrap_or(0)
            > 0,
        "untyped mygcd should hit the I64 specialize dispatch cache: {counts:?}"
    );
}

#[cfg(feature = "profiling")]
#[test]
fn calc_pi_uses_direct_euclidean_modulo_i64_function_path_6293() {
    profiler::clear();
    profiler::enable();
    let result = run_calc_pi_10();
    profiler::disable();

    match result {
        Value::F64(value) => assert!(
            (value - 3.0860669992418384).abs() < 1.0e-12,
            "unexpected calc_pi(10) result: {value}"
        ),
        other => panic!("expected Float64 calc_pi result, got {other:?}"),
    }

    let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
    assert!(
        counts
            .get("ExecutableBlock::EuclideanModuloI64Function")
            .copied()
            .unwrap_or(0)
            > 0,
        "calc_pi should direct-execute Euclidean modulo I64 loop calls: {counts:?}"
    );
    assert!(
        counts
            .get("ExecutableBlock::I64FunctionCompareBranch")
            .copied()
            .unwrap_or(0)
            > 0,
        "calc_pi should consume gcd-result equality branches directly: {counts:?}"
    );
}
