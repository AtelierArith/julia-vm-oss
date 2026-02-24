# Test isinteger, isfinite, isinf, isnan float query functions (Issue #1870)

using Test

@testset "isinteger" begin
    @test isinteger(1.0) == true
    @test isinteger(2.0) == true
    @test isinteger(-3.0) == true
    @test isinteger(0.0) == true
    @test isinteger(1.5) == false
    @test isinteger(0.1) == false
end

@testset "isfinite" begin
    @test isfinite(1.0) == true
    @test isfinite(0.0) == true
    @test isfinite(-1e300) == true
    @test isfinite(Inf) == false
    @test isfinite(-Inf) == false
    @test isfinite(NaN) == false
end

@testset "isinf" begin
    @test isinf(Inf) == true
    @test isinf(-Inf) == true
    @test isinf(1.0) == false
    @test isinf(0.0) == false
    @test isinf(NaN) == false
end

@testset "isnan" begin
    @test isnan(NaN) == true
    @test isnan(1.0) == false
    @test isnan(0.0) == false
    @test isnan(Inf) == false
end

true
