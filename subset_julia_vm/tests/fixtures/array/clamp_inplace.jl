# Test clamp! - clamp array values in place
# clamp!(a, lo, hi) restricts each element to [lo, hi]

using Test

@testset "clamp! function" begin
    # Basic test - clamp values to range
    A = [1.0, 5.0, 10.0, 15.0]
    clamp!(A, 3.0, 12.0)
    @test A[1] == 3.0   # 1.0 clamped up to 3.0
    @test A[2] == 5.0   # 5.0 unchanged (in range)
    @test A[3] == 10.0  # 10.0 unchanged (in range)
    @test A[4] == 12.0  # 15.0 clamped down to 12.0

    # Test with negative values
    B = [-10.0, -5.0, 0.0, 5.0, 10.0]
    clamp!(B, -3.0, 3.0)
    @test B[1] == -3.0
    @test B[2] == -3.0
    @test B[3] == 0.0
    @test B[4] == 3.0
    @test B[5] == 3.0

    # Test with all values in range
    C = [2.0, 3.0, 4.0]
    clamp!(C, 1.0, 5.0)
    @test C[1] == 2.0
    @test C[2] == 3.0
    @test C[3] == 4.0

    # Test with all values below range
    D = [1.0, 2.0, 3.0]
    clamp!(D, 5.0, 10.0)
    @test D[1] == 5.0
    @test D[2] == 5.0
    @test D[3] == 5.0
end

true
