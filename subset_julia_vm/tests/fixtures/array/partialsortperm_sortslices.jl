# Test partialsortperm, partialsortperm!, sortslices

using Test

@testset "partialsortperm basic" begin
    arr = [3.0, 1.0, 4.0, 1.5, 2.0]
    # k=1: index of smallest element (1.0 at index 2)
    p1 = partialsortperm(arr, 1)
    @test p1[1] == 2

    # k=3: indices of 3 smallest elements in sorted order
    p3 = partialsortperm(arr, 3)
    @test p3[1] == 2  # 1.0
    @test p3[2] == 4  # 1.5
    @test p3[3] == 5  # 2.0
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
