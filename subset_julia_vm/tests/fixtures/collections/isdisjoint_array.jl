# Test isdisjoint function with arrays
# isdisjoint(a, b) returns true if a and b have no common elements

using Test

@testset "isdisjoint - check if arrays and sets have no common elements" begin

    # =============================================================================
    # isdisjoint with Array{Int64}
    # =============================================================================

    # Disjoint arrays
    arr1 = [1, 2, 3]
    arr2 = [4, 5, 6]
    check1 = isdisjoint(arr1, arr2)  # true

    # Non-disjoint arrays (have common element 3)
    arr3 = [1, 2, 3]
    arr4 = [3, 4, 5]
    check2 = !isdisjoint(arr3, arr4)  # true (not disjoint)

    # Empty array is disjoint with any array
    arr5 = Int64[]
    arr6 = [1, 2, 3]
    check3 = isdisjoint(arr5, arr6)  # true

    # Both empty arrays are disjoint
    arr7 = Int64[]
    arr8 = Int64[]
    check4 = isdisjoint(arr7, arr8)  # true

    # Single element arrays
    arr9 = [1]
    arr10 = [2]
    check5 = isdisjoint(arr9, arr10)  # true

    arr11 = [1]
    arr12 = [1]
    check6 = !isdisjoint(arr11, arr12)  # true (not disjoint)

    # =============================================================================
    # isdisjoint with mixed Array and Set
    # =============================================================================

    # Set vs Array (disjoint)
    s1 = Set([1, 2, 3])
    a1 = [4, 5, 6]
    check7 = isdisjoint(s1, a1)  # true

    # Array vs Set (not disjoint)
    a2 = [1, 2, 3]
    s2 = Set([3, 4, 5])
    check8 = !isdisjoint(a2, s2)  # true (not disjoint)

    # =============================================================================
    # Final check
    # =============================================================================

    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8)
end

true  # Test passed
