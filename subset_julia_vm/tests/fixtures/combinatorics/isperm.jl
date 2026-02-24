# Test isperm function

using Test

@testset "isperm: check if array is a valid permutation (Issue #353)" begin

    # Valid permutations
    r1 = isperm([1, 2, 3])       # true - identity permutation
    r2 = isperm([2, 1, 3])       # true - swap first two
    r3 = isperm([3, 2, 1])       # true - reverse
    r4 = isperm([2, 4, 3, 1])    # true - random permutation

    # Invalid permutations
    r5 = isperm([1, 3])          # false - missing element (2)
    r6 = isperm([1, 1, 2])       # false - duplicate element
    r7 = isperm([0, 1, 2])       # false - 0 is out of range
    r8 = isperm([1, 2, 4])       # false - 4 is out of range

    # Edge cases
    r9 = isperm([1])             # true - single element

    # All tests must pass
    @test ((r1 && r2 && r3 && r4 && !r5 && !r6 && !r7 && !r8 && r9) ? 1 : 0) == 1.0
end

true  # Test passed
