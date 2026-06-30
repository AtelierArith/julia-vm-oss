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

    # Test CompositeException default constructor (now vector-based)
    err3 = CompositeException()
    @test length(err3) == 0
    @test isa(err3, Exception)

    # Test CompositeException with exceptions vector
    excs = Any[]
    push!(excs, ErrorException("first"))
    push!(excs, ErrorException("second"))
    err3b = CompositeException(excs)
    @test length(err3b) == 2
    @test isempty(err3) == true
    @test isempty(err3b) == false

    # Test TaskFailedException with task (now holds task object, not string)
    err4 = TaskFailedException(nothing)
    @test err4.task === nothing
    @test isa(err4, Exception)

    # Test TaskFailedException default constructor
    err4b = TaskFailedException()
    @test err4b.task === nothing

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
