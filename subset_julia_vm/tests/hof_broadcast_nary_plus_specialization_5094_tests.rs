//! Bytecode checks for n-ary numeric broadcast specialization (Issue #5094).

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
fn nary_broadcast_plus_vector_uses_typed_specialized_candidate_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function broadcast_plus3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
    broadcast(+, xs, ys, zs)
end

function broadcast_plus3_f32_5094(xs::Vector{Float32}, ys::Vector{Float32}, zs::Vector{Float32})
    broadcast(+, xs, ys, zs)
end

function broadcast_plus4_i32_5094(a::Vector{Int32}, b::Vector{Int32}, c::Vector{Int32}, d::Vector{Int32})
    broadcast(+, a, b, c, d)
end

function broadcast_plus5_i32_5094(a::Vector{Int32}, b::Vector{Int32}, c::Vector{Int32}, d::Vector{Int32}, e::Vector{Int32})
    broadcast(+, a, b, c, d, e)
end

function broadcast_mul3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
    broadcast(*, xs, ys, zs)
end

function broadcast_mul3_bool_5094(xs::Vector{Bool}, ys::Vector{Bool}, zs::Vector{Bool})
    broadcast(*, xs, ys, zs)
end

function broadcast_max3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
    broadcast(max, xs, ys, zs)
end

function broadcast_min3_f32_5094(xs::Vector{Float32}, ys::Vector{Float32}, zs::Vector{Float32})
    broadcast(min, xs, ys, zs)
end

broadcast_plus3_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6])
broadcast_plus3_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0], Float32[5.0, 6.0])
broadcast_plus4_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6], Int32[7, 8])
broadcast_plus5_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6], Int32[7, 8], Int32[9, 10])
broadcast_mul3_i32_5094(Int32[2, 3], Int32[4, 5], Int32[6, 7])
broadcast_mul3_bool_5094([true, false], [true, true], [false, true])
broadcast_max3_i32_5094(Int32[1, 20], Int32[10, -2], Int32[100, 2])
broadcast_min3_f32_5094(Float32[1.0, 20.0], Float32[10.0, -2.0], Float32[100.0, 2.0])
"#,
    );

    for (function_name, arg_count, return_type) in [
        (
            "broadcast_plus3_i32_5094",
            4,
            ValueType::ArrayOf(ArrayElementType::I32, None),
        ),
        (
            "broadcast_plus3_f32_5094",
            4,
            ValueType::ArrayOf(ArrayElementType::F32, None),
        ),
        (
            "broadcast_plus4_i32_5094",
            5,
            ValueType::ArrayOf(ArrayElementType::I32, None),
        ),
        (
            "broadcast_plus5_i32_5094",
            6,
            ValueType::ArrayOf(ArrayElementType::I32, None),
        ),
        (
            "broadcast_mul3_i32_5094",
            4,
            ValueType::ArrayOf(ArrayElementType::I32, None),
        ),
        (
            "broadcast_mul3_bool_5094",
            4,
            ValueType::ArrayOf(ArrayElementType::Bool, None),
        ),
        (
            "broadcast_max3_i32_5094",
            4,
            ValueType::ArrayOf(ArrayElementType::I32, None),
        ),
        (
            "broadcast_min3_f32_5094",
            4,
            ValueType::ArrayOf(ArrayElementType::F32, None),
        ),
    ] {
        let func = get_function(&compiled, function_name);
        assert_eq!(
            func.return_type,
            return_type,
            "{function_name} body: {:?}",
            function_body(&compiled, func)
        );
        assert!(
            function_body(&compiled, func)
                .iter()
                .any(|instr| matches!(instr, Instr::CallResolved(_, n) if *n == arg_count)),
            "typed n-ary broadcast should resolve to a concrete method: {:?}",
            function_body(&compiled, func)
        );
    }
}
