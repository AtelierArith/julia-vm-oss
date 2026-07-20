# Test zero() and one() numeric identity functions (Issue #1870)

using Test

@testset "zero basic" begin
    @test zero(1) == 0
    @test zero(1.0) == 0.0
    @test zero(Int64) == 0
    @test zero(Float64) == 0.0
end

@testset "one basic" begin
    @test one(1) == 1
    @test one(1.0) == 1.0
    @test one(Int64) == 1
    @test one(Float64) == 1.0
end

@testset "oneunit basic" begin
    @test oneunit(1) == 1
    @test oneunit(1.0) == 1.0
    @test oneunit(Int64) == 1
    @test oneunit(Float64) == 1.0
end

true
