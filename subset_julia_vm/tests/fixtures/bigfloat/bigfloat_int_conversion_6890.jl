using Test

# Issue #6890: converting a BigFloat to an integer type was unsupported
# ("Type error: Cannot convert BigFloat(...) to Int64"), so `Int(big(2.0))` and
# the typed rounding forms `floor(Int, x)` / `round(Int, x)` / `ceil(Int, x)` /
# `trunc(Int, x)` (= `T(round(x))`, base/floatfuncs.jl) all failed. The integer
# converters (convert_to_iNN / convert_to_uNN) now accept BigFloat via an exact
# integer-valued conversion (RustBigFloat::to_bigint_exact), raising InexactError
# when the value is non-finite or has a fractional part — matching upstream
# `(::Type{<:Integer})(x::BigFloat)`. Verified vs julia 1.12.6.

@testset "Int(::BigFloat) exact (Issue #6890)" begin
    @test Int(big(2.0)) == 2
    @test Int64(big(3.0)) == 3
    @test Int(big(-5.0)) == -5
    @test typeof(Int(big(2.0))) === Int64
end

@testset "typed floor/ceil/round/trunc Int (Issue #6890)" begin
    @test floor(Int, big(2.7)) == 2
    @test ceil(Int, big(2.3)) == 3
    @test round(Int, big(2.5)) == 2   # ties to even
    @test round(Int, big(3.5)) == 4   # ties to even
    @test trunc(Int, big(2.9)) == 2
    @test trunc(Int, big(-2.9)) == -2
    @test floor(Int, big(-2.1)) == -3
end

@testset "widths and unsigned (Issue #6890)" begin
    @test Int8(big(-128.0)) == -128
    @test Int8(big(127.0)) == 127
    @test UInt8(big(255.0)) == 255
    @test UInt8(big(0.0)) == 0
    @test Int16(big(-32768.0)) == -32768
    @test UInt64(big(1.0)) == 1
end

@testset "precision beyond Float64 (Issue #6890)" begin
    # 2^70 and an 18-digit integer exceed Float64's exact integer range, so a
    # round-trip through f64 would corrupt them.
    @test Int128(big(2.0)^70) == 1180591620717411303424
    @test Int(big"123456789012345678") == 123456789012345678
end

@testset "InexactError on non-integer / non-finite (Issue #6890)" begin
    @test_throws InexactError Int(big(2.5))
    @test_throws InexactError Int8(big(200.0))   # out of range
    @test_throws InexactError UInt8(big(-1.0))   # negative -> unsigned
    @test_throws InexactError Int(big(NaN))
end

true
