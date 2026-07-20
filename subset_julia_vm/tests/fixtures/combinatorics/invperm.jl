# Test invperm function

using Test

@testset "invperm: compute inverse permutation (Issue #353)" begin

    # invperm returns the inverse permutation
    # If p[i] = j, then invperm(p)[j] = i

    p1 = [2, 4, 3, 1]
    ip1 = invperm(p1)
    @test ip1 == [4, 1, 3, 2]
    @test eltype(ip1) == Int64

    # Identity permutation is its own inverse
    p2 = [1, 2, 3]
    ip2 = invperm(p2)
    @test ip2 == [1, 2, 3]
    @test eltype(ip2) == Int64

    # Applying permutation then inverse returns original
    A = [10, 20, 30, 40]
    @test A[p1][ip1] == A
end

true  # Test passed
