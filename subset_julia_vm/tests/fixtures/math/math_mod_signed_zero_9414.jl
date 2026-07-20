# Float mod signed-zero parity (Issue #9414)
# A zero remainder must carry the sign of the divisor, mirroring upstream
# `mod(x::T, y::T) where {T<:AbstractFloat}` (base/float.jl):
# `r == 0 && return copysign(r, y)`. Compare with string()/signbit()
# because -0.0 == 0.0 under ==.
#
# Note: mixed Int×Float mod results are bound to variables before signbit()
# to dodge the unrelated inference fallback that coerces nested
# div/rem/mod results to Int64 (Issue #9528). Float16/Float32 results are
# checked via string() only because signbit(Float16(-0.0)) is itself
# broken (Issue #9529).

using Test

@testset "mod zero remainder carries divisor sign (Issue #9414)" begin
    # Negative divisor -> -0.0
    @test string(mod(5.0, -2.5)) == "-0.0"
    @test signbit(mod(5.0, -2.5))
    @test string(mod(Float16(5.0), Float16(-2.5))) == "-0.0"
    @test string(mod(Float32(5.0), Float32(-2.5))) == "-0.0"

    # Mixed Int x Float (variable-bound before signbit, Issue #9528)
    @test string(mod(Int64(5), -2.5)) == "-0.0"
    mif = mod(Int64(5), -2.5)
    @test signbit(mif)
    @test string(mod(UInt128(5), Float16(-2.5))) == "-0.0"

    # Zero dividend follows the divisor sign too
    @test string(mod(0.0, -3.0)) == "-0.0"
    @test string(mod(-0.0, -3.0)) == "-0.0"

    # Positive divisor -> +0.0 (no spurious negative zero)
    @test string(mod(-5.0, 2.5)) == "0.0"
    @test !signbit(mod(-5.0, 2.5))
    @test string(mod(7.5, 2.5)) == "0.0"
    @test string(mod(-0.0, 3.0)) == "0.0"

    # Nonzero remainders are unchanged
    @test mod(5.5, -2.5) == -2.0
    @test mod(-5.5, 2.5) == 2.0
    @test mod(5.5, 2.5) == 0.5

    # NaN propagation is unchanged
    @test isnan(mod(NaN, 2.0))
    @test isnan(mod(2.0, 0.0))
end

true
