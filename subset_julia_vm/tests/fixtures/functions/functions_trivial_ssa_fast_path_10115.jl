# Issue #10115: functions whose body is a single literal-arg call or a bare
# literal return skip the SSA build/optimize/plan pipeline entirely (nothing
# in a one-statement, param-free-argument body is foldable/eliminable/
# reorderable) and compile through the legacy path instead. This fixture pins
# correctness for each of the matched shapes plus a couple of near-miss shapes
# that must NOT match (so they keep going through SSA) to guard the predicate
# boundary.

using Test

# Matches: bare literal return (explicit).
f_ret_lit_10115() = return 42

# Matches: implicit-tail literal.
f_tail_lit_10115() = 7

# Matches: single call with only literal args (no kwargs/splat).
f_call_lit_10115() = min(3, 5)

# Matches: empty return.
function f_empty_ret_10115()
    return
end

# Near-miss: call argument references a parameter, not a literal -> must
# still compile correctly (through the normal SSA/legacy path, unaffected by
# the fast-path predicate).
f_call_param_10115(x) = min(x, 5)

# Near-miss: two statements -> not trivial by the single-statement rule.
function f_two_stmts_10115()
    y = 1
    return y + 1
end

@testset "trivial ssa fast path 10115" begin
    @test f_ret_lit_10115() == 42
    @test f_tail_lit_10115() == 7
    @test f_call_lit_10115() == 3
    @test f_empty_ret_10115() === nothing
    @test f_call_param_10115(10) == 5
    @test f_two_stmts_10115() == 2
end

true
