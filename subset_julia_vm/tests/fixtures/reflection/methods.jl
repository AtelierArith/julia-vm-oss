# Test methods function
# methods(f) returns all methods for the given function

using Test

function baz(x)
    x
end

function baz(x::Int64)
    x + 1
end

function baz(x::Float64)
    x * 2.0
end

@testset "methods - get all methods for a function" begin




    # Get all methods for baz
    ms = methods(baz)

    # Should have 3 methods
    @test (length(ms) == 3)
end

true  # Test passed
