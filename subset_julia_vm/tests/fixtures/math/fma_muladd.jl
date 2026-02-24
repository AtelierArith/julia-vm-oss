# Test fma and muladd - fused multiply-add functions
# fma(x, y, z) and muladd(x, y, z) compute x*y+z

using Test

@testset "fma and muladd (Issue #489)" begin
    # fma: fused multiply-add
    # fma(x, y, z) = x * y + z
    @test (fma(2, 3, 4) == 10)     # 2*3+4 = 10
    @test (fma(1.5, 2.0, 0.5) == 3.5)  # 1.5*2.0+0.5 = 3.5
    @test (fma(-1, 2, 3) == 1)    # -1*2+3 = 1
    @test (fma(0, 10, 5) == 5)    # 0*10+5 = 5

    # muladd: multiply-add (may be fused depending on hardware)
    # muladd(x, y, z) = x * y + z
    @test (muladd(2, 3, 4) == 10)     # 2*3+4 = 10
    @test (muladd(1.5, 2.0, 0.5) == 3.5)  # 1.5*2.0+0.5 = 3.5
    @test (muladd(-1, 2, 3) == 1)    # -1*2+3 = 1
    @test (muladd(0, 10, 5) == 5)    # 0*10+5 = 5

    # Both functions should give the same result for simple cases
    @test (fma(3, 4, 5) == muladd(3, 4, 5))
end

true  # Test passed
