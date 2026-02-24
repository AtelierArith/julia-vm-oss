# Test clamp! function for in-place array clamping
# clamp!(a, lo, hi) - clamp all elements to be between lo and hi

using Test

@testset "clamp! - in-place array element clamping" begin

    # =============================================================================
    # Basic clamp! tests
    # =============================================================================

    # Test clamping values that exceed bounds
    a1 = [-5, -1, 0, 5, 10, 15]
    result1 = clamp!(a1, 0, 10)
    check1 = result1 === a1 && a1[1] == 0 && a1[2] == 0 && a1[3] == 0 && a1[4] == 5 && a1[5] == 10 && a1[6] == 10

    # Test all values below lower bound
    a2 = [-10, -5, -1]
    result2 = clamp!(a2, 0, 100)
    check2 = a2[1] == 0 && a2[2] == 0 && a2[3] == 0

    # Test all values above upper bound
    a3 = [100, 200, 300]
    result3 = clamp!(a3, 0, 50)
    check3 = a3[1] == 50 && a3[2] == 50 && a3[3] == 50

    # Test all values within bounds (no change)
    a4 = [3, 5, 7]
    result4 = clamp!(a4, 0, 10)
    check4 = a4[1] == 3 && a4[2] == 5 && a4[3] == 7

    # Test with floats
    a5 = [-1.5, 0.5, 1.5, 2.5]
    result5 = clamp!(a5, 0.0, 2.0)
    check5 = a5[1] == 0.0 && a5[2] == 0.5 && a5[3] == 1.5 && a5[4] == 2.0

    # Test single element
    a6 = [42]
    result6 = clamp!(a6, 0, 10)
    check6 = a6[1] == 10

    # Test edge case: equal lo and hi
    a7 = [1, 5, 10]
    result7 = clamp!(a7, 5, 5)
    check7 = a7[1] == 5 && a7[2] == 5 && a7[3] == 5

    # =============================================================================
    # Final check
    # =============================================================================

    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7)
end

true  # Test passed
