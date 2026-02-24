# Test exported string builtin functions

using Test

@testset "String builtin exports" begin
    # ascii - validate ASCII string
    @test ascii("hello") === "hello"

    # bitstring - binary representation
    bs = bitstring(5)
    @test length(bs) == 64  # Int64 has 64 bits

    # bytes2hex - convert bytes to hex string
    @test bytes2hex([0x01, 0x0a, 0xff]) === "010aff"

    # hex2bytes - convert hex string to bytes
    arr = hex2bytes("010aff")
    @test length(arr) == 3
    @test arr[1] == 0x01
    @test arr[2] == 0x0a
    @test arr[3] == 0xff

    # repr - string representation
    @test repr(42) === "42"
    @test repr(3.14) === "3.14"
end

true
