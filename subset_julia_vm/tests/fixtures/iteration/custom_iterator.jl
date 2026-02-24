# Test: Custom iterator using user-defined iterate methods
# This is the most important test for the iterate protocol

using Test

struct Counter
    n::Int64
end

function iterate(c::Counter)
    if c.n <= 0
        return nothing
    end
    return (1, 2)  # first element is 1, state is 2 (next element)
end

function iterate(c::Counter, state::Int64)
    if state > c.n
        return nothing
    end
    return (state, state + 1)
end

@testset "Custom iterator with user-defined iterate methods" begin




    # Test: sum 1 + 2 + 3 + 4 + 5 = 15
    total = 0
    for x in Counter(5)
        total += x
    end
    @test (total) == 15.0
end

true  # Test passed
