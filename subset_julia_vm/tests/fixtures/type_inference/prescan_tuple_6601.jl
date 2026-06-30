# Issue #6601: TupleLiteral Assign-RHS slots typed through the shared engine.
# Any tuple literal is a `Tuple` regardless of element types (upstream Julia:
# typeof((1, "x", [])) == Tuple{...}), so a tuple slot with a non-concrete
# element must still type as Tuple, not collapse to Any.
function mixed_tuple(x)
    t = (1, x, "s")
    return t[1] + length(t[3])
end
function concrete_tuple()
    t = (1, 2.0)
    return t[1] + t[2]
end
function empty_tuple()
    t = ()
    return length(t)
end
@assert mixed_tuple(42) == 2
@assert mixed_tuple("ab") == 2
@assert concrete_tuple() === 3.0
@assert typeof(concrete_tuple()) === Float64
@assert empty_tuple() === 0
true
