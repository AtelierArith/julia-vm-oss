# Issue #6740: exponent / significand / frexp / issubnormal / nextfloat /
# prevfloat are now pure Julia (base/float.jl) over `reinterpret` + per-type
# IEEE bit-field helpers, replacing the Float64-only Rust builtins. They now
# match upstream julia 1.12 across Float64/Float32/Float16 (the old builtins
# collapsed Float32/Float16 to Float64), preserve element type, and handle the
# IEEE edge cases (Inf/NaN/±0/subnormal). The only Rust boundary is `reinterpret`.

using Test

@testset "exponent (Issue #6740)" begin
    @test exponent(8.0) === 3
    @test exponent(0.5) === -1
    @test exponent(1.0) === 0
    @test exponent(Float32(8.0)) === 3
    @test exponent(Float16(8.0)) === 3
    @test exponent(1.0e-310) === -1030     # subnormal Float64
    @test exponent(8) === 3                # Integer
    @test_throws DomainError exponent(0.0)
    @test_throws DomainError exponent(Inf)
    @test_throws DomainError exponent(NaN)
end

@testset "significand preserves type (Issue #6740)" begin
    @test significand(12.0) === 1.5
    @test significand(8.0) === 1.0
    @test significand(Float32(12.0)) === 1.5f0
    @test significand(Float16(12.0)) === Float16(1.5)
    @test significand(-12.0) === -1.5
    @test significand(Inf) === Inf
    @test significand(0.0) === 0.0
end

@testset "frexp (Issue #6740)" begin
    @test frexp(8.0) === (0.5, 4)
    @test frexp(0.625) === (0.625, 0)
    @test frexp(0.0) === (0.0, 0)
    @test frexp(Float32(8.0)) === (0.5f0, 4)
    @test frexp(Inf) === (Inf, 0)
end

@testset "issubnormal (Issue #6740)" begin
    @test issubnormal(1.0e-310) === true
    @test issubnormal(1.0) === false
    @test issubnormal(0.0) === false
    @test issubnormal(Float32(1.0f-40)) === true
    @test issubnormal(Float32(1.0)) === false
end

@testset "nextfloat / prevfloat preserve type (Issue #6740)" begin
    @test nextfloat(1.0) === 1.0000000000000002
    @test prevfloat(1.0) === 0.9999999999999999
    @test nextfloat(Float32(1.0)) === 1.0000001f0
    @test nextfloat(Float16(1.0)) === Float16(1.001)
    @test nextfloat(0.0) === 5.0e-324      # smallest subnormal
    @test prevfloat(0.0) === -5.0e-324
    @test nextfloat(Inf) === Inf
    @test isnan(nextfloat(NaN))
    # 2-arg stepping (n ULPs), including negative n
    @test nextfloat(1.0, 3) === 1.0000000000000007
    @test prevfloat(1.0, 2) === 0.9999999999999998
    @test nextfloat(1.0, -1) === prevfloat(1.0)
end

true
