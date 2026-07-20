using Test

@testset "scoped TypeVar identity separates same-name nested where binders (Issue #10279)" begin
    pattern = Tuple{T, Vector{T} where T} where T

    @test Tuple{Int64, Vector{Float64}} <: pattern
    @test Tuple{String, Vector{Int64}} <: pattern

    # The outer T still binds consistently outside the inner UnionAll.
    outer_reused = Tuple{T, T, Vector{T} where T} where T
    @test Tuple{Int64, Int64, Vector{Float64}} <: outer_reused
    @test !(Tuple{Int64, Float64, Vector{Int64}} <: outer_reused)
end

true
