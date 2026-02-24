using Test

@testset "Float32 + Int64 type preservation" begin
    # Float32 + Int64 should return Float32 (Issue #2204)
    @test typeof(Float32(2.0) + Int64(1)) == Float32
    @test typeof(Int64(1) + Float32(2.0)) == Float32
    @test typeof(Float32(2.0) - Int64(1)) == Float32
    @test typeof(Float32(2.0) * Int64(2)) == Float32
    @test typeof(Float32(6.0) / Int64(2)) == Float32

    # Float16 + Int64 should return Float16
    @test typeof(Float16(2.0) + Int64(1)) == Float16
    @test typeof(Int64(1) + Float16(2.0)) == Float16
    @test typeof(Float16(2.0) * Int64(2)) == Float16

    # Value correctness
    @test Float32(2.0) + Int64(1) == Float32(3.0)
    @test Float32(6.0) / Int64(2) == Float32(3.0)
    @test Float32(2.0) * Int64(3) == Float32(6.0)
end

true
