# Issue #6742 (#6726-1): floor / ceil / round / trunc are pure Julia
# (base/floatfuncs.jl) over the CPU intrinsics floor_llvm / ceil_llvm /
# trunc_llvm / rint_llvm (base/boot.jl). round uses round-to-nearest-ties-to-even
# (the default RoundNearest). The digits/sigdigits/base keyword forms and the
# RoundingMode forms (base/rounding.jl) are pure Julia too. Verified vs julia 1.12.

using Test

@testset "plain rounding is round-half-to-even (Issue #6742)" begin
    @test round(2.5) == 2.0
    @test round(3.5) == 4.0
    @test round(0.5) == 0.0
    @test round(-2.5) == -2.0
    @test round(2.5) === 2.0
    @test floor(3.7) == 3.0 && ceil(3.2) == 4.0 && trunc(-3.7) == -3.0
    @test round(3.0f0) === 3.0f0   # Float32 is preserved
end

@testset "integer identity & typed forms (Issue #6742)" begin
    @test floor(5) === 5           # not 5.0
    @test round(7) === 7
    @test floor(Int, 3.7) === 3
    @test ceil(Int, 3.2) === 4
    @test round(Int, 2.5) === 2
    @test trunc(Int, -3.7) === -3
    @test round(Int8, 3.5) === Int8(4)   # the requested integer type is preserved
end

@testset "digits / sigdigits / base keywords (Issue #6742)" begin
    @test round(3.14159, digits=2) == 3.14
    @test round(123.456, sigdigits=2) == 120.0
    @test floor(3.14159, digits=2) == 3.14
    @test ceil(3.14159, digits=2) == 3.15
    @test trunc(3.14159, digits=2) == 3.14
    @test round(3.7, digits=2, base=2) == 3.75   # base is honored (pure Julia)
end

@testset "RoundingMode variants (Issue #6742)" begin
    @test round(2.5, RoundNearestTiesUp) == 3.0
    @test round(-2.5, RoundNearestTiesUp) == -2.0
    @test round(2.5, RoundNearestTiesAway) == 3.0
    @test round(-2.5, RoundNearestTiesAway) == -3.0
    @test round(2.3, RoundFromZero) == 3.0
    @test round(2.5, RoundUp) == 3.0
    @test round(2.5, RoundDown) == 2.0
    @test round(2.5, RoundToZero) == 2.0
end

true
