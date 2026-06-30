//! Bytecode checks for predicate reducer specialization (Issue #5094).

use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::types::JuliaType;
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

fn resolved_target_debug(
    compiled: &CompiledProgram,
    body: &[Instr],
) -> Vec<(String, Vec<JuliaType>)> {
    body.iter()
        .filter_map(|instr| match instr {
            Instr::CallResolved(target, _) => {
                let target_func = &compiled.functions[*target];
                Some((
                    target_func.name.clone(),
                    target_func.param_julia_types.clone(),
                ))
            }
            _ => None,
        })
        .collect()
}

fn has_specialized_candidate(
    compiled: &CompiledProgram,
    body: &[Instr],
    function: &str,
    predicate: &str,
) -> bool {
    let expected_predicate = format!("typeof({predicate})");
    body.iter().any(|instr| match instr {
        // Issue #6496: CallTypedDispatch candidates are function indices; the
        // expected signature is derived from each candidate's FunctionInfo.
        Instr::CallTypedDispatch(name, 2, _, candidates) if name == function => {
            candidates.iter().any(|idx| {
                compiled.functions.get(*idx).is_some_and(|target_func| {
                    let signature: Vec<String> = target_func
                        .param_julia_types
                        .iter()
                        .map(|ty| ty.to_string())
                        .collect();
                    signature.len() == 2
                        && signature[0] == expected_predicate
                        && signature[1] == "Vector{Int32}"
                })
            })
        }
        Instr::CallResolved(target, 2) => {
            let target_func = &compiled.functions[*target];
            target_func.name == function
                && matches!(
                    target_func.param_julia_types.as_slice(),
                    [
                        JuliaType::Struct(function_type),
                        JuliaType::VectorOf(element),
                    ] if function_type == &expected_predicate
                        && **element == JuliaType::Int32
                )
        }
        _ => false,
    })
}

#[test]
fn predicate_reducers_use_typed_specialized_candidates_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function any_iszero_i32_5094(xs::Vector{Int32})
    any(iszero, xs)
end

function all_iseven_i32_5094(xs::Vector{Int32})
    all(iseven, xs)
end

function count_signbit_i32_5094(xs::Vector{Int32})
    count(signbit, xs)
end

function findall_isodd_i32_5094(xs::Vector{Int32})
    findall(isodd, xs)
end

any_iszero_i32_5094(Int32[-3, 0, 4])
all_iseven_i32_5094(Int32[0, 4])
count_signbit_i32_5094(Int32[-3, 0, 4])
findall_isodd_i32_5094(Int32[-3, 0, 5])
"#,
    );

    let any_func = get_function(&compiled, "any_iszero_i32_5094");
    assert_eq!(any_func.return_type, ValueType::Bool);
    assert!(
        has_specialized_candidate(&compiled, function_body(&compiled, any_func), "any", "iszero"),
        "typed any(iszero, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}; resolved targets: {:?}",
        function_body(&compiled, any_func),
        resolved_target_debug(&compiled, function_body(&compiled, any_func))
    );

    let all_func = get_function(&compiled, "all_iseven_i32_5094");
    assert_eq!(all_func.return_type, ValueType::Bool);
    assert!(
        has_specialized_candidate(&compiled, function_body(&compiled, all_func), "all", "iseven"),
        "typed all(iseven, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
        function_body(&compiled, all_func)
    );

    let count_func = get_function(&compiled, "count_signbit_i32_5094");
    assert_eq!(count_func.return_type, ValueType::I64);
    assert!(
        has_specialized_candidate(
            &compiled,
            function_body(&compiled, count_func),
            "count",
            "signbit"
        ),
        "typed count(signbit, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
        function_body(&compiled, count_func)
    );

    let findall_func = get_function(&compiled, "findall_isodd_i32_5094");
    assert!(
        has_specialized_candidate(
            &compiled,
            function_body(&compiled, findall_func),
            "findall",
            "isodd"
        ),
        "typed findall(isodd, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
        function_body(&compiled, findall_func)
    );
}
