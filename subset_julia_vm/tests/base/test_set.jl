# Test for src/base/set.jl
# Based on Julia's test/sets.jl
using Test

@testset "unique" begin
    result = unique([1, 2, 3])
    @test length(result) == 3
    @test result[1] == 1

    result2 = unique([1, 1, 2, 2, 3, 3])
    @test length(result2) == 3
end

@testset "union" begin
    result = union([1, 2], [2, 3])
    @test length(result) == 3
end

@testset "intersect" begin
    result = intersect([1, 2, 3], [2, 3, 4])
    @test length(result) == 2
    @test result[1] == 2
    @test result[2] == 3
end

@testset "setdiff" begin
    result = setdiff([1, 2, 3], [2, 3, 4])
    @test length(result) == 1
    @test result[1] == 1
end

@testset "issubset" begin
    @test issubset([1, 2], [1, 2, 3]) == true
    @test issubset([1, 2, 3], [1, 2, 3]) == true
    @test issubset([1, 4], [1, 2, 3]) == false
end

println("test_set.jl: All tests passed!")
