# Test gcdx() - extended Euclidean algorithm (Issue #1865)

using Test

@testset "gcdx basic" begin
    g, x, y = gcdx(12, 8)
    @test g == 4
    @test 12 * x + 8 * y == g
end

@testset "gcdx coprime" begin
    g, x, y = gcdx(7, 13)
    @test g == 1
    @test 7 * x + 13 * y == g
end

@testset "gcdx with zero" begin
    g, x, y = gcdx(5, 0)
    @test g == 5
    @test 5 * x + 0 * y == g
end

true
