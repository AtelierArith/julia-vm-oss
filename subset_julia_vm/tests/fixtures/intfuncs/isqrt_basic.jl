# Test isqrt() - integer square root (Issue #1865)

using Test

@testset "isqrt perfect squares" begin
    @test isqrt(0) == 0
    @test isqrt(1) == 1
    @test isqrt(4) == 2
    @test isqrt(9) == 3
    @test isqrt(16) == 4
    @test isqrt(25) == 5
    @test isqrt(100) == 10
end

@testset "isqrt non-perfect squares" begin
    @test isqrt(2) == 1
    @test isqrt(3) == 1
    @test isqrt(5) == 2
    @test isqrt(8) == 2
    @test isqrt(10) == 3
    @test isqrt(99) == 9
end

true
