//! Bytecode checks for binary HOF map specialization (Issue #5094).

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
fn binary_map_vector_uses_concrete_resolved_method_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function map_add_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    map(+, xs, ys)
end

function map_div_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    map(/, xs, ys)
end

function map_div_f32_5094(xs::Vector{Float32}, ys::Vector{Float32})
    map(/, xs, ys)
end

function map_min_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
    map(min, xs, ys)
end

function map_max_bool_5094(xs::Vector{Bool}, ys::Vector{Bool})
    map(max, xs, ys)
end

map_add_i32_5094(Int32[1, 2], Int32[3, 4])
map_div_i32_5094(Int32[1, 2], Int32[3, 4])
map_div_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0])
map_min_i32_5094(Int32[1, 2], Int32[3, 4])
map_max_bool_5094(Bool[true, false], Bool[false, true])
"#,
    );

    for (function_name, callable_name, element_type, return_type) in [
        (
            "map_add_i32_5094",
            "+",
            "Vector{Int32}",
            ValueType::ArrayOf(ArrayElementType::I32, None),
        ),
        (
            "map_div_i32_5094",
            "/",
            "Vector{Int32}",
            ValueType::ArrayOf(ArrayElementType::F64, None),
        ),
        (
            "map_div_f32_5094",
            "/",
            "Vector{Float32}",
            ValueType::ArrayOf(ArrayElementType::F32, None),
        ),
        (
            "map_min_i32_5094",
            "min",
            "Vector{Int32}",
            ValueType::ArrayOf(ArrayElementType::I32, None),
        ),
        (
            "map_max_bool_5094",
            "max",
            "Vector{Bool}",
            ValueType::ArrayOf(ArrayElementType::Bool, None),
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
                .any(|instr| matches!(instr, Instr::CallResolved(_, 3))),
            "typed map({}, {}, {}) should resolve to a concrete 3-arg method: {:?}",
            callable_name,
            element_type,
            element_type,
            function_body(&compiled, func)
        );
    }
}
