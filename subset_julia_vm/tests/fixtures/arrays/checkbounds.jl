# Test checkbounds and checkindex functions

using Test

@testset "checkbounds and checkindex for bounds checking" begin

    a = [1, 2, 3, 4, 5]

    # checkbounds(Bool, A, i) - returns true if index is valid
    r1 = checkbounds(Bool, a, 1)   # true (first element)
    r2 = checkbounds(Bool, a, 5)   # true (last element)
    r3 = checkbounds(Bool, a, 0)   # false (before first)
    r4 = checkbounds(Bool, a, 6)   # false (after last)
    r5 = checkbounds(Bool, a, 3)   # true (middle)

    # checkindex with range
    r6 = checkindex(Bool, 1:10, 5)   # true (in range)
    r7 = checkindex(Bool, 1:10, 0)   # false (before range)
    r8 = checkindex(Bool, 1:10, 11)  # false (after range)
    r9 = checkindex(Bool, 1:10, 1)   # true (first)
    r10 = checkindex(Bool, 1:10, 10) # true (last)

    # All tests must pass
    @test ((r1 && r2 && !r3 && !r4 && r5 && r6 && !r7 && !r8 && r9 && r10) ? 1 : 0) == 1.0
end

true  # Test passed
