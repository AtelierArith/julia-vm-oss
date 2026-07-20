# Issue #10566(b)+(c): a typed-loop block that mixes an array store
# (`a[i] += 1`, an `IndexLoadTypedInbounds` + `IndexStoreTyped` pair on the
# SAME slot -- exercising blocker (b)'s `StoreSlotArray` same-slot elision)
# with a data-dependent BAIL-capable op (`7 % (i - 3)`, zero divisor at
# i == 3 -- `ModI64` bails rather than raising, per Issue #10504) in the SAME
# block. Before blocker (c), `IndexStoreTyped` was classified as an
# irreversible in-place side effect, so the #10504 mixing guard rejected this
# shape outright and it never reached the typed-loop fast path at all.
#
# With (c), the array store lands in a private transactional buffer that is
# simply discarded on `Bail` -- never committed to the array. The interpreter
# then re-runs the WHOLE loop from the header on the generic path, which is
# the source of truth and must NOT observe any already-applied buffered
# writes from the aborted native attempt (that would double-apply `+= 1`).
#
# Verified against upstream `julia`: `a[3] += 1` happens BEFORE `7 % 0`
# raises `DivideError` in the SAME iteration (upstream applies stores that
# precede an error within an iteration too), so the correct final state is
# `[1, 1, 1, 0, 0]` -- each element incremented exactly once, never twice.

function bump_then_maybe_divide_error!(a, n)
    for i in 1:n
        a[i] += 1
        r = 7 % (i - 3)
        r
    end
    return a
end

a = zeros(Int64, 5)
err = nothing
try
    bump_then_maybe_divide_error!(a, 5)
catch e
    global err = e
end
@assert err isa DivideError
@assert a == [1, 1, 1, 0, 0]

# Second call over the same (now partially mutated) array: still no
# double-application on a second bail/re-run cycle.
err2 = nothing
try
    bump_then_maybe_divide_error!(a, 5)
catch e
    global err2 = e
end
@assert err2 isa DivideError
@assert a == [2, 2, 2, 0, 0]

true
