# Test type conversion functions: signed, unsigned, float, widemul

using Test

@testset "Type conversion functions: signed, unsigned, float, widemul" begin

    # === signed(x) - convert to signed integer ===

    # signed on Int64 (identity)
    r1 = signed(42) == 42
    r2 = signed(-42) == -42
    r3 = signed(0) == 0

    # signed on Float64
    r4 = signed(3.14) == 3
    r5 = signed(-2.9) == -2
    r6 = signed(0.0) == 0

    # signed on Bool
    r7 = signed(true) == 1
    r8 = signed(false) == 0

    # === unsigned(x) - convert to unsigned integer ===

    # unsigned on positive Int64
    r9 = unsigned(42) == 42
    r10 = unsigned(0) == 0

    # unsigned on Float64
    r11 = unsigned(3.14) == 3
    r12 = unsigned(0.0) == 0

    # unsigned on Bool
    r13 = unsigned(true) == 1
    r14 = unsigned(false) == 0

    # === float(x) - convert to Float64 ===

    # float on Int64
    r15 = float(42) == 42.0
    r16 = float(-42) == -42.0
    r17 = float(0) == 0.0

    # float on Float64 (identity)
    r18 = float(3.14) == 3.14
    r19 = float(-2.5) == -2.5
    r20 = float(0.0) == 0.0

    # float on Bool
    r21 = float(true) == 1.0
    r22 = float(false) == 0.0

    # Type check
    r23 = typeof(float(42)) == Float64
    r24 = typeof(float(true)) == Float64

    # === widemul(a, b) - wide multiplication (no overflow) ===

    # Basic multiplication
    r25 = widemul(2, 3) == 6
    r26 = widemul(10, 10) == 100
    r27 = widemul(-5, 4) == -20
    r28 = widemul(-3, -7) == 21

    # Multiplication with zero
    r29 = widemul(0, 1000000) == 0
    r30 = widemul(1000000, 0) == 0

    # Large numbers (would overflow with regular Int64 multiplication)
    # 2^30 * 2^30 = 2^60, which fits in Int64
    r31 = widemul(1073741824, 1073741824) == 1152921504606846976

    # Float multiplication
    r32 = widemul(2.5, 4.0) == 10.0
    r33 = widemul(-1.5, 3.0) == -4.5

    # Mixed types
    r34 = widemul(2, 3.5) == 7.0
    r35 = widemul(4.0, 3) == 12.0

    # Count all passing tests
    count = 0
    if r1; count = count + 1; end
    if r2; count = count + 1; end
    if r3; count = count + 1; end
    if r4; count = count + 1; end
    if r5; count = count + 1; end
    if r6; count = count + 1; end
    if r7; count = count + 1; end
    if r8; count = count + 1; end
    if r9; count = count + 1; end
    if r10; count = count + 1; end
    if r11; count = count + 1; end
    if r12; count = count + 1; end
    if r13; count = count + 1; end
    if r14; count = count + 1; end
    if r15; count = count + 1; end
    if r16; count = count + 1; end
    if r17; count = count + 1; end
    if r18; count = count + 1; end
    if r19; count = count + 1; end
    if r20; count = count + 1; end
    if r21; count = count + 1; end
    if r22; count = count + 1; end
    if r23; count = count + 1; end
    if r24; count = count + 1; end
    if r25; count = count + 1; end
    if r26; count = count + 1; end
    if r27; count = count + 1; end
    if r28; count = count + 1; end
    if r29; count = count + 1; end
    if r30; count = count + 1; end
    if r31; count = count + 1; end
    if r32; count = count + 1; end
    if r33; count = count + 1; end
    if r34; count = count + 1; end
    if r35; count = count + 1; end

    @test (count) == 35.0
end

true  # Test passed
