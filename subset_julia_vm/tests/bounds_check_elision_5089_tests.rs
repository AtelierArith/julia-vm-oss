//! Bytecode checks for proven in-bounds index loads/stores (Issue #5089).

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
fn eachindex_and_length_loops_emit_inbounds_typed_load_issue_5089() {
    let compiled = compile_source_with_base(
        r#"
function eachindex_sum_5089(xs::Vector{Int32})
    total = 0
    for i in eachindex(xs)
        total = total + xs[i]
    end
    total
end

function length_sum_5089(xs::Vector{Int32})
    total = 0
    for i in 1:length(xs)
        total = total + xs[i]
    end
    total
end

function base_eachindex_sum_5089(xs::Vector{Int32})
    total = 0
    for i in Base.eachindex(xs)
        total = total + xs[i]
    end
    total
end

function base_length_sum_5089(xs::Vector{Int32})
    total = 0
    for i in 1:Base.length(xs)
        total = total + xs[i]
    end
    total
end

function lastindex_sum_5089(xs::Vector{Int32})
    total = 0
    for i in 1:lastindex(xs)
        total = total + xs[i]
    end
    total
end

function first_lastindex_sum_5089(xs::Vector{Int32})
    total = 0
    for i in firstindex(xs):lastindex(xs)
        total = total + xs[i]
    end
    total
end

function base_first_lastindex_sum_5089(xs::Vector{Int32})
    total = 0
    for i in Base.firstindex(xs):Base.lastindex(xs)
        total = total + xs[i]
    end
    total
end

function axes_sum_5089(xs::Vector{Int32})
    total = 0
    for i in axes(xs, 1)
        total = total + xs[i]
    end
    total
end

function base_axes_sum_5089(xs::Vector{Int32})
    total = 0
    for i in Base.axes(xs, 1)
        total = total + xs[i]
    end
    total
end

function base_oneto_length_sum_5089(xs::Vector{Int32})
    total = 0
    for i in Base.OneTo(length(xs))
        total = total + xs[i]
    end
    total
end

function base_oneto_function_length_sum_5089(xs::Vector{Int32})
    total = 0
    for i in Base.oneto(length(xs))
        total = total + xs[i]
    end
    total
end

function direct_getindex_sum_5089(xs::Vector{Int32})
    total = 0
    for i in eachindex(xs)
        total = total + getindex(xs, i)
    end
    total
end

function base_getindex_sum_5089(xs::Vector{Int32})
    total = 0
    for i in eachindex(xs)
        total = total + Base.getindex(xs, i)
    end
    total
end

function unchecked_not_proven_5089(xs::Vector{Int32}, i)
    xs[i]
end

function eachindex_store_5089(xs::Vector{Float64})
    for i in eachindex(xs)
        xs[i] = xs[i] + 1.0
    end
    xs
end

function length_store_5089(xs::Vector{Float64})
    for i in 1:length(xs)
        xs[i] = xs[i] + 1.0
    end
    xs
end

function axes_store_5089(xs::Vector{Float64})
    for i in axes(xs, 1)
        xs[i] = xs[i] + 1.0
    end
    xs
end

function base_axes_store_5089(xs::Vector{Float64})
    for i in Base.axes(xs, 1)
        xs[i] = xs[i] + 1.0
    end
    xs
end

function base_oneto_lastindex_store_5089(xs::Vector{Float64})
    for i in Base.OneTo(Base.lastindex(xs))
        xs[i] = xs[i] + 1.0
    end
    xs
end

function base_oneto_function_lastindex_store_5089(xs::Vector{Float64})
    for i in Base.oneto(Base.lastindex(xs))
        xs[i] = xs[i] + 1.0
    end
    xs
end

function unchecked_store_not_proven_5089(xs::Vector{Float64}, i)
    xs[i] = 2.0
    xs
end

function eachindex_setindex_call_5089(xs::Vector{Float64})
    for i in eachindex(xs)
        setindex!(xs, xs[i] + 1.0, i)
    end
    xs
end

function length_setindex_call_5089(xs::Vector{Float64})
    for i in 1:length(xs)
        setindex!(xs, xs[i] + 1.0, i)
    end
    xs
end

function base_lastindex_store_5089(xs::Vector{Float64})
    for i in 1:Base.lastindex(xs)
        setindex!(xs, xs[i] + 1.0, i)
    end
    xs
end

function first_lastindex_store_5089(xs::Vector{Float64})
    for i in firstindex(xs):lastindex(xs)
        setindex!(xs, xs[i] + 1.0, i)
    end
    xs
end

function mismatched_first_lastindex_not_proven_5089(xs::Vector{Float64}, ys::Vector{Float64})
    for i in firstindex(xs):lastindex(ys)
        xs[i] = xs[i] + 1.0
    end
    xs
end

function axes_dim2_not_proven_5089(xs::Vector{Float64})
    for i in axes(xs, 2)
        xs[i] = xs[i] + 1.0
    end
    xs
end

function mismatched_axes_not_proven_5089(xs::Vector{Float64}, ys::Vector{Float64})
    for i in axes(ys, 1)
        xs[i] = xs[i] + 1.0
    end
    xs
end

function direct_getindex_not_proven_5089(xs::Vector{Int32}, i)
    getindex(xs, i)
end
"#,
    );

    for function_name in [
        "eachindex_sum_5089",
        "length_sum_5089",
        "base_eachindex_sum_5089",
        "base_length_sum_5089",
        "lastindex_sum_5089",
        "first_lastindex_sum_5089",
        "base_first_lastindex_sum_5089",
        "axes_sum_5089",
        "base_axes_sum_5089",
        "base_oneto_length_sum_5089",
        "base_oneto_function_length_sum_5089",
        "direct_getindex_sum_5089",
        "base_getindex_sum_5089",
    ] {
        let func = get_function(&compiled, function_name);
        assert!(
            function_body(&compiled, func).iter().any(|instr| matches!(
                instr,
                Instr::IndexLoadInbounds(1) | Instr::IndexLoadTypedInbounds(1)
            )),
            "{function_name} should emit an in-bounds index load: {:?}",
            function_body(&compiled, func)
        );
    }

    let fallback = get_function(&compiled, "unchecked_not_proven_5089");
    assert!(
        function_body(&compiled, fallback)
            .iter()
            .all(|instr| !matches!(
                instr,
                Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
            )),
        "unproven index loads must keep the checked typed load: {:?}",
        function_body(&compiled, fallback)
    );

    let fallback = get_function(&compiled, "direct_getindex_not_proven_5089");
    assert!(
        function_body(&compiled, fallback)
            .iter()
            .all(|instr| !matches!(
                instr,
                Instr::IndexLoadInbounds(_) | Instr::IndexLoadTypedInbounds(_)
            )),
        "unproven direct getindex calls must keep checked loads: {:?}",
        function_body(&compiled, fallback)
    );
    assert!(
        function_body(&compiled, fallback)
            .iter()
            .any(|instr| matches!(instr, Instr::IndexLoadTyped(1))),
        "direct getindex on typed arrays should use the typed checked load: {:?}",
        function_body(&compiled, fallback)
    );

    for function_name in [
        "eachindex_store_5089",
        "length_store_5089",
        "axes_store_5089",
        "base_axes_store_5089",
        "base_oneto_lastindex_store_5089",
        "base_oneto_function_lastindex_store_5089",
        "eachindex_setindex_call_5089",
        "length_setindex_call_5089",
        "base_lastindex_store_5089",
        "first_lastindex_store_5089",
    ] {
        let func = get_function(&compiled, function_name);
        assert!(
            function_body(&compiled, func)
                .iter()
                .any(|instr| matches!(instr, Instr::IndexStoreInbounds(1))),
            "{function_name} should emit an in-bounds index store: {:?}",
            function_body(&compiled, func)
        );
    }

    let fallback = get_function(&compiled, "unchecked_store_not_proven_5089");
    assert!(
        function_body(&compiled, fallback)
            .iter()
            .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
        "unproven index stores must keep the checked store: {:?}",
        function_body(&compiled, fallback)
    );

    let fallback = get_function(&compiled, "mismatched_first_lastindex_not_proven_5089");
    assert!(
        function_body(&compiled, fallback)
            .iter()
            .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
        "mismatched firstindex/lastindex arrays must keep checked stores: {:?}",
        function_body(&compiled, fallback)
    );

    for function_name in [
        "axes_dim2_not_proven_5089",
        "mismatched_axes_not_proven_5089",
    ] {
        let fallback = get_function(&compiled, function_name);
        assert!(
            function_body(&compiled, fallback)
                .iter()
                .all(|instr| !matches!(instr, Instr::IndexStoreInbounds(_))),
            "{function_name} must keep checked stores: {:?}",
            function_body(&compiled, fallback)
        );
    }
}
