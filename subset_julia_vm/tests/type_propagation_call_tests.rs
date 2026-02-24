use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::specialize::specialize_function;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr};
use subset_julia_vm::vm::{Value, ValueType, Vm};

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

fn run_source_with_base(source: &str) -> (Value, Vm<StableRng>) {
    let compiled = compile_source_with_base(source);
    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run().expect("vm run failed");
    (result, vm)
}

fn resolve_value(v: &Value, heap: &[subset_julia_vm::vm::value::StructInstance]) -> Value {
    match v {
        Value::StructRef(idx) => heap
            .get(*idx)
            .map(|s| Value::Struct(s.clone()))
            .unwrap_or_else(|| v.clone()),
        _ => v.clone(),
    }
}

#[test]
fn test_typed_xy_propagate_to_static_call_for_f_xy() {
    let src = r#"
function f(x::Int64, y::Int64)
    x + y
end

function g(x::Int64, y::Int64)
    f(x, y)
end

g(1, 2)
"#;

    let compiled = compile_source_with_base(src);
    let g = get_function(&compiled, "g");
    let body = function_body(&compiled, g);

    println!("g bytecode: {:?}", body);

    let has_direct_call_to_f = body.iter().any(|instr| match instr {
        Instr::Call(func_idx, 2) => compiled
            .functions
            .get(*func_idx)
            .map(|fi| fi.name == "f")
            .unwrap_or(false),
        _ => false,
    });
    let has_dynamic_dispatch = body.iter().any(|instr| {
        matches!(
            instr,
            Instr::CallDynamic(_, _, _)
                | Instr::CallDynamicBinary(_, _, _)
                | Instr::CallDynamicBinaryBoth(_, _)
                | Instr::CallTypedDispatch(_, _, _, _)
        )
    });

    assert!(
        has_direct_call_to_f,
        "Expected direct Call to f in typed g(x::Int64, y::Int64)"
    );
    assert!(
        !has_dynamic_dispatch,
        "Typed g(x::Int64, y::Int64) should not require dynamic dispatch"
    );
}

#[test]
fn test_untyped_xy_uses_dynamic_dispatch_when_f_is_overloaded() {
    let src = r#"
function f(x::Int64, y::Int64)
    x + y
end

function f(x::Float64, y::Float64)
    x + y
end

function h(x, y)
    f(x, y)
end

h(1, 2)
"#;

    let compiled = compile_source_with_base(src);
    let h = get_function(&compiled, "h");
    let body = function_body(&compiled, h);

    println!("h bytecode: {:?}", body);

    let has_typed_dispatch = body
        .iter()
        .any(|instr| matches!(instr, Instr::CallTypedDispatch(_, 2, _, _)));
    assert!(
        has_typed_dispatch,
        "Expected CallTypedDispatch in untyped h(x, y) with overloaded f methods"
    );
}

#[test]
fn test_untyped_f_xy_uses_runtime_specialization_for_int_and_complex_calls() {
    let src_for_bytecode = r#"
function f(x, y)
    x + 2y
end

function g1()
    f(1, 2)
end

function g2()
    f(1, 2im)
end

g1()
g2()
"#;

    let compiled = compile_source_with_base(src_for_bytecode);
    let g1 = get_function(&compiled, "g1");
    let g2 = get_function(&compiled, "g2");
    let g1_body = function_body(&compiled, g1);
    let g2_body = function_body(&compiled, g2);

    println!("g1 bytecode: {:?}", g1_body);
    println!("g2 bytecode: {:?}", g2_body);

    assert!(
        g1_body
            .iter()
            .any(|instr| matches!(instr, Instr::CallSpecialize(_, 2))),
        "g1() should call f through CallSpecialize for untyped parameters"
    );
    assert!(
        g2_body
            .iter()
            .any(|instr| matches!(instr, Instr::CallSpecialize(_, 2))),
        "g2() should call f through CallSpecialize for untyped parameters"
    );

    let (result_int, _vm1) = run_source_with_base(
        r#"
function f(x, y)
    x + 2y
end
f(1, 2)
"#,
    );
    match result_int {
        Value::I64(v) => assert_eq!(v, 5),
        other => panic!("Expected I64(5) for f(1,2), got {:?}", other),
    }

    let (result_complex, vm2) = run_source_with_base(
        r#"
function f(x, y)
    x + 2y
end
f(1, 2im)
"#,
    );
    let resolved_complex = resolve_value(&result_complex, vm2.get_struct_heap());
    let (re, im) = resolved_complex
        .as_complex_parts()
        .unwrap_or_else(|| panic!("Expected Complex for f(1, 2im), got {:?}", result_complex));
    assert!((re - 1.0).abs() < 1e-10, "real part mismatch: {}", re);
    assert!((im - 4.0).abs() < 1e-10, "imag part mismatch: {}", im);
}

#[test]
fn test_specialized_f_xy_instruction_selection_int_vs_complex() {
    let src = r#"
function f(x, y)
    x + 2y
end

f(1, 2)
f(1, 2im)
"#;

    let compiled = compile_source_with_base(src);
    let f = compiled
        .specializable_functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap_or_else(|| panic!("specializable function 'f' not found"));

    let int_spec =
        specialize_function(&f.ir, &[ValueType::I64, ValueType::I64]).expect("int specialize");
    assert!(
        int_spec.code.iter().any(|i| matches!(i, Instr::MulI64)),
        "Int specialization should emit MulI64"
    );
    assert!(
        int_spec.code.iter().any(|i| matches!(i, Instr::AddI64)),
        "Int specialization should emit AddI64"
    );

    let complex_type_id = compiled
        .struct_defs
        .iter()
        .enumerate()
        .find(|(_, d)| d.name == "Complex" || d.name.starts_with("Complex{"))
        .map(|(idx, _)| idx)
        .expect("Complex type not found");
    let complex_spec = specialize_function(
        &f.ir,
        &[ValueType::I64, ValueType::Struct(complex_type_id)],
    )
    .expect("complex specialize");

    assert!(
        complex_spec
            .code
            .iter()
            .any(|i| matches!(i, Instr::DynamicMul)),
        "Complex specialization currently uses DynamicMul"
    );
    assert!(
        complex_spec
            .code
            .iter()
            .any(|i| matches!(i, Instr::DynamicAdd)),
        "Complex specialization currently uses DynamicAdd"
    );
}
