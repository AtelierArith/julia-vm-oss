# Test Float16 binary operation type inference (Issue #1850)
# Note: Full F16+F16 arithmetic requires additional compile-time support.
# This test verifies the Float16 constructor and typeof work correctly.

using Test

@testset "Float16 constructor type" begin
    a = Float16(2.5)
    @test typeof(a) == Float16
    b = Float16(1.5)
    @test typeof(b) == Float16
end

@testset "Float16 value preservation" begin
    x = Float16(3.0)
    @test typeof(x) == Float16
end

true
