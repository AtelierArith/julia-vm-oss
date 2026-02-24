using Test

@testset "Mixed narrow-integer division with Rational{T} (Issue #2478)" begin
    # Int32 / Rational{Int32} => Rational
    r1 = Int32(2) / (Int32(3) // Int32(4))
    @test numerator(r1) == 8
    @test denominator(r1) == 3

    # Rational{Int32} / Int32 (already tested in PR #2476 but verify)
    r2 = (Int32(3) // Int32(4)) / Int32(2)
    @test numerator(r2) == 3
    @test denominator(r2) == 8

    # Int16 / Rational{Int16}
    r3 = Int16(3) / (Int16(1) // Int16(2))
    @test numerator(r3) == 6
    @test denominator(r3) == 1
end

true
