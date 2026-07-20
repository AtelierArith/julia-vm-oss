using Test

# Issue #9290: floatmax(::Type{BigFloat}) / floatmin(::Type{BigFloat}) threw
# MethodError because only the Float16/32/64 methods existed. They are now
# defined in base/gmp.jl mirroring upstream julia/base/mpfr.jl:
#   floatmin(::Type{BigFloat}) = nextfloat(zero(BigFloat))
#   floatmax(::Type{BigFloat}) = prevfloat(BigFloat(Inf))
#
# NOTE on expected values: sjulia's astro-float backend has an i32-class
# exponent range (~10^±6.46e8) vs MPFR's emax = 2^62 (~10^±1.39e18), so the
# decimal VALUES of floatmax/floatmin(BigFloat) intentionally differ from
# upstream Julia (documented backend divergence, Issue #9290, same family as
# Issue #8885 — see docs/vm/NUMERIC_TYPES.md). This fixture therefore asserts
# the upstream INVARIANTS, all verified to hold in julia 1.12 as well —
# except the floatmin-precision note below.

@testset "floatmax/floatmin for BigFloat (Issue #9290)" begin
    fm = floatmax(BigFloat)
    fn = floatmin(BigFloat)

    # ---- the MethodError facet: methods exist and return BigFloat ----
    @test fm isa BigFloat
    @test fn isa BigFloat

    # ---- upstream definitional identities (base/mpfr.jl) ----
    @test fm == prevfloat(BigFloat(Inf))
    @test fm == prevfloat(big(Inf))
    @test fn == nextfloat(zero(BigFloat))
    @test fn == nextfloat(big(0.0))

    # ---- largest/smallest positive finite: boundary stepping ----
    @test isfinite(fm)
    @test isfinite(fn)
    @test nextfloat(fm) == BigFloat(Inf)
    @test iszero(prevfloat(fn))
    @test nextfloat(BigFloat(-Inf)) == -fm

    # ---- ordering: strictly beyond the Float64 limits ----
    @test fm > 0
    @test fn > 0
    @test fm > floatmax(Float64)
    @test fn < floatmin(Float64)
    @test fn < fm

    # ---- precision dependence ----
    # floatmax gains mantissa bits with precision, so it strictly grows.
    fm128 = setprecision(() -> floatmax(BigFloat), BigFloat, 128)
    @test isfinite(fm128)
    @test fm128 < fm
    # floatmin: MPFR's 2^(emin-1) is precision-independent (equality
    # upstream); astro-float's minimum boundary varies with precision
    # (strict > in sjulia). `>=` holds in both backends.
    fn128 = setprecision(() -> floatmin(BigFloat), BigFloat, 128)
    @test fn128 >= fn
    @test fn128 > 0
end

true
