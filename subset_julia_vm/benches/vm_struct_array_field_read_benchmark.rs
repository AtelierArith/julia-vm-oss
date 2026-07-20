//! VM-only benchmark for the array-of-struct field-read hot loop (Issue #9188):
//! `p = c.items[i]; s += p.x` for a struct field declared `Vector{T}`. This is
//! the pattern behind the slow Aizawa attractor sample (#9154): before the
//! fix, `c.items[i]` yielded an `unknown`-typed local, so `.x`/`.y`/`.z`
//! degraded to `GetFieldByName` + `CallDynamicBinaryBoth` instead of typed
//! `GetField` + `AddF64`.
//!
//! This isolates `Vm::run()` from CLI startup, parsing, lowering, and
//! bytecode compilation. Run with:
//!
//!     cargo bench -p subset_julia_vm --bench vm_struct_array_field_read_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const SOURCE: &str = include_str!("../../benchmarks/vm_struct_array_field_read.jl");
const EXPECTED_OUTPUT: &str = "6.0003e10\n";

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

fn bench_vm_struct_array_field_read(c: &mut Criterion) {
    let compiled = compile_source();
    validate(&compiled);

    let mut group = c.benchmark_group("vm_struct_array_field_read");

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

criterion_group!(benches, bench_vm_struct_array_field_read);
criterion_main!(benches);
