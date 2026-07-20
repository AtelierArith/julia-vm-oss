# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: type_inference/bounds_check_elision_5089.jl =====
module Agg_bounds_check_elision_5089
using Test

function collect_eachindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in eachindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in 1:length(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_base_eachindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.eachindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_base_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in 1:Base.length(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_lastindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in 1:lastindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_first_lastindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in firstindex(arr):lastindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_base_first_lastindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.firstindex(arr):Base.lastindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_axes_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in axes(arr, 1)
        push!(out, arr[i])
    end
    return out
end

function collect_base_axes_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.axes(arr, 1)
        push!(out, arr[i])
    end
    return out
end

function collect_base_oneto_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.OneTo(length(arr))
        push!(out, arr[i])
    end
    return out
end

function collect_base_oneto_function_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.oneto(length(arr))
        push!(out, arr[i])
    end
    return out
end

function collect_direct_getindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in eachindex(arr)
        push!(out, getindex(arr, i))
    end
    return out
end

function collect_base_getindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in eachindex(arr)
        push!(out, Base.getindex(arr, i))
    end
    return out
end

function increment_eachindex_store_inbounds_5089(arr::Vector{Float64})
    for i in eachindex(arr)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_length_store_inbounds_5089(arr::Vector{Float64})
    for i in 1:length(arr)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_axes_store_inbounds_5089(arr::Vector{Float64})
    for i in axes(arr, 1)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_base_axes_store_inbounds_5089(arr::Vector{Float64})
    for i in Base.axes(arr, 1)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_base_oneto_lastindex_store_inbounds_5089(arr::Vector{Float64})
    for i in Base.OneTo(Base.lastindex(arr))
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_base_oneto_function_lastindex_store_inbounds_5089(arr::Vector{Float64})
    for i in Base.oneto(Base.lastindex(arr))
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_eachindex_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in eachindex(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

function increment_length_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in 1:length(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

function increment_base_lastindex_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in 1:Base.lastindex(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

function increment_first_lastindex_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in firstindex(arr):lastindex(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

@testset "bounds-check elision proof patterns (Issue #5089)" begin
    values = Int32[10, 20, 30]
    @test collect_eachindex_inbounds_5089(values) == values
    @test collect_length_inbounds_5089(values) == values
    @test collect_base_eachindex_inbounds_5089(values) == values
    @test collect_base_length_inbounds_5089(values) == values
    @test collect_lastindex_inbounds_5089(values) == values
    @test collect_first_lastindex_inbounds_5089(values) == values
    @test collect_base_first_lastindex_inbounds_5089(values) == values
    @test collect_axes_inbounds_5089(values) == values
    @test collect_base_axes_inbounds_5089(values) == values
    @test collect_base_oneto_length_inbounds_5089(values) == values
    @test collect_base_oneto_function_length_inbounds_5089(values) == values
    @test collect_direct_getindex_inbounds_5089(values) == values
    @test collect_base_getindex_inbounds_5089(values) == values

    @test increment_eachindex_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_length_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_axes_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_base_axes_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_base_oneto_lastindex_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_base_oneto_function_lastindex_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_eachindex_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
    @test increment_length_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
    @test increment_base_lastindex_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
    @test increment_first_lastindex_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
end
end # module Agg_bounds_check_elision_5089

# ===== source: type_inference/builtin_type_dispatch.jl =====
module Agg_builtin_type_dispatch
# Test that builtin type names are typed correctly even when they have methods
# This ensures proper dispatch for functions like nameof(t::Type)
# Related to Issue #1692 and #1701: Type inference order for builtin type names vs method_tables

using Test

# Define test functions for type vs function dispatch
function test_type_dispatch(t::Type)
    return :type
end

function test_type_dispatch(f::Function)
    return :function
end

@testset "Builtin type inference priority" begin
    # Test nameof for builtin types
    # These types have methods defined but should still dispatch to Type{T}
    @test nameof(Tuple) == :Tuple
    @test nameof(Array) == :Array
    @test nameof(Dict) == :Dict
    @test nameof(Int64) == :Int64
    @test nameof(Float64) == :Float64
    @test nameof(String) == :String

    # Test that type dispatch works correctly for builtin types
    # Tuple has methods (e.g., Tuple(ci::CartesianIndex)) but should be typed as TypeOf(Tuple)
    @test test_type_dispatch(Tuple) == :type
    @test test_type_dispatch(Array) == :type
    @test test_type_dispatch(Int64) == :type
    @test test_type_dispatch(Float64) == :type

    # Test that function dispatch works correctly for actual functions
    @test test_type_dispatch(sin) == :function
    @test test_type_dispatch(cos) == :function
end
end # module Agg_builtin_type_dispatch

# ===== source: type_inference/const_specialization_parity_4272.jl =====
module Agg_const_specialization_parity_4272
# Observable parity for the shared const-specialization policy (Issue #4272).
#
# The compile-side inference cache key and the AoT specialization key now derive
# their preserve-vs-widen decision from one shared predicate
# (`const_specialization` / `is_const_eligible`). This fixture exercises the
# value classes the policy treats specially -- Bool, Symbol, Nothing, and small
# Int -- to confirm const-influenced inference still produces correct runtime
# results across the pipeline (and matches upstream Julia).

using Test

# --- Helpers (OUTSIDE @testset per project guidelines) ---

# Bool: const flag selects a branch with a different result type.
flagged(flag) = flag ? 1 : 1.0

# Symbol: const symbol acts as a field selector / dispatch tag.
select(nt, name) = getfield(nt, name)

# Nothing: const `nothing` participates in a `=== nothing` test.
or_default(x) = x === nothing ? 0 : x

# Small Int: const small integer drives a branch.
classify(n) = n == 0 ? :zero : (n == 1 ? :one : :many)

@testset "const-specialization parity (#4272)" begin
    # Bool const branch selection preserves per-branch result types.
    @test flagged(true) === 1
    @test flagged(false) === 1.0

    # Symbol const field selection.
    nt = (a = 10, b = 20)
    @test select(nt, :a) === 10
    @test select(nt, :b) === 20

    # Nothing const singleton handling.
    @test or_default(nothing) === 0
    @test or_default(42) === 42

    # Small-int const dispatch.
    @test classify(0) === :zero
    @test classify(1) === :one
    @test classify(7) === :many
end
end # module Agg_const_specialization_parity_4272

# ===== source: type_inference/function_chain_inference.jl =====
module Agg_function_chain_inference
# Function chain type inference test
# Tests inter-procedural type propagation through function call chains

using Test

# Helper function that performs addition
function helper_add(x, y)
    return x + y
end

# Helper function that doubles a value
function helper_double(x)
    return x * 2
end

# Caller function that chains multiple helper calls
function caller_chain(a, b)
    sum_result = helper_add(a, b)
    doubled = helper_double(sum_result)
    return doubled
end

# Function with type-dependent return (polymorphic)
function identity_int(x)
    return x
end

function identity_float(x)
    return x
end

@testset "Inter-procedural function call chain type inference" begin
    # Test basic function chain with Int64
    @test caller_chain(1, 2) == 6  # (1+2)*2 = 6

    # Test basic function chain with Float64
    @test caller_chain(1.0, 2.0) == 6.0

    # Test helper functions directly
    @test helper_add(10, 20) == 30
    @test helper_double(5) == 10

    # Test polymorphic identity functions with numeric types
    @test identity_int(42) == 42
    @test identity_int(100) == 100
    @test identity_float(3.14) == 3.14
    @test identity_float(2.71) == 2.71

    # Test nested function calls
    @test helper_add(helper_double(2), helper_double(3)) == 10  # 4 + 6 = 10

    # Test multiple chained calls
    @test helper_double(helper_double(helper_double(1))) == 8  # 1*2*2*2 = 8
end
end # module Agg_function_chain_inference

# ===== source: type_inference/letblock_under_expression.jl =====
module Agg_letblock_under_expression
# Locals introduced inside a begin/let block under expression contexts
# (binary op, tuple, array, index) must be collected for inference.
# Issue #3537

using Test

function f3537_binop()
    y = (begin
        x = 41
        x
    end) + 1
    return y
end

function f3537_tuple()
    t = (begin
        a = 10
        a
    end, begin
        b = 20
        b
    end)
    return t
end

function f3537_array()
    arr = [begin
        z = 7
        z
    end]
    return arr[1]
end

function f3537_unary()
    y = -(begin
        w = 5
        w
    end)
    return y
end

@testset "LetBlock locals nested in expression positions" begin
    @test f3537_binop() == 42
    @test f3537_tuple() == (10, 20)
    @test f3537_array() == 7
    @test f3537_unary() == -5
end
end # module Agg_letblock_under_expression

# ===== source: type_inference/limit_type_size_3507.jl =====
module Agg_limit_type_size_3507
# Issue #3507: Replace fixed union widening with Julia-inspired type-size
# limiting (`limit_type_size`). The widener must (a) keep small/canonical
# unions like `Union{T, Nothing}` intact, (b) widen large heterogeneous
# accumulations to a sound abstract supertype rather than `Top`, and (c)
# never widen short unions to a less precise shape unnecessarily.
#
# These are runtime checks against the values produced by the inference
# engine; the goal is to demonstrate that programs which previously
# triggered the fixed-length widener still execute correctly under the
# new comparison-aware policy.

using Test

# ---------------------------------------------------------------------------
# (a) Small `Union{T, Nothing}` — must round-trip through inference and stay
# small enough to dispatch correctly.
# ---------------------------------------------------------------------------
function nullable_passthrough(x::Int)
    if x > 0
        return x
    end
    return nothing
end

# ---------------------------------------------------------------------------
# (b) Loop accumulator returning increasingly heterogeneous integer types.
# Each branch returns a different numeric width; with a hard length cap
# this would have collapsed early, but with comparison-aware widening the
# fall-back is the abstract `Integer` supertype, which still supports
# arithmetic.
# ---------------------------------------------------------------------------
function mixed_int_loop(n::Int)
    acc = 0
    for i in 1:n
        if i % 4 == 0
            acc = acc + Int8(1)
        elseif i % 4 == 1
            acc = acc + Int16(1)
        elseif i % 4 == 2
            acc = acc + Int32(1)
        else
            acc = acc + Int64(1)
        end
    end
    return acc
end

# ---------------------------------------------------------------------------
# (c) Tail-vararg-shaped tuple — depth 4 of identical Int64 elements.
# Inference must not widen this tuple to `Tuple{Any, Any, ...}`.
# ---------------------------------------------------------------------------
function fixed_tuple()
    return (1, 2, 3, 4)
end

@testset "limit_type_size (Issue #3507)" begin
    # Nullable kept narrow.
    @test nullable_passthrough(1) === 1
    @test nullable_passthrough(-1) === nothing

    # Mixed-int loop produces an Integer-typed accumulator at runtime.
    @test mixed_int_loop(0) === 0
    @test mixed_int_loop(8) isa Integer

    # Fixed-shape tuple preserved.
    t = fixed_tuple()
    @test length(t) == 4
    @test t[1] === 1
    @test t[4] === 4
end
end # module Agg_limit_type_size_3507

# ===== source: type_inference/parametric_ctor_resolution_5922.jl =====
module Agg_parametric_ctor_resolution_5922
# Test: parametric constructor return-type resolution (Issue #5922 wave 5)
# Pins the constructor-inference families migrated to the tfuncs adapter's
# StructInstantiation seam: parametric struct ctor (inferred type args +
# on-demand instantiation), `{`-instantiated ctor names (Val{N}(), T{P}()),
# and the parametric Rational ctor. The Dict non-builtin-pattern fallback is
# pinned by unit tests only: non-builtin Dict(...) argument shapes are not
# yet compilable end-to-end (Issue #6531).
using Test

struct CtorPoint5922{T}
    x::T
    y::T
end

@testset "parametric ctor resolution (Issue #5922)" begin
    # Parametric struct ctor: type args inferred from arguments.
    p = CtorPoint5922(1, 2)
    @test p.x + p.y == 3
    @test p isa CtorPoint5922{Int64}

    pf = CtorPoint5922(1.5, 2.5)
    @test pf isa CtorPoint5922{Float64}
    @test pf.y == 2.5

    # Array of parametric struct instances keeps the concrete element type.
    arr = [CtorPoint5922(1.0, 2.0), CtorPoint5922(3.0, 4.0)]
    @test arr[2].y == 4.0
    @test eltype(arr) == CtorPoint5922{Float64}

    # `{`-instantiated ctor names.
    v = Val{2}()
    @test v isa Val{2}
    pe = CtorPoint5922{Int64}(7, 8)
    @test pe isa CtorPoint5922{Int64}
    @test pe.x == 7

    # Parametric Rational ctor.
    r = Rational(1, 2)
    @test r + r == 1//1
    @test r isa Rational{Int64}
end
end # module Agg_parametric_ctor_resolution_5922

# ===== source: type_inference/promote_type_nested_typeobject_dispatch_9914.jl =====
module Agg_promote_type_nested_typeobject_dispatch_9914
using Test

nested_promote_float_9914(a::T, b::S) where {T<:Real,S<:Real} =
    float(promote_type(T, S))

bound_promote_float_9914(a::T, b::S) where {T<:Real,S<:Real} = begin
    P = promote_type(T, S)
    float(P)
end

@testset "nested promote_type type-object dispatch (Issue #9914)" begin
    @test nested_promote_float_9914(0.0, 1.0) === Float64
    @test nested_promote_float_9914(1, 2) === Float64
    @test nested_promote_float_9914(Float32(0), Float32(1)) === Float32

    @test nested_promote_float_9914(0.0, 1.0) === bound_promote_float_9914(0.0, 1.0)
    # Reflection precision for float(::Type) is tracked separately by Issue #9955.
end
end # module Agg_promote_type_nested_typeobject_dispatch_9914

# ===== source: type_inference/tuple_length_const_5142.jl =====
module Agg_tuple_length_const_5142
using Test

# Issue #5142: the length of a fixed-arity tuple is a statically known value.
# Inference now propagates it as Const(N) (mirroring upstream `nfields_tfunc`),
# which enables constant folding and branch elimination while keeping the
# observable runtime result identical to upstream Julia.

# Length of a known-arity tuple.
tuple_len_3() = length((1, 2.0, "three"))

# Constant folding on top of a known tuple length: the whole expression is a
# compile-time constant, but must still evaluate to the same runtime value.
tuple_len_plus_one() = length((10, 20)) + 1

# Branch selected by a known tuple length. The condition is statically known,
# so dead branches can be eliminated; the chosen branch must still run.
function classify_pair(t)
    if length(t) == 2
        return :pair
    else
        return :other
    end
end

# Empty tuple has length 0.
empty_tuple_len() = length(())

@testset "tuple length constant propagation (Issue #5142)" begin
    @test tuple_len_3() == 3
    @test tuple_len_plus_one() == 3
    @test classify_pair((1, 2)) == :pair
    @test classify_pair((1, 2, 3)) == :other
    @test empty_tuple_len() == 0

    # length of a literal tuple used directly in an expression.
    @test length((1, 2, 3, 4)) == 4
    @test ntuple(identity, 3) == (1, 2, 3)
    @test length(ntuple(identity, 3)) == 3
end
end # module Agg_tuple_length_const_5142

# ===== source: type_inference/typed_assignment_annotation_9121.jl =====
module Agg_typed_assignment_annotation_9121
# Regression fixture: a type annotation on a local (`x::Float64 = f()`),
# a typed const global (`const B::Float64 = 2.0`), or a typed global must
# PRESERVE the declared type through inference, never degrade it (Issue #9121,
# sibling #9132).
#
# `x::T = rhs` lowers to `x = convert(T, rhs)`; before the fix the compile-time
# type oracle inferred that convert call as `Any`, so the annotated version of
# a function compiled `x * 2.0` to dynamic dispatch while the un-annotated
# version compiled `MulF64` — a WORSE result for MORE type information. The
# bytecode-level guards live in tests/annotation_inference_9121_tests.rs; this
# fixture pins the value-level semantics (results and types match upstream).

using Test

function get_x_9121()::Float64
    return 3.0
end

function annot_local_9121()
    x::Float64 = get_x_9121()
    return x * 2.0
end

function no_annot_local_9121()
    x = get_x_9121()
    return x * 2.0
end

function annot_two_uses_9121()
    x::Float64 = get_x_9121()
    y = x * 2.0
    return y + x
end

function annot_int_9121()
    n::Int64 = 5
    return n * 3
end

# Annotation coerces the RHS (Int literal -> Float64), matching upstream.
function annot_coerce_9121()
    v::Float64 = 1
    return v + 0.5
end

const B_9121::Float64 = 2.0

function use_typed_const_9121(n)
    s = 0.0
    for i in 1:n
        s += i * B_9121
    end
    return s
end

@testset "typed assignment annotation preserves inference (Issue #9121/#9132)" begin
    @test annot_local_9121() == 6.0
    @test typeof(annot_local_9121()) == Float64
    @test annot_local_9121() == no_annot_local_9121()
    @test annot_two_uses_9121() == 9.0
    @test typeof(annot_two_uses_9121()) == Float64
    @test annot_int_9121() == 15
    @test typeof(annot_int_9121()) == Int64
    @test annot_coerce_9121() == 1.5
    @test typeof(annot_coerce_9121()) == Float64
    @test B_9121 === 2.0
    @test use_typed_const_9121(10) == 110.0
    @test typeof(use_typed_const_9121(10)) == Float64
    # InexactError parity: annotation conversion must still error like upstream.
    @test_throws InexactError (() -> begin
        m::Int64 = 1.5
        m
    end)()
end
end # module Agg_typed_assignment_annotation_9121

# ===== source: type_inference/varargs_interproc_inference.jl =====
module Agg_varargs_interproc_inference
# Test: Interprocedural inference packs varargs parameters as Tuples (Issue #3526)
# A user-defined varargs function should be analyzed with `xs` bound as
# `Tuple{Int64, Int64, Int64}` so that `for x in xs` and `s += x` infer Int64.
using Test

sum_varargs(xs...) = begin
    s = 0
    for x in xs
        s += x
    end
    s
end

function call_sum_varargs()
    sum_varargs(1, 2, 3)
end

function call_sum_varargs_one()
    sum_varargs(42)
end

function call_sum_varargs_zero()
    sum_varargs()
end

@testset "Varargs interprocedural inference" begin
    @test call_sum_varargs() == 6
    @test call_sum_varargs_one() == 42
    @test call_sum_varargs_zero() == 0
end
end # module Agg_varargs_interproc_inference

true
