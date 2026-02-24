# Test extrema() and diff() reduction functions (Issue #1874)

using Test

@testset "extrema basic" begin
    @test extrema([3, 1, 4, 1, 5, 9]) == (1, 9)
    @test extrema([1.0, 2.0, 3.0]) == (1.0, 3.0)
    @test extrema([-5, -1, 0, 3]) == (-5, 3)
    @test extrema([42]) == (42, 42)
end

@testset "diff basic" begin
    result = diff([1, 3, 6, 10])
    @test result[1] == 2.0
    @test result[2] == 3.0
    @test result[3] == 4.0
    @test length(result) == 3
end

@testset "diff float" begin
    result = diff([1.0, 2.5, 4.0])
    @test result[1] == 1.5
    @test result[2] == 1.5
end

true
