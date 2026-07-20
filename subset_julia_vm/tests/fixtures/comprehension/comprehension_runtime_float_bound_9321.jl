# Integer comprehension / for-head range whose BOUND infers `Any` but arrives as
# a `Float` at runtime (Issue #9321).
#
# `[i for i in 1:n]` and `for i in 1:n` keep the `Int64` element/counter fast
# path when the bound `n` infers `Any` (deliberate scope choice from #9291/#9293:
# an `Any` bound usually IS an integer, and diverting would deoptimize the common
# integer comprehension). Before the fix a runtime `Float` bound (`n = 5.5`) built
# a `Float` range whose elements crashed the `Int64` store with
# `Type error: expected I64, got Float64` (comprehension), while the for-head
# fast path truncated the bound toward zero (`DynamicToI64`) and so mis-counted a
# negative float bound (`-3:-1.5` ran three times, not upstream's two).
#
# The fast path now coerces the runtime bound to `Int` with upstream range
# last-element semantics (`floor` for an ascending step, `ceil` for a descending
# one — `CoerceRangeStopI64`), so the LENGTH / iteration count matches upstream
# exactly. The element TYPE stays `Int64` on this fast path (upstream would give
# `Float64`); only counts/lengths are asserted here since those achieve parity.

using Test

@testset "Comprehension/for-head runtime Float bound (Issue #9321)" begin
    # --- Comprehension, unit step, integer start, Any (runtime Float) bound ---
    h(n) = length([i for i in 1:n])
    @test h(5.5) == 5   # the crashing MWE: last integer element is 5
    @test h(0.5) == 0   # empty range
    @test h(5.0) == 5
    @test h(-1.0) == 0  # empty range
    @test h(1.0) == 1
    @test h(0.9) == 0
    @test h(5.9) == 5
    # integer bound still works (fast path unchanged)
    @test h(5) == 5
    @test h(0) == 0

    # --- Comprehension, explicit POSITIVE step, Any bound ---
    hp(n) = length([i for i in 1:2:n])
    @test hp(5.5) == 3  # 1, 3, 5
    @test hp(6.0) == 3
    @test hp(7.0) == 4  # 1, 3, 5, 7

    # --- Comprehension, explicit NEGATIVE step, integer start, Any bound ---
    # descending: last element is the smallest >= ceil(stop)
    hn(n) = length([i for i in 10:-2:n])
    @test hn(2.5) == 4  # 10, 8, 6, 4
    @test hn(1.5) == 5  # 10, 8, 6, 4, 2
    @test hn(2.0) == 5

    # --- Comprehension, negative unit-step bound, integer (literal) start ---
    hu(n) = length([i for i in -3:n])
    @test hu(-1.5) == 2  # -3, -2   (floor(-1.5) == -2, NOT trunc -1)
    @test hu(-1.9) == 2
    @test hu(-3.0) == 1

    # --- for-head, unit step, Any bound ---
    g(n) = (s = 0; for i in 1:n; s += 1; end; s)
    @test g(5.5) == 5
    @test g(0.5) == 0
    @test g(-1.0) == 0
    @test g(5) == 5

    # --- for-head, negative unit-step bound (the truncation bug) ---
    gn(n) = (s = 0; for i in -3:n; s += 1; end; s)
    @test gn(-1.5) == 2  # was 3 before the fix (trunc toward zero)
    @test gn(-1.9) == 2

    # --- for-head, constant NEGATIVE step, Any bound ---
    gs(n) = (s = 0; for i in 10:-2:n; s += 1; end; s)
    @test gs(2.5) == 4
    @test gs(1.5) == 5

    # --- for-head, RUNTIME variable step (dynamic-step path), Any bound ---
    gv(k, n) = (s = 0; for i in 1:k:n; s += 1; end; s)
    @test gv(2, 5.5) == 3   # 1, 3, 5
    @test gv(-2, 2.5) == 0  # descending from 1 with stop 2.5 is empty

    # --- Control: a STATIC float bound still diverts to the Float element path ---
    # (`1:5.5` where the bound is a float literal keeps Float64 elements/length)
    @test length([i for i in 1:5.5]) == 5
    @test eltype([i for i in 1:5.5]) == Float64
end

# The same fast-path bound coercion applies to the multi-clause comprehension
# arms (Issue #9377 tracks the shared VM helper). Before the fix these two forms
# crashed with `LoadAddI64Slot: expected integer` because the `Float` range fed a
# `Float` loop variable into the slotized `Int64` body arithmetic. As with the
# single-var arm, only counts/lengths/shapes reach parity — element values match
# numerically (`2 == 2.0`) while the element TYPE stays `Int64`.
@testset "Cartesian / flatten runtime Float bound (Issue #9321)" begin
    # --- Cartesian comma form `[expr for i in R1, j in R2]` ---
    mc(n) = length([i + j for i in 1:n, j in 1:2])
    @test mc(5.5) == 10   # the crashing MWE: i:1..5 (5) * j:2
    @test mc(5) == 10     # integer bound unchanged
    @test mc(0.5) == 0    # empty outer range
    # positive explicit step in the first clause
    mc2(n) = length([i for i in 1:2:n, j in 1:3])
    @test mc2(5.5) == 9   # i:1,3,5 (3) * j:3
    @test mc2(6.0) == 9
    # negative explicit step in the first clause
    mcn(n) = length([i + j for i in 10:-2:n, j in 1:2])
    @test mcn(2.5) == 8   # i:10,8,6,4 (4) * 2
    # Float bound in the SECOND clause
    mc_second(n) = length([i + j for i in 1:2, j in 1:n])
    @test mc_second(3.5) == 6
    # 2-D shape and element values (Int-typed, but numerically equal to upstream)
    mcs(n) = size([i + j for i in 1:n, j in 1:2])
    @test mcs(5.5) == (5, 2)
    mcv(n) = [i + j for i in 1:n, j in 1:2]
    @test mcv(5.5) == [2 3; 3 4; 4 5; 5 6; 6 7]

    # --- Flatten whitespace form `[expr for i in R1 for j in R2]` ---
    fl(n) = length([i + j for i in 1:n for j in 1:2])
    @test fl(5.5) == 10
    @test fl(5) == 10
    @test fl(0.5) == 0
    # dependent inner range re-evaluated per outer value (outer bound coerced)
    fld(n) = length([i + j for i in 1:n for j in 1:i])
    @test fld(3.5) == 6   # i:1,2,3 -> j counts 1+2+3
    # Float bound in the INNER clause
    fli(n) = length([i + j for i in 1:2 for j in 1:n])
    @test fli(3.5) == 6   # i:2 * j:1..3 (3)
    # negative explicit step in the outer clause
    fln(n) = length([i for i in 10:-2:n for j in 1:2])
    @test fln(2.5) == 8   # i:10,8,6,4 (4) * 2
    # flat 1-D element values (Int-typed, numerically equal to upstream)
    flv(n) = [i + j for i in 1:n for j in 1:2]
    @test flv(5.5) == [2, 3, 3, 4, 4, 5, 5, 6, 6, 7]
end

true
