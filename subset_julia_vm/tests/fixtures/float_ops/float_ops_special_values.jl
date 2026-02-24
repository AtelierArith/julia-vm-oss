using Test

@testset "float special values" begin
    @test isinf(Inf) == true
    @test isinf(-Inf) == true
    @test isinf(1.0) == false
    @test isnan(NaN) == true
    @test isnan(1.0) == false
    @test isfinite(1.0) == true
    @test isfinite(Inf) == false
    @test isfinite(NaN) == false
end

@testset "float rounding" begin
    @test floor(3.7) == 3.0
    @test floor(-3.2) == -4.0
    @test ceil(3.2) == 4.0
    @test ceil(-3.7) == -3.0
    @test round(3.5) == 4.0
    @test round(2.5) == 2.0
    @test trunc(3.9) == 3.0
    @test trunc(-3.9) == -3.0
end

@testset "float conversion" begin
    @test Float64(3) == 3.0
    @test Float64(true) == 1.0
    @test Int64(3.0) == 3
end

true
