# Test empty! and filter! functions

using Test

@testset "In-place collection functions: empty!, filter!" begin

    # =============================================================================
    # empty! tests
    # =============================================================================

    # Test empty! on integer array
    a1 = [1, 2, 3, 4, 5]
    empty!(a1)
    check_empty_1 = length(a1) == 0

    # Test empty! on float array
    a2 = [1.0, 2.0, 3.0]
    empty!(a2)
    check_empty_2 = length(a2) == 0

    # Test empty! returns the array
    a3 = [1, 2, 3]
    result3 = empty!(a3)
    check_empty_3 = result3 === a3

    # Test empty! on already empty array
    a4 = Int64[]
    empty!(a4)
    check_empty_4 = length(a4) == 0

    # =============================================================================
    # filter! tests
    # =============================================================================

    # Test filter! to keep even numbers
    a5 = [1, 2, 3, 4, 5, 6]
    filter!(x -> x % 2 == 0, a5)
    check_filter_1 = length(a5) == 3 && a5[1] == 2 && a5[2] == 4 && a5[3] == 6

    # Test filter! to keep positive numbers
    a6 = [-2, -1, 0, 1, 2]
    filter!(x -> x > 0, a6)
    check_filter_2 = length(a6) == 2 && a6[1] == 1 && a6[2] == 2

    # Test filter! returns the array
    a7 = [1, 2, 3, 4, 5]
    result7 = filter!(x -> x > 2, a7)
    check_filter_3 = result7 === a7

    # Test filter! with all elements passing
    a8 = [1, 2, 3]
    filter!(x -> true, a8)
    check_filter_4 = length(a8) == 3 && a8[1] == 1 && a8[2] == 2 && a8[3] == 3

    # Test filter! with no elements passing
    a9 = [1, 2, 3, 4, 5]
    filter!(x -> false, a9)
    check_filter_5 = length(a9) == 0

    # Test filter! on floats
    a10 = [1.5, 2.5, 3.5, 4.5]
    filter!(x -> x > 2.0, a10)
    check_filter_6 = length(a10) == 3 && a10[1] == 2.5 && a10[2] == 3.5 && a10[3] == 4.5

    # Test filter! on empty array
    a11 = Int64[]
    filter!(x -> true, a11)
    check_filter_7 = length(a11) == 0

    # Test filter! keeps order
    a12 = [5, 4, 3, 2, 1]
    filter!(x -> x % 2 == 1, a12)  # Keep odd: 5, 3, 1
    check_filter_8 = length(a12) == 3 && a12[1] == 5 && a12[2] == 3 && a12[3] == 1

    # =============================================================================
    # Combined operations test
    # =============================================================================

    # Test using both functions together
    a13 = [1, 2, 3, 4, 5]
    filter!(x -> x > 2, a13)  # [3, 4, 5]
    check_combined_1 = length(a13) == 3
    empty!(a13)  # []
    check_combined_2 = length(a13) == 0

    # All checks must pass
    all_empty = check_empty_1 && check_empty_2 && check_empty_3 && check_empty_4
    all_filter = check_filter_1 && check_filter_2 && check_filter_3 && check_filter_4
    all_filter = all_filter && check_filter_5 && check_filter_6 && check_filter_7 && check_filter_8
    all_combined = check_combined_1 && check_combined_2

    @test (all_empty && all_filter && all_combined)
end

true  # Test passed
