using Test

# Issue #6794: fractional BigFloat exponents (big(2.0)^0.5) previously hung
# (astro_float's exp/ln/pow Ziv refinement loops never terminate on
# table-maker's-dilemma inputs such as big(4.0)^0.5 = exactly 2.0). The vendored
# astro-float-num is patched to bound those loops, so fractional powers now
# terminate. Exact (representable) results match upstream bit-for-bit; for
# irrational results the value is correct to ~1 ULP (astro_float rounds the last
# bit independently of upstream's MPFR), so those are checked with isapprox /
# round-trip rather than exact ==.

@testset "BigFloat fractional power exact results (Issue #6794)" begin
    @test big(4.0)^0.5 == 2          # was an infinite hang
    @test big(9.0)^0.5 == 3
    @test big(16.0)^0.5 == 4
    @test big(25.0)^0.5 == 5
    @test big(100.0)^0.5 == 10
    @test big(4.0)^(-0.5) == 0.5
    @test big(2.0)^2.0 == 4          # integer-valued float exponent
    @test typeof(big(4.0)^0.5) === BigFloat
end

@testset "BigFloat fractional power irrational results (Issue #6794)" begin
    @test isapprox(big(2.0)^0.5, 1.4142135623730951)
    @test isapprox(big(2.0)^0.25, 1.189207115002721)
    @test isapprox(big(2.0)^1.5, 2.8284271247461903)
    @test isapprox(big(3.0)^0.5, 1.7320508075688772)
    @test (big(2.0)^0.5)^2 ≈ 2
    @test typeof(big(2.0)^0.5) === BigFloat
end

@testset "BigFloat fractional power via BigFloat exponent (Issue #6794)" begin
    @test big(4.0)^big(0.5) == 2
    @test isapprox(big(2.0)^big(0.5), 1.4142135623730951)
    @test typeof(big(2.0)^big(0.5)) === BigFloat
end

true
