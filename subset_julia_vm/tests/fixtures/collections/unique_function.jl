# unique(f, itr) - unique elements by function (Issue #2155)

using Test

square(x) = x * x

@testset "unique(f, itr) - basic" begin
    # unique by modulo
    result = unique(x -> x % 3, [1, 2, 3, 4, 5, 6])
    @test length(result) == 3
    @test result[1] == 1.0
    @test result[2] == 2.0
    @test result[3] == 3.0
end

@testset "unique(f, itr) - abs" begin
    # unique by absolute value
    result = unique(abs, [-1, 2, -2, 1, 3])
    @test length(result) == 3
    @test result[1] == -1.0
    @test result[2] == 2.0
    @test result[3] == 3.0
end

@testset "unique(f, itr) - identity" begin
    # unique by identity is same as unique(arr)
    result = unique(identity, [1, 2, 2, 3, 3, 3])
    @test length(result) == 3
    @test result[1] == 1.0
    @test result[2] == 2.0
    @test result[3] == 3.0
end

@testset "unique(f, itr) - square" begin
    # unique by square: -2 and 2 have same square
    result = unique(square, [-2, -1, 1, 2, 3])
    @test length(result) == 3
    @test result[1] == -2.0
    @test result[2] == -1.0
    @test result[3] == 3.0
end

@testset "unique(f, itr) - empty" begin
    result = unique(abs, Float64[])
    @test length(result) == 0
end

@testset "unique(f, itr) - single element" begin
    result = unique(abs, [42])
    @test length(result) == 1
    @test result[1] == 42.0
end

true
