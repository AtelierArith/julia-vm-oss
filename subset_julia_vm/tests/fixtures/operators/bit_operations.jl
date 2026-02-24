# Test bit operation functions
# count_ones, count_zeros, leading_zeros, trailing_ones, bitreverse

using Test

@testset "Bit operations: count_ones, count_zeros, leading_zeros, trailing_ones, bitreverse" begin

    # count_ones - popcount (number of 1 bits)
    r1 = count_ones(85) == 4      # 85 = 0b1010101 has 4 ones
    r2 = count_ones(255) == 8     # 255 = 0xFF has 8 ones
    r3 = count_ones(0) == 0       # 0 has no ones

    # count_zeros - number of 0 bits
    r4 = count_zeros(0) == 64     # 0 has all 64 bits as zeros
    r5 = count_zeros(-1) == 0     # -1 (all bits 1) has no zeros

    # leading_zeros - leading zero bits
    r6 = leading_zeros(1) == 63   # 1 has 63 leading zeros
    r7 = leading_zeros(256) == 55 # 256 = 2^8 has 55 leading zeros

    # trailing_ones - trailing 1 bits
    r8 = trailing_ones(7) == 3    # 7 = 0b111 has 3 trailing ones
    r9 = trailing_ones(8) == 0    # 8 = 0b1000 has 0 trailing ones
    r10 = trailing_ones(-1) == 64 # -1 (all bits 1) has 64 trailing ones

    # bitreverse - reverse all bits
    # bitreverse(1) should set the highest bit (result is negative in signed)
    r11 = bitreverse(1) < 0       # reversed 1 has highest bit set

    # All tests pass: 11 true values summed = 11.0
    @test (Float64(r1) + Float64(r2) + Float64(r3) + Float64(r4) + Float64(r5) + Float64(r6) + Float64(r7) + Float64(r8) + Float64(r9) + Float64(r10) + Float64(r11)) == 11.0
end

true  # Test passed
