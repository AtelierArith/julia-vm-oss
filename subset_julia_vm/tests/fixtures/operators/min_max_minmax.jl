# Test min, max, minmax functions (Issue #1883)

using Test

@testset "min basic" begin
    @test min(3, 5) == 3
    @test min(5, 3) == 3
    @test min(-1, 1) == -1
    @test min(0, 0) == 0
end

@testset "min float" begin
    @test min(3.0, 5.0) == 3.0
    @test min(-1.5, 2.5) == -1.5
end

@testset "max basic" begin
    @test max(3, 5) == 5
    @test max(5, 3) == 5
    @test max(-1, 1) == 1
    @test max(0, 0) == 0
end

@testset "max float" begin
    @test max(3.0, 5.0) == 5.0
    @test max(-1.5, 2.5) == 2.5
end

@testset "minmax basic" begin
    lo, hi = minmax(3, 5)
    @test lo == 3
    @test hi == 5

    lo2, hi2 = minmax(5, 3)
    @test lo2 == 3
    @test hi2 == 5
end

@testset "minmax equal" begin
    lo, hi = minmax(7, 7)
    @test lo == 7
    @test hi == 7
end

@testset "minmax float" begin
    lo, hi = minmax(-2.5, 1.5)
    @test lo == -2.5
    @test hi == 1.5
end

true
