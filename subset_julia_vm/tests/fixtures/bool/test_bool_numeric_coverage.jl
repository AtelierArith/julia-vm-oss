# Test Bool coverage in Pure Julia numeric functions
# Issue #2724: Ensure Bool type is covered in numeric function migrations
#
# Bool is a subtype of Integer in Julia, so it participates in numeric operations.
# When migrating functions to Pure Julia, Bool specializations must not be forgotten.

using Test

@testset "Bool numeric function coverage (Issue #2724)" begin
    @testset "zero and one for Bool" begin
        @test zero(true) === false
        @test zero(false) === false
        @test one(true) === true
        @test one(false) === true
        @test typeof(zero(true)) == Bool
        @test typeof(one(false)) == Bool
    end

    @testset "float(::Bool) returns Float64" begin
        @test float(true) === 1.0
        @test float(false) === 0.0
        @test typeof(float(true)) == Float64
        @test typeof(float(false)) == Float64
    end

    @testset "iszero and isone for Bool" begin
        @test iszero(false) == true
        @test iszero(true) == false
        @test isone(true) == true
        @test isone(false) == false
    end

    @testset "abs and abs2 for Bool" begin
        @test abs(true) == 1
        @test abs(false) == 0
        @test abs2(true) == 1
        @test abs2(false) == 0
    end

    @testset "signbit for Bool" begin
        @test signbit(true) == false
        @test signbit(false) == false
    end
end

true
