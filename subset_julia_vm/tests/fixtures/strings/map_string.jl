# Test map(f, s::String) returns String (Issue #2609)
using Test

@testset "map on String returns String" begin
    # Basic: map uppercase over a string
    @test map(uppercase, "hello") == "HELLO"
    @test map(lowercase, "HELLO") == "hello"

    # Identity function
    @test map(identity, "abc") == "abc"

    # Lambda function
    @test map(c -> uppercase(c), "world") == "WORLD"

    # Empty string
    @test map(uppercase, "") == ""

    # Single character
    @test map(uppercase, "a") == "A"

    # Return type is String
    @test isa(map(uppercase, "hello"), String)
end

true
