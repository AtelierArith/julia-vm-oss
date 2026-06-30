//! Bytecode checks for unary HOF callee/element-type specialization (Issue #5094).

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

fn assert_specialized_hof_candidate(
    compiled: &CompiledProgram,
    func: &FunctionInfo,
    function: &str,
    expected_signature: &[&str],
    message: &str,
) {
    let body = function_body(compiled, func);
    assert!(
        has_resolved_or_typed_candidate(
            compiled,
            body,
            function,
            expected_signature.len(),
            expected_signature
        ),
        "{message}: {:?}; resolved targets: {:?}",
        body,
        resolved_target_debug(compiled, body)
    );
}

#[test]
fn unary_map_abs_vector_uses_typed_specialized_candidate_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function unary_map_abs_5094(xs::Vector{Int64})
    map(abs, xs)
end

function unary_map_abs2_5094(xs::Vector{Int64})
    map(abs2, xs)
end

function unary_map_abs_i32_5094(xs::Vector{Int32})
    map(abs, xs)
end

function unary_map_identity_i32_5094(xs::Vector{Int32})
    map(identity, xs)
end

function unary_map_iszero_i32_5094(xs::Vector{Int32})
    map(iszero, xs)
end

function unary_map_isone_i32_5094(xs::Vector{Int32})
    map(isone, xs)
end

function unary_map_signbit_i32_5094(xs::Vector{Int32})
    map(signbit, xs)
end

function unary_map_iseven_i32_5094(xs::Vector{Int32})
    map(iseven, xs)
end

function unary_map_isodd_i32_5094(xs::Vector{Int32})
    map(isodd, xs)
end

function unary_map_neg_i32_5094(xs::Vector{Int32})
    map(-, xs)
end

unary_map_abs_5094([-3, 0, 4])
unary_map_abs2_5094([-3, 0, 4])
unary_map_abs_i32_5094(Int32[-3, 0, 4])
unary_map_identity_i32_5094(Int32[-3, 0, 4])
unary_map_iszero_i32_5094(Int32[-3, 0, 4])
unary_map_isone_i32_5094(Int32[0, 1, 2])
unary_map_signbit_i32_5094(Int32[-3, 0, 4])
unary_map_iseven_i32_5094(Int32[-3, 0, 4])
unary_map_isodd_i32_5094(Int32[-3, 0, 4])
unary_map_neg_i32_5094(Int32[-3, 0, 4])
"#,
    );
    let func = get_function(&compiled, "unary_map_abs_5094");

    assert_eq!(
        func.return_type,
        ValueType::ArrayOf(ArrayElementType::I64, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        func,
        "map",
        &["typeof(abs)", "Vector{Int64}"],
        "typed map(abs, Vector{Int64}) dispatch should resolve to the concrete specialization",
    );

    let abs2_func = get_function(&compiled, "unary_map_abs2_5094");
    assert_eq!(
        abs2_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I64, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        abs2_func,
        "map",
        &["typeof(abs2)", "Vector{Int64}"],
        "typed map(abs2, Vector{Int64}) dispatch should resolve to the concrete specialization",
    );

    let abs_i32_func = get_function(&compiled, "unary_map_abs_i32_5094");
    assert_eq!(
        abs_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I32, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        abs_i32_func,
        "map",
        &["typeof(abs)", "Vector{Int32}"],
        "typed map(abs, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let identity_i32_func = get_function(&compiled, "unary_map_identity_i32_5094");
    assert_eq!(
        identity_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I32, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        identity_i32_func,
        "map",
        &["typeof(identity)", "Vector{Int32}"],
        "typed map(identity, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let iszero_i32_func = get_function(&compiled, "unary_map_iszero_i32_5094");
    assert_eq!(
        iszero_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::Bool, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        iszero_i32_func,
        "map",
        &["typeof(iszero)", "Vector{Int32}"],
        "typed map(iszero, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let isone_i32_func = get_function(&compiled, "unary_map_isone_i32_5094");
    assert_eq!(
        isone_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::Bool, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        isone_i32_func,
        "map",
        &["typeof(isone)", "Vector{Int32}"],
        "typed map(isone, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let signbit_i32_func = get_function(&compiled, "unary_map_signbit_i32_5094");
    assert_eq!(
        signbit_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::Bool, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        signbit_i32_func,
        "map",
        &["typeof(signbit)", "Vector{Int32}"],
        "typed map(signbit, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let iseven_i32_func = get_function(&compiled, "unary_map_iseven_i32_5094");
    assert_eq!(
        iseven_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::Bool, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        iseven_i32_func,
        "map",
        &["typeof(iseven)", "Vector{Int32}"],
        "typed map(iseven, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let isodd_i32_func = get_function(&compiled, "unary_map_isodd_i32_5094");
    assert_eq!(
        isodd_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::Bool, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        isodd_i32_func,
        "map",
        &["typeof(isodd)", "Vector{Int32}"],
        "typed map(isodd, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let neg_i32_func = get_function(&compiled, "unary_map_neg_i32_5094");
    assert_specialized_hof_candidate(
        &compiled,
        neg_i32_func,
        "map",
        &["typeof(-)", "Vector{Int32}"],
        "typed map(-, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );
}

#[test]
fn filter_parity_vector_uses_typed_specialized_candidate_issue_5094() {
    let compiled = compile_source_with_base(
        r#"
function filter_iseven_i32_5094(xs::Vector{Int32})
    filter(iseven, xs)
end

function filter_isodd_i32_5094(xs::Vector{Int32})
    filter(isodd, xs)
end

function filter_iszero_i32_5094(xs::Vector{Int32})
    filter(iszero, xs)
end

function filter_isone_i32_5094(xs::Vector{Int32})
    filter(isone, xs)
end

function filter_signbit_i32_5094(xs::Vector{Int32})
    filter(signbit, xs)
end

filter_iseven_i32_5094(Int32[-3, 0, 4, 5])
filter_isodd_i32_5094(Int32[-3, 0, 4, 5])
filter_iszero_i32_5094(Int32[-3, 0, 4, 5])
filter_isone_i32_5094(Int32[-3, 0, 1, 5])
filter_signbit_i32_5094(Int32[-3, 0, 4, 5])
"#,
    );

    let iseven_i32_func = get_function(&compiled, "filter_iseven_i32_5094");
    assert_eq!(
        iseven_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I32, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        iseven_i32_func,
        "filter",
        &["typeof(iseven)", "Vector{Int32}"],
        "typed filter(iseven, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let isodd_i32_func = get_function(&compiled, "filter_isodd_i32_5094");
    assert_eq!(
        isodd_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I32, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        isodd_i32_func,
        "filter",
        &["typeof(isodd)", "Vector{Int32}"],
        "typed filter(isodd, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let iszero_i32_func = get_function(&compiled, "filter_iszero_i32_5094");
    assert_eq!(
        iszero_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I32, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        iszero_i32_func,
        "filter",
        &["typeof(iszero)", "Vector{Int32}"],
        "typed filter(iszero, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let isone_i32_func = get_function(&compiled, "filter_isone_i32_5094");
    assert_eq!(
        isone_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I32, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        isone_i32_func,
        "filter",
        &["typeof(isone)", "Vector{Int32}"],
        "typed filter(isone, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );

    let signbit_i32_func = get_function(&compiled, "filter_signbit_i32_5094");
    assert_eq!(
        signbit_i32_func.return_type,
        ValueType::ArrayOf(ArrayElementType::I32, None)
    );
    assert_specialized_hof_candidate(
        &compiled,
        signbit_i32_func,
        "filter",
        &["typeof(signbit)", "Vector{Int32}"],
        "typed filter(signbit, Vector{Int32}) dispatch should resolve to the concrete specialization",
    );
}
