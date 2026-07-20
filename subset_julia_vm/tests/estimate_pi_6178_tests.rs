use std::collections::HashSet;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::specialize::specialize_function;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, Value, ValueType};

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
fn pow2_random_point_test_lowers_to_typed_square_and_branch_6178() {
    let source = r#"
function random_point_inside_6178()::Int64
    x = rand()
    y = rand()
    if x^2 + y^2 <= 1
        return 1
    end
    return 0
end

random_point_inside_6178()
"#;

    let compiled = compile_source(source);
    let body = function_body(
        &compiled,
        get_function(&compiled, "random_point_inside_6178"),
    );

    assert!(
        body.iter()
            .filter(|instr| matches!(instr, Instr::LoadSquareF64Slot(_)))
            .count()
            >= 2,
        "expected both x^2 and y^2 to lower to LoadSquareF64Slot: {body:?}"
    );
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::JumpIfNotLeF64(_))),
        "expected Float64 <= false branch to fuse to JumpIfNotLeF64: {body:?}"
    );
    assert!(
        !body.iter().any(|instr| matches!(instr, Instr::DynamicPow)),
        "literal exponent 2 on Float64 slots must not emit DynamicPow: {body:?}"
    );
    assert!(
        !body.iter().any(|instr| matches!(
            instr,
            Instr::Call(_, 2)
                | Instr::CallResolved(_, 2)
                | Instr::CallDynamicBinary(_, _, _)
                | Instr::CallDynamicBinaryBoth(_, _)
                | Instr::CallDynamicBinaryNoFallback(_)
        )),
        "random point comparison should not use generic binary calls: {body:?}"
    );
}

#[test]
fn runtime_specialized_estimate_pi_has_no_issue_6178_hot_loop_dynamic_ops() {
    let source = r#"
function estimate_pi(n)
    inside = 0
    for _ in 1:n
        x, y = rand(), rand()
        if x^2 + y^2 <= 1
            inside += 1
        end
    end
    return 4 * inside / n
end

estimate_pi(10000)
"#;

    let compiled = compile_source(source);
    let specializable = compiled
        .specializable_functions
        .iter()
        .find(|f| f.name == "estimate_pi")
        .expect("estimate_pi should be registered for runtime specialization");
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        &specializable.ir,
        &[ValueType::I64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize estimate_pi");
    let code = &specialized.code;

    assert!(
        !code
            .iter()
            .any(|instr| matches!(instr, Instr::DynamicToI64)),
        "estimate_pi(::Int64) specialization should not convert n dynamically: {code:?}"
    );
    assert!(
        !code.iter().any(|instr| matches!(
            instr,
            Instr::NewTuple(_)
                | Instr::StoreSlotTuple(_)
                | Instr::LoadSlotTuple(_)
                | Instr::TupleGet
        )),
        "x, y = rand(), rand() should not leave tuple destructuring traffic: {code:?}"
    );
    assert!(
        !code.iter().any(|instr| matches!(instr, Instr::DynamicPow)),
        "x^2/y^2 should not emit DynamicPow in estimate_pi(::Int64): {code:?}"
    );
    assert!(
        !code.iter().any(|instr| matches!(
            instr,
            Instr::Call(_, 2)
                | Instr::CallResolved(_, 2)
                | Instr::CallDynamicBinary(_, _, _)
                | Instr::CallDynamicBinaryBoth(_, _)
                | Instr::CallDynamicBinaryNoFallback(_)
        )),
        "estimate_pi(::Int64) hot arithmetic should not use generic binary calls: {code:?}"
    );
    assert!(
        code.iter().any(|instr| matches!(instr, Instr::RandF64)),
        "specialization should retain direct RandF64 instructions: {code:?}"
    );
    assert!(
        code.windows(2)
            .any(|pair| matches!(pair, [Instr::DupF64, Instr::MulF64])),
        "specialization should lower x^2/y^2 to typed square arithmetic: {code:?}"
    );
    assert!(
        code.iter().any(|instr| matches!(instr, Instr::DivF64)),
        "4 * inside / n should end on typed Float64 division: {code:?}"
    );
    assert_eq!(specialized.return_type, ValueType::F64);
}

#[test]
fn estimate_pi_original_and_typed_friendly_match_seeded_result_6178() {
    let original = r#"
function estimate_pi(n)
    inside = 0
    for _ in 1:n
        x, y = rand(), rand()
        if x^2 + y^2 <= 1
            inside += 1
        end
    end
    return 4 * inside / n
end

estimate_pi(10000)
"#;
    let typed_friendly = r#"
function estimate_pi(n::Int64)::Float64
    inside = 0
    for _ in 1:n
        x = rand()
        y = rand()
        if x * x + y * y <= 1.0
            inside += 1
        end
    end
    return 4.0 * Float64(inside) / Float64(n)
end

estimate_pi(10000)
"#;

    let mut original_vm = Vm::new_program(compile_source(original), StableRng::new(0));
    let mut typed_vm = Vm::new_program(compile_source(typed_friendly), StableRng::new(0));
    let original_result = original_vm.run().expect("run original estimate_pi");
    let typed_result = typed_vm.run().expect("run typed-friendly estimate_pi");

    match (original_result, typed_result) {
        (Value::F64(original_value), Value::F64(typed_value)) => {
            assert_eq!(original_value, typed_value);
        }
        (original_result, typed_result) => panic!(
            "expected Float64 results from both estimate_pi forms, got {original_result:?} and {typed_result:?}"
        ),
    }
}
