# Test splice! function for in-place array modification
# Supports:
# - splice!(a, i) - remove element at index i
# - splice!(a, i, v) - replace element at index i with v (single value or array)

using Test

@testset "splice! - remove and optionally replace elements in-place" begin

    # =============================================================================
    # splice!(a, i) - Remove single element
    # =============================================================================

    # Test removing element at index
    a1 = [1, 2, 3, 4, 5]
    v1 = splice!(a1, 3)
    check1 = v1 == 3 && length(a1) == 4 && a1[1] == 1 && a1[2] == 2 && a1[3] == 4 && a1[4] == 5

    # Test removing first element
    a2 = [10, 20, 30]
    v2 = splice!(a2, 1)
    check2 = v2 == 10 && length(a2) == 2 && a2[1] == 20 && a2[2] == 30

    # Test removing last element
    a3 = [10, 20, 30]
    v3 = splice!(a3, 3)
    check3 = v3 == 30 && length(a3) == 2 && a3[1] == 10 && a3[2] == 20

    # =============================================================================
    # splice!(a, i, v) - Replace with single value
    # =============================================================================

    # Test replacing element
    a4 = [1, 2, 3, 4, 5]
    v4 = splice!(a4, 3, 99)
    check4 = v4 == 3 && length(a4) == 5 && a4[3] == 99

    # Test replacing first element
    a5 = [1, 2, 3]
    v5 = splice!(a5, 1, 100)
    check5 = v5 == 1 && a5[1] == 100

    # Test replacing last element
    a6 = [1, 2, 3]
    v6 = splice!(a6, 3, 200)
    check6 = v6 == 3 && a6[3] == 200

    # =============================================================================
    # splice!(a, i, arr) - Replace with multiple values
    # =============================================================================

    # Test replacing one element with multiple
    a7 = [1, 2, 3, 4, 5]
    v7 = splice!(a7, 3, [30, 31, 32])
    check7_a = v7 == 3
    check7_b = length(a7) == 7
    check7_c = a7[1] == 1 && a7[2] == 2 && a7[3] == 30 && a7[4] == 31 && a7[5] == 32 && a7[6] == 4 && a7[7] == 5
    check7 = check7_a && check7_b && check7_c

    # Test replacing first element with array
    a8 = [1, 2, 3]
    v8 = splice!(a8, 1, [10, 20])
    check8 = v8 == 1 && length(a8) == 4 && a8[1] == 10 && a8[2] == 20 && a8[3] == 2 && a8[4] == 3

    # Test replacing with single-element array
    a9 = [1, 2, 3]
    v9 = splice!(a9, 2, [99])
    check9 = v9 == 2 && length(a9) == 3 && a9[2] == 99

    # Test replacing with empty array (deletion)
    a10 = [1, 2, 3, 4, 5]
    v10 = splice!(a10, 3, Int64[])
    check10 = v10 == 3 && length(a10) == 4 && a10[3] == 4

    # =============================================================================
    # Final check
    # =============================================================================

    all_remove = check1 && check2 && check3
    all_replace = check4 && check5 && check6
    all_array = check7 && check8 && check9 && check10

    @test (all_remove && all_replace && all_array)
end

true  # Test passed
