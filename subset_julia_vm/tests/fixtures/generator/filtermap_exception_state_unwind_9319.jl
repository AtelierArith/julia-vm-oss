# Issue #9319: an exception thrown inside a driven generator/HOF callable
# (filtered-generator `FilterMap` predicate or map body, or a plain map body)
# and caught by an ancestor `try`/`catch` must NOT leave a stale broadcast /
# generator driver behind. The VM re-enters such a driver when the callable
# frame *returns*, keyed to that frame's depth; a leaked state was re-triggered
# by a later, unrelated function that happened to return at the same depth —
# silently re-running the stale generator body (observed as a phantom
# "Division by zero" from a previous `__gen_body`, or a duplicated side effect).
# The fix unwinds the driven-callable state stacks to the handler's frame floor
# alongside the frame truncation (see `Vm::unwind_driven_callable_state`).
#
# Regression guard: WITHOUT the fix, scenario A double-runs `noisyA` and
# scenario B crashes with a phantom "Division by zero" inside `divmap` even
# though the caught error was `error("stop")` and `noisyB` performs no division.

using Test

# A: FilterMap MAP-body exception, caught; a later non-specialized call must run
# EXACTLY once (the stale driver must not swallow its return + re-enter the body).
mapdiv(x) = 10 ÷ x
alltrue(x) = true
callsA = Int[]
noisyA(z) = (push!(callsA, z); z + 1)
function scenarioA()
    try
        collect(mapdiv(x) for x in [1, 0] if alltrue(x))
    catch
    end
    return noisyA(41)
end
@testset "filtermap map-body exception, later call runs once (Issue #9319)" begin
    empty!(callsA)
    r = scenarioA()
    @test r == 42
    @test callsA == [41]
end

# B: FilterMap PREDICATE exception, caught; a later call returning a truthy value
# must NOT be misread as that predicate returning truthy (which would re-invoke
# the stale map on the parked `0` element -> phantom division by zero).
divmap(x) = 10 ÷ x
prderr(x) = x != 0 ? true : error("stop")
callsB = Int[]
noisyB(z) = (push!(callsB, z); z + 1)
function scenarioB()
    try
        collect(divmap(x) for x in [1, 0] if prderr(x))
    catch
    end
    return noisyB(7)
end
@testset "filtermap predicate exception, no phantom division (Issue #9319)" begin
    empty!(callsB)
    r = scenarioB()
    @test r == 8
    @test callsB == [7]
end

# C: plain (unfiltered) map / Broadcast body exception, caught; later call once.
# Exercises the same leak on the `HofOpKind::Broadcast` driver, not just FilterMap.
mapdivC(x) = 100 ÷ x
callsC = Int[]
noisyC(z) = (push!(callsC, z); z + 1)
function scenarioC()
    try
        collect(mapdivC(x) for x in [5, 0])
    catch
    end
    return noisyC(9)
end
@testset "plain map body exception, later call runs once (Issue #9319)" begin
    empty!(callsC)
    r = scenarioC()
    @test r == 10
    @test callsC == [9]
end

# D: an exception thrown AND caught INSIDE the driven callable (the map body
# returns from its own catch) must leave the driver consistent so `collect`
# completes normally over every element.
function safe_div(x)
    try
        return 10 ÷ x
    catch
        return -1
    end
end
scenarioD() = collect(safe_div(x) for x in [1, 0, 2])
@testset "nested try inside the driven callable (Issue #9319)" begin
    @test scenarioD() == [10, -1, 5]
end

# E: a `rethrow` inside the map body propagates through the outer `try`; the
# later call still runs exactly once.
callsE = Int[]
noisyE(z) = (push!(callsE, z); z + 1)
function rethrower(x)
    try
        return 10 ÷ x
    catch e
        rethrow(e)
    end
end
function scenarioE()
    try
        collect(rethrower(x) for x in [1, 0])
    catch
    end
    return noisyE(100)
end
@testset "rethrow through outer try, later call runs once (Issue #9319)" begin
    empty!(callsE)
    r = scenarioE()
    @test r == 101
    @test callsE == [100]
end

# F: after a caught filtered-generator exception, a fresh unrelated `collect`
# still produces the correct result (the driver state is fully cleared).
function scenarioF()
    try
        collect(10 ÷ x for x in [2, 0] if true)
    catch
    end
    return collect(x * x for x in 1:4 if x > 1)
end
@testset "fresh collect works after caught exception (Issue #9319)" begin
    @test scenarioF() == [4, 9, 16]
end

true
