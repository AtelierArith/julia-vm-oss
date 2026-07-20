# Chained different-name where bound and `isa` (Issue #10410): for
# `r = Vector{T} where T<:S where S<:Real`, a member whose element type
# satisfies the transitive bound (`Float64 <: S <: Real`) must be `isa r`,
# matching the `Vector{Float64} <: r` answer. The subtype engine resolved the
# chain correctly all along; the bug was string-level on the `isa` route:
#   - `JuliaType::from_name`'s `Vector{...}`/`Tuple{...}` prefix arms
#     mis-split the rendered brace form `Vector{T} where {S<:Real, T<:S}`
#     into a garbage element type (`"T} where {S<:Real, T<:S"`);
#   - `normalize_type_for_isa` stripped the spaces of the top-level
#     ` where ` keyword (`MyWrap{T}where{S<:Real,T<:S}`), so the structured
#     re-parse dropped the whole where chain for user-struct targets.
# All expectations below were verified against upstream Julia 1.12.

using Test

struct ChainWrap10410{T} end

@testset "chained different-name where bound: isa matches <: (Issue #10410)" begin
    r = Vector{T} where T<:S where S<:Real
    @test string(r) == "Vector{T} where {S<:Real, T<:S}"
    @test Vector{Float64} <: r
    @test Float64[1.0] isa r
    @test Int64[1, 2] isa r
    @test !(Any[1, 2] isa r)
    @test !(["a"] isa r)
    # The literal (non-variable) spelling takes the same runtime path.
    @test Float64[1.0] isa (Vector{T} where T<:S where S<:Real)
end

@testset "chained where bound on a user parametric struct (Issue #10410)" begin
    r = ChainWrap10410{T} where T<:S where S<:Real
    @test ChainWrap10410{Float64} <: r
    @test ChainWrap10410{Float64}() isa r
    @test !(ChainWrap10410{String}() isa r)
end

@testset "single where bound on a user parametric struct (Issue #10410)" begin
    # Single-binder targets hit the same space-stripping normalization on the
    # struct-value isa route.
    r = ChainWrap10410{T} where T<:Real
    @test ChainWrap10410{Float64}() isa r
    @test !(ChainWrap10410{String}() isa r)
end

@testset "same-name chained where bound stays terminating (Issues #10274/#10302)" begin
    # The same-name nested-binder case must keep terminating (no unbounded
    # bound-resolution recursion) and keep its member answers.
    r = Vector{T} where T<:T where T<:Real
    @test Float64[1.0] isa r
    @test !(Any[1] isa r)
end

true
