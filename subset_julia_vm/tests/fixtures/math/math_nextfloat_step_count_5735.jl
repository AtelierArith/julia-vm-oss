using Test

# Issue #5735: the two-argument nextfloat(x, n) / prevfloat(x, n) must step `n`
# ULPs. They previously ignored `n` and returned a single step. Julia defines
# nextfloat(x, n) as `n` iterative applications of nextfloat (or -n of prevfloat).

@testset "nextfloat/prevfloat step count (Issue #5735)" begin
    # Multi-step matches upstream.
    @test nextfloat(1.0, 2) == 1.0000000000000004
    @test prevfloat(1.0, 3) == 0.9999999999999997
    @test prevfloat(2.0, 5) == 1.999999999999999

    # n == 0 is the identity.
    @test nextfloat(1.0, 0) === 1.0
    @test prevfloat(1.0, 0) === 1.0

    # Negative n reverses direction: nextfloat(x, -n) == prevfloat(x, n).
    @test nextfloat(1.0, -1) == prevfloat(1.0)
    @test prevfloat(1.0, -2) == nextfloat(1.0, 2)

    # n steps of nextfloat then n of prevfloat round-trips.
    @test prevfloat(nextfloat(3.14, 4), 4) == 3.14

    # Width preserved for the 2-arg form (Issue #5690 interaction).
    @test nextfloat(1.0f0, 2) isa Float32
    @test nextfloat(1.0f0, 2) == 1.0000002f0
    @test nextfloat(Float16(1.0), 2) isa Float16
    @test nextfloat(Float16(1.0), 2) == Float16(1.002)

    # Subnormal step from zero.
    @test nextfloat(0.0, 1) === 5.0e-324

    # The 1-arg form still works (regression guard).
    @test nextfloat(1.0) == 1.0000000000000002
    @test prevfloat(1.0) == 0.9999999999999999

    # Integer x is a MethodError for both arities.
    @test_throws MethodError nextfloat(1, 2)
    @test_throws MethodError prevfloat(1, 2)
end

true
