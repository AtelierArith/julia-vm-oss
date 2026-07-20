//! VM-only benchmark for the same-shape in-place broadcast fast path
//! (Issue #9489; guards the `copyto!` fast paths added for #2807).
//!
//! `out .= a .+ b` on same-shape `Vector{Float64}` / `Vector{Int}` operands
//! must stay within a small constant factor of the equivalent explicit for
//! loop. Before the fast path, the broadcast form fell back to the generic
//! per-element interpret loop and was ~40x slower. These guards used to live
//! as wall-clock ratio `@test`s in
//! `tests/fixtures/broadcast/broadcast_perf_fastpath_regression.jl` and
//! `broadcast_perf_int_fastpath_regression.jl`, but wall-clock ratios in the
//! shared fixture suite are structurally load-sensitive (nextest saturation
//! flaked them once the #9360 @testset gate made fixture @test failures
//! gating). Per repo policy (Issue #3210) the perf guard lives here with
//! Criterion's stable statistical methodology; the fixtures keep the
//! correctness checks only. A regression is visible as a widening
//! broadcast-vs-loop gap within each type pair. This benchmark isolates
//! `Vm::run()` from CLI startup, parsing, lowering, and bytecode compilation.
//!
//!     cargo bench -p subset_julia_vm --bench vm_broadcast_fastpath_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

const N: i64 = 1000;
const ITERS: i64 = 50;

// Each source builds same-shape input vectors once, then runs the add kernel
// `ITERS` times, returning a checksum accumulated from the first and last
// output elements so the harness can validate broadcast/loop agreement.
fn make_source(init_a: &str, init_b: &str, init_out: &str, kernel: &str, zero: &str) -> String {
    format!(
        r#"
function bench_kernel()
    n = {N}
    a = [{init_a} for i in 1:n]
    b = [{init_b} for i in 1:n]
    out = [{init_out} for _ in 1:n]
    acc = {zero}
    j = 0
    while j < {ITERS}
        {kernel}
        acc = acc + out[1] + out[n]
        j = j + 1
    end
    acc
end

bench_kernel()
"#
    )
}

fn compile_source(source: &str) -> CompiledProgram {
    let program = parse_and_lower(source).unwrap();
    compile_with_cache(&program).unwrap()
}

// Value does not implement PartialEq; extract the scalar checksum.
fn run_checksum(compiled: &CompiledProgram) -> f64 {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    match vm.run().unwrap() {
        Value::F64(x) => x,
        Value::I64(x) => x as f64,
        other => panic!("expected numeric checksum, got {other:?}"),
    }
}

struct Case {
    name: &'static str,
    bcast_kernel: &'static str,
    loop_kernel: &'static str,
    init_a: &'static str,
    init_b: &'static str,
    init_out: &'static str,
    zero: &'static str,
}

fn bench_vm_broadcast_fastpath(c: &mut Criterion) {
    let cases = [
        Case {
            name: "f64_add",
            bcast_kernel: "out .= a .+ b",
            loop_kernel: "for i in 1:n\n            out[i] = a[i] + b[i]\n        end",
            init_a: "Float64(i)",
            init_b: "Float64(2 * i)",
            init_out: "0.0",
            zero: "0.0",
        },
        Case {
            name: "int_add",
            bcast_kernel: "out .= a .+ b",
            loop_kernel: "for i in 1:n\n            out[i] = a[i] + b[i]\n        end",
            init_a: "i",
            init_b: "2 * i",
            init_out: "0",
            zero: "0",
        },
    ];

    let mut compiled = Vec::new();
    for case in &cases {
        let bcast_prog = compile_source(&make_source(
            case.init_a,
            case.init_b,
            case.init_out,
            case.bcast_kernel,
            case.zero,
        ));
        let loop_prog = compile_source(&make_source(
            case.init_a,
            case.init_b,
            case.init_out,
            case.loop_kernel,
            case.zero,
        ));
        // The broadcast and loop members of a pair must produce identical
        // checksums; assert that to guard correctness before timing.
        let bcast_val = run_checksum(&bcast_prog);
        let loop_val = run_checksum(&loop_prog);
        assert_eq!(
            bcast_val, loop_val,
            "{}: broadcast checksum != loop checksum",
            case.name
        );
        compiled.push((format!("{}_broadcast", case.name), bcast_prog));
        compiled.push((format!("{}_loop", case.name), loop_prog));
    }

    let mut group = c.benchmark_group("vm_broadcast_fastpath");
    group.throughput(criterion::Throughput::Elements((N * ITERS) as u64));

    for (name, program) in compiled {
        group.bench_function(BenchmarkId::new("run_only", name), |b| {
            b.iter_batched(
                || Vm::new_program(black_box(program.clone()), StableRng::new(0)),
                |mut vm| {
                    let result = vm.run().unwrap();
                    black_box(result);
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, bench_vm_broadcast_fastpath);
criterion_main!(benches);
