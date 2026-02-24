# Test cycle iterator - infinite repetition of collection

using Test
using Iterators

@testset "cycle - infinite repetition of collection" begin

    # Basic cycle test via manual iteration
    c = cycle([10, 20, 30])

    # First element
    next = iterate(c)
    @assert next !== nothing
    @assert next[1] == 10

    # Second element
    next = iterate(c, next[2])
    @assert next !== nothing
    @assert next[1] == 20

    # Third element
    next = iterate(c, next[2])
    @assert next !== nothing
    @assert next[1] == 30

    # Fourth element (back to start - cycle)
    next = iterate(c, next[2])
    @assert next !== nothing
    @assert next[1] == 10

    @test (true)
end

true  # Test passed
