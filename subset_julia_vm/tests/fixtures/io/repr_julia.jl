# Test repr function implemented in Julia
# repr(x) should return a string representation using show(io, x)

using Test

@testset "repr function" begin
    # Basic types
    @test repr(1) == "1"
    @test repr(1.5) == "1.5"
    @test repr(true) == "true"
    @test repr(false) == "false"
    @test repr(nothing) == "nothing"

    # String and Char with quotes
    @test repr("hello") == "\"hello\""
    @test repr('a') == "'a'"

    # Symbols
    @test repr(:symbol) == ":symbol"

    # Tuples
    @test repr((1, 2)) == "(1, 2)"
    @test repr((1,)) == "(1,)"

    # Pairs
    @test repr(:a => 1) == ":a => 1"

    # Ranges
    @test repr(1:5) == "1:5"
    @test repr(1:2:10) == "1:2:10"

    # Complex numbers
    @test repr(1 + 2im) == "1 + 2im"
    @test repr(1 - 2im) == "1 - 2im"

    # Rational numbers
    @test repr(1//2) == "1//2"
end

true
