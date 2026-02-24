# Test: Set iteration in for loops (Issue #1122)
# In Julia, iterating over a Set yields each element directly.

using Test

@testset "Set iteration" begin
    # Basic Set iteration
    s = Set([1, 2, 3])

    # Collect elements via iteration
    elements_collected = Int[]
    for x in s
        push!(elements_collected, x)
    end

    # Check that we got all elements (order may vary)
    @test length(elements_collected) == 3
    @test 1 in elements_collected
    @test 2 in elements_collected
    @test 3 in elements_collected

    # Empty Set iteration
    empty_s = Set{Int}()
    count = 0
    for _ in empty_s
        count = count + 1
    end
    @test count == 0

    # Sum elements via iteration
    s2 = Set([10, 20, 30])
    total = 0
    for x in s2
        total = total + x
    end
    @test total == 60
end

true
