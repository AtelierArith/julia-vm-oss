//! Handler-table dispatch vs `match` dispatch measurement harness
//! (Issue #8562; reuses the Issue #8559 harness pattern for comparability).
//!
//! Runs the #8559 benchmark set — `fib(25)` recursion, `calc_pi(1_000_000)`
//! while loop, `lorenz_accum(1_000_000)` attractor loop — plus
//! `calc_pi_call(1_000_000)` (a per-iteration user-function call keeps the
//! loop on the per-instruction interpreter instead of an executable block),
//! once per dispatch mechanism (gate off = production `match`, gate on =
//! function-pointer handler table) and prints:
//!
//! - deterministic dynamic counters (load-independent): interpreter
//!   dispatches + executable-block runs (identical on both paths by
//!   construction — asserted), and the table path's hot-row hits vs
//!   fallback dispatches (= hot-subset coverage)
//! - wall times over `reps` fresh-`Vm` runs of the precompiled program
//!   (VM execution only; parsing/lowering/compilation excluded), with
//!   metrics collection disabled so counters do not perturb timing
//!
//! The same binary runs on the macOS host and on the iOS Simulator
//! (`cargo build --release -p subset_julia_vm --bin handler_table_bench_8562
//! --features vm-handler-table --target <iOS-simulator-target>`, then
//! `xcrun simctl spawn <device> <bin>`). The Wasm counterpart is
//! `scripts/handler_table_wasm_bench_8562.mjs` driving
//! `subset_julia_vm_web::HandlerTableBench`.
//!
//! Usage: `handler_table_bench_8562 [reps]` (default 7 — medians of 7).
//! Follow the repository machine-quiet benchmark protocol for the wall-time
//! columns; the counter columns are deterministic and unaffected by load.

use std::time::Instant;

use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{
    set_handler_table_forced, set_stack_vm_metrics_forced, StackVmMetrics, Vm,
};
use subset_julia_vm_bytecode::CompiledProgram;

/// Recursive benchmark (`fib`-class, Issue #8448 target list). Upstream
/// Julia: 75025.
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

/// Loop with a per-iteration user-function call (Issue #8562 addition): the
/// call keeps the loop on the per-instruction interpreter — the production
/// executable-block fast path swallows the pure `calc_pi`/`lorenz` loops
/// into single native blocks, leaving almost no dispatches for the two
/// mechanisms to differ on. Upstream Julia: 3.1415916535897743.
const CALC_PI_CALL_SRC: &str = r#"
function pi_term(k::Int64)
    sign = 1.0 - 2.0 * (k % 2)
    return sign / (2.0 * k + 1.0)
end

function calc_pi_call(n::Int64)
    acc = 0.0
    k = 0
    while k < n
        acc = acc + pi_term(k)
        k = k + 1
    end
    return 4.0 * acc
end

println(calc_pi_call(1000000))
"#;

/// Attractor-style Float64 loop (Lorenz step + accumulator; slot-heavy F64
/// arithmetic). Upstream Julia: -11779.830551874697.
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

struct Bench {
    name: &'static str,
    src: &'static str,
    expected_output: &'static str,
}

const BENCHES: [Bench; 4] = [
    Bench {
        name: "fib(25)",
        src: FIB_SRC,
        expected_output: "75025\n",
    },
    Bench {
        name: "calc_pi(1_000_000)",
        src: CALC_PI_SRC,
        expected_output: "3.1415916535897743\n",
    },
    Bench {
        name: "calc_pi_call(1_000_000)",
        src: CALC_PI_CALL_SRC,
        expected_output: "3.1415916535897743\n",
    },
    Bench {
        name: "lorenz_accum(1_000_000)",
        src: LORENZ_SRC,
        expected_output: "-11779.830551874697\n",
    },
];

fn compile(src: &str) -> CompiledProgram {
    let program = parse_and_lower_with_base_dir(src, None)
        .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
    compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"))
}

struct CounterRun {
    output: String,
    stack: StackVmMetrics,
    table_hits: u64,
    table_fallbacks: u64,
}

/// One instrumented run with the handler-table gate set to `table`.
fn counter_run(compiled: &CompiledProgram, table: bool) -> CounterRun {
    set_handler_table_forced(table);
    set_stack_vm_metrics_forced(true);
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    set_stack_vm_metrics_forced(false);
    set_handler_table_forced(false);
    vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
    let (table_hits, table_fallbacks) = vm.handler_table_metrics().unwrap_or((0, 0));
    CounterRun {
        output: vm.get_output().to_string(),
        stack: vm.stack_vm_metrics().unwrap_or_else(|| {
            eprintln!("internal error: stack VM metrics were forced on but not collected");
            std::process::exit(1);
        }),
        table_hits,
        table_fallbacks,
    }
}

/// `reps` uninstrumented wall-time runs of the precompiled program
/// (fresh `Vm` per run, timing `Vm::run` only). Returns times in ms.
fn wall_runs(compiled: &CompiledProgram, reps: usize, table: bool) -> Vec<f64> {
    set_handler_table_forced(table);
    let samples = (0..reps)
        .map(|_| {
            let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
            let start = Instant::now();
            vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    set_handler_table_forced(false);
    samples
}

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(|a, b| a.total_cmp(b));
    let mid = samples.len() / 2;
    if samples.len().is_multiple_of(2) {
        (samples[mid - 1] + samples[mid]) / 2.0
    } else {
        samples[mid]
    }
}

fn main() {
    let reps: usize = std::env::args()
        .nth(1)
        .map(|arg| {
            arg.parse().unwrap_or_else(|e| {
                eprintln!("invalid reps '{arg}': {e}");
                std::process::exit(2);
            })
        })
        .unwrap_or(7);
    if reps < 1 {
        eprintln!("invalid reps '{reps}': must be >= 1");
        std::process::exit(2);
    }

    println!(
        "# handler_table_bench_8562 (target: {}, profile: {}, reps: {reps})",
        target_description(),
        profile_description(),
    );

    let mut parity_failed = false;
    for bench in &BENCHES {
        let compiled = compile(bench.src);

        // ---- deterministic counters, both dispatch mechanisms ----
        let off = counter_run(&compiled, false);
        let on = counter_run(&compiled, true);

        if off.output != bench.expected_output || on.output != bench.expected_output {
            parity_failed = true;
            println!(
                "PARITY FAIL: expected {:?}, match {:?}, table {:?}",
                bench.expected_output, off.output, on.output
            );
        }
        // Same instruction stream on both paths: the dispatch/block counters
        // must agree exactly; only the dispatch mechanism differs.
        if off.stack.dispatches != on.stack.dispatches
            || off.stack.executable_block_runs != on.stack.executable_block_runs
        {
            parity_failed = true;
            println!(
                "COUNTER MISMATCH: match dispatches={} blocks={} vs table dispatches={} blocks={}",
                off.stack.dispatches,
                off.stack.executable_block_runs,
                on.stack.dispatches,
                on.stack.executable_block_runs
            );
        }

        println!("\n## {}", bench.name);
        println!(
            "counters[match ]: dispatches={} executable_blocks={} \
             operand_stack_high_water={} frames_high_water={}",
            off.stack.dispatches,
            off.stack.executable_block_runs,
            off.stack.operand_stack_high_water,
            off.stack.frames_high_water,
        );
        let total = on.table_hits + on.table_fallbacks;
        println!(
            "counters[table ]: table_hits={} table_fallbacks={} hot_coverage={:.2}%",
            on.table_hits,
            on.table_fallbacks,
            if total == 0 {
                0.0
            } else {
                100.0 * on.table_hits as f64 / total as f64
            },
        );

        // ---- wall times (uninstrumented) ----
        let mut off_ms = wall_runs(&compiled, reps, false);
        let mut on_ms = wall_runs(&compiled, reps, true);
        println!(
            "wall_ms: match median={:.3} min={:.3} | table median={:.3} min={:.3} \
             (samples: match {:?} table {:?})",
            median(&mut off_ms),
            off_ms.first().copied().unwrap_or(f64::NAN),
            median(&mut on_ms),
            on_ms.first().copied().unwrap_or(f64::NAN),
            off_ms
                .iter()
                .map(|ms| (ms * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>(),
            on_ms
                .iter()
                .map(|ms| (ms * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>(),
        );
    }

    if parity_failed {
        println!("\nPARITY FAILURES DETECTED");
        std::process::exit(1);
    }
    println!("\nall benchmarks matched the upstream-Julia-pinned output on both dispatch paths");
}

fn target_description() -> &'static str {
    if cfg!(target_os = "ios") {
        if cfg!(target_abi = "sim") {
            "ios-sim"
        } else {
            "ios"
        }
    } else if cfg!(target_arch = "wasm32") {
        "wasm32"
    } else if cfg!(target_os = "macos") {
        "macos-host"
    } else {
        "other"
    }
}

fn profile_description() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}
