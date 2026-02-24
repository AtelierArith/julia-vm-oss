# Test promote for Float32 with small integer types (Issue #1790)
# Verifies that promote correctly converts Float32 + Int32/Int16/Int8 to Float32
# and that Float32 identity promotion works via the builtin handler

using Test

@testset "promote Float32 with small integer types" begin
    @testset "Float32 with Int32" begin
        result = promote(Float32(1.5), Int32(2))
        @test result[1] === Float32(1.5)
        @test result[2] === Float32(2.0)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Int32 with Float32" begin
        result = promote(Int32(3), Float32(2.5))
        @test result[1] === Float32(3.0)
        @test result[2] === Float32(2.5)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Float32 with Int16" begin
        result = promote(Float32(1.5), Int16(2))
        @test result[1] === Float32(1.5)
        @test result[2] === Float32(2.0)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Int16 with Float32" begin
        result = promote(Int16(4), Float32(3.5))
        @test result[1] === Float32(4.0)
        @test result[2] === Float32(3.5)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Float32 with Int8" begin
        result = promote(Float32(1.5), Int8(2))
        @test result[1] === Float32(1.5)
        @test result[2] === Float32(2.0)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Int8 with Float32" begin
        result = promote(Int8(5), Float32(4.5))
        @test result[1] === Float32(5.0)
        @test result[2] === Float32(4.5)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end

    @testset "Float32 identity" begin
        result = promote(Float32(1.5), Float32(2.5))
        @test result[1] === Float32(1.5)
        @test result[2] === Float32(2.5)
        @test typeof(result[1]) === Float32
        @test typeof(result[2]) === Float32
    end
end

true
