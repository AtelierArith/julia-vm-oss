using Test

@testset "Rational div/fld/cld/rem/mod (Issue #2486)" begin
    # div (truncated division) - Rational // Rational
    @test div(7//3, 2//3) == 3
    @test div(7//2, 3//2) == 2
    @test div(-7//3, 2//3) == -3
    @test div(7//3, -2//3) == -3

    # fld (floored division) - Rational // Rational
    @test fld(7//3, 2//3) == 3
    @test fld(7//2, 3//2) == 2
    @test fld(-7//3, 2//3) == -4
    @test fld(7//3, -2//3) == -4

    # cld (ceiled division) - Rational // Rational
    @test cld(7//3, 2//3) == 4
    @test cld(7//2, 3//2) == 3
    @test cld(-7//3, 2//3) == -3
    @test cld(7//3, -2//3) == -3

    # rem (truncated remainder) - Rational // Rational
    @test rem(7//3, 2//3) == 1//3
    @test rem(7//2, 3//2) == 1//2
    @test rem(-7//3, 2//3) == -1//3

    # mod (floored remainder) - Rational // Rational
    @test mod(7//3, 2//3) == 1//3
    @test mod(7//2, 3//2) == 1//2
    @test mod(-7//3, 2//3) == 1//3

    # Mixed: Rational / Integer
    @test div(7//3, 2) == 1
    @test fld(7//3, 2) == 1
    @test cld(7//3, 2) == 2
    @test rem(7//3, 2) == 1//3
    @test mod(7//3, 2) == 1//3

    # Mixed: Integer / Rational
    @test div(5, 3//2) == 3
    @test fld(5, 3//2) == 3
    @test cld(5, 3//2) == 4
    @test rem(5, 3//2) == 1//2
    @test mod(5, 3//2) == 1//2
end

true
