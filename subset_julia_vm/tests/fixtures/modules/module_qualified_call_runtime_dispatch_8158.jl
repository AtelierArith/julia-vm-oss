using Test

# Issue #8158: a module-qualified call `Module.f(x)` must defer to runtime
# multiple dispatch exactly like the unqualified `f(x)` call. Previously, when
# the argument's type was statically `Any` (e.g. a function parameter or a
# keyword argument with a default) and the callee had a catch-all `f(::Any)`
# method, the qualified path statically bound the catch-all instead of
# dispatching on the runtime value — so a more-specific method was silently
# skipped. (The real-world failure: `SciMLBase._callbacks(cb::CallbackSet)`
# mis-dispatched to the `_callbacks(cb) = (cb,)` catch-all, silently disabling
# every callback inside a `CallbackSet`.) The unqualified / imported call already
# dispatched correctly; only the qualified `Module.f` form was wrong.
module Dispatch8158
struct Tagged
    items
end

# Specific method + catch-all (the exact shape that mis-dispatched).
unpack(t::Tagged) = t.items
unpack(x) = (x,)

# Multi-argument analogue (catch-all is all-Any; a more-specific method exists).
combine(t::Tagged, n) = length(t.items) + n
combine(x, n) = n
end

using .Dispatch8158: Tagged

t = Tagged((10, 20, 30))

# `x` is an untyped parameter → statically Any at the qualified call site.
via_positional(x) = Dispatch8158.unpack(x)

# `cb` flows through a keyword argument with a default → also statically Any.
function via_kwarg(; cb=nothing)
    return Dispatch8158.unpack(cb)
end

# Multi-arg qualified call with an Any first argument.
via_multi(x, n) = Dispatch8158.combine(x, n)

ok =
    # qualified call dispatches to the SPECIFIC method on the runtime value
    via_positional(t) === (10, 20, 30) &&
    via_kwarg(cb=t) === (10, 20, 30) &&
    via_multi(t, 5) == 8 &&
    # the catch-all still wins for a value that is NOT a Tagged
    via_positional(99) === (99,) &&
    via_multi(99, 5) == 5 &&
    # a direct qualified call on a precisely-typed value is unchanged
    Dispatch8158.unpack(t) === (10, 20, 30)

@test ok
println(ok)
ok
