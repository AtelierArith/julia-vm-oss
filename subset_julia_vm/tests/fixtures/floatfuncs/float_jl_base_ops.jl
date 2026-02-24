# Tests that float.jl base operations load and work correctly
# Verifies Float64/Float32 arithmetic and comparisons via intrinsics
# See Issue #1765

using Test

@testset "float.jl base operations" begin
    @testset "Float64 arithmetic via intrinsics" begin
        @test 2.5 + 1.5 == 4.0
        @test 5.0 - 2.0 == 3.0
        @test 2.5 * 2.0 == 5.0
        @test 5.0 / 2.0 == 2.5
    end

    @testset "Float32 arithmetic preserves type" begin
        @test typeof(Float32(2.5) + Float32(1.5)) == Float32
        @test typeof(Float32(5.0) - Float32(2.0)) == Float32
        @test typeof(Float32(2.5) * Float32(2.0)) == Float32
        @test typeof(Float32(5.0) / Float32(2.0)) == Float32
    end

    @testset "Float32 + Int64 mixed-type preserves Float32 (Issue #1771)" begin
        @test typeof(Float32(2.5) + 1) == Float32
        @test typeof(1 + Float32(2.5)) == Float32
        @test typeof(Float32(5.0) - 2) == Float32
        @test typeof(5 - Float32(2.0)) == Float32
        @test typeof(Float32(2.5) * 2) == Float32
        @test typeof(2 * Float32(2.5)) == Float32
        @test typeof(Float32(5.0) / 2) == Float32
        @test typeof(5 / Float32(2.0)) == Float32
    end

    @testset "Float32 + Bool mixed-type preserves Float32 (Issue #1771)" begin
        @test typeof(Float32(2.5) + true) == Float32
        @test typeof(true + Float32(2.5)) == Float32
        @test typeof(Float32(5.0) - false) == Float32
        @test typeof(false - Float32(2.0)) == Float32
        @test typeof(Float32(2.5) * true) == Float32
        @test typeof(true * Float32(2.5)) == Float32
        @test typeof(Float32(5.0) / true) == Float32
        @test typeof(true / Float32(2.0)) == Float32
    end

    @testset "Float64 comparisons" begin
        @test 1.0 < 2.0
        @test 2.0 > 1.0
        @test 1.0 <= 1.0
        @test 1.0 >= 1.0
        @test 1.0 == 1.0
        @test 1.0 != 2.0
    end

    @testset "signbit function" begin
        @test signbit(-1.0) == true
        @test signbit(1.0) == false
        @test signbit(0.0) == false
    end

    @testset "abs function" begin
        @test abs(-3.5) == 3.5
        @test abs(3.5) == 3.5
        @test abs(0.0) == 0.0
    end
end

true
