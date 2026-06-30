# Issue #4819: Any[expr for x in iter] raised
# `ErrorException: Unknown function: Any` because the typed
# comprehension lowering at `wrap_comprehension_body_with_call`
# wrapped the body in a `T(x)` call regardless of `T`. `Any(x)` is
# not a defined Julia constructor.
#
# Fix: special-case `T == "Any"` in the wrap helper to route the
# entire comprehension through `Vector{Any}(...)`, which hits the
# Vector{Any} compile intercept added in #4818 (PR #4820) and
# produces a `Vector{Any}` with each element boxed.
#
# Multi-iter `Any[expr for i in R1, j in R2]` produces a flat
# `Vector{Any}` (not `Matrix{Any}`) because the underlying multi-
# comprehension shape bug is tracked separately as #4779; this
# fixture only asserts the eltype, not the dimensionality, for that
# case.

using Test

@testset "Any[x for x in Vector{Int}] (Issue #4819)" begin
    v = Any[x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test length(v) == 3
    @test v[1] == 1
    @test v[3] == 3
end

@testset "Any[x for x in Vector{Float}] (Issue #4819)" begin
    v = Any[x for x in [1.0, 2.0, 3.0]]
    @test typeof(v) === Vector{Any}
    @test v[1] == 1.0
end

@testset "Any[x for x in 1:3] range source (Issue #4819)" begin
    v = Any[x for x in 1:3]
    @test typeof(v) === Vector{Any}
    @test v == [1, 2, 3]
end

@testset "Any[expr for x in iter] non-identity body (Issue #4819)" begin
    v = Any[x * 2 for x in [1, 2, 3]]
    @test typeof(v) === Vector{Any}
    @test v == [2, 4, 6]
end

@testset "Any[x for x in iter if cond] with filter (Issue #4819)" begin
    v = Any[x for x in [1, 2, 3, 4] if x > 2]
    @test typeof(v) === Vector{Any}
    @test v == [3, 4]
end

@testset "Any[x for x in empty range] empty result (Issue #4819)" begin
    v = Any[x for x in 1:0]
    @test typeof(v) === Vector{Any}
    @test length(v) == 0
end

@testset "Float64[x for x in arr] regression — non-Any T still wraps (Issue #4819)" begin
    # Make sure the non-Any path of wrap_comprehension_body_with_call
    # still wraps the body in the conversion call.
    v = Float64[x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end

true
