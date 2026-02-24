# Mixed-type comparison operators via Number/Real fallback
# Verifies that ==(x::Number, y::Number) and <(x::Real, y::Real) work

using Test

@testset "Number fallback comparison operators" begin
    # == with mixed types (Number fallback)
    @test Float32(3.0) == 3
    @test 3 == Float32(3.0)
    @test !(Float32(3.0) == 4)

    # < with mixed types (Real fallback)
    @test Float32(1.0) < 2
    @test 1 < Float32(2.0)
    @test !(Float32(3.0) < 2)

    # <= with mixed types (Real fallback)
    @test Float32(1.0) <= 2
    @test Float32(2.0) <= 2
    @test 1 <= Float32(2.0)
    @test !(Float32(3.0) <= 2)
end

true
