//! VM-only benchmark for the OrdinaryDiffEq Lorenz `Tsit5` solve (Issue #8094).
//!
//! The adaptive in-place-buffered Tsit5 stepper is ~96% of the iOS "Lorenz
//! Attractor" sample wall time. This isolates `Vm::run()` (the solve) from CLI
//! startup, parsing, lowering, bytecode compilation, and Plots artifact
//! generation, so regressions in the stepper (e.g. losing the reusable stage
//! buffers added in #8094) are visible. Run with:
//!
//!     cargo bench -p subset_julia_vm --bench vm_ode_tsit5_lorenz_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const SOURCE: &str = include_str!("../../benchmarks/vm_ode_tsit5_lorenz.jl");
const EXPECTED_OUTPUT: &str = "1001\n";

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

fn bench_vm_ode_tsit5_lorenz(c: &mut Criterion) {
    let compiled = compile_source();
    validate(&compiled);

    let mut group = c.benchmark_group("vm_ode_tsit5_lorenz");

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

criterion_group!(benches, bench_vm_ode_tsit5_lorenz);
criterion_main!(benches);
