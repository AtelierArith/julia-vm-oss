using Test

# Issue #8102: the default constructor of `struct Foo{T}; a::T; b::T; end` is
# `Foo(a::T, b::T) where {T}` — both arguments must share ONE concrete `T`.
# `Foo(1, 2.0)` therefore matches NO method (a single `T` cannot be both
# `Int64` and `Float64`) and upstream raises a `MethodError`; it must NOT widen
# to `Foo{Float64}`. Only the EXPLICIT `Foo{Float64}(1, 2.0)` form may convert.

struct Pt9_8102{T}; x::T; y::T; end
struct Foo9_8102{T}; a::T; b::Int; end          # non-`T` field is free
struct Bar9_8102{S, U}; a::S; b::U; end         # independent params never conflict

@testset "non-unifiable same-T default ctor is a MethodError (Issue #8102)" begin
    # The regression: mismatched `T` must MethodError, not widen.
    @test_throws MethodError Pt9_8102(1, 2.0)
    @test_throws MethodError Pt9_8102(2.0, 1)
    @test_throws MethodError Pt9_8102(Int8(1), 2)
end

@testset "legitimate same-T constructions still succeed" begin
    # Both arguments share one T.
    @test typeof(Pt9_8102(1, 2)) === Pt9_8102{Int64}
    @test typeof(Pt9_8102(1.0, 2.0)) === Pt9_8102{Float64}
    @test typeof(Bar9_8102(1, 1)) === Bar9_8102{Int64, Int64}
    @test Pt9_8102(1, 2).x === 1
    @test Pt9_8102(1.0, 2.0).y === 2.0
end

@testset "explicit {T} converts; independent params and free fields are fine" begin
    # Explicit `{T}` permits conversion (separate code path from inference).
    pc = Pt9_8102{Float64}(1, 2.0)
    @test pc.x === 1.0
    @test pc.y === 2.0
    @test typeof(pc) === Pt9_8102{Float64}

    # A `{T}` struct with a non-`T` field: only the `T` field is inferred.
    f = Foo9_8102(1.5, 2)
    @test f.a === 1.5
    @test f.b === 2
    @test typeof(f) === Foo9_8102{Float64}

    # Independent type parameters bind per field; no unification conflict.
    b = Bar9_8102(1, 2.0)
    @test typeof(b) === Bar9_8102{Int64, Float64}
    @test typeof(Bar9_8102("k", 5)) === Bar9_8102{String, Int64}
end

true
