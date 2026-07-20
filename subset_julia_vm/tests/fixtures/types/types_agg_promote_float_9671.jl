# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: types/char_comparison.jl =====
# Test Char comparison operators
# Issue #945: Char comparison (==) not implemented


@testset "Char comparison operators" begin
    # Basic equality
    @test ('a' == 'a') == true
    @test ('a' == 'b') == false

    # Inequality
    @test ('a' != 'b') == true
    @test ('a' != 'a') == false

    # With variables
    c1 = 'a'
    c2 = 'a'
    c3 = 'b'
    @test (c1 == c2) == true
    @test (c1 == c3) == false
    @test (c1 != c3) == true

    # In array context
    chars = Char[]
    push!(chars, 'a')
    @test (chars[1] == 'a') == true
    @test (chars[1] != 'b') == true
end

# ===== source: types/float_all_numeric.jl =====
# Test float() for all numeric types (Issue #2165)
# Based on Julia's base/float.jl:375
# float(x::AbstractFloat) returns x (identity, preserves type)
# float(x::Integer) returns Float64(x)


@testset "float identity for AbstractFloat types" begin
    @test float(1.5) == 1.5
    @test float(0.0) == 0.0
    @test float(-3.14) == -3.14

    # Float32 preserves type (identity)
    f32 = Float32(1.5)
    @test float(f32) == Float32(1.5)

    # Float16 preserves type (identity)
    f16 = Float16(1.5)
    @test float(f16) == Float16(1.5)
end

@testset "float converts Int64 to Float64" begin
    @test float(42) == 42.0
    @test float(0) == 0.0
    @test float(-7) == -7.0
end

@testset "float converts other integer types to Float64" begin
    @test float(Int32(42)) == 42.0
    @test float(Int16(100)) == 100.0
    @test float(Int8(7)) == 7.0
    @test float(UInt8(255)) == 255.0
    @test float(UInt16(1000)) == 1000.0
    @test float(UInt32(100000)) == 100000.0
end

@testset "float converts Bool to Float64" begin
    @test float(true) == 1.0
    @test float(false) == 0.0
end

# ===== source: types/float_identity_4582.jl =====

@testset "Float identity and isequal parity (#4582 #4583 #4584)" begin
    @test Float32(1.5) === Float32(1.5)
    @test Float32(NaN) === Float32(NaN)
    @test !(Float32(-0.0) === Float32(0.0))
    @test isequal(Float32(NaN), Float32(NaN))
    @test !isequal(Float32(-0.0), Float32(0.0))
    @test !isequal(Float32(-0.0), 0.0)
    @test !isequal(0.0, Float32(-0.0))
    @test isequal(Float32(-0.0), -0.0)
    @test !isequal(Float32(-0.0), 0)
    @test !isequal(0, Float32(-0.0))

    @test Float16(1.5) === Float16(1.5)
    @test Float16(NaN) === Float16(NaN)
    @test !(Float16(-0.0) === Float16(0.0))
    @test isequal(Float16(NaN), Float16(NaN))
    @test !isequal(Float16(-0.0), Float16(0.0))
    @test !isequal(Float16(-0.0), 0.0)
    @test !isequal(0.0, Float16(-0.0))
    @test isequal(Float16(-0.0), -0.0)
    @test !isequal(Float16(-0.0), 0)
    @test !isequal(0, Float16(-0.0))

    @test true === true
    @test false === false
    @test !(true === false)
end

# ===== source: types/float_type_preservation.jl =====
# Float arithmetic type preservation test
# Ensures that arithmetic operations (+, -, *, /) preserve float types (F16, F32, F64)
# Prevention test for Issue #1647 / #1653


@testset "Float16 arithmetic type preservation" begin
    @test typeof(Float16(1.0) + Float16(2.0)) === Float16
    @test typeof(Float16(1.0) - Float16(2.0)) === Float16
    @test typeof(Float16(1.0) * Float16(2.0)) === Float16
    @test typeof(Float16(1.0) / Float16(2.0)) === Float16
end

@testset "Float32 arithmetic type preservation" begin
    @test typeof(Float32(1.0) + Float32(2.0)) === Float32
    @test typeof(Float32(1.0) - Float32(2.0)) === Float32
    @test typeof(Float32(1.0) * Float32(2.0)) === Float32
    @test typeof(Float32(1.0) / Float32(2.0)) === Float32
end

@testset "Float64 arithmetic type preservation" begin
    @test typeof(Float64(1.0) + Float64(2.0)) === Float64
    @test typeof(Float64(1.0) - Float64(2.0)) === Float64
    @test typeof(Float64(1.0) * Float64(2.0)) === Float64
    @test typeof(Float64(1.0) / Float64(2.0)) === Float64
end

# ===== source: types/map_mixed_typeof.jl =====
# map with mixed numeric inputs should return Vector{Float64}


@testset "map over mixed numeric inputs returns Vector{Float64}" begin
    arr = [1.0, 2, 3]
    result = map(x -> x ^ 2, arr)
    @test (typeof(result) === Vector{Float64})
end

# ===== source: types/map_rational_typeof.jl =====
# map over Rational inputs should preserve Rational element type


@testset "map over Rational inputs preserves Rational element type" begin
    arr = [1//3, 1//3, 1//3]
    result = map(x -> x ^ 2, arr)
    @test (typeof(result) === Vector{Rational{Int64}})
end

# ===== source: types/missing_basic.jl =====
# Test Missing type and related functions


@testset "Missing type: literal, typeof, ismissing, coalesce functions" begin

    # Basic missing value
    x = missing
    @assert typeof(x) == Missing

    # ismissing function - use @assert directly (Bool comparison issue workaround)
    @assert ismissing(missing)
    if ismissing(42)
        error("ismissing(42) should be false")
    end
    if ismissing(nothing)
        error("ismissing(nothing) should be false")
    end
    if ismissing("hello")
        error("ismissing('hello') should be false")
    end

    # coalesce returns first non-missing value
    @assert coalesce(1, 2) == 1
    @assert coalesce(missing, 2) == 2
    @assert coalesce(missing, missing, 3) == 3
    @assert coalesce(1, missing, 3) == 1

    # skipmissing with for loop
    data = [1, 2, 3, 4, 5]
    total = 0
    for v in skipmissing(data)
        total = total + v
    end
    @assert total == 15

    # skipmissing with collect (returns Float64 array due to collect implementation)
    collected = collect(skipmissing([1.0, 2.0, 3.0]))
    @assert length(collected) == 3
    @assert collected[1] == 1.0
    @assert collected[2] == 2.0
    @assert collected[3] == 3.0

    @test (true)
end

# ===== source: types/promote_type.jl =====
# Test: promote_type type promotion (Issue #762)


@testset "promote_type type promotion (Issue #762)" begin
    # Same type returns that type
    @test promote_type(Int64, Int64) == Int64
    @test promote_type(Float64, Float64) == Float64

    # Int64 + Float64 should return Float64 (Issue #762)
    @test promote_type(Int64, Float64) == Float64
    @test promote_type(Float64, Int64) == Float64

    # Smaller integers promote to larger integers
    @test promote_type(Int32, Int64) == Int64
    @test promote_type(Int64, Int32) == Int64

    # Bool promotes to integers
    @test promote_type(Bool, Int64) == Int64
    @test promote_type(Int64, Bool) == Int64

    # Bool promotes to Float64
    @test promote_type(Bool, Float64) == Float64
    @test promote_type(Float64, Bool) == Float64
end

# ===== source: types/test_rounding_mode.jl =====
# Test RoundingMode type and constants (Issue #428)
# Note: SubsetJuliaVM uses struct with Symbol field, Julia uses parametric type


@testset "RoundingMode type and constants" begin
    # Test RoundingMode struct exists and constants are instances
    @test isa(RoundNearest, RoundingMode)
    @test isa(RoundToZero, RoundingMode)
    @test isa(RoundUp, RoundingMode)
    @test isa(RoundDown, RoundingMode)
    @test isa(RoundFromZero, RoundingMode)
    @test isa(RoundNearestTiesAway, RoundingMode)
    @test isa(RoundNearestTiesUp, RoundingMode)

    # Test that different rounding modes are not equal (identity test)
    @test RoundNearest !== RoundToZero
    @test RoundUp !== RoundDown
    @test RoundFromZero !== RoundToZero

    # Test total count of 7 standard rounding modes
    modes = [RoundNearest, RoundToZero, RoundUp, RoundDown,
             RoundFromZero, RoundNearestTiesAway, RoundNearestTiesUp]
    @test length(modes) == 7
end

# ===== source: types/test_version.jl =====
# Test VersionNumber type and VERSION constant


@testset "VersionNumber type" begin
    # Test basic construction
    v = VersionNumber(1, 2, 3)
    @test v.major == 1
    @test v.minor == 2
    @test v.patch == 3
    
    # Test constructor with defaults
    v2 = VersionNumber(2, 5)
    @test v2.major == 2
    @test v2.minor == 5
    @test v2.patch == 0
    
    v3 = VersionNumber(3)
    @test v3.major == 3
    @test v3.minor == 0
    @test v3.patch == 0
end

@testset "VERSION constant" begin
    # VERSION should be a VersionNumber
    @test typeof(VERSION) == VersionNumber
    
    # VERSION should have valid fields
    @test VERSION.major >= 0
    @test VERSION.minor >= 0
    @test VERSION.patch >= 0
end

# ===== source: types/widen.jl =====
# Test widen function
# widen returns a wider type for numeric values


@testset "widen function for type widening" begin

    # Value-based widen - check that values are correctly widened
    r1 = widen(Int8(42)) == 42
    r2 = widen(Int16(100)) == 100
    r3 = widen(Int32(1000)) == 1000
    r4 = widen(Int64(9999)) == 9999

    # Float32 to Float64 conversion
    f32 = Float32(3.14)
    f64 = widen(f32)
    # Check that the widened value is approximately correct (Float32 precision)
    r5 = abs(f64 - 3.14) < 0.01

    # Check that widen of Int64 stays Int64 (can't widen further)
    r6 = widen(Int64(123)) == 123

    # All tests must pass
    @test ((r1 && r2 && r3 && r4 && r5 && r6) ? 1 : 0) == 1.0
end

# ===== source: types/widen_promotion_hierarchy_5110.jl =====
# widen promotion hierarchy (Issue #5110)
# Verifies the full upstream widen chain for the supported numeric types:
# type form `widen(::Type{T}) === wider` and value form
# `widen(x)` returns a value of the widened type with the same magnitude.
# Matches upstream Julia 1.12 (base/int.jl, base/float.jl, base/gmp.jl,
# base/mpfr.jl, base/operators.jl).


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

# ===== source: types/zero_type_correctness.jl =====
# Test zero() returns correct types (Issue #2181)
# Julia: zero(T) returns zero of type T, zero(x) returns zero of typeof(x)


@testset "zero(Type) returns correct type" begin
    @test zero(Int64) === Int64(0)
    @test zero(Float64) === 0.0
    @test zero(Float32) === Float32(0.0)
    @test zero(Bool) === false
end

@testset "zero(value) returns correct type" begin
    @test zero(42) === Int64(0)
    @test zero(3.14) === 0.0
    @test typeof(zero(42)) == Int64
    @test typeof(zero(3.14)) == Float64
end

@testset "zero(Type) typeof check" begin
    @test typeof(zero(Int64)) == Int64
    @test typeof(zero(Float64)) == Float64
    @test typeof(zero(Float32)) == Float32
    @test typeof(zero(Bool)) == Bool
end

@testset "one(Type) returns correct type (regression)" begin
    @test one(Int64) === Int64(1)
    @test one(Float64) === 1.0
    @test typeof(one(Int64)) == Int64
    @test typeof(one(Float64)) == Float64
end

true
