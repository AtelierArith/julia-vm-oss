#[cfg(feature = "profiling")]
use std::collections::HashMap;

use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
#[cfg(feature = "profiling")]
use subset_julia_vm::vm::profiler;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr, Value, Vm};

const NESTED_RESOLVED_I64_CALL_SOURCE: &str = r#"
function score6314(x::Int64)
    y = x * x
    return y + 1
end

function sum_score6314(n::Int64)
    total = 0
    for i in 1:n
        total += score6314(i)
    end
    return total
end

sum_score6314(20)
"#;

const DIRECT_SLOT_RESOLVED_I64_CALL_SOURCE: &str = r#"
function score6315(x::Int64, y::Int64)
    z = x + y
    return z * y
end

function sum_score6315(n::Int64)
    total = 0
    step = 2
    for i in 1:n
        total += score6315(i, step)
    end
    return total
end

sum_score6315(20)
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

#[test]
fn nested_resolved_i64_helper_preserves_result_6314() {
    let compiled = compile_source(NESTED_RESOLVED_I64_CALL_SOURCE);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let result = vm.run().expect("run nested resolved I64 helper");

    match result {
        Value::I64(value) => assert_eq!(value, 2890),
        other => panic!("expected Int64 sum_score6314 result, got {other:?}"),
    }
}

#[test]
fn direct_slot_resolved_i64_helper_preserves_result_6315() {
    let compiled = compile_source(DIRECT_SLOT_RESOLVED_I64_CALL_SOURCE);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let result = vm.run().expect("run direct slot resolved I64 helper");

    match result {
        Value::I64(value) => assert_eq!(value, 500),
        other => panic!("expected Int64 sum_score6315 result, got {other:?}"),
    }
}

#[test]
fn direct_slot_resolved_i64_helper_uses_slot_call_6315() {
    let compiled = compile_source(DIRECT_SLOT_RESOLVED_I64_CALL_SOURCE);
    let body = function_body(&compiled, "sum_score6315");
    let slot_calls = body
        .iter()
        .filter(|instr| match instr {
            Instr::CallResolvedI64Slots(operands) if operands.slots.len() == 2 => compiled
                .functions
                .get(operands.func_index)
                .map(|func| func.name == "score6315")
                .unwrap_or(false),
            _ => false,
        })
        .count();

    assert!(
        slot_calls > 0,
        "resolved non-gcd I64 helper should read call arguments from slots: {body:?}"
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
                    .map(|func| func.name == "score6315")
                    .unwrap_or(false)
            )
        }),
        "non-gcd helper should not keep the old LoadSlotI64/LoadSlotI64/CallResolved sequence: {body:?}"
    );
}

#[cfg(feature = "profiling")]
#[test]
fn nested_resolved_i64_helper_uses_nested_i64_function_block_6314() {
    let compiled = compile_source(NESTED_RESOLVED_I64_CALL_SOURCE);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));

    profiler::clear();
    profiler::enable();
    let result = vm.run().expect("run nested resolved I64 helper");
    profiler::disable();

    match result {
        Value::I64(value) => assert_eq!(value, 2890),
        other => panic!("expected Int64 sum_score6314 result, got {other:?}"),
    }

    let counts: HashMap<String, u64> = profiler::get_results().into_iter().collect();
    assert!(
        counts
            .get("ExecutableBlock::I64FunctionNestedCall")
            .copied()
            .unwrap_or(0)
            > 0,
        "resolved helper calls inside I64Function blocks should use nested I64 execution: {counts:?}"
    );
}
