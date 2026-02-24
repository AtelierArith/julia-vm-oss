# Float32 mixed-type arithmetic tests (Issue #1659)
# Tests that Float32 operations with other numeric types work correctly

using Test

@testset "Float32 mixed-type arithmetic" begin
    @testset "Float32 + Int64" begin
        result = Float32(2.5) + 3
        @test result == 5.5
    end

    @testset "Float32 - Int64" begin
        result = Float32(5.5) - 3
        @test result == 2.5
    end

    @testset "Float32 * Int64" begin
        result = Float32(2.5) * 2
        @test result == 5.0
    end

    @testset "Float32 / Int64" begin
        result = Float32(6.0) / 2
        @test result == 3.0
    end

    @testset "Float32 + Float64" begin
        result = Float32(2.5) + 1.5
        @test result == 4.0
    end

    @testset "Float32 - Float64" begin
        result = Float32(5.0) - 1.5
        @test result == 3.5
    end

    @testset "Float32 * Float64" begin
        result = Float32(2.0) * 1.5
        @test result == 3.0
    end

    @testset "Float32 / Float64" begin
        result = Float32(6.0) / 2.0
        @test result == 3.0
    end
end

true
