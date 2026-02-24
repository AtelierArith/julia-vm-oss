# Test peel - split iterator into first element and rest
# peel(iter) returns (first_element, rest_iterator) or nothing if empty
# Uses manual iteration to work around VM limitation with for loops over structs

using Test
using Iterators

@testset "peel basic (Issue #759)" begin
    # Test peel on non-empty array
    arr = [1, 2, 3]
    result = peel(arr)

    # result should not be nothing
    @test (result !== nothing)

    # First element should be 1
    first_elem = result[1]
    @test (first_elem == 1)

    # Manually iterate through rest (should yield 2, 3)
    rest_iter = result[2]

    # First iteration of rest should give 2
    next = iterate(rest_iter)
    @assert next !== nothing
    @test (next[1] == 2)

    # Second iteration should give 3
    next = iterate(rest_iter, next[2])
    @assert next !== nothing
    @test (next[1] == 3)

    # Third iteration should be nothing
    next = iterate(rest_iter, next[2])
    @test (next === nothing)
end

true  # Test passed
