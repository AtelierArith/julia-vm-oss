//! Bytecode checks for local `@inbounds` indexing metadata (Issue #4286).

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};

fn compile_source_with_base(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let user_program = lowering.lower(parsed).expect("lower source");
    compile_core_program(&user_program).expect("compile failed")
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
fn explicit_inbounds_indexing_emits_inbounds_bytecode_issue_4286() {
    let compiled = compile_source_with_base(
        r#"
function checked_load_4286(xs::Vector{Int32}, i)
    xs[i]
end

function inbounds_load_4286(xs::Vector{Int32}, i)
    @inbounds xs[i]
end

function inbounds_getindex_4286(xs::Vector{Int32}, i)
    @inbounds getindex(xs, i)
end

function inbounds_base_getindex_4286(xs::Vector{Int32}, i)
    @inbounds Base.getindex(xs, i)
end

function checked_store_4286(xs::Vector{Float64}, i)
    xs[i] = 2.0
    xs
end

function inbounds_store_4286(xs::Vector{Float64}, i)
    @inbounds xs[i] = 2.0
    xs
end

function inbounds_setindex_call_4286(xs::Vector{Float64}, i)
    @inbounds setindex!(xs, 2.0, i)
    xs
end

function checked_foreach_body_load_4286(xs::Vector{Int32}, idxs::Vector{Int64})
    acc = Int32(0)
    for i in idxs
        acc += xs[i]
    end
    acc
end

function inbounds_foreach_body_load_4286(xs::Vector{Int32}, idxs::Vector{Int64})
    acc = Int32(0)
    @inbounds for i in idxs
        acc += xs[i]
    end
    acc
end

function checked_while_body_store_4286(xs::Vector{Float64}, idxs::Vector{Int64})
    j = 1
    while j <= length(idxs)
        xs[idxs[j]] = 4.0
        j += 1
    end
    xs
end

function inbounds_while_body_store_4286(xs::Vector{Float64}, idxs::Vector{Int64})
    j = 1
    @inbounds while j <= length(idxs)
        xs[idxs[j]] = 4.0
        j += 1
    end
    xs
end
"#,
    );

    let checked = get_function(&compiled, "checked_load_4286");
    assert!(
        function_body(&compiled, checked)
            .iter()
            .all(|instr| !matches!(
                instr,
                Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
            )),
        "plain load must stay checked: {:?}",
        function_body(&compiled, checked)
    );

    for function_name in [
        "inbounds_load_4286",
        "inbounds_getindex_4286",
        "inbounds_base_getindex_4286",
    ] {
        let func = get_function(&compiled, function_name);
        assert!(
            function_body(&compiled, func).iter().any(|instr| matches!(
                instr,
                Instr::IndexLoadInbounds(1) | Instr::IndexLoadTypedInbounds(1)
            )),
            "{function_name} should emit an in-bounds load: {:?}",
            function_body(&compiled, func)
        );
    }

    let checked = get_function(&compiled, "checked_store_4286");
    assert!(
        function_body(&compiled, checked)
            .iter()
            .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
        "plain store must stay checked: {:?}",
        function_body(&compiled, checked)
    );

    for function_name in ["inbounds_store_4286", "inbounds_setindex_call_4286"] {
        let func = get_function(&compiled, function_name);
        assert!(
            function_body(&compiled, func)
                .iter()
                .any(|instr| matches!(instr, Instr::IndexStoreInbounds(1))),
            "{function_name} should emit an in-bounds store: {:?}",
            function_body(&compiled, func)
        );
    }

    let checked = get_function(&compiled, "checked_foreach_body_load_4286");
    assert!(
        function_body(&compiled, checked)
            .iter()
            .all(|instr| !matches!(
                instr,
                Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
            )),
        "plain for-each body load must stay checked: {:?}",
        function_body(&compiled, checked)
    );

    let func = get_function(&compiled, "inbounds_foreach_body_load_4286");
    assert!(
        function_body(&compiled, func).iter().any(|instr| matches!(
            instr,
            Instr::IndexLoadInbounds(1) | Instr::IndexLoadTypedInbounds(1)
        )),
        "@inbounds for-each body should emit an in-bounds load: {:?}",
        function_body(&compiled, func)
    );

    let checked = get_function(&compiled, "checked_while_body_store_4286");
    assert!(
        function_body(&compiled, checked)
            .iter()
            .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
        "plain while body store must stay checked: {:?}",
        function_body(&compiled, checked)
    );

    let func = get_function(&compiled, "inbounds_while_body_store_4286");
    assert!(
        function_body(&compiled, func)
            .iter()
            .any(|instr| matches!(instr, Instr::IndexStoreInbounds(1))),
        "@inbounds while body should emit an in-bounds store: {:?}",
        function_body(&compiled, func)
    );
}
