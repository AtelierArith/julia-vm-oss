# Test timedwait function (Issue #3501)
#
# timedwait(testcb, timeout::Real; pollint::Real=0.1)
#   Polls testcb() until it returns true or timeout seconds elapse.
#   Returns :ok if predicate became true, :timed_out otherwise.

using Test

# ---------------------------------------------------------------------------
# Helper: counter closure that returns true after `threshold` calls.
# ---------------------------------------------------------------------------
function make_after_calls(threshold)
    count = 0
    function predicate()
        count = count + 1
        count >= threshold
    end
    predicate
end

# Helper: predicate that returns true once a deadline has passed.
function make_after_seconds(deadline_s)
    start = time()
    function predicate()
        (time() - start) >= deadline_s
    end
    predicate
end

# Helper: always-false predicate.
function always_false()
    false
end

# Helper: always-true predicate.
function always_true()
    true
end

@testset "timedwait returns :ok when predicate already true" begin
    # Predicate becomes true on the very first call (before any sleeping).
    result = timedwait(always_true, 1.0)
    @test result === :ok
end

@testset "timedwait returns :ok when predicate becomes true" begin
    # Predicate flips to true after a few polls.
    pred = make_after_calls(3)
    result = timedwait(pred, 5.0; pollint=0.05)
    @test result === :ok
end

@testset "timedwait returns :timed_out for stuck predicate" begin
    # Predicate never returns true; we should time out within ~timeout seconds.
    t0 = time()
    result = timedwait(always_false, 0.2; pollint=0.05)
    elapsed = time() - t0
    @test result === :timed_out
    # Lower bound: timedwait must wait at least roughly `timeout` before giving up.
    # Use a generous tolerance to avoid flakes on slow CI / coarse clocks.
    @test elapsed >= 0.1
end

@testset "timedwait honors small pollint" begin
    # With a fine-grained pollint, an after-deadline predicate must succeed
    # well before the outer timeout fires.
    pred = make_after_seconds(0.1)
    result = timedwait(pred, 2.0; pollint=0.05)
    @test result === :ok
end

@testset "timedwait honors larger pollint" begin
    # With a coarser pollint we still complete eventually.
    pred = make_after_seconds(0.05)
    result = timedwait(pred, 2.0; pollint=0.2)
    @test result === :ok
end

@testset "timedwait rejects non-positive pollint" begin
    @test_throws ArgumentError timedwait(always_true, 1.0; pollint=0.0)
end

true
