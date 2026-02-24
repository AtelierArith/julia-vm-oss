using Test

@testset "Chained Float32/Float16 operations type preservation" begin
    # Chained operations should preserve float type (Issue #2225)
    @test typeof(Float32(1.0) + true + true) == Float32
    @test typeof(Float32(1.0) + Float32(2.0) + Float32(3.0)) == Float32
    @test typeof(Float32(2.0) * true + Float32(1.0)) == Float32

    # Float16 chained operations
    @test typeof(Float16(1.0) + true + true) == Float16
    @test typeof(Float16(1.0) + Float16(2.0) + Float16(3.0)) == Float16

    # Value correctness
    @test Float32(1.0) + true + true == Float32(3.0)
    @test Float16(1.0) + true + true == Float16(3.0)
end

true
