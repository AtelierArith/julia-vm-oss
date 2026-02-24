# Test for Statistics stdlib
# Based on Julia's stdlib/Statistics tests
using Test
using Statistics

@testset "var and std" begin
    # Variance of [1,2,3,4,5] with Bessel's correction = 2.5
    @test var([1, 2, 3, 4, 5]) == 2.5
    @test var([1, 1, 1, 1]) == 0.0

    # varm: variance with known mean
    @test varm([1, 2, 3, 4, 5], 3) == 2.5

    # std = sqrt(var)
    @test isapprox(std([1, 2, 3, 4, 5]), sqrt(2.5)) == true

    # stdm: std with known mean
    @test isapprox(stdm([1, 2, 3, 4, 5], 3), sqrt(2.5)) == true
end

@testset "median" begin
    # Odd number of elements
    @test median([1, 2, 3, 4, 5]) == 3
    @test median([5, 1, 3, 2, 4]) == 3
    @test median([1, 3, 5]) == 3

    # Even number of elements
    @test median([1, 2, 3, 4]) == 2.5
    @test median([4, 2, 1, 3]) == 2.5

    # Single element
    @test median([5]) == 5
end

@testset "cor" begin
    # Perfect positive correlation
    x = [1, 2, 3, 4, 5]
    y = [2, 4, 6, 8, 10]
    @test isapprox(cor(x, y), 1.0) == true

    # Perfect negative correlation
    y2 = [10, 8, 6, 4, 2]
    @test isapprox(cor(x, y2), -1.0) == true

    # Self correlation
    @test isapprox(cor(x, x), 1.0) == true
end

@testset "cov" begin
    x = [1, 2, 3, 4, 5]
    y = [2, 4, 6, 8, 10]
    # cov = sum((xi - mx)(yi - my)) / (n-1)
    # mx = 3, my = 6
    # = ((-2)(-4) + (-1)(-2) + 0 + 1*2 + 2*4) / 4
    # = (8 + 2 + 0 + 2 + 8) / 4 = 5
    @test cov(x, y) == 5.0

    # cov(x, x) = var(x)
    @test isapprox(cov(x, x), var(x)) == true
end

println("test_statistics.jl: All tests passed!")
