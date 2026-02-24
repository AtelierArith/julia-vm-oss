# Test print(io, ...) writing to IOBuffer (Issue #1217)
# Verifies that print(io, ...) writes to the IOBuffer instead of stdout

using Test

@testset "print(io, ...) to IOBuffer" begin
    # Test 1: Basic print to IOBuffer
    @testset "Basic print to IOBuffer" begin
        io = IOBuffer()
        io = print(io, "hello")
        result = take!(io)
        @test length(result) == 5
        @test result == "hello"
    end

    # Test 2: Multiple arguments
    @testset "Multiple arguments" begin
        io = IOBuffer()
        io = print(io, "a", "b", "c")
        result = take!(io)
        @test length(result) == 3
        @test result == "abc"
    end

    # Test 3: Different types
    @testset "Different types" begin
        io = IOBuffer()
        io = print(io, 42)
        result = take!(io)
        @test length(result) == 2
        @test result == "42"
    end

    # Test 4: Chained prints
    @testset "Chained prints" begin
        io = IOBuffer()
        io = print(io, "hello")
        io = print(io, " ")
        io = print(io, "world")
        result = take!(io)
        @test length(result) == 11
        @test result == "hello world"
    end

    # Test 5: Mixed with write
    @testset "Mixed with write" begin
        io = IOBuffer()
        io = write(io, "first")
        io = print(io, "second")
        result = take!(io)
        @test length(result) == 11
        @test result == "firstsecond"
    end
end

true
