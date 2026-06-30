# Issue #5121: keyword arguments do not participate in dispatch (kwsorter), with
# two verified gaps:
#
#   1. A keyword-argument default *expression* must be re-evaluated on every call
#      where the keyword is omitted -- not evaluated once at definition time.
#      A side-effecting default (here a `Ref`-backed counter) makes the
#      per-call re-evaluation observable.
#
#   2. Passing an *unknown* keyword argument must raise an error (upstream Julia
#      raises a `MethodError`: "unsupported keyword argument"), not silently
#      ignore the keyword.
#
# Verified against upstream Julia 1.12 before implementation.
#
# Definitions are kept at top level (functions + counters) so the test does not
# also exercise the separate closure-capture-in-`@testset` limitation; the
# `@testset` blocks only hold assertions.
#
# The issue's Case 1 example uses a `global` counter (`global n; n = n + 1`); a
# `Ref`-backed counter is used here instead because the `global` read-modify-write
# form trips a separate, pre-existing bug unrelated to keyword arguments
# (Issue #5548). The per-call keyword-default re-evaluation that #5121 fixes is
# fully observable through the `Ref` counter.

using Test

# --- side-effecting default, positional argument present ---------------------
const counter_a_5121 = Ref(0)
function inc_a_5121()
    counter_a_5121[] = counter_a_5121[] + 1
    return counter_a_5121[]
end
f_reeval_5121(x; k=inc_a_5121()) = (x, k)

@testset "kwargs_default_reeval_5121: side-effecting default re-evaluated per call" begin
    @test f_reeval_5121(1) == (1, 1)        # k omitted -> inc_a() -> 1
    @test f_reeval_5121(2) == (2, 2)        # k omitted -> inc_a() -> 2
    @test f_reeval_5121(10; k=99) == (10, 99)  # k supplied -> NO re-evaluation
    @test counter_a_5121[] == 2             # supplying k must not bump the counter
    @test f_reeval_5121(3) == (3, 3)        # k omitted -> inc_a() -> 3
end

# --- side-effecting default, keyword-only function ---------------------------
const counter_b_5121 = Ref(0)
function inc_b_5121()
    counter_b_5121[] = counter_b_5121[] + 1
    return counter_b_5121[]
end
g_reeval_5121(; k=inc_b_5121()) = k

@testset "kwargs_default_reeval_5121: kwargs-only side-effecting default" begin
    @test g_reeval_5121() == 1
    @test g_reeval_5121() == 2
    @test g_reeval_5121(k=100) == 100
    @test counter_b_5121[] == 2
    @test g_reeval_5121() == 3
end

# --- pure call default with a positional argument ----------------------------
base_val_5121() = 42
h_reeval_5121(x; k=base_val_5121()) = (x, k)

@testset "kwargs_default_reeval_5121: pure call default with positional arg" begin
    @test h_reeval_5121(1) == (1, 42)
    @test h_reeval_5121(2; k=7) == (2, 7)
end

# --- defaults referencing earlier args/kwargs (existing behavior preserved) ---
p_reeval_5121(x; y=x + 1) = (x, y)
q_reeval_5121(; a=1, b=a + 10) = (a, b)

@testset "kwargs_default_reeval_5121: defaults referencing earlier args/kwargs still work" begin
    @test p_reeval_5121(5) == (5, 6)
    @test p_reeval_5121(5; y=100) == (5, 100)

    @test q_reeval_5121() == (1, 11)
    @test q_reeval_5121(a=5) == (5, 15)
    @test q_reeval_5121(a=5, b=0) == (5, 0)
end

# --- unknown keyword argument raises -----------------------------------------
r_unknown_5121(x; a=1) = x + a

@testset "kwargs_unknown_keyword_5121: unknown keyword argument raises" begin
    @test r_unknown_5121(1) == 2
    @test r_unknown_5121(1; a=10) == 11
    # `z` is not a declared keyword argument of `r_unknown_5121`
    @test_throws MethodError r_unknown_5121(1, z=99)
    @test_throws MethodError r_unknown_5121(1; a=2, z=3)
end

true
