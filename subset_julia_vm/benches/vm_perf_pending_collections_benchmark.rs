//! A/B benchmark for array-indexing, array-mutation, range, iteration, string,
//! and miscellaneous instruction families (Issue #9098).
//!
//! Covers perf-pending items:
//!   IndexLoad IndexLoadInbounds IndexLoadTyped IndexLoadTypedInbounds
//!   IndexStore IndexStoreInbounds IndexStoreTyped
//!   IndexSlice SliceAll
//!   ArrayDeleteAt ArrayDeleteAtIndices ArrayInsert ArrayPop ArrayPopFirst ArrayPushFirst
//!   MakeRange MakeRangeF64 MakeRangeLazy MakeStepRangeLazy
//!   RangeCollect RangeFirst RangeLast RangeGetIndex
//!   IterateDynamic IterateFirst IterateFirstSplit IterateNext IterateNextSplit
//!   EqStr LtStr LeStr GtStr GeStr
//!   ConcatStrings StringConcat ToStr ToString
//!   IsNothing Zero EqStruct
//!
//! A-side (fast path): typed code that exercises the specific Rust fast-path
//!   instruction.
//! B-side (dispatch path): same operation routed through a @noinline barrier
//!   or untyped path that forces the VM to use the Julia method table.
//!
//! Run with:
//!   cargo bench -p subset_julia_vm --bench vm_perf_pending_collections_benchmark

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use std::hint::black_box;
use std::time::Duration;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::CompiledProgram;

// ---------------------------------------------------------------------------
// Array indexing & mutation
// ---------------------------------------------------------------------------

const ARRAY_INDEX_A: &str = r#"
function array_index_a_9098(n::Int64)::Int64
    a = Vector{Int64}(undef, n)
    total = Int64(0)
    for i in 1:n
        a[i] = i * 3
    end
    for i in 1:n
        total += a[i]
    end
    return total
end
array_index_a_9098(2000)
"#;

const ARRAY_INDEX_B: &str = r#"
@noinline set_elem_9098(a, i, v) = setindex!(a, v, i)
@noinline get_elem_9098(a, i) = getindex(a, i)

function array_index_b_9098(n)
    a = Vector{Int64}(undef, n)
    total = 0
    for i in 1:n
        set_elem_9098(a, i, i * 3)
    end
    for i in 1:n
        total += get_elem_9098(a, i)
    end
    return total
end
array_index_b_9098(2000)
"#;

const ARRAY_MUTATION_A: &str = r#"
function array_mutation_a_9098(n::Int64)::Int64
    a = Int64[]
    for i in 1:n
        push!(a, i)
    end
    pushfirst!(a, Int64(0))
    for i in 1:div(n, 4)
        pop!(a)
    end
    for i in 1:div(n, 8)
        popfirst!(a)
    end
    for i in 1:div(n, 16)
        insert!(a, 1, Int64(i))
    end
    for i in 1:div(n, 32)
        deleteat!(a, 1)
    end
    return length(a) + a[1]
end
array_mutation_a_9098(512)
"#;

const ARRAY_MUTATION_B: &str = r#"
@noinline do_push_9098(a, v) = push!(a, v)
@noinline do_pushfirst_9098(a, v) = pushfirst!(a, v)
@noinline do_pop_9098(a) = pop!(a)
@noinline do_popfirst_9098(a) = popfirst!(a)
@noinline do_insert_9098(a, i, v) = insert!(a, i, v)
@noinline do_deleteat_9098(a, i) = deleteat!(a, i)

function array_mutation_b_9098(n)
    a = Int64[]
    for i in 1:n
        do_push_9098(a, i)
    end
    do_pushfirst_9098(a, 0)
    for i in 1:div(n, 4)
        do_pop_9098(a)
    end
    for i in 1:div(n, 8)
        do_popfirst_9098(a)
    end
    for i in 1:div(n, 16)
        do_insert_9098(a, 1, i)
    end
    for i in 1:div(n, 32)
        do_deleteat_9098(a, 1)
    end
    return length(a) + a[1]
end
array_mutation_b_9098(512)
"#;

// ---------------------------------------------------------------------------
// Array slicing (IndexSlice SliceAll)
// ---------------------------------------------------------------------------

// A-side: typed slice via a[lo:hi] and a[:] → compiler emits IndexSlice / SliceAll.
const ARRAY_SLICE_A: &str = r#"
function array_slice_a_9098(n::Int64)::Int64
    a = [i for i in 1:n]
    total = Int64(0)
    for i in 1:3
        s1 = a[1:div(n, 2)]
        s2 = a[:]
        total += length(s1) + length(s2)
    end
    return total
end
array_slice_a_9098(200)
"#;

// B-side: manual element-by-element copy, bypassing IndexSlice/SliceAll.
// Shows the overhead savings of the fast slice instruction vs element-wise copy.
const ARRAY_SLICE_B: &str = r#"
function slice_manual_9098(a, lo::Int64, hi::Int64)
    result = Int64[]
    for i in lo:hi
        push!(result, a[i])
    end
    return result
end

function slice_all_manual_9098(a)
    n = length(a)
    result = Int64[]
    for i in 1:n
        push!(result, a[i])
    end
    return result
end

function array_slice_b_9098(n::Int64)::Int64
    a = [i for i in 1:n]
    total = Int64(0)
    for i in 1:3
        s1 = slice_manual_9098(a, 1, div(n, 2))
        s2 = slice_all_manual_9098(a)
        total += length(s1) + length(s2)
    end
    return total
end
array_slice_b_9098(200)
"#;

// ---------------------------------------------------------------------------
// Range construction & query (MakeRange MakeRangeF64 MakeRangeLazy MakeStepRangeLazy
//                              RangeFirst RangeLast RangeGetIndex RangeCollect)
// ---------------------------------------------------------------------------

const RANGE_A: &str = r#"
function range_a_9098(n::Int64)::Int64
    total = Int64(0)
    for k in 1:n
        r = 1:k
        total += first(r) + last(r) + r[1]
        rf = 0.0:0.1:1.0
        total += Int64(first(rf) == 0.0)
        step_r = 1:2:k
        c = collect(step_r)
        total += length(c)
    end
    return total
end
range_a_9098(1000)
"#;

const RANGE_B: &str = r#"
@noinline make_range_9098(a, b) = a:b
@noinline range_first_9098(r) = first(r)
@noinline range_last_9098(r) = last(r)
@noinline range_idx_9098(r, i) = r[i]
@noinline range_collect_9098(r) = collect(r)

function range_b_9098(n)
    total = 0
    for k in 1:n
        r = make_range_9098(1, k)
        total += range_first_9098(r) + range_last_9098(r) + range_idx_9098(r, 1)
        rf = make_range_9098(0.0, 1.0)
        total += Int64(range_first_9098(rf) == 0.0)
        step_r = 1:2:k
        c = range_collect_9098(step_r)
        total += length(c)
    end
    return total
end
range_b_9098(1000)
"#;

// ---------------------------------------------------------------------------
// Iteration protocol (IterateFirst IterateFirstSplit IterateNext IterateNextSplit
//                      IterateDynamic)
// ---------------------------------------------------------------------------

const ITER_A: &str = r#"
function iter_a_9098(n::Int64)::Int64
    total = Int64(0)
    a = [i for i in 1:n]
    for x in a
        total += x
    end
    d = Dict{Int64,Int64}()
    for i in 1:10
        d[i] = i * 2
    end
    for kv in d
        total += kv.second
    end
    return total
end
iter_a_9098(500)
"#;

const ITER_B: &str = r#"
@noinline do_iterate_9098(coll) = begin
    total = 0
    st = iterate(coll)
    while st !== nothing
        (v, s) = st
        total += isa(v, Int64) ? v : 0
        st = iterate(coll, s)
    end
    return total
end

function iter_b_9098(n)
    a = [i for i in 1:n]
    total = do_iterate_9098(a)
    d = Dict{Int64,Int64}()
    for i in 1:10
        d[i] = i * 2
    end
    for kv in d
        total += kv.second
    end
    return total
end
iter_b_9098(500)
"#;

// ---------------------------------------------------------------------------
// String comparisons & operations (EqStr LtStr LeStr GtStr GeStr
//                                   ConcatStrings StringConcat ToStr ToString)
// ---------------------------------------------------------------------------

const STRING_OPS_A: &str = r#"
function string_ops_a_9098(n::Int64)::Int64
    count = Int64(0)
    for i in 1:n
        s1 = string("hello_", i)
        s2 = string("hello_", i + 1)
        if s1 == s1
            count += 1
        end
        if s1 != s2
            count += 1
        end
        if s1 < s2
            count += 1
        end
        if s1 <= s2
            count += 1
        end
        if s2 > s1
            count += 1
        end
        if s2 >= s1
            count += 1
        end
        joined = s1 * s2
        count += length(joined)
        ts = string(i)
        count += length(ts)
    end
    return count
end
string_ops_a_9098(500)
"#;

const STRING_OPS_B: &str = r#"
@noinline str_eq_9098(a, b) = a == b
@noinline str_lt_9098(a, b) = a < b
@noinline str_concat_9098(a, b) = a * b
@noinline to_str_9098(x) = string(x)

function string_ops_b_9098(n)
    count = 0
    for i in 1:n
        s1 = string("hello_", i)
        s2 = string("hello_", i + 1)
        if str_eq_9098(s1, s1)
            count += 1
        end
        if !str_eq_9098(s1, s2)
            count += 1
        end
        if str_lt_9098(s1, s2)
            count += 1
        end
        if str_lt_9098(s1, s2) || str_eq_9098(s1, s2)
            count += 1
        end
        if str_lt_9098(s1, s2)
            count += 1
        end
        if str_lt_9098(s1, s2) || str_eq_9098(s2, s2)
            count += 1
        end
        joined = str_concat_9098(s1, s2)
        count += length(joined)
        ts = to_str_9098(i)
        count += length(ts)
    end
    return count
end
string_ops_b_9098(500)
"#;

// ---------------------------------------------------------------------------
// Misc: IsNothing, Zero, EqStruct
// ---------------------------------------------------------------------------

const MISC_A: &str = r#"
struct Point9098
    x::Float64
    y::Float64
end

function misc_a_9098(n::Int64)::Int64
    count = Int64(0)
    p1 = Point9098(1.0, 2.0)
    p2 = Point9098(1.0, 2.0)
    p3 = Point9098(3.0, 4.0)
    for i in 1:n
        v = i % 3 == 0 ? nothing : i
        if v === nothing
            count += 1
        end
        z = zero(Float64)
        count += Int64(z == 0.0)
        if p1 == p2
            count += 1
        end
        if p1 != p3
            count += 1
        end
    end
    return count
end
misc_a_9098(5000)
"#;

const MISC_B: &str = r#"
struct Point9098b
    x::Float64
    y::Float64
end

@noinline check_nothing_9098(v) = v === nothing
@noinline get_zero_9098(T) = zero(T)
@noinline struct_eq_9098(a, b) = a == b

function misc_b_9098(n)
    count = 0
    p1 = Point9098b(1.0, 2.0)
    p2 = Point9098b(1.0, 2.0)
    p3 = Point9098b(3.0, 4.0)
    for i in 1:n
        v = i % 3 == 0 ? nothing : i
        if check_nothing_9098(v)
            count += 1
        end
        z = get_zero_9098(Float64)
        count += Int64(z == 0.0)
        if struct_eq_9098(p1, p2)
            count += 1
        end
        if !struct_eq_9098(p1, p3)
            count += 1
        end
    end
    return count
end
misc_b_9098(5000)
"#;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn compile(source: &str) -> CompiledProgram {
    let program = parse_and_lower(source).unwrap();
    compile_with_cache(&program).unwrap()
}

fn run(compiled: &CompiledProgram) {
    let mut vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    vm.run().unwrap();
    black_box(vm);
}

struct Case {
    name: &'static str,
    a_src: &'static str,
    b_src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "array_index",
        a_src: ARRAY_INDEX_A,
        b_src: ARRAY_INDEX_B,
    },
    Case {
        name: "array_mutation",
        a_src: ARRAY_MUTATION_A,
        b_src: ARRAY_MUTATION_B,
    },
    Case {
        name: "array_slice",
        a_src: ARRAY_SLICE_A,
        b_src: ARRAY_SLICE_B,
    },
    Case {
        name: "range_ops",
        a_src: RANGE_A,
        b_src: RANGE_B,
    },
    Case {
        name: "iteration",
        a_src: ITER_A,
        b_src: ITER_B,
    },
    Case {
        name: "string_ops",
        a_src: STRING_OPS_A,
        b_src: STRING_OPS_B,
    },
    Case {
        name: "misc",
        a_src: MISC_A,
        b_src: MISC_B,
    },
];

fn bench_collections(c: &mut Criterion) {
    for case in CASES {
        let a_compiled = compile(case.a_src);
        let b_compiled = compile(case.b_src);

        run(&a_compiled);
        run(&b_compiled);

        let mut group = c.benchmark_group(format!("perf_pending_collections/{}", case.name));
        group.warm_up_time(Duration::from_millis(500));
        group.measurement_time(Duration::from_millis(2000));

        group.bench_with_input(
            BenchmarkId::new("fast_path", case.name),
            &a_compiled,
            |b, compiled| {
                b.iter_batched(
                    || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                    |mut vm| {
                        let r = vm.run().unwrap();
                        black_box(r);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("dispatch_path", case.name),
            &b_compiled,
            |b, compiled| {
                b.iter_batched(
                    || Vm::new_program(black_box(compiled.clone()), StableRng::new(0)),
                    |mut vm| {
                        let r = vm.run().unwrap();
                        black_box(r);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.finish();
    }
}

criterion_group!(benches, bench_collections);
criterion_main!(benches);
