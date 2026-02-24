# Test sortperm! function
# sortperm! fills an existing array with permutation indices

using Test

@testset "sortperm! function" begin
    # Basic test
    A = [3.0, 1.0, 4.0, 1.0, 5.0]
    perm = collect(1:5)  # Create Int64 array
    sortperm!(perm, A)
    # Elements at indices 2 and 4 are both 1.0 (smallest)
    # Element at index 1 is 3.0 (middle)
    # Element at index 3 is 4.0 (second largest)
    # Element at index 5 is 5.0 (largest)
    @test perm[1] == 2  # First smallest is at index 2
    @test perm[5] == 5  # Largest is at index 5

    # Test that sortperm and sortperm! give same result
    B = [5.0, 2.0, 8.0, 1.0, 9.0]
    perm1 = sortperm(B)
    perm2 = collect(1:5)
    sortperm!(perm2, B)
    @test perm1[1] == perm2[1]
    @test perm1[2] == perm2[2]
    @test perm1[3] == perm2[3]
    @test perm1[4] == perm2[4]
    @test perm1[5] == perm2[5]

    # Test with already sorted array
    C = [1.0, 2.0, 3.0]
    perm3 = collect(1:3)
    sortperm!(perm3, C)
    @test perm3[1] == 1
    @test perm3[2] == 2
    @test perm3[3] == 3
end

true
