# Issue #10871 / #10786: a `DispatchFirst` Base function (isbitstype) with a
# Rust builtin fallback must keep that fallback reachable at a *dynamic* call
# site (an unannotated parameter passing a type object through), not only at
# static call sites or call sites that go through `typeof(x)` explicitly.
#
# Root cause: `compile_generic_dispatch_call`'s single-Any-arg fallback arm
# emitted a plain `CallDynamic` with only the user's method as candidate, so a
# dispatch miss (an unrelated Int64 type object) raised MethodError instead of
# falling back to `BuiltinOp::Isbitstype`.
using Test

struct IsbitstypeDynamicCallsiteBox10871
    n::Int64
end
Base.isbitstype(::Type{IsbitstypeDynamicCallsiteBox10871}) = false

# Static call site: the type object is a compile-time-known literal.
g_static() = Base.isbitstype(Int64)

# Dynamic call site: the argument is an unannotated parameter (statically
# `Any`), so the runtime value is only known to be a type object at runtime.
g_dynamic(T) = Base.isbitstype(T)

# Dynamic call site via typeof(x): the argument statically infers as
# `DataType`, a different code path from the bare-parameter case above.
h(x) = Base.isbitstype(typeof(x))

@test g_static()
@test g_dynamic(Int64)
@test h(1)

# The user's own type must still correctly select the user override (not
# loosely match, and not silently take the builtin) through the same dynamic
# call site shape.
@test !g_dynamic(IsbitstypeDynamicCallsiteBox10871)
@test !h(IsbitstypeDynamicCallsiteBox10871(7))

# `isbitstype(t)` takes an unconstrained `t` upstream (runtime_internals.jl)
# and returns `false` for a non-Type argument rather than raising
# MethodError. The builtin fallback must preserve this leniency at the
# dynamic call site too (not just the pre-fix "no candidate matched -> raise
# MethodError" behavior, which happened to look upstream-correct for this
# one input by accident).
@test !g_dynamic(1)

# isbits/ismutable have no Rust builtin fallback (pure-Julia catch-all,
# Issue #6738: `isbits(x) = isbitstype(typeof(x))`) — confirm the same
# dynamic-call-site shape still reaches that catch-all correctly and is
# unaffected by the isbitstype fallback fix above.
isbits_dynamic(x) = Base.isbits(x)
ismutable_dynamic(x) = Base.ismutable(x)

@test isbits_dynamic(1)
@test !ismutable_dynamic(1)

true
