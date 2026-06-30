using Test

# Issue #5979: an error raised *through* a doubly- (or more) nested `eval`
# (`eval(:(eval(:(f()))))`) that should be caught by an enclosing ancestor `try`
# must run the `catch` and bind `e` — matching the single-`eval` path (#5972) and
# upstream Julia.
#
# The inner `eval` is dispatched as an Immediate builtin via
# `execute_runtime_builtin_immediate`, whose `self.raise(err)` previously CAUGHT
# the ancestor handler (the `eval_dispatch_floor` installed by
# `run_until_frame_return` was already restored by then) and then re-surfaced the
# error through `RuntimeCallableResult::Raised` → `Err(pending_error)` — a
# double-handling that popped the handler AND clobbered `catch_ip` with
# `saved_ip`, so the exception escaped uncaught to the host. The fix installs the
# floor over the *entire* `eval_dispatch_call` dispatch (not just the
# `run_until_frame_return` arm), so the inner `self.raise` declines the ancestor
# handler and propagates `Err`; the outer `run()` loop re-routes it once.

boom() = error("explode")
domainboom() = throw(DomainError(-1.0, "neg"))

@testset "error across a doubly-nested eval is caught by an ancestor try (Issue #5979)" begin
    caught = "none"
    try
        eval(:(eval(:(boom()))))
    catch e
        caught = "$(typeof(e))"
    end
    @test caught == "ErrorException"
end

@testset "exception type preserved across a doubly-nested eval" begin
    ty = "none"
    try
        eval(:(eval(:(domainboom()))))
    catch e
        ty = "$(typeof(e))"
    end
    @test ty == "DomainError"
end

@testset "triple-nested eval error is also caught" begin
    is_err = false
    try
        eval(:(eval(:(eval(:(boom()))))))
    catch e
        is_err = e isa ErrorException
    end
    @test is_err
end

@testset "single-eval path still works (no #5972 regression)" begin
    caught = "none"
    try
        eval(:(boom()))
    catch e
        caught = "$(typeof(e))"
    end
    @test caught == "ErrorException"
end

@testset "control resumes and outer finally runs across the nested-eval catch" begin
    log = String[]
    total = 0
    try
        eval(:(eval(:(boom()))))
    catch e
        total = 10
        push!(log, "catch")
    finally
        push!(log, "finally")
    end
    total += 5
    @test total == 15
    @test log == ["catch", "finally"]
end

@testset "a non-raising nested eval returns its value through the try" begin
    r = 0
    try
        r = eval(:(eval(:(40 + 2))))
    catch e
        r = -1
    end
    @test r == 42
end

true
