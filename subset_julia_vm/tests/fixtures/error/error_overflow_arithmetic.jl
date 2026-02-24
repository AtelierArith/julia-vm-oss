using Test

# Tests for integer overflow errors (Issue #3046)
# Julia raises OverflowError for operations that exceed the range of Int64.

function factorial_overflow_caught()
    caught = false
    try
        # factorial(21) overflows Int64
        result = factorial(21)
    catch e
        caught = isa(e, OverflowError)
    end
    return caught
end

function factorial_small_no_error()
    caught = false
    try
        result = factorial(20)
    catch e
        caught = true
    end
    return caught
end

function overflow_add_caught()
    caught = false
    try
        # Explicitly throw OverflowError
        throw(OverflowError("overflow in addition"))
    catch e
        caught = isa(e, OverflowError)
    end
    return caught
end

function overflow_msg_accessible()
    msg = ""
    try
        throw(OverflowError("test overflow"))
    catch e
        msg = e.msg
    end
    return msg
end

@testset "integer overflow errors are catchable (Issue #3046)" begin
    @test factorial_overflow_caught()
    @test !factorial_small_no_error()
    @test overflow_add_caught()
    @test overflow_msg_accessible() == "test overflow"
end

true
