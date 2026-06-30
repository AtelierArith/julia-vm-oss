//! Precomputed-bytecode VM calc_pi benchmarks.
//!
//! This benchmark intentionally separates `Vm::run()` from CLI startup,
//! parsing, lowering, and bytecode compilation. Use it for VM interpreter
//! changes on gcd-heavy integer loops.
//!
//! Run with: cargo bench -p subset_julia_vm --bench calc_pi_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use std::time::Duration;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, Value, Vm};

const CALC_PI_BENCHMARK_SOURCE: &str = include_str!("../../benchmarks/calc_pi_benchmark.jl");
const FIRST_CLI_BENCH_MARKER: &str = "# Benchmark for N=100";
const BASE_GCD_CALC_PI_SOURCE: &str = r#"
function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if gcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end
"#;
const I64_FUNCTION_CALL_SOURCE: &str = r#"
function advance(a, b)
    while b > 0
        a += 1
        b -= 1
    end
    a
end

function sum_pairs(N)
    total = 0
    step = 2
    for i in 1:N
        total += advance(i, step)
    end
    total
end
"#;
const NESTED_I64_FUNCTION_CALL_SOURCE: &str = r#"
function score6314(x::Int64, y::Int64)
    z = x + y
    return z * y
end

function sum_score6314(n::Int64)
    total = 0
    step = 2
    for i in 1:n
        total += score6314(i, step)
    end
    return total
end
"#;

struct CalcPiCase {
    variant: &'static str,
    n: u64,
    expected: f64,
    compiled: CompiledProgram,
}

struct I64FunctionCase {
    variant: &'static str,
    n: u64,
    expected: i64,
    compiled: CompiledProgram,
}

fn source_for_n(n: u64) -> String {
    let (definitions, _) = CALC_PI_BENCHMARK_SOURCE
        .split_once(FIRST_CLI_BENCH_MARKER)
        .expect("calc_pi benchmark source must keep the CLI benchmark marker");
    format!("{definitions}\ncalc_pi({n})\n")
}

fn base_gcd_source_for_n(n: u64) -> String {
    format!("{BASE_GCD_CALC_PI_SOURCE}\ncalc_pi({n})\n")
}

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn compile_calc_pi(n: u64) -> CompiledProgram {
    compile_source(&source_for_n(n))
}

fn compile_base_gcd_calc_pi(n: u64) -> CompiledProgram {
    compile_source(&base_gcd_source_for_n(n))
}

fn compile_i64_function_case(n: u64) -> CompiledProgram {
    compile_source(&format!("{I64_FUNCTION_CALL_SOURCE}\nsum_pairs({n})\n"))
}

fn compile_nested_i64_function_case(n: u64) -> CompiledProgram {
    compile_source(&format!(
        "{NESTED_I64_FUNCTION_CALL_SOURCE}\nsum_score6314({n})\n"
    ))
}

fn validate_calc_pi(case: &CalcPiCase) {
    let mut vm = Vm::new_program(case.compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    let actual = match result {
        Value::F64(value) => value,
        other => panic!(
            "{} calc_pi({}) returned non-Float64 value: {:?}",
            case.variant, case.n, other
        ),
    };
    assert!(
        (actual - case.expected).abs() < 1.0e-12,
        "{} calc_pi({}) returned {actual}, expected {}",
        case.variant,
        case.n,
        case.expected
    );
    assert_eq!(vm.get_output(), "");
    black_box(actual);
}

fn validate_i64_function(case: &I64FunctionCase) {
    let mut vm = Vm::new_program(case.compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    let actual = match result {
        Value::I64(value) => value,
        other => panic!(
            "sum_pairs({}) returned non-Int64 value: {:?}",
            case.n, other
        ),
    };
    assert_eq!(
        actual, case.expected,
        "sum_pairs({}) returned {actual}, expected {}",
        case.n, case.expected
    );
    assert_eq!(vm.get_output(), "");
    black_box(actual);
}

fn calc_pi_cases() -> Vec<CalcPiCase> {
    let mut cases = Vec::new();
    for (n, expected) in [(100, 3.139597498005517), (500, 3.139019975582346)] {
        cases.push(CalcPiCase {
            variant: "mygcd",
            n,
            expected,
            compiled: compile_calc_pi(n),
        });
        cases.push(CalcPiCase {
            variant: "base_gcd",
            n,
            expected,
            compiled: compile_base_gcd_calc_pi(n),
        });
    }
    cases
}

fn calc_pi_large_cases() -> Vec<CalcPiCase> {
    [(1000, 3.140415340380906)]
        .into_iter()
        .flat_map(|(n, expected)| {
            [
                CalcPiCase {
                    variant: "mygcd",
                    n,
                    expected,
                    compiled: compile_calc_pi(n),
                },
                CalcPiCase {
                    variant: "base_gcd",
                    n,
                    expected,
                    compiled: compile_base_gcd_calc_pi(n),
                },
            ]
        })
        .collect()
}

fn i64_function_cases() -> Vec<I64FunctionCase> {
    [
        (
            "advance",
            20_000,
            200_050_000_i64,
            compile_i64_function_case as fn(u64) -> CompiledProgram,
        ),
        (
            "nested_resolved_helper",
            20_000,
            400_100_000_i64,
            compile_nested_i64_function_case as fn(u64) -> CompiledProgram,
        ),
    ]
    .into_iter()
    .map(|(variant, n, expected, compile)| I64FunctionCase {
        variant,
        n,
        expected,
        compiled: compile(n),
    })
    .collect()
}

fn bench_calc_pi_vm(c: &mut Criterion) {
    let cases = calc_pi_cases();
    for case in &cases {
        validate_calc_pi(case);
    }

    let mut group = c.benchmark_group("vm_calc_pi");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);

    for case in &cases {
        let run_only_id = if case.variant == "mygcd" {
            "run_only".to_string()
        } else {
            format!("{}_run_only", case.variant)
        };
        group.bench_with_input(
            BenchmarkId::new(run_only_id, case.n),
            &case.compiled,
            |b, compiled| {
                b.iter_batched(
                    || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                    |mut vm| {
                        let result = vm.run().unwrap();
                        black_box(result);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        let clone_run_id = if case.variant == "mygcd" {
            "clone_new_program_run".to_string()
        } else {
            format!("{}_clone_new_program_run", case.variant)
        };
        group.bench_with_input(
            BenchmarkId::new(clone_run_id, case.n),
            &case.compiled,
            |b, compiled| {
                b.iter(|| {
                    let mut vm = Vm::new_program(black_box(compiled.clone()), StableRng::new(0));
                    let result = vm.run().unwrap();
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

fn bench_calc_pi_large_vm(c: &mut Criterion) {
    let cases = calc_pi_large_cases();
    for case in &cases {
        validate_calc_pi(case);
    }

    let mut group = c.benchmark_group("vm_calc_pi_large");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);

    for case in &cases {
        group.bench_with_input(
            BenchmarkId::new(format!("{}_run_only", case.variant), case.n),
            &case.compiled,
            |b, compiled| {
                b.iter_batched(
                    || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                    |mut vm| {
                        let result = vm.run().unwrap();
                        black_box(result);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn bench_i64_function_calls_vm(c: &mut Criterion) {
    let cases = i64_function_cases();
    for case in &cases {
        validate_i64_function(case);
    }

    let mut group = c.benchmark_group("vm_i64_function_calls");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));
    group.sample_size(10);

    for case in &cases {
        let run_only_id = if case.variant == "advance" {
            "run_only".to_string()
        } else {
            format!("{}_run_only", case.variant)
        };
        group.bench_with_input(
            BenchmarkId::new(run_only_id, case.n),
            &case.compiled,
            |b, compiled| {
                b.iter_batched(
                    || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                    |mut vm| {
                        let result = vm.run().unwrap();
                        black_box(result);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        let clone_run_id = if case.variant == "advance" {
            "clone_new_program_run".to_string()
        } else {
            format!("{}_clone_new_program_run", case.variant)
        };
        group.bench_with_input(
            BenchmarkId::new(clone_run_id, case.n),
            &case.compiled,
            |b, compiled| {
                b.iter(|| {
                    let mut vm = Vm::new_program(black_box(compiled.clone()), StableRng::new(0));
                    let result = vm.run().unwrap();
                    black_box(result);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_calc_pi_vm,
    bench_calc_pi_large_vm,
    bench_i64_function_calls_vm
);
criterion_main!(benches);
