# Regression test for Issue #1659 / #1661
# Float32 mixed-type arithmetic in a single file with nested @testset blocks.
# This combination previously caused MethodError due to Float32 not being
# recognized as a primitive type in the dynamic dispatch fallback path.

using Test

@testset "Float32 Mixed-Type Combined Regression" begin
    @testset "Float32 × Float32" begin
        @test Float32(2.5) + Float32(1.5) == Float32(4.0)
        @test typeof(Float32(2.5) + Float32(1.5)) == Float32
        @test Float32(5.0) - Float32(1.5) == Float32(3.5)
        @test typeof(Float32(5.0) - Float32(1.5)) == Float32
        @test Float32(2.0) * Float32(3.0) == Float32(6.0)
        @test typeof(Float32(2.0) * Float32(3.0)) == Float32
        @test Float32(6.0) / Float32(2.0) == Float32(3.0)
        @test typeof(Float32(6.0) / Float32(2.0)) == Float32
    end
    @testset "Float32 × Int64" begin
        @test Float32(2.5) + 3 == 5.5
        @test typeof(Float32(2.5) + 3) == Float32
        @test 3 + Float32(2.5) == 5.5
        @test typeof(3 + Float32(2.5)) == Float32
        @test Float32(5.0) - 2 == 3.0
        @test typeof(Float32(5.0) - 2) == Float32
        @test Float32(2.5) * 3 == 7.5
        @test typeof(Float32(2.5) * 3) == Float32
        @test Float32(6.0) / 2 == 3.0
        @test typeof(Float32(6.0) / 2) == Float32
    end
    @testset "Float32 × Float64" begin
        @test Float32(2.5) + 1.5 == 4.0
        @test typeof(Float32(2.5) + 1.5) == Float64
        @test 1.5 + Float32(2.5) == 4.0
        @test typeof(1.5 + Float32(2.5)) == Float64
        @test Float32(5.0) - 1.5 == 3.5
        @test typeof(Float32(5.0) - 1.5) == Float64
        @test Float32(2.5) * 1.5 == 3.75
        @test typeof(Float32(2.5) * 1.5) == Float64
        @test Float32(6.0) / 1.5 == 4.0
        @test typeof(Float32(6.0) / 1.5) == Float64
    end
    @testset "Float32 × Bool" begin
        @test Float32(2.5) + true == 3.5
        @test typeof(Float32(2.5) + true) == Float32
        @test true + Float32(2.5) == 3.5
        @test typeof(true + Float32(2.5)) == Float32
        @test Float32(2.5) - true == 1.5
        @test typeof(Float32(2.5) - true) == Float32
        @test Float32(2.5) * true == 2.5
        @test typeof(Float32(2.5) * true) == Float32
        @test Float32(2.5) * false == 0.0
    end
end

true
