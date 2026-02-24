# Test promote_type for Float32 combinations (Issue #1772)
# Verifies that the promotion system correctly handles Float32 type pairs

using Test

@testset "promote_type Float32 combinations" begin
    @testset "Float32 with integers" begin
        # Float32 + Int64 should promote to Float32
        @test promote_type(Float32, Int64) === Float32
        @test promote_type(Int64, Float32) === Float32

        # Float32 + Bool should promote to Float32
        @test promote_type(Float32, Bool) === Float32
        @test promote_type(Bool, Float32) === Float32

        # Float32 + Int32 should promote to Float32
        @test promote_type(Float32, Int32) === Float32
        @test promote_type(Int32, Float32) === Float32
    end

    @testset "Float32 with Float64" begin
        # Float32 + Float64 should promote to Float64
        @test promote_type(Float32, Float64) === Float64
        @test promote_type(Float64, Float32) === Float64
    end

    @testset "Float32 same type" begin
        # Float32 + Float32 should return Float32
        @test promote_type(Float32, Float32) === Float32
    end
end

true
