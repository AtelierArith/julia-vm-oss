# Test isvalid function - check if index is valid character boundary

using Test

@testset "isvalid(s, i) - check if index is valid character boundary" begin

    # === ASCII string - all indices valid ===
    s1 = "hello"
    @assert isvalid(s1, 1)
    @assert isvalid(s1, 2)
    @assert isvalid(s1, 3)
    @assert isvalid(s1, 4)
    @assert isvalid(s1, 5)

    # === Out of bounds ===
    @assert !isvalid(s1, 0)
    @assert !isvalid(s1, 6)
    @assert !isvalid(s1, 10)

    # === Empty string ===
    @assert !isvalid("", 0)
    @assert !isvalid("", 1)

    # === Single character ===
    @assert isvalid("a", 1)
    @assert !isvalid("a", 0)
    @assert !isvalid("a", 2)

    # All tests passed
    @test (true)
end

true  # Test passed
