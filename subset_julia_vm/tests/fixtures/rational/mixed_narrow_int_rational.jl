using Test

@testset "Mixed narrow-integer + Rational{T} operations (Issue #2475)" begin
    # Int32 + Rational{Int32}
    r1 = Int32(1) + (Int32(1) // Int32(2))
    @test numerator(r1) == 3
    @test denominator(r1) == 2

    # Rational{Int32} + Int32
    r2 = (Int32(1) // Int32(2)) + Int32(1)
    @test numerator(r2) == 3
    @test denominator(r2) == 2

    # Int32 - Rational{Int32}
    r3 = Int32(2) - (Int32(3) // Int32(4))
    @test numerator(r3) == 5
    @test denominator(r3) == 4

    # Rational{Int32} - Int32
    r4 = (Int32(7) // Int32(4)) - Int32(1)
    @test numerator(r4) == 3
    @test denominator(r4) == 4

    # Int32 * Rational{Int32}
    r5 = Int32(3) * (Int32(2) // Int32(5))
    @test numerator(r5) == 6
    @test denominator(r5) == 5

    # Rational{Int32} * Int32
    r6 = (Int32(2) // Int32(5)) * Int32(3)
    @test numerator(r6) == 6
    @test denominator(r6) == 5

    # Rational{Int32} / Int32
    r7 = (Int32(3) // Int32(4)) / Int32(2)
    @test numerator(r7) == 3
    @test denominator(r7) == 8

    # Int16 + Rational{Int16}
    r8 = Int16(1) + (Int16(1) // Int16(3))
    @test numerator(r8) == 4
    @test denominator(r8) == 3

    # Int16 * Rational{Int16}
    r9 = Int16(2) * (Int16(1) // Int16(5))
    @test numerator(r9) == 2
    @test denominator(r9) == 5
end

true
