use std::collections::{HashMap, HashSet};

use crate::bytecode::{AbstractTypeDefInfo, CompiledProgram, Instr, StructDefInfo, ValueType};
use crate::compile::context::StructRegistry;
use crate::compile::context::{SharedCompileContext, StructInfo};
use crate::compile::method_table::ConstructorSelfFamily;
use crate::compile::{CResult, CoreCompiler, MethodSig, MethodTable};
use crate::ir::core::{Expr, Literal};
use crate::span::Span;
use crate::types::{JuliaType, TypeExpr, TypeParam};

fn compile_constructor_source(source: &str) -> Result<CompiledProgram, String> {
    let program = crate::pipeline::parse_and_lower(source)?;
    crate::compile::compile_with_cache(&program).map_err(|error| format!("compile error: {error}"))
}

fn run_constructor_source(source: &str) -> Result<String, String> {
    let compiled = compile_constructor_source(source)?;
    crate::test_runtime::run_compiled_program(compiled, 1)
}

fn assert_wrapper_resolves_only_owner(
    compiled: &CompiledProgram,
    wrapper_name: &str,
    expected_owner: &str,
) -> Result<(), String> {
    let wrapper = compiled
        .functions
        .iter()
        .find(|function| function.name == wrapper_name)
        .ok_or_else(|| format!("missing wrapper function {wrapper_name}"))?;
    let wrapper_code = &compiled.code[wrapper.code_start..wrapper.code_end];
    let candidates = wrapper_code
        .iter()
        .find_map(|instr| match instr {
            Instr::PushResolvedFunction(operands) => Some(&operands.candidate_indices),
            _ => None,
        })
        .ok_or_else(|| {
            format!("{wrapper_name} did not freeze owner candidates: {wrapper_code:?}")
        })?;
    if candidates.is_empty() {
        return Err(format!("{wrapper_name} unexpectedly has no candidates"));
    }
    for &candidate in candidates {
        let name = compiled
            .functions
            .get(candidate)
            .ok_or_else(|| format!("invalid candidate {candidate} in {wrapper_name}"))?
            .name
            .as_str();
        if name != expected_owner {
            return Err(format!(
                "{wrapper_name} leaked candidate {candidate} from {name}; expected only {expected_owner}"
            ));
        }
    }
    Ok(())
}

#[test]
fn nested_parametric_default_keeps_invariant_dispatch_11434() -> Result<(), String> {
    let output = run_constructor_source(
        r#"
struct Inner11434{T}
    value::T
end

struct Outer11434{T}
    value::T
end

classify11434(x::Outer11434{Inner11434{Number}}) = "bad"
classify11434(x) = "ok"
classify_any11434(x::Any) = classify11434(x)

value = Outer11434(Inner11434(1))
println(typeof(value))
println(classify11434(value))
println(classify_any11434(value))
"#,
    )?;

    assert_eq!(output, "Outer11434{Inner11434{Int64}}\nok\nok\n");
    Ok(())
}

#[test]
fn generic_dispatch_keeps_unknown_constructor_return_dynamic_11434() -> CResult<()> {
    let function = "UnknownOuter11434";
    let mut table = MethodTable::new(function.to_string());
    table.add_method(MethodSig::from_julia_projections(
        0,
        0,
        vec![("x".to_string(), JuliaType::Int64)],
        ValueType::Any,
        None,
        false,
        vec![],
        None,
        None,
    ));
    let method_tables = HashMap::from([(function.to_string(), table)]);
    let module_functions = HashMap::new();
    let module_exports = HashMap::new();
    let imported_functions = HashSet::from([function.to_string()]);
    let usings = HashSet::new();
    let abstract_type_names = HashSet::new();
    let module_constants = HashMap::new();
    let mut shared_ctx = constructor_identity_ctx(
        vec![("UnknownOuter11434{Inner11434{Number}}", 0, None)],
        vec![],
    );
    let mut compiler = CoreCompiler::new_for_function(
        &method_tables,
        &module_functions,
        &module_exports,
        &imported_functions,
        &usings,
        Vec::new(),
        &mut shared_ctx,
        &abstract_type_names,
        &module_constants,
    );
    let span = Span::new(0, 0, 0, 0, 0, 0);

    let return_type = compiler.compile_generic_dispatch_call(
        function,
        &[Expr::Literal(Literal::Int(1), span)],
        &[],
        &[],
        false,
    )?;

    assert_eq!(return_type, ValueType::Any);
    Ok(())
}

#[test]
fn collection_constructor_transfer_respects_user_method_return_11434() -> Result<(), String> {
    let output = run_constructor_source(
        r#"
module ConstructorOverrides11434
import Base: Set, Dict
Set(xs::Vector{Int64})::Any = 42
Dict(p::Pair{String,Int64})::Any = 43
end

classify_set11434(x::Int64) = x + 1
classify_set11434(x::Set{Int64}) = 0
classify_dict11434(x::Int64) = x + 1
classify_dict11434(x::Dict{String,Int64}) = 0
make_set11434() = Set([1])
make_dict11434() = Dict("a" => 1)
make_dynamic_set11434(xs) = Set(xs)

println(classify_set11434(Set([1])))
println(classify_dict11434(Dict("a" => 1)))
println(classify_set11434(make_dynamic_set11434([1])))
set_value = make_set11434()
dict_value = make_dict11434()
println(set_value + 1)
println(dict_value + 1)
println(typeof(set_value))
println(typeof(dict_value))
"#,
    )?;

    assert_eq!(output, "43\n44\n43\n43\n44\nInt64\nInt64\n");
    Ok(())
}

#[test]
fn range_type_recovery_respects_user_constructor_return_11434() -> Result<(), String> {
    let output = run_constructor_source(
        r#"
import Base: StepRange
StepRange(a::Int64, s::Int64, b::Int64) = 42

classify_range11434(x::Int64) = "int"
classify_range11434(x::StepRange) = "range"
classify_range11434(x) = "other"

println(typeof(1:1:2))
println(classify_range11434(1:1:2))
"#,
    )?;

    assert_eq!(output, "Int64\nint\n");
    Ok(())
}

#[test]
fn explicit_collection_constructors_keep_static_dispatch_11434() -> Result<(), String> {
    let compiled = compile_constructor_source(
        r#"
classify_explicit_set11434(x::Set{Int64}) = 1
classify_explicit_set11434(x) = 0
classify_explicit_dict11434(x::Dict{String,Int64}) = 1
classify_explicit_dict11434(x) = 0

explicit_set11434() = classify_explicit_set11434(Set{Int64}([1]))
explicit_dict11434() = classify_explicit_dict11434(Dict{String,Int64}("a" => 1))

println(explicit_set11434())
println(explicit_dict11434())
"#,
    )?;

    for function_name in ["explicit_set11434", "explicit_dict11434"] {
        let function = compiled
            .functions
            .iter()
            .find(|function| function.name == function_name)
            .ok_or_else(|| format!("missing function {function_name}"))?;
        let body = &compiled.code[function.code_start..function.code_end];
        if body
            .iter()
            .any(|instr| matches!(instr, Instr::CallDynamic(_)))
        {
            return Err(format!(
                "{function_name} lost explicit collection dispatch precision: {body:?}"
            ));
        }
    }
    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "1\n1\n");
    Ok(())
}

#[test]
fn qualified_bare_parametric_default_calls_outer_constructor_11076_7302_10342_11147(
) -> Result<(), String> {
    let output = run_constructor_source(
        r#"
module QualifiedDefaultOwner11076
export Box

struct Box{T}
    value::T
end
end

value = QualifiedDefaultOwner11076.Box(1)
println(typeof(value))
println(value.value)
"#,
    )?;

    assert_eq!(output, "QualifiedDefaultOwner11076.Box{Int64}\n1\n");
    Ok(())
}

#[test]
fn qualified_bare_parametric_defaults_keep_sibling_owners_and_explicit_surface_11076(
) -> Result<(), String> {
    let output = run_constructor_source(
        r#"
module QualifiedSiblingA11076
struct Box{T}
    value::T
end
end

module QualifiedSiblingB11076
struct Box{T}
    value::T
end
end

sibling_owner(::QualifiedSiblingA11076.Box) = "a"
sibling_owner(::QualifiedSiblingB11076.Box) = "b"

a = QualifiedSiblingA11076.Box(1)
b = QualifiedSiblingB11076.Box(2.5)
explicit = QualifiedSiblingA11076.Box{Float64}(3.5)
make_a_box(value) = QualifiedSiblingA11076.Box(value)
dynamic = make_a_box(7)
println(a isa QualifiedSiblingA11076.Box{Int64})
println(sibling_owner(a))
println(a.value)
println(b isa QualifiedSiblingB11076.Box{Float64})
println(sibling_owner(b))
println(b.value)
println(explicit isa QualifiedSiblingA11076.Box{Float64})
println(explicit.value)
println(dynamic isa QualifiedSiblingA11076.Box{Int64})
println(dynamic.value)
"#,
    )?;

    assert_eq!(
        output,
        concat!(
            "true\n", "a\n", "1\n", "true\n", "b\n", "2.5\n", "true\n", "3.5\n", "true\n", "7\n",
        )
    );
    Ok(())
}

#[test]
fn imported_bare_parametric_default_keeps_qualified_owner_11076() -> Result<(), String> {
    let output = run_constructor_source(
        r#"
module QualifiedImportedOwner11076
export ImportedBox

struct ImportedBox{T}
    value::T
end
end

using .QualifiedImportedOwner11076: ImportedBox

value = ImportedBox(4)
println(value isa QualifiedImportedOwner11076.ImportedBox{Int64})
println(value.value)
"#,
    )?;

    assert_eq!(output, "true\n4\n");
    Ok(())
}

#[test]
fn qualified_bare_parametric_call_keeps_source_written_bare_inner_11076() -> Result<(), String> {
    let output = run_constructor_source(
        r#"
module QualifiedBareInnerOwner11076
struct Wrapped{T}
    value::T
    Wrapped(value::T) where {T} = new{T}(value + 1)
end
end

value = QualifiedBareInnerOwner11076.Wrapped(41)
println(value isa QualifiedBareInnerOwner11076.Wrapped{Int64})
println(value.value)
"#,
    )?;

    assert_eq!(output, "true\n42\n");
    Ok(())
}

#[test]
fn qualified_splatted_parametric_bare_inners_keep_sibling_owners_11147() -> Result<(), String> {
    let compiled = compile_constructor_source(
        r#"
module ParamBareInnerA11147
export Box
struct Box{T}
    value::T
    Box(value::T) where {T} = new{T}(value + 10)
end
end

module ParamBareInnerB11147
struct Box{T}
    value::T
    Box(value::T) where {T} = new{T}(value + 20)
end
end

using .ParamBareInnerA11147: Box

make_param_a_11147(values) = ParamBareInnerA11147.Box(values...)
make_param_b_11147(values) = ParamBareInnerB11147.Box(values...)
make_imported_param_a_11147(values) = Box(values...)

a = make_param_a_11147((1,))
b = make_param_b_11147((1,))
imported = make_imported_param_a_11147((2,))
println(a isa ParamBareInnerA11147.Box{Int64})
println(a.value)
println(b isa ParamBareInnerB11147.Box{Int64})
println(b.value)
println(imported isa ParamBareInnerA11147.Box{Int64})
println(imported.value)
"#,
    )?;

    assert_wrapper_resolves_only_owner(
        &compiled,
        "make_param_a_11147",
        "ParamBareInnerA11147.Box",
    )?;
    assert_wrapper_resolves_only_owner(
        &compiled,
        "make_param_b_11147",
        "ParamBareInnerB11147.Box",
    )?;

    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "true\n11\ntrue\n21\ntrue\n12\n");
    Ok(())
}

#[test]
fn qualified_splatted_nonparametric_bare_inners_keep_sibling_owners_11147() -> Result<(), String> {
    let compiled = compile_constructor_source(
        r#"
module ConcreteBareInnerA11147
struct Box
    value::Int64
    Box(value::Int64) = new(value + 10)
end
end

module ConcreteBareInnerB11147
struct Box
    value::Int64
    Box(value::Int64) = new(value + 20)
end
end

make_concrete_a_11147(values) = ConcreteBareInnerA11147.Box(values...)
make_concrete_b_11147(values) = ConcreteBareInnerB11147.Box(values...)

a = make_concrete_a_11147((1,))
b = make_concrete_b_11147((1,))
println(a isa ConcreteBareInnerA11147.Box)
println(a.value)
println(b isa ConcreteBareInnerB11147.Box)
println(b.value)
"#,
    )?;

    assert_wrapper_resolves_only_owner(
        &compiled,
        "make_concrete_a_11147",
        "ConcreteBareInnerA11147.Box",
    )?;
    assert_wrapper_resolves_only_owner(
        &compiled,
        "make_concrete_b_11147",
        "ConcreteBareInnerB11147.Box",
    )?;

    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "true\n11\ntrue\n21\n");
    Ok(())
}

#[test]
fn qualified_bare_call_does_not_use_explicit_self_only_inner_11076_7302_10342_11147(
) -> Result<(), String> {
    let compiled = compile_constructor_source(
        r#"
module QualifiedExplicitOnlyOwner11076
struct Only{T}
    value::T
    Only{T}(value::T) where {T} = new{T}(value)
end
end

try
    QualifiedExplicitOnlyOwner11076.Only(1)
    println(false)
catch err
    println(err isa MethodError)
end

function bare_only_errors(value)
    try
        QualifiedExplicitOnlyOwner11076.Only(value)
        return false
    catch err
        return err isa MethodError
    end
end

println(bare_only_errors(2))

function bare_only_splat_errors(values)
    try
        QualifiedExplicitOnlyOwner11076.Only(values...)
        return false
    catch err
        return err isa MethodError
    end
end

function bare_only_kw_splat_errors(value, options)
    try
        QualifiedExplicitOnlyOwner11076.Only(value; options...)
        return false
    catch err
        return err isa MethodError
    end
end

println(bare_only_splat_errors((3,)))
println(bare_only_kw_splat_errors(4, (unused=1,)))
value = QualifiedExplicitOnlyOwner11076.Only{Int64}(5)
println(value isa QualifiedExplicitOnlyOwner11076.Only{Int64})
println(value.value)
"#,
    )?;

    for wrapper_name in [
        "bare_only_errors",
        "bare_only_splat_errors",
        "bare_only_kw_splat_errors",
    ] {
        let wrapper = compiled
            .functions
            .iter()
            .find(|function| function.name == wrapper_name)
            .ok_or_else(|| format!("missing wrapper function {wrapper_name}"))?;
        let wrapper_code = &compiled.code[wrapper.code_start..wrapper.code_end];
        assert!(
            wrapper_code.iter().any(|instr| matches!(
                instr,
                Instr::PushResolvedFunction(operands) if operands.candidate_indices.is_empty()
            )),
            "{wrapper_name} must preserve the authoritative empty candidate view: {wrapper_code:?}"
        );
        let has_runtime_call = wrapper_code.iter().any(|instr| match wrapper_name {
            "bare_only_errors" => matches!(instr, Instr::CallFunctionVariable(1)),
            "bare_only_splat_errors" => {
                matches!(instr, Instr::CallFunctionVariableWithSplat(1, mask) if mask == &[true])
            }
            "bare_only_kw_splat_errors" => {
                matches!(instr, Instr::CallFunctionVariableWithKwargsSplat(_))
            }
            _ => false,
        });
        assert!(
            has_runtime_call,
            "{wrapper_name} must reach the normal runtime expansion/dispatch path: {wrapper_code:?}"
        );
        assert!(
            !wrapper_code
                .iter()
                .any(|instr| matches!(instr, Instr::ThrowMethodError(_))),
            "{wrapper_name} must not throw before runtime argument preparation: {wrapper_code:?}"
        );
    }

    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "true\ntrue\ntrue\ntrue\ntrue\n5\n");
    Ok(())
}

#[test]
fn qualified_bare_parametric_runtime_kwargs_and_splat_keep_outer_view_11147() -> Result<(), String>
{
    let compiled = compile_constructor_source(
        r#"
module QualifiedRuntimeSurface11147
struct Box{T}
    value::T
end

Box(value::Int64; bump=0) = Box{Int64}(value + bump)
end

module QualifiedDefaultSplat11147
struct Box{T}
    value::T
end
end

make_kwarg_box(value) = QualifiedRuntimeSurface11147.Box(value; bump=1)
make_splat_box(values) = QualifiedRuntimeSurface11147.Box(values...)
make_default_splat_box(values) = QualifiedDefaultSplat11147.Box(values...)

kwarg_value = make_kwarg_box(41)
splat_value = make_splat_box((8,))
default_splat_value = make_default_splat_box((9,))
println(kwarg_value isa QualifiedRuntimeSurface11147.Box{Int64})
println(kwarg_value.value)
println(splat_value isa QualifiedRuntimeSurface11147.Box{Int64})
println(splat_value.value)
println(default_splat_value isa QualifiedDefaultSplat11147.Box{Int64})
println(default_splat_value.value)
"#,
    )?;

    for (wrapper_name, constructor_name) in [
        ("make_kwarg_box", "QualifiedRuntimeSurface11147.Box"),
        ("make_splat_box", "QualifiedRuntimeSurface11147.Box"),
        ("make_default_splat_box", "QualifiedDefaultSplat11147.Box"),
    ] {
        let wrapper = compiled
            .functions
            .iter()
            .find(|function| function.name == wrapper_name)
            .ok_or_else(|| format!("missing wrapper function {wrapper_name}"))?;
        let wrapper_code = &compiled.code[wrapper.code_start..wrapper.code_end];
        let resolved = wrapper_code
            .iter()
            .find_map(|instr| match instr {
                Instr::PushResolvedFunction(operands) => Some(operands.as_ref()),
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "{wrapper_name} must carry filtered constructor candidates: {wrapper_code:?}"
                )
            })?;
        let candidate_names: Vec<&str> = resolved
            .candidate_indices
            .iter()
            .map(|&index| compiled.functions[index].name.as_str())
            .collect();
        assert!(
            !candidate_names.is_empty()
                && candidate_names.iter().all(|name| *name == constructor_name),
            "{wrapper_name} reintroduced an explicit-self candidate: {candidate_names:?}"
        );
    }

    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "true\n42\ntrue\n8\ntrue\n9\n");
    Ok(())
}

#[test]
fn partial_explicit_self_reaches_runtime_outer_dispatch_11147_7734() -> Result<(), String> {
    let compiled = compile_constructor_source(
        r#"
struct PartialSelfGate11147{A,B}
    value::B
end

function PartialSelfGate11147{A}(value::Int64) where {A}
    return PartialSelfGate11147{A,Int64}(value)
end

function PartialSelfGate11147{A}(value::String) where {A}
    return PartialSelfGate11147{A,String}(value)
end

make_partial_self_11147(value) = PartialSelfGate11147{7}(value)

value = make_partial_self_11147(41)
println(value isa PartialSelfGate11147{7,Int64})
println(value.value)
"#,
    )?;

    let wrapper = compiled
        .functions
        .iter()
        .find(|function| function.name == "make_partial_self_11147")
        .ok_or_else(|| "missing make_partial_self_11147 wrapper".to_string())?;
    let wrapper_code = &compiled.code[wrapper.code_start..wrapper.code_end];
    assert!(
        wrapper_code
            .iter()
            .any(|instr| matches!(instr, Instr::ApplyTypeDynamic(1))),
        "partial explicit self must be formed before runtime outer dispatch: {wrapper_code:?}"
    );
    assert!(
        wrapper_code
            .iter()
            .any(|instr| matches!(instr, Instr::CallFunctionVariable(1))),
        "partial explicit self must remain callable at runtime: {wrapper_code:?}"
    );
    assert!(
        !wrapper_code
            .iter()
            .any(|instr| matches!(instr, Instr::ThrowMethodError(_))),
        "a full synthetic inner must not turn a partial self into a premature MethodError: {wrapper_code:?}"
    );

    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "true\n41\n");
    Ok(())
}

#[test]
fn runtime_type_arg_prefers_source_vararg_over_inapplicable_synthetic_default_11147(
) -> Result<(), String> {
    let compiled = compile_constructor_source(
        r#"
struct RuntimeVarargCtor11147{N,T}
    data::Tuple
end

function RuntimeVarargCtor11147{N,T}(xs...) where {N,T}
    return RuntimeVarargCtor11147{N,T}(xs)
end

function make_runtime_vararg_ctor_11147(a::T) where {T<:Real}
    F = float(T)
    return RuntimeVarargCtor11147{1,F}(a)
end

value = make_runtime_vararg_ctor_11147(1.25)
println(value isa RuntimeVarargCtor11147{1,Float64})
println(value.data == (1.25,))
"#,
    )?;

    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "true\ntrue\n");
    Ok(())
}

#[test]
fn invalid_synthetic_constructor_self_bound_preserves_bound_error_11147() {
    let error = match compile_constructor_source(
        r#"
abstract type ConstructorBoundGate11147 end

struct WrongConstructorBoundGate11147
    value::Int64
end

struct BoundedConstructorGate11147{T<:ConstructorBoundGate11147}
    value::T
end

BoundedConstructorGate11147{WrongConstructorBoundGate11147}(
    WrongConstructorBoundGate11147(41),
)
"#,
    ) {
        Ok(_) => panic!("invalid explicit self bound must reject compilation"),
        Err(error) => error,
    };

    assert!(
        error.contains("does not satisfy bound") || error.contains("not satisfy"),
        "expected the type-application bound error, got: {error}"
    );
    assert!(
        !error.contains("no method matching"),
        "synthetic inner arity must not replace a bound error with MethodError: {error}"
    );
}

#[test]
fn splat_validation_precedes_target_dispatch_11372() -> Result<(), String> {
    let compiled = compile_constructor_source(
        r#"
positional_accepts_all(values...) = true
keyword_accepts_all(; options...) = true
combined_accepts_all(values...; options...) = true

function static_positional_validation()
    try
        positional_accepts_all(nothing...)
        return false
    catch err
        return err isa MethodError
    end
end

function static_keyword_validation()
    try
        keyword_accepts_all(; 2...)
        return false
    catch err
        return err isa BoundsError
    end
end

function variable_positional_validation(func)
    try
        func(nothing...)
        return false
    catch err
        return err isa MethodError
    end
end

function variable_keyword_validation(func)
    try
        func(; 2...)
        return false
    catch err
        return err isa BoundsError
    end
end

function combined_invalid_both_validation(func)
    try
        func(nothing...; 2...)
        return false
    catch err
        return err isa BoundsError
    end
end

function combined_valid_keyword_invalid_positional_validation(func)
    try
        func(nothing...; (a = 1,)...)
        return false
    catch err
        return err isa MethodError
    end
end

function combined_empty_keyword_validation(func)
    return func(1; ()...)
end

println(static_positional_validation())
println(static_keyword_validation())
println(variable_positional_validation(positional_accepts_all))
println(variable_keyword_validation(keyword_accepts_all))
println(combined_invalid_both_validation(combined_accepts_all))
println(combined_valid_keyword_invalid_positional_validation(combined_accepts_all))
println(combined_empty_keyword_validation(combined_accepts_all))
"#,
    )?;

    for (wrapper_name, expected_call) in [
        (
            "static_positional_validation",
            "CallFunctionVariableWithSplat",
        ),
        ("static_keyword_validation", "CallWithKwargsSplat"),
        (
            "variable_positional_validation",
            "CallFunctionVariableWithKwargsSplat",
        ),
        (
            "variable_keyword_validation",
            "CallFunctionVariableWithKwargsSplat",
        ),
        (
            "combined_invalid_both_validation",
            "CallFunctionVariableWithKwargsSplat",
        ),
        (
            "combined_valid_keyword_invalid_positional_validation",
            "CallFunctionVariableWithKwargsSplat",
        ),
        (
            "combined_empty_keyword_validation",
            "CallFunctionVariableWithKwargsSplat",
        ),
    ] {
        let wrapper = compiled
            .functions
            .iter()
            .find(|function| function.name == wrapper_name)
            .ok_or_else(|| format!("missing wrapper function {wrapper_name}"))?;
        let wrapper_code = &compiled.code[wrapper.code_start..wrapper.code_end];
        let has_expected_call = wrapper_code.iter().any(|instr| match expected_call {
            "CallFunctionVariableWithSplat" => {
                matches!(instr, Instr::CallFunctionVariableWithSplat(_, _))
            }
            "CallFunctionVariableWithKwargsSplat" => {
                matches!(instr, Instr::CallFunctionVariableWithKwargsSplat(_))
            }
            "CallWithKwargsSplat" => matches!(instr, Instr::CallWithKwargsSplat(_, _, _, _)),
            _ => false,
        });
        assert!(
            has_expected_call,
            "{wrapper_name} must exercise {expected_call}: {wrapper_code:?}"
        );
    }

    let output = crate::test_runtime::run_compiled_program(compiled, 1)?;
    assert_eq!(output, "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");
    Ok(())
}

fn explicit_outer_method(global_index: usize, arg_type: JuliaType) -> MethodSig {
    MethodSig::from_julia_projections(
        0,
        global_index,
        vec![("x".to_string(), arg_type)],
        ValueType::Any,
        None,
        false,
        vec![TypeParam::with_upper_bound(
            "T".to_string(),
            "Integer".to_string(),
        )],
        None,
        None,
    )
}

fn explicit_inner_method(global_index: usize, arg_type: JuliaType) -> MethodSig {
    MethodSig::from_julia_projections(
        0,
        global_index,
        vec![("x".to_string(), arg_type)],
        ValueType::Any,
        None,
        false,
        vec![TypeParam::new("T".to_string())],
        None,
        None,
    )
}

fn compile_dynamic_constructor(
    method_tables: HashMap<String, MethodTable>,
    function: &str,
    arg: Expr,
) -> CResult<(Option<ValueType>, Vec<Instr>)> {
    let module_functions = HashMap::new();
    let module_exports = HashMap::new();
    let imported_functions = HashSet::new();
    let usings = HashSet::new();
    let abstract_type_names = HashSet::new();
    let module_constants = HashMap::new();
    let mut shared_ctx = SharedCompileContext::new(
        StructRegistry::new(),
        Vec::new(),
        HashMap::new(),
        HashMap::new(),
        Vec::new(),
        0,
    );
    let mut compiler = CoreCompiler::new_for_function(
        &method_tables,
        &module_functions,
        &module_exports,
        &imported_functions,
        &usings,
        Vec::new(),
        &mut shared_ctx,
        &abstract_type_names,
        &module_constants,
    );
    compiler.current_type_param_index.insert("T".to_string(), 0);
    compiler.locals.insert("x".to_string(), ValueType::Any);
    let result = compiler.try_compile_parametric_constructor_call(function, &[arg])?;
    Ok((result, compiler.code))
}

pub(super) fn imprecise_inner_call_carries_unique_outer_validation_fallback_10969() -> CResult<()> {
    let mut inner_table = MethodTable::new("Probe10969".to_string());
    inner_table.add_inner_constructor_method(
        explicit_inner_method(10, JuliaType::TypeVar("T".to_string(), None)),
        ConstructorSelfFamily::ExplicitParametricInner,
    );
    let mut outer_table = MethodTable::new("Probe10969{T}".to_string());
    outer_table.add_method(explicit_outer_method(20, JuliaType::Any));
    let method_tables = HashMap::from([
        ("Probe10969".to_string(), inner_table),
        ("Probe10969{T}".to_string(), outer_table),
    ]);
    let span = Span::new(0, 0, 0, 0, 0, 0);

    let (_, code) =
        compile_dynamic_constructor(method_tables, "Probe10969{T}", Expr::var("x", span))?;

    assert!(code.iter().any(|instr| matches!(
        instr,
        Instr::CallStaticParametric(call)
            if call.func_index == 10
                && call.validate_argument_types
                && call
                    .validation_fallback
                    .as_ref()
                    .is_some_and(|fallback| fallback.func_index == 20)
    )));
    Ok(())
}

pub(super) fn ambiguous_outer_keeps_sole_inner_runtime_validation_10969() -> CResult<()> {
    let mut inner_table = MethodTable::new("Probe10969".to_string());
    inner_table.add_inner_constructor_method(
        explicit_inner_method(10, JuliaType::TypeVar("T".to_string(), None)),
        ConstructorSelfFamily::ExplicitParametricInner,
    );
    let mut outer_table = MethodTable::new("Probe10969{T}".to_string());
    outer_table.add_method(explicit_outer_method(20, JuliaType::Integer));
    outer_table.add_method(explicit_outer_method(21, JuliaType::Real));
    let method_tables = HashMap::from([
        ("Probe10969".to_string(), inner_table),
        ("Probe10969{T}".to_string(), outer_table),
    ]);
    let span = Span::new(0, 0, 0, 0, 0, 0);

    let (result, code) = compile_dynamic_constructor(
        method_tables,
        "Probe10969{T}",
        Expr::Literal(Literal::Str("bad".into()), span),
    )?;

    assert_eq!(result, Some(ValueType::Any));
    assert!(code.iter().any(|instr| matches!(
        instr,
        Instr::CallStaticParametric(call)
            if call.func_index == 10
                && call.validate_argument_types
                && call.validation_fallback.is_none()
    )));
    assert!(!code
        .iter()
        .any(|instr| matches!(instr, Instr::ThrowMethodError(_))));
    Ok(())
}

pub(super) fn multiple_imprecise_inners_do_not_fall_through_to_unique_outer_10971() -> CResult<()> {
    let mut inner_table = MethodTable::new("Probe10971".to_string());
    for (global_index, param_type) in [(10, JuliaType::Int64), (11, JuliaType::String)] {
        inner_table.add_inner_constructor_method(
            explicit_inner_method(global_index, param_type),
            ConstructorSelfFamily::ExplicitParametricInner,
        );
    }
    let mut outer_table = MethodTable::new("Probe10971{T}".to_string());
    outer_table.add_method(explicit_outer_method(20, JuliaType::Any));
    let method_tables = HashMap::from([
        ("Probe10971".to_string(), inner_table),
        ("Probe10971{T}".to_string(), outer_table),
    ]);
    let span = Span::new(0, 0, 0, 0, 0, 0);

    let (_, code) =
        compile_dynamic_constructor(method_tables, "Probe10971{T}", Expr::var("x", span))?;

    assert!(code
        .iter()
        .any(|instr| matches!(instr, Instr::ThrowMethodError(_))));
    assert!(!code
        .iter()
        .any(|instr| matches!(instr, Instr::CallStaticParametric(_))));
    Ok(())
}

fn constructor_identity_ctx(
    structs: Vec<(&str, usize, Option<&str>)>,
    abstract_types: Vec<(&str, Option<&str>)>,
) -> SharedCompileContext {
    let mut struct_table = StructRegistry::new();
    let mut struct_defs = Vec::new();
    for (name, type_id, parent) in &structs {
        struct_table.insert(
            name.to_string(),
            StructInfo {
                type_id: *type_id,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        struct_defs.push(StructDefInfo {
            name: name.to_string(),
            is_mutable: false,
            fields: vec![("x".to_string(), ValueType::F64)],
            field_julia_types: vec![JuliaType::Float64],
            parent_type: parent.map(str::to_string),
        });
    }
    let abstract_types = abstract_types
        .iter()
        .map(|(name, parent)| AbstractTypeDefInfo {
            name: name.to_string(),
            parent: parent.map(str::to_string),
            type_params: vec![],
        })
        .collect();
    SharedCompileContext::new(
        struct_table,
        struct_defs,
        HashMap::new(),
        HashMap::new(),
        abstract_types,
        structs.len(),
    )
}

pub(super) fn constructor_self_arguments_expand_aliases_without_capturing_binders_11019() {
    let mut ctx = constructor_identity_ctx(vec![], vec![]);
    ctx.type_aliases
        .insert("MyVector".to_string(), "Vector".to_string());
    ctx.type_aliases
        .insert("S".to_string(), "String".to_string());
    let arguments = vec![TypeExpr::Parameterized {
        base: "MyVector".to_string(),
        params: vec![TypeExpr::TypeVar("S".to_string())],
    }];

    let expanded = ctx.expand_constructor_self_type_arguments(
        &arguments,
        &[TypeParam::new("S".to_string())],
        None,
    );

    assert_eq!(
        expanded,
        vec![TypeExpr::Parameterized {
            base: "Vector".to_string(),
            params: vec![TypeExpr::TypeVar("S".to_string())],
        }]
    );
}

pub(super) fn constructor_self_arguments_qualify_module_local_types_11019() {
    let ctx = constructor_identity_ctx(vec![("Owner.Tag", 0, None)], vec![]);
    let arguments = vec![TypeExpr::Concrete(JuliaType::Struct("Tag".to_string()))];

    assert_eq!(
        ctx.expand_constructor_self_type_arguments(&arguments, &[], Some("Owner")),
        vec![TypeExpr::Concrete(JuliaType::Struct(
            "Owner.Tag".to_string()
        ))]
    );
}

pub(super) fn constructor_self_alias_prefers_lexical_module_over_ambiguous_suffix_11019() {
    let mut ctx = constructor_identity_ctx(vec![], vec![]);
    ctx.type_aliases
        .insert("Owner.Alias".to_string(), "Vector".to_string());
    ctx.type_aliases
        .insert("Other.Alias".to_string(), "Matrix".to_string());
    let arguments = vec![TypeExpr::Parameterized {
        base: "Alias".to_string(),
        params: vec![TypeExpr::TypeVar("S".to_string())],
    }];

    assert_eq!(
        ctx.expand_constructor_self_type_arguments(
            &arguments,
            &[TypeParam::new("S".to_string())],
            Some("Owner"),
        ),
        vec![TypeExpr::Parameterized {
            base: "Vector".to_string(),
            params: vec![TypeExpr::TypeVar("S".to_string())],
        }]
    );
}

pub(super) fn qualified_constructor_alias_is_not_captured_by_same_leaf_binder_11019() {
    let mut ctx = constructor_identity_ctx(vec![], vec![]);
    ctx.type_aliases
        .insert("AliasOwner.S".to_string(), "Vector".to_string());
    let arguments = vec![TypeExpr::Parameterized {
        base: "AliasOwner.S".to_string(),
        params: vec![TypeExpr::TypeVar("S".to_string())],
    }];

    assert_eq!(
        ctx.expand_constructor_self_type_arguments(
            &arguments,
            &[TypeParam::new("S".to_string())],
            None,
        ),
        vec![TypeExpr::Parameterized {
            base: "Vector".to_string(),
            params: vec![TypeExpr::TypeVar("S".to_string())],
        }]
    );
}

pub(super) fn constructor_bounds_qualify_bare_lexical_owner_11019() {
    let ctx = constructor_identity_ctx(vec![], vec![("Owner.Bound", None)]);
    let expanded = ctx.expand_constructor_type_param_bounds(
        &[TypeParam::with_upper_bound(
            "T".to_string(),
            "Bound".to_string(),
        )],
        Some("Owner"),
    );

    assert_eq!(
        expanded[0].get_upper_bound().map(String::as_str),
        Some("Owner.Bound")
    );
}

pub(super) fn qualified_bound_owner_is_exact_when_both_sides_are_qualified_11019() {
    let ctx = constructor_identity_ctx(
        vec![
            ("A.Value", 0, Some("A.Bound")),
            ("B.Value", 1, Some("B.Bound")),
        ],
        vec![("A.Bound", None), ("B.Bound", None)],
    );

    assert!(ctx.concrete_type_satisfies_bound(&JuliaType::Struct("A.Value".to_string()), "A.Bound"));
    assert!(
        !ctx.concrete_type_satisfies_bound(&JuliaType::Struct("A.Value".to_string()), "B.Bound")
    );
}
