# Test findmax() and findmin() functions (Issue #1874)

using Test

@testset "findmax basic" begin
    @test findmax([3, 1, 4, 1, 5]) == (5, 5)
    @test findmax([10, 20, 30]) == (30, 3)
    @test findmax([-1, -2, -3]) == (-1, 1)
    @test findmax([42]) == (42, 1)
end

@testset "findmin basic" begin
    @test findmin([3, 1, 4, 1, 5]) == (1, 2)
    @test findmin([10, 20, 30]) == (10, 1)
    @test findmin([-1, -2, -3]) == (-3, 3)
    @test findmin([42]) == (42, 1)
end

@testset "findmax findmin float" begin
    @test findmax([1.5, 2.5, 0.5]) == (2.5, 2)
    @test findmin([1.5, 2.5, 0.5]) == (0.5, 3)
end

true
