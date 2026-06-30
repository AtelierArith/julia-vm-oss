# Issue #3593 — selectdim should return a view that shares storage with the
# parent matrix and preserves element type. Mutating the result must mutate
# the parent.

using Test

@testset "selectdim preserves Int64 element type and aliases parent" begin
    A = [1 2; 3 4]
    @test eltype(A) == Int64
    r = selectdim(A, 1, 1)

    # Element type preserved (not widened to Float64)
    @test r[1] == 1
    @test r[2] == 2
    @test typeof(r[1]) == Int64
    @test length(r) == 2

    # Mutation through view aliases parent
    r[1] = 99
    @test A[1, 1] == 99
    @test r[1] == 99
    @test typeof(A[1, 1]) == Int64

    # Untouched column row should be unchanged
    @test A[1, 2] == 2
    @test A[2, 1] == 3
    @test A[2, 2] == 4
end

@testset "selectdim dim=2 (column) aliases parent" begin
    A = [1 2 3; 4 5 6; 7 8 9]
    c = selectdim(A, 2, 1)
    @test length(c) == 3
    @test c[1] == 1
    @test c[2] == 4
    @test c[3] == 7

    # Mutate column through view
    c[2] = 100
    @test A[2, 1] == 100
end

@testset "selectdim preserves Float64 element type and aliases parent" begin
    A = [1.0 2.0; 3.0 4.0]
    r = selectdim(A, 1, 2)
    @test length(r) == 2
    @test r[1] == 3.0
    @test r[2] == 4.0

    r[1] = 99.0
    @test A[2, 1] == 99.0
    @test A[2, 2] == 4.0
end

@testset "selectdim preserves Bool element type and aliases parent" begin
    A = [true false; false true]
    @test eltype(A) == Bool
    r = selectdim(A, 1, 1)
    @test length(r) == 2
    @test r[1] == true
    @test r[2] == false

    r[1] = false
    @test A[1, 1] == false
end

true
