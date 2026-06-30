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

const ADVANCE_SUM_PAIRS_SOURCE: &str = r#"
function advance(a, b)
    while b > 0
        a += 1
        b -= 1
    end
    a
end

function sum_pairs(N)
    total = 0
    step = 2
    for i in 1:N
        total += advance(i, step)
    end
    total
end

sum_pairs(20)
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
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function '{name}' not found"))
}

fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
    &compiled.code[f.code_start..f.code_end]
}

#[test]
fn calc_pi_loop_increments_use_counted_loop_superinstructions_6167() {
    let compiled = compile_source(CALC_PI_SOURCE);
    let body = function_body(&compiled, get_function(&compiled, "calc_pi"));

    assert!(
        !body.windows(2).any(|window| {
            matches!(
                window,
                [Instr::PushI64(_), Instr::IncVarI64Slot(_)]
                    | [Instr::PushI64(_), Instr::DecVarI64Slot(_)]
            )
        }),
        "const-step loop increments should not leave PushI64 + Inc/DecVarI64Slot: {body:?}"
    );
    assert!(
        body.iter()
            .filter(|instr| matches!(instr, Instr::AddConstI64SlotAndJumpIfLe(_, _, _, _)))
            .count()
            >= 2,
        "calc_pi should use fused counted-loop backedges for inner and outer loop increments: {body:?}"
    );
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::AddConstI64Slot(_, 1))),
        "calc_pi should still use AddConstI64Slot for the conditional counter increment: {body:?}"
    );
    assert!(
        !body.windows(3).any(|window| {
            matches!(
                window,
                [
                    Instr::LoadSlotI64(_),
                    Instr::LoadSlotI64(_),
                    Instr::JumpIfGtI64(_)
                ]
            )
        }),
        "const-step loop exit tests should not leave LoadSlotI64 + LoadSlotI64 + JumpIfGtI64: {body:?}"
    );
    assert!(
        body.iter()
            .filter(|instr| matches!(instr, Instr::JumpIfGtI64Slots(_, _, _)))
            .count()
            >= 2,
        "calc_pi should use JumpIfGtI64Slots for inner and outer loop exits: {body:?}"
    );
    assert!(
        !body.windows(3).any(|window| {
            matches!(
                window,
                [
                    Instr::LoadSlotI64(_),
                    Instr::LoadSlotI64(_),
                    Instr::CallSpecialize(_, 2)
                ]
            )
        }),
        "slot-specialized calls should not leave LoadSlotI64 + LoadSlotI64 + CallSpecialize: {body:?}"
    );
    assert!(
        body.iter().any(|instr| matches!(
            instr,
            Instr::CallSpecializeI64Slots(operands)
                if operands.slots.len() == 2
        )),
        "calc_pi should pass typed loop slots directly into specialized calls: {body:?}"
    );

    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let result = vm.run().expect("run calc_pi(10)");
    match result {
        Value::F64(value) => assert!(
            (value - 3.0860669992418384).abs() < 1.0e-12,
            "unexpected calc_pi(10) result: {value}"
        ),
        other => panic!("expected Float64 calc_pi result, got {other:?}"),
    }
}

#[test]
fn i64_slot_specialized_calls_preserve_generic_function_results_6301() {
    let compiled = compile_source(ADVANCE_SUM_PAIRS_SOURCE);
    let body = function_body(&compiled, get_function(&compiled, "sum_pairs"));
    assert!(
        body.iter().any(|instr| matches!(
            instr,
            Instr::CallSpecializeI64Slots(operands)
                if operands.slots.len() == 2
        )),
        "sum_pairs should call advance through I64 slot arguments: {body:?}"
    );

    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let result = vm.run().expect("run sum_pairs");

    match result {
        Value::I64(value) => assert_eq!(value, 250),
        other => panic!("expected Int64 sum_pairs result, got {other:?}"),
    }
}

#[cfg(feature = "profiling")]
#[test]
fn generic_i64_slot_specialized_calls_use_direct_i64_function_path_6308() {
    let compiled = compile_source(ADVANCE_SUM_PAIRS_SOURCE);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));

    profiler::clear();
    profiler::enable();
    let result = vm.run().expect("run sum_pairs");
    profiler::disable();

    match result {
        Value::I64(value) => assert_eq!(value, 250),
        other => panic!("expected Int64 sum_pairs result, got {other:?}"),
    }

    let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
    assert!(
        counts
            .get("ExecutableBlock::I64Function")
            .copied()
            .unwrap_or(0)
            > 0,
        "generic I64 specialized calls should use the direct function path: {counts:?}"
    );
}
