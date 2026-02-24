# Test for src/base/array.jl
# Based on Julia's test/arrayops.jl
using Test
using Statistics

@testset "prod" begin
    @test prod([1, 2, 3, 4]) == 24
    @test prod([2, 3, 5]) == 30
    @test prod([1]) == 1
    @test prod([1, 1, 1, 1]) == 1
end

@testset "minimum and maximum" begin
    @test minimum([3, 1, 4, 1, 5]) == 1
    @test minimum([5, 4, 3, 2, 1]) == 1
    @test minimum([42]) == 42
    @test minimum([-1, 0, 1]) == -1

    @test maximum([3, 1, 4, 1, 5]) == 5
    @test maximum([1, 2, 3, 4, 5]) == 5
    @test maximum([42]) == 42
    @test maximum([-1, 0, 1]) == 1
end

@testset "argmin and argmax" begin
    @test argmin([3, 1, 4, 1, 5]) == 2
    @test argmin([5, 4, 3, 2, 1]) == 5
    @test argmin([1]) == 1

    @test argmax([3, 1, 4, 1, 5]) == 5
    @test argmax([1, 2, 3, 4, 5]) == 5
    @test argmax([1]) == 1
end

@testset "reverse" begin
    r = reverse([1, 2, 3, 4, 5])
    @test r[1] == 5
    @test r[2] == 4
    @test r[3] == 3
    @test r[4] == 2
    @test r[5] == 1

    r2 = reverse([1, 2, 3])
    @test r2[1] == 3
    @test r2[3] == 1
end

@testset "issorted" begin
    @test issorted([1, 2, 3, 4, 5]) == true
    @test issorted([1, 3, 2, 4, 5]) == false
    @test issorted([1]) == true
    @test issorted([1, 1, 1]) == true
    @test issorted([5, 4, 3, 2, 1]) == false
end

@testset "mean" begin
    @test mean([1, 2, 3, 4, 5]) == 3.0
    @test mean([2, 4, 6]) == 4.0
    @test mean([10]) == 10.0
    @test mean([0, 0, 0]) == 0.0
end

println("test_array.jl: All tests passed!")
