# Test map! function for in-place array modification
# Supports:
# - map!(f, A) - apply f to each element of A in-place
# - map!(f, dest, A) - apply f to each element of A, store results in dest

using Test

@testset "map! - apply function to array elements in-place" begin

    # =============================================================================
    # map!(f, A) - Apply function to A in-place
    # =============================================================================

    # Test in-place doubling
    a1 = [1, 2, 3, 4]
    result1 = map!(x -> x * 2, a1)
    check1 = result1 === a1 && a1[1] == 2 && a1[2] == 4 && a1[3] == 6 && a1[4] == 8

    # Test in-place squaring
    a2 = [2, 3, 4]
    result2 = map!(x -> x * x, a2)
    check2 = a2[1] == 4 && a2[2] == 9 && a2[3] == 16

    # Test in-place negation
    a3 = [1, -2, 3, -4]
    result3 = map!(x -> -x, a3)
    check3 = a3[1] == -1 && a3[2] == 2 && a3[3] == -3 && a3[4] == 4

    # Test single element
    a4 = [42]
    result4 = map!(x -> x + 8, a4)
    check4 = a4[1] == 50

    # =============================================================================
    # map!(f, dest, A) - Apply function to A, store in dest
    # =============================================================================

    # Test basic mapping to destination (same size)
    a5 = [1, 2, 3, 4, 5]
    dest5 = zeros(Int64, 5)
    result5 = map!(x -> x * 2, dest5, a5)
    check5 = result5 === dest5 && dest5[1] == 2 && dest5[2] == 4 && dest5[3] == 6 && dest5[4] == 8 && dest5[5] == 10

    # Test with smaller dest (processes only as many as dest can hold)
    a6 = [10, 20, 30]
    dest6 = zeros(Int64, 1)
    result6 = map!(x -> x + 1, dest6, a6)
    check6 = length(dest6) == 1 && dest6[1] == 11

    # Test with larger dest (processes only as many as src has, leaves rest unchanged)
    a7 = [1, 2]
    dest7 = zeros(Int64, 5)
    result7 = map!(x -> x * 10, dest7, a7)
    check7 = length(dest7) == 5 && dest7[1] == 10 && dest7[2] == 20 && dest7[3] == 0 && dest7[4] == 0 && dest7[5] == 0

    # Test with identity function
    a8 = [5, 10, 15]
    dest8 = zeros(Int64, 3)
    result8 = map!(identity, dest8, a8)
    check8 = dest8[1] == 5 && dest8[2] == 10 && dest8[3] == 15

    # =============================================================================
    # Final check
    # =============================================================================

    all_inplace = check1 && check2 && check3 && check4
    all_dest = check5 && check6 && check7 && check8

    @test (all_inplace && all_dest)
end

true  # Test passed
