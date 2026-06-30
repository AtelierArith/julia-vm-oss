//! Bytecode checks for unary predicate broadcast specialization (Issue #5094).

mod common;

use common::{has_resolved_or_typed_candidate, resolved_target_debug};
use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType};

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
fn predicate_broadcast_vector_uses_typed_specialized_candidate_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function broadcast_iszero_i32_5094(xs::Vector{Int32})
    broadcast(iszero, xs)
end

function broadcast_signbit_i32_5094(xs::Vector{Int32})
    broadcast(signbit, xs)
end

function broadcast_iseven_i32_5094(xs::Vector{Int32})
    broadcast(iseven, xs)
end

broadcast_iszero_i32_5094(Int32[-3, 0, 4])
broadcast_signbit_i32_5094(Int32[-3, 0, 4])
broadcast_iseven_i32_5094(Int32[-3, 0, 4])
"#,
    );

    for (function_name, callable_name) in [
        ("broadcast_iszero_i32_5094", "iszero"),
        ("broadcast_signbit_i32_5094", "signbit"),
        ("broadcast_iseven_i32_5094", "iseven"),
    ] {
        let func = get_function(&compiled, function_name);
        assert_eq!(
            func.return_type,
            ValueType::ArrayOf(ArrayElementType::Bool, None)
        );
        let expected_callable = format!("typeof({callable_name})");
        let body = function_body(&compiled, func);
        let has_specialized_broadcast_candidate = has_resolved_or_typed_candidate(
            &compiled,
            body,
            "broadcast",
            2,
            &[&expected_callable, "Vector{Int32}"],
        );
        assert!(
            has_specialized_broadcast_candidate,
            "typed broadcast({}, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            callable_name,
            body,
            resolved_target_debug(&compiled, body)
        );
    }
}
