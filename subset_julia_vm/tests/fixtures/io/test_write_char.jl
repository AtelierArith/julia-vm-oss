# Test write(io, char) with character literals

using Test

@testset "write(io, char) supports character literals directly" begin

    # Test 1: write with character literal
    io1 = IOBuffer()
    io1 = write(io1, '(')
    r1 = take!(io1)
    check1 = r1 == "("

    # Test 2: write multiple characters
    io2 = IOBuffer()
    io2 = write(io2, '[')
    io2 = write(io2, ',')
    io2 = write(io2, ']')
    r2 = take!(io2)
    check2 = r2 == "[,]"

    # Test 3: write mixed string and char
    io3 = IOBuffer()
    io3 = write(io3, "hello")
    io3 = write(io3, '!')
    r3 = take!(io3)
    check3 = r3 == "hello!"

    # All checks must pass
    @test (check1 && check2 && check3)
end

true  # Test passed
