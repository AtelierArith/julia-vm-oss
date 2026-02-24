# Float32 negation type preservation (Issue #1762)
# Tests that unary minus preserves Float32 type

using Test

@testset "Float32 negation" begin
    @test -Float32(2.5) == Float32(-2.5)
    @test typeof(-Float32(2.5)) == Float32
    @test -Float32(0.0) == Float32(0.0)
    @test typeof(-Float32(0.0)) == Float32
    @test -Float32(-3.5) == Float32(3.5)
    @test typeof(-Float32(-3.5)) == Float32
end

true
