use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{CompiledProgram, Instr};

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    compile_core_program(&program).expect("compile failed")
}

#[test]
fn const_numeric_bindings_fold_to_immediate_bytecode_issue_5086() {
    let compiled = compile_source(
        r#"
const A = 20
const B = 22
A + B
"#,
    );
    let main = &compiled.code[compiled.entry..];

    assert!(
        main.windows(2)
            .any(|w| matches!(w, [Instr::PushI64(42), Instr::ReturnI64])),
        "const-folded main expression should return PushI64(42): {main:?}"
    );
    assert!(
        !main.iter().any(|instr| matches!(instr, Instr::AddI64)),
        "const-folded main expression should not emit AddI64: {main:?}"
    );
}

#[test]
fn const_bool_binding_eliminates_dead_if_branch_issue_5086() {
    let compiled = compile_source(
        r#"
const FLAG = false
x = 0
if FLAG
    x = 1
else
    x = 2
end
x
"#,
    );
    let main = &compiled.code[compiled.entry..];
    let user_tail = &main[main.len().saturating_sub(12)..];

    assert!(
        !user_tail
            .iter()
            .any(|instr| matches!(instr, Instr::JumpIfZero(_))),
        "const bool condition should remove the conditional jump: {main:?}"
    );
    assert!(
        !user_tail
            .iter()
            .any(|instr| matches!(instr, Instr::PushI64(1))),
        "dead then branch should not be emitted: {main:?}"
    );
    assert!(
        user_tail
            .iter()
            .any(|instr| matches!(instr, Instr::PushI64(2))),
        "live else branch should be emitted: {main:?}"
    );
}
