# Test IOBuffer basic operations

using Test

@testset "IOBuffer basic operations" begin

    # Test 1: Create IOBuffer and take string
    io1 = IOBuffer()
    bytes1 = write(io1, "hello")
    result1 = take!(io1)
    check1 = bytes1 == 5 && typeof(result1) == Vector{UInt8} && String(result1) == "hello"

    # Test 2: Multiple writes (chaining)
    io2 = IOBuffer()
    bytes2a = write(io2, "foo")
    bytes2b = write(io2, "bar")
    result2 = take!(io2)
    check2 = bytes2a == 3 && bytes2b == 3 && typeof(result2) == Vector{UInt8} && String(result2) == "foobar"

    # Test 3: Write different types
    io3 = IOBuffer()
    write(io3, '!')
    result3 = take!(io3)
    check3 = typeof(result3) == Vector{UInt8} && result3 == UInt8[0x21]

    # Test 4: Empty IOBuffer
    io4 = IOBuffer()
    result4 = take!(io4)
    check4 = typeof(result4) == Vector{UInt8} && length(result4) == 0

    # All checks must pass
    @test (check1 && check2 && check3 && check4)
end

true  # Test passed
