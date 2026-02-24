using Test

@testset "Mixed narrow-integer comparison with Rational{T} (Issue #2478)" begin
    # Int32 < Rational{Int32}
    @test Int32(1) < (Int32(3) // Int32(2))

    # Int32 > Rational{Int32}
    @test Int32(2) > (Int32(3) // Int32(2))

    # Int32 <= Rational{Int32}
    @test Int32(1) <= (Int32(3) // Int32(2))
    @test Int32(1) <= (Int32(2) // Int32(2))

    # Int32 >= Rational{Int32}
    @test Int32(2) >= (Int32(3) // Int32(2))
    @test Int32(1) >= (Int32(2) // Int32(2))

    # Int32 == Rational{Int32}
    @test Int32(2) == (Int32(4) // Int32(2))

    # Rational{Int32} < Int32
    @test (Int32(1) // Int32(2)) < Int32(1)

    # Rational{Int32} > Int32
    @test (Int32(3) // Int32(2)) > Int32(1)

    # Rational{Int32} == Int32
    @test (Int32(4) // Int32(2)) == Int32(2)

    # Int16 comparisons
    @test Int16(1) < (Int16(3) // Int16(2))
    @test (Int16(3) // Int16(2)) > Int16(1)
end

true
