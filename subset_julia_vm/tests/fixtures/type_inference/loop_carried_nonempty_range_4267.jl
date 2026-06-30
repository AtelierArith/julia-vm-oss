using Test

# Issue #4267 (production inference precision for loops): a `for` loop over a
# provably non-empty constant range (`1:10`, `1:2:9`) runs at least once, so the
# pre-loop environment never falls through to the post-loop state — the loop's
# exit env is the body's exit env. sjulia previously joined the pre-loop type of
# a body-reassigned carried variable into the post-loop env, over-widening the
# result (e.g. `Union{Float64, Int64}` where upstream infers `Float64`).
#
# This mirrors the existing non-empty-const-range break narrowing (#4680): when
# the range is provably non-empty and the body has no dominant `break`, the
# post-loop env is taken from the body's fixpoint exit env rather than re-merging
# the pre-loop snapshot. Possibly-empty ranges (dynamic bounds) must stay
# conservative because the loop may run zero times.
#
# Values verified field-for-field against upstream Julia 1.12.6.

# Carried Int64 promoted to Float64 by the body over a non-empty range.
function loop_carried_promote_4267()
    x = 0
    for i in 1:10
        x = x + 1.0
    end
    return x
end

# Carried variable reassigned to a different type over a non-empty range.
function loop_carried_reassign_4267()
    x = 0
    for i in 1:3
        x = "s"
    end
    return x
end

# Non-empty range with an explicit step.
function loop_carried_step_4267()
    x = 0
    for i in 1:2:9
        x = x + 1.0
    end
    return x
end

# Regression guard: same-type accumulation stays concrete.
function loop_carried_accum_4267()
    s = 0
    for i in 1:5
        s = s + 1
    end
    return s
end

# Soundness guard: a possibly-empty (dynamic-bound) range may run zero times, so
# the pre-loop type must still fall through (stays the conservative union).
function loop_carried_dynamic_4267(n::Int64)
    x = 0
    for i in 1:n
        x = x + 1.0
    end
    return x
end

@testset "non-empty const range loop-carried precision (#4267)" begin
    @test Base.infer_return_type(loop_carried_promote_4267, Tuple{}) === Float64
    @test Base.infer_return_type(loop_carried_reassign_4267, Tuple{}) === String
    @test Base.infer_return_type(loop_carried_step_4267, Tuple{}) === Float64
end

@testset "loop-carried regression and soundness guards (#4267)" begin
    # Same-type accumulation is unchanged.
    @test Base.infer_return_type(loop_carried_accum_4267, Tuple{}) === Int64
    # Possibly-empty range keeps the conservative union (may run zero times).
    @test Base.infer_return_type(loop_carried_dynamic_4267, Tuple{Int64}) ==
        Union{Float64,Int64}
    # Runtime behavior matches.
    @test loop_carried_promote_4267() === 10.0
    @test loop_carried_reassign_4267() == "s"
    @test loop_carried_accum_4267() === 5
end

true
