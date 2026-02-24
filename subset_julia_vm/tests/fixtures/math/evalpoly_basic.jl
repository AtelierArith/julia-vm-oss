# Test evalpoly() function - Horner's method polynomial evaluation (Issue #1861)

using Test

@testset "evalpoly constant" begin
    # p(x) = 5
    @test evalpoly(2.0, (5.0,)) == 5.0
    @test evalpoly(0.0, (5.0,)) == 5.0
end

@testset "evalpoly linear" begin
    # p(x) = 1 + 2x
    @test evalpoly(0.0, (1.0, 2.0)) == 1.0
    @test evalpoly(1.0, (1.0, 2.0)) == 3.0
    @test evalpoly(3.0, (1.0, 2.0)) == 7.0
end

@testset "evalpoly quadratic" begin
    # p(x) = 1 + 0x + 1x^2 = 1 + x^2
    @test evalpoly(0.0, (1.0, 0.0, 1.0)) == 1.0
    @test evalpoly(2.0, (1.0, 0.0, 1.0)) == 5.0
    @test evalpoly(3.0, (1.0, 0.0, 1.0)) == 10.0
end

@testset "evalpoly cubic" begin
    # p(x) = 1 + 2x + 3x^2 + 4x^3
    @test evalpoly(1.0, (1.0, 2.0, 3.0, 4.0)) == 10.0
    @test evalpoly(0.0, (1.0, 2.0, 3.0, 4.0)) == 1.0
end

true
