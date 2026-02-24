# Test repeated iterator - repeat a value n times or infinitely

using Test
using Iterators

@testset "repeated - repeat a value n times or infinitely" begin

    # Finite repetition via manual iteration
    r = repeated(42, 3)

    # First element
    next = iterate(r)
    @assert next !== nothing
    @assert next[1] == 42

    # Second element
    next = iterate(r, next[2])
    @assert next !== nothing
    @assert next[1] == 42

    # Third element
    next = iterate(r, next[2])
    @assert next !== nothing
    @assert next[1] == 42

    # Fourth should be nothing (only 3 repeats)
    next = iterate(r, next[2])
    @assert next === nothing

    @test (true)
end

true  # Test passed
