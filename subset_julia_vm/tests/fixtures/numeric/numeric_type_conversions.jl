using Test

# Numeric type conversions
@testset "numeric type conversions" begin
    # Int to Float
    @test Float64(42) == 42.0
    @test Float32(42) === Float32(42.0)
    @test Float16(10) == Float16(10.0)

    # Float to Int (exact)
    @test Int64(3.0) == 3
    @test Int32(5.0) === Int32(5)

    # Int width conversions
    @test Int32(Int64(100)) === Int32(100)
    @test Int64(Int32(100)) === Int64(100)

    # Unsigned
    @test UInt64(42) == UInt64(42)
    @test UInt32(255) == UInt32(255)

    # Type assertions
    @test typeof(Int64(1)) == Int64
    @test typeof(Float32(1.0)) == Float32
    @test typeof(UInt8(255)) == UInt8
end

true
