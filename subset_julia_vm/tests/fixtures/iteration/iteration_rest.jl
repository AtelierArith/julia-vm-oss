# Test rest iterator - returns all but the first element

using Test
using Iterators

@testset "rest - all but first element of collection" begin

    # Basic rest of array - test via manual iteration
    arr = [10, 20, 30, 40]
    r = rest(arr)

    # First call to iterate(r) should return second element
    next = iterate(r)
    @assert next !== nothing
    @assert next[1] == 20

    # Second iteration
    next = iterate(r, next[2])
    @assert next !== nothing
    @assert next[1] == 30

    # Third iteration
    next = iterate(r, next[2])
    @assert next !== nothing
    @assert next[1] == 40

    # Fourth should be nothing
    next = iterate(r, next[2])
    @assert next === nothing

    @test (true)
end

true  # Test passed
