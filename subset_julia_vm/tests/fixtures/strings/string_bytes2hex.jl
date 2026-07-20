# Test bytes2hex function - convert byte array to hex string
# Upstream bytes2hex requires eltype UInt8 (ArgumentError otherwise), so this
# fixture uses UInt8 arrays throughout (Issues #10324 / #10237).

using Test

@testset "bytes2hex(a) - convert byte array to hex string" begin

    # === Basic conversion ===

    # Simple bytes ("Hello" in ASCII)
    arr1 = UInt8[0x48, 0x65, 0x6c, 0x6c, 0x6f]
    hex1 = bytes2hex(arr1)
    @assert hex1 == "48656c6c6f"

    # Empty array
    arr2 = UInt8[]
    hex2 = bytes2hex(arr2)
    @assert hex2 == ""

    # Single byte
    arr3 = UInt8[0xff]
    hex3 = bytes2hex(arr3)
    @assert hex3 == "ff"

    # Zero byte
    arr4 = UInt8[0x00]
    hex4 = bytes2hex(arr4)
    @assert hex4 == "00"

    # === Various byte values ===

    # Low values (leading zeros required)
    arr5 = UInt8[0x01, 0x02, 0x0a, 0x0f]
    hex5 = bytes2hex(arr5)
    @assert hex5 == "01020a0f"

    # High values (deadbeef)
    arr6 = UInt8[0xde, 0xad, 0xbe, 0xef]
    hex6 = bytes2hex(arr6)
    @assert hex6 == "deadbeef"

    # All zeros
    arr7 = UInt8[0x00, 0x00, 0x00]
    hex7 = bytes2hex(arr7)
    @assert hex7 == "000000"

    # All ones (0xff = 255)
    arr8 = UInt8[0xff, 0xff]
    hex8 = bytes2hex(arr8)
    @assert hex8 == "ffff"

    # All tests passed
    @test (true)
end

# Upstream requires `eltype(itr) === UInt8`; a non-UInt8 eltype throws
# ArgumentError rather than being silently converted (Issue #10324 item 1).
@testset "bytes2hex rejects non-UInt8 eltype / hex2bytes returns Vector{UInt8}" begin
    @test_throws ArgumentError bytes2hex([222, 173, 190, 239])   # Vector{Int64}
    @test_throws ArgumentError bytes2hex(Int8[1, 2, 3])

    # hex2bytes returns Vector{UInt8}, so round-trips through bytes2hex stay valid
    bs = hex2bytes("deadbeef")
    @test typeof(bs) == Vector{UInt8}
    @test bs == UInt8[0xde, 0xad, 0xbe, 0xef]
    @test bytes2hex(bs) == "deadbeef"
    @test typeof(hex2bytes("")) == Vector{UInt8}
end

true  # Test passed
