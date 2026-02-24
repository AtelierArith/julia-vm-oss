# Test: isa infix syntax (a isa T => isa(a, T))
# Tests that the infix form of isa works correctly

using Test

@testset "isa infix syntax: a isa T => isa(a, T)" begin

    x = 1
    @assert x isa Int64
    @assert x isa Integer
    @assert !(x isa Float64)

    y = 1.5
    @assert y isa Float64
    @assert y isa Real
    @assert !(y isa Int64)

    s = "hello"
    @assert s isa String
    @assert s isa AbstractString

    # Test with negation
    @assert !(1 isa Float64)
    @assert !(1.0 isa Int64)

    @test (42) == 42.0
end

true  # Test passed
