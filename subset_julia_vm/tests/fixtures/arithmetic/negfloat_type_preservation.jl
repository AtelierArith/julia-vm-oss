using Test

# Test that NegFloat intrinsic preserves Float32/Float16 type (Issue #2220)

@testset "NegFloat type preservation" begin
    # Float64 negation preserves type
    @test typeof(-Float64(1.5)) == Float64
    @test -Float64(1.5) == -1.5

    # Float32 negation preserves type
    @test typeof(-Float32(1.5)) == Float32
    @test -Float32(1.5) == Float32(-1.5)

    # Negation of negative Float32 preserves type
    @test typeof(-Float32(-2.0)) == Float32
    @test -Float32(-2.0) == Float32(2.0)

    # Variable negation preserves type
    x = Float32(3.0)
    @test typeof(-x) == Float32
    @test -x == Float32(-3.0)
end

true
