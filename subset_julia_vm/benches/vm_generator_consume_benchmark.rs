//! Precomputed-bytecode VM benchmark for consuming SIMPLE generators
//! (`sum(x*x for x in 1:N)` / `collect(2x for x in 1:N)`) — the hot paths the
//! Issue #9200 S2 generator desugar touches.
//!
//! Isolates `Vm::run()` from CLI startup, parsing, lowering, and bytecode
//! compilation so the generator + `collect_generator` fast paths can be measured
//! directly. The S2 desugar rewrites `(x*x for x in 1:N)` to the upstream
//! `Base.Generator(func, iter)` shape but compiles to the byte-identical
//! `MakeGenerator(FunctionIndex) + CallDynamic(collect/sum)` the pre-desugar
//! `Expr::Generator` path produced, so this is a regression guard.
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_generator_consume_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const GENERATOR_CONSUME_SOURCE: &str = include_str!("../../benchmarks/vm_generator_consume.jl");
const EXPECTED_OUTPUT: &str = "66766900000\n";

// Issue #9200 S3: the FILTERED-generator consume path (`sum`/`collect` over
// `(f(x) for x in 1:N if p(x))`), which the S3 desugar collapses to the native
// `MakeGenerator(FilteredFunctionIndex) + CallDynamic` shape.
const FILTERED_GENERATOR_CONSUME_SOURCE: &str =
    include_str!("../../benchmarks/vm_generator_consume_filtered.jl");
const FILTERED_EXPECTED_OUTPUT: &str = "33433500000\n";

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate(compiled: &CompiledProgram, expected: &str) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), expected);
    black_box(result);
}

fn bench_run_only(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    compiled: &CompiledProgram,
) {
    group.bench_function(name, |b| {
        b.iter_batched(
            || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
            |mut vm| {
                let result = vm.run().unwrap();
                black_box(result);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_vm_generator_consume(c: &mut Criterion) {
    let simple = compile_source(GENERATOR_CONSUME_SOURCE);
    validate(&simple, EXPECTED_OUTPUT);
    let filtered = compile_source(FILTERED_GENERATOR_CONSUME_SOURCE);
    validate(&filtered, FILTERED_EXPECTED_OUTPUT);

    let mut group = c.benchmark_group("vm_generator_consume");
    bench_run_only(&mut group, "run_only", &simple);
    bench_run_only(&mut group, "run_only_filtered", &filtered);
    group.finish();
}

criterion_group!(benches, bench_vm_generator_consume);
criterion_main!(benches);
