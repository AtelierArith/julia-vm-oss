//! Precomputed-bytecode VM benchmark for immutable-aggregate copy bandwidth.
//!
//! Companion to `vm_field_update_benchmark` (which targets *mutable* struct
//! field writes): this one isolates `Vm::run()` for the *immutable* copy path.
//! Immutable `Struct`/`Tuple` values are deep-cloned (their backing
//! `Vec<Value>` copied element-by-element) on every stack push, slot store, and
//! call-argument pass. The driver (`benchmarks/vm_struct_copy.jl`) shuffles
//! small `Vec3` structs and 3-tuples through function calls and locals in a hot
//! loop, so this is the regression gate for the `Value`-shrink / `Rc`-sharing
//! work in Issue #7966 (e.g. boxing `Regex`, removing `StructInstance.struct_name`,
//! `Rc<StructInstance>`/`Rc<TupleValue>`).
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_struct_copy_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const STRUCT_COPY_SOURCE: &str = include_str!("../../benchmarks/vm_struct_copy.jl");
const EXPECTED_OUTPUT: &str = "400.0\n";

fn compile_struct_copy() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(STRUCT_COPY_SOURCE).unwrap();
    let mut lowering = Lowering::new(STRUCT_COPY_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_struct_copy(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(vm.get_output(), EXPECTED_OUTPUT);
    black_box(result);
}

fn bench_vm_struct_copy(c: &mut Criterion) {
    let compiled = compile_struct_copy();
    validate_struct_copy(&compiled);

    let mut group = c.benchmark_group("vm_struct_copy");

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

criterion_group!(benches, bench_vm_struct_copy);
criterion_main!(benches);
