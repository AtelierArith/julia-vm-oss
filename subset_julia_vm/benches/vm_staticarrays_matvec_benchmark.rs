//! Precomputed-bytecode VM benchmark for StaticArrays SMatrix*SVector arithmetic
//! (Issues #7461 / #7956).
//!
//! The driver (`benchmarks/vm_staticarrays_matvec.jl`) iterates the 2x2 affine
//! map `x <- W*x + b` so each step runs `SMatrix{2,2} * SVector{2}` and
//! `SVector{2} + SVector{2}`. It isolates `Vm::run()` from CLI startup, parsing,
//! lowering, and compilation so regressions in the static-array hot paths are
//! caught directly. #7956 made these paths roughly 3x faster by (a) reading dims
//! via where-clause `size`/`length` value methods instead of
//! `typeof(x).parameters[i]` reflection and (b) indexing the backing `.data`
//! tuple directly in hand-unrolled fast paths instead of per-element typed
//! `getindex`; this bench guards against that regressing back to the allocation-
//! and reflection-heavy generic loop.
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_staticarrays_matvec_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const STATICARRAYS_MATVEC_SOURCE: &str = include_str!("../../benchmarks/vm_staticarrays_matvec.jl");
const EXPECTED_OUTPUT: &str = "21730\n";

fn compile_staticarrays_matvec() -> CompiledProgram {
    // `parse_and_lower` runs the full frontend including `using StaticArrays`
    // package resolution, which the bare Parser+Lowering pipeline skips.
    let program = parse_and_lower(STATICARRAYS_MATVEC_SOURCE).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_staticarrays_matvec(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_staticarrays_matvec(c: &mut Criterion) {
    let compiled = compile_staticarrays_matvec();
    validate_staticarrays_matvec(&compiled);

    let mut group = c.benchmark_group("vm_staticarrays_matvec");

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

criterion_group!(benches, bench_vm_staticarrays_matvec);
criterion_main!(benches);
