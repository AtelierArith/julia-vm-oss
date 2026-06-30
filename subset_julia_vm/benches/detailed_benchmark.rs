//! Detailed benchmarks with phase separation
//!
//! This benchmark separates the compilation pipeline phases:
//! 1. Parse + Lowering (source -> Core IR)
//! 2. Compilation (Core IR -> Bytecode)
//! 3. VM Execution (Bytecode -> Result)
//!
//! Run with: cargo bench --bench detailed_benchmark

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm::{compile::compile_with_cache, compile_and_run_str};

/// Benchmark: Parse + Lower only (source -> Core IR)
fn bench_parse_lower(c: &mut Criterion) {
    let sources = vec![
        ("simple_arithmetic", "1 + 2 * 3 - 4 / 2"),
        (
            "for_loop",
            r#"
total = 0.0
for i in 1:100
    total = total + i
end
total
"#,
        ),
        (
            "fib_recursive",
            r#"
function fib(n)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end
fib(10)
"#,
        ),
    ];

    let mut group = c.benchmark_group("parse_lower");
    for (name, source) in sources {
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, &src| {
            b.iter(|| {
                let mut parser = Parser::new().unwrap();
                let outcome = parser.parse(black_box(src)).unwrap();
                let mut lowering = Lowering::new(src);
                lowering.lower(outcome).unwrap()
            });
        });
    }
    group.finish();
}

/// Benchmark: Compile only (Core IR -> Bytecode)
/// Pre-parsed IR is reused across iterations
fn bench_compile(c: &mut Criterion) {
    let sources = vec![
        ("simple_arithmetic", "1 + 2 * 3 - 4 / 2"),
        (
            "for_loop",
            r#"
total = 0.0
for i in 1:100
    total = total + i
end
total
"#,
        ),
    ];

    let mut group = c.benchmark_group("compile");
    for (name, source) in sources {
        // Pre-parse once
        let mut parser = Parser::new().unwrap();
        let outcome = parser.parse(source).unwrap();
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(outcome).unwrap();

        group.bench_with_input(BenchmarkId::from_parameter(name), &program, |b, prog| {
            b.iter(|| compile_with_cache(black_box(prog)).unwrap());
        });
    }
    group.finish();
}

/// Benchmark: VM execution only (Bytecode -> Result)
/// Pre-compiled bytecode is reused across iterations
fn bench_vm_execution(c: &mut Criterion) {
    let sources = vec![
        ("simple_arithmetic", "1 + 2 * 3 - 4 / 2"),
        (
            "for_loop",
            r#"
total = 0.0
for i in 1:100
    total = total + i
end
total
"#,
        ),
        (
            "fib_10",
            r#"
function fib(n)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end
fib(10)
"#,
        ),
    ];

    let mut group = c.benchmark_group("vm_execution");
    for (name, source) in sources {
        // Pre-compile once
        let mut parser = Parser::new().unwrap();
        let outcome = parser.parse(source).unwrap();
        let mut lowering = Lowering::new(source);
        let program = lowering.lower(outcome).unwrap();
        let compiled = compile_with_cache(&program).unwrap();

        group.bench_with_input(BenchmarkId::from_parameter(name), &compiled, |b, comp| {
            b.iter(|| {
                let rng = StableRng::new(0);
                let mut vm = Vm::new_program(black_box(comp.clone()), rng);
                vm.run().unwrap()
            });
        });
    }
    group.finish();
}

/// Benchmark: Full pipeline (baseline for comparison)
fn bench_full_pipeline(c: &mut Criterion) {
    let sources = vec![
        ("simple_arithmetic", "1 + 2 * 3 - 4 / 2"),
        (
            "for_loop_100",
            r#"
total = 0.0
for i in 1:100
    total = total + i
end
total
"#,
        ),
        (
            "fib_10",
            r#"
function fib(n)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end
fib(10)
"#,
        ),
    ];

    let mut group = c.benchmark_group("full_pipeline");
    for (name, source) in sources {
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, &src| {
            b.iter(|| compile_and_run_str(black_box(src), 0));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_parse_lower,
    bench_compile,
    bench_vm_execution,
    bench_full_pipeline
);
criterion_main!(benches);
