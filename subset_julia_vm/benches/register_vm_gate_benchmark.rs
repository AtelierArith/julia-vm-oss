//! Register VM gate wall-time benchmarks (Issue #8559).
//!
//! Times `Vm::run()` on precompiled programs for the Issue #8448 benchmark
//! set — `fib(25)`-class recursion, `calc_pi(1_000_000)`-class loop, and an
//! attractor-style Float64 loop — once with the register VM gate off (the
//! production stack VM, including its executable-block fast path) and once
//! with it on (eligible bodies on the register VM prototype). Parsing,
//! lowering, and compilation are excluded; the `Vm` is constructed inside
//! `iter_batched` setup so only execution is timed.
//!
//! Follow the repository machine-quiet protocol: run this bench alone, never
//! concurrently with builds or tests. Deterministic dispatch/frame counters
//! (load-independent) come from `src/bin/register_vm_bench_8559.rs`, not from
//! this bench.
//!
//! Run with: cargo bench -p subset_julia_vm --bench register_vm_gate_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use std::time::Duration;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{set_register_vm_forced, Vm};
use subset_julia_vm_bytecode::CompiledProgram;

/// Recursive benchmark (`fib`-class). Upstream Julia: 75025.
const FIB_SRC: &str = r#"
function fib(n::Int64)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

println(fib(25))
"#;

/// Loop benchmark (`calc_pi`-class). Upstream Julia: 3.1415916535897743.
const CALC_PI_SRC: &str = r#"
function calc_pi(n::Int64)
    acc = 0.0
    sign = 1.0
    k = 0
    while k < n
        acc = acc + sign / (2.0 * k + 1.0)
        sign = -sign
        k = k + 1
    end
    return 4.0 * acc
end

println(calc_pi(1000000))
"#;

/// Attractor-style Float64 loop (Lorenz step + accumulator). Upstream Julia:
/// -11779.830551874697.
const LORENZ_SRC: &str = r#"
function lorenz_accum(n::Int64)
    x = 1.0
    y = 1.0
    z = 1.0
    dt = 0.001
    acc = 0.0
    k = 0
    while k < n
        dx = 10.0 * (y - x)
        dy = x * (28.0 - z) - y
        dz = x * y - 2.6666666666666665 * z
        x = x + dt * dx
        y = y + dt * dy
        z = z + dt * dz
        acc = acc + x
        k = k + 1
    end
    return acc
end

println(lorenz_accum(1000000))
"#;

fn compile(src: &str) -> CompiledProgram {
    let program = parse_and_lower_with_base_dir(src, None)
        .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
    compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"))
}

fn validate(compiled: &CompiledProgram, expected_output: &str) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
    assert_eq!(vm.get_output(), expected_output);
}

fn bench_register_vm_gate(c: &mut Criterion) {
    let cases: [(&str, &str, &str); 3] = [
        ("fib_25", FIB_SRC, "75025\n"),
        ("calc_pi_1e6", CALC_PI_SRC, "3.1415916535897743\n"),
        ("lorenz_accum_1e6", LORENZ_SRC, "-11779.830551874697\n"),
    ];

    let mut group = c.benchmark_group("register_vm_gate");
    // Each iteration runs hundreds of ms of VM work; keep total bench time
    // bounded (Issue #8559 measurement protocol).
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));
    group.warm_up_time(Duration::from_secs(2));

    for (name, src, expected_output) in cases {
        let compiled = compile(src);

        // Output parity on both engines before timing anything.
        set_register_vm_forced(false);
        validate(&compiled, expected_output);
        set_register_vm_forced(true);
        validate(&compiled, expected_output);
        set_register_vm_forced(false);

        // `PerIteration` keeps exactly one Base-sized `Vm` clone resident at
        // a time, and returning the `Vm` from the routine defers its (large)
        // drop until after the measurement — both materially reduce noise.
        group.bench_function(format!("{name}/stack_vm"), |b| {
            set_register_vm_forced(false);
            b.iter_batched(
                || Vm::new_program(compiled.clone(), StableRng::new(0)),
                |mut vm| {
                    let result = vm.run().unwrap();
                    black_box(&result);
                    vm
                },
                BatchSize::PerIteration,
            );
        });

        group.bench_function(format!("{name}/register_vm"), |b| {
            set_register_vm_forced(true);
            b.iter_batched(
                || Vm::new_program(compiled.clone(), StableRng::new(0)),
                |mut vm| {
                    let result = vm.run().unwrap();
                    black_box(&result);
                    vm
                },
                BatchSize::PerIteration,
            );
            set_register_vm_forced(false);
        });
    }

    group.finish();
    set_register_vm_forced(false);
}

criterion_group!(benches, bench_register_vm_gate);
criterion_main!(benches);
