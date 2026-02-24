# Test showerror function for exception display
# Tests the _showerror_str internal helper function directly
# Note: Using direct calls to _showerror_str instead of sprint_showerror
# to avoid Issue #1218 (string comparison bug with nested function returns)

using Test

@testset "_showerror_str function" begin
    # Test DivideError
    @testset "DivideError" begin
        ex = DivideError()
        result = _showerror_str(ex)
        @test result == "DivideError: integer division error"
    end

    # Test EOFError
    @testset "EOFError" begin
        ex = EOFError()
        result = _showerror_str(ex)
        @test result == "EOFError: read end of file"
    end

    # Test StackOverflowError
    @testset "StackOverflowError" begin
        ex = StackOverflowError()
        result = _showerror_str(ex)
        @test result == "StackOverflowError:"
    end

    # Test OutOfMemoryError
    @testset "OutOfMemoryError" begin
        ex = OutOfMemoryError()
        result = _showerror_str(ex)
        @test result == "OutOfMemoryError()"
    end

    # Test UndefRefError
    @testset "UndefRefError" begin
        ex = UndefRefError()
        result = _showerror_str(ex)
        @test result == "UndefRefError: access to undefined reference"
    end

    # Test ErrorException using startswith and length
    @testset "ErrorException" begin
        ex = ErrorException("test message")
        result = _showerror_str(ex)
        @test startswith(result, "test")
        @test length(result) == 12
    end

    # Test DimensionMismatch using startswith
    @testset "DimensionMismatch" begin
        ex = DimensionMismatch("size mismatch")
        result = _showerror_str(ex)
        @test startswith(result, "DimensionMismatch: ")
    end

    # Test AssertionError using startswith
    @testset "AssertionError" begin
        ex = AssertionError("failed")
        result = _showerror_str(ex)
        @test startswith(result, "AssertionError: ")
    end

    # Test ArgumentError using startswith
    @testset "ArgumentError" begin
        ex = ArgumentError("invalid")
        result = _showerror_str(ex)
        @test startswith(result, "ArgumentError: ")
    end

    # Test OverflowError using startswith
    @testset "OverflowError" begin
        ex = OverflowError("overflow")
        result = _showerror_str(ex)
        @test startswith(result, "OverflowError: ")
    end

    # Test KeyError using startswith
    @testset "KeyError" begin
        ex = KeyError("mykey")
        result = _showerror_str(ex)
        @test startswith(result, "KeyError: key ")
    end
end

true
