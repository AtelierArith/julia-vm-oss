# Test print(io, ...) writing to IOBuffer (Issue #1217)
# Verifies that print(io, ...) writes to the IOBuffer instead of stdout

using Test

@testset "print(io, ...) to IOBuffer" begin
    # Test 1: Basic print to IOBuffer
    io = IOBuffer()
    print(io, "hello")
    result = String(take!(io))
    @test length(result) == 5
    @test result == "hello"

    # Test 2: Multiple arguments
    io = IOBuffer()
    print(io, "a", "b", "c")
    result = String(take!(io))
    @test length(result) == 3
    @test result == "abc"

    # Test 3: Different types
    io = IOBuffer()
    print(io, 42)
    result = String(take!(io))
    @test length(result) == 2
    @test result == "42"

    # Test 4: Chained prints
    io = IOBuffer()
    print(io, "hello")
    print(io, " ")
    print(io, "world")
    result = String(take!(io))
    @test length(result) == 11
    @test result == "hello world"

    # Test 5: Mixed with write
    io = IOBuffer()
    @test write(io, "first") == 5
    print(io, "second")
    result = String(take!(io))
    @test length(result) == 11
    @test result == "firstsecond"
end

true
