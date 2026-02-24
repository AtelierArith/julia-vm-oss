# first() and last() for strings (Issue #2048)

using Test

@testset "first(::String) - get first character" begin
    @test first("hello") == 'h'
    @test first("A") == 'A'
    @test first("123") == '1'
end

@testset "last(::String) - get last character" begin
    @test last("hello") == 'o'
    @test last("A") == 'A'
    @test last("123") == '3'
end

@testset "first(::String, n) - get first n characters" begin
    @test first("hello", 3) == "hel"
    @test first("hello", 1) == "h"
    @test first("hello", 5) == "hello"
end

@testset "last(::String, n) - get last n characters" begin
    @test last("hello", 3) == "llo"
    @test last("hello", 1) == "o"
    @test last("hello", 5) == "hello"
end

true
