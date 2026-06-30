# Test Float32 type preservation for arithmetic and math functions (Issue #3462)

using Test

@testset "type_inference_float32_preservation: Float32 preserved in arithmetic" begin
    x = Float32(2.0)
    y = Float32(3.0)

    # Division preserves Float32
    @test typeof(x / y) == Float32
    # Power preserves Float32 when exponent is not Float64
    @test typeof(x ^ 2) == Float32
    @test typeof(x ^ y) == Float32
    # Float64 dominates
    @test typeof(x / 1.0) == Float64
end

true
