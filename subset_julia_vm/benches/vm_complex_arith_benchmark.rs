//! Complex{Float64} arithmetic VM benchmark (Issue #9125).
//!
//! Measures the hot Complex arithmetic path: in typed functions `z = z*z + c`
//! compiles to direct `Call`s of the Julia Complex methods, whose cost is
//! dominated by `StructInstance` clones (`LoadSlotStruct` / `ReturnStruct`).
//! The Issue #9125 changes attack both sides: `struct_name: Rc<str>` halves
//! per-clone heap allocations, and `try_complex_f64_binary_op` intercepts the
//! `CallDynamicBinaryBoth` route taken by dynamically-typed Complex operands.
//! The Mandelbrot variant runs one `*` and one `+` on Complex{Float64} per
//! inner iteration, ~5.5k inner iterations at (30, 20, 25).
//!
//! Run with: cargo bench -p subset_julia_vm --bench vm_complex_arith_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const COMPLEX_SOURCE: &str = include_str!("../../benchmarks/vm_complex_arith.jl");
const EXPECTED_OUTPUT: &str = "5519\n";

fn compile_complex() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(COMPLEX_SOURCE).unwrap();
    let mut lowering = Lowering::new(COMPLEX_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_complex(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    assert_eq!(
        vm.get_output(),
        EXPECTED_OUTPUT,
        "output mismatch — fast path arithmetic is wrong"
    );
    black_box(result);
}

fn bench_vm_complex_arith(c: &mut Criterion) {
    let compiled = compile_complex();
    validate_complex(&compiled);

    let mut group = c.benchmark_group("vm_complex_arith");

    group.bench_function("mandelbrot_complex_run_only", |b| {
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

criterion_group!(benches, bench_vm_complex_arith);
criterion_main!(benches);
