# Test Float16 is recognized as primitive type for dispatch (Issue #1848)
# Note: Float16 mixed-type arithmetic (F16 + I64) requires additional dispatch paths
# This test verifies Float16 constructor and typeof work correctly

using Test

@testset "Float16 constructor and typeof" begin
    a = Float16(2.5)
    @test typeof(a) == Float16
end

@testset "Float16 value creation" begin
    x = Float16(0.0)
    @test typeof(x) == Float16
    y = Float16(1.0)
    @test typeof(y) == Float16
end

true
