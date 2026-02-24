# Test Float16 mod/rem type preservation (Issue #1972)

using Test

@testset "mod(Float16, Float16) type preservation" begin
    x = Float16(5.0)
    y = Float16(3.0)
    result = mod(x, y)
    @test result == Float16(2.0)
    @test typeof(result) == Float16
end

@testset "mod(Float16, Int64) returns Float16" begin
    x = Float16(5.0)
    result = mod(x, 3)
    @test result == Float16(2.0)
    @test typeof(result) == Float16
end

@testset "mod(Float16, Float64) promotes to Float64" begin
    x = Float16(5.0)
    y = 3.0
    result = mod(x, y)
    @test typeof(result) == Float64
end

true
