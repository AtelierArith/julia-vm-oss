//! Precomputed-bytecode VM benchmark for mutable-struct field-update loops.
//!
//! Companion to `vm_mandelbrot_benchmark`: it isolates `Vm::run()` from CLI
//! startup, parsing, lowering, and bytecode compilation so VM interpreter and
//! lazy-specialization changes can be measured directly. The driver
//! (`benchmarks/vm_field_update.jl`) repeatedly calls a struct-mutating `step!`
//! function in a hot loop, the pattern that #6346's `FieldAssign` specialization
//! targets.
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_field_update_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const FIELD_UPDATE_SOURCE: &str = include_str!("../../benchmarks/vm_field_update.jl");
const EXPECTED_OUTPUT: &str = "-76010.9082\n";

fn compile_field_update() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(FIELD_UPDATE_SOURCE).unwrap();
    let mut lowering = Lowering::new(FIELD_UPDATE_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_field_update(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_field_update(c: &mut Criterion) {
    let compiled = compile_field_update();
    validate_field_update(&compiled);

    let mut group = c.benchmark_group("vm_field_update");

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

criterion_group!(benches, bench_vm_field_update);
criterion_main!(benches);
