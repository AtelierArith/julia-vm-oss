# Test contains function

using Test

@testset "contains - check if string contains substring" begin

    # Basic usage - test positive cases
    @assert contains("hello world", "world")
    @assert contains("hello world", "hello")
    @assert contains("hello world", " ")

    # Negative cases - check that result is falsy
    if contains("hello world", "xyz")
        @assert false "xyz should not be found"
    end

    if contains("", "x")
        @assert false "x should not be in empty string"
    end

    if contains("Hello", "hello")
        @assert false "case sensitivity should matter"
    end

    # Edge cases
    @assert contains("", "")
    @assert contains("hello", "")
    @assert contains("Hello", "Hello")

    @test (true)
end

true  # Test passed
