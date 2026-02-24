# Test hex2bytes function - convert hex string to byte array

using Test

@testset "hex2bytes(s) - convert hex string to byte array" begin

    # === Basic conversion ===

    # Simple hex string
    bytes1 = hex2bytes("48656c6c6f")
    @assert length(bytes1) == 5
    @assert bytes1[1] == 72   # 0x48
    @assert bytes1[2] == 101  # 0x65
    @assert bytes1[3] == 108  # 0x6c
    @assert bytes1[4] == 108  # 0x6c
    @assert bytes1[5] == 111  # 0x6f

    # Empty string
    bytes2 = hex2bytes("")
    @assert length(bytes2) == 0

    # Single byte
    bytes3 = hex2bytes("ff")
    @assert length(bytes3) == 1
    @assert bytes3[1] == 255

    # Zero byte
    bytes4 = hex2bytes("00")
    @assert length(bytes4) == 1
    @assert bytes4[1] == 0

    # === Various hex values ===

    # Lowercase hex
    bytes5 = hex2bytes("deadbeef")
    @assert length(bytes5) == 4
    @assert bytes5[1] == 222  # 0xde
    @assert bytes5[2] == 173  # 0xad
    @assert bytes5[3] == 190  # 0xbe
    @assert bytes5[4] == 239  # 0xef

    # Uppercase hex (should also work)
    bytes6 = hex2bytes("DEADBEEF")
    @assert length(bytes6) == 4
    @assert bytes6[1] == 222  # 0xde
    @assert bytes6[2] == 173  # 0xad
    @assert bytes6[3] == 190  # 0xbe
    @assert bytes6[4] == 239  # 0xef

    # Mixed case
    bytes7 = hex2bytes("DeAdBeEf")
    @assert length(bytes7) == 4

    # Leading zeros
    bytes8 = hex2bytes("01020a0f")
    @assert bytes8[1] == 1
    @assert bytes8[2] == 2
    @assert bytes8[3] == 10
    @assert bytes8[4] == 15

    # All tests passed
    @test (true)
end

true  # Test passed
