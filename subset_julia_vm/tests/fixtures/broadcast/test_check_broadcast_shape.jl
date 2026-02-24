# Test check_broadcast_shape validation (Issue #2536)
# Verifies shape compatibility checking for broadcast operations.

using Test

@testset "check_broadcast_shape validation" begin
    # Compatible shapes should not throw
    check_broadcast_shape((3,), (3,))
    check_broadcast_shape((3,), (1,))
    check_broadcast_shape((2, 3), (2, 3))
    check_broadcast_shape((2, 3), (1, 3))
    check_broadcast_shape((2, 3), (2, 1))

    # Empty array shape is always compatible
    check_broadcast_shape((3,))

    # Incompatible shape should throw DimensionMismatch
    threw1 = false
    try
        check_broadcast_shape((3,), (2,))
    catch e
        threw1 = e isa DimensionMismatch
    end
    @test threw1

    # Extra non-singleton dimensions should throw
    threw2 = false
    try
        check_broadcast_shape((), (2,))
    catch e
        threw2 = e isa DimensionMismatch
    end
    @test threw2

    # Singleton extra dimensions are OK
    check_broadcast_shape((), (1,))

    @test true  # All checks passed
end

true
