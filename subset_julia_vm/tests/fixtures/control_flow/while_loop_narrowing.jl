# While loop type narrowing test (Issue #2303)
#
# Verifies that type narrowing is applied inside while loops,
# similar to how it works in if statements.

using Test

# Test 1: Basic narrowing with !== nothing
function while_narrowing_basic(x)
    while x !== nothing
        return x + 1  # x should be narrowed to exclude Nothing
    end
    return 0
end

# Test 2: Narrowing with iteration (multiple loop executions)
function while_narrowing_iterate()
    # Simulate a nullable iterator pattern
    values = [1, 2, 3]
    i = 1
    total = 0
    while i <= 3
        # Inside the loop, i is guaranteed to be a valid index
        total = total + values[i]
        i = i + 1
    end
    return total
end

# Test 3: Narrowing with early return
function while_narrowing_early_return(arr)
    i = 1
    while i <= length(arr)
        if arr[i] > 10
            return arr[i]  # arr[i] narrowed to be valid
        end
        i = i + 1
    end
    return 0
end

@testset "While loop type narrowing" begin
    @testset "Basic narrowing with !== nothing" begin
        @test while_narrowing_basic(5) == 6
        @test while_narrowing_basic(0) == 1
        @test while_narrowing_basic(nothing) == 0
    end

    @testset "Narrowing with iteration" begin
        @test while_narrowing_iterate() == 6  # 1 + 2 + 3
    end

    @testset "Narrowing with early return" begin
        @test while_narrowing_early_return([1, 2, 15, 4]) == 15
        @test while_narrowing_early_return([1, 2, 3, 4]) == 0
    end
end

true  # Test passed
