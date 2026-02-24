# Test that type errors are caught gracefully instead of causing VM panics
# This tests the fix for issue #1599 - avoiding panic!() in VM error paths

using Test

@testset "Type error handling" begin
    # Test that attempting to negate a string throws a MethodError
    # (not a VM panic)
    caught_error = false
    try
        -"hello"
    catch e
        caught_error = true
        @test isa(e, MethodError) || isa(e, TypeError)
    end
    @test caught_error

    # Test that math operations on strings throw errors gracefully
    caught_add_error = false
    try
        "a" + 1
    catch e
        caught_add_error = true
        @test isa(e, MethodError) || isa(e, TypeError)
    end
    @test caught_add_error

    # Test that sin on a string throws an error
    caught_sin_error = false
    try
        sin("hello")
    catch e
        caught_sin_error = true
        @test isa(e, MethodError) || isa(e, TypeError)
    end
    @test caught_sin_error
end

true
