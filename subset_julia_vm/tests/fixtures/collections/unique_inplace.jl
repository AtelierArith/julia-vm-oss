# Test unique! function for in-place duplicate removal

using Test

@testset "unique! - remove duplicate elements in-place while preserving order" begin

    # =============================================================================
    # Basic unique! tests
    # =============================================================================

    # Test unique! on array with duplicates
    a1 = [1, 2, 2, 3, 1, 4, 3, 5]
    unique!(a1)
    check1 = length(a1) == 5 && a1[1] == 1 && a1[2] == 2 && a1[3] == 3 && a1[4] == 4 && a1[5] == 5

    # Test unique! preserves order of first occurrences
    a2 = [5, 1, 3, 1, 2, 3, 5]
    unique!(a2)
    check2 = length(a2) == 4 && a2[1] == 5 && a2[2] == 1 && a2[3] == 3 && a2[4] == 2

    # Test unique! returns the array
    a3 = [1, 1, 2, 2]
    result3 = unique!(a3)
    check3 = result3 === a3

    # =============================================================================
    # Edge cases
    # =============================================================================

    # Test unique! on already unique array
    a4 = [1, 2, 3, 4, 5]
    original_len = length(a4)
    unique!(a4)
    check4 = length(a4) == original_len

    # Test unique! on single element array
    a5 = [42]
    unique!(a5)
    check5 = length(a5) == 1 && a5[1] == 42

    # Test unique! on empty array
    a6 = Int64[]
    unique!(a6)
    check6 = length(a6) == 0

    # Test unique! on array with all duplicates
    a7 = [7, 7, 7, 7, 7]
    unique!(a7)
    check7 = length(a7) == 1 && a7[1] == 7

    # =============================================================================
    # Float array tests
    # =============================================================================

    # Test unique! on float array
    a8 = [1.5, 2.5, 1.5, 3.5, 2.5]
    unique!(a8)
    check8 = length(a8) == 3 && a8[1] == 1.5 && a8[2] == 2.5 && a8[3] == 3.5

    # Test unique! on array with consecutive duplicates
    a9 = [1.0, 1.0, 2.0, 2.0, 3.0, 3.0]
    unique!(a9)
    check9 = length(a9) == 3

    # =============================================================================
    # Two element tests
    # =============================================================================

    # Test unique! on two element array - same
    a10 = [5, 5]
    unique!(a10)
    check10 = length(a10) == 1 && a10[1] == 5

    # Test unique! on two element array - different
    a11 = [5, 6]
    unique!(a11)
    check11 = length(a11) == 2 && a11[1] == 5 && a11[2] == 6

    # =============================================================================
    # Final check
    # =============================================================================

    all_basic = check1 && check2 && check3
    all_edge = check4 && check5 && check6 && check7
    all_float = check8 && check9
    all_two = check10 && check11

    @test (all_basic && all_edge && all_float && all_two)
end

true  # Test passed
