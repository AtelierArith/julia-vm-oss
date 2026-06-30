using Test

# Issue #5691: div(x, y, r::RoundingMode) rounds the quotient per `r`, and
# rem(x, y, r) = x - y*div(x, y, r). RoundUp==cld, RoundDown==fld, RoundToZero==div,
# RoundFromZero rounds away from zero, RoundNearest rounds half-to-even.

@testset "div with a RoundingMode (Issue #5691)" begin
    @test div(7, 2, RoundUp) == 4
    @test div(7, 2, RoundDown) == 3
    @test div(7, 2, RoundToZero) == 3
    @test div(7, 2, RoundNearest) == 4
    @test div(-7, 2, RoundUp) == -3
    @test div(-7, 2, RoundDown) == -4
    @test div(-7, 2, RoundNearest) == -4
    @test div(7, 2, RoundFromZero) == 4
    @test div(-7, 2, RoundFromZero) == -4

    # Ties round to even.
    @test div(5, 2, RoundNearest) == 2
    @test div(9, 2, RoundNearest) == 4
    @test div(11, 2, RoundNearest) == 6
    @test div(-5, 2, RoundNearest) == -2
end

@testset "rem with a RoundingMode (Issue #5691)" begin
    @test rem(7, 2, RoundUp) == -1
    @test rem(7, 2, RoundDown) == 1
    @test rem(7, 2, RoundNearest) == -1
    @test rem(10, 3, RoundNearest) == 1
    @test rem(-7, 2, RoundDown) == 1
end

@testset "two-argument div/rem unchanged (Issue #5691)" begin
    @test div(7, 2) == 3
    @test rem(7, 2) == 1
    @test cld(7, 2) == 4
    @test fld(7, 2) == 3
end

true
