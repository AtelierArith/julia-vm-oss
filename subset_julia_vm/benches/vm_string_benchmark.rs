//! Precomputed-bytecode VM string benchmarks (Issue #8629, parent #8612).
//!
//! This benchmark separates `Vm::run()` from CLI startup, parsing, lowering,
//! and bytecode compilation. It exercises the paths where `Value::Str`
//! payloads are cloned: long-string assignment/argument-passing loops,
//! `Dict{String, Int64}` insertion and lookup, `join`/`split`/concatenation,
//! and storing long strings into arrays.
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_string_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

const ASSIGN_PASS_SOURCE: &str = r#"
function make_long_string_8629(target_len::Int64)
    s = "abcdefghijklmnop"
    while length(s) < target_len
        s = s * s
    end
    return s
end

probe_len_8629(s::String) = length(s) % 97

function assign_pass_loop_8629(s::String, iters::Int64)
    total = 0
    t = s
    for i in 1:iters
        u = t
        total += probe_len_8629(u)
        t = u
    end
    return total
end

assign_pass_loop_8629(make_long_string_8629(4096), 2000)
"#;

const DICT_STRING_SOURCE: &str = r#"
function dict_insert_lookup_8629(n::Int64)
    d = Dict{String, Int64}()
    for i in 1:n
        d[string("key_", i)] = i
    end
    total = 0
    for i in 1:n
        total += d[string("key_", i)]
    end
    return total + length(d)
end

dict_insert_lookup_8629(400)
"#;

const JOIN_SPLIT_CONCAT_SOURCE: &str = r#"
function join_split_concat_8629(n::Int64)
    parts = String[]
    for i in 1:n
        push!(parts, string("part", i))
    end
    joined = join(parts, ",")
    pieces = split(joined, ",")
    total = 0
    for p in pieces
        total += length(p)
    end
    acc = ""
    for i in 1:n
        acc = acc * "x"
    end
    return total + length(acc) + length(joined)
end

join_split_concat_8629(200)
"#;

const ARRAY_STORE_SOURCE: &str = r#"
function make_long_string_8629(target_len::Int64)
    s = "abcdefghijklmnop"
    while length(s) < target_len
        s = s * s
    end
    return s
end

probe_len_8629(s::String) = length(s) % 97

function array_store_8629(s::String, n::Int64)
    arr = String[]
    for i in 1:n
        push!(arr, s)
    end
    total = 0
    for x in arr
        total += probe_len_8629(x)
    end
    return total
end

array_store_8629(make_long_string_8629(4096), 2000)
"#;

struct StringBenchCase {
    name: &'static str,
    source: &'static str,
    expected: i64,
}

struct CompiledStringBenchCase {
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

fn compile_case(case: StringBenchCase) -> CompiledStringBenchCase {
    CompiledStringBenchCase {
        name: case.name,
        expected: case.expected,
        compiled: compile_source(case.source),
    }
}

fn validate_case(case: &CompiledStringBenchCase) {
    let mut vm = Vm::new_program(case.compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    match result {
        Value::I64(actual) => assert_eq!(actual, case.expected, "{}", case.name),
        other => panic!("{} returned non-Int64 value: {:?}", case.name, other),
    }
}

fn string_cases() -> Vec<CompiledStringBenchCase> {
    vec![
        compile_case(StringBenchCase {
            name: "long_string_assign_pass",
            source: ASSIGN_PASS_SOURCE,
            expected: 44_000,
        }),
        compile_case(StringBenchCase {
            name: "dict_string_keys_insert_lookup",
            source: DICT_STRING_SOURCE,
            expected: 80_600,
        }),
        compile_case(StringBenchCase {
            name: "join_split_concat",
            source: JOIN_SPLIT_CONCAT_SOURCE,
            expected: 2_983,
        }),
        compile_case(StringBenchCase {
            name: "array_store_long_string",
            source: ARRAY_STORE_SOURCE,
            expected: 44_000,
        }),
    ]
}

fn bench_vm_string(c: &mut Criterion) {
    let cases = string_cases();
    for case in &cases {
        validate_case(case);
    }

    let mut group = c.benchmark_group("vm_string");
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

criterion_group!(benches, bench_vm_string);
criterion_main!(benches);
