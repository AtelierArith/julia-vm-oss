# Test number utility functions: isreal, abs2, float, inv (Issue #1877)

using Test

@testset "isreal" begin
    @test isreal(42) == true
    @test isreal(3.14) == true
    @test isreal(0) == true
    @test isreal(-1) == true
end

@testset "abs2 integer" begin
    @test abs2(3) == 9
    @test abs2(-4) == 16
    @test abs2(0) == 0
end

@testset "abs2 float" begin
    @test abs2(2.5) == 6.25
    @test abs2(-1.5) == 2.25
end

@testset "float conversion" begin
    @test float(42) == 42.0
    @test float(3.14) == 3.14
    @test float(-1) == -1.0
end

@testset "inv number" begin
    @test abs(inv(2.0) - 0.5) < 1e-14
    @test abs(inv(4.0) - 0.25) < 1e-14
    @test abs(inv(0.5) - 2.0) < 1e-14
end

true
