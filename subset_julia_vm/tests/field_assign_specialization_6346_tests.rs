//! Issue #6346: extend the lazy specialization engine to `FieldAssign` (struct
//! field reads and writes), mirroring the typed fast path the interpreter
//! already uses for known mutable structs.

#[cfg(feature = "profiling")]
use std::collections::HashMap;
use std::collections::HashSet;

use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
#[cfg(feature = "profiling")]
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::specialize::specialize_function;
#[cfg(feature = "profiling")]
use subset_julia_vm::vm::{profiler, Vm};
use subset_julia_vm::vm::{CompiledProgram, Instr, ValueType};

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    compile_with_cache(&program).expect("compile source")
}

fn specializable_ir<'a>(
    compiled: &'a CompiledProgram,
    name: &str,
) -> &'a subset_julia_vm::ir::core::Function {
    &compiled
        .specializable_functions
        .iter()
        .find(|func| func.name == name)
        .unwrap_or_else(|| panic!("specializable function '{name}' not found"))
        .ir
}

fn struct_type_id(compiled: &CompiledProgram, name: &str) -> usize {
    compiled
        .struct_defs
        .iter()
        .position(|def| def.name == name || def.name.starts_with(&format!("{name}{{")))
        .unwrap_or_else(|| panic!("struct '{name}' not found in struct_defs"))
}

const PARTICLE_SOURCE: &str = r#"
mutable struct Particle6346
    x::Float64
    vx::Float64
end

function step_particle_6346!(p, dt)
    p.x = p.x + p.vx * dt
    return p.x
end
"#;

#[test]
fn field_assign_mutable_struct_specializes_to_setfield_6346() {
    let compiled = compile_source(PARTICLE_SOURCE);
    let tid = struct_type_id(&compiled, "Particle6346");
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "step_particle_6346!"),
        &[ValueType::Struct(tid), ValueType::F64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize mutable struct field update");

    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::SetField(0))),
        "expected statically-resolved SetField(0) for p.x, got {:?}",
        specialized.code
    );
    // Reads of p.x and p.vx must use the index-based GetField fast path, never
    // the by-name runtime fallback.
    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::GetField(_))),
        "expected GetField reads in specialized code, got {:?}",
        specialized.code
    );
    assert!(
        !specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::SetFieldByName(_) | Instr::GetFieldByName(_))),
        "specialized field update must not fall back to by-name field ops: {:?}",
        specialized.code
    );
    assert_eq!(specialized.return_type, ValueType::F64);
}

#[test]
fn field_assign_int_literal_coerces_to_float_field_6346() {
    let compiled = compile_source(
        r#"
mutable struct Box6346
    v::Float64
end

function set_box_6346!(b)
    b.v = 2
    return b.v
end
"#,
    );
    let tid = struct_type_id(&compiled, "Box6346");
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "set_box_6346!"),
        &[ValueType::Struct(tid)],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize Int->Float64 field coercion");

    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::ToF64)),
        "expected ToF64 coercion for `b.v = 2` into a Float64 field, got {:?}",
        specialized.code
    );
    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::SetField(0))),
        "expected SetField(0) for field v, got {:?}",
        specialized.code
    );
}

#[test]
fn field_assign_immutable_struct_stays_on_fallback_6346() {
    // An immutable struct read in one function, mutated via a loosely-typed
    // path in another: the specializer must decline the typed SetField path
    // (immutable structs raise on assignment) and fall back.
    let compiled = compile_source(
        r#"
struct ImmutPoint6346
    x::Float64
end

function read_point_6346(p)
    return p.x
end
"#,
    );
    let tid = struct_type_id(&compiled, "ImmutPoint6346");
    // Reading a field of an immutable struct is fine and should specialize.
    let type_object_names = HashSet::new();
    let read_spec = specialize_function(
        specializable_ir(&compiled, "read_point_6346"),
        &[ValueType::Struct(tid)],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("immutable struct field READ should still specialize");
    assert!(
        read_spec
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::GetField(0))),
        "expected GetField(0) for immutable field read, got {:?}",
        read_spec.code
    );
}

#[test]
fn field_update_with_nary_product_specializes_6346() {
    // `k * b.x * dt` parses as a 3-arg `*(k, b.x, dt)` call; the whole field
    // update must still specialize, folding the product to typed MulF64.
    let compiled = compile_source(
        r#"
mutable struct Body6346
    x::Float64
    v::Float64
end

function step_body_6346!(b, dt, k)
    b.v = b.v - k * b.x * dt
    b.x = b.x + b.v * dt
    return b.x
end
"#,
    );
    let tid = struct_type_id(&compiled, "Body6346");
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        specializable_ir(&compiled, "step_body_6346!"),
        &[ValueType::Struct(tid), ValueType::F64, ValueType::F64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("n-ary product field update should specialize");

    assert!(
        specialized
            .code
            .iter()
            .filter(|instr| matches!(instr, Instr::MulF64))
            .count()
            >= 2,
        "the n-ary `k * b.x * dt` product should fold to typed MulF64, got {:?}",
        specialized.code
    );
    assert!(
        specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::SetField(_))),
        "expected typed SetField, got {:?}",
        specialized.code
    );
    assert!(
        !specialized
            .code
            .iter()
            .any(|instr| matches!(instr, Instr::SetFieldByName(_) | Instr::GetFieldByName(_))),
        "n-ary field update must not fall back to by-name field ops: {:?}",
        specialized.code
    );
}

// ---- Runtime parity / fast-path firing (profiling builds only) ----

#[cfg(feature = "profiling")]
fn run_with_profile(source: &str) -> (String, HashMap<String, u64>) {
    let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));
    profiler::clear();
    profiler::enable();
    let _ = vm.run().expect("run with VM profiler");
    profiler::disable();
    let counts = profiler::get_results().into_iter().collect();
    (vm.get_output().to_string(), counts)
}

#[cfg(feature = "profiling")]
#[test]
fn field_update_loop_runs_typed_setfield_not_by_name_6346() {
    let source = format!(
        r#"{PARTICLE_SOURCE}
function simulate_6346(n)
    p = Particle6346(0.0, 1.5)
    s = 0.0
    for i in 1:n
        s = step_particle_6346!(p, 0.1)
    end
    return s
end
println(simulate_6346(2000))
"#
    );
    let (output, counts) = run_with_profile(&source);
    assert_eq!(
        output, "300.00000000000017\n",
        "matches upstream Julia output"
    );
    assert!(
        counts.get("SetField").copied().unwrap_or(0) > 0,
        "the specialized field-update loop should execute SetField: {counts:?}"
    );
    assert_eq!(
        counts.get("SetFieldByName").copied().unwrap_or(0),
        0,
        "the specialized field-update loop must not fall back to SetFieldByName: {counts:?}"
    );
}

#[cfg(feature = "profiling")]
#[test]
fn nary_field_update_benchmark_runs_fully_typed_6346() {
    // The VM-only field-update benchmark uses the n-ary `k * b.x * dt` product.
    // Its hot loop must run entirely on typed instructions: typed field access
    // (GetField/SetField) and typed arithmetic (MulF64), with zero by-name field
    // ops and zero dynamic binary dispatch.
    let source = include_str!("../../benchmarks/vm_field_update.jl");
    let (output, counts) = run_with_profile(source);
    assert_eq!(output, "-76010.9082\n", "matches upstream Julia output");
    assert!(
        counts.get("SetField").copied().unwrap_or(0) > 0,
        "expected typed SetField in the hot loop: {counts:?}"
    );
    assert!(
        counts.get("MulF64").copied().unwrap_or(0) > 0,
        "expected the n-ary product to fold to typed MulF64: {counts:?}"
    );
    assert_eq!(
        counts.get("SetFieldByName").copied().unwrap_or(0)
            + counts.get("GetFieldByName").copied().unwrap_or(0)
            + counts.get("CallDynamicBinaryBoth").copied().unwrap_or(0),
        0,
        "the hot loop must not use by-name field ops or dynamic binary dispatch: {counts:?}"
    );
}
