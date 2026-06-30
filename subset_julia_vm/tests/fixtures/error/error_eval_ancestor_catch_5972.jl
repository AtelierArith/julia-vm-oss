using Test

# Issue #5972: an error raised inside an `eval`-driven nested VM dispatch that is
# caught by a handler installed by an *ancestor* frame (a `try` opened *outside*
# the `eval` call) must run that `catch` and bind `e` to the raised exception —
# exactly as the equivalent non-`eval` code does.
#
# Previously `run_until_frame_return` let `handle_error` route such an error to
# the ancestor handler from *inside* the nested loop: it truncated the frame
# stack below the loop's `target_depth`, the loop's return check fired mid-catch,
# and a garbage value was returned to `eval` — the `catch` body never ran and the
# exception was silently swallowed (a `StackOverflowError` surfaced as `no error`;
# a user `throw`/`error` corrupted the operand stack into a `Stack underflow`).
# The fix makes the error propagate as a normal `Err` out of the nested dispatch
# so the outer `run()` loop re-routes it to the ancestor handler at the right
# level. See `Vm::eval_dispatch_floor` / `handle_error`.

# A user function reached through the real VM call path (so `eval` drives a VM
# frame via `run_until_frame_return`, not the mini-interpreter's fast arms).
boom() = error("explode")
domainboom() = throw(DomainError(-1.0, "must be nonneg"))

# Non-tail recursion: a real frame outlives each call, so the call-frame stack
# genuinely grows and crosses MAX_CALL_DEPTH (10_000), raising StackOverflowError.
countdown(n) = n <= 0 ? 0 : countdown(n - 1)

# A function that catches its OWN error locally (handler installed *inside* the
# eval'd call, `frame_len > target_depth`). The ancestor-handler floor must NOT
# block this — the local catch still runs and no error escapes.
self_healing() =
    try
        error("inner")
    catch
        "handled-inside"
    end

@testset "ancestor `catch` runs for a user error raised across `eval` (Issue #5972)" begin
    caught = "no error"
    try
        eval(:(boom()))
    catch e
        caught = "$(typeof(e))"
    end
    @test caught == "ErrorException"
end

@testset "ancestor `catch` binds the right exception type across `eval`" begin
    ty = "none"
    try
        eval(:(domainboom()))
    catch e
        ty = "$(typeof(e))"
    end
    @test ty == "DomainError"
end

@testset "eval-driven StackOverflow is caught by an ancestor handler (Issue #5972)" begin
    is_so = false
    try
        eval(:(countdown(20000)))
    catch e
        is_so = e isa StackOverflowError
    end
    @test is_so
end

@testset "`eval` path agrees with the non-`eval` path" begin
    eval_caught = false
    try
        eval(:(countdown(20000)))
    catch e
        eval_caught = e isa StackOverflowError
    end
    noeval_caught = false
    try
        countdown(20000)
    catch e
        noeval_caught = e isa StackOverflowError
    end
    @test eval_caught == noeval_caught == true
end

@testset "control resumes after the cross-`eval` catch" begin
    total = 0
    try
        eval(:(boom()))
    catch e
        total = 10
    end
    total += 5
    @test total == 15
end

@testset "a `finally` outside `eval` still runs when the eval'd code throws" begin
    log = String[]
    try
        eval(:(boom()))
    catch e
        push!(log, "catch")
    finally
        push!(log, "finally")
    end
    @test log == ["catch", "finally"]
end

@testset "a handler installed INSIDE the eval'd call is unaffected (regression guard)" begin
    # `self_healing` catches its own error; the OUTER `try` must never see it.
    outer_saw_error = false
    r = "unset"
    try
        r = eval(:(self_healing()))
    catch e
        outer_saw_error = true
    end
    @test r == "handled-inside"
    @test outer_saw_error == false
end

true
