# Test invpermute! function

using Test

@testset "invpermute!: inverse permute vector in-place (Issue #353)" begin

    # invpermute!(v, p) permutes v according to the inverse of p
    # After invpermute!, the element that was at v[i] is now at v[p[i]]

    # Basic test
    A1 = [10.0, 20.0, 30.0, 40.0]
    perm1 = [2, 4, 3, 1]
    invpermute!(A1, perm1)
    # v[p[1]] = v[2] = old[1] = 10
    # v[p[2]] = v[4] = old[2] = 20
    # v[p[3]] = v[3] = old[3] = 30
    # v[p[4]] = v[1] = old[4] = 40
    @test A1[1] == 40.0
    @test A1[2] == 10.0
    @test A1[3] == 30.0
    @test A1[4] == 20.0

    # Identity permutation should leave array unchanged
    A2 = [1.0, 2.0, 3.0]
    perm2 = [1, 2, 3]
    invpermute!(A2, perm2)
    @test A2[1] == 1.0
    @test A2[2] == 2.0
    @test A2[3] == 3.0

    # Reversal permutation (is its own inverse)
    A3 = [1.0, 2.0, 3.0, 4.0]
    perm3 = [4, 3, 2, 1]
    invpermute!(A3, perm3)
    @test A3[1] == 4.0
    @test A3[2] == 3.0
    @test A3[3] == 2.0
    @test A3[4] == 1.0

    # Returns the array
    A4 = [5.0, 6.0]
    result = invpermute!(A4, [2, 1])
    @test result[1] == 6.0
    @test result[2] == 5.0
end

true
