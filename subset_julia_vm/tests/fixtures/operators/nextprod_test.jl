# Test nextprod function: Next integer >= n that is product of factors

using Test

@testset "nextprod function: Next integer >= n that is product of factors" begin

    # === Basic cases ===
    r1 = nextprod((2,), 7) == 8   # nextprod((2,), 7) = 8 = 2^3
    r2 = nextprod((2,), 8) == 8   # nextprod((2,), 8) = 8 = 2^3
    r3 = nextprod((2,), 9) == 16   # nextprod((2,), 9) = 16 = 2^4

    # === Two factors ===
    r4 = nextprod((2, 3), 7) == 8   # 8 = 2^3
    r5 = nextprod((2, 3), 10) == 12   # 12 = 2^2 * 3^1
    r6 = nextprod((2, 3), 105) == 108   # 108 = 2^2 * 3^3 (Julia example)

    # === Three factors (most common: (2, 3, 5)) ===
    r7 = nextprod((2, 3, 5), 20) == 20   # 20 = 2^2 * 5^1
    r8 = nextprod((2, 3, 5), 30) == 30   # 30 = 2^1 * 3^1 * 5^1
    r9 = nextprod((2, 3, 5), 31) == 32   # 32 = 2^5

    # === Edge cases ===
    r10 = nextprod((2, 3), 1) == 1   # n = 1 returns 1
    r11 = nextprod((2, 3), 2) == 2   # n = 2 returns 2
    r12 = nextprod((2, 3), 3) == 3   # n = 3 returns 3 (3 = 2^0 * 3^1)

    # Sum all results: 12 tests
    @test (Float64(r1) + Float64(r2) + Float64(r3) + Float64(r4) + Float64(r5) + Float64(r6) + Float64(r7) + Float64(r8) + Float64(r9) + Float64(r10) + Float64(r11) + Float64(r12)) == 12.0
end

true  # Test passed
