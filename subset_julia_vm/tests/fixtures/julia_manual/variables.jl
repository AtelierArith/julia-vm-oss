# Julia Manual: Variables
# https://docs.julialang.org/en/v1/manual/variables/
# Tests variable assignment, naming conventions, and Unicode support.

using Test

@testset "Variables - Basic assignment" begin
    x = 10
    @test x == 10

    msg = "Hello"
    @test msg == "Hello"

    pi_val = 3.14159
    @test pi_val == 3.14159
end

@testset "Variables - Reassignment and types" begin
    x = 1
    @test x == 1
    @test typeof(x) == Int64

    x = 1.0
    @test x == 1.0
    @test typeof(x) == Float64

    x = "Hello"
    @test x == "Hello"
    @test typeof(x) == String
end

@testset "Variables - Unicode names" begin
    delta = 0.1
    @test delta == 0.1

    x1 = 1
    x2 = 2
    @test x1 + x2 == 3
end

@testset "Variables - Multiple assignment" begin
    x, y = 1, 2
    @test x == 1
    @test y == 2

    a, b, c = 1, 2, 3
    @test a + b + c == 6
end

true
