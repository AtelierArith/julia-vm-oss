//! Precomputed-bytecode VM Mandelbrot benchmarks.
//!
//! This benchmark intentionally separates `Vm::run()` from CLI startup,
//! parsing, lowering, and bytecode compilation. Use it for VM interpreter
//! changes where `sjulia benchmarks/vm_mandelbrot.jl` is dominated by frontend
//! setup cost.
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_mandelbrot_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const MANDELBROT_SOURCE: &str = include_str!("../../benchmarks/vm_mandelbrot.jl");
const EXPECTED_OUTPUT: &str = "166265\n";

fn compile_mandelbrot() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(MANDELBROT_SOURCE).unwrap();
    let mut lowering = Lowering::new(MANDELBROT_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_mandelbrot(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_mandelbrot(c: &mut Criterion) {
    let compiled = compile_mandelbrot();
    validate_mandelbrot(&compiled);

    let mut group = c.benchmark_group("vm_mandelbrot");

    group.bench_function("run_only", |b| {
        b.iter_batched(
            || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
            |mut vm| {
                let result = vm.run().unwrap();
                black_box(result);
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("clone_new_program_run", |b| {
        b.iter(|| {
            let mut vm = Vm::new_program(black_box(compiled.clone()), StableRng::new(0));
            let result = vm.run().unwrap();
            black_box(result);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_vm_mandelbrot);
criterion_main!(benches);
