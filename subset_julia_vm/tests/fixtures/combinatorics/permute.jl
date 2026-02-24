# Test permute! function

using Test

@testset "permute!: permute vector in-place (Issue #353)" begin

    # permute!(v, p) permutes v according to p
    # After permute!, v[i] contains the element that was at v[p[i]]

    # Basic test
    A1 = [10.0, 20.0, 30.0, 40.0]
    perm1 = [2, 4, 3, 1]
    permute!(A1, perm1)
    # v[1] = old[p[1]] = old[2] = 20
    # v[2] = old[p[2]] = old[4] = 40
    # v[3] = old[p[3]] = old[3] = 30
    # v[4] = old[p[4]] = old[1] = 10
    @test A1[1] == 20.0
    @test A1[2] == 40.0
    @test A1[3] == 30.0
    @test A1[4] == 10.0

    # Identity permutation should leave array unchanged
    A2 = [1.0, 2.0, 3.0]
    perm2 = [1, 2, 3]
    permute!(A2, perm2)
    @test A2[1] == 1.0
    @test A2[2] == 2.0
    @test A2[3] == 3.0

    # Reversal permutation
    A3 = [1.0, 2.0, 3.0, 4.0]
    perm3 = [4, 3, 2, 1]
    permute!(A3, perm3)
    @test A3[1] == 4.0
    @test A3[2] == 3.0
    @test A3[3] == 2.0
    @test A3[4] == 1.0

    # Returns the array
    A4 = [5.0, 6.0]
    result = permute!(A4, [2, 1])
    @test result[1] == 6.0
    @test result[2] == 5.0
end

true
