using Test

# Issue #10984 regression guard: found via advisor review of the original
# fix (not caught by the initial fixture sweep, which never exercised a
# real-world Base function). Two SIBLING (not nested) `for`/`foreach` loops
# that bind the SAME fresh induction-variable name, where neither has a
# genuinely live outer local of that name, but the FIRST loop's iterable is
# empty (zero runtime iterations).
#
# Root cause: `CoreCompiler::shadow_local_enter`'s original implementation
# used `initialized_locals.contains(name)` alone to decide whether a "live
# outer value" exists to save/restore. After compiling the first loop, the
# compile-time bookkeeping unconditionally marks the induction variable as
# initialized (regardless of whether the loop body ever executes at
# runtime) — so the second sibling loop's `shadow_local_enter` mistook that
# residue for a genuine outer value and emitted a `load_local` to snapshot
# it. When the first loop's iterable was empty, that variable's runtime
# slot was never actually stored to, so the load crashed with
# `UndefVarError: waiter not defined` — reproduced live in
# `subset_julia_vm/src/julia/base/channels.jl`'s
# `_wake_all_channel_waiters` (fixture `concurrency/channel_basic.jl`,
# `Channel close` testset: `close(ch)` on a channel with no parked tasks
# calls `_wake_all_channel_waiters`, whose two `waiters` tuple slots are
# both empty).
#
# Fix: `shadow_local_enter`/`shadow_local_exit` now snapshot and restore
# ALL FIVE compile-time bookkeeping structures (`initialized_locals`,
# `locals`, `julia_type_locals`, `known_any_rank_array_locals`,
# `mixed_type_vars`) symmetrically to their pre-enter state, emitting
# runtime save/restore bytecode only when a genuine live value exists.
# When there is no genuine outer value, the maps are RESTORED (removed)
# to "not a local" on exit, so a sibling construct reusing the same fresh
# name never observes a phantom collision.
#
# Verified against `julia --startup-file=no` (1.12.6): all three calls
# return the expected woken lists with no error.
mutable struct WaiterHolder
    waiters::Any
end

function wake_all(c::WaiterHolder)
    woken = String[]
    for waiter in c.waiters[1]
        push!(woken, waiter)
    end
    for waiter in c.waiters[2]
        push!(woken, waiter)
    end
    c.waiters = (String[], String[])
    return woken
end

# Both sibling loops empty (the exact `channels.jl` crash shape).
c1 = WaiterHolder((String[], String[]))
@test wake_all(c1) == String[]

# First loop empty, second loop non-empty.
c2 = WaiterHolder((String[], ["a", "b"]))
@test wake_all(c2) == ["a", "b"]

# First loop non-empty, second loop empty.
c3 = WaiterHolder((["x"], String[]))
@test wake_all(c3) == ["x"]

true
