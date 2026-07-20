# Issue #9514: rem/mod with an infinite Rational divisor (1//0) returned the
# invalid rational 0//0 instead of raising ArgumentError. The Rational x Rational
# rem/mod path `x - div(x, y) * y` constructs 0//0 when the divisor is infinite;
# upstream's Rational{T}(num, den) constructor rejects num == 0 && den == 0 with
# ArgumentError ("invalid rational: zero(T)//zero(T)") in julia/base/rational.jl.
# sjulia's constructors now mirror that check while still preserving the 1//0 /
# -1//0 Inf sentinels.
#
# All expected values/behaviors verified against upstream julia 1.12.

using Test

@testset "rem/mod with infinite Rational divisor throws ArgumentError (Issue #9514)" begin
    @test_throws ArgumentError rem(1 // 2, 1 // 0)
    @test_throws ArgumentError mod(1 // 2, 1 // 0)
    @test_throws ArgumentError rem(3 // 4, -1 // 0)
    @test_throws ArgumentError mod(3 // 4, 1 // 0)
end

@testset "0//0 construction is rejected, Inf sentinels preserved (Issue #9514)" begin
    # Directly constructing 0//0 is invalid for every element type.
    @test_throws ArgumentError 0 // 0
    @test_throws ArgumentError Rational{Int8}(0, 0)
    @test_throws ArgumentError Rational{BigInt}(big(0), big(0))

    # num != 0 && den == 0 stays a valid Inf/-Inf sentinel.
    @test (1 // 0).num == 1
    @test (1 // 0).den == 0
    @test (-1 // 0).num == -1
    @test (-1 // 0).den == 0
end

@testset "legitimate Rationals still normalize (Issue #9514)" begin
    @test 6 // 4 == 3 // 2
    @test (6 // 4).den == 2
    @test 3 // 4 + 1 // 4 == 1 // 1
    @test rem(1 // 2, 1 // 3) == 1 // 6
    @test mod(7 // 3, 2 // 3) == 1 // 3
end

true
