# Test product iterator - Cartesian product of iterables

using Test
using Iterators

@testset "product - Cartesian product of iterables" begin

    # Basic product via manual iteration
    p = product([1, 2], [10, 20])

    # First pair: (1, 10)
    next = iterate(p)
    @assert next !== nothing
    pair1 = next[1]
    @assert pair1[1] == 1
    @assert pair1[2] == 10

    # Second pair: (1, 20)
    next = iterate(p, next[2])
    @assert next !== nothing
    pair2 = next[1]
    @assert pair2[1] == 1
    @assert pair2[2] == 20

    # Third pair: (2, 10)
    next = iterate(p, next[2])
    @assert next !== nothing
    pair3 = next[1]
    @assert pair3[1] == 2
    @assert pair3[2] == 10

    # Fourth pair: (2, 20)
    next = iterate(p, next[2])
    @assert next !== nothing
    pair4 = next[1]
    @assert pair4[1] == 2
    @assert pair4[2] == 20

    # Fifth should be nothing (2 x 2 = 4 pairs)
    next = iterate(p, next[2])
    @assert next === nothing

    @test (true)
end

true  # Test passed
