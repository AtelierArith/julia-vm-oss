# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: type_inference/array_element_inference.jl =====
module Agg_array_element_inference
# Test: Array element type inference
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function first_element(arr)
    if !isempty(arr)
        return arr[1]  # Should infer element type from array
    else
        return nothing
    end
end

@testset "Array element type inference" begin
    @test first_element([1, 2, 3]) == 1
    @test first_element([1.0, 2.0, 3.0]) == 1.0
    @test first_element(Int64[]) === nothing
end
end # module Agg_array_element_inference

# ===== source: type_inference/array_eltype_propagation_5083.jl =====
module Agg_array_eltype_propagation_5083
# Issue #5083: array element types beyond Int64/Float64 must propagate through
# inference so that `a[i]` yields the concrete element type instead of `Any`.
# This previously collapsed to `Top` for Int8/16/32, UInt*, Float32, Int128,
# UInt128, Symbol, and Complex element arrays.
using Test

# Functions must be defined OUTSIDE the @testset block per project guidelines.
function index_type(arr)
    return typeof(arr[1])
end

function sum_scan(arr)
    # A numeric scan whose accumulator type depends on the element type being
    # preserved through inference.
    s = zero(eltype(arr))
    for i in eachindex(arr)
        s += arr[i]
    end
    return s
end

@testset "Issue #5083 array element type propagation" begin
    @test index_type(Int8[1, 2, 3]) === Int8
    @test index_type(Int16[1, 2, 3]) === Int16
    @test index_type(Int32[1, 2, 3]) === Int32
    @test index_type(UInt8[1, 2, 3]) === UInt8
    @test index_type(UInt16[1, 2, 3]) === UInt16
    @test index_type(UInt32[1, 2, 3]) === UInt32
    @test index_type(UInt64[1, 2, 3]) === UInt64
    @test index_type(Float32[1.0, 2.0, 3.0]) === Float32
    @test index_type([:a, :b, :c]) === Symbol

    # Numeric scans preserve the element type in the accumulator.
    @test sum_scan(Int8[1, 2, 3]) === Int8(6)
    @test sum_scan(Int32[10, 20, 30]) === Int32(60)
    @test sum_scan(UInt8[1, 2, 3]) === UInt8(6)
    @test sum_scan(Float32[1.0, 2.0, 3.0]) === 6.0f0
end
end # module Agg_array_eltype_propagation_5083

# ===== source: type_inference/heterogeneous_array_inference.jl =====
module Agg_heterogeneous_array_inference
# Test: Heterogeneous array literal inference preserves Array container shape
# (Issue #3528). The inference engine no longer collapses `[1, 2.0]` to Top —
# it preserves an Array container with a Union element type. Direct evaluation
# of `[1, nothing]` is tracked separately because of a downstream codegen bug
# unrelated to inference.
using Test

function len_int_float()
    xs = [1, 2.0]
    return length(xs)
end

function eltype_int_float()
    xs = [1, 2.0]
    return eltype(xs)
end

@testset "Heterogeneous array literal inference" begin
    @test len_int_float() == 2
    # `[1, 2.0]` promotes to Float64 in Julia, but the VM may infer
    # Union{Int64, Float64}; either is fine here as long as the array shape
    # is preserved.
    et = eltype_int_float()
    @test et === Float64 || et === Int64 || et === Real || et === Number
end
end # module Agg_heterogeneous_array_inference

# ===== source: type_inference/hex_literal_range_runtime_element_3559.jl =====
module Agg_hex_literal_range_runtime_element_3559
# Issue #3559: hex / binary / octal integer literals encode their bit width in
# the source form (`0x01` is `UInt8`, `0x0001` is `UInt16`, …). Ranges built
# from such literals must preserve that element type at runtime — both
# `typeof(range)` and the loop variable should reflect the literal's typed
# width, not widen to `Int64` / `UnitRange{Int64}`.
using Test

# Hard assertions that must hold for the fixture to pass — these `@assert`s
# raise on failure (returning a non-true value from the script) so the test
# runner detects a regression even though `@test` failures alone wouldn't.

# Plain hex / binary / octal literal types.
@assert typeof(0x01) === UInt8
@assert typeof(0x0001) === UInt16
@assert typeof(0x00000001) === UInt32
@assert typeof(0x0000000000000001) === UInt64
@assert typeof(0b1) === UInt8
@assert typeof(0b1_00000000) === UInt16
@assert typeof(0o7) === UInt8
@assert typeof(0o400) === UInt16

# Range types preserve the typed-literal width.
@assert typeof(0x01:0x05) === UnitRange{UInt8}
@assert typeof(0x0001:0x000a) === UnitRange{UInt16}
@assert typeof(0x00000001:0x0000000a) === UnitRange{UInt32}
@assert typeof(0x0000000000000001:0x000000000000000a) === UnitRange{UInt64}

# Iteration yields elements of the typed width.
let observed = String[]
    for x in 0x01:0x03
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt8", "UInt8", "UInt8"]
end

let observed = String[]
    for x in 0x0001:0x0003
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt16", "UInt16", "UInt16"]
end

let observed = String[]
    for x in 0x00000001:0x00000003
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt32", "UInt32", "UInt32"]
end

let observed = String[]
    for x in 0x0000000000000001:0x0000000000000003
        push!(observed, string(typeof(x)))
    end
    @assert observed == ["UInt64", "UInt64", "UInt64"]
end

# Plain integer ranges still default to Int64.
@assert typeof(1:3) === UnitRange{Int64}
@assert typeof(first(1:3)) === Int64

@testset "Issue #3559 hex literal range element types" begin
    # Plain hex literal types.
    @test typeof(0x01) === UInt8
    @test typeof(0x0001) === UInt16
    @test typeof(0x00000001) === UInt32
    @test typeof(0x0000000000000001) === UInt64

    # Binary and octal literals follow the same width rules.
    @test typeof(0b1) === UInt8
    @test typeof(0b1_00000000) === UInt16
    @test typeof(0o7) === UInt8
    @test typeof(0o400) === UInt16

    # ── Hex ranges of varying widths ─────────────────────────────────────────
    @test typeof(0x01:0x05) === UnitRange{UInt8}
    @test typeof(0x0001:0x000a) === UnitRange{UInt16}
    @test typeof(0x00000001:0x0000000a) === UnitRange{UInt32}
    @test typeof(0x0000000000000001:0x000000000000000a) === UnitRange{UInt64}

    # Iteration variable preserves the typed element type.
    for x in 0x01:0x03
        @test typeof(x) === UInt8
    end
    for x in 0x0001:0x0003
        @test typeof(x) === UInt16
    end
    for x in 0x00000001:0x00000003
        @test typeof(x) === UInt32
    end

    # Plain integer ranges still default to Int64.
    @test typeof(1:3) === UnitRange{Int64}
end
end # module Agg_hex_literal_range_runtime_element_3559

# ===== source: type_inference/loop_inference.jl =====
module Agg_loop_inference
# Test: Loop variable type inference
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function sum_array(arr)
    total = 0
    for x in arr  # x should be inferred as Int64
        total += x
    end
    total
end

@testset "Loop variable type inference" begin
    @test sum_array([1, 2, 3]) == 6
    @test sum_array(Int64[]) == 0
end
end # module Agg_loop_inference

# ===== source: type_inference/multidim_getindex_inference.jl =====
module Agg_multidim_getindex_inference
# Test: Multi-dimensional getindex preserves index arity / kind (Issue #3529)
# Scalar index returns an element, while range/slice indexing returns an array.
using Test

function elem_of_matrix()
    m = reshape(collect(1:6), 2, 3)
    return m[1, 1]
end

function row_of_matrix()
    m = reshape(collect(1:6), 2, 3)
    return m[1, :]
end

function col_of_matrix()
    m = reshape(collect(1:6), 2, 3)
    return m[:, 1]
end

@testset "Multi-dim getindex inference" begin
    @test elem_of_matrix() == 1
    @test length(row_of_matrix()) == 3
    @test length(col_of_matrix()) == 2
end
end # module Agg_multidim_getindex_inference

# ===== source: type_inference/post_loop_fallthrough_runtime.jl =====
module Agg_post_loop_fallthrough_runtime
# Test: post-loop fallthrough runtime behaviour (Issue #3547).
# A loop body containing `return` may execute zero iterations, so the
# post-loop value must remain reachable. The function-return slot must
# accept the joined type — not the in-loop short-circuit type.
#
# Regression: previously the function's return slot was pinned to the
# in-loop `return i` type (I64). With n=0, the post-loop `"no iter"`
# string fell through and triggered a runtime "expected I64, got String"
# type error.

using Test

# Core repro: Int loop body return + String post-loop fallthrough.
function maybe_loop(n)
    for i in 1:n
        return i
    end
    "no iter"
end

# foreach variant.
function maybe_foreach(xs)
    for x in xs
        return x
    end
    "empty"
end

# while variant.
function maybe_while(go::Bool)
    while go
        return 42
    end
    "skipped"
end

# Nested: post-loop fallthrough is itself a string-typed expression.
function tail_string(n)
    for i in 1:n
        return i
    end
    s = "fall"
    s
end

@testset "Post-loop fallthrough runtime (Issue #3547)" begin
    @test maybe_loop(0) == "no iter"
    @test maybe_loop(3) == 1
    @test maybe_foreach(Int[]) == "empty"
    @test maybe_foreach([10, 20]) == 10
    @test maybe_while(false) == "skipped"
    @test maybe_while(true) == 42
    @test tail_string(0) == "fall"
    @test tail_string(2) == 1
end
end # module Agg_post_loop_fallthrough_runtime

# ===== source: type_inference/range_dict_set.jl =====
module Agg_range_dict_set
# Test: Range, Dict, Set type inference
using Test

# Functions must be defined OUTSIDE @testset block per project guidelines
function sum_range()
    total = 0
    for i in 1:10  # i should be Int64
        total += i
    end
    total
end

function process_dict()
    d = Dict("a" => 1, "b" => 2)
    result = 0
    for (k, v) in d  # k: String, v: Int64
        result += v
    end
    result
end

function process_set()
    s = Set([1, 2, 3])
    total = 0
    for x in s  # x: Int64
        total += x
    end
    total
end

@testset "Range type inference" begin
    @test sum_range() == 55
end

@testset "Dict type inference" begin
    @test process_dict() == 3
end

@testset "Set type inference" begin
    @test process_set() == 6
end
end # module Agg_range_dict_set

# ===== source: type_inference/recursive_call_inference.jl =====
module Agg_recursive_call_inference
# Test: Recursive calls produce concrete return types (Issue #3527)
# Previously the recursive edge poisoned inference to Top/Any. Now the
# fixpoint refines the recursive call to Int64 once the base case settles.
using Test

function fact(n::Int64)
    n <= 1 && return 1
    return n * fact(n - 1)
end

function fib(n::Int64)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

@testset "Recursive call inference" begin
    @test fact(5) == 120
    @test fact(10) == 3628800
    @test fib(0) == 0
    @test fib(1) == 1
    @test fib(10) == 55
end
end # module Agg_recursive_call_inference

# ===== source: type_inference/recursive_type_growth_4273.jl =====
module Agg_recursive_type_growth_4273
# Issue #4273: comparison-aware `limit_type_size` widening in lattice joins.
#
# A loop accumulator whose type nests one structural level deeper each
# iteration (e.g. `x = (x,)`, `a = [a]`) used to keep growing the inferred
# type until the absolute depth/length caps were hit — much later than
# upstream Julia, which bounds the growth as soon as the new type is more
# complex than the previous-iteration comparison type and widens it to its
# wrapper.
#
# These are runtime checks: the programs must still execute and produce the
# correct values. The point of the issue is that inference of these
# deeply-/recursively-nested accumulators stays *bounded* (no blow-up), while
# normal, non-growing accumulators are inferred exactly as before.

using Test

# ---------------------------------------------------------------------------
# (a) Tuple that nests one level deeper each iteration. Runtime values must be
# correct; inference must terminate (bounded) rather than chase unbounded
# tuple nesting.
# ---------------------------------------------------------------------------
function nest_tuple(n::Int)
    x = (0,)
    for _ in 1:n
        x = (x,)
    end
    return x
end

# Count how deeply a value is nested as `(inner,)` single-element tuples.
function nest_depth(x)
    d = 0
    while x isa Tuple && length(x) == 1 && x[1] isa Tuple
        d += 1
        x = x[1]
    end
    return d
end

# ---------------------------------------------------------------------------
# (b) Vector that wraps itself each iteration. Same recursive-growth shape via
# a different wrapper (Array). Runtime correctness is what we assert.
# ---------------------------------------------------------------------------
function nest_vector(n::Int)
    a = [1]
    for _ in 1:n
        a = [a]
    end
    return a
end

# ---------------------------------------------------------------------------
# (c) Control: a plain numeric accumulator must be unaffected by the new
# comparison-aware widening — it never grows structurally.
# ---------------------------------------------------------------------------
function plain_sum(n::Int)
    acc = 0
    for i in 1:n
        acc = acc + i
    end
    return acc
end

@testset "recursive type growth bounded (Issue #4273)" begin
    # Tuple nesting: values stay correct at small and larger depths.
    @test nest_tuple(0) === (0,)
    @test nest_tuple(1) === ((0,),)
    @test nest_depth(nest_tuple(3)) == 3
    @test nest_depth(nest_tuple(10)) == 10

    # Vector nesting executes and round-trips the innermost element.
    v = nest_vector(3)
    inner = v
    while inner isa Vector && length(inner) == 1 && inner[1] isa Vector
        inner = inner[1]
    end
    @test inner == [1]

    # Control accumulator unchanged.
    @test plain_sum(0) === 0
    @test plain_sum(10) === 55
end
end # module Agg_recursive_type_growth_4273

# ===== source: type_inference/typed_array_param_eltype_9133.jl =====
module Agg_typed_array_param_eltype_9133
# Regression fixture: `a::Vector{T}` / `a::Matrix{T}` parameter annotations
# must PRESERVE the element type through inference (Issue #9133).
#
# Before the fix, `julia_type_to_value_type_with_ctx` widened `VectorOf(T)` /
# `MatrixOf(T)` to the element-less `ValueType::Array`, so `a[i]` inferred
# unknown, loop accumulators widened to `Any`, and `length(a)` routed through
# a resolved Pure Julia call + `DynamicToI64` — the ANNOTATED function
# compiled to more dynamic dispatch than its un-annotated twin. The
# bytecode-level guards live in tests/annotation_inference_9121_tests.rs;
# this fixture pins the value-level semantics against upstream.

using Test

function sum_f64_9133(a::Vector{Float64})
    s = 0.0
    for i in 1:length(a)
        s = s + a[i]
    end
    return s
end

function sum_any_9133(a)
    s = 0.0
    for i in 1:length(a)
        s = s + a[i]
    end
    return s
end

function sum_i64_9133(a::Vector{Int64})
    s = 0
    for i in 1:length(a)
        s = s + a[i]
    end
    return s
end

function mat_trace_9133(m::Matrix{Float64})
    t = 0.0
    n = size(m, 1)
    for i in 1:n
        t += m[i, i]
    end
    return t
end

function first_elem_9133(a::Vector{Float64})
    return a[1] * 2.0
end

@testset "typed array parameter element type propagation (Issue #9133)" begin
    a = [1.0, 2.0, 3.0, 4.0, 5.0]
    @test sum_f64_9133(a) == 15.0
    @test typeof(sum_f64_9133(a)) == Float64
    @test sum_f64_9133(a) == sum_any_9133(a)
    @test length(a) == 5

    b = [1, 2, 3, 4]
    @test sum_i64_9133(b) == 10
    @test typeof(sum_i64_9133(b)) == Int64

    m = [1.0 2.0; 3.0 4.0]
    @test mat_trace_9133(m) == 5.0
    @test typeof(mat_trace_9133(m)) == Float64

    @test first_elem_9133(a) == 2.0
    @test typeof(first_elem_9133(a)) == Float64
end
end # module Agg_typed_array_param_eltype_9133

# ===== source: type_inference/typed_empty_array_widths.jl =====
module Agg_typed_empty_array_widths
# Test: Typed empty arrays preserve exact element widths (Issue #3532)
using Test

function int32_array_eltype()
    xs = Int32[]
    push!(xs, Int32(1))
    return eltype(xs)
end

function float32_array_eltype()
    xs = Float32[]
    push!(xs, 1.0f0)
    return eltype(xs)
end

function uint8_array_eltype()
    xs = UInt8[]
    push!(xs, 0x01)
    return eltype(xs)
end

@testset "Typed empty arrays preserve element widths" begin
    @test int32_array_eltype() == Int32
    @test float32_array_eltype() == Float32
    @test uint8_array_eltype() == UInt8
end
end # module Agg_typed_empty_array_widths

true
