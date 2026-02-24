# Tests for 1-arg functions with scalar overloads after HOF chains (Issue #2296)
# Verifies that findall(A::Array) is selected over findall(x::Bool) when
# the argument comes from a filter/map chain with compile-time type Any.

using Test

# Helper predicate
ispositive(x) = x > 0

@testset "findall after filter (Issue #2296)" begin
    # Basic case: filter returns Vector{Bool} at runtime but may have Any type at compile time
    bools = filter(x -> x, [false, true, false, true])
    result = findall(bools)
    @test result == [1, 2]

    # Chain with explicit predicate function
    data = [true, false, true, false, true]
    filtered = filter(identity, data)
    indices = findall(filtered)
    @test length(indices) == length(filtered)
    @test indices == [1, 2, 3]

    # Empty filter result
    empty_filtered = filter(x -> false, [true, false, true])
    empty_result = findall(empty_filtered)
    @test length(empty_result) == 0
end

@testset "findall after map (Issue #2296)" begin
    # map returns type-inferred array, test dispatch still works
    nums = [1, -2, 3, -4, 5]
    bool_mapped = map(ispositive, nums)
    result = findall(bool_mapped)
    @test result == [1, 3, 5]

    # map with anonymous function
    mapped = map(x -> x > 0, [-1, 0, 1, 2, -3])
    indices = findall(mapped)
    @test indices == [3, 4]
end

@testset "findall with nested HOF chains (Issue #2296)" begin
    # map on filter result
    data = [1, 2, 3, 4, 5, 6]
    filtered = filter(x -> x > 2, data)  # [3, 4, 5, 6]
    bool_map = map(x -> x % 2 == 0, filtered)  # [false, true, false, true]
    result = findall(bool_map)
    @test result == [2, 4]
end

@testset "Multiple 1-arg functions in chain" begin
    # Verify length, sum, etc. also work with filter results
    bools = [true, false, true, true, false]
    filtered = filter(identity, bools)

    # findall should dispatch to Array version
    idx = findall(filtered)
    @test length(idx) == 3

    # Verify the result is usable
    @test idx[1] == 1
    @test idx[end] == 3
end

true
