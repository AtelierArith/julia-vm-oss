# Test P0 error exception types (Issue #498)
# Verifies that all P0 error types are defined and can be instantiated

using Test

@testset "P0 Error Exception Types" begin
    # Test 1: ErrorException
    e1 = ErrorException("test error")
    @test isa(e1, ErrorException)
    @test isa(e1, Exception)
    @test e1.msg == "test error"

    # Test 2: ArgumentError
    e2 = ArgumentError("invalid argument")
    @test isa(e2, ArgumentError)
    @test isa(e2, Exception)
    @test e2.msg == "invalid argument"

    # Test 3: AssertionError
    e3 = AssertionError("assertion failed")
    @test isa(e3, AssertionError)
    @test isa(e3, Exception)
    @test e3.msg == "assertion failed"

    e3_empty = AssertionError()
    @test isa(e3_empty, AssertionError)
    @test e3_empty.msg == ""

    # Test 4: DivideError
    e4 = DivideError()
    @test isa(e4, DivideError)
    @test isa(e4, Exception)

    # Test 5: DomainError
    e5 = DomainError(-1.0, "sqrt of negative number")
    @test isa(e5, DomainError)
    @test isa(e5, Exception)
    @test e5.val == -1.0
    @test e5.msg == "sqrt of negative number"

    e5_no_msg = DomainError(0.0)
    @test isa(e5_no_msg, DomainError)
    @test e5_no_msg.msg == ""

    # Test 6: InexactError
    e6 = InexactError(:Int64, Int64, 3.14)
    @test isa(e6, InexactError)
    @test isa(e6, Exception)
    @test e6.func == :Int64
    # Note: Official Julia uses args field, SubsetJuliaVM uses T and val

    # Test 7: TypeError
    e7 = TypeError(:typeassert, "context", Int64, String)
    @test isa(e7, TypeError)
    @test isa(e7, Exception)
    @test e7.func == :typeassert

    # Test 8: MethodError
    e8 = MethodError(abs, ("not a number",))
    @test isa(e8, MethodError)
    @test isa(e8, Exception)

    # Test 9: UndefVarError
    e9 = UndefVarError(:undefined_var)
    @test isa(e9, UndefVarError)
    @test isa(e9, Exception)
    @test e9.var == :undefined_var

    # Test 10: UndefKeywordError
    e10 = UndefKeywordError(:missing_keyword)
    @test isa(e10, UndefKeywordError)
    @test isa(e10, Exception)
    @test e10.var == :missing_keyword

    # Test 11: UndefRefError
    e11 = UndefRefError()
    @test isa(e11, UndefRefError)
    @test isa(e11, Exception)

    # Test 12: EOFError
    e12 = EOFError()
    @test isa(e12, EOFError)
    @test isa(e12, Exception)

    # Test 13: ParseError (defined in SubsetJuliaVM)
    # Note: In official Julia, ParseError is in Meta module
    # In SubsetJuliaVM it's exported from Base for convenience
    # Skip this test in official Julia
    # e13 = ParseError("syntax error", nothing)
    # @test isa(e13, ParseError)
    # @test isa(e13, Exception)
    # @test e13.msg == "syntax error"

    # Test 14: OverflowError
    e14 = OverflowError("integer overflow")
    @test isa(e14, OverflowError)
    @test isa(e14, Exception)
    @test e14.msg == "integer overflow"

    # Test 15: OutOfMemoryError
    e15 = OutOfMemoryError()
    @test isa(e15, OutOfMemoryError)
    @test isa(e15, Exception)

    # Test 16: StackOverflowError
    e16 = StackOverflowError()
    @test isa(e16, StackOverflowError)
    @test isa(e16, Exception)

    # Test 17: SystemError
    e17 = SystemError("open failed", 2)
    @test isa(e17, SystemError)
    @test isa(e17, Exception)
    @test e17.prefix == "open failed"
    @test e17.errnum == 2

    # Test 18: IOError (defined in SubsetJuliaVM)
    # Note: In official Julia, IOError is in Base.Libuv module
    # In SubsetJuliaVM it's exported from Base for convenience
    # Skip this test in official Julia
    # e18 = IOError("read failed", -1)
    # @test isa(e18, IOError)
    # @test isa(e18, Exception)
    # @test e18.msg == "read failed"
    # @test e18.code == -1

    # Test 19: LoadError
    inner_err = ErrorException("inner error")
    e19 = LoadError("file.jl", 10, inner_err)
    @test isa(e19, LoadError)
    @test isa(e19, Exception)
    @test e19.file == "file.jl"
    @test e19.line == 10
    @test isa(e19.error, ErrorException)

    # Note: WrappedException is abstract and not exported in official Julia
    # In SubsetJuliaVM it's exported for convenience
end

true
