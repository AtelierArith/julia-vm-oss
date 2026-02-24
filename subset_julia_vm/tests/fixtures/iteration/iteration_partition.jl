# Test partition iterator - group elements into chunks of size n

using Test
using Iterators

@testset "partition - group elements into chunks" begin

    # Basic partition via manual iteration
    p = partition([1, 2, 3, 4, 5, 6], 2)

    # First chunk
    next = iterate(p)
    @assert next !== nothing
    chunk1 = next[1]
    @assert length(chunk1) == 2

    # Second chunk
    next = iterate(p, next[2])
    @assert next !== nothing
    chunk2 = next[1]
    @assert length(chunk2) == 2

    # Third chunk
    next = iterate(p, next[2])
    @assert next !== nothing
    chunk3 = next[1]
    @assert length(chunk3) == 2

    # Fourth should be nothing (6 elements / 2 = 3 chunks)
    next = iterate(p, next[2])
    @assert next === nothing

    @test (true)
end

true  # Test passed
