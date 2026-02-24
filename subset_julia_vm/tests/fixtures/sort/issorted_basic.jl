# Test issorted function
# Sorted arrays should return true, unsorted should return false

using Test

@testset "issorted: check if array is sorted (Issue #495)" begin

    sorted = [1, 2, 3, 4, 5]
    unsorted = [5, 3, 1]
    single = [42]
    empty = Float64[]

    test1 = issorted(sorted) == true
    test2 = issorted(unsorted) == false
    test3 = issorted(single) == true
    test4 = issorted(empty) == true

    @test (test1 && test2 && test3 && test4)
end

true  # Test passed
