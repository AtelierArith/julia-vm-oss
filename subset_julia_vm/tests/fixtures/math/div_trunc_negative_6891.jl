using Test

# Issue #6891: the generic fallback `div(x, y) = floor(x / y)` rounded toward
# -Inf, but Julia's `div` rounds toward zero (RoundToZero). Results diverged for
# operands of opposite sign on Float64 / BigFloat (the Int path was already
# correct via the typed sdiv intrinsic). The fix uses `trunc(x / y)`.
#
# Issue #6895: `divrem(x, y) = (div(x, y), rem(x, y))`, and the dynamic Float `%`
# fallback computed mod (floor) instead of the truncated remainder, so the rem
# component was wrong too. Both fixes are needed for divrem(-7.0, 3.0).
#
# Verified vs julia 1.12.6. NOTE: the trailing `ok` (AND of every check) is the
# real regression gate -- the harness only inspects the final value, so plain
# @test (which the harness does not read) would mask these wrong-value, non-
# throwing regressions.

@testset "div rounds toward zero, Float64 (Issue #6891)" begin
    @test div(-7.0, 3.0) == -2.0
    @test div(7.0, -3.0) == -2.0
    @test div(7.0, 3.0) == 2.0
    @test div(-7.0, -3.0) == 2.0
    @test typeof(div(-7.0, 3.0)) === Float64
end

@testset "div rounds toward zero, BigFloat (Issue #6891)" begin
    @test div(big(-7.0), big(3.0)) == -2
    @test div(big(7.0), big(-3.0)) == -2
    @test div(big(7.0), big(3.0)) == 2
    @test typeof(div(big(-7.0), big(3.0))) === BigFloat
end

@testset "divrem: trunc quotient + truncated rem (Issue #6891/#6895)" begin
    drf = divrem(-7.0, 3.0)
    @test drf[1] == -2.0
    @test drf[2] == -1.0
    drb = divrem(big(-7.0), big(3.0))
    @test drb[1] == -2
    @test drb[2] == -1
end

@testset "fld/cld stay floor/ceil; Int div unchanged" begin
    @test fld(-7.0, 3.0) == -3.0
    @test cld(-7.0, 3.0) == -2.0
    @test fld(big(-7.0), big(3.0)) == -3
    @test cld(big(-7.0), big(3.0)) == -2
    @test div(-7, 3) == -2
    @test div(7, -3) == -2
end

# --- Regression gate (not masked): final value is the AND of every check. ---
ok = true
ok = ok && (div(-7.0, 3.0) == -2.0)
ok = ok && (div(7.0, -3.0) == -2.0)
ok = ok && (div(7.0, 3.0) == 2.0)
ok = ok && (div(-7.0, -3.0) == 2.0)
ok = ok && (typeof(div(-7.0, 3.0)) === Float64)

ok = ok && (div(big(-7.0), big(3.0)) == -2)
ok = ok && (div(big(7.0), big(-3.0)) == -2)
ok = ok && (div(big(7.0), big(3.0)) == 2)
ok = ok && (typeof(div(big(-7.0), big(3.0))) === BigFloat)

drf = divrem(-7.0, 3.0)
ok = ok && (drf[1] == -2.0)
ok = ok && (drf[2] == -1.0)
drb = divrem(big(-7.0), big(3.0))
ok = ok && (drb[1] == -2)
ok = ok && (drb[2] == -1)

ok = ok && (fld(-7.0, 3.0) == -3.0)
ok = ok && (cld(-7.0, 3.0) == -2.0)
ok = ok && (fld(big(-7.0), big(3.0)) == -3)
ok = ok && (cld(big(-7.0), big(3.0)) == -2)
ok = ok && (div(-7, 3) == -2)
ok = ok && (div(7, -3) == -2)

ok
