using Test

# Tests for error handling with multiple catch scenarios (Issue #3046)
# Verifies correct error propagation through nested and sequential try/catch blocks.

function catch_preserves_error_type()
    error_type = ""
    try
        throw(ArgumentError("bad arg"))
    catch e
        error_type = string(typeof(e))
    end
    return error_type
end

function sequential_errors_independent()
    first_caught = false
    second_caught = false
    try
        throw(DomainError(-1, "negative"))
    catch e
        first_caught = isa(e, DomainError)
    end
    try
        throw(BoundsError([1, 2], 5))
    catch e
        second_caught = isa(e, BoundsError)
    end
    return first_caught && second_caught
end

function error_in_catch_block()
    outer_caught = false
    try
        try
            throw(ErrorException("inner"))
        catch e
            # Throwing in catch block should propagate to outer try/catch
            throw(ArgumentError("from catch"))
        end
    catch e
        outer_caught = isa(e, ArgumentError)
    end
    return outer_caught
end

function no_error_returns_try_value()
    result = 0
    try
        result = 42
    catch e
        result = -1
    end
    return result
end

function finally_with_error()
    finally_ran = false
    caught = false
    try
        throw(ErrorException("test"))
    catch e
        caught = true
    finally
        finally_ran = true
    end
    return caught && finally_ran
end

function nested_finally_all_run()
    outer_finally = false
    inner_finally = false
    try
        try
            throw(ErrorException("inner"))
        catch e
            # caught
        finally
            inner_finally = true
        end
    finally
        outer_finally = true
    end
    return outer_finally && inner_finally
end

@testset "multiple catch scenarios (Issue #3046)" begin
    @test catch_preserves_error_type() == "ArgumentError"
    @test sequential_errors_independent()
    @test error_in_catch_block()
    @test no_error_returns_try_value() == 42
    @test finally_with_error()
    @test nested_finally_all_run()
end

true
