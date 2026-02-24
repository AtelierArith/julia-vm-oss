# Test indexin, isperm, and invperm functions

using Test

@testset "Permutation functions: indexin, isperm, invperm" begin

    # =============================================================================
    # indexin tests
    # =============================================================================

    # Test basic indexin with all elements found
    a1 = [2, 4, 1]
    b1 = [1, 2, 3, 4, 5]
    result1 = indexin(a1, b1)
    check_indexin_1 = result1[1] == 2 && result1[2] == 4 && result1[3] == 1

    # Test indexin with some elements not found
    a2 = [1, 10, 3]
    b2 = [1, 2, 3, 4, 5]
    result2 = indexin(a2, b2)
    check_indexin_2 = result2[1] == 1 && result2[2] === nothing && result2[3] == 3

    # Test indexin with duplicates in b (should return first occurrence)
    a3 = [2]
    b3 = [1, 2, 3, 2, 4]
    result3 = indexin(a3, b3)
    check_indexin_3 = result3[1] == 2  # First occurrence of 2

    # Test indexin with empty a
    a4 = Int64[]
    b4 = [1, 2, 3]
    result4 = indexin(a4, b4)
    check_indexin_4 = length(result4) == 0

    # Test indexin with all elements not found
    a5 = [10, 20, 30]
    b5 = [1, 2, 3, 4, 5]
    result5 = indexin(a5, b5)
    check_indexin_5 = result5[1] === nothing && result5[2] === nothing && result5[3] === nothing

    # =============================================================================
    # isperm tests
    # =============================================================================

    # Test valid permutation
    check_isperm_1 = isperm([1, 2, 3, 4, 5]) == true

    # Test valid permutation (shuffled)
    check_isperm_2 = isperm([3, 1, 4, 2, 5]) == true

    # Test invalid permutation (duplicate)
    check_isperm_3 = isperm([1, 2, 2, 4, 5]) == false

    # Test invalid permutation (out of range)
    check_isperm_4 = isperm([1, 2, 6, 4, 5]) == false

    # Test invalid permutation (missing value)
    check_isperm_5 = isperm([1, 2, 4, 5]) == false  # Length 4, but max is 5

    # Test empty permutation (valid)
    check_isperm_6 = isperm(Int64[]) == true

    # Test single element permutation
    check_isperm_7 = isperm([1]) == true

    # Test single element invalid
    check_isperm_8 = isperm([2]) == false

    # Test two element permutations
    check_isperm_9 = isperm([1, 2]) == true
    check_isperm_10 = isperm([2, 1]) == true
    check_isperm_11 = isperm([1, 1]) == false

    # =============================================================================
    # invperm tests
    # =============================================================================

    # Test invperm with identity permutation
    p1 = [1, 2, 3, 4, 5]
    inv1 = invperm(p1)
    check_invperm_1 = inv1[1] == 1 && inv1[2] == 2 && inv1[3] == 3 && inv1[4] == 4 && inv1[5] == 5

    # Test invperm with reversed permutation
    p2 = [5, 4, 3, 2, 1]
    inv2 = invperm(p2)
    check_invperm_2 = inv2[1] == 5 && inv2[2] == 4 && inv2[3] == 3 && inv2[4] == 2 && inv2[5] == 1

    # Test invperm with shuffled permutation
    p3 = [3, 1, 2]
    inv3 = invperm(p3)
    # invperm(p)[p[i]] should equal i
    check_invperm_3 = inv3[p3[1]] == 1 && inv3[p3[2]] == 2 && inv3[p3[3]] == 3

    # Test invperm is inverse: invperm(invperm(p)) == p
    p4 = [2, 4, 1, 3]
    inv4 = invperm(p4)
    inv_inv4 = invperm(inv4)
    check_invperm_4 = inv_inv4[1] == p4[1] && inv_inv4[2] == p4[2] && inv_inv4[3] == p4[3] && inv_inv4[4] == p4[4]

    # Test invperm with single element
    p5 = [1]
    inv5 = invperm(p5)
    check_invperm_5 = inv5[1] == 1

    # Test invperm with two elements
    p6 = [2, 1]
    inv6 = invperm(p6)
    check_invperm_6 = inv6[1] == 2 && inv6[2] == 1

    # =============================================================================
    # Combined check
    # =============================================================================

    all_indexin = check_indexin_1 && check_indexin_2 && check_indexin_3 && check_indexin_4 && check_indexin_5
    all_isperm = check_isperm_1 && check_isperm_2 && check_isperm_3 && check_isperm_4 && check_isperm_5
    all_isperm = all_isperm && check_isperm_6 && check_isperm_7 && check_isperm_8
    all_isperm = all_isperm && check_isperm_9 && check_isperm_10 && check_isperm_11
    all_invperm = check_invperm_1 && check_invperm_2 && check_invperm_3 && check_invperm_4
    all_invperm = all_invperm && check_invperm_5 && check_invperm_6

    @test (all_indexin && all_isperm && all_invperm)
end

true  # Test passed
