# Test codeunits function - returns Vector{UInt8} of UTF-8 bytes

using Test

@testset "codeunits - get string as Vector{UInt8}" begin

    # Basic ASCII string
    s = "Hello"
    cu = codeunits(s)
    @assert length(cu) == 5
    @assert Int64(cu[1]) == 72   # 'H' = 0x48
    @assert Int64(cu[2]) == 101  # 'e' = 0x65
    @assert Int64(cu[3]) == 108  # 'l' = 0x6c
    @assert Int64(cu[4]) == 108  # 'l' = 0x6c
    @assert Int64(cu[5]) == 111  # 'o' = 0x6f

    # Empty string
    s2 = ""
    cu2 = codeunits(s2)
    @assert length(cu2) == 0

    # Single character
    s3 = "A"
    cu3 = codeunits(s3)
    @assert length(cu3) == 1
    @assert Int64(cu3[1]) == 65  # 'A' = 0x41

    # "Hi" has bytes 72 ('H') and 105 ('i')
    hi_cu = codeunits("Hi")
    @assert Int64(hi_cu[1]) == 72
    @assert Int64(hi_cu[2]) == 105

    @test (true)
end

true  # Test passed
