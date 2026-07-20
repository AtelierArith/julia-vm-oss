//! Cache hard-cap eviction strategy benchmark (Issue #9097).
//!
//! Measures the throughput cost of the full-clear strategy implemented in
//! #8610 (`enforce_*_cache_limit()`) when the cache cap is set low enough to
//! force frequent clears during a single eval.
//!
//! Decision formula (stated in the issue before running):
//!   - `spike_ratio` = median latency at low_cap / median latency at high_cap
//!   - Implement partial eviction only if spike_ratio > 2.0 AND
//!     clears_per_eval ≥ 2 at a realistic low cap.
//!   - Otherwise close #9097 as measured-rejected.
//!
//! Run with:
//!   cargo bench --bench cache_eviction_benchmark

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use subset_julia_vm::{repl::REPLSession, vm::set_default_cache_entry_limits};

/// A dispatch-heavy workload: many generic function calls over different type
/// combinations, which fills dispatch caches fast. Two struct types + methods
/// + arithmetic keeps the dispatch fan-out broad without needing many source
/// lines.
const DISPATCH_HEAVY: &str = r#"
mutable struct Point9097
    x::Float64
    y::Float64
end

mutable struct Counter9097
    n::Int
end

function norm(p::Point9097)
    sqrt(p.x * p.x + p.y * p.y)
end

function bump!(c::Counter9097, k::Int)
    c.n += k
    c.n
end

function dot(a::Point9097, b::Point9097)
    a.x * b.x + a.y * b.y
end

function scale(p::Point9097, s::Float64)
    Point9097(p.x * s, p.y * s)
end

total = 0.0
c = Counter9097(0)
for i in 1:40
    p = Point9097(Float64(i), Float64(i + 1))
    q = scale(p, 0.5)
    total += norm(p) + dot(p, q)
    bump!(c, i)
end
total + Float64(c.n)
"#;

/// Warmup iterations before timing (to load Base etc.)
const WARMUP: usize = 3;

fn bench_cache_caps(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_eviction_9097");
    // Throughput unit = one full program eval
    group.throughput(Throughput::Elements(1));

    // Caps to test: very low (forces clears), medium, default (4096 = no clears)
    let caps: &[(usize, &str)] = &[
        (32, "cap=32"),
        (128, "cap=128"),
        (512, "cap=512"),
        (4096, "cap=4096_default"),
    ];

    for &(cap, label) in caps {
        // set_default_cache_entry_limits is process-wide; criterion runs each
        // benchmark in the same process. We reset before building the session.
        set_default_cache_entry_limits(Some(cap), Some(cap));

        // Warmup: build the session outside the timing loop so Base prelude
        // cost doesn't dominate the measurement.
        let mut session = REPLSession::new(42);
        for _ in 0..WARMUP {
            let r = session.eval(DISPATCH_HEAVY);
            assert!(
                r.error.is_none(),
                "warmup failed at cap={}: {:?}",
                cap,
                r.error
            );
        }

        group.bench_with_input(BenchmarkId::from_parameter(label), &cap, |b, _| {
            b.iter(|| {
                let r = session.eval(black_box(DISPATCH_HEAVY));
                assert!(r.error.is_none());
                r
            });
        });
    }

    // Reset to defaults so other benches in the same process are not affected.
    set_default_cache_entry_limits(None, None);
    group.finish();
}

criterion_group!(benches, bench_cache_caps);
criterion_main!(benches);
