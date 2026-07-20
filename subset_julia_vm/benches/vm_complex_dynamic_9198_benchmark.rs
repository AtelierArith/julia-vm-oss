//! Complex{Float64} dynamic-dispatch A/B benchmark for the #9125/#9154 Rust
//! fast-path retirement (Issue #9198, slice S6).
//!
//! By S5 the typed-loop Complex path is fully SROA'd (S2/S3) and Complex arrays
//! are contiguous (S4/S5), so the `mandelbrot_complex` kernel below never
//! reaches the Rust fast paths — it is the **regression guard** proving their
//! removal does not perturb the SROA'd path. The other three cases exercise the
//! *residual* dynamic Complex route where the fast paths still fire (confirmed
//! via `SJULIA_VM_PROFILE`):
//!
//! | case               | fast-path fired (with) | route                          |
//! |--------------------|------------------------|--------------------------------|
//! | `dyn_binary`       | 1 `BinaryBothComplexF64FastHit`/iter | `s = s + z*2.0 + 1.0` (non-SROA'd) |
//! | `array_sum`        | (len-1) hits per `sum` | `sum(::Vector{ComplexF64})` reduction |
//! | `dyn_pow`          | `DynamicPow` `try_complex_f64_int_pow`/iter | materialized `z^3` |
//! | `mandelbrot` guard | 0 (SROA'd)             | typed `z = z*z + c`            |
//!
//! Each case runs two interleaved arms via [`set_complex_fastpath_disabled`]:
//! `/with_fastpath` (shipping) and `/without_fastpath` (fall through to the
//! general Julia method resolver). Output parity between the arms is asserted
//! before timing, so a behavioural divergence fails the bench rather than
//! silently skewing the numbers (the byte-identical retirement invariant). This
//! is the acceptance-criterion-4 evidence, gathered per the Performance Decision
//! Protocol (CHECKLISTS.md).
//!
//! Run: `cargo bench -p subset_julia_vm --bench vm_complex_dynamic_9198_benchmark`

use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{set_complex_fastpath_disabled, Vm};
use subset_julia_vm_bytecode::CompiledProgram;

const MANDELBROT_SRC: &str = include_str!("../../benchmarks/vm_complex_arith.jl");
const DYN_BINARY_SRC: &str = include_str!("../../benchmarks/vm_complex_dynamic_binary.jl");
const ARRAY_SUM_SRC: &str = include_str!("../../benchmarks/vm_complex_dynamic_array_sum.jl");
const DYN_POW_SRC: &str = include_str!("../../benchmarks/vm_complex_dynamic_pow.jl");

fn compile(src: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(src).unwrap();
    let mut lowering = Lowering::new(src);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn run_output(compiled: &CompiledProgram) -> String {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap();
    vm.get_output().to_string()
}

fn bench_complex_dynamic(c: &mut Criterion) {
    // (name, source, expected stdout)
    let cases: [(&str, &str, &str); 4] = [
        ("dyn_binary", DYN_BINARY_SRC, "600000.0\n"),
        ("array_sum", ARRAY_SUM_SRC, "2.002e8\n"),
        ("dyn_pow", DYN_POW_SRC, "200598.19780099962\n"),
        // Regression guard: SROA'd typed loop, fast paths not reached — the two
        // arms must be within noise (removal does not perturb the SROA'd path).
        ("mandelbrot_guard", MANDELBROT_SRC, "5519\n"),
    ];

    let mut group = c.benchmark_group("vm_complex_dynamic");
    group.sample_size(60);
    group.measurement_time(Duration::from_secs(8));
    group.warm_up_time(Duration::from_secs(2));

    for (name, src, expected) in cases {
        let compiled = compile(src);

        // Parity + arms-differ safety: both arms must produce the expected
        // output before either is timed. If `without_fastpath` diverged
        // (behavioural regression) this panics instead of skewing the numbers.
        set_complex_fastpath_disabled(false);
        assert_eq!(
            run_output(&compiled),
            expected,
            "{name} with_fastpath output"
        );
        set_complex_fastpath_disabled(true);
        assert_eq!(
            run_output(&compiled),
            expected,
            "{name} without_fastpath output (retirement must stay byte-identical)"
        );
        set_complex_fastpath_disabled(false);

        group.bench_function(format!("{name}/with_fastpath"), |b| {
            set_complex_fastpath_disabled(false);
            b.iter_batched(
                || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                |mut vm| {
                    let result = vm.run().unwrap();
                    black_box(&result);
                    vm
                },
                BatchSize::PerIteration,
            );
        });

        group.bench_function(format!("{name}/without_fastpath"), |b| {
            set_complex_fastpath_disabled(true);
            b.iter_batched(
                || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                |mut vm| {
                    let result = vm.run().unwrap();
                    black_box(&result);
                    vm
                },
                BatchSize::PerIteration,
            );
            set_complex_fastpath_disabled(false);
        });
    }

    group.finish();
    set_complex_fastpath_disabled(false);
}

criterion_group!(benches, bench_complex_dynamic);
criterion_main!(benches);
