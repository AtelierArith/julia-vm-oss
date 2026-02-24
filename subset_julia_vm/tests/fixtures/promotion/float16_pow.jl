# Test Float16 power (^) operations with type preservation (Issue #1972)

using Test

@testset "Float16 ^ Float16 type preservation" begin
    x = Float16(2.0)
    y = Float16(3.0)
    result = x ^ y
    @test result == Float16(8.0)
    @test typeof(result) == Float16
end

@testset "Float16 ^ Int64 returns Float16" begin
    x = Float16(2.0)
    result = x ^ 3
    @test result == Float16(8.0)
    @test typeof(result) == Float16
end

@testset "Float16 ^ Float64 promotes to Float64" begin
    x = Float16(2.0)
    y = 3.0
    result = x ^ y
    @test typeof(result) == Float64
end

true
