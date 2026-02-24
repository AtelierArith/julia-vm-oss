using Test

@testset "Rational mixed-type promotion" begin
    # Rational{Int32} + Rational{Int64} => Rational{Int64}
    # (Int32 fields get widened to Int64 by intrinsics, then Rational outer constructor produces Rational{Int64})
    a = Int32(1) // Int32(2)
    b = 1 // 3  # Int64
    r = a + b
    @test typeof(r) == Rational{Int64}
    @test numerator(r) == 5
    @test denominator(r) == 6

    # Int64 + Rational{Int64} => Rational{Int64} (existing functionality)
    r2 = 1 + (1 // 2)
    @test typeof(r2) == Rational{Int64}
    @test numerator(r2) == 3
    @test denominator(r2) == 2

    # Rational{Int32} constructed and accessed correctly
    r3 = Int32(7) // Int32(3)
    @test typeof(r3) == Rational{Int32}
    @test numerator(r3) == 7
    @test denominator(r3) == 3
end

true
