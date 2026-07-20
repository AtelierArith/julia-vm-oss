//! VM-only benchmark for exp(::Real) hot loops (Issue #7455).
//!
//! This separates `Vm::run()` from CLI startup, parsing, lowering, and bytecode
//! compilation. Run with:
//!
//!     cargo bench -p subset_julia_vm --bench vm_exp_real_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

const N: i64 = 10_000;

const LOOP_SIN_SOURCE: &str = r#"
function loop_sin(n::Int64)::Float64
    i = 0
    x = -1.0
    step = 0.000001
    acc = 0.0
    while i < n
        x = x + step
        acc = acc + sin(x)
        i = i + 1
    end
    acc
end

loop_sin(10000)
"#;

const LOOP_COS_SOURCE: &str = r#"
function loop_cos(n::Int64)::Float64
    i = 0
    x = -1.0
    step = 0.000001
    acc = 0.0
    while i < n
        x = x + step
        acc = acc + cos(x)
        i = i + 1
    end
    acc
end

loop_cos(10000)
"#;

const LOOP_EXP_SOURCE: &str = r#"
function loop_exp(n::Int64)::Float64
    i = 0
    x = -1.0
    step = 0.000001
    acc = 0.0
    while i < n
        x = x + step
        acc = acc + exp(x)
        i = i + 1
    end
    acc
end

loop_exp(10000)
"#;

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate(compiled: &CompiledProgram, expected: f64) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    match result {
        Value::F64(actual) => {
            assert!(
                (actual - expected).abs() <= 1.0e-9,
                "expected {expected}, got {actual}"
            );
            black_box(actual);
        }
        other => panic!("expected Float64 result, got {other:?}"),
    }
}

fn bench_vm_exp_real(c: &mut Criterion) {
    let cases = [
        ("sin", LOOP_SIN_SOURCE, -8387.551990943264),
        ("cos", LOOP_COS_SOURCE, 5445.010401154077),
        ("exp", LOOP_EXP_SOURCE, 3697.2516992291353),
    ];

    let compiled = cases
        .iter()
        .map(|(name, source, expected)| {
            let program = compile_source(source);
            validate(&program, *expected);
            (*name, program)
        })
        .collect::<Vec<_>>();

    let mut group = c.benchmark_group("vm_exp_real");
    group.throughput(criterion::Throughput::Elements(N as u64));

    for (name, program) in compiled {
        group.bench_function(BenchmarkId::new("run_only", name), |b| {
            b.iter_batched(
                || Vm::new_program(black_box(program.clone()), StableRng::new(0)),
                |mut vm| {
                    let result = vm.run().unwrap();
                    black_box(result);
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_vm_exp_real);
criterion_main!(benches);
