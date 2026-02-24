# Verify that same-type operations do not infinitely recurse
# The promote(::T, ::T) fast path and concrete-type operators prevent this

using Test

@testset "No recursion for same-type operations" begin
    # Int64 + Int64 uses intrinsic add_int (not Number fallback)
    @test 1 + 2 == 3
    @test 10 - 3 == 7
    @test 4 * 5 == 20
    @test 6 / 2 == 3.0

    # Float64 + Float64 uses intrinsic add_float (not Number fallback)
    @test 1.5 + 2.5 == 4.0
    @test 3.5 - 1.0 == 2.5
    @test 2.0 * 3.0 == 6.0
    @test 6.0 / 2.0 == 3.0

    # Float32 + Float32 uses Float32-specific method (not Number fallback)
    @test Float32(1.5) + Float32(2.5) == Float32(4.0)
    @test Float32(3.5) - Float32(1.0) == Float32(2.5)
    @test Float32(2.0) * Float32(3.0) == Float32(6.0)
    @test Float32(6.0) / Float32(2.0) == Float32(3.0)

    # Int64 comparisons
    @test 1 == 1
    @test 1 != 2
    @test 1 < 2
    @test 2 > 1
    @test 1 <= 1
    @test 2 >= 1
end

true
