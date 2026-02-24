# Test exception types: DimensionMismatch, KeyError, StringIndexError, OverflowError, StackOverflowError, OutOfMemoryError, UndefRefError, UndefVarError, MethodError, ParseError, SystemError, IOError, LoadError, MissingException, InvalidStateException

using Test
using Base: IOError, ParseError, WrappedException

@testset "Exception types: DimensionMismatch, KeyError, StringIndexError (Issue #342)" begin

    # Test DimensionMismatch
    e1 = DimensionMismatch("arrays have different dimensions")
    @assert e1.msg == "arrays have different dimensions"

    e2 = DimensionMismatch()
    @assert e2.msg == ""

    # Test KeyError
    k = KeyError("missing_key")
    @assert k.key == "missing_key"

    # Test StringIndexError
    s = StringIndexError("hello", 3)
    @assert s.string == "hello"
    @assert s.index == 3

    # Test OverflowError
    o1 = OverflowError("integer overflow")
    @assert o1.msg == "integer overflow"

    # Test StackOverflowError
    # StackOverflowError is a zero-field struct
    soe = StackOverflowError()
    @assert soe isa StackOverflowError

    # Test OutOfMemoryError
    # OutOfMemoryError is a zero-field struct
    oome = OutOfMemoryError()
    @assert oome isa OutOfMemoryError

    # Test UndefRefError
    # UndefRefError is a zero-field struct
    ure = UndefRefError()
    @assert ure isa UndefRefError

    # Test UndefVarError
    uve = UndefVarError(:undefined_var)
    @assert uve.var == :undefined_var

    # Test MethodError
    # Use a symbol instead of a function since function values aren't supported as arguments
    me = MethodError(:some_function, (1, "two"))
    @assert me.args == (1, "two")

    # Test ParseError
    pe1 = ParseError("unexpected token")
    @assert pe1.msg == "unexpected token"
    @assert isnothing(pe1.detail)

    pe2 = ParseError("syntax error", 42)
    @assert pe2.msg == "syntax error"
    @assert pe2.detail == 42

    # Test SystemError
    se = SystemError("opening file", 2)
    @assert se.prefix == "opening file"
    @assert se.errnum == 2

    # Test IOError
    ioe = IOError("open failed", -2)
    @assert ioe.msg == "open failed"
    @assert ioe.code == -2

    # Test LoadError
    le = LoadError("test.jl", 10, 404)
    @assert le.file == "test.jl"
    @assert le.line == 10
    @assert le.error == 404

    # Test MissingException
    me2 = MissingException("cannot convert missing to Int")
    @assert me2.msg == "cannot convert missing to Int"

    # Test InvalidStateException
    ise = InvalidStateException("channel is closed", :closed)
    @assert ise.msg == "channel is closed"
    @assert ise.state == :closed

    # Test Exception abstract type exists
    # (Exception is the supertype of all exception types)
    @assert DimensionMismatch <: Exception
    @assert KeyError <: Exception
    @assert StringIndexError <: Exception
    @assert OverflowError <: Exception
    @assert StackOverflowError <: Exception
    @assert OutOfMemoryError <: Exception
    @assert UndefRefError <: Exception
    @assert UndefVarError <: Exception
    @assert MethodError <: Exception
    @assert ParseError <: Exception
    @assert SystemError <: Exception
    @assert IOError <: Exception
    @assert WrappedException <: Exception
    @assert LoadError <: WrappedException
    @assert MissingException <: Exception
    @assert InvalidStateException <: Exception

    @test (true)
end

true  # Test passed
