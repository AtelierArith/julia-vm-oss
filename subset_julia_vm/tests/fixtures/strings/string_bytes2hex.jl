# Test bytes2hex function - convert byte array to hex string

using Test

@testset "bytes2hex(a) - convert byte array to hex string" begin

    # === Basic conversion ===

    # Simple bytes using integer array
    arr1 = [72, 101, 108, 108, 111]  # "Hello" in ASCII (decimal values)
    hex1 = bytes2hex(arr1)
    @assert hex1 == "48656c6c6f"

    # Empty array
    arr2 = Int64[]
    hex2 = bytes2hex(arr2)
    @assert hex2 == ""

    # Single byte
    arr3 = [255]
    hex3 = bytes2hex(arr3)
    @assert hex3 == "ff"

    # Zero byte
    arr4 = [0]
    hex4 = bytes2hex(arr4)
    @assert hex4 == "00"

    # === Various byte values ===

    # Low values (leading zeros required)
    arr5 = [1, 2, 10, 15]
    hex5 = bytes2hex(arr5)
    @assert hex5 == "01020a0f"

    # High values (deadbeef)
    arr6 = [222, 173, 190, 239]  # 0xde, 0xad, 0xbe, 0xef
    hex6 = bytes2hex(arr6)
    @assert hex6 == "deadbeef"

    # All zeros
    arr7 = [0, 0, 0]
    hex7 = bytes2hex(arr7)
    @assert hex7 == "000000"

    # All ones (0xff = 255)
    arr8 = [255, 255]
    hex8 = bytes2hex(arr8)
    @assert hex8 == "ffff"

    # All tests passed
    @test (true)
end

true  # Test passed
