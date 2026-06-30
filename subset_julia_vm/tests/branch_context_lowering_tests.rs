use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr, Value, Vm};

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
fn while_and_condition_branches_without_bool_materialization() {
    let source = r#"
function branch_loop(x::Float64, y::Int64)::Int64
    total = 0
    while x <= 4.0 && y < 3
        total += 1
        y += 1
    end
    return total
end

branch_loop(1.0, 0)
"#;

    let compiled = compile_source(source);
    let body = function_body(&compiled, get_function(&compiled, "branch_loop"));

    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::JumpIfNotLeF64(_))),
        "expected Float64 false branch fusion in branch-context &&: {body:?}"
    );
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::JumpIfGeI64(_))),
        "expected Int64 false branch fusion in branch-context &&: {body:?}"
    );
    assert!(
        !body.iter().any(|instr| matches!(instr, Instr::PushBool(_))),
        "branch-context && should not materialize Bool values: {body:?}"
    );

    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let result = vm.run().expect("vm run failed");
    assert!(matches!(result, Value::I64(3)));
}

#[test]
fn if_and_condition_rejects_non_bool_left_operand() {
    let source = r#"
function bad_condition_6162()::Int64
    if 1 && true
        return 1
    end
    return 2
end

bad_condition_6162()
"#;

    let compiled = compile_source(source);
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let err = vm.run().expect_err("non-Bool && condition should fail");
    let message = err.to_string();
    assert!(
        message.contains("Type error") || message.contains("non-boolean"),
        "expected Bool condition TypeError for Issue #6162, got: {message}"
    );
}
