using Test

# Regression test for Issue #3589:
# `rotl90([1 2; 3 4])` previously returned `Matrix{Float64}` because the
# implementation pre-allocated `result = zeros(n, m)`. The same defect
# affected `rotr90` and `rot180`. Per the #3589 acceptance criteria, all
# three should preserve the element type.
#
# Implementation: dispatch-based specialization. Matrix literals like
# `[1 2; 3 4]` infer as `Vector{T}` at compile time even though the
# runtime type is `Matrix{T}`, so methods are declared on `Vector{T}`
# and seed the flat buffer with `T[]`. push! + reshape preserves the
# element type. Generic fallback returns `Matrix{Any}` because pure-Julia
# `similar(mat, n, m)` is blocked by Issue #3648.

@testset "rotl90 preserves Matrix{Int64} (#3589)" begin
    m = [1 2; 3 4]
    r = rotl90(m)
    @test r == [2 4; 1 3]
    @test typeof(r) === Matrix{Int64}

    # Non-square 3x4
    m3 = [1 2 3 4; 5 6 7 8; 9 10 11 12]
    r3 = rotl90(m3)
    @test r3 == [4 8 12; 3 7 11; 2 6 10; 1 5 9]
    @test typeof(r3) === Matrix{Int64}
    @test size(r3) == (4, 3)
end

@testset "rotr90 preserves Matrix{Int64}" begin
    m = [1 2; 3 4]
    r = rotr90(m)
    @test r == [3 1; 4 2]
    @test typeof(r) === Matrix{Int64}
end

@testset "rot180 preserves Matrix{Int64}" begin
    m = [1 2; 3 4]
    r = rot180(m)
    @test r == [4 3; 2 1]
    @test typeof(r) === Matrix{Int64}
end

@testset "rotl90 preserves Matrix{Bool}" begin
    m = [true false; false true]
    r = rotl90(m)
    @test r == [false true; true false]
    @test typeof(r) === Matrix{Bool}
end

@testset "rotation regressions for Matrix{Float64}" begin
    m = [1.0 2.0; 3.0 4.0]
    r1 = rotl90(m)
    r2 = rotr90(m)
    r3 = rot180(m)
    @test r1[1, 1] == 2.0
    @test r2[1, 1] == 3.0
    @test r3[1, 1] == 4.0
    @test typeof(r1) === Matrix{Float64}
    @test typeof(r2) === Matrix{Float64}
    @test typeof(r3) === Matrix{Float64}
end

@testset "rot composition still cancels (regression)" begin
    m = [1 2; 3 4]
    @test rotr90(rotl90(m)) == m
    @test rot180(rot180(m)) == m
end

true
