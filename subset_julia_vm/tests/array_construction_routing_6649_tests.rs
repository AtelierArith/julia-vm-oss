//! Bytecode guards for public Array construction routing (Issue #6649).

use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};

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
        .unwrap_or_else(|| panic!("function '{name}' not found"))
}

fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
    &compiled.code[f.code_start..f.code_end]
}

fn is_native_array_carrier_builder(instr: &Instr) -> bool {
    // NOTE: `FinalizeArray`/`FinalizeArrayTyped` are intentionally NOT listed.
    // They were the legacy native-carrier finalize, but Issue #6807 (Slice 4)
    // de-varianted the build buffer onto `Value::Memory`, so they now finalize a
    // `Memory` into the MemoryRef-backed `Array{T,N}` wrapper. Issue #6846 routes
    // public array literals through that native finalize (instead of a
    // per-literal pure-Julia `wrap` call), so a `FinalizeArray` in public
    // construction bytecode is the *wrapper* path, not a native carrier.
    matches!(
        instr,
        Instr::NewArray(_)
            | Instr::PushArrayValue(_)
            | Instr::PushElem
            | Instr::NewArrayTyped(_, _)
            | Instr::PushElemTyped
            | Instr::AllocUndefTyped(_, _)
            | Instr::AllocUndefTypedFromTuple(_)
            | Instr::AllocUndefDynamicTyped(_)
            | Instr::AllocUndefDynamicTypedFromTuple
    )
}

#[test]
fn public_array_literals_do_not_emit_native_array_builders_issue_6649() {
    let compiled = compile_source_with_base(
        r#"
function public_array_literal_construction_6649()
    a = [1, 2, 3]
    b = Int64[4, 5]
    c = [i for i in 1:3]
    d = Int64[i for i in 1:3]
    e = Vector{Int64}()
    m = [1 2; 3 4]
    return a[2] + b[1] + c[3] + d[2] + length(e) + m[2, 1]
end
"#,
    );
    let function = get_function(&compiled, "public_array_literal_construction_6649");
    let body = function_body(&compiled, function);

    let offenders: Vec<_> = body
        .iter()
        .filter(|instr| is_native_array_carrier_builder(instr))
        .collect();

    assert!(
        offenders.is_empty(),
        "public array literal bytecode must use Memory + Array wrapper construction: {offenders:#?}"
    );

    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::NewMemory(_, _))),
        "expected public array literal bytecode to allocate Memory storage: {body:#?}"
    );
    // Issue #6846: literals finalize the `Memory` into the `Array{T,N}` wrapper
    // natively via `FinalizeArray` instead of a per-literal pure-Julia
    // `wrap(Array, memory, dims)` call (which spun up ~5 Julia frames per
    // literal). The wrapper is still MemoryRef-backed — no native carrier.
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::FinalizeArray(_) | Instr::FinalizeArrayTyped(_))),
        "expected public array literal bytecode to finalize Memory into the Array wrapper natively: {body:#?}"
    );
}

#[test]
fn public_array_materialization_routes_do_not_emit_native_array_carriers_issue_6653() {
    let compiled = compile_source_with_base(
        r#"
function public_array_materialization_surface_6653()
    a = [1, 2, 3]
    b = Array{Int64}(undef, 3)
    for i in 1:3
        b[i] = i
    end
    c = Array{Int64}(undef, (2, 2))
    for i in 1:4
        c[i] = i
    end
    d = collect(1:3)
    e = collect((1, 2, 3))
    f = collect(x * 2 for x in a)
    g = [x + 1 for x in a]
    h = map(x -> x + 1, a)
    i = filter(isodd, a)
    j = broadcast(+, a, a)
    k = similar(a)
    l = zeros(Int64, 3)
    m = ones(Int64, 3)
    n = reshape([1, 2, 3, 4], (2, 2))
    return b[2] + c[2, 2] + d[3] + e[1] + f[2] + g[3] +
        h[1] + i[2] + j[3] + length(k) + l[1] + m[2] + n[2, 1]
end
"#,
    );
    let function = get_function(&compiled, "public_array_materialization_surface_6653");
    let body = function_body(&compiled, function);

    let offenders: Vec<_> = body
        .iter()
        .filter(|instr| is_native_array_carrier_builder(instr))
        .collect();

    assert!(
        offenders.is_empty(),
        "public array materialization bytecode must not emit native array carrier builders: {offenders:#?}"
    );

    // Issue #6846: literal/comprehension construction finalizes `Memory` into
    // the `Array{T,N}` wrapper natively via `FinalizeArray` (no per-literal
    // pure-Julia `wrap` call); the wrapper stays MemoryRef-backed.
    assert!(
        body.iter()
            .any(|instr| matches!(instr, Instr::FinalizeArray(_) | Instr::FinalizeArrayTyped(_))),
        "expected public array routes to finalize Memory into the Array wrapper natively: {body:#?}"
    );
    assert!(
        body.iter().any(
            |instr| matches!(instr, Instr::PushFunction(name) if name == "_array_undef_from_dims")
        ),
        "expected Array{{T}}(undef, ...) to route through _array_undef_from_dims: {body:#?}"
    );
}
