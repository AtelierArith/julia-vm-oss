# Test occursin with Regex

using Test

@testset "occursin with Regex" begin
    # Basic match
    @test occursin(r"world", "hello world") == true

    # No match
    @test occursin(r"xyz", "hello world") == false

    # Pattern at start
    @test occursin(r"hello", "hello world") == true

    # Pattern at end
    @test occursin(r"world", "hello world") == true

    # Pattern in middle
    @test occursin(r"lo wo", "hello world") == true

    # Regex with quantifiers
    @test occursin(r"l+", "hello") == true

    # Regex with character classes
    @test occursin(r"[aeiou]", "hello") == true

    # Regex that doesn't match
    @test occursin(r"[0-9]", "hello") == false
end

true
