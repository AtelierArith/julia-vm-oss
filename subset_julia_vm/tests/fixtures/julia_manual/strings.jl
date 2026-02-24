# Julia Manual: Strings
# https://docs.julialang.org/en/v1/manual/strings/
# Tests string creation, interpolation, concatenation, and operations.

using Test

@testset "String basics" begin
    s = "Hello, World!"
    @test typeof(s) == String
    @test length(s) == 13
end

@testset "String concatenation" begin
    @test string("Hello", " ", "World") == "Hello World"
    @test "Hello" * " " * "World" == "Hello World"
end

@testset "String interpolation" begin
    myname = "Julia"
    @test "Hello, $(myname)!" == "Hello, Julia!"

    x = 10
    y = 20
    @test "sum = $(x + y)" == "sum = 30"
end

@testset "String comparison" begin
    @test "abc" == "abc"
    @test "abc" != "xyz"
end

@testset "String functions" begin
    @test uppercase("hello") == "HELLO"
    @test lowercase("HELLO") == "hello"
    @test strip("  hello  ") == "hello"
end

@testset "String search" begin
    s = "Hello, World!"
    @test startswith(s, "Hello")
    @test endswith(s, "World!")
    @test occursin("World", s)
    @test occursin("xyz", s) == false
end

@testset "String operations" begin
    @test repeat("ab", 3) == "ababab"
    @test replace("Hello World", "World" => "Julia") == "Hello Julia"
    @test join(["a", "b", "c"], ", ") == "a, b, c"
    @test split("a,b,c", ",") == ["a", "b", "c"]
end

true
