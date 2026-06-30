//! VM benchmarks using Criterion
//!
//! Run with: cargo bench

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile_and_run_str;

/// Benchmark: Fibonacci calculation
fn bench_fibonacci(c: &mut Criterion) {
    let source = r#"
function fib(n)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end
fib(20)
"#;

    c.bench_function("fib_20", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: Array sum
fn bench_array_sum(c: &mut Criterion) {
    let source = r#"
arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
sum(arr)
"#;

    c.bench_function("array_sum_10", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: Simple arithmetic
fn bench_arithmetic(c: &mut Criterion) {
    let source = "1 + 2 * 3 - 4 / 2";

    c.bench_function("simple_arithmetic", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: For loop
fn bench_for_loop(c: &mut Criterion) {
    let source = r#"
total = 0.0
for i in 1:100
    total = total + i
end
total
"#;

    c.bench_function("for_loop_100", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: filter-free comprehension (Issue #5186)
///
/// `[f(x) for x in 1:n]` has a final length equal to `length(iter)`, so the
/// compiler reserves the backing storage up front (`ReserveArray`) instead of
/// growing it with O(log n) reallocations as each element is pushed. This
/// benchmark exercises that pre-sized growth path over a moderately large
/// range so regressions in the reserve hint are visible.
fn bench_comprehension_filter_free(c: &mut Criterion) {
    let source = r#"
[2 * x + 1 for x in 1:1000]
"#;

    c.bench_function("comprehension_filter_free_1000", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

criterion_group!(
    benches,
    bench_fibonacci,
    bench_array_sum,
    bench_arithmetic,
    bench_for_loop,
    bench_comprehension_filter_free
);
criterion_main!(benches);
