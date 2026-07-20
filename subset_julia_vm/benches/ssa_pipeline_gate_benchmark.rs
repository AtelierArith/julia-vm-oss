//! SSA pipeline gate wall-time benchmarks (Issue #8440).
//!
//! Times `Vm::run()` on two precompiled programs per workload — one compiled
//! with `SJULIA_SSA_PIPELINE=0` (legacy `CoreCompiler` bodies; explicit
//! opt-out since the default flip, Issue #8832) and one compiled with the
//! gate on (eligible user bodies through
//! Core IR → SSA build → opt passes → bytecode lowering, Issue #8552). The
//! gate is a **compile-time** switch: the VM is identical, so the measured
//! difference is purely the emitted bytecode. Parsing, lowering, and
//! compilation are excluded; the `Vm` is constructed inside `iter_batched`
//! setup so only execution is timed.
//!
//! Workloads:
//!
//! * `cse_branch_dominated` — a loop over a function whose branch arms repeat
//!   a dominating pure user call. SSA pure-call CSE with the body-derived
//!   effect summaries (Issue #8441 wiring) merges the arm calls into the
//!   dominating one (3 calls → 1 call per invocation); the legacy user-scope
//!   CSE is straight-line only and keeps all three. This is the "SSA path
//!   beats legacy" case of the Issue #8440 acceptance criteria.
//! * `calc_pi_loop_carried` — the three-variable `while` loop that exposed
//!   the loop-phi spill gap (0.18 s → 0.60 s in the first #8552 slice).
//!   Phi-copy coalescing restores the legacy store shape, so gate-on must
//!   sit at parity here (go/no-go flip criterion 3 in `docs/vm/SSA_IR.md`).
//! * `union_isa_elision` / `typeof_guard_specialization` — the Issue #5077
//!   branch-type-narrowing shapes whose bytecode assertions were gated to
//!   the legacy path when the default flipped (Issue #8832). They measure
//!   the runtime cost of the SSA pipeline's missing branch-type propagation
//!   (Issue #9085); `union_isa_elision` SSA-on parity with legacy is the
//!   acceptance measurement for that issue.
//!
//! Follow the repository machine-quiet protocol: run this bench alone, never
//! concurrently with builds or tests.
//!
//! Run with: cargo bench -p subset_julia_vm --bench ssa_pipeline_gate_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use std::time::Duration;
use subset_julia_vm::compile::host_support::{clear_compile_cache, compile_with_cache};
use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

const GATE_ENV: &str = "SJULIA_SSA_PIPELINE";

/// Branch-dominated repeated pure call. Upstream Julia: 5.333373335199996e15.
///
/// All parameters are typed so no function is registered for runtime
/// specialization (which the SSA path declines, see `ssa_ir/lower.rs`). The
/// callee body is binary-operator-only on purpose: a callee calling a
/// multi-method Base name (`sqrt`, or an n-ary `*` chain, which lowers to a
/// call of Base `*`) inherits that name's conservative merged summary and
/// stays out of CSE.
const CSE_BRANCH_SRC: &str = r#"
mydist(a::Float64, b::Float64) = a * a + b * b

function branchy(a::Float64, b::Float64)
    base = mydist(a, b)
    if a > b
        r = mydist(a, b) + 1.0
    else
        r = mydist(a, b) - 1.0
    end
    base + r
end

function drive(n::Int64)
    acc = 0.0
    k = 1
    while k <= n
        acc += branchy(1.0 * k, 2.0)
        k += 1
    end
    acc
end

println(drive(200000))
"#;

/// Loop-carried phi workload (Leibniz calc_pi). Upstream Julia:
/// 3.1415916535897743.
const CALC_PI_SRC: &str = r#"
function calc_pi(n::Int64)
    s = 0.0
    sign = 1.0
    k = 1
    while k <= n
        s += sign / (2.0 * k - 1.0)
        sign = -sign
        k += 1
    end
    4.0 * s
end

println(calc_pi(1000000))
"#;

/// Branch-narrowed `isa` elision workload (Issue #5077 / #9085). The legacy
/// path constant-folds the inner `x isa Int64` re-check to `PushBool(true)`;
/// the SSA path does not yet implement branch-type propagation, so with the
/// default flip (Issue #8832) the elision assertions were gated to
/// `SJULIA_SSA_PIPELINE=0`. This workload measures the runtime cost of that
/// gap. Upstream Julia: 2000000.
const UNION_ISA_SRC: &str = r#"
function check_isa(x::Union{Int64,String})
    if x isa Int64
        return x isa Int64
    else
        return x isa String
    end
end

function drive_isa(n::Int64)
    acc = 0
    k = 1
    while k <= n
        if check_isa(k)
            acc += 1
        end
        k += 1
    end
    acc
end

println(drive_isa(2000000))
"#;

/// Typeof-guard arithmetic-specialization workload (Issue #5077 / #9085). The
/// legacy path narrows `x` to Int64 inside the guard and emits `AddI64`; the
/// SSA path currently keeps dynamic dispatch. Upstream Julia: 2000003000000.
const TYPEOF_GUARD_SRC: &str = r#"
function add_one(x::Union{Int64,String})
    if typeof(x) === Int64
        return x + 1
    else
        return length(x)
    end
end

function drive_typeof(n::Int64)
    acc = 0
    k = 1
    while k <= n
        acc += add_one(k)
        k += 1
    end
    acc
end

println(drive_typeof(2000000))
"#;

/// Compile `src` with the SSA pipeline gate set as requested. The gate env
/// var is read once per program compile, so both variants coexist safely in
/// one process; the persistent compile cache is cleared between compiles so
/// each one actually re-emits user bytecode.
fn compile_with_gate(src: &str, gate_on: bool) -> CompiledProgram {
    if gate_on {
        std::env::set_var(GATE_ENV, "1");
    } else {
        // Since the default flip (Issue #8832) an unset env var means SSA ON;
        // the legacy arm must opt out explicitly with "0".
        std::env::set_var(GATE_ENV, "0");
    }
    clear_compile_cache();
    let program = parse_and_lower_with_base_dir(src, None)
        .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
    let compiled = compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"));
    std::env::remove_var(GATE_ENV);
    compiled
}

fn validate(compiled: &CompiledProgram, expected_output: &str) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
    assert_eq!(vm.get_output(), expected_output);
}

fn bench_ssa_pipeline_gate(c: &mut Criterion) {
    let cases: [(&str, &str, &str); 4] = [
        (
            "cse_branch_dominated",
            CSE_BRANCH_SRC,
            "5.333373335199996e15\n",
        ),
        ("calc_pi_loop_carried", CALC_PI_SRC, "3.1415916535897743\n"),
        ("union_isa_elision", UNION_ISA_SRC, "2000000\n"),
        (
            "typeof_guard_specialization",
            TYPEOF_GUARD_SRC,
            "2000003000000\n",
        ),
    ];

    let mut group = c.benchmark_group("ssa_pipeline_gate");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(2));

    for (name, src, expected_output) in cases {
        let legacy = compile_with_gate(src, false);
        let gated = compile_with_gate(src, true);

        // Output parity between the two compiles before timing anything.
        validate(&legacy, expected_output);
        validate(&gated, expected_output);

        for (variant, compiled) in [("legacy", &legacy), ("ssa_pipeline", &gated)] {
            group.bench_function(format!("{name}/{variant}"), |b| {
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
        }
    }

    group.finish();
}

criterion_group!(benches, bench_ssa_pipeline_gate);
criterion_main!(benches);
