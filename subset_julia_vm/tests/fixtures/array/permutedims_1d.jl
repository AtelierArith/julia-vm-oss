# Test permutedims for 1D vector -> 1×N row vector

using Test

@testset "permutedims for 1D vector returns 1×N row vector" begin
    v = [1, 2, 3]
    pv = permutedims(v)
    # Check size is (1, 3) and values are preserved
    @test (size(pv) == (1, 3) && pv[1,1] == 1 && pv[1,2] == 2 && pv[1,3] == 3)
end

true  # Test passed
