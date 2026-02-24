# Test conj! function for in-place conjugate of arrays
# conj!(A) - for real arrays, conjugate is identity (returns array unchanged)
#
# NOTE: Complex array support is limited due to VM constraints.
# This test covers real array behavior only.

using Test

@testset "conj! - in-place complex conjugate for arrays" begin

    # =============================================================================
    # conj! for integer arrays (should be identity)
    # =============================================================================

    # Test integer array (should remain unchanged)
    r1 = [1, 2, 3, 4, 5]
    result1 = conj!(r1)
    check1 = result1 === r1 && r1[1] == 1 && r1[5] == 5

    # Test negative integers
    r2 = [-1, -2, 0, 2, 1]
    result2 = conj!(r2)
    check2 = result2 === r2 && r2[1] == -1 && r2[3] == 0

    # =============================================================================
    # conj! for float arrays (should be identity)
    # =============================================================================

    # Test float array (should remain unchanged)
    r3 = [1.5, 2.5, 3.5]
    result3 = conj!(r3)
    check3 = result3 === r3 && r3[1] == 1.5 && r3[3] == 3.5

    # Test mixed positive/negative floats
    r4 = [-1.5, 0.0, 2.5, -3.5]
    result4 = conj!(r4)
    check4 = result4 === r4 && r4[1] == -1.5 && r4[4] == -3.5

    # =============================================================================
    # Final check
    # =============================================================================

    @test (check1 && check2 && check3 && check4)
end

true  # Test passed
