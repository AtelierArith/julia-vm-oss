# Test string repeat function and ^ operator

using Test

@testset "String repeat function" begin
    # Basic repeat
    @test repeat("ha", 3) == "hahaha"
    @test repeat("ab", 2) == "abab"

    # Edge cases
    @test repeat("abc", 0) == ""
    @test repeat("abc", 1) == "abc"
    @test repeat("", 5) == ""

    # Single character
    @test repeat("x", 4) == "xxxx"

    # Longer string
    @test repeat("Hello ", 2) == "Hello Hello "
end

@testset "String power operator (s^n)" begin
    # Basic string repeat with ^ (Julia syntax: s^n is equivalent to repeat(s, n))
    @test "ha"^3 == "hahaha"
    @test "ab"^2 == "abab"

    # Single character repeat
    @test "x"^5 == "xxxxx"

    # Empty string edge cases
    @test ""^0 == ""
    @test ""^5 == ""
    @test "hello"^0 == ""

    # Single repeat (n=1)
    @test "test"^1 == "test"

    # Multi-character string
    @test "abc"^3 == "abcabcabc"

    # With spaces
    @test " "^4 == "    "
    @test "a b"^2 == "a ba b"

    # Verify equivalence with repeat function
    @test "foo"^3 == repeat("foo", 3)
    @test "bar"^2 == repeat("bar", 2)
end

true
