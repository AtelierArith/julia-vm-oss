using Test

# Tests for nested try/catch blocks.
# Inner exceptions must not leak to outer blocks.
# Re-throwing from catch must propagate to outer block.

function nested_catch_inner()
    # Inner exception caught by inner catch; outer not reached
    outer_caught = false
    inner_caught = false
    try
        try
            throw(ErrorException("inner"))
        catch e
            inner_caught = isa(e, ErrorException)
        end
    catch e
        outer_caught = true
    end
    return (inner_caught, outer_caught)
end

function nested_rethrow_to_outer()
    # Inner catch re-throws a different exception; outer catch receives it
    try
        try
            throw(ArgumentError("original"))
        catch e
            throw(ErrorException("wrapped: " * e.msg))
        end
    catch e
        return isa(e, ErrorException)
    end
    return false
end

function sequential_try_blocks()
    # Two separate try blocks, each catches its own exception
    r1 = false
    r2 = false
    try
        throw(ErrorException("first"))
    catch e
        r1 = isa(e, ErrorException)
    end
    try
        throw(ArgumentError("second"))
    catch e
        r2 = isa(e, ArgumentError)
    end
    return (r1, r2)
end

function exception_type_changes_in_nested_catch()
    # Catch ArgumentError, throw OverflowError, outer catches OverflowError
    try
        try
            throw(ArgumentError("arg"))
        catch e
            throw(OverflowError("overflow from " * e.msg))
        end
    catch e
        return isa(e, OverflowError)
    end
    return false
end

function exception_message_accessible()
    msg = ""
    try
        throw(ArgumentError("hello world"))
    catch e
        msg = e.msg
    end
    return msg
end

function inner_exception_does_not_escape()
    # No outer catch — inner exception must be caught by inner catch only
    inner_caught = false
    try
        throw(DomainError(-1.0, "neg"))
    catch e
        inner_caught = isa(e, DomainError)
    end
    return inner_caught
end

function typeof_assign_to_outer()
    # typeof(e) assigned to outer variable must persist after catch block (Issue #3044)
    t = nothing
    try
        throw(ErrorException("test"))
    catch e
        t = typeof(e)
    end
    return t
end

function type_value_assign_to_outer()
    # Assigning a type object to an outer variable in catch must persist (Issue #3044)
    t = nothing
    try
        throw(ErrorException("test"))
    catch e
        t = ErrorException
    end
    return t
end

function typeof_outer_in_nested_catch()
    # typeof inside nested catch assigned to outer variable
    t = nothing
    try
        try
            throw(ArgumentError("inner"))
        catch inner_e
            t = typeof(inner_e)
        end
    catch e
        t = typeof(e)
    end
    return t
end

@testset "nested try/catch semantics" begin
    (inner, outer) = nested_catch_inner()
    @test inner == true
    @test outer == false

    @test nested_rethrow_to_outer() == true
    @test sequential_try_blocks() == (true, true)
    @test exception_type_changes_in_nested_catch() == true
    @test exception_message_accessible() == "hello world"
    @test inner_exception_does_not_escape() == true
end

@testset "typeof in catch block assigns to outer variable (Issue #3044)" begin
    @test typeof_assign_to_outer() == ErrorException
    @test type_value_assign_to_outer() == ErrorException
    @test typeof_outer_in_nested_catch() == ArgumentError
end

true
