//! Precomputed-bytecode VM benchmark for generic Value-move bandwidth (Issue #8650 / #8676).
//!
//! This is the GAIN-SIDE harness for the I128/U128 boxing decision (Issue #8650 / #8676).
//! Shrinking `Value` from 64→56 bytes (by boxing I128/U128 to drop the 16-byte alignment)
//! reduces every stack push, slot store, Vec push, and argument-copy cost by 12.5%.
//! This benchmark isolates `Vm::run()` for a push!/pop! + copy workload on `Vector{Int64}`.
//!
//! The Int64 element type is intentional: this is the "generic move" path that benefits
//! from a smaller Value enum irrespective of whether I128/U128 is boxing or inline.
//! Comparing this benchmark's time before vs. after boxing shows the headline gain.
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_value_move_push_pop_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const VALUE_MOVE_SOURCE: &str = include_str!("../../benchmarks/vm_value_move_push_pop.jl");
const EXPECTED_OUTPUT: &str = "41280\n82560\n";

fn compile_value_move() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(VALUE_MOVE_SOURCE).unwrap();
    let mut lowering = Lowering::new(VALUE_MOVE_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_value_move(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
}

fn bench_vm_value_move_push_pop(c: &mut Criterion) {
    let compiled = compile_value_move();
    validate_value_move(&compiled);

    let mut group = c.benchmark_group("vm_value_move_push_pop");
    group.sample_size(50);

    // run_only: isolates Vm::run() so the 64→56B Value enum size change
    // is visible as a direct throughput delta.
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

criterion_group!(benches, bench_vm_value_move_push_pop);
criterion_main!(benches);
