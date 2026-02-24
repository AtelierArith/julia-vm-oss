using Test

# Prevention test: Type{T}→T function return types (Issue #2245)
# Verifies that functions like typemin, typemax, zero, one return the
# correct type matching their type argument.
#
# This catches bugs where the method table incorrectly infers the return
# type for Pure Julia methods.

@testset "typemin return types (Issue #2245)" begin
    # Float types
    @test typeof(typemin(Float64)) == Float64
    @test typeof(typemin(Float32)) == Float32
    @test typeof(typemin(Float16)) == Float16

    # Signed integer types
    @test typeof(typemin(Int64)) == Int64
    @test typeof(typemin(Int32)) == Int32
    @test typeof(typemin(Int16)) == Int16
    @test typeof(typemin(Int8)) == Int8

    # UInt64 - only unsigned type with typemin implemented
    @test typeof(typemin(UInt64)) == UInt64

    # Bool
    @test typeof(typemin(Bool)) == Bool
end

@testset "typemax return types (Issue #2245)" begin
    # Float types
    @test typeof(typemax(Float64)) == Float64
    @test typeof(typemax(Float32)) == Float32
    @test typeof(typemax(Float16)) == Float16

    # Signed integer types
    @test typeof(typemax(Int64)) == Int64
    @test typeof(typemax(Int32)) == Int32
    @test typeof(typemax(Int16)) == Int16
    @test typeof(typemax(Int8)) == Int8

    # UInt64 - only unsigned type with typemax implemented
    @test typeof(typemax(UInt64)) == UInt64

    # Bool
    @test typeof(typemax(Bool)) == Bool
end

@testset "zero return types (Issue #2245)" begin
    # Test zero(Type)
    @test typeof(zero(Int64)) == Int64
    @test typeof(zero(Int32)) == Int32
    @test typeof(zero(Int16)) == Int16
    @test typeof(zero(Int8)) == Int8
    @test typeof(zero(Float64)) == Float64
    @test typeof(zero(Float32)) == Float32
    @test typeof(zero(Bool)) == Bool
end

@testset "one return types (Issue #2245)" begin
    # Test one(Type)
    @test typeof(one(Int64)) == Int64
    @test typeof(one(Float64)) == Float64
    @test typeof(one(Bool)) == Bool
end

@testset "typemin/typemax in expressions (Issue #2231)" begin
    # These originally failed because the return type was inferred as Bool
    @test typemin(Float64) < 0
    @test typemax(Float64) > 0
    @test typemin(Float64) + 1 == typemin(Float64) + 1
    @test typemax(Int64) - 1 < typemax(Int64)
end

true
