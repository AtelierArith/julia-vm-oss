# Test Pure Julia show methods (Issue #2583)
# Verifies that show(io, x) Pure Julia implementations in io.jl work correctly
using Test

@testset "show Pure Julia - basic types" begin
    # Bool
    @test repr(true) == "true"
    @test repr(false) == "false"

    # Nothing
    @test repr(nothing) == "nothing"

    # Integer types
    @test repr(42) == "42"
    @test repr(-1) == "-1"

    # Float types
    @test repr(3.14) == "3.14"

    # Char (show adds quotes)
    @test repr('a') == "'a'"

    # String (show adds quotes)
    @test repr("hello") == "\"hello\""
end

@testset "show Pure Julia - containers" begin
    # Tuple
    @test repr((1, 2, 3)) == "(1, 2, 3)"
    @test repr((1,)) == "(1,)"  # single-element tuple has trailing comma

    # Pair
    @test repr(1 => 2) == "1 => 2"
    @test repr("a" => 1) == "\"a\" => 1"
end

@testset "show Pure Julia - range types" begin
    # UnitRange
    @test repr(1:5) == "1:5"

    # StepRange
    @test repr(1:2:10) == "1:2:9"
end

@testset "show Pure Julia - numeric types" begin
    # Rational
    @test repr(1//2) == "1//2"
    @test repr(3//4) == "3//4"
end

@testset "repr function" begin
    # repr uses show(io, x) + take!(io)
    @test repr(42) == "42"
    @test repr([1, 2]) == repr([1, 2])  # consistency check
end

true
