# Regression fixture for Issue #10242 (macro hygiene epic #10253): a stdlib
# Test macro's quote-internal `catch` variable must be gensym-renamed by the
# static stdlib-macro quote expansion so it can never shadow a same-named
# user/global variable -- whether that global is read before the macro's
# catch clause has ever run, or after it has actually caught an exception.
#
# Before the #10242 fix, `collect_introduced_vars` (the hygiene pass in
# subset_julia_vm_lowering/src/lowering/expr/quote/handlers.rs) never registered the
# `catch` variable as an introduced local -- only `local`/assignment targets
# were -- so a plain `catch e` in @test/@test_throws/@test_broken leaked
# into the enclosing @testset scope under the literal name `e`, shadowing
# Base.MathConstants.e for the rest of the testset.

using Test
using Base.MathConstants: e

@testset "quote-internal catch variable hygiene (Issue #10242)" begin
    # Read `e` BEFORE any macro invocation containing a `catch` clause has
    # run in this testset. Under the pre-fix bug, merely having an
    # un-hygienic `catch e` anywhere in the compiled scope could make every
    # reference to `e` in that scope -- including earlier ones -- resolve
    # through the local catch-variable slot instead of the global import
    # (the stale-shadowed-global-type symptom, the Issue #8852 class).
    @test typeof(e) == Irrational{:ℯ}
    @test abs(e - 2.718281828459045) < 1e-10

    @test_throws ErrorException error("probe one")

    # Read `e` AFTER a catch clause has actually caught an exception: the
    # caught exception must not leak through under the name `e`.
    @test e == ℯ
    @test typeof(e) == Irrational{:ℯ}

    @test_throws BoundsError [1, 2, 3][10]

    # A second macro invocation with its own (independently gensym'd) catch
    # clause must not disturb `e` either.
    @test abs(e - 2.718281828459045) < 1e-10
end

true
