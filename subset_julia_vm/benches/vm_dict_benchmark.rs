//! Precomputed-bytecode VM Dict benchmarks (Issue #6622).
//!
//! This benchmark separates `Vm::run()` from CLI startup, parsing, lowering,
//! and bytecode compilation. It exercises insert, lookup, iteration, deletion,
//! and post-delete insertion for integer and string keys.
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_dict_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

const INT_DICT_SOURCE: &str = r#"
function int_dict_workload_6622(n)
    d = Dict{Int64, Int64}()
    total = 0
    for i in 1:n
        d[i] = i * 2
    end
    for i in 1:n
        total += d[i]
    end
    for pair in d
        total += pair.first + pair.second
    end
    for i in 1:n
        if i % 4 == 0
            delete!(d, i)
        end
    end
    for i in (n + 1):(n + 64)
        d[i] = i * 3
    end
    return total + length(d)
end

int_dict_workload_6622(128)
"#;

const STRING_DICT_SOURCE: &str = r#"
function string_dict_workload_6622(n)
    d = Dict{String, Int64}()
    total = 0
    for i in 1:n
        d[string("k", i)] = i
    end
    for i in 1:n
        total += d[string("k", i)]
    end
    for pair in d
        total += length(pair.first) + pair.second
    end
    for i in 1:n
        if i % 5 == 0
            delete!(d, string("k", i))
        end
    end
    for i in (n + 1):(n + 32)
        d[string("k", i)] = i
    end
    return total + length(d)
end

string_dict_workload_6622(96)
"#;

struct DictBenchCase {
    name: &'static str,
    source: &'static str,
    expected: i64,
}

struct CompiledDictBenchCase {
    name: &'static str,
    expected: i64,
    compiled: CompiledProgram,
}

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let parsed = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).unwrap();
    compile_with_cache(&program).unwrap()
}

fn compile_case(case: DictBenchCase) -> CompiledDictBenchCase {
    CompiledDictBenchCase {
        name: case.name,
        expected: case.expected,
        compiled: compile_source(case.source),
    }
}

fn validate_case(case: &CompiledDictBenchCase) {
    let mut vm = Vm::new_program(case.compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    match result {
        Value::I64(actual) => assert_eq!(actual, case.expected, "{}", case.name),
        other => panic!("{} returned non-Int64 value: {:?}", case.name, other),
    }
}

fn dict_cases() -> Vec<CompiledDictBenchCase> {
    vec![
        compile_case(DictBenchCase {
            name: "int_keys_insert_lookup_iterate_rehash",
            source: INT_DICT_SOURCE,
            expected: 41_440,
        }),
        compile_case(DictBenchCase {
            name: "string_keys_insert_lookup_iterate_rehash",
            source: STRING_DICT_SOURCE,
            expected: 9_700,
        }),
    ]
}

fn bench_vm_dict(c: &mut Criterion) {
    let cases = dict_cases();
    for case in &cases {
        validate_case(case);
    }

    let mut group = c.benchmark_group("vm_dict");
    for case in cases {
        group.bench_with_input(
            BenchmarkId::new("run_only", case.name),
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

criterion_group!(benches, bench_vm_dict);
criterion_main!(benches);
