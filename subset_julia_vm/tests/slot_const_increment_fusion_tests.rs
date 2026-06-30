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
fn slot_const_increment_fuses_iadd_one_update() {
    let source = r#"
function count_to(n::Int64)::Int64
    i = 0
    while i < n
        i += 1
    end
    return i
end

count_to(3)
"#;

    let compiled = compile_source(source);
    let body = function_body(&compiled, get_function(&compiled, "count_to"));
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::AddConstI64Slot(_, 1))),
        "expected i += 1 to fuse to AddConstI64Slot: {body:?}"
    );
    assert!(
        !body.windows(4).any(|window| matches!(
            window,
            [
                Instr::LoadSlotI64(_),
                Instr::PushI64(1),
                Instr::AddI64,
                Instr::StoreSlotI64(_)
            ]
        )),
        "unfused slot increment remains: {body:?}"
    );

    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run().expect("vm run failed");
    assert!(matches!(result, Value::I64(3)));
}
