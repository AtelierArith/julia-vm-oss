# Issue #3504: MustAlias-style refinements must be invalidated when the
# underlying field/element/variable is mutated. Otherwise inference would
# happily reuse a stale `obj.field => Int64` refinement past a
# `obj.field = nothing` write.
#
# Runtime correctness is independent of inference soundness, but exercising
# every code path (assign-after-narrow, field-assign-after-narrow, index-
# assign-after-narrow, var-rebind-after-narrow) ensures the inference engine
# at least *runs* the new invalidation hooks. Soundness of the resulting
# refinement table is covered by unit tests in
# `subset_julia_vm/src/compile/abstract_interp/env.rs::tests`.

mutable struct Box
    val::Union{Int64, Nothing}
    other::Union{String, Nothing}
end

# (1) FieldAssign drops only the named-field refinement; sibling fields
# of the same object remain refined.
function field_overwrite(b::Box)
    if b.val !== nothing && b.other !== nothing
        # b.val is refined to Int64, b.other to String here.
        x = b.val + 1            # uses the Int64 refinement
        y = b.other * "!"        # uses the String refinement
        b.val = nothing          # invalidates b.val refinement only
        # b.other refinement still holds — we can keep using it.
        z = b.other * "?"
        return x + length(y) + length(z)
    end
    return -1
end

@assert field_overwrite(Box(10, "hi")) == 11 + 3 + 3
@assert field_overwrite(Box(nothing, "hi")) == -1

# (2) Reassigning the root variable drops every path refinement under it.
# After `b = Box(...)`, sjulia must NOT keep believing `b.val :: Int64`
# from the original guard.
function var_rebind(b::Box)
    if b.val isa Int64
        a = b.val + 1
        b = Box(nothing, nothing)   # rebind: every b.* refinement is gone
        # Re-test on the new b: the old refinement would have made this
        # arithmetic dispatch to Int64+Int64 incorrectly.
        return a + (b.val === nothing ? 100 : 0)
    end
    return 0
end

@assert var_rebind(Box(7, nothing)) == 8 + 100

# (3) IndexAssign with a constant index runs the new infer_stmt arm.
# Even with a homogeneous Vector{Int64} (where the refinement was trivial),
# this exercises the precise-key invalidation path.
function index_overwrite(xs::Vector{Int64})
    if xs[1] isa Int64
        a = xs[1] + 1
        xs[1] = 99            # constant index ⇒ only xs[1] refinement dropped
        b = xs[1] + 2         # value-type still Int64 from declared element type
        return a + b
    end
    return -1
end

@assert index_overwrite([5, 6]) == 6 + 101

# (4) IndexAssign with a non-constant index runs the conservative drop-all
# path on the same array.
function index_overwrite_dynamic(xs::Vector{Int64}, k::Int64)
    if xs[1] isa Int64
        a = xs[1] + 1
        xs[k] = 0             # non-constant index ⇒ all xs[*] refinements gone
        b = xs[1] + 0
        return a + b
    end
    return -1
end

@assert index_overwrite_dynamic([5, 6], 1) == 6 + 0
@assert index_overwrite_dynamic([5, 6], 2) == 6 + 5

true
