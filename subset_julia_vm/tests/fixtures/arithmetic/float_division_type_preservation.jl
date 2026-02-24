using Test

@testset "Float32/Float16 division type preservation" begin
    # Float32 division should preserve Float32 (Issue #2225)
    @test typeof(Float32(6.0) / true) == Float32
    @test typeof(Float32(6.0) / Float32(2.0)) == Float32

    # Float16 division should preserve Float16
    @test typeof(Float16(6.0) / Float16(2.0)) == Float16
    @test typeof(Float16(6.0) / true) == Float16

    # Value correctness
    @test Float32(6.0) / true == Float32(6.0)
    @test Float32(6.0) / Float32(2.0) == Float32(3.0)
    @test Float16(6.0) / Float16(2.0) == Float16(3.0)

    # Float64 division baseline
    @test typeof(6.0 / true) == Float64
    @test typeof(6.0 / 2.0) == Float64
end

true
