# Test IO type dispatch - Issue #399
# Verifies that IOBuffer type name, subtype relationship, and method dispatch work correctly.

using Test

function accepts_io(io::IO)
    true
end

@testset "IO type dispatch: functions with io::IO parameter dispatch correctly for IOBuffer (Issue #399)" begin

    # Test 1: typeof(IOBuffer()) should return IOBuffer
    io = IOBuffer()
    type_name = typeof(io)
    test1 = type_name == IOBuffer

    # Test 2: IOBuffer <: IO should be true
    test2 = IOBuffer <: IO

    # Test 3: isa(IOBuffer(), IO) should be true
    io3 = IOBuffer()
    test3 = isa(io3, IO)

    # Test 4: Function dispatch with io::IO parameter should correctly match IOBuffer
    # This tests that the type annotation ::IO properly matches IOBuffer values

    io4 = IOBuffer()
    test4 = accepts_io(io4)

    # Return true if all tests pass
    @test (test1 && test2 && test3 && test4)
end

true  # Test passed
