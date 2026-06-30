# Test inv(::BigInt) returns a float, not integer division
# Issue #7309: `inv(big(2))` returned 0 (integer division 1 ÷ 2) instead of 0.5.
#   Upstream: inv(x::Integer) = float(one(x)) / float(x); for a BigInt the
#   result is a BigFloat (`inv(big(2))` == 0.5 :: BigFloat).
# Only exact dyadic values are checked for bit-parity; astro-float vs MPFR may
# differ in the last ULP for non-dyadic results (e.g. inv(big(3))).

using Test

@testset "inv on integers (Issue #7309)" begin
    @testset "inv(::BigInt) is a BigFloat" begin
        @test inv(big(2)) == 0.5
        @test typeof(inv(big(2))) === BigFloat
        @test inv(big(4)) == 0.25
        @test inv(big(8)) == 0.125
    end

    @testset "inv(::Int) / inv(::Float64) unchanged" begin
        @test inv(2) === 0.5
        @test typeof(inv(2)) === Float64
        @test inv(2.0) === 0.5
        @test inv(4) === 0.25
    end
end

true
