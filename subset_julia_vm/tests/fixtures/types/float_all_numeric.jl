# Test float() for all numeric types (Issue #2165)
# Based on Julia's base/float.jl:375
# float(x::AbstractFloat) returns x (identity, preserves type)
# float(x::Integer) returns Float64(x)

using Test

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

true
