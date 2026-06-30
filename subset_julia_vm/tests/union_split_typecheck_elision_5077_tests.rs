//! Bytecode checks for branch-narrowed `isa` elimination (Issue #5077).

use subset_julia_vm::base;
use subset_julia_vm::builtins::BuiltinId;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};

fn compile_source_with_base(source: &str) -> CompiledProgram {
    let prelude_src = base::get_base();
    let mut parser = Parser::new().expect("create parser");
    let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let mut user_program = lowering.lower(parsed).expect("lower source");

    merge_programs(prelude_program, &mut user_program);
    compile_core_program(&user_program).expect("compile failed")
}

fn merge_programs(mut prelude: Program, user: &mut Program) {
    prelude.functions.append(&mut user.functions);
    user.functions = prelude.functions;

    prelude.structs.append(&mut user.structs);
    user.structs = prelude.structs;

    prelude.abstract_types.append(&mut user.abstract_types);
    user.abstract_types = prelude.abstract_types;
}

fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
    compiled
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function '{}' not found", name))
}

fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
    &compiled.code[f.code_start..f.code_end]
}

fn count_isa_calls(body: &[Instr]) -> usize {
    body.iter()
        .filter(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::Isa, 2)))
        .count()
}

fn assert_i64_arithmetic_specialized(compiled: &CompiledProgram, function_name: &str) {
    let func = get_function(compiled, function_name);
    let body = function_body(compiled, func);
    assert!(
        body.iter().any(|instr| matches!(instr, Instr::AddI64)),
        "{function_name} should specialize narrowed Int64 addition: {:?}",
        body
    );
    assert!(
        body.iter().all(|instr| !matches!(instr, Instr::DynamicAdd)),
        "{function_name} should not use dynamic addition after narrowing: {:?}",
        body
    );
}

#[test]
fn branch_narrowed_isa_checks_are_constant_folded_issue_5077() {
    let compiled = compile_source_with_base(
        r#"
function narrowed_isa_5077(x::Union{Int64,String})
    if x isa Int64
        return x isa Int64
    else
        return x isa String
    end
end
"#,
    );
    let func = get_function(&compiled, "narrowed_isa_5077");
    let body = function_body(&compiled, func);

    assert_eq!(
        count_isa_calls(body),
        1,
        "only the outer branch guard should remain as runtime isa: {:?}",
        body
    );
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::PushBool(true))),
        "branch-local isa checks should lower to PushBool(true): {:?}",
        body
    );
}

#[test]
fn typeof_guards_drive_branch_codegen_narrowing_issue_5077() {
    let compiled = compile_source_with_base(
        r#"
function narrowed_typeof_add_5077(x::Union{Int64,String})
    if typeof(x) === Int64
        return x + 1
    else
        return length(x)
    end
end

function narrowed_reversed_typeof_add_5077(x::Union{Int64,String})
    if Int64 == typeof(x)
        return x + 1
    else
        return length(x)
    end
end

function narrowed_typeof_not_else_add_5077(x::Union{Int64,String})
    if typeof(x) !== Int64
        return length(x)
    else
        return x + 1
    end
end
"#,
    );

    for function_name in [
        "narrowed_typeof_add_5077",
        "narrowed_reversed_typeof_add_5077",
        "narrowed_typeof_not_else_add_5077",
    ] {
        assert_i64_arithmetic_specialized(&compiled, function_name);
    }
}
