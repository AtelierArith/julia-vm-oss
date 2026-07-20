# Test @test try/catch wrapping regression coverage (Issue #10093)
#
# `Test.@test` now wraps the evaluation of its expression in try/catch,
# mirroring upstream `get_test_result`/`do_test` (`Returned`/`Threw`): an
# exception thrown by the test expression is recorded as an "errored" outcome
# instead of propagating out of the enclosing `@testset`.
#
# This fixture is the GREEN half of the coverage: it verifies that the new
# try/catch wrapping does not change the semantics of passing tests —
# exceptions handled *inside* the test expression, assignments inside the
# expression, and tests running in loops all still behave exactly as before
# (and exactly as upstream julia). The RED half — an actually-errored `@test`
# (which must set the sticky failure flag and exit non-zero AFTER the testset
# summary) — cannot be a fixture (the harness's @testset gate rejects any
# fixture that records a failure, Issue #9360); it is covered by Rust
# integration tests in tests/testset_exit_code_8191_tests.rs.

using Test
using Base.MathConstants: e

function catches_internally()
    try
        error("internal boom")
    catch
        42
    end
end

@testset "test expr with internally caught exceptions" begin
    # An exception raised and caught INSIDE the @test expression is invisible
    # to the @test try/catch wrapper: the test records its Bool result.
    @test catches_internally() == 42
    @test (try
        error("x")
        false
    catch e
        true
    end)
    # @test_throws still owns the "expression must throw" behavior.
    @test_throws ErrorException error("propagates to @test_throws")
end

@testset "test expr side effects still apply" begin
    # The try block introduced by the @test expansion must not re-scope
    # variables used by the test expression: the counter updates the
    # enclosing testset-local binding.
    count = 0
    for i in 1:3
        @test (count += 1; count == i)
    end
    @test count == 3
end

# Regression for Issue #10242: the Test macros' quote-internal catch variable
# must not shadow a user/global `e` in the enclosing @testset scope. Before
# the `__test_*` renames, any expansion containing `catch e` (@test after
# #10093, @test_broken/@test_throws since always) made later references to
# `e` in the same testset resolve to the caught exception (or a stale
# shadowed-global type, the #8852 class) instead of Base.MathConstants.e.
# (The @test_broken variant of the same leak is covered in
# tests/testset_exit_code_8191_tests.rs, because julia's Broken summary
# column is not parseable by scripts/fixture_julia_parity.sh.)
@testset "catch variable does not shadow user e (Issue #10242)" begin
    @test abs(e - 2.718281828459045) < 1e-10
    @test_throws ErrorException error("shadow probe")
    @test e == ℯ
    @test typeof(e) == Irrational{:ℯ}
    @test abs(e - 2.718281828459045) < 1e-10
end

true
