# Test promote for Float16 value conversions (Issue #1790)
# Verifies that promote correctly handles Float16 mixed-type values
# Float16 must trigger Julia fallback, not the hardcoded Int64/Float64 path

using Test

@testset "promote Float16 value conversions" begin
    @testset "Float16 with Int64" begin
        result = promote(Float16(1.5), Int64(2))
        @test typeof(result[1]) === Float16
        @test typeof(result[2]) === Float16
    end

    @testset "Int64 with Float16" begin
        result = promote(Int64(3), Float16(2.5))
        @test typeof(result[1]) === Float16
        @test typeof(result[2]) === Float16
    end

    @testset "Float16 with Bool" begin
        result = promote(Float16(1.5), true)
        @test typeof(result[1]) === Float16
        @test typeof(result[2]) === Float16
    end

    @testset "Float16 with Float32" begin
        result = promote(Float16(1.5), Float32(2.5))
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Float16 with Float64" begin
        result = promote(Float16(1.5), 2.5)
        @test typeof(result[1]) === Float64
        @test typeof(result[2]) === Float64
    end
end

true
