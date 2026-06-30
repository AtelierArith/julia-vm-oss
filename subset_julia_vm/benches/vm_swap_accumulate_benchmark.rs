//! Precomputed-bytecode VM benchmark for self-referential destructuring swaps
//! whose targets are consumed downstream.
//!
//! Companion to `vm_field_update_benchmark`: it isolates `Vm::run()` from CLI
//! startup, parsing, lowering, and bytecode compilation so VM interpreter and
//! lazy-specialization changes can be measured directly. The driver
//! (`benchmarks/vm_swap_accumulate.jl`) carries loop state through
//! `a, b = b, (a + b) % 1000003` and then accumulates the swapped `a` into `s`.
//! Before #6561 the desugared `temp[k]` reads widened `a` to `Any`, which forced
//! the downstream `s += a` onto a dynamic `DynamicAdd` and poisoned `s`; the
//! tuple-element-type tracking keeps the whole loop typed.
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_swap_accumulate_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, Vm};

const SWAP_ACCUMULATE_SOURCE: &str = include_str!("../../benchmarks/vm_swap_accumulate.jl");
const EXPECTED_OUTPUT: &str = "149905498950\n";

fn compile_swap_accumulate() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(SWAP_ACCUMULATE_SOURCE).unwrap();
    let mut lowering = Lowering::new(SWAP_ACCUMULATE_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_swap_accumulate(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_swap_accumulate(c: &mut Criterion) {
    let compiled = compile_swap_accumulate();
    validate_swap_accumulate(&compiled);

    let mut group = c.benchmark_group("vm_swap_accumulate");

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

criterion_group!(benches, bench_vm_swap_accumulate);
criterion_main!(benches);
