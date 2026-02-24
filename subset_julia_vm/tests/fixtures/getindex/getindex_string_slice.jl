# Test: getindex on String with range returns substring

using Test

@testset "getindex(s, range) returns substring" begin
    s = "Hello"

    # getindex(s, range) returns substring
    # Check length to verify correct substring extraction
    @assert length(getindex(s, 2:4)) == 3  # "ell" has 3 chars
    @assert length(getindex(s, 1:5)) == 5  # "Hello" has 5 chars
    @assert length(getindex(s, 1:1)) == 1  # "H" has 1 char

    # Verify getindex and s[range] return same length
    @assert length(getindex(s, 2:4)) == length(s[2:4])

    @test (true)
end

true  # Test passed
