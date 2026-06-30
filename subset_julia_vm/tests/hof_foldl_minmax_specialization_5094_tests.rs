//! Bytecode checks for `foldl(min/max, ::Vector{T})` specialization (Issue #5094).

mod common;

use common::{has_resolved_or_typed_candidate, resolved_target_debug};
use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr, ValueType};

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

#[test]
fn foldl_minmax_vector_uses_typed_specialized_candidate_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function foldl_min_i32_5094(xs::Vector{Int32})
    foldl(min, xs)
end

function foldl_max_i32_5094(xs::Vector{Int32})
    foldl(max, xs)
end

foldl_min_i32_5094(Int32[-3, 0, 4, 2])
foldl_max_i32_5094(Int32[-3, 0, 4, 2])
"#,
    );

    let min_i32_func = get_function(&compiled, "foldl_min_i32_5094");
    assert_eq!(min_i32_func.return_type, ValueType::I32);
    let min_body = function_body(&compiled, min_i32_func);
    let has_min_i32_specialized_foldl_candidate = has_resolved_or_typed_candidate(
        &compiled,
        min_body,
        "foldl",
        2,
        &["typeof(min)", "Vector{Int32}"],
    );
    assert!(
        has_min_i32_specialized_foldl_candidate,
        "typed foldl(min, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
        min_body,
        resolved_target_debug(&compiled, min_body)
    );

    let max_i32_func = get_function(&compiled, "foldl_max_i32_5094");
    assert_eq!(max_i32_func.return_type, ValueType::I32);
    let max_body = function_body(&compiled, max_i32_func);
    let has_max_i32_specialized_foldl_candidate = has_resolved_or_typed_candidate(
        &compiled,
        max_body,
        "foldl",
        2,
        &["typeof(max)", "Vector{Int32}"],
    );
    assert!(
        has_max_i32_specialized_foldl_candidate,
        "typed foldl(max, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
        max_body,
        resolved_target_debug(&compiled, max_body)
    );
}
