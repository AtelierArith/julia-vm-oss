//! Bytecode checks for binary numeric broadcast specialization (Issue #5094).

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

fn has_resolved_or_typed_broadcast_candidate(
    compiled: &CompiledProgram,
    body: &[Instr],
    callable_name: &str,
    element_type: &str,
) -> bool {
    body.iter().any(|instr| {
        matches!(instr, Instr::CallResolved(_, 3))
            || matches!(
                instr,
                // Issue #6496: CallTypedDispatch candidates are function
                // indices; the expected signature is derived from each
                // candidate's FunctionInfo.
                Instr::CallTypedDispatch(name, 3, _, candidates)
                    if name == "broadcast"
                        && candidates.iter().any(|idx| {
                            compiled.functions.get(*idx).is_some_and(|target_func| {
                                target_func
                                    .param_julia_types
                                    .iter()
                                    .map(|ty| ty.to_string())
                                    .eq([
                                        format!("typeof({callable_name})"),
                                        element_type.to_string(),
                                        element_type.to_string(),
                                    ])
                            })
                        })
            )
    })
}

#[test]
fn binary_broadcast_vector_uses_typed_specialized_candidate_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function broadcast_add_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    broadcast(+, xs, ys)
end

function broadcast_sub_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    broadcast(-, xs, ys)
end

function broadcast_mul_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    broadcast(*, xs, ys)
end

function broadcast_div_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    broadcast(/, xs, ys)
end

function broadcast_div_f32_5094(xs::Vector{Float32}, ys::Vector{Float32})
    broadcast(/, xs, ys)
end

function broadcast_div_f64_5094(xs::Vector{Float64}, ys::Vector{Float64})
    broadcast(/, xs, ys)
end

function broadcast_min_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    broadcast(min, xs, ys)
end

function broadcast_max_f32_5094(xs::Vector{Float32}, ys::Vector{Float32})
    broadcast(max, xs, ys)
end

broadcast_add_i32_5094(Int32[1, 2], Int32[3, 4])
broadcast_sub_i32_5094(Int32[1, 2], Int32[3, 4])
broadcast_mul_i32_5094(Int32[1, 2], Int32[3, 4])
broadcast_div_i32_5094(Int32[1, 2], Int32[3, 4])
broadcast_div_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0])
broadcast_div_f64_5094([1.0, 2.0], [3.0, 4.0])
broadcast_min_i32_5094(Int32[1, 2], Int32[3, 4])
broadcast_max_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0])
"#,
    );

    for (function_name, callable_name) in [
        ("broadcast_add_i32_5094", "+"),
        ("broadcast_sub_i32_5094", "-"),
        ("broadcast_mul_i32_5094", "*"),
        ("broadcast_min_i32_5094", "min"),
    ] {
        let func = get_function(&compiled, function_name);
        assert_eq!(
            func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert!(
            has_resolved_or_typed_broadcast_candidate(
                &compiled,
                function_body(&compiled, func),
                callable_name,
                "Vector{Int32}",
            ),
            "typed broadcast({}, Vector{{Int32}}, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
            callable_name,
            function_body(&compiled, func)
        );
    }

    let max_f32_func = get_function(&compiled, "broadcast_max_f32_5094");
    assert_eq!(
        max_f32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::F32, None)
    );
    assert!(
        has_resolved_or_typed_broadcast_candidate(
            &compiled,
            function_body(&compiled, max_f32_func),
            "max",
            "Vector{Float32}",
        ),
        "typed broadcast(max, Vector{{Float32}}, Vector{{Float32}}) dispatch should include the concrete specialization: {:?}",
        function_body(&compiled, max_f32_func)
    );

    let div_func = get_function(&compiled, "broadcast_div_i32_5094");
    assert_eq!(
        div_func.return_type,
        ValueType::ArrayOf(ArrayElementType::F64, None)
    );
    assert!(
        has_resolved_or_typed_broadcast_candidate(
            &compiled,
            function_body(&compiled, div_func),
            "/",
            "Vector{Int32}",
        ),
        "typed broadcast(/, Vector{{Int32}}, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
        function_body(&compiled, div_func)
    );

    for (function_name, element_type, return_type) in [
        (
            "broadcast_div_f32_5094",
            "Vector{Float32}",
            ValueType::ArrayOf(ArrayElementType::F32, None),
        ),
        (
            "broadcast_div_f64_5094",
            "Vector{Float64}",
            ValueType::ArrayOf(ArrayElementType::F64, None),
        ),
    ] {
        let func = get_function(&compiled, function_name);
        assert_eq!(func.return_type, return_type);
        assert!(
            has_resolved_or_typed_broadcast_candidate(
                &compiled,
                function_body(&compiled, func),
                "/",
                element_type,
            ),
            "typed broadcast(/, {0}, {0}) dispatch should include the concrete specialization: {1:?}",
            element_type,
            function_body(&compiled, func)
        );
    }
}
