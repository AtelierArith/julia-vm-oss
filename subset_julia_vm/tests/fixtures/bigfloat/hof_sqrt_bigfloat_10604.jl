# Issue #10604: sqrt passed as a FUNCTION VALUE (map/HOF lane) lacked the
# BigFloat arm that the direct SqrtF64 builtin already had, so
# map(sqrt, [big"2.0"]) raised MethodError while sqrt(big"2.0") worked.
# The callable-lane intrinsic fallback now mirrors the builtin's BigFloat
# handling (full precision, DomainError on negative input).
# NOTE: the dot-broadcast lane still narrows BigFloat results — Issue #11727.

using Test

@testset "map/HOF sqrt keeps BigFloat (Issue #10604)" begin
    r = map(sqrt, [big"2.0"])
    @test eltype(r) === BigFloat
    @test abs(r[1] - sqrt(big"2.0")) < big"1e-70"

    # Full precision, not a Float64 round-trip.
    @test map(sqrt, [big"2.0"])[1] == sqrt(big"2.0")

    # Negative input keeps upstream's DomainError through the HOF lane.
    @test_throws DomainError map(sqrt, [big"-1.0"])

    # Non-numeric input still misses dispatch like upstream.
    @test_throws MethodError map(sqrt, ["a"])
end

true
