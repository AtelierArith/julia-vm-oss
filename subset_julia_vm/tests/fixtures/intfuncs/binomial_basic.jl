# Test binomial() - binomial coefficients (Issue #1865)

using Test

@testset "binomial basic" begin
    @test binomial(0, 0) == 1
    @test binomial(1, 0) == 1
    @test binomial(1, 1) == 1
    @test binomial(5, 0) == 1
    @test binomial(5, 5) == 1
    @test binomial(5, 1) == 5
    @test binomial(5, 2) == 10
    @test binomial(5, 3) == 10
    @test binomial(10, 3) == 120
    @test binomial(10, 5) == 252
end

@testset "binomial symmetry" begin
    @test binomial(10, 3) == binomial(10, 7)
    @test binomial(8, 2) == binomial(8, 6)
end

@testset "binomial edge cases" begin
    @test binomial(5, 6) == 0
    @test binomial(5, -1) == 0
end

true
