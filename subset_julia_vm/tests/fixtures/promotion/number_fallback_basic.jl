# Basic mixed-type arithmetic via Number fallback operators
# Verifies that +(x::Number, y::Number) correctly promotes and computes

using Test

@testset "Number fallback basic arithmetic" begin
    # Float32 + Int64 -> Float32
    @test Float32(1.5) + 2 == Float32(3.5)
    @test 2 + Float32(1.5) == Float32(3.5)

    # Float32 - Int64 -> Float32
    @test Float32(3.5) - 1 == Float32(2.5)
    @test 5 - Float32(1.5) == Float32(3.5)

    # Float32 * Int64 -> Float32
    @test Float32(2.0) * 3 == Float32(6.0)
    @test 3 * Float32(2.0) == Float32(6.0)

    # Float32 / Int64 -> Float32
    @test Float32(6.0) / 2 == Float32(3.0)
    @test 6 / Float32(2.0) == Float32(3.0)

    # Float32 + Bool -> Float32
    @test Float32(1.5) + true == Float32(2.5)
    @test true + Float32(1.5) == Float32(2.5)
end

true
