# Issue #3594 — dropdims should return a view that shares storage with the
# parent matrix and preserves element type. Mutating the result must mutate
# the parent.

using Test

@testset "dropdims preserves Int64 element type and aliases parent" begin
    # 1×3 Int matrix → drop dim 1 → 1D view of length 3
    A = reshape([1, 2, 3], 1, 3)
    @test eltype(A) == Int64
    r = dropdims(A, dims=1)

    # Element type must be preserved (not widened to Float64)
    @test r[1] == 1
    @test r[2] == 2
    @test r[3] == 3
    @test typeof(r[1]) == Int64
    @test length(r) == 3

    # Mutation through the view must alias the parent matrix
    r[1] = 99
    @test A[1, 1] == 99
    @test r[1] == 99
    @test typeof(A[1, 1]) == Int64
end

@testset "dropdims dims=2 aliases parent column" begin
    # 3×1 Int matrix → drop dim 2 → 1D view of length 3
    A = reshape([10, 20, 30], 3, 1)
    @test eltype(A) == Int64
    r = dropdims(A, dims=2)
    @test length(r) == 3
    @test r[1] == 10
    @test r[2] == 20
    @test r[3] == 30

    # Mutate through view, observe in parent
    r[2] = 200
    @test A[2, 1] == 200
end

@testset "dropdims preserves Float64 element type and aliases parent" begin
    A = reshape([1.5, 2.5, 3.5, 4.5], 1, 4)
    r = dropdims(A, dims=1)
    @test length(r) == 4
    @test r[1] == 1.5
    @test r[4] == 4.5

    r[3] = 99.0
    @test A[1, 3] == 99.0
end

@testset "dropdims preserves Bool element type and aliases parent" begin
    A = reshape([true, false, true], 1, 3)
    @test eltype(A) == Bool
    r = dropdims(A, dims=1)
    @test length(r) == 3
    @test r[1] == true
    @test r[2] == false
    @test r[3] == true

    r[2] = true
    @test A[1, 2] == true
end

true
