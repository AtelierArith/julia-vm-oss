# Test zero() returns correct types (Issue #2181)
# Julia: zero(T) returns zero of type T, zero(x) returns zero of typeof(x)

using Test

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
