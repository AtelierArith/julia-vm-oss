# Test for stdlib/Random
# Based on Julia's Random tests
using Test
using Random

@testset "seed! reproducibility" begin
    # Same seed produces same sequence
    Random.seed!(42)
    a1 = rand()
    a2 = rand()
    a3 = rand()

    Random.seed!(42)
    b1 = rand()
    b2 = rand()
    b3 = rand()

    @test a1 == b1
    @test a2 == b2
    @test a3 == b3
end

@testset "different seeds produce different sequences" begin
    Random.seed!(1)
    x1 = rand()

    Random.seed!(2)
    x2 = rand()

    @test x1 != x2
end

@testset "rand range" begin
    # rand() should be in [0, 1)
    Random.seed!(12345)
    for i in 1:100
        r = rand()
        @test r >= 0.0
        @test r < 1.0
    end
end

@testset "rand(n) produces n elements" begin
    Random.seed!(42)
    arr = rand(10)
    @test length(arr) == 10

    # All elements should be in [0, 1)
    for i in 1:10
        @test arr[i] >= 0.0
        @test arr[i] < 1.0
    end
end

@testset "randn reproducibility" begin
    # Same seed produces same randn sequence
    Random.seed!(99)
    n1 = randn()
    n2 = randn()

    Random.seed!(99)
    m1 = randn()
    m2 = randn()

    @test n1 == m1
    @test n2 == m2
end

@testset "randn(n) produces n elements" begin
    Random.seed!(42)
    arr = randn(5)
    @test length(arr) == 5
end

println("test_Random.jl: All tests passed!")
