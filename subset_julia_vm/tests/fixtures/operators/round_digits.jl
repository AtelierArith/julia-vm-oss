# round() with digits and sigdigits keyword arguments (Issue #2051)

using Test

@testset "round with digits keyword" begin
    @test round(3.14159, digits=2) == 3.14
    @test round(3.14159, digits=4) == 3.1416
    @test round(3.14159, digits=0) == 3.0
    @test round(1.005, digits=1) == 1.0
    @test round(-1.7, digits=0) == -2.0
    @test round(100.0, digits=2) == 100.0
end

@testset "round with sigdigits keyword" begin
    @test round(3.14159, sigdigits=3) == 3.14
    @test round(3.14159, sigdigits=1) == 3.0
    @test round(0.00123, sigdigits=2) == 0.0012
    @test round(1234.0, sigdigits=2) == 1200.0
end

@testset "round default (no keywords)" begin
    @test round(3.14) == 3.0
    @test round(-1.7) == -2.0
    @test round(0.0) == 0.0
end

true
