#[cfg(feature = "profiling")]
use std::collections::HashMap;
use std::collections::HashSet;
use subset_julia_vm::builtins::BuiltinId;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
#[cfg(feature = "profiling")]
use subset_julia_vm::vm::profiler;
use subset_julia_vm::vm::specialize::specialize_function;
use subset_julia_vm::vm::{CompiledProgram, Instr, Value, ValueType, Vm};

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().expect("create parser");
    let parsed = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).expect("lower source");
    compile_with_cache(&program).expect("compile source")
}

#[cfg(feature = "profiling")]
fn run_with_profile(source: &str) -> HashMap<String, u64> {
    let mut vm = Vm::new_program(compile_source(source), StableRng::new(0));

    profiler::clear();
    profiler::enable();
    let _ = vm.run().expect("run with VM profiler");
    profiler::disable();

    profiler::get_results().into_iter().collect()
}

const MANDELBROT_ESCAPE_SOURCE: &str = r#"
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end
"#;

const VM_MANDELBROT_SOURCE: &str = include_str!("../../benchmarks/vm_mandelbrot.jl");

fn function_body<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a [Instr] {
    let function = compiled
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function '{name}' not found"));
    &compiled.code[function.code_start..function.code_end]
}

#[test]
fn runtime_specialized_mandelbrot_escape_uses_concrete_complex_f64_ops_6259() {
    let compiled = compile_source(&format!(
        "{MANDELBROT_ESCAPE_SOURCE}\nmandelbrot_escape(0.0 + 0.0im, 10)"
    ));
    let specializable = compiled
        .specializable_functions
        .iter()
        .find(|f| f.name == "mandelbrot_escape")
        .expect("mandelbrot_escape should be registered for runtime specialization");
    let type_object_names = HashSet::new();
    let specialized = specialize_function(
        &specializable.ir,
        &[ValueType::ComplexF64, ValueType::I64],
        &compiled.struct_defs,
        &type_object_names,
        None,
        false,
        false,
    )
    .expect("specialize mandelbrot_escape(::ComplexF64, ::Int64)");
    let code = &specialized.code;

    assert_eq!(specialized.return_type, ValueType::I64);
    assert!(
        !code.iter().any(|instr| matches!(instr, Instr::DynamicPow)),
        "ComplexF64 z^2 should not emit DynamicPow: {code:?}"
    );
    assert!(
        !code
            .iter()
            .any(|instr| matches!(instr, Instr::CallDynamicBinaryBoth(_, _))),
        "ComplexF64 z^2 + c should not emit generic binary dispatch: {code:?}"
    );
    assert!(
        !code
            .iter()
            .any(|instr| matches!(instr, Instr::CallResolved(_, _))),
        "abs2(::ComplexF64) should inline field arithmetic, not call a resolved method: {code:?}"
    );
    assert!(
        code.iter()
            .filter(|instr| matches!(instr, Instr::GetField(0) | Instr::GetField(1)))
            .count()
            >= 8,
        "ComplexF64 arithmetic should expose real/imag field loads: {code:?}"
    );
    assert!(
        code.iter()
            .any(|instr| matches!(instr, Instr::NewParametricStruct(name, 2) if name == "Complex")),
        "ComplexF64 arithmetic should rebuild Complex through the existing parametric struct path: {code:?}"
    );
    assert!(
        code.iter()
            .filter(|instr| matches!(instr, Instr::MulF64))
            .count()
            >= 4,
        "ComplexF64 abs2/square should use typed Float64 multiplication: {code:?}"
    );
}

#[test]
fn mandelbrot_escape_original_source_preserves_results_6259() {
    let source = format!(
        r#"{MANDELBROT_ESCAPE_SOURCE}

mandelbrot_escape(0.0 + 0.0im, 10) +
mandelbrot_escape(-0.75 + 0.0im, 20) * 10 +
mandelbrot_escape(1.0 + 1.0im, 20) * 100
"#
    );
    let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
    let result = vm.run().expect("run Mandelbrot escape probes");
    match result {
        Value::I64(value) => assert_eq!(value, 510),
        other => panic!("expected Int64 Mandelbrot probe checksum, got {other:?}"),
    }
}

#[test]
fn mandelbrot_broadcast_original_source_preserves_results_6259() {
    let source = format!(
        r#"{MANDELBROT_ESCAPE_SOURCE}

C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
grid = mandelbrot_escape.(C, Ref(10))
grid[1,1] + grid[1,2] + grid[2,1] + grid[2,2]
"#
    );
    let mut vm = Vm::new_program(compile_source(&source), StableRng::new(0));
    let result = vm.run().expect("run broadcast Mandelbrot probes");
    match result {
        Value::I64(value) => assert_eq!(value, 25),
        other => panic!("expected Int64 broadcast Mandelbrot checksum, got {other:?}"),
    }
}

#[test]
fn mandel_count_fuses_loop_branches_and_i64_float_conversions_6167() {
    let compiled = compile_source(VM_MANDELBROT_SOURCE);
    let body = function_body(&compiled, "mandel_count");

    assert!(
        body.iter()
            .filter(|instr| matches!(instr, Instr::JumpIfGtI64Slots(_, _, _)))
            .count()
            >= 2,
        "mandel_count should compare x/y loop slots directly: {body:?}"
    );
    assert!(
        body.iter()
            .filter(|instr| matches!(instr, Instr::LoadSlotI64ToF64(_)))
            .count()
            >= 4,
        "mandel_count should fuse Float64(slot) conversions: {body:?}"
    );
    assert!(
        !body.windows(2).any(|window| {
            matches!(
                window,
                [
                    Instr::LoadSlotI64(_),
                    Instr::CallBuiltin(BuiltinId::Float64, 1)
                ]
            )
        }),
        "mandel_count should not leave LoadSlotI64 + CallBuiltin(Float64): {body:?}"
    );

    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let _ = vm.run().expect("run VM Mandelbrot benchmark source");
    assert_eq!(vm.get_output(), "166265\n");
}

#[cfg(feature = "profiling")]
#[test]
fn broadcast_runtime_callable_escape_avoids_dynamic_pow_6259() {
    let source = format!(
        r#"{MANDELBROT_ESCAPE_SOURCE}

C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
grid = mandelbrot_escape.(C, Ref(10))
grid[1,1]
"#
    );
    let counts = run_with_profile(&source);

    assert!(
        counts.get("CallFunctionVariable").copied().unwrap_or(0) > 0,
        "probe should exercise broadcast _broadcast_apply's function-variable path: {counts:?}"
    );
    assert_eq!(
        counts.get("DynamicPow").copied().unwrap_or(0),
        0,
        "broadcasted mandelbrot_escape should not execute DynamicPow in the escape loop: {counts:?}"
    );
}

#[cfg(feature = "profiling")]
#[test]
fn broadcast_runtime_callable_escape_uses_executable_block_6253() {
    let source = format!(
        r#"{MANDELBROT_ESCAPE_SOURCE}

C = [0.0 + 0.0im 1.0 + 1.0im; -1.0 + 0.5im 0.5 + 0.0im]
grid = mandelbrot_escape.(C, Ref(10))
grid[1,1] + grid[1,2] + grid[2,1] + grid[2,2]
"#
    );
    let counts = run_with_profile(&source);

    assert!(
        counts
            .get("ExecutableBlock::ComplexF64MandelbrotEscapeLoop")
            .copied()
            .unwrap_or(0)
            > 0,
        "broadcasted mandelbrot_escape should run through the ComplexF64 executable block: {counts:?}"
    );
}

#[cfg(feature = "profiling")]
#[test]
fn function_variable_escape_avoids_dynamic_complex_ops_6259() {
    let source = format!(
        r#"{MANDELBROT_ESCAPE_SOURCE}

f = mandelbrot_escape
f(0.0 + 0.0im, 10)
"#
    );
    let counts = run_with_profile(&source);

    assert!(
        counts.get("CallFunctionVariable").copied().unwrap_or(0) > 0,
        "probe should exercise function-variable dispatch: {counts:?}"
    );
    assert_eq!(
        counts.get("DynamicPow").copied().unwrap_or(0),
        0,
        "function-variable mandelbrot_escape should not execute DynamicPow: {counts:?}"
    );
}
