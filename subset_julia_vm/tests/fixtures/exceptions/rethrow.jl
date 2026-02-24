# Test rethrow() function (Issue #448)
# Tests that rethrow() re-throws the current exception from catch blocks

using Test

@testset "rethrow function" begin
    # Test rethrow() - re-throws current exception
    caught_outer = false
    caught_inner = false
    try
        try
            error("test error")
        catch e
            caught_inner = true
            rethrow()  # Re-throw the same exception
        end
    catch e
        caught_outer = true
        @test isa(e, ErrorException)
    end
    @test caught_inner
    @test caught_outer

    # Test rethrow(e) - re-throws with new exception
    new_exception_caught = false
    try
        try
            error("original error")
        catch
            rethrow(ErrorException("replacement error"))
        end
    catch e
        new_exception_caught = true
        @test isa(e, ErrorException)
        @test e.msg == "replacement error"
    end
    @test new_exception_caught
end

true
