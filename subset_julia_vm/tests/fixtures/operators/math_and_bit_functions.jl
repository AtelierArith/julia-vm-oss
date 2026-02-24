# Test new math and bit functions
# - Bit operations: leading_ones, trailing_zeros, bitrotate, bswap
# - Math: fma, muladd
# - Float decomposition: exponent, significand, frexp
# - Sign predicates: isnegative, ispositive
# - Float inspection: issubnormal, maxintfloat

using Test

@testset "Math and bit functions: leading_ones, trailing_zeros, bitrotate, bswap, fma, muladd, exponent, significand, frexp, isnegative, ispositive, issubnormal, maxintfloat" begin

    # === Bit operations ===

    # leading_ones - count leading 1 bits
    r1 = leading_ones(-1) == 64   # -1 = all 1s, 64 leading ones
    r2 = leading_ones(0) == 0     # 0 has no leading ones

    # trailing_zeros - count trailing 0 bits
    r3 = trailing_zeros(8) == 3   # 8 = 0b1000, 3 trailing zeros
    r4 = trailing_zeros(1) == 0   # 1 = 0b1, 0 trailing zeros
    r5 = trailing_zeros(-1) == 0  # -1 = all 1s, 0 trailing zeros

    # bitrotate - rotate bits
    r6 = bitrotate(1, 1) == 2     # rotate 1 left by 1 = 2
    r7 = bitrotate(2, -1) == 1    # rotate 2 right by 1 = 1

    # bswap - byte swap (reverse byte order)
    # bswap(0x0102030405060708) = 0x0807060504030201
    # In decimal: 72623859790382856 -> 578437695752307201
    r8 = bswap(72623859790382856) == 578437695752307201

    # === Fused multiply-add ===
    r9 = fma(2.0, 3.0, 4.0) == 10.0   # 2*3 + 4 = 10
    r10 = muladd(2.0, 3.0, 4.0) == 10.0

    # === Float decomposition ===

    # exponent - get exponent part
    r11 = exponent(8.0) == 3      # 8 = 2^3
    r12 = exponent(1.0) == 0      # 1 = 2^0
    r13 = exponent(0.5) == -1     # 0.5 = 2^(-1)

    # significand - get significand in [1, 2)
    r14 = significand(8.0) == 1.0   # 8 = 1.0 * 2^3
    r15 = significand(12.0) == 1.5  # 12 = 1.5 * 2^3

    # frexp - returns (significand, exponent) tuple
    # frexp(8.0) = (0.5, 4) because 8 = 0.5 * 2^4
    fr = frexp(8.0)
    r16 = fr[1] == 0.5
    r17 = fr[2] == 4

    # === Sign predicates ===
    r18 = isnegative(-5)
    r19 = !isnegative(5)
    r20 = !isnegative(0)
    r21 = ispositive(5)
    r22 = !ispositive(-5)
    r23 = !ispositive(0)

    # === Float inspection ===

    # issubnormal - check if subnormal number
    # 5.0e-324 is the smallest positive subnormal
    r24 = issubnormal(5.0e-324)
    r25 = !issubnormal(1.0)

    # maxintfloat - largest integer exactly representable as Float64
    r26 = maxintfloat() == 9007199254740992.0  # 2^53

    # Sum all results: 26 tests
    @test (Float64(r1) + Float64(r2) + Float64(r3) + Float64(r4) + Float64(r5) + Float64(r6) + Float64(r7) + Float64(r8) + Float64(r9) + Float64(r10) + Float64(r11) + Float64(r12) + Float64(r13) + Float64(r14) + Float64(r15) + Float64(r16) + Float64(r17) + Float64(r18) + Float64(r19) + Float64(r20) + Float64(r21) + Float64(r22) + Float64(r23) + Float64(r24) + Float64(r25) + Float64(r26)) == 26.0
end

true  # Test passed
