# Test hypot() function (Issue #1861)

using Test

@testset "hypot basic" begin
    @test hypot(3.0, 4.0) == 5.0
    @test hypot(5.0, 12.0) == 13.0
    @test hypot(0.0, 0.0) == 0.0
    @test hypot(1.0, 0.0) == 1.0
    @test hypot(0.0, 1.0) == 1.0
end

@testset "hypot integer arguments" begin
    @test hypot(3, 4) == 5.0
    @test hypot(5, 12) == 13.0
end

true
