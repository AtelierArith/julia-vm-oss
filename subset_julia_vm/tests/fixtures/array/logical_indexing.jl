# Logical indexing tests: arr[arr .> 0], arr[[true, false, true]]

using Test

@testset "Logical indexing: arr[arr .> 0], arr[[true, false, true]]" begin

    # Test 1: Basic logical indexing with broadcast comparison
    arr = [1, 2, 3, 4, 5]
    result = arr[arr .> 2]
    @assert result == [3, 4, 5]
    @assert length(result) == 3

    # Test 2: Inline broadcast expression
    arr2 = [-3, -2, -1, 0, 1, 2, 3]
    @assert arr2[arr2 .> 0] == [1, 2, 3]
    @assert arr2[arr2 .< 0] == [-3, -2, -1]
    @assert arr2[arr2 .== 0] == [0]

    # Test 3: Pre-computed mask
    mask = arr .> 3
    @assert arr[mask] == [4, 5]

    # Test 4: Direct boolean array
    bool_arr = [true, false, true, false, true]
    @assert arr[bool_arr] == [1, 3, 5]

    # Test 5: All true / all false masks
    all_true = [true, true, true, true, true]
    @assert arr[all_true] == [1, 2, 3, 4, 5]

    all_false = [false, false, false, false, false]
    @assert arr[all_false] == []

    # Test 6: Float array with logical indexing
    floats = [1.5, -2.3, 3.7, -0.5, 2.1]
    positive = floats[floats .> 0]
    @assert length(positive) == 3
    @assert positive[1] == 1.5
    @assert positive[2] == 3.7
    @assert positive[3] == 2.1

    # Test 7: Mixed comparisons
    @assert arr[arr .>= 3] == [3, 4, 5]
    @assert arr[arr .<= 2] == [1, 2]
    @assert arr[arr .!= 3] == [1, 2, 4, 5]

    # All tests passed
    @test (true)
end

true  # Test passed
