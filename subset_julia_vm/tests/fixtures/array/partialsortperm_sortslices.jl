# Test partialsortperm, partialsortperm!, sortslices

using Test

@testset "partialsortperm basic (Issue #5745)" begin
    arr = [3.0, 1.0, 4.0, 1.5, 2.0]
    # Integer k returns a single index (the k-th order statistic), not the
    # whole permutation.
    @test partialsortperm(arr, 1) == 2   # 1.0 at index 2
    @test partialsortperm(arr, 3) == 5   # 2.0 at index 5 (3rd smallest)
    @test partialsortperm(arr, 1) isa Integer

    # A range k returns the vector of indices for those order statistics.
    @test partialsortperm(arr, 1:3) == [2, 4, 5]   # 1.0, 1.5, 2.0
    @test partialsortperm(arr, 2:4) == [4, 5, 1]   # 1.5, 2.0, 3.0
end

@testset "partialsortperm! in-place" begin
    arr = [5.0, 2.0, 8.0, 1.0]
    perm = collect(1:4)
    partialsortperm!(perm, arr, 2)
    @test perm[1] == 4  # 1.0
    @test perm[2] == 2  # 2.0
end

@testset "sortslices dims=1 (sort rows)" begin
    A = [3.0 1.0; 1.0 2.0; 2.0 3.0]
    S = sortslices(A; dims=1)
    # Rows sorted lexicographically: [1,2], [2,3], [3,1]
    @test S[1, 1] == 1.0
    @test S[1, 2] == 2.0
    @test S[2, 1] == 2.0
    @test S[2, 2] == 3.0
    @test S[3, 1] == 3.0
    @test S[3, 2] == 1.0
end

@testset "sortslices dims=2 (sort columns)" begin
    A = [3.0 1.0 2.0; 4.0 2.0 3.0]
    S = sortslices(A; dims=2)
    # Columns sorted lexicographically: [1,2], [2,3], [3,4]
    @test S[1, 1] == 1.0
    @test S[2, 1] == 2.0
    @test S[1, 2] == 2.0
    @test S[2, 2] == 3.0
    @test S[1, 3] == 3.0
    @test S[2, 3] == 4.0
end

true
