using Test

# Issue #10092: a standalone WeakRef whose target is only reachable through the
# weak cell must be cleared by GC.gc() under BOTH cache modes. With the Base
# bytecode cache enabled, the struct table rebuilt from cached struct_defs lost
# the inner-constructor flag, so `WeakRef(tmp)` compiled as a raw field-count
# default construction instead of dispatching to the outer constructor
# `WeakRef(x) = _weakref_new(x)` — the weak cell was never registered with the
# GC and the target survived collection.

mutable struct RefWeakBox10092
    x::Int
end

function make_weak_ref_10092()
    tmp = RefWeakBox10092(7)
    return WeakRef(tmp)
end

wr = make_weak_ref_10092()
@test typeof(wr) === WeakRef
GC.gc()
GC.gc()
@test wr.value === nothing

# A WeakRef target that is still strongly rooted must NOT be cleared.
rooted = RefWeakBox10092(3)
wr2 = WeakRef(rooted)
GC.gc()
@test wr2.value === rooted
@test wr2.value.x == 3

true
