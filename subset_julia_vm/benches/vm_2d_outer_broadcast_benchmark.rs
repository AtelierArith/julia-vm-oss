//! VM-only benchmark for 2D "outer" binary broadcasts (Issue #9155).
//!
//! `xs' .+ im .* ys` (a row Array combined with a column Array, e.g. the
//! Mandelbrot-grid construction pattern `mandelbrot_grid(width, height,
//! maxiter)`) used to re-derive each operand's shape via `size(...)` on every
//! output cell inside `_copyto_fastpath_2d_binary!`'s generic loop. This
//! isolates `Vm::run()` from CLI startup, parsing, lowering, and bytecode
//! compilation. Run with:
//!
//!     cargo bench -p subset_julia_vm --bench vm_2d_outer_broadcast_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const SOURCE: &str = include_str!("../../benchmarks/vm_2d_outer_broadcast.jl");
const EXPECTED_OUTPUT: &str = "3.015e6 + 2.265e6im\n";

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

fn bench_vm_2d_outer_broadcast(c: &mut Criterion) {
    let compiled = compile_source();
    validate(&compiled);

    let mut group = c.benchmark_group("vm_2d_outer_broadcast");

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

criterion_group!(benches, bench_vm_2d_outer_broadcast);
criterion_main!(benches);
