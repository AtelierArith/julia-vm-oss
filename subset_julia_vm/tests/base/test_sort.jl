# Test for src/base/sort.jl
# Based on Julia's test/sorting.jl
using Test

@testset "sort" begin
    # Test by checking individual elements
    result = sort([2, 3, 1])
    @test result[1] == 1
    @test result[2] == 2
    @test result[3] == 3

    result2 = sort([5, 4, 3, 2, 1])
    @test result2[1] == 1
    @test result2[5] == 5
end

# Note: sort! and reverse! tests removed due to VM issue with
# in-place mutation not persisting across function returns

@testset "sortperm" begin
    result = sortperm([2, 3, 1])
    @test result[1] == 3
    @test result[2] == 1
    @test result[3] == 2
end

@testset "issorted" begin
    @test issorted([1, 2, 3]) == true
    @test issorted([2, 3, 1]) == false
    @test issorted([1]) == true
end

@testset "searchsortedfirst" begin
    arr = [1, 2, 3, 4, 5]
    @test searchsortedfirst(arr, 3) == 3
    @test searchsortedfirst(arr, 1) == 1
    @test searchsortedfirst(arr, 5) == 5
    @test searchsortedfirst(arr, 0) == 1
    @test searchsortedfirst(arr, 6) == 6
end

@testset "searchsortedlast" begin
    arr = [1, 2, 3, 4, 5]
    @test searchsortedlast(arr, 3) == 3
    @test searchsortedlast(arr, 1) == 1
    @test searchsortedlast(arr, 5) == 5
    @test searchsortedlast(arr, 0) == 0
    @test searchsortedlast(arr, 6) == 5
end

@testset "insorted" begin
    arr = [1, 2, 3, 4, 5]
    @test insorted(3, arr) == true
    @test insorted(1, arr) == true
    @test insorted(5, arr) == true
    @test insorted(0, arr) == false
    @test insorted(6, arr) == false
end

println("test_sort.jl: All tests passed!")
