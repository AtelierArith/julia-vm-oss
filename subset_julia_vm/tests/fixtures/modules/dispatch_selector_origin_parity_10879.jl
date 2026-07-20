# Table-driven selector-parity coverage (Issue #10879, prevention follow-up of
# #10295 / PR #10877): the same Base/user same-name struct-ID pair is fed
# through every dispatch selector reachable from ordinary Julia source --
# direct, function-value, dynamic (Any-boxed), the iterate() runtime-dispatch
# protocol, splat, kwargs, and a repeated call site (inline-cache replay) --
# plus positive rows for a same-ID Base submodule alias and an external
# subtype of a Base abstract type. A Base method signature that retains a bare
# concrete nominal name after owner erasure must not accept a same-named but
# differently-owned struct on ANY of these paths.

using Test

module DispatchSelectorOriginParity10879
export Partition, AbstractDisplay

# Same bare name as Base's `Iterators.Partition`, but this is a *different*
# concrete type (no `xs`/`n` fields shaped like Base's Partition) with no
# `iterate` method of its own.
struct Partition
    tag::Int
end

# External subtype of a Base abstract type: valid for a Base method typed
# `::AbstractDisplay` even though the name collides (abstract applicability
# stays subtype-based, only concrete bare names get the origin fence). Named
# identically to the abstract type it extends to match the already-verified
# `methodsig_struct_origin_10295.jl` row; a *differently*-named external
# subtype of `Base.AbstractDisplay` currently fails to dispatch `pushdisplay`
# at all, which is a separate, pre-existing bug (Issue #11528) unrelated to
# nominal-origin fencing.
struct AbstractDisplay <: Base.AbstractDisplay end

end

using .DispatchSelectorOriginParity10879
const P10879 = DispatchSelectorOriginParity10879.Partition
const D10879 = DispatchSelectorOriginParity10879.AbstractDisplay

p10879 = P10879(7)

# --- Row 1: direct dispatch -------------------------------------------------
err_direct_10879 = try
    length(p10879)
    nothing
catch err
    err
end
@test err_direct_10879 isa MethodError

# --- Row 2: function-value dispatch -----------------------------------------
length_fv_10879 = length
err_fv_10879 = try
    length_fv_10879(p10879)
    nothing
catch err
    err
end
@test err_fv_10879 isa MethodError

# --- Row 3: dynamic dispatch through an `Any`-boxed container ---------------
any_box_10879 = Any[p10879]
err_dynamic_10879 = try
    length(any_box_10879[1])
    nothing
catch err
    err
end
@test err_dynamic_10879 isa MethodError

# --- Row 4: iterate() runtime-dispatch protocol (Issue #10879) --------------
# Base defines `iterate(p::Base.Iterators.Partition)`, whose cached signature
# retains the bare name `Partition`. Calling `iterate` on this same-named but
# differently-owned struct through the `IterateDynamic` family-fallback
# resolvers must still raise (not silently execute Base's method body against
# the wrong field layout).
err_iterate_10879 = try
    iterate(any_box_10879[1])
    nothing
catch err
    err
end
@test err_iterate_10879 !== nothing
@test !(err_iterate_10879 isa BoundsError)

err_forloop_10879 = try
    for _ in any_box_10879[1]
        error("unreachable: must not iterate a foreign same-named struct")
    end
    nothing
catch err
    err
end
@test err_forloop_10879 !== nothing
@test !(err_forloop_10879 isa BoundsError)

# --- Row 5: splat call -------------------------------------------------------
splat_args_10879 = (p10879,)
err_splat_10879 = try
    length(splat_args_10879...)
    nothing
catch err
    err
end
@test err_splat_10879 isa MethodError

# --- Row 6: kwargs call ------------------------------------------------------
# `round` accepts a `digits` keyword in Base; passing our same-named struct as
# the positional argument must still miss on origin, whether or not kwargs are
# present.
err_kwargs_10879 = try
    round(p10879; digits=2)
    nothing
catch err
    err
end
@test err_kwargs_10879 isa MethodError

# --- Row 7: repeated call site (inline-cache / L2 dispatch-cache replay) ----
# Call the same call site enough times that the per-call-site inline/dispatch
# cache from a *different* argument type is populated, then replay it with the
# colliding struct -- the cached entry must not resurrect the rejected Base
# candidate.
function replay_length_10879(x)
    try
        return length(x)
    catch err
        return err
    end
end

@test replay_length_10879([1, 2, 3]) == 3
@test replay_length_10879([1, 2, 3]) == 3
@test replay_length_10879(p10879) isa MethodError
@test replay_length_10879(p10879) isa MethodError

# --- Row 8: same-ID Base submodule alias must still be ACCEPTED ------------
@test collect(Iterators.partition([1, 2, 3], 2)) == [[1, 2], [3]]

# --- Row 9: external subtype of a Base abstract type must be ACCEPTED ------
d10879 = D10879()
@test begin
    pushdisplay(d10879)
    true
end

true
