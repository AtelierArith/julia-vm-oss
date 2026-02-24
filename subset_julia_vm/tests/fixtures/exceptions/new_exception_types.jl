# Test new exception types added in Issue #429
# Tests CanonicalIndexError, CapturedException, CompositeException,
# TaskFailedException, and ProcessFailedException

using Test

@testset "New Exception Types" begin
    # Test CanonicalIndexError
    err1 = CanonicalIndexError("getindex", "Array{Int64}")
    @test err1.func == "getindex"
    @test err1.arr_type == "Array{Int64}"
    @test isa(err1, Exception)

    # Test CapturedException with message
    inner_err = ErrorException("inner error")
    err2 = CapturedException(inner_err, "captured during task execution")
    @test err2.msg == "captured during task execution"
    @test isa(err2, Exception)

    # Test CapturedException default constructor
    err2b = CapturedException(inner_err)
    @test err2b.msg == ""

    # Test CompositeException default constructor
    err3 = CompositeException()
    @test err3.count == 0
    @test err3.first_msg == ""
    @test isa(err3, Exception)

    # Test CompositeException with data
    err3b = CompositeException(3, "first of 3 failures")
    @test err3b.count == 3
    @test err3b.first_msg == "first of 3 failures"

    # Test TaskFailedException with message
    err4 = TaskFailedException("task failed with error")
    @test err4.msg == "task failed with error"
    @test isa(err4, Exception)

    # Test TaskFailedException default constructor
    err4b = TaskFailedException()
    @test err4b.msg == ""

    # Test ProcessFailedException with exitcode and message
    err5 = ProcessFailedException(127, "command not found")
    @test err5.exitcode == 127
    @test err5.msg == "command not found"
    @test isa(err5, Exception)

    # Test ProcessFailedException with just exitcode
    err5b = ProcessFailedException(1)
    @test err5b.exitcode == 1
    @test err5b.msg == ""
end

true
