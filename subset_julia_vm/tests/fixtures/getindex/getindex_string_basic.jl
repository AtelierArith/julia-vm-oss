# Test: getindex on String returns character

using Test

@testset "getindex(s, i) returns character from string" begin
    s = "Hello"

    # getindex(s, i) returns the character at position i
    # Compare char codes using Int() conversion
    @assert Int(getindex(s, 1)) == 72  # 'H' = 72
    @assert Int(getindex(s, 2)) == 101  # 'e' = 101
    @assert Int(getindex(s, 5)) == 111  # 'o' = 111

    # Verify getindex and s[i] return the same char code
    @assert Int(getindex(s, 1)) == Int(s[1])

    @test (true)
end

true  # Test passed
