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
fn f64_compare_jump_fusion_preserves_nan_false_branch() {
    let source = r#"
function branch_le(x::Float64)::Int64
    if x <= 4.0
        return 1
    end
    return 2
end

branch_le(0.0 / 0.0)
"#;

    let compiled = compile_source(source);
    let body = function_body(&compiled, get_function(&compiled, "branch_le"));
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::JumpIfNotLeF64(_))),
        "expected LeF64 + JumpIfZero to fuse to JumpIfNotLeF64: {body:?}"
    );
    assert!(
        !body
            .windows(2)
            .any(|pair| matches!(pair, [Instr::LeF64, Instr::JumpIfZero(_)])),
        "unfused LeF64 + JumpIfZero remains: {body:?}"
    );

    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run().expect("vm run failed");
    assert!(matches!(result, Value::I64(2)));
}
