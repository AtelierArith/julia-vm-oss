# getfield(x, i) out of range on a Rust-backed composite carrier (Expr,
# Base.Generator, ...) raises a BoundsError, but before this fix it carried
# the wrong payload on both axes: the reported object was always `nothing`
# instead of the actual receiver, and the reported index was one less than
# the index the caller passed (the internal 0-based field_idx leaked into the
# user-visible message instead of the original 1-based index). Fixed at the
# shared VmError::FieldIndexOutOfBounds -> BoundsError conversion so every
# carrier that raises through it benefits (Issue #11509).
using Test

mutable struct RegressionRcvr11580
    x::Int
    y::Int
end

@testset "getfield BoundsError on Expr carries receiver + 1-based index (Issue #11509)" begin
    e = Expr(:call, :f, 1, 2)
    err = try
        getfield(e, 3)
    catch caught
        caught
    end
    @test err isa BoundsError
    @test err.a === e
    @test err.i == 3
    @test occursin("attempt to access Expr at index [3]", sprint(showerror, err))

    outer = Expr(:call, :outer, 1, 2)
    nested = try
        getfield(outer, 3)
    catch
        inner = Expr(:call, :inner, 3, 4)
        try
            getfield(inner, 3)
        catch
        end
        try
            rethrow()
            nothing
        catch rethrown
            rethrown
        end
    end
    @test nested isa BoundsError
    @test nested.a === outer
    @test nested.i == 3
end

@testset "getfield BoundsError on Base.Generator carries receiver + 1-based index (Issue #11509)" begin
    g = (x^2 for x in 1:3)
    err = try
        getfield(g, 3)
    catch caught
        caught
    end
    @test err isa BoundsError
    # `===` identity on Base.Generator has its own tracked gap (Issue
    # #11570, found while writing this fixture) so the receiver is checked
    # structurally here instead of with `err.a === g`.
    @test err.a isa Base.Generator
    @test collect(err.a) == collect(g)
    @test err.i == 3
end

@testset "successful getfield does not leave a stale receiver for a later unrelated FieldIndexOutOfBounds (regression)" begin
    # This reproduces a cross-contamination regression that PR #11580's fix
    # for #11509 introduced: it parked the receiver in
    # `Vm::pending_field_index_error_receiver` unconditionally before every
    # field lookup -- including one that succeeds -- instead of only at the
    # moment of the raise. A stale receiver from an earlier *successful*
    # getfield could then attach to a later, unrelated
    # `FieldIndexOutOfBounds` raised through a path that never sets this
    # side-channel (e.g. `setfield!`), misreporting the wrong object. This
    # mirrors the non-transactional pending side-channel bug class from
    # Issue #9787. Fixed by parking atomically with the raise (via
    # `Vm::field_index_out_of_bounds_with_receiver`), keyed by the exact
    # `(index, field_count)` pair, instead of ahead of the lookup.
    e = Expr(:call, :f, 1, 2)
    v = getfield(e, 1) # succeeds; must not leave `e` parked afterwards
    @test v === :call

    b = RegressionRcvr11580(1, 2)
    err = try
        setfield!(b, 99, 7) # out-of-range index -> BoundsError via a
        # DIFFERENT path (setfield!) that does not set the getfield
        # receiver side-channel at all.
    catch caught
        caught
    end
    @test err isa BoundsError
    # `.a` must never be the stale `e` left behind by the earlier successful
    # getfield. Upstream Julia reports the real receiver `b` here; sjulia's
    # `setfield!` does not thread a receiver through this raise at all yet
    # (a separate, narrower, pre-existing gap outside #11509's getfield-only
    # scope, tracked as Issue #11596), so sjulia currently reports `nothing`
    # instead of `b`. Either is correct as long as it is not the stale `e`,
    # so this assertion checks for the absence of contamination rather than
    # a fixed value, keeping it upstream-parity-safe.
    @test err.a !== e
end

true
