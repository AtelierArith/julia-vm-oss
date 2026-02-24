# Test thisind and reverseind functions

using Test

@testset "thisind/reverseind - string index functions" begin

    # === thisind tests ===

    # ASCII string - every byte is a valid index
    s1 = "hello"
    @assert thisind(s1, 1) == 1
    @assert thisind(s1, 2) == 2
    @assert thisind(s1, 3) == 3
    @assert thisind(s1, 4) == 4
    @assert thisind(s1, 5) == 5

    # Edge cases
    @assert thisind(s1, 0) == 0        # Before start
    @assert thisind(s1, 6) == 6        # Past end (ncodeunits + 1)

    # Empty string
    @assert thisind("", 0) == 0
    @assert thisind("", 1) == 1

    # === reverseind tests ===

    # ASCII string - simple mapping
    s2 = "abc"
    # reverse("abc") = "cba"
    # reverseind(s, i) maps index in reverse(s) to index in s
    @assert reverseind(s2, 1) == 3  # 'c' at index 1 in reverse -> index 3 in original
    @assert reverseind(s2, 2) == 2  # 'b' at index 2 in reverse -> index 2 in original
    @assert reverseind(s2, 3) == 1  # 'a' at index 3 in reverse -> index 1 in original

    # Single character
    s3 = "x"
    @assert reverseind(s3, 1) == 1

    # Edge cases
    @assert reverseind(s2, 0) == 4  # Before start in reverse -> past end in original
    @assert reverseind(s2, 4) == 0  # Past end in reverse -> before start in original

    # Empty string
    @assert reverseind("", 0) == 1

    # All tests passed
    @test (true)
end

true  # Test passed
