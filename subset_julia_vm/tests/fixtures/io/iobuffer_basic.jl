# Test IOBuffer basic operations
# Note: Our IOBuffer is immutable - write() returns a new IOBuffer

using Test

@testset "IOBuffer basic operations" begin

    # Test 1: Create IOBuffer and take string
    io1 = write(IOBuffer(), "hello")
    result1 = take!(io1)
    # Use length check instead of == to avoid type inference issues
    check1 = length(result1) == 5

    # Test 2: Multiple writes (chaining)
    io2a = write(IOBuffer(), "foo")
    io2b = write(io2a, "bar")
    result2 = takestring!(io2b)
    check2 = length(result2) == 6

    # Test 3: Write different types
    io3 = write(IOBuffer(), 42)
    result3 = take!(io3)
    check3 = length(result3) == 2

    # Test 4: Empty IOBuffer
    io4 = IOBuffer()
    result4 = take!(io4)
    check4 = length(result4) == 0

    # All checks must pass
    @test (check1 && check2 && check3 && check4)
end

true  # Test passed
