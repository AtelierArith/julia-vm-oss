using Test

# Tests for explicitly thrown exceptions via throw() and error()
# All cases in this file use explicitly constructed exception objects,
# which are properly catchable (as opposed to VM-generated runtime errors).

function catch_bounds_error()
    try
        throw(BoundsError([1, 2, 3], 10))
        return false
    catch e
        return isa(e, BoundsError)
    end
end

function catch_domain_error_with_msg()
    try
        throw(DomainError(-1.0, "negative sqrt"))
        return ""
    catch e
        return e.msg
    end
end

function catch_overflow_error()
    try
        throw(OverflowError("integer overflow"))
        return false
    catch e
        return isa(e, OverflowError)
    end
end

function catch_argument_error_msg()
    try
        throw(ArgumentError("bad argument"))
        return ""
    catch e
        return e.msg
    end
end

function catch_error_fn()
    try
        error("something went wrong")
        return false
    catch e
        return isa(e, ErrorException)
    end
end

function catch_error_fn_msg()
    try
        error("specific message")
        return ""
    catch e
        return e.msg
    end
end

function catch_undefvar_error()
    try
        throw(UndefVarError(:my_var))
        return :none
    catch e
        return e.var
    end
end

function catch_divide_error()
    try
        throw(DivideError())
        return false
    catch e
        return isa(e, DivideError)
    end
end

function no_throw_returns_value()
    try
        return 42
    catch e
        return 0
    end
end

function assert_throws_assertion_error()
    try
        @assert false
        return false
    catch e
        return isa(e, AssertionError)
end
end

function assert_with_msg_throws_assertion_error()
    try
        @assert 1 == 2 "numbers must be equal"
        return ""
    catch e
        return e.msg
    end
end

function assert_passes_when_true()
    try
        @assert true
        @assert 1 == 1 "always true"
        return true
    catch e
        return false
    end
end

@testset "explicit throw() and error() are catchable" begin
    @test catch_bounds_error() == true
    @test catch_domain_error_with_msg() == "negative sqrt"
    @test catch_overflow_error() == true
    @test catch_argument_error_msg() == "bad argument"
    @test catch_error_fn() == true
    @test catch_error_fn_msg() == "specific message"
    @test catch_undefvar_error() == :my_var
    @test catch_divide_error() == true
    @test no_throw_returns_value() == 42
end

@testset "@assert throws AssertionError (Issue #3043)" begin
    @test assert_throws_assertion_error() == true
    @test assert_with_msg_throws_assertion_error() == "numbers must be equal"
    @test assert_passes_when_true() == true
end

true
