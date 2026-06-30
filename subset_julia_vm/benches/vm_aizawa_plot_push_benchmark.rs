//! VM-only benchmark for Plots `push!(plt, x, y, z)` growth used by the Aizawa
//! attractor animation (Issue #7431).
//!
//! This isolates `Vm::run()` from CLI startup, parsing, lowering, and bytecode
//! compilation. Run with:
//!
//!     cargo bench -p subset_julia_vm --bench vm_aizawa_plot_push_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, Vm};

const SOURCE: &str = include_str!("../../benchmarks/vm_aizawa_plot_push.jl");
const EXPECTED_OUTPUT: &str = "3000\n";

fn compile_source() -> CompiledProgram {
    let program = parse_and_lower(SOURCE).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_aizawa_plot_push(c: &mut Criterion) {
    let compiled = compile_source();
    validate(&compiled);

    let mut group = c.benchmark_group("vm_aizawa_plot_push");

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

    group.finish();
}

criterion_group!(benches, bench_vm_aizawa_plot_push);
criterion_main!(benches);
