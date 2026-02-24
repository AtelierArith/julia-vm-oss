# Test match function with Regex

using Test

@testset "match with Regex" begin
    # Basic match returns non-nothing value
    m = match(r"world", "hello world")
    @test m !== nothing

    # No match returns nothing
    m2 = match(r"xyz", "hello world")
    @test m2 === nothing

    # Match at start returns non-nothing
    m3 = match(r"hello", "hello world")
    @test m3 !== nothing

    # Match in middle returns non-nothing
    m4 = match(r"o", "hello world")
    @test m4 !== nothing

    # Test with character class
    m5 = match(r"[0-9]+", "abc123def")
    @test m5 !== nothing

    # No digits in string
    m6 = match(r"[0-9]+", "no numbers")
    @test m6 === nothing
end

true
