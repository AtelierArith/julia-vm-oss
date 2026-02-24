# Test tanpi function: tan(π*x)
# More accurate than tan(pi*x) for some cases

using Test

@testset "tanpi function: tan(π*x) with special cases for integers and half-integers" begin

    # === Basic cases ===
    r1 = abs(tanpi(0.0) - 0.0) < 1e-10   # tan(π*0) = 0
    r2 = abs(tanpi(0.25) - 1.0) < 1e-10   # tan(π*0.25) = tan(π/4) = 1
    r3 = isinf(tanpi(0.5))   # tan(π*0.5) = tan(π/2) = Inf

    # === Integer cases ===
    # For integers, tan(π*x) = 0
    r4 = abs(tanpi(0) - 0.0) < 1e-10
    r5 = abs(tanpi(1) - 0.0) < 1e-10
    r6 = abs(tanpi(-1) - 0.0) < 1e-10
    r7 = abs(tanpi(2) - 0.0) < 1e-10

    # === Half-integer cases ===
    # For half-integers (x = n + 0.5), tan(π*x) = ±Inf
    # Note: isinf() check might be needed
    r8 = isinf(tanpi(0.5))   # tan(π*0.5) = Inf
    r9 = isinf(tanpi(1.5))   # tan(π*1.5) = -Inf
    r10 = isinf(tanpi(-0.5))   # tan(π*(-0.5)) = -Inf

    # === Other cases ===
    r11 = abs(tanpi(1.0/6.0) - tan(pi/6)) < 1e-10   # tan(π/6) = 1/√3 ≈ 0.577
    r12 = abs(tanpi(1.0/3.0) - tan(pi/3)) < 1e-10   # tan(π/3) = √3 ≈ 1.732

    # Sum all results: 12 tests
    @test (Float64(r1) + Float64(r2) + Float64(r3) + Float64(r4) + Float64(r5) + Float64(r6) + Float64(r7) + Float64(r8) + Float64(r9) + Float64(r10) + Float64(r11) + Float64(r12)) == 12.0
end

true  # Test passed
