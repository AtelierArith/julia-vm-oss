# Issue #5935: a local first assigned a quoted literal (`:(0)` collapses to the
# literal Int 0, `:(0.0)` to the Float 0.0) and then re-assigned a built Expr
# (`:($ex + $i)`) inside a loop used to crash with
#   InternalError: LoadSlotI64: expected numeric in ex, got Expr(...)
# because the slot pass emitted the typed fast-path load (LoadSlotI64/LoadSlotF64)
# unconditionally for a slot whose stored type changes. The load now falls back to
# the generic LoadSlot, matching upstream Julia.
using Test

function acc(n)
    ex = :(0)
    for i in 1:n
        ex = :($ex + $i)
    end
    return ex
end

# Float64-seeded accumulator exercises the LoadSlotF64 fallback path.
function accf(n)
    ex = :(0.0)
    for i in 1:n
        ex = :($ex + $i)
    end
    return ex
end

r1 = acc(1)
r3 = acc(3)
rf = accf(2)

@test r1 == Expr(:call, :+, 0, 1)
@test r3 == Expr(:call, :+, Expr(:call, :+, Expr(:call, :+, 0, 1), 2), 3)
@test rf == Expr(:call, :+, Expr(:call, :+, 0.0, 1), 2)

# The fixture harness checks only the file's final value (== true) and sjulia does
# not abort on a failing bare @test, so make the final value the conjunction of the
# checks. This gates the nextest run on correctness, not merely on "did not crash".
r1 == Expr(:call, :+, 0, 1) &&
    r3 == Expr(:call, :+, Expr(:call, :+, Expr(:call, :+, 0, 1), 2), 3) &&
    rf == Expr(:call, :+, Expr(:call, :+, 0.0, 1), 2)
