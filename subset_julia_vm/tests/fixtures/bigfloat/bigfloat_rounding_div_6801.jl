using Test

# Issue #6801: floor / ceil / round / trunc on BigFloat were unsupported
# ("Type error: expected numeric value, got BigFloat(...)"), so div / fld / cld
# and divrem / fldmod (which build on floor/ceil) failed transitively. The
# rounding paths now route BigFloat through astro_float's native
# floor/ceil/int/round (apply_unary_rounding_op_with_heap) so arbitrary
# precision is preserved. Verified vs julia 1.12.6.

@testset "BigFloat floor (Issue #6801)" begin
    @test floor(big(2.7)) == 2
    @test floor(big(-2.1)) == -3
    @test floor(big(2.0)) == 2
    @test typeof(floor(big(2.7))) === BigFloat
end

@testset "BigFloat ceil (Issue #6801)" begin
    @test ceil(big(2.3)) == 3
    @test ceil(big(-2.1)) == -2
    @test ceil(big(2.0)) == 2
    @test typeof(ceil(big(2.3))) === BigFloat
end

@testset "BigFloat trunc, toward zero (Issue #6801)" begin
    @test trunc(big(2.9)) == 2
    @test trunc(big(-2.9)) == -2
    @test typeof(trunc(big(2.9))) === BigFloat
end

@testset "BigFloat round, ties to even (Issue #6801)" begin
    @test round(big(2.5)) == 2   # ties to even
    @test round(big(3.5)) == 4   # ties to even
    @test round(big(0.5)) == 0
    @test round(big(-2.5)) == -2
    @test round(big(2.4)) == 2
    @test typeof(round(big(2.5))) === BigFloat
end

@testset "BigFloat div / fld / cld (Issue #6801)" begin
    @test div(big(7.0), big(3.0)) == 2
    @test fld(big(7.0), big(3.0)) == 2
    @test cld(big(7.0), big(3.0)) == 3
    # fld / cld genuinely round toward -Inf / +Inf, so negatives are exact.
    @test fld(big(-7.0), big(3.0)) == -3
    @test cld(big(-7.0), big(3.0)) == -2
    @test typeof(div(big(7.0), big(3.0))) === BigFloat
end

@testset "BigFloat divrem / fldmod (Issue #6801)" begin
    # Element-wise checks: tuple `==` over mixed BigFloat/Float64 elements is a
    # separate pre-existing gap, so compare the components directly.
    dr = divrem(big(7.0), big(3.0))
    @test dr[1] == 2 && dr[2] == 1
    fm = fldmod(big(7.0), big(3.0))
    @test fm[1] == 2 && fm[2] == 1
    fmn = fldmod(big(-7.0), big(3.0))
    @test fmn[1] == -3 && fmn[2] == 2
end

@testset "BigFloat rounding through user functions (Issue #6801)" begin
    f(x) = floor(x)
    g(x) = ceil(x)
    h(x) = round(x)
    k(x) = trunc(x)
    @test f(big(2.7)) == 2
    @test g(big(2.3)) == 3
    @test h(big(2.5)) == 2
    @test k(big(-2.9)) == -2
end

@testset "BigFloat rounding helpers (Issue #6801)" begin
    @test isinteger(big(2.0))
    @test !isinteger(big(2.5))
    mf = modf(big(2.75))
    @test mf[1] == 0.75 && mf[2] == 2.0
end

@testset "BigFloat rounding keeps precision beyond Float64 (Issue #6801)" begin
    # 24 significant digits — well past Float64's ~15-16, so a round-trip
    # through f64 would corrupt the integer part.
    x = big"123456789012345678901234.75"
    @test floor(x) == big"123456789012345678901234.0"
    @test ceil(x) == big"123456789012345678901235.0"
    @test trunc(x) == big"123456789012345678901234.0"
end

true
