# Test eachmatch function with Regex

using Test

@testset "eachmatch with Regex" begin
    # Multiple matches - count the number found
    matches = eachmatch(r"o", "hello world")
    @test length(matches) == 2  # 'o' appears at positions 5 and 8

    # No matches returns empty array
    matches2 = eachmatch(r"xyz", "hello world")
    @test length(matches2) == 0

    # Match all digits
    matches3 = eachmatch(r"\d", "a1b2c3")
    @test length(matches3) == 3

    # Match words
    matches4 = eachmatch(r"\w+", "hello world")
    @test length(matches4) == 2

    # Match with quantifier
    matches5 = eachmatch(r"l+", "hello")
    @test length(matches5) == 1  # "ll" is one match
end

true
