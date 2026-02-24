# Test Float16 unary negation type preservation (Issue #1972)

using Test

@testset "Float16 negation type preservation" begin
    x = Float16(2.5)
    result = -x
    @test result == Float16(-2.5)
    @test typeof(result) == Float16
end

@testset "Float16 negation of zero" begin
    x = Float16(0.0)
    result = -x
    @test typeof(result) == Float16
end

true
