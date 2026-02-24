# Test broadcast_shape / _bcs / _bcs1 shape computation (Issue #2535)
# Verifies that broadcast shape calculation matches Julia semantics.
# All assertions verified against official Julia.

using Test

@testset "broadcast_shape computation" begin
    # Single shape: identity
    @test broadcast_shape((3,)) == (3,)
    @test broadcast_shape((2, 3)) == (2, 3)

    # Two identical shapes
    @test broadcast_shape((3,), (3,)) == (3,)
    @test broadcast_shape((2, 3), (2, 3)) == (2, 3)

    # Scalar broadcast (dimension 1 stretches)
    @test broadcast_shape((1,), (5,)) == (5,)
    @test broadcast_shape((5,), (1,)) == (5,)

    # Scalar-like shapes
    @test broadcast_shape((1,), (1,)) == (1,)

    # Different length tuples (shorter is padded with 1 on the right)
    # (1,1) with (1,) -> _bcs1(1,1)=1, remaining (1,) -> (1,1)
    @test broadcast_shape((1, 1), (1,)) == (1, 1)

    # 2D broadcast with singleton dimensions
    @test broadcast_shape((1, 3), (2, 1)) == (2, 3)
    @test broadcast_shape((2, 1), (1, 3)) == (2, 3)

    # 3D shapes
    @test broadcast_shape((2, 3, 4), (2, 3, 4)) == (2, 3, 4)
    @test broadcast_shape((1, 3, 1), (2, 1, 4)) == (2, 3, 4)

    # Incompatible shapes should throw DimensionMismatch
    threw = false
    try
        broadcast_shape((3,), (2, 3))
    catch e
        threw = e isa DimensionMismatch
    end
    @test threw
end

true
