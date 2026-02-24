# Float32 mod/rem type preservation (Issue #1762)
# Tests that mod() and % preserve Float32 type for Float32 operands

using Test

@testset "Float32 mod type preservation" begin
    @testset "Float32 % Float32" begin
        @test Float32(5.0) % Float32(3.0) == Float32(2.0)
        @test typeof(Float32(5.0) % Float32(3.0)) == Float32
        @test Float32(7.5) % Float32(2.5) == Float32(0.0)
        @test typeof(Float32(7.5) % Float32(2.5)) == Float32
    end

    @testset "mod(Float32, Float32)" begin
        @test mod(Float32(5.0), Float32(3.0)) == Float32(2.0)
        @test typeof(mod(Float32(5.0), Float32(3.0))) == Float32
        @test mod(Float32(7.5), Float32(2.5)) == Float32(0.0)
        @test typeof(mod(Float32(7.5), Float32(2.5))) == Float32
    end

    @testset "mod(Float32, Int)" begin
        @test mod(Float32(7.5), 3) == Float32(1.5)
        @test typeof(mod(Float32(7.5), 3)) == Float32
    end

    @testset "mod(Int, Float32)" begin
        @test mod(7, Float32(3.0)) == Float32(1.0)
        @test typeof(mod(7, Float32(3.0))) == Float32
    end
end

true
