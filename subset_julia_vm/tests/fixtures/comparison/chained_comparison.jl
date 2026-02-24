# Test chained comparisons (Issue #368)
# Julia expands a < b < c to (a < b) && (b < c)

using Test

@testset "Chained comparisons: a < b < c expanded to (a < b) && (b < c) (Issue #368)" begin

    # Basic case from the bug report
    r1 = 1 <= 0 <= 5  # Should be false: (1 <= 0) && (0 <= 5) = false && true = false

    # Other chained comparison cases
    r2 = 1 < 2 < 3    # true: (1 < 2) && (2 < 3) = true && true = true
    r3 = 1 < 3 < 2    # false: (1 < 3) && (3 < 2) = true && false = false
    r4 = 0 <= 5 <= 10 # true: (0 <= 5) && (5 <= 10) = true && true = true
    r5 = 1 == 1 == 1  # true: (1 == 1) && (1 == 1) = true && true = true
    r6 = 1 < 2 > 1    # true: (1 < 2) && (2 > 1) = true && true = true

    # Longer chains
    r7 = 1 <= 2 <= 3 <= 4  # true: (1 <= 2) && (2 <= 3) && (3 <= 4)
    r8 = 1 < 2 < 3 < 4 < 5 # true: all comparisons are true

    # Return true if all tests pass
    @test (!r1 && r2 && !r3 && r4 && r5 && r6 && r7 && r8)
end

true  # Test passed
