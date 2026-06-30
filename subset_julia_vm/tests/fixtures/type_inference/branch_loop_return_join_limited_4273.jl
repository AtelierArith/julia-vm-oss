# Issue #4273: migrate inference return-aggregation joins to comparison-aware
# `join_limited` so small / already-known structured unions built from branch
# (`if`/`elseif`/`try`) and loop-body (`for x in ...`, `while`) returns are
# preserved across the join instead of collapsing to `Any` / `Top`.
#
# Before this change the `for-in` (ForEach) and `while` loop-body return
# accumulators, plus the block-level return aggregation, used the plain
# `join` path whose unconditional length/complexity bound widens any union
# whose deepest member exceeds the absolute complexity cap — even when that
# member is already present in the previously-accumulated comparison type.
# Routing these through `join_limited(.., compare_to = previous_return)`
# matches upstream Julia's `limit_type_size`, which only counts *new*
# complexity relative to the comparison type.
#
# All assertions are verified against upstream Julia 1.12.

using Test

# ---------------------------------------------------------------------------
# (a) DISTINGUISHING CASE — a deeply-nested tuple return seeds the accumulator
# and a later shallow `Int` return is joined against it. The deep member is
# already part of the comparison type, so the union must be preserved.
# Plain `join` widened this union to `Any` because its deepest member exceeds
# the absolute complexity cap; comparison-aware `join_limited` keeps it.
# ---------------------------------------------------------------------------
function tinf_branch_deep_first_4273(x::Int)
    if x > 10
        return ((((((x,),),),),),)   # depth-6 nested tuple, seen FIRST
    elseif x > 5
        return x                       # Int64
    else
        return x + 1                   # Int64
    end
end

# ---------------------------------------------------------------------------
# (b) `for x in collection` (ForEach) loop accumulating a small union of
# distinct tuple shapes via two in-body returns, with a post-loop fallthrough.
# ---------------------------------------------------------------------------
function tinf_foreach_tuple_union_4273(xs::Vector{Int})
    for x in xs
        if x > 5
            return (x, x)
        elseif x > 0
            return (x, x, x)
        end
    end
    return nothing
end

# ---------------------------------------------------------------------------
# (c) `while` loop accumulating a small `Union{T, Nothing, Missing}` from
# branch + fallthrough returns — the canonical "small union across a loop"
# acceptance case.
# ---------------------------------------------------------------------------
function tinf_while_nullable_union_4273(n::Int)
    i = 0
    while i < n
        if i > 5
            return i
        elseif i > 2
            return nothing
        end
        i += 1
    end
    return missing
end

@testset "branch/loop return aggregation uses comparison-aware join (Issue #4273)" begin
    # (a) deep-first branch union preserved, not widened to Any.
    @test Base.infer_return_type(tinf_branch_deep_first_4273, Tuple{Int}) ==
        Union{Int64, Tuple{Tuple{Tuple{Tuple{Tuple{Tuple{Int64}}}}}}}
    @test Base.return_types(tinf_branch_deep_first_4273, Tuple{Int})[1] ==
        Union{Int64, Tuple{Tuple{Tuple{Tuple{Tuple{Tuple{Int64}}}}}}}

    # (b) for-in loop small tuple-shape union preserved across the body
    # accumulator and the post-loop fallthrough.
    @test Base.return_types(tinf_foreach_tuple_union_4273, Tuple{Vector{Int}})[1] ==
        Union{Nothing, Tuple{Int64, Int64}, Tuple{Int64, Int64, Int64}}

    # (c) while loop small nullable union preserved.
    @test Base.return_types(tinf_while_nullable_union_4273, Tuple{Int})[1] ==
        Union{Missing, Nothing, Int64}
end

true
