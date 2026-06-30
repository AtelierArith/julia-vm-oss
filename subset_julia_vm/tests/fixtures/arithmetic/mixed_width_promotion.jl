# Issue #3742: Mixed-width narrow integer arithmetic must preserve the
# promoted type (wider operand wins; for same-width signed/unsigned the
# unsigned type wins). The same shape applies to Int + Float32, Bool +
# Float32, and Float16 + Float32: dispatch reaches Pure Julia
# +(::Number, ::Number) which calls promote(), and the result type must
# match `promote_type` rather than collapsing to Int64/Float64.

using Test

@testset "mixed-width narrow integer promotion" begin
    # Mixed-width same-sign integers
    @test typeof(Int8(1) + Int16(2)) === Int16
    @test typeof(Int8(1) + Int32(2)) === Int32
    @test typeof(Int16(1) + Int32(2)) === Int32
    @test typeof(UInt8(1) + UInt16(2)) === UInt16
    @test typeof(UInt8(1) + UInt32(2)) === UInt32
    @test typeof(UInt32(1) + UInt64(2)) === UInt64
    @test typeof(UInt16(1) + UInt64(2)) === UInt64

    # Mixed signed/unsigned: same width promotes to unsigned
    @test typeof(Int8(1) + UInt8(2)) === UInt8
    @test typeof(UInt8(1) + Int8(2)) === UInt8
    @test typeof(Int16(1) + UInt16(2)) === UInt16
    @test typeof(Int32(1) + UInt32(2)) === UInt32

    # Mixed signed/unsigned: wider wins
    @test typeof(Int16(1) + UInt8(2)) === Int16
    @test typeof(Int32(1) + UInt8(2)) === Int32
    @test typeof(Int8(1) + UInt16(2)) === UInt16
    @test typeof(Int8(1) + UInt32(2)) === UInt32

    # Subtraction, multiplication preserve the promotion as well
    @test typeof(Int8(3) - Int16(1)) === Int16
    @test typeof(Int8(2) * Int16(3)) === Int16
    @test typeof(UInt32(5) - UInt64(2)) === UInt64
end

@testset "narrow Int + narrow Float promotion" begin
    # Int + Float32 must yield Float32 (not Float64)
    @test typeof(Int8(1) + Float32(1.0)) === Float32
    @test typeof(Int16(1) + Float32(1.0)) === Float32
    @test typeof(Int32(1) + Float32(1.0)) === Float32
    @test typeof(Int64(1) + Float32(1.0)) === Float32
    @test typeof(UInt32(1) + Float32(1.0)) === Float32
    @test typeof(UInt64(1) + Float32(1.0)) === Float32
    @test typeof(Float32(1.0) + Int8(1)) === Float32

    # Float16 + Float32 → Float32 (wider float wins)
    @test typeof(Float16(1.0) + Float32(2.0)) === Float32

    # Bool participates in promotion: Bool + narrow type → narrow type
    @test typeof(true + Int8(1)) === Int8
    @test typeof(true + Int16(1)) === Int16
    @test typeof(true + UInt8(1)) === UInt8
    @test typeof(true + Float32(1.0)) === Float32

    # Float64 still wins over narrow ints (existing behavior, not regressed)
    @test typeof(Int8(1) + Float64(1.0)) === Float64
    @test typeof(Float32(1.0) + Float64(2.0)) === Float64
end

@testset "typed-parameter mixed-width promotion (Issue #3742)" begin
    f1(a::Int8, b::Int16) = a + b
    f2(a::UInt32, b::UInt64) = a + b
    f3(a::Int64, b::Float32) = a + b
    f4(a::Int8, b::UInt8) = a + b

    @test typeof(f1(Int8(1), Int16(2))) === Int16
    @test f1(Int8(1), Int16(2)) == 3
    @test typeof(f2(UInt32(1), UInt64(2))) === UInt64
    @test f2(UInt32(1), UInt64(2)) == 3
    @test typeof(f3(Int64(1), Float32(1.0))) === Float32
    @test f3(Int64(1), Float32(1.0)) ≈ 2.0f0
    @test typeof(f4(Int8(1), UInt8(2))) === UInt8
    @test f4(Int8(1), UInt8(2)) == 3
end

@testset "mixed-width arithmetic values" begin
    # Sanity: numeric values must still be correct, not just types
    @test (Int8(1) + Int16(2)) == 3
    @test (UInt32(1) + UInt64(2)) == 3
    @test (1 + Float32(1.0)) ≈ 2.0f0
    @test (Int8(10) - Int16(3)) == 7
    @test (Int8(2) * Int16(3)) == 6
    @test (Int16(10) ÷ Int8(3)) == 3
    @test (Int16(10) % Int8(3)) == 1
end

true
