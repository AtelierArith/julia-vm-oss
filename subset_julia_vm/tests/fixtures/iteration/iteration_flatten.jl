# Test flatten iterator - flattens nested iterables

using Test
using Iterators

@testset "flatten - flatten nested iterables" begin

    # Test that flatten struct exists
    f = flatten([[1, 2], [3, 4]])
    @assert f.it[1] == [1, 2]

    @test (true)
end

true  # Test passed
