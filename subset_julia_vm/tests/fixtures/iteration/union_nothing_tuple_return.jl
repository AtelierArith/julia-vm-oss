# Test for functions returning Union{Nothing, Tuple}
# Issue #777: Functions with mixed return types (Nothing and Tuple) should work correctly

using Test

# Test function that returns either nothing or a tuple based on condition
function test_union_return(arr)
    y = iterate(arr)
    if y === nothing
        return nothing
    end
    val = y[1]
    s = y[2]
    return (val, s)
end

@testset "Union{Nothing, Tuple} return type" begin
    # Test with non-empty array - should return tuple
    arr = [1, 2, 3]
    r = test_union_return(arr)
    @test r !== nothing
    @test r[1] == 1
    @test r[2] == 1

    # Test with empty array - should return nothing
    empty_arr = Int64[]
    r2 = test_union_return(empty_arr)
    @test r2 === nothing
end

# Test function with explicit return nothing in one branch
function maybe_find_first_even(arr)
    for i in 1:length(arr)
        if arr[i] % 2 == 0
            return (arr[i], i)
        end
    end
    return nothing
end

@testset "Maybe find functions" begin
    # Test finding an even number
    arr_with_even = [1, 3, 4, 5]
    result = maybe_find_first_even(arr_with_even)
    @test result !== nothing
    @test result[1] == 4
    @test result[2] == 3

    # Test with no even numbers
    arr_no_even = [1, 3, 5, 7]
    result2 = maybe_find_first_even(arr_no_even)
    @test result2 === nothing
end

true
