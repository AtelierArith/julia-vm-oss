# Test argmax and argmin functions (Issue #492)
# Based on Julia's base/reduce.jl

using Test

@testset "argmax and argmin: return index of max/min element (Issue #492)" begin

    # Test argmax - returns index of maximum element
    arr1 = [3, 1, 4, 1, 5, 9, 2, 6]
    @assert argmax(arr1) == 6  # 9 is at index 6

    arr2 = [1.5, 2.5, 0.5, 3.5]
    @assert argmax(arr2) == 4  # 3.5 is at index 4

    arr3 = [10, 20, 30]
    @assert argmax(arr3) == 3  # 30 is at index 3

    # First maximum wins for ties
    arr4 = [5, 5, 5]
    @assert argmax(arr4) == 1  # First 5 is at index 1

    # Test argmin - returns index of minimum element
    @assert argmin(arr1) == 2  # 1 is at index 2 (first occurrence)

    arr5 = [1.5, 2.5, 0.5, 3.5]
    @assert argmin(arr5) == 3  # 0.5 is at index 3

    arr6 = [30, 20, 10]
    @assert argmin(arr6) == 3  # 10 is at index 3

    # First minimum wins for ties
    arr7 = [3, 3, 3]
    @assert argmin(arr7) == 1  # First 3 is at index 1

    # Negative numbers
    arr8 = [-5, -3, -10, -1]
    @assert argmax(arr8) == 4  # -1 is at index 4
    @assert argmin(arr8) == 3  # -10 is at index 3

    @test (true)
end

true  # Test passed
