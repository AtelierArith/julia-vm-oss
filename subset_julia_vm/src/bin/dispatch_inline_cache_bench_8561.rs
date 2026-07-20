//! Dispatch-heavy inline-cache A/B measurement harness (Issue #8561).
//!
//! Runs typed dynamic dispatch (`CallTypedDispatch`) hot loops — a
//! monomorphic site and a mixed `Int64`/`Float64` two-identity site — twice:
//! with the per-call-site inline caches enabled (default) and disabled
//! (`set_call_site_inline_cache_disabled`, the pre-#8561 resolver-every-call
//! baseline), and prints:
//!
//! - deterministic counters (load-independent, the primary evidence):
//!   inline-cache hits/misses at cache-eligible dynamic dispatch sites and
//!   total interpreter dispatches, per engine configuration
//! - wall times over `reps` fresh-`Vm` runs of the precompiled program
//!   (VM execution only), with metrics collection off so the counters do not
//!   perturb timing
//!
//! Usage: `dispatch_inline_cache_bench_8561 [reps]` (default 5). Follow the
//! repository machine-quiet benchmark protocol for the wall-time columns;
//! the counter columns are deterministic and unaffected by load.

use std::time::Instant;

use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower_with_base_dir;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{
    set_call_site_inline_cache_disabled, set_stack_vm_metrics_forced, StackVmMetrics, Vm,
};
use subset_julia_vm_bytecode::CompiledProgram;

/// Monomorphic typed dynamic dispatch in a hot loop (400_000 `g(x)` calls
/// through an `Any[]` element). Upstream Julia: 400000.
const MONO_SRC: &str = r#"
g(x::Int64) = 1
g(x::Float64) = 2
function warm(xs, n)
    s = 0
    k = 0
    while k < n
        for x in xs
            s += g(x)
        end
        k += 1
    end
    s
end
xs = Any[1, 2, 3, 4]
println(warm(xs, 100000))
"#;

/// The same loop over a mixed Int64/Float64 array: the `g(x)` site
/// alternates between two exact scalar identities (two-way slot coverage).
/// Upstream Julia: 600000.
const MIXED_SRC: &str = r#"
g(x::Int64) = 1
g(x::Float64) = 2
function warm(xs, n)
    s = 0
    k = 0
    while k < n
        for x in xs
            s += g(x)
        end
        k += 1
    end
    s
end
xs = Any[1, 2.0, 3, 4.0]
println(warm(xs, 100000))
"#;

/// Issue #9108: monomorphic dispatch on a user-defined struct type.
/// Before #9108 the L1 cache never populated for struct arguments; every
/// dispatch fell to L2. After the fix the hit rate should approach 100%.
/// Upstream Julia: 400000.
const STRUCT_MONO_SRC: &str = r#"
struct MyPoint
    x::Float64
    y::Float64
end
g(p::MyPoint) = 1
g(x::Int64) = 2
function warm(xs, n)
    s = 0
    k = 0
    while k < n
        for x in xs
            s += g(x)
        end
        k += 1
    end
    s
end
xs = Any[MyPoint(1.0, 2.0), MyPoint(3.0, 4.0), MyPoint(5.0, 6.0), MyPoint(7.0, 8.0)]
println(warm(xs, 100000))
"#;

/// Issue #9113: dispatch on a function with 8 arguments via type-unstable
/// `Any[]` arguments so the call site is truly dynamic (not type-resolved at
/// compile time).  Before #9113 `exact_call_site_fingerprint` returned `None`
/// for ≥8 args, permanently disabling L1 at that call site. After the fix the
/// hit rate should approach 100% once the site warms.
/// Upstream Julia: 400000.
const EIGHT_ARG_SRC: &str = r#"
g(a::Int64, b::Int64, c::Int64, d::Int64, e::Int64, f::Int64, h::Int64, i::Int64) = 1
g(a::Float64, b::Int64, c::Int64, d::Int64, e::Int64, f::Int64, h::Int64, i::Int64) = 2
# Pass args through an Any[] so the call is not type-resolved at compile time.
function warm(args, n)
    s = 0
    k = 0
    while k < n
        s += g(args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8])
        s += g(args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8])
        s += g(args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8])
        s += g(args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8])
        k += 1
    end
    s
end
args = Any[Int64(1), Int64(2), Int64(3), Int64(4), Int64(5), Int64(6), Int64(7), Int64(8)]
println(warm(args, 100000))
"#;

struct Bench {
    name: &'static str,
    src: &'static str,
    expected_output: &'static str,
}

const BENCHES: [Bench; 4] = [
    Bench {
        name: "mono  g(x::Any[Int64;4]) x 400k",
        src: MONO_SRC,
        expected_output: "400000\n",
    },
    Bench {
        name: "mixed g(x::Any[Int64|Float64;4]) x 400k",
        src: MIXED_SRC,
        expected_output: "600000\n",
    },
    // Issue #9108: struct dispatch L1 coverage.
    Bench {
        name: "struct g(p::Any[MyPoint;4]) x 400k (Issue #9108)",
        src: STRUCT_MONO_SRC,
        expected_output: "400000\n",
    },
    // Issue #9113: 8-argument dispatch L1 coverage.
    Bench {
        name: "8-arg g(a,b,c,d,e,f,h,i::Int64) x 400k (Issue #9113)",
        src: EIGHT_ARG_SRC,
        expected_output: "400000\n",
    },
];

fn compile(src: &str) -> CompiledProgram {
    let program = parse_and_lower_with_base_dir(src, None)
        .unwrap_or_else(|e| panic!("pipeline error: {e:?}"));
    compile_with_cache(&program).unwrap_or_else(|e| panic!("compile error: {e:?}"))
}

/// One instrumented run: deterministic counters for one cache configuration.
fn counter_run(compiled: &CompiledProgram) -> (String, StackVmMetrics) {
    set_stack_vm_metrics_forced(true);
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    set_stack_vm_metrics_forced(false);
    vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
    let metrics = vm
        .stack_vm_metrics()
        .unwrap_or_else(|| panic!("metrics were forced on"));
    (vm.get_output().to_string(), metrics)
}

/// `reps` uninstrumented wall-time runs of the precompiled program
/// (fresh `Vm` per run, timing `Vm::run` only). Returns times in ms.
fn wall_runs(compiled: &CompiledProgram, reps: usize) -> Vec<f64> {
    (0..reps)
        .map(|_| {
            let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
            let start = Instant::now();
            vm.run().unwrap_or_else(|e| panic!("runtime error: {e}"));
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect()
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
        .unwrap_or(5);
    if reps < 1 {
        eprintln!("invalid reps '{reps}': must be >= 1");
        std::process::exit(2);
    }

    println!(
        "# dispatch_inline_cache_bench_8561 (profile: {}, reps: {reps})",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    );

    let mut parity_failed = false;
    for bench in &BENCHES {
        let compiled = compile(bench.src);
        println!("\n## {}", bench.name);

        // ---- deterministic counters, cache on vs off ----
        set_call_site_inline_cache_disabled(false);
        let (on_output, on) = counter_run(&compiled);
        set_call_site_inline_cache_disabled(true);
        let (off_output, off) = counter_run(&compiled);
        set_call_site_inline_cache_disabled(false);

        if on_output != bench.expected_output || off_output != bench.expected_output {
            parity_failed = true;
            println!(
                "PARITY FAIL: expected {:?}, cache-on {:?}, cache-off {:?}",
                bench.expected_output, on_output, off_output
            );
        }
        let hit_rate = |m: &StackVmMetrics| {
            let total = m.dispatch_inline_cache_hits + m.dispatch_inline_cache_misses;
            if total == 0 {
                0.0
            } else {
                m.dispatch_inline_cache_hits as f64 / total as f64 * 100.0
            }
        };
        println!(
            "counters[cache-on ]: inline_hits={} inline_misses={} hit_rate={:.3}% dispatches={}",
            on.dispatch_inline_cache_hits,
            on.dispatch_inline_cache_misses,
            hit_rate(&on),
            on.dispatches,
        );
        println!(
            "counters[cache-off]: inline_hits={} inline_misses={} hit_rate={:.3}% dispatches={}",
            off.dispatch_inline_cache_hits,
            off.dispatch_inline_cache_misses,
            hit_rate(&off),
            off.dispatches,
        );

        // ---- wall times (uninstrumented) ----
        set_call_site_inline_cache_disabled(false);
        let mut on_ms = wall_runs(&compiled, reps);
        set_call_site_inline_cache_disabled(true);
        let mut off_ms = wall_runs(&compiled, reps);
        set_call_site_inline_cache_disabled(false);
        println!(
            "wall_ms: cache-on median={:.3} min={:.3} | cache-off median={:.3} min={:.3} \
             (samples: on {:?} off {:?})",
            median(&mut on_ms),
            on_ms.first().copied().unwrap_or(f64::NAN),
            median(&mut off_ms),
            off_ms.first().copied().unwrap_or(f64::NAN),
            on_ms
                .iter()
                .map(|ms| (ms * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>(),
            off_ms
                .iter()
                .map(|ms| (ms * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>(),
        );
    }

    if parity_failed {
        println!("\nPARITY FAILURES DETECTED");
        std::process::exit(1);
    }
    println!("\nall benchmarks matched the upstream-Julia-pinned output in both configurations");
}
