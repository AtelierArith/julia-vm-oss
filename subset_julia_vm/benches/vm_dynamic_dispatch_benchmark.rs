//! Precomputed-bytecode VM benchmark for the per-dynamic-call frame-setup cost
//! (Issue #6853).
//!
//! The driver (`benchmarks/vm_dynamic_dispatch.jl`) calls two untyped-parameter
//! functions (`norm2`, `sinc_approx`) ~2 times per point over 10000 points, so
//! every call resolves through the runtime dynamic-dispatch path. Before #6853
//! that path cloned the whole selected `FunctionInfo` (many `Vec`/`String`
//! fields) on every call to release the `self.functions[idx]` borrow before
//! frame setup. Switching `Vm.functions` to `Vec<Rc<FunctionInfo>>` turns that
//! per-call clone into an O(1) refcount bump; this benchmark isolates
//! `Vm::run()` so the win is measurable directly.
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_dynamic_dispatch_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, Vm};

const DYNAMIC_DISPATCH_SOURCE: &str = include_str!("../../benchmarks/vm_dynamic_dispatch.jl");
const EXPECTED_OUTPUT: &str = "335.7282850538752\n";

fn compile_dynamic_dispatch() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(DYNAMIC_DISPATCH_SOURCE).unwrap();
    let mut lowering = Lowering::new(DYNAMIC_DISPATCH_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_dynamic_dispatch(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_dynamic_dispatch(c: &mut Criterion) {
    let compiled = compile_dynamic_dispatch();
    validate_dynamic_dispatch(&compiled);

    let mut group = c.benchmark_group("vm_dynamic_dispatch");

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

criterion_group!(benches, bench_vm_dynamic_dispatch);
criterion_main!(benches);
