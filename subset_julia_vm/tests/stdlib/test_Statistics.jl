# Test for Statistics stdlib
# Based on Julia's Statistics.jl tests
using Test
using Statistics

@testset "mean" begin
    # Basic mean
    @test mean([1, 2, 3]) == 2.0
    @test mean([1.0, 2.0, 3.0]) == 2.0
    @test mean([1, 2, 3, 4, 5]) == 3.0
    # Note: mean(f, arr) not tested - requires full dispatch support
end

@testset "var" begin
    # Sample variance with Bessel's correction (n-1)
    # var([1,2,3]) = ((1-2)^2 + (2-2)^2 + (3-2)^2) / 2 = 2/2 = 1.0
    @test var([1, 2, 3]) == 1.0

    # var([1,2,3,4,5]) = (4+1+0+1+4) / 4 = 10/4 = 2.5
    @test var([1, 2, 3, 4, 5]) == 2.5

    # Constant array has zero variance
    @test var([5, 5, 5, 5]) == 0.0
end

@testset "std" begin
    # Standard deviation is sqrt of variance
    @test std([1, 2, 3]) == 1.0
    @test isapprox(std([1, 2, 3, 4, 5]), sqrt(2.5)) == true

    # Constant array has zero std
    @test std([5, 5, 5, 5]) == 0.0
end

# Note: varm and stdm not available from base prelude

@testset "median" begin
    # Odd length: middle element
    @test median([1, 2, 3]) == 2.0
    @test median([3, 1, 2]) == 2.0  # Unsorted input
    @test median([1, 2, 3, 4, 5]) == 3.0

    # Even length: mean of two middle elements
    @test median([1, 2, 3, 4]) == 2.5
    @test median([1, 2]) == 1.5

    # Single element
    @test median([42]) == 42.0
end

# Note: middle function not available from base prelude

@testset "cor" begin
    # Perfect positive correlation
    x = [1, 2, 3, 4, 5]
    y = [2, 4, 6, 8, 10]
    @test isapprox(cor(x, y), 1.0) == true

    # Perfect negative correlation
    z = [10, 8, 6, 4, 2]
    @test isapprox(cor(x, z), -1.0) == true

    # Note: cor(x) single-arg version not tested yet
end

@testset "cov" begin
    # Covariance of identical arrays equals variance
    x = [1, 2, 3, 4, 5]
    @test isapprox(cov(x, x), var(x)) == true

    # Note: cov(x) single-arg version not tested yet

    # Positive covariance for positively related data
    y = [2, 4, 6, 8, 10]
    @test cov(x, y) > 0
end

# Note: quantile not tested yet

println("test_Statistics.jl: All tests passed!")
