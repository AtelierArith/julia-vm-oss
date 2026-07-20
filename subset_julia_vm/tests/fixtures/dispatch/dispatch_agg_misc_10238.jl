# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: dispatch/anonymous_typed_dispatch.jl =====
module Agg_anonymous_typed_dispatch
# Test method dispatch for anonymous typed parameters (::StructType)
# Issue #635: Method dispatch fails to distinguish struct subtypes

using Test

# Define abstract type hierarchy
abstract type AbstractIrrational <: Real end

# Define concrete struct types
struct IrrationalPi <: AbstractIrrational end
struct IrrationalCatalan <: AbstractIrrational end

# Define methods with anonymous typed parameters
# The :: syntax without parameter name should correctly parse the type
f(::IrrationalPi) = 1
f(::IrrationalCatalan) = 2

# Test that dispatch works correctly
@test f(IrrationalPi()) == 1
@test f(IrrationalCatalan()) == 2

# Test with multiple concrete types
struct PointA end
struct PointB end

g(::PointA) = "A"
g(::PointB) = "B"

@test g(PointA()) == "A"
@test g(PointB()) == "B"

# Test mixed named and anonymous parameters
h(x::Int64, ::PointA) = x + 1
h(x::Int64, ::PointB) = x + 2

@test h(10, PointA()) == 11
@test h(10, PointB()) == 12

# Return true to indicate success
end # module Agg_anonymous_typed_dispatch

# ===== source: dispatch/any_single_specific_methoderror_5984.jl =====
module Agg_any_single_specific_methoderror_5984
using Test

h_5984(x::String) = "got string: " * x
g_5984(x::Any) = h_5984(x)

@testset "Any static arg defers single specific method to runtime (Issue #5984)" begin
    @test g_5984("ok") == "got string: ok"
    @test_throws MethodError g_5984(42)
end
end # module Agg_any_single_specific_methoderror_5984

# ===== source: dispatch/array_dimension_dispatch.jl =====
module Agg_array_dimension_dispatch
using Test

rank_dispatch(x::Array{Int64, 1}) = 1
rank_dispatch(x::Array{Int64, 2}) = 2
rank_dispatch(x::Array{Int64, 3}) = 3

alias_dispatch(x::Vector{Int64}) = 10
alias_dispatch(x::Matrix{Int64}) = 20

# Workaround: force runtime dispatch for the 3D Array case because static call
# resolution drops Array rank for similar-created arrays (Issue #9642)
rank_dispatch_any(x::Any) = rank_dispatch(x)

@testset "Array dimension parameter dispatch" begin
    v = [1, 2, 3]
    m = [1 2; 3 4]
    a3 = similar(v, 1, 3, 1)

    @test rank_dispatch(v) == 1
    @test rank_dispatch(m) == 2
    @test rank_dispatch_any(a3) == 3

    @test alias_dispatch(v) == 10
    @test alias_dispatch(m) == 20
end
end # module Agg_array_dimension_dispatch

# ===== source: dispatch/bool_int_dispatch.jl =====
module Agg_bool_int_dispatch
# Test that Bool argument dispatches to Bool method, not Int64 method
# Issue #1441: Method dispatch prefers Integer over Bool

using Test

# Define overloaded functions with Bool and Int64 variants
function dispatch_test(b::Bool)
    return 1  # Bool version
end

function dispatch_test(n::Int64)
    return 2  # Int64 version
end

@testset "Bool vs Int64 dispatch" begin
    # Bool argument should dispatch to Bool method
    @test dispatch_test(true) == 1
    @test dispatch_test(false) == 1
    
    # Int64 argument should dispatch to Int64 method
    @test dispatch_test(42) == 2
    @test dispatch_test(0) == 2
end
end # module Agg_bool_int_dispatch

# ===== source: dispatch/broadcast_identity_materialization_4276.jl =====
module Agg_broadcast_identity_materialization_4276
using Test

double_broadcast_4276(x) = x + x

@testset "generic unary broadcast materializes typed arrays (Issue #4276)" begin
    ints = broadcast(identity, [1, 2])
    @test ints == [1, 2]
    @test typeof(ints) == Vector{Int64}
    @test eltype(ints) == Int64

    doubled = broadcast(double_broadcast_4276, [1, 2])
    @test doubled == [2, 4]
    @test typeof(doubled) == Vector{Int64}
    @test eltype(doubled) == Int64

    floats = broadcast(identity, [1.0, 2.0])
    @test floats == [1.0, 2.0]
    @test typeof(floats) == Vector{Float64}
    @test eltype(floats) == Float64
end
end # module Agg_broadcast_identity_materialization_4276

# ===== source: dispatch/complex_array_logical_dispatch_3908.jl =====
module Agg_complex_array_logical_dispatch_3908
using Test

complex_array_dispatch_3908(a::Vector{Complex{Float64}}) = :complex_vector
complex_array_dispatch_3908(a::Vector{Float64}) = :float_vector
complex_array_dispatch_3908(a) = :fallback

complex_array_dispatch_any_3908(a::Any) = complex_array_dispatch_3908(a)

@testset "complex array dispatch uses logical element type (Issue #3908)" begin
    zeros_complex = zeros(Complex{Float64}, 2)
    ones_complex = ones(Complex{Float64}, 2)
    erased = Any[zeros_complex]

    @test complex_array_dispatch_3908(zeros_complex) == :complex_vector
    @test complex_array_dispatch_any_3908(ones_complex) == :complex_vector
    @test complex_array_dispatch_3908(erased[1]) == :complex_vector
    @test complex_array_dispatch_3908([1.0, 2.0]) == :float_vector
end
end # module Agg_complex_array_logical_dispatch_3908

# ===== source: dispatch/dispatch_morespecific_partial_order_5926.jl =====
module Agg_dispatch_morespecific_partial_order_5926
using Test

# Issue #5926 (part of #5072): method specificity now consults the upstream-style
# `morespecific` *partial order* (the subtype-decidable fragment) before the
# integer specificity score. The score alone mis-ranks several real relations, so
# these calls previously dispatched to the LESS specific method or raised a
# spurious ambiguity `MethodError`. Each result matches upstream Julia.
#
# The dominance override is gated on the argument tuple being a *subtype* of the
# chosen method's signature, so an imprecise (e.g. statically-`Any`) argument
# still falls through to runtime dispatch — see the "imprecise argument" set,
# which guards against the codegen-coupling regression the gate prevents.

# --- container vs. its abstract supertype: Vector{T} ≺ AbstractVector ---
cv(::AbstractVector) = :abstract
cv(::Vector{T}) where {T} = :vector

cm(::AbstractMatrix) = :abstract
cm(::Matrix{T}) where {T} = :matrix

# --- diagonal Tuple{T,T} ≺ Tuple{Any,Any} ---
diag2(::T, ::T) where {T} = :diagonal
diag2(::Any, ::Any) = :anyany

# --- bounded where-params: Vector{<:Integer} ≺ Vector{<:Real} ---
bnd(::Vector{T}) where {T<:Real} = :real
bnd(::Vector{T}) where {T<:Integer} = :integer

# --- nested parametric: Vector{Vector{T}} ≺ Vector{T} ---
nst(::Vector{T}) where {T} = :outer
nst(::Vector{Vector{T}}) where {T} = :nested

# NOTE: invariant parametric *struct* args (e.g. `Pair{T,T}` vs `Pair{A,B}`) are
# intentionally NOT covered here. A `Pair` value's element types are not tracked
# at runtime (its dispatch type is the bare `Pair`), so the morespecific override
# cannot — and must not — commit to the diagonal; that case is governed by the
# pre-existing score tie-breaker, independent of this change. Same for `Dict`.

@testset "container vs abstract supertype picks the concrete container" begin
    @test cv([1, 2, 3]) === :vector
    @test cm([1 2; 3 4]) === :matrix
end

@testset "diagonal Tuple{T,T} is more specific than Tuple{Any,Any}" begin
    @test diag2(1, 2) === :diagonal            # same type -> diagonal wins
    @test diag2(1, "x") === :anyany            # different types -> only Any,Any matches
end

@testset "bounded where-param picks the tighter bound (was a spurious ambiguity)" begin
    @test bnd([1, 2, 3]) === :integer
    @test bnd(Real[1.0, 2.0]) === :real
end

@testset "nested parametric is more specific than the shallow one" begin
    @test nst([[1], [2, 3]]) === :nested
    @test nst([1, 2, 3]) === :outer
end

# --- imprecise-argument guard (codegen-coupling regression prevention) ---
# When the static argument type is too coarse to *definitively* select the more
# specific method, the override must defer to runtime dispatch rather than commit
# to (and have codegen lower) a method the value may not satisfy. A `::Any`
# fallback called through an abstractly-typed container element still resolves
# correctly at runtime.
spec(::Int) = :int
spec(::Any) = :any

@testset "imprecise argument still dispatches correctly (no over-commit)" begin
    xs = Any[1, "two", 3.0]          # element static type is Any
    @test spec(xs[1]) === :int       # runtime value is Int -> Int method
    @test spec(xs[2]) === :any       # runtime value is String -> Any method
    # A generic higher-order call over a concrete collection must still lower and
    # run (this is the shape that regressed in earlier attempts).
    @test map(x -> 2x + 3, collect(1:5))[end] === 13
end
end # module Agg_dispatch_morespecific_partial_order_5926

# ===== source: dispatch/number_array_dispatch_bug.jl =====
module Agg_number_array_dispatch_bug
# Test for Issue #1658: Number type should not match Array types in dispatch
# This bug occurs when an array is passed through a higher-order function to
# a method that expects Number

using Test

# Define a function that only works with numbers
function process(x::Number)
    return x + 1
end

# Define a higher-order wrapper that calls through a function variable
function call_func(f, arg)
    return f(arg)
end

@testset "Number type should not match Array in dynamic dispatch" begin
    # Test that passing Number works correctly
    @test call_func(process, 5) == 6
    @test call_func(process, 2.5) == 3.5

    # Note: Calling process with an array through call_func correctly throws
    # MethodError: no method matching process(Vector{Float64})
    # This is tested by the error handling tests separately
end
end # module Agg_number_array_dispatch_bug

# ===== source: dispatch/println_arg_stack_dispatch_3780.jl =====
module Agg_println_arg_stack_dispatch_3780
using Test

function dispatch_stack_side_effect()
    println("a")
    return 1
end

function dispatch_stack_ifelse(c, x, y)
    if c
        return x
    else
        return y
    end
end

@test dispatch_stack_ifelse(true, 1, dispatch_stack_side_effect()) == 1
@test dispatch_stack_ifelse(false, 1, dispatch_stack_side_effect()) == 1
end # module Agg_println_arg_stack_dispatch_3780

# ===== source: dispatch/struct_method_primitive_dispatch_5314.jl =====
module Agg_struct_method_primitive_dispatch_5314
# Adding a Struct-arg method must not break primitive-arg dispatch (Issue #5314)
#
# Extending a base function (min/max/isless/zero/oneunit) with a Struct-typed
# method previously broke dispatch for primitive arguments. The concrete struct
# parameter `::Q5314` was misclassified: its name (an uppercase letter followed
# by digits) is read by the context-free type layer as an unbounded type
# variable, so `::Q5314` matched ANY argument. A Float64 is not a subtype of
# Q5314, so for primitive arguments the struct method must be excluded and the
# base `[Any, Any]` method selected (matching upstream Julia for a
# `Base.`-qualified extension).

using Test

struct Q5314
    I
end
Base.min(a::Q5314, b::Q5314) = Q5314(a.I)
Base.max(a::Q5314, b::Q5314) = Q5314(b.I)
Base.isless(a::Q5314, b::Q5314) = a.I < b.I
Base.zero(a::Q5314) = Q5314(0)
Base.oneunit(a::Q5314) = Q5314(1)

@testset "struct method addition keeps primitive dispatch (Issue #5314)" begin
    # Primitive arguments reach the base methods (no AmbiguousMethod / mis-dispatch).
    @test min(1.0, 2.0) == 1.0
    @test max(1.0, 2.0) == 2.0
    @test isless(1.0, 2.0)
    @test zero(3.0) == 0.0
    @test oneunit(3.0) == 1.0
    @test min(3, 7) == 3

    # Struct arguments still use the user-defined methods.
    @test min(Q5314(5), Q5314(2)).I == 5
    @test max(Q5314(5), Q5314(2)).I == 2
    @test isless(Q5314(1), Q5314(2))
    @test zero(Q5314(9)).I == 0
    @test oneunit(Q5314(9)).I == 1
end
end # module Agg_struct_method_primitive_dispatch_5314

# ===== source: dispatch/type_any_specificity_4131.jl =====
module Agg_type_any_specificity_4131
using Test

function dispatch_type_any_specificity_f(::Type)
    1
end

function dispatch_type_any_specificity_f(::Type{Any})
    2
end

function dispatch_type_any_specificity_g(::Type)
    1
end

function dispatch_type_any_specificity_g(::Type{T}) where {T}
    2
end

function dispatch_type_any_specificity_h(::Type{Any})
    1
end

function dispatch_type_any_specificity_h(::Type{Int64})
    2
end

function dispatch_type_any_specificity_h(::Type)
    3
end

function dispatch_type_any_specificity_only_any(::Type{Any})
    4
end

@testset "Type{Any} method specificity (Issue #4131)" begin
    @test dispatch_type_any_specificity_f(Any) == 2
    @test dispatch_type_any_specificity_f(Int64) == 1

    @test dispatch_type_any_specificity_g(Any) == 2
    @test dispatch_type_any_specificity_g(Int64) == 2

    @test dispatch_type_any_specificity_h(Any) == 1
    @test dispatch_type_any_specificity_h(Int64) == 2
    @test dispatch_type_any_specificity_h(Float64) == 3

    @test dispatch_type_any_specificity_only_any(Any) == 4
    @test_throws MethodError dispatch_type_any_specificity_only_any(Int64)
    @test_throws MethodError dispatch_type_any_specificity_only_any(Symbol)
end
end # module Agg_type_any_specificity_4131

# ===== source: dispatch/typebound_strict_structhierarchy_6596.jl =====
module Agg_typebound_strict_structhierarchy_6596
# Issue #6596: `Type{<:Bound}` bound names that are user abstracts / parametric
# spellings must be judged through the struct hierarchy, not permissively
# accepted. Pins the parity points against upstream julia 1.12.
using Test

abstract type Animal end
struct Dog <: Animal end
struct Cat <: Animal end
struct Rock end

describe(::Type{<:Animal}) = "animal"
describe(::Type) = "generic"

only_animal(::Type{<:Animal}) = "ok"

struct Tree end
m(::Type{Tree}) = "exact-tree"
m(::Type{<:Animal}) = "animal"

classify_pairs(::Type{<:Base.Pairs}) = "pairs"
classify_pairs(::Type) = "other"

function rock_method_errors()
    try
        only_animal(Rock)
        return false
    catch e
        return e isa MethodError
    end
end

@testset "Type{<:Bound} strict struct hierarchy (Issue #6596)" begin
    # `Type{<:UserAbstract}` subtyping via the `<:` operator.
    @test (Type{Dog} <: Type{<:Animal}) == true
    @test (Type{Cat} <: Type{<:Animal}) == true
    # The strictening: an unrelated concrete type is NOT a subtype.
    @test (Type{Rock} <: Type{<:Animal}) == false
    # Bare user-abstract bound on the value-type `<:`.
    @test (Dog <: Animal) == true
    @test (Rock <: Animal) == false

    # Dispatch on Type{<:UserAbstract}: matches subtypes, falls through for others.
    @test describe(Dog) == "animal"
    @test describe(Cat) == "animal"
    @test describe(Rock) == "generic"

    # A single Type{<:Animal} method MethodErrors for a non-Animal type object.
    @test only_animal(Dog) == "ok"
    @test rock_method_errors() == true

    # Exact Type{T} stays more specific than the bound.
    @test m(Tree) == "exact-tree"
    @test m(Dog) == "animal"

    # Pairs-family parametric bound (`Type{<:Base.Pairs}`) still resolves.
    p = pairs((a = 1, b = 2))
    @test classify_pairs(typeof(p)) == "pairs"
    @test classify_pairs(Int) == "other"
end
end # module Agg_typebound_strict_structhierarchy_6596

true
