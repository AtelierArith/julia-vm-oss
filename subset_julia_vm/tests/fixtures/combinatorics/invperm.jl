# Test invperm function

using Test

@testset "invperm: compute inverse permutation (Issue #353)" begin

    # invperm returns the inverse permutation
    # If p[i] = j, then invperm(p)[j] = i

    p1 = [2, 4, 3, 1]
    ip1 = invperm(p1)
    # p1[1]=2, so ip1[2]=1
    # p1[2]=4, so ip1[4]=2
    # p1[3]=3, so ip1[3]=3
    # p1[4]=1, so ip1[1]=4
    r1 = ip1[1] == 4.0
    r2 = ip1[2] == 1.0
    r3 = ip1[3] == 3.0
    r4 = ip1[4] == 2.0

    # Identity permutation is its own inverse
    p2 = [1, 2, 3]
    ip2 = invperm(p2)
    r5 = ip2[1] == 1.0 && ip2[2] == 2.0 && ip2[3] == 3.0

    # Applying permutation then inverse returns original
    # If A = [10, 20, 30, 40], then A[p1] = [20, 40, 30, 10]
    # Then A[p1][invperm(p1)] should give back [10, 20, 30, 40]
    # But since invperm returns Float64, we test differently:
    # p1 composed with invperm(p1) should be identity
    # That is: p1[invperm(p1)[i]] = i for all i
    r6 = true
    for i in 1:4
        idx = Int64(ip1[i])
        if p1[idx] != i
            r6 = false
        end
    end

    @test ((r1 && r2 && r3 && r4 && r5 && r6) ? 1 : 0) == 1.0
end

true  # Test passed
