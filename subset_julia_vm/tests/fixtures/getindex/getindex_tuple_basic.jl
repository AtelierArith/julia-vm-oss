# Test: getindex on Tuple

using Test

@testset "getindex(tuple, i) returns tuple element" begin
    t = (1, 2, 3)

    # getindex(t, i) returns the element at position i
    @assert getindex(t, 1) == 1
    @assert getindex(t, 2) == 2
    @assert getindex(t, 3) == 3

    # Compare with indexing syntax
    @assert getindex(t, 2) == t[2]

    @test (true)
end

true  # Test passed
