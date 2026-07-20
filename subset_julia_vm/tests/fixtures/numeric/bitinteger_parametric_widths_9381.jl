# Issue #9381: fixed-width integer methods should use upstream-shaped
# BitSigned / BitUnsigned parametric dispatch without losing the type
# preservation previously guarded by per-width div specializations.

using Test

function positive_width_probe(x::T, y::T) where {T}
    @test div(x, y) === T(2)
    @test rem(x, y) === T(1)
    @test mod(x, y) === T(1)

    rotl = bitrotate(x, 1)
    rotr = bitrotate(x, -1)
    @test typeof(rotl) === T
    @test typeof(rotr) === T
end

function signed_mod_probe(x::T, y::T) where {T}
    @test mod(x, y) === T(2)
end

@testset "BitSigned parametric div/rem/mod preserve widths (Issue #9381)" begin
    positive_width_probe(Int8(7), Int8(3))
    positive_width_probe(Int16(7), Int16(3))
    positive_width_probe(Int32(7), Int32(3))
    positive_width_probe(Int64(7), Int64(3))
    positive_width_probe(Int128(7), Int128(3))

    signed_mod_probe(Int8(-7), Int8(3))
    signed_mod_probe(Int16(-7), Int16(3))
    signed_mod_probe(Int32(-7), Int32(3))
    signed_mod_probe(Int64(-7), Int64(3))
    signed_mod_probe(Int128(-7), Int128(3))

    @test rem(typemin(Int8), Int8(-1)) === Int8(0)
    @test rem(typemin(Int16), Int16(-1)) === Int16(0)
    @test rem(typemin(Int32), Int32(-1)) === Int32(0)
    @test rem(typemin(Int64), Int64(-1)) === Int64(0)
    @test rem(typemin(Int128), Int128(-1)) === Int128(0)

    @test mod(typemin(Int8), Int8(-1)) === Int8(0)
    @test mod(typemin(Int16), Int16(-1)) === Int16(0)
    @test mod(typemin(Int32), Int32(-1)) === Int32(0)
    @test mod(typemin(Int64), Int64(-1)) === Int64(0)
    @test mod(typemin(Int128), Int128(-1)) === Int128(0)

    @test_throws DivideError div(typemin(Int8), Int8(-1))
    @test_throws DivideError div(typemin(Int16), Int16(-1))
    @test_throws DivideError div(typemin(Int32), Int32(-1))
    @test_throws DivideError div(typemin(Int64), Int64(-1))
    @test_throws DivideError div(typemin(Int128), Int128(-1))
end

@testset "BitUnsigned parametric div/rem/mod preserve widths (Issue #9381)" begin
    positive_width_probe(UInt8(7), UInt8(3))
    positive_width_probe(UInt16(7), UInt16(3))
    positive_width_probe(UInt32(7), UInt32(3))
    positive_width_probe(UInt64(7), UInt64(3))
    positive_width_probe(UInt128(7), UInt128(3))

    @test div(UInt64(0xffffffffffffffff), UInt64(3)) === UInt64(0x5555555555555555)
    @test div(UInt128(0xffffffffffffffffffffffffffffffff), UInt128(3)) ===
          UInt128(0x55555555555555555555555555555555)
end

@testset "Bool and BigInt stay outside BitInteger aliases (Issue #9381)" begin
    @test div(true, true) === true
    @test typeof(div(true, true)) === Bool

    q = div(big(6), big(2))
    @test q == big(3)
    @test typeof(q) === BigInt
end

function scalar_helper_probe(x::T) where {T<:Number}
    @test eltype(x) === T
    @test zero(x) === zero(T)
    @test one(x) === one(T)
end

@testset "Number scalar helpers use parametric value methods (Issue #9381)" begin
    scalar_helper_probe(Int8(3))
    scalar_helper_probe(Int16(3))
    scalar_helper_probe(Int32(3))
    scalar_helper_probe(Int64(3))
    scalar_helper_probe(Int128(3))
    scalar_helper_probe(UInt8(3))
    scalar_helper_probe(UInt16(3))
    scalar_helper_probe(UInt32(3))
    scalar_helper_probe(UInt64(3))
    scalar_helper_probe(UInt128(3))
    scalar_helper_probe(Float16(3))
    scalar_helper_probe(Float32(3))
    scalar_helper_probe(Float64(3))
    scalar_helper_probe(true)

    z = zero(big(3))
    o = one(big(3))
    @test z == big(0)
    @test o == big(1)
    @test typeof(z) === BigInt
    @test typeof(o) === BigInt

    @test float(Int8(3)) === 3.0
    @test float(UInt128(3)) === 3.0
    @test float(Float32(3)) === Float32(3)
    @test float(BigFloat(3)) == BigFloat(3)
    @test typeof(float(big(3))) === BigFloat
end

@testset "signed/unsigned use parametric value methods (Issue #9381)" begin
    @test signed(UInt8(0xff)) === Int8(-1)
    @test signed(UInt16(0xffff)) === Int16(-1)
    @test signed(UInt32(0xffffffff)) === Int32(-1)
    @test signed(UInt64(0xffffffffffffffff)) === Int64(-1)
    @test signed(UInt128(0xffffffffffffffffffffffffffffffff)) === Int128(-1)
    @test signed(Int8(-1)) === Int8(-1)
    @test signed(Bool(true)) === Int64(1)

    @test unsigned(Int8(-1)) === UInt8(0xff)
    @test unsigned(Int16(-1)) === UInt16(0xffff)
    @test unsigned(Int32(-1)) === UInt32(0xffffffff)
    @test unsigned(Int64(-1)) === UInt64(0xffffffffffffffff)
    @test unsigned(Int128(-1)) === UInt128(0xffffffffffffffffffffffffffffffff)
    @test unsigned(UInt8(1)) === UInt8(1)
    @test unsigned(Bool(true)) === UInt64(1)
end

@testset "convert uses parametric numeric target methods (Issue #9381)" begin
    @test convert(Int32, true) === Int32(1)
    @test convert(Float32, true) === Float32(1)
    @test convert(Float64, Int8(3)) === 3.0
    @test convert(Int64, 3.0) === Int64(3)

    @test convert(Complex{Float64}, Int8(1)) === Complex{Float64}(1.0, 0.0)
    @test convert(Complex{Float64}, Complex{Int64}(1, 2)) === Complex{Float64}(1.0, 2.0)
    @test convert(Complex{Float32}, true) === Complex{Float32}(Float32(1), Float32(0))

    @test convert(Rational{Int8}, true) === Rational{Int8}(Int8(1), Int8(1))
    @test convert(Rational{UInt8}, Int16(3) // Int16(2)) === Rational{UInt8}(UInt8(3), UInt8(2))
    r_big = convert(Rational{BigInt}, Int8(3))
    @test r_big == Rational{BigInt}(big(3), big(1))
    @test typeof(r_big) === Rational{BigInt}

    @test convert(Float64, Int8(3) // Int8(2)) === 1.5
    @test convert(Float32, Int8(3) // Int8(2)) === Float32(1.5)
end

@testset "Rational and // use integer-parametric constructors (Issue #9381)" begin
    r8 = Rational(Int8(6), Int8(4))
    @test r8 == Int8(3) // Int8(2)
    @test typeof(r8) === Rational{Int8}
    @test typeof(Int8(6) // Int8(4)) === Rational{Int8}

    r128 = Rational(Int128(6), Int128(4))
    @test r128 == Int128(3) // Int128(2)
    @test typeof(r128) === Rational{Int128}

    rb = Rational(big(6), big(4))
    @test rb == big(3) // big(2)
    @test typeof(rb) === Rational{BigInt}
end

@testset "Complex constructors use Real-parametric promotion (Issue #9381)" begin
    @test Complex(1, 2) === Complex{Int64}(1, 2)
    @test Complex(Int8(1), Int8(2)) === Complex{Int8}(Int8(1), Int8(2))
    @test Complex(true, false) === Complex{Bool}(true, false)
    @test Complex(1, Float32(2)) === Complex{Float32}(Float32(1), Float32(2))
    @test Complex(1.0, Float32(2)) === Complex{Float64}(1.0, 2.0)
    @test Complex(Int8(3)) === Complex{Int8}(Int8(3), Int8(0))
end

@testset "zeros/ones use parametric allocation helpers (Issue #9381)" begin
    zi = zeros(Int64, 2, 2)
    @test typeof(zi) === Matrix{Int64}
    @test zi[1, 1] === Int64(0)

    zu = zeros(UInt8, (2, 2))
    @test typeof(zu) === Matrix{UInt8}
    @test zu[1, 1] === UInt8(0)

    oc = ones(Complex{Float64}, 2, 2)
    @test typeof(oc) === Matrix{ComplexF64}
    @test oc[1, 1] === Complex{Float64}(1.0, 0.0)

    of = ones(Float32, (2,))
    @test typeof(of) === Vector{Float32}
    @test of[1] === Float32(1)
end

true
