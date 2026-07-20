//! Precomputed-bytecode VM Array benchmarks (Issue #6653).
//!
//! This benchmark separates `Vm::run()` from CLI startup, parsing, lowering,
//! and bytecode compilation. It exercises public MemoryRef-backed `Array{T,N}`
//! wrappers after the native array carrier demotion.
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_array_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

const ARRAY_INDEX_MUTATION_SOURCE: &str = r#"
function array_index_mutation_workload_6653(n)
    a = [0 for i in 1:n]
    total = 0
    for i in 1:n
        a[i] = i * 3
    end
    for i in 1:n
        total += a[i]
    end
    for i in 1:64
        push!(a, i)
    end
    for i in 1:32
        pop!(a)
    end
    for i in 1:16
        pushfirst!(a, i)
    end
    for i in 1:8
        popfirst!(a)
    end
    return total + length(a) + a[1] + a[length(a)]
end

array_index_mutation_workload_6653(128)
"#;

const ARRAY_HOF_BROADCAST_SOURCE: &str = r#"
function array_hof_broadcast_workload_6653(n)
    a = [i for i in 1:n]
    b = map(x -> x + 1, a)
    c = broadcast(+, a, b)
    d = filter(isodd, c)
    return reduce(+, d) + length(d) + d[length(d)]
end

array_hof_broadcast_workload_6653(128)
"#;

// Multi-dimensional indexing baseline (Issue #6805). Builds an `n x n`
// `Matrix{Int64}` and walks it with `a[i, j]` cartesian indexing so the
// migration off `Value::ExprArgs` keeps the multi-dim `IndexLoad` path fast.
const ARRAY_MULTIDIM_INDEX_SOURCE: &str = r#"
function array_multidim_index_workload_6805(n)
    a = [i + j for i in 1:n, j in 1:n]
    total = 0
    for j in 1:n
        for i in 1:n
            total += a[i, j]
        end
    end
    return total
end

array_multidim_index_workload_6805(32)
"#;

// Array construction baseline (Issue #6805). Repeatedly constructs
// MemoryRef-backed `Array{T,N}` wrappers via `Vector{Int64}(undef, k)` and
// `zeros(Int64, k)` so allocation/construction cost is tracked across the
// carrier removal.
const ARRAY_CONSTRUCTION_SOURCE: &str = r#"
function array_construction_workload_6805(n)
    total = 0
    for k in 1:n
        a = Vector{Int64}(undef, k)
        for i in 1:k
            a[i] = i
        end
        b = zeros(Int64, k)
        total += length(a) + length(b) + a[k]
    end
    return total
end

array_construction_workload_6805(128)
"#;

// `view` / `SubArray` parent-sharing baseline (Issue #6805). Creates repeated
// `view(a, s:n)` slices that share the parent buffer and sums them, exercising
// the parent-sharing path that `MemoryRef` offset semantics must preserve.
const ARRAY_VIEW_SOURCE: &str = r#"
function array_view_workload_6805(n)
    a = [i for i in 1:n]
    total = 0
    for s in 1:n-1
        v = view(a, s:n)
        total += sum(v)
    end
    return total + length(a)
end

array_view_workload_6805(64)
"#;

// Small array-literal allocation baseline (Issue #6846). A tight nested loop
// that allocates a fresh 2-element `[i, j]` literal per iteration mirrors the
// `(x, y) -> sinc(norm([x, y]))` surface-plot kernel that regressed when array
// construction moved onto Memory-backed wrappers via a per-literal pure-Julia
// `wrap(::Type{Array}, ...)` call. The literal builder now finalizes the
// backing `Memory` into the wrapper natively, so this case tracks that the
// per-literal allocation stays cheap.
const ARRAY_LITERAL_ALLOC_SOURCE: &str = r#"
function array_literal_alloc_workload_6846(n)
    total = 0
    for i in 1:n
        for j in 1:n
            v = [i, j]
            total += v[1] + v[2]
        end
    end
    return total
end

array_literal_alloc_workload_6846(128)
"#;

// Sequential array-growth baseline (Issue #6873). A comprehension and a `push!`
// loop both grow an `Array{T}` wrapper one element at a time. This regressed to
// O(n^2) because each append reallocated an exact-size backing `Memory` and
// copied every prior element (`push_array_wrapper`), so an n-element build did
// ~n^2/2 element copies — the dominant cost of the `Float64[zf for ...]`
// surface-plot kernel (Issue #6846). Appends now grow the backing `Memory` in
// place via amortized (geometric) Vec growth, so this case must stay ~O(n).
const ARRAY_GROWTH_SOURCE: &str = r#"
function array_growth_workload_6873(n)
    a = [i for i in 1:n]
    b = Int64[]
    for i in 1:n
        push!(b, i)
    end
    return sum(a) + sum(b) + length(a) + length(b)
end

array_growth_workload_6873(2048)
"#;

struct ArrayBenchCase {
    name: &'static str,
    source: &'static str,
    expected: i64,
}

struct CompiledArrayBenchCase {
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

fn compile_case(case: ArrayBenchCase) -> CompiledArrayBenchCase {
    CompiledArrayBenchCase {
        name: case.name,
        expected: case.expected,
        compiled: compile_source(case.source),
    }
}

fn validate_case(case: &CompiledArrayBenchCase) {
    let mut vm = Vm::new_program(case.compiled.clone(), StableRng::new(0));
    let result = vm.run().unwrap();
    match result {
        Value::I64(actual) => assert_eq!(actual, case.expected, "{}", case.name),
        other => panic!("{} returned non-Int64 value: {:?}", case.name, other),
    }
}

fn array_cases() -> Vec<CompiledArrayBenchCase> {
    vec![
        compile_case(ArrayBenchCase {
            name: "index_mutation_push_pop_128",
            source: ARRAY_INDEX_MUTATION_SOURCE,
            expected: 24_976,
        }),
        compile_case(ArrayBenchCase {
            name: "hof_broadcast_filter_reduce_128",
            source: ARRAY_HOF_BROADCAST_SOURCE,
            expected: 17_025,
        }),
        compile_case(ArrayBenchCase {
            name: "multidim_index_32x32",
            source: ARRAY_MULTIDIM_INDEX_SOURCE,
            expected: 33_792,
        }),
        compile_case(ArrayBenchCase {
            name: "construction_undef_zeros_128",
            source: ARRAY_CONSTRUCTION_SOURCE,
            expected: 24_768,
        }),
        compile_case(ArrayBenchCase {
            name: "view_subarray_parent_share_64",
            source: ARRAY_VIEW_SOURCE,
            expected: 89_440,
        }),
        compile_case(ArrayBenchCase {
            name: "literal_alloc_2elem_128",
            source: ARRAY_LITERAL_ALLOC_SOURCE,
            expected: 2_113_536,
        }),
        compile_case(ArrayBenchCase {
            name: "growth_comprehension_push_2048",
            source: ARRAY_GROWTH_SOURCE,
            expected: 4_200_448,
        }),
    ]
}

fn bench_vm_array(c: &mut Criterion) {
    let cases = array_cases();
    for case in &cases {
        validate_case(case);
    }

    let mut group = c.benchmark_group("vm_array");
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

criterion_group!(benches, bench_vm_array);
criterion_main!(benches);
