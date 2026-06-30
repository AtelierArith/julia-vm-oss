//! Precomputed-bytecode VM benchmark for direct calls to a `where`-parametric
//! method (Issue #6868).
//!
//! Isolates `Vm::run()` from CLI startup, parsing, lowering, and bytecode
//! compilation so the direct-call specialization change can be measured
//! directly. The driver (`benchmarks/vm_where_specialization.jl`) calls
//! `sinc_w(x::T) where T<:Real` in a hot nested loop with a concrete `Float64`
//! argument.
//!
//! Before #6868 the direct-call path (`execute_direct_call_with_func_args`)
//! jumped to the method's unspecialized generic body, binding every parameter
//! to `Any` and dynamically dispatching the inner `==`/`*`/`sin`/`/` operators —
//! making the `where`-parametric form slower than both the untyped-generic and
//! the concrete-typed forms. The fix specializes the body for the concrete
//! runtime argument types (cached) on the direct-call path.
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_where_specialization_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, Vm};

const WHERE_SPECIALIZATION_SOURCE: &str =
    include_str!("../../benchmarks/vm_where_specialization.jl");
const EXPECTED_OUTPUT: &str = "19889.019616542755\n";

fn compile_where_specialization() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(WHERE_SPECIALIZATION_SOURCE).unwrap();
    let mut lowering = Lowering::new(WHERE_SPECIALIZATION_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_where_specialization(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_where_specialization(c: &mut Criterion) {
    let compiled = compile_where_specialization();
    validate_where_specialization(&compiled);

    let mut group = c.benchmark_group("vm_where_specialization");

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

criterion_group!(benches, bench_vm_where_specialization);
criterion_main!(benches);
