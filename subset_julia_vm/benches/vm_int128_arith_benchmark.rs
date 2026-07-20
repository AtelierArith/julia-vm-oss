//! Precomputed-bytecode VM benchmark for Int128/UInt128 arithmetic throughput.
//!
//! This is the LOSS-SIDE harness for the I128/U128 boxing decision (Issue #8650 / #8676).
//! Boxing `Value::I128(i128)` → `Value::I128(Box<i128>)` shrinks the enum from 64→56B,
//! but forces every Int128 construction/read through a heap allocation and pointer
//! dereference.  This benchmark isolates `Vm::run()` for a tight 4-accumulator
//! Int128/UInt128 arithmetic loop so the A/B comparison in #8677 is unconfounded
//! by parse/compile overhead.
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_int128_arith_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const INT128_ARITH_SOURCE: &str = include_str!("../../benchmarks/vm_int128_arith.jl");
// Expected output: two identical 39-digit Int128 values (s and u start symmetric).
const EXPECTED_LINE_COUNT: usize = 2;

fn compile_int128_arith() -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(INT128_ARITH_SOURCE).unwrap();
    let mut lowering = Lowering::new(INT128_ARITH_SOURCE);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn validate_int128_arith(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap();
    let output = vm.get_output();
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(
        lines.len(),
        EXPECTED_LINE_COUNT,
        "expected {} output lines, got {}: {:?}",
        EXPECTED_LINE_COUNT,
        lines.len(),
        output
    );
    // Both print statements should produce non-empty numeric strings.
    for line in &lines {
        assert!(!line.is_empty(), "unexpected empty output line");
    }
}

fn bench_vm_int128_arith(c: &mut Criterion) {
    let compiled = compile_int128_arith();
    validate_int128_arith(&compiled);

    let mut group = c.benchmark_group("vm_int128_arith");
    group.sample_size(50);

    // run_only: measures pure Vm::run() cost — the metric that changes between
    // baseline (inline i128) and prototype (Box<i128>).
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

criterion_group!(benches, bench_vm_int128_arith);
criterion_main!(benches);
