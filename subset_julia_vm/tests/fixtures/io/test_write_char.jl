# Test write(io, char) with character literals

using Test

@testset "write(io, char) supports character literals directly" begin

    # Test 1: write with character literal
    io1 = IOBuffer()
    bytes1 = write(io1, '(')
    r1 = String(take!(io1))
    check1 = r1 == "("

    # Test 2: write multiple characters
    io2 = IOBuffer()
    bytes2a = write(io2, '[')
    bytes2b = write(io2, ',')
    bytes2c = write(io2, ']')
    r2 = String(take!(io2))
    check2 = r2 == "[,]"

    # Test 3: write mixed string and char
    io3 = IOBuffer()
    bytes3a = write(io3, "hello")
    bytes3b = write(io3, '!')
    r3 = String(take!(io3))
    check3 = r3 == "hello!"

    # All checks must pass
    @test (bytes1 == 1 && bytes2a == 1 && bytes2b == 1 && bytes2c == 1 &&
           bytes3a == 5 && bytes3b == 1 && check1 && check2 && check3)
end

true  # Test passed
