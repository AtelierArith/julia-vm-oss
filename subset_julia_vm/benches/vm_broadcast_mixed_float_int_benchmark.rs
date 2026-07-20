//! VM-only benchmark for Float64-array × Int-scalar broadcasting (Issue #7587).
//!
//! `x .^ 2` (Int exponent) on a `Vector{Float64}` used to be ~8-14x slower than
//! `x .^ 2.0` (Float exponent) because the per-element mixed-type call
//! `^(::Float64, ::Int)` fell through to the generic `^(::Number, ::Number)`
//! promote() fallback. Concrete mixed Float/Int methods (base/float.jl) close the
//! gap. This benchmark isolates `Vm::run()` from CLI/parse/lower/compile and pairs
//! each Int-scalar form with its Float-scalar twin so a regression is visible as a
//! widening run_only gap.
//!
//!     cargo bench -p subset_julia_vm --bench vm_broadcast_mixed_float_int_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

const N: i64 = 629;

// Each source builds a Float64 vector once, then runs the broadcast `iters`
// times, returning the running sum of the first element so the result is a
// scalar Float64 the harness can validate.
fn make_source(expr: &str) -> String {
    format!(
        r#"
function loop_bcast()::Float64
    x = collect(-3.14:0.01:3.14)
    acc = 0.0
    i = 0
    while i < 200
        y = {expr}
        acc = acc + y[1]
        i = i + 1
    end
    acc
end

loop_bcast()
"#
    )
}

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn run_value(compiled: &CompiledProgram) -> f64 {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    match vm.run().unwrap() {
        Value::F64(actual) => actual,
        other => panic!("expected Float64 result, got {other:?}"),
    }
}

fn bench_vm_broadcast_mixed(c: &mut Criterion) {
    // Pair Int-scalar forms with their Float-scalar twins. The two members of a
    // pair must produce identical values; assert that to guard correctness.
    let pairs = [
        ("pow_int", "x .^ 2", "pow_float", "x .^ 2.0"),
        ("add_int", "x .+ 2", "add_float", "x .+ 2.0"),
        ("mul_int", "x .* 2", "mul_float", "x .* 2.0"),
    ];

    let mut compiled = Vec::new();
    for (int_name, int_expr, flt_name, flt_expr) in pairs {
        let int_prog = compile_source(&make_source(int_expr));
        let flt_prog = compile_source(&make_source(flt_expr));
        let int_val = run_value(&int_prog);
        let flt_val = run_value(&flt_prog);
        assert!(
            (int_val - flt_val).abs() <= 1.0e-12,
            "{int_name} vs {flt_name}: {int_val} != {flt_val}"
        );
        compiled.push((int_name, int_prog));
        compiled.push((flt_name, flt_prog));
    }

    let mut group = c.benchmark_group("vm_broadcast_mixed_float_int");
    group.throughput(criterion::Throughput::Elements((N * 200) as u64));

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

criterion_group!(benches, bench_vm_broadcast_mixed);
criterion_main!(benches);
