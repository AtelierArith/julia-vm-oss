# String(::Vector{Char}) constructor - convert character array to string (Issue #2038)

using Test

@testset "String(collect(s)) round-trip" begin
    @test String(collect("hello")) == "hello"
    @test String(collect("world")) == "world"
    @test String(collect("")) == ""
    @test String(collect("abc def")) == "abc def"
end

@testset "String(char_array) from literal array" begin
    @test String(['a', 'b', 'c']) == "abc"
    @test String(['h', 'e', 'l', 'l', 'o']) == "hello"
end

@testset "String(s) identity for strings" begin
    @test String("hello") == "hello"
end

true
