//! Hot-path VM benchmarks for performance-critical code paths.
//!
//! This file benchmarks the VM execution paths identified in Issue #2926:
//! - Dynamic dispatch overhead (multiple-method dispatch)
//! - String operations (concatenation, interpolation)
//! - Collection HOF operations (map, filter, reduce)
//! - Array creation and indexed access
//!
//! Run with: cargo bench --bench hot_paths_benchmark

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::compile_and_run_str;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::{CompiledProgram, Value};

// ── Dynamic dispatch ──────────────────────────────────────────────────────────

const MONOMORPHIC_DYNAMIC_DISPATCH_6345_SOURCE: &str = r#"
function dispatch_string_6345(x::String)
    length(x)
end

function run_dispatch_string_6345(n, x::Any)
    total = 0
    for i in 1:n
        total += dispatch_string_6345(x)
    end
    total
end

run_dispatch_string_6345(20000, "abcd")
"#;

const SLOT_CONST_ADD_8446_SOURCE: &str = r#"
function add_const_8446(x::Int64)
    x + 1
end

function run_slot_const_add_8446(n::Int64)
    total = 0
    for i in 1:n
        total += add_const_8446(i)
    end
    total
end

run_slot_const_add_8446(20000)
"#;

fn compile_source(source: &str) -> CompiledProgram {
    let mut parser = Parser::new().unwrap();
    let parsed = parser.parse(source).unwrap();
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parsed).unwrap();
    compile_with_cache(&program).unwrap()
}

/// Benchmark: Dynamic dispatch — multiple methods, type-based dispatch
fn bench_dynamic_dispatch(c: &mut Criterion) {
    let source = r#"
function describe(x::Int64)
    "integer"
end
function describe(x::Float64)
    "float"
end
function describe(x::String)
    "string"
end
describe(1) * describe(1.0) * describe("hi")
"#;
    c.bench_function("dynamic_dispatch_3methods", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: VM-only monomorphic dynamic dispatch on an Any-typed argument.
///
/// The callee has a single `::String` method, so the caller emits
/// `CallDynamic` rather than `CallTypedDispatch`. The loop warms the L1
/// call-site cache once, then repeatedly exercises the monomorphic hit path.
fn bench_monomorphic_dynamic_dispatch_vm_run(c: &mut Criterion) {
    let compiled = compile_source(MONOMORPHIC_DYNAMIC_DISPATCH_6345_SOURCE);
    let mut validation_vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    match validation_vm.run().unwrap() {
        Value::I64(value) => assert_eq!(value, 80_000),
        other => panic!("expected Int64 result from dispatch benchmark, got {other:?}"),
    }

    c.bench_function("vm_monomorphic_dynamic_dispatch_string_20000", |b| {
        b.iter(|| {
            let mut vm = Vm::new_program(black_box(compiled.clone()), StableRng::new(0));
            black_box(vm.run().unwrap())
        })
    });
}

/// Benchmark: Static (monomorphic) dispatch for comparison
fn bench_static_dispatch(c: &mut Criterion) {
    let source = r#"
function add_ints(x::Int64, y::Int64)
    x + y
end
add_ints(10, 20)
"#;
    c.bench_function("static_dispatch_int_add", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── String operations ─────────────────────────────────────────────────────────

/// Benchmark: String concatenation
fn bench_string_concat(c: &mut Criterion) {
    let source = r#"
s = "hello"
t = "world"
s * " " * t
"#;
    c.bench_function("string_concat_3parts", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: String interpolation
fn bench_string_interpolation(c: &mut Criterion) {
    let source = r#"
x = 42
name = "Julia"
"The answer is $x in $name"
"#;
    c.bench_function("string_interpolation_2vars", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── Array operations ──────────────────────────────────────────────────────────

/// Benchmark: Array creation (literal)
fn bench_array_creation(c: &mut Criterion) {
    let source = "arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]; length(arr)";
    c.bench_function("array_create_10elem", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: Array indexing in a loop
fn bench_array_indexing(c: &mut Criterion) {
    let source = r#"
arr = [1, 2, 3, 4, 5]
total = 0
for i in 1:5
    total += arr[i]
end
total
"#;
    c.bench_function("array_indexing_loop_5", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: scalar `getindex` through an `Any`-typed binding when a user
/// `getindex(::Vector{Int64}, ::Int)` override exists (Issue #6657).
///
/// This exercises the new `CallTypedDispatchOrBuiltin(GetIndex, ..)` runtime
/// dispatch path (with the native-`IndexLoad` builtin fallback) that lets an
/// `xs[i]` reach a user array override. The no-override common case keeps the
/// native fast path and is covered by `bench_array_indexing`.
fn bench_getindex_any_user_override(c: &mut Criterion) {
    let source = r#"
import Base: getindex
getindex(xs::Vector{Int64}, i::Int) = i
function index_loop(xs, n)
    s = 0
    for i in 1:n
        s += xs[1]
    end
    s
end
index_loop([10, 20, 30], 20000)
"#;
    c.bench_function("getindex_any_user_override_20000", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── Constant-step integer range loops (Issue #5166) ───────────────────────────

/// Benchmark: tight counting loop with an implicit unit step.
///
/// With the constant-step specialization the per-iteration sign check is hoisted
/// out and the increment becomes a single `IncVarI64`, so this measures the
/// best-case integer range loop throughput.
fn bench_const_step_count_loop(c: &mut Criterion) {
    let source = r#"
function count_loop(n)
    s = 0
    for i in 1:n
        s += i
    end
    s
end
count_loop(10000)
"#;
    c.bench_function("const_step_count_loop_10000", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: generic `for x in array` ForEach loop (Issue #5168).
///
/// This exercises the tuple-free ForEach lowering (`IterateFirstSplit` /
/// `IterateNextSplit`). Before #5168 every iteration allocated a
/// `(element, state)` tuple on the heap and emitted `TupleFirst` / `TupleSecond`
/// clones; this benchmark tracks the per-iteration allocation/clone cost.
fn bench_foreach_array_sum(c: &mut Criterion) {
    let source = r#"
function array_sum(n)
    a = collect(1:n)
    s = 0
    for x in a
        s += x
    end
    s
end
array_sum(10000)
"#;
    c.bench_function("foreach_array_sum_10000", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: counting down with a literal negative unit step (`n:-1:1`).
fn bench_const_step_countdown_loop(c: &mut Criterion) {
    let source = r#"
function countdown_loop(n)
    s = 0
    for i in n:-1:1
        s += i
    end
    s
end
countdown_loop(10000)
"#;
    c.bench_function("const_step_countdown_loop_10000", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: per-instruction dispatch overhead in a tight inner loop
/// (Issue #5177). Each iteration executes several cheap arithmetic/store
/// instructions, so the cost is dominated by the dispatch loop itself — the
/// path from which the per-instruction `mem::replace(.., Nop)` swap/restore was
/// removed in favour of an immutable `Rc<[Instr]>` snapshot.
fn bench_dispatch_loop_overhead(c: &mut Criterion) {
    let source = r#"
function tight_loop(n)
    a = 0
    b = 1
    for i in 1:n
        a = a + i
        b = b + a
        a = a - 1
    end
    a + b
end
tight_loop(20000)
"#;
    c.bench_function("dispatch_loop_overhead_20000", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: VM-only slot load plus literal-add superinstruction (Issue #8446).
///
/// The leaf function compiles `x + 1` to `LoadAddConstI64Slot`, reducing the
/// function body from three arithmetic/load instructions to one before return.
fn bench_slot_const_add_vm_run(c: &mut Criterion) {
    let compiled = compile_source(SLOT_CONST_ADD_8446_SOURCE);
    let mut validation_vm = Vm::new_program(compiled.clone(), StableRng::new(0));
    match validation_vm.run().unwrap() {
        Value::I64(value) => assert_eq!(value, 200_030_000),
        other => panic!("expected Int64 result from slot const add benchmark, got {other:?}"),
    }

    c.bench_function("vm_slot_const_add_20000", |b| {
        b.iter(|| {
            let mut vm = Vm::new_program(black_box(compiled.clone()), StableRng::new(0));
            black_box(vm.run().unwrap())
        })
    });
}

// ── Collection HOF operations ─────────────────────────────────────────────────

/// Benchmark: map over array
fn bench_map_operation(c: &mut Criterion) {
    let source = r#"
arr = [1, 2, 3, 4, 5]
result = map(x -> x * 2, arr)
sum(result)
"#;
    c.bench_function("map_double_5elem", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: filter over array
fn bench_filter_operation(c: &mut Criterion) {
    let source = r#"
arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
result = filter(x -> x % 2 == 0, arr)
length(result)
"#;
    c.bench_function("filter_even_10elem", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: reduce/foldl over array
fn bench_reduce_operation(c: &mut Criterion) {
    let source = r#"
arr = [1, 2, 3, 4, 5]
foldl(+, arr; init=0)
"#;
    c.bench_function("foldl_sum_5elem", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── Function call overhead ────────────────────────────────────────────────────

/// Benchmark: Recursive function calls (call stack overhead)
fn bench_recursive_calls(c: &mut Criterion) {
    let source = r#"
function count_down(n)
    if n <= 0
        return 0
    end
    1 + count_down(n - 1)
end
count_down(10)
"#;
    c.bench_function("recursive_calls_depth10", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: tree-recursive `fib(25)` — heavy call-frame churn (Issue #5172).
///
/// `fib(25)` performs ~242k calls, each of which pushes and pops a call frame.
/// This is the workload the frame pool targets: with pooling, the frame's ~20
/// backing maps are recycled instead of re-allocated on every call. Pairs with
/// `recursive_calls_depth10` (shallow) to measure deep-recursion behaviour.
fn bench_fib_recursion(c: &mut Criterion) {
    let source = r#"
function fib(n)
    n < 2 ? n : fib(n - 1) + fib(n - 2)
end
fib(25)
"#;
    c.bench_function("fib_recursion_25", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: Higher-order function call (function as argument)
fn bench_hof_call(c: &mut Criterion) {
    let source = r#"
function apply_twice(f, x)
    f(f(x))
end
apply_twice(x -> x + 1, 0)
"#;
    c.bench_function("hof_apply_twice_lambda", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── Closure-capture HOF (Issue #5189) ──────────────────────────────────────────

/// Benchmark: a closure capturing two outer variables applied per-element over a
/// sizable array — the `map(x -> a*x + b, big_arr)` pattern from Issue #5189.
///
/// Each per-element call clones the `ClosureValue`; storing the capture set
/// behind an `Rc` (Issue #5189) makes that clone an O(1) refcount bump that
/// shares the frozen `Vec<(String, Value)>` instead of deep-cloning it N times.
fn bench_closure_capture_map(c: &mut Criterion) {
    let source = r#"
function affine_map(a, b, xs)
    map(x -> a * x + b, xs)
end
affine_map(2, 3, collect(1:1000))
"#;
    c.bench_function("closure_capture_affine_map_1000", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── Dict string-key reads (Issue #5187) ────────────────────────────────────────

/// Benchmark: repeated `Dict{String,Int}` string-key reads.
///
/// Exercises the borrowed-key probe (`DictValue::get_by_value`) introduced in
/// Issue #5187, which hashes/compares the `&str` key against stored keys without
/// allocating an owned `DictKey` String on each read.
fn bench_dict_string_key_reads(c: &mut Criterion) {
    let source = r#"
d = Dict("alpha" => 1, "beta" => 2, "gamma" => 3, "delta" => 4)
s = 0
for _ in 1:1000
    s += d["alpha"] + d["beta"] + d["gamma"] + d["delta"]
end
s
"#;
    c.bench_function("dict_string_key_reads_4keys_1000x", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── Dict hashing (Issue #5188) ────────────────────────────────────────────────

/// Benchmark: integer-keyed Dict insert + lookup. Exercises the internal
/// open-addressing slot hash (FxHash since Issue #5188) under resize.
fn bench_dict_int_insert_lookup(c: &mut Criterion) {
    let source = r#"
d = Dict{Int,Int}()
for i in 1:500
    d[i] = i * 2
end
s = 0
for i in 1:500
    s += d[i]
end
s
"#;
    c.bench_function("dict_int_insert_lookup_500", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: string-keyed Dict insert + lookup. String keys hash variable-
/// length byte slices, the path most sensitive to the slot-hash swap.
fn bench_dict_string_insert_lookup(c: &mut Criterion) {
    let source = r#"
d = Dict{String,Int}()
for i in 1:300
    d["key_" * string(i)] = i
end
s = 0
for i in 1:300
    s += d["key_" * string(i)]
end
s
"#;
    c.bench_function("dict_string_insert_lookup_300", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: `isa`-guarded arithmetic over an `Any`-typed container (Issue
/// #5181). Each element flows through an `if x isa Int64 ... end` guard; with
/// flow-sensitive codegen narrowing the guarded body loads `x` via `LoadI64`
/// and adds with the typed integer path instead of dynamic dispatch.
fn bench_isa_narrowed_sum(c: &mut Criterion) {
    let source = r#"
function sum_ints(v)
    s = 0
    for x in v
        if x isa Int64
            s += x * x + 1
        end
    end
    s
end
v = Any[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
total = 0
for _ in 1:200
    total += sum_ints(v)
end
total
"#;
    c.bench_function("isa_narrowed_int_sum_200", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

// ── Symbol interning (Issue #5174) ────────────────────────────────────────────

/// Benchmark: repeated Symbol construction.
///
/// `Symbol(...)` builds a `SymbolValue` each iteration. With interning
/// (Issue #5174) the backing `Rc<str>` is shared per distinct name, so the
/// common case of constructing the same handful of symbols repeatedly is
/// allocation-free after the first occurrence.
fn bench_symbol_construction(c: &mut Criterion) {
    let source = r#"
n = 0
for i in 1:2000
    s = Symbol("alpha")
    t = Symbol("beta")
    if s == :alpha
        n += 1
    end
    if t != s
        n += 1
    end
end
n
"#;
    c.bench_function("symbol_construction_repeated", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

/// Benchmark: Symbol equality / comparison heavy loop.
///
/// Exercises the `SymbolValue` `PartialEq` fast path (pointer compare on
/// interned symbols) on a Symbol-keyed branch.
fn bench_symbol_equality(c: &mut Criterion) {
    let source = r#"
function classify(tag::Symbol)
    if tag == :red
        1
    elseif tag == :green
        2
    elseif tag == :blue
        3
    else
        0
    end
end
acc = 0
tags = [:red, :green, :blue, :other]
for i in 1:2000
    acc += classify(tags[(i % 4) + 1])
end
acc
"#;
    c.bench_function("symbol_equality_dispatch", |b| {
        b.iter(|| compile_and_run_str(black_box(source), 0))
    });
}

criterion_group!(
    hot_path_benches,
    bench_dynamic_dispatch,
    bench_monomorphic_dynamic_dispatch_vm_run,
    bench_static_dispatch,
    bench_isa_narrowed_sum,
    bench_string_concat,
    bench_string_interpolation,
    bench_array_creation,
    bench_array_indexing,
    bench_getindex_any_user_override,
    bench_const_step_count_loop,
    bench_const_step_countdown_loop,
    bench_dispatch_loop_overhead,
    bench_slot_const_add_vm_run,
    bench_foreach_array_sum,
    bench_map_operation,
    bench_filter_operation,
    bench_reduce_operation,
    bench_recursive_calls,
    bench_fib_recursion,
    bench_hof_call,
    bench_closure_capture_map,
    bench_dict_string_key_reads,
    bench_dict_int_insert_lookup,
    bench_dict_string_insert_lookup,
    bench_symbol_construction,
    bench_symbol_equality,
);
criterion_main!(hot_path_benches);
