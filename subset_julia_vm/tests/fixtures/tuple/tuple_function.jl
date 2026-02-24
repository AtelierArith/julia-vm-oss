# Test tuple() function for creating tuples (Issue #2173)
# Julia: tuple(args...) creates a tuple, equivalent to (args...)

using Test

@testset "tuple with integers" begin
    t = tuple(1, 2, 3)
    @test t == (1, 2, 3)
    @test length(t) == 3
    @test t[1] == 1
    @test t[3] == 3
end

@testset "tuple with mixed types" begin
    t = tuple("hello", 42, true)
    @test length(t) == 3
    @test t[1] == "hello"
    @test t[2] == 42
    @test t[3] == true
end

@testset "tuple with single argument" begin
    t = tuple(42)
    @test length(t) == 1
    @test t[1] == 42
end

@testset "tuple with no arguments" begin
    t = tuple()
    @test length(t) == 0
end

@testset "tuple with floats" begin
    t = tuple(1.0, 2.5, 3.14)
    @test t[1] == 1.0
    @test t[2] == 2.5
    @test t[3] == 3.14
end

true
