# Test that exception types are usable (exported or Base-qualified)

using Test
using Base: IOError, ParseError

@testset "Exception types available" begin
    # Test that exception types exist and are subtypes of Exception
    @test ArgumentError <: Exception
    @test AssertionError <: Exception
    @test BoundsError <: Exception
    @test DivideError <: Exception
    @test DomainError <: Exception
    @test DimensionMismatch <: Exception
    @test EOFError <: Exception
    @test ErrorException <: Exception
    @test InexactError <: Exception
    @test InvalidStateException <: Exception
    @test IOError <: Exception
    @test KeyError <: Exception
    @test MethodError <: Exception
    @test MissingException <: Exception
    @test OutOfMemoryError <: Exception
    @test OverflowError <: Exception
    @test ParseError <: Exception
    @test StackOverflowError <: Exception
    @test StringIndexError <: Exception
    @test SystemError <: Exception
    @test TypeError <: Exception
    @test UndefKeywordError <: Exception
    @test UndefRefError <: Exception
    @test UndefVarError <: Exception
end

@testset "Exception construction" begin
    # Test that exceptions can be constructed
    @test ErrorException("test").msg == "test"
    @test DimensionMismatch("sizes").msg == "sizes"
    @test KeyError("key").key == "key"
    @test StringIndexError("str", 5).string == "str"
    @test StringIndexError("str", 5).index == 5
end

true  # Test passed
