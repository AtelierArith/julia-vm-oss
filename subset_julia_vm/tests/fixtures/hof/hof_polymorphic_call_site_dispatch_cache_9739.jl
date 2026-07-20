# Issue #9739: the VM's per-call-site dynamic dispatch cache (previously
# CallDynamic-only, extended here to CallFunctionVariable — the instruction
# behind `map`/`filter`/broadcast callback application) keys each call site
# on `[callee, args...]`, not just `args...`. A shared bytecode call site
# (Pure Julia's single `f(args[1])`/`f(args[1], args[2])` inside `map`'s and
# `_broadcast_apply`'s body) is reused across every distinct HOF callback in
# the whole program, so caching must not let one callback's cached resolution
# leak into another callback reusing the same call site.
#
# Verified against upstream Julia (julia --startup-file=no): prints
# true five times.

using Test

double9739(x) = x * 2
triple9739(x) = x * 3

@testset "polymorphic call-site dispatch cache (Issue #9739)" begin
    # Warm the shared `map` call site with `double9739`, then switch to
    # `triple9739`, then switch back — must never return a stale target.
    @test map(double9739, [1, 2, 3]) == [2, 4, 6]
    @test map(triple9739, [1, 2, 3]) == [3, 6, 9]
    @test map(double9739, [1, 2, 3]) == [2, 4, 6]

    # Same call site, alternating callee AND argument type on each element —
    # both axes of the cache key must be respected simultaneously.
    mixed9739 = Any[1, 2.0, 3, 4.0]
    h9739(x) = x isa Int ? x + 1 : x - 1.0
    @test map(h9739, mixed9739) == Any[2, 1.0, 4, 3.0]

    # Broadcast form funnels through the same `_broadcast_apply` call site as
    # `map`/`filter` — alternate callees there too.
    @test double9739.([1, 2, 3]) == [2, 4, 6]
    @test triple9739.([1, 2, 3]) == [3, 6, 9]
    @test double9739.([1, 2, 3]) == [2, 4, 6]
end

true
