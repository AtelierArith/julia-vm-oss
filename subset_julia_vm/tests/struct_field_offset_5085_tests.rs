use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr, Value, Vm};

// Compile a snippet through the cached Base path (`parse_and_lower` merges the
// process-wide prelude, `compile_with_cache` reuses the persistent/thread-local
// Base bytecode). This compiles only the user functions instead of recompiling
// the whole prelude from source on every test, which keeps these field-offset
// checks fast (Issue #7589). The user functions whose bytecode we assert on are
// compiled identically to the uncached path — see
// `cached_base_inference_parity_6538_tests.rs`.
fn compile_source_with_base(source: &str) -> CompiledProgram {
    let program = parse_and_lower(source).expect("parse and lower source");
    compile_with_cache(&program).expect("compile failed")
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

fn run_source_with_base(source: &str) -> Value {
    let compiled = compile_source_with_base(source);
    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    vm.run().expect("vm run failed")
}

fn assert_i64(value: Value, expected: i64) {
    match value {
        Value::I64(actual) => assert_eq!(actual, expected),
        other => panic!("expected I64({}), got {:?}", expected, other),
    }
}

#[test]
fn concrete_struct_field_reads_emit_offset_getfield_issue_5085() {
    let src = r#"
struct Point5085
    x::Int64
    y::Int64
end

function read_point_5085(p::Point5085)
    p.y + p.x
end

read_point_5085(Point5085(3, 4))
"#;

    let compiled = compile_source_with_base(src);
    let func = get_function(&compiled, "read_point_5085");
    let body = function_body(&compiled, func);

    assert!(
        body.iter().any(|instr| matches!(instr, Instr::GetField(1))),
        "p.y should compile to fixed field offset 1: {:?}",
        body
    );
    assert!(
        body.iter().any(|instr| matches!(instr, Instr::GetField(0))),
        "p.x should compile to fixed field offset 0: {:?}",
        body
    );
    assert!(
        body.iter()
            .all(|instr| !matches!(instr, Instr::GetFieldByName(_))),
        "concrete struct field reads should not use name lookup: {:?}",
        body
    );

    assert_i64(run_source_with_base(src), 7);
}

#[test]
fn nested_concrete_struct_fields_keep_offset_access_issue_5085() {
    let src = r#"
struct Inner5085
    a::Int64
end

struct Outer5085
    inner::Inner5085
end

function nested_field_5085()
    o = Outer5085(Inner5085(7))
    o.inner.a
end

nested_field_5085()
"#;

    let compiled = compile_source_with_base(src);
    let func = get_function(&compiled, "nested_field_5085");
    let body = function_body(&compiled, func);

    let offset_reads = body
        .iter()
        .filter(|instr| matches!(instr, Instr::GetField(0)))
        .count();
    assert!(
        offset_reads >= 2,
        "o.inner and inner.a should both use fixed field offset 0: {:?}",
        body
    );
    assert!(
        body.iter()
            .all(|instr| !matches!(instr, Instr::GetFieldByName(_))),
        "nested concrete struct field reads should not use name lookup: {:?}",
        body
    );

    assert_i64(run_source_with_base(src), 7);
}

#[test]
fn concrete_mutable_struct_field_writes_emit_offset_setfield_issue_5085() {
    let src = r#"
mutable struct Box5085
    value::Int64
    other::Int64
end

function update_box_5085()
    b = Box5085(1, 2)
    b.other = 5
    b.other
end

update_box_5085()
"#;

    let compiled = compile_source_with_base(src);
    let func = get_function(&compiled, "update_box_5085");
    let body = function_body(&compiled, func);

    assert!(
        body.iter().any(|instr| matches!(instr, Instr::SetField(1))),
        "b.other assignment should compile to fixed field offset 1: {:?}",
        body
    );
    assert!(
        body.iter().any(|instr| matches!(instr, Instr::GetField(1))),
        "b.other read should compile to fixed field offset 1: {:?}",
        body
    );
    assert!(
        body.iter()
            .all(|instr| { !matches!(instr, Instr::SetFieldByName(_) | Instr::GetFieldByName(_)) }),
        "concrete mutable struct field access should not use name lookup: {:?}",
        body
    );

    assert_i64(run_source_with_base(src), 5);
}
