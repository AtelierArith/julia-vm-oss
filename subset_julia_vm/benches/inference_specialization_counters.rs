//! Inference/specialization effectiveness counters (Issue #5095).
//!
//! Run with:
//!   cargo bench --features profiling --bench inference_specialization_counters
//!
//! The `profiling` feature keeps these counters out of the default VM hot path.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;
use std::sync::Once;
use subset_julia_vm::compile::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::profiler;
use subset_julia_vm::vm::Vm;

struct CounterCase {
    name: &'static str,
    source: &'static str,
}

const COUNTER_CASES: &[CounterCase] = &[
    CounterCase {
        name: "typed_int_loop",
        source: r#"
function typed_int_loop(n)
    s = 0
    for i in 1:n
        s += i
    end
    s
end
typed_int_loop(1000)
"#,
    },
    CounterCase {
        name: "dynamic_number_dispatch",
        source: r#"
function twice(x::Int64)
    x + x
end
function twice(x::Float64)
    x + x
end
a = Any[1, 2.0, 3, 4.0]
s = 0.0
for x in a
    s += twice(x)
end
s
"#,
    },
    CounterCase {
        name: "foreach_array_sum",
        source: r#"
function foreach_array_sum(n)
    a = collect(1:n)
    s = 0
    for x in a
        s += x
    end
    s
end
foreach_array_sum(1000)
"#,
    },
    CounterCase {
        name: "hof_map_reduce",
        source: r#"
a = collect(1:100)
b = map(x -> x * 2, a)
foldl(+, b; init=0)
"#,
    },
];

static REPORT_ONCE: Once = Once::new();

fn compile_case(source: &str) -> subset_julia_vm::vm::CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let outcome = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(outcome).unwrap();
    compile_with_cache(&program).unwrap()
}

fn run_with_counters(source: &str) -> profiler::SpecializationCounters {
    let compiled = compile_case(source);
    run_compiled_with_counters(compiled)
}

fn run_compiled_with_counters(
    compiled: subset_julia_vm::vm::CompiledProgram,
) -> profiler::SpecializationCounters {
    profiler::clear();
    profiler::enable();
    let rng = StableRng::new(0);
    let mut vm = Vm::new_program(compiled, rng);
    let _result = vm.run().unwrap();
    profiler::disable();
    let counters = profiler::specialization_counters();
    profiler::clear();
    counters
}

fn print_counter_report() {
    REPORT_ONCE.call_once(|| {
        eprintln!("\n=== Inference Specialization Counters (Issue #5095) ===");
        if !cfg!(feature = "profiling") {
            eprintln!("profiling feature disabled; counters are compile-time no-ops");
        }
        eprintln!(
            "{:<26} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
            "case", "boxing", "miss", "devirt", "typed_op", "dyn_disp", "boxed"
        );
        for case in COUNTER_CASES {
            let counters = run_with_counters(case.source);
            eprintln!(
                "{:<26} {:>9.2}% {:>9.2}% {:>9.2}% {:>9.2}% {:>10} {:>10}",
                case.name,
                counters.boxing_rate() * 100.0,
                counters.dispatch_miss_rate() * 100.0,
                counters.devirtualization_rate() * 100.0,
                counters.specialized_arithmetic_rate() * 100.0,
                counters.dynamic_dispatches,
                counters.boxed_value_accesses
            );
        }
        eprintln!("=======================================================\n");
    });
}

fn bench_inference_specialization_counters(c: &mut Criterion) {
    print_counter_report();

    let compiled_cases: Vec<_> = COUNTER_CASES
        .iter()
        .map(|case| (case.name, compile_case(case.source)))
        .collect();

    let mut group = c.benchmark_group("inference_specialization_counters");
    for (name, compiled) in compiled_cases {
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &compiled,
            |b, program| {
                b.iter(|| run_compiled_with_counters(black_box(program.clone())));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_inference_specialization_counters);
criterion_main!(benches);
