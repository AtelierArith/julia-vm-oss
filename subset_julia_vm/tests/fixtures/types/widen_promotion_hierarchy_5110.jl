# widen promotion hierarchy (Issue #5110)
# Verifies the full upstream widen chain for the supported numeric types:
# type form `widen(::Type{T}) === wider` and value form
# `widen(x)` returns a value of the widened type with the same magnitude.
# Matches upstream Julia 1.12 (base/int.jl, base/float.jl, base/gmp.jl,
# base/mpfr.jl, base/operators.jl).

using Test

@testset "widen type-form promotion hierarchy" begin
    # Signed integers
    @test widen(Int8) === Int16
    @test widen(Int16) === Int32
    @test widen(Int32) === Int64
    @test widen(Int64) === Int128
    @test widen(Int128) === BigInt
    # Unsigned integers
    @test widen(UInt8) === UInt16
    @test widen(UInt16) === UInt32
    @test widen(UInt32) === UInt64
    @test widen(UInt64) === UInt128
    @test widen(UInt128) === BigInt
    # Arbitrary precision integer is already widest
    @test widen(BigInt) === BigInt
    # Floating point
    @test widen(Float16) === Float32
    @test widen(Float32) === Float64
    @test widen(Float64) === BigFloat
    # Arbitrary precision float is already widest
    @test widen(BigFloat) === BigFloat
end

@testset "widen value-form returns widened type" begin
    # Signed integers: typeof is the widened type
    @test typeof(widen(Int8(1))) === Int16
    @test typeof(widen(Int16(1))) === Int32
    @test typeof(widen(Int32(1))) === Int64
    @test typeof(widen(Int64(1))) === Int128
    @test typeof(widen(Int128(1))) === BigInt
    # Unsigned integers
    @test typeof(widen(UInt8(1))) === UInt16
    @test typeof(widen(UInt16(1))) === UInt32
    @test typeof(widen(UInt32(1))) === UInt64
    @test typeof(widen(UInt64(1))) === UInt128
    @test typeof(widen(UInt128(1))) === BigInt
    # Floating point
    @test typeof(widen(Float16(1))) === Float32
    @test typeof(widen(Float32(1))) === Float64
    @test typeof(widen(Float64(1))) === BigFloat
end

@testset "widen value-form preserves magnitude" begin
    @test widen(Int8(42)) == 42
    @test widen(Int16(100)) == 100
    @test widen(Int32(1000)) == 1000
    @test widen(Int64(9999)) == 9999
    @test widen(UInt8(7)) == 7
    @test widen(UInt64(123)) == 123
    @test widen(Float32(1.5f0)) == 1.5
    @test widen(Float64(2.5)) == 2.5
    # Values at the Int64 boundary widen without truncation
    @test widen(Int64(typemax(Int64))) == 9223372036854775807
    @test widen(UInt64(typemax(UInt64))) == 18446744073709551615
end

true
