//! VM benchmark for Float64-typed function calls inside a typed loop.
//!
//! This benchmark measures the interpreter path where a function annotated
//! `::Float64` is called from a `Float64`-accumulating loop. It is intended
//! to exercise the F64 function inline / fast-call path in the VM.
//!
//! Run with: cargo bench -p subset_julia_vm --bench f64_function_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use std::time::Duration;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

const F64_FUNCTION_SOURCE: &str = r#"
f(x::Float64)::Float64 = x * 2.0 + 1.0
function sum_map(n::Int)::Float64
    s = 0.0
    for i in 1:n
        s += f(Float64(i))
    end
    s
end
"#;

const EXPECTED_SUM_MAP_10000: f64 = 100_020_000.0;

fn source_for_n(n: u64) -> String {
    format!("{F64_FUNCTION_SOURCE}\nsum_map({n})\n")
}

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn compile_f64_function_case(n: u64) -> CompiledProgram {
    compile_source(&source_for_n(n))
}

fn validate_result(compiled: &CompiledProgram, n: u64) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    let actual = match result {
        Value::F64(value) => value,
        other => panic!("sum_map({n}) returned non-Float64 value: {:?}", other),
    };
    assert!(
        (actual - EXPECTED_SUM_MAP_10000).abs() < 1.0e-9,
        "sum_map({n}) returned {actual}, expected {EXPECTED_SUM_MAP_10000}"
    );
    assert_eq!(vm.get_output(), "");
    black_box(actual);
}

fn bench_f64_function_inline(c: &mut Criterion) {
    let compiled = compile_f64_function_case(10_000);
    validate_result(&compiled, 10_000);

    let mut group = c.benchmark_group("vm_f64_function_inline");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);

    group.bench_with_input(
        BenchmarkId::new("run_only", 10_000),
        &compiled,
        |b, compiled| {
            b.iter_batched(
                || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                |mut vm| {
                    let result = vm.run().unwrap();
                    black_box(result);
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_with_input(
        BenchmarkId::new("clone_new_program_run", 10_000),
        &compiled,
        |b, compiled| {
            b.iter(|| {
                let mut vm = Vm::new_program(black_box(compiled.clone()), StableRng::new(0));
                let result = vm.run().unwrap();
                black_box(result);
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_f64_function_inline);
criterion_main!(benches);
