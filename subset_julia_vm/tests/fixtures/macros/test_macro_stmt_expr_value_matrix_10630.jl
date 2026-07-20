# Issue #10630 (prevention for #10307/#10496/PR #10625): consolidated matrix
# pinning the stdlib-Test macro statement/value adapter contract.
#
# Stdlib macro quotes lower through nested Block and LetBlock wrappers. The
# STATEMENT-position adapter must retain every effect statement (the recorder
# control-flow subtree) and remove ONLY the nested result tail; the
# EXPRESSION-position path must preserve the recorded Test.Result value.
# Treating the outer final statement as a disposable return value can delete
# the recorder subtree, while constructing a discarded result object in
# statement position can overwrite an unrelated caller slot.
#
# This fixture is the GREEN half of the matrix (all tests pass, upstream
# parity checkable). The deliberately-FAILING bare statements — which prove
# the sticky failure flag and would flip the process exit code — live in
# `tests/testset_exit_code_8191_tests.rs`
# (`statement_position_matrix_10630` / `expression_position_matrix_10630`):
# upstream julia throws for a bare failing @test, so those halves cannot be a
# parity fixture. Sibling fixture:
# macros/stdlib_macro_expr_position_10293_10307.jl (expression-position
# dispatch coverage).
using Test

# Statement position: the recorder effect must run (pass counted), the result
# value must be discarded, and no unrelated caller slot may be overwritten by
# the discarded Test.Pass object.
@testset "statement position keeps effects, discards value" begin
    sentinel = 42
    @test 1 + 1 == 2
    @test sentinel == 42
end

# Statement position in a non-tail slot of a function body: the discarded
# result must not clobber the function's actual return value.
function stmt_position_then_return()
    @test true
    return "kept"
end
@testset "statement expansion inside a function body" begin
    @test stmt_position_then_return() == "kept"
end

# Expression position (assignment RHS): the value-preserving path returns the
# recorded Test.Result instead of nothing.
@testset "expression position preserves the recorded value" begin
    p = @test 2 + 2 == 4
    @test p isa Test.Pass
    # (@test_broken value preservation is pinned by
    # stdlib_macro_expr_position_10293_10307.jl — upstream's Broken summary
    # column is not parseable by fixture_julia_parity.sh, so it is kept out
    # of this parity fixture.)
    t = @test_throws ErrorException error("boom_10630")
    @test t isa Test.Pass
end

# Block-tail expression position: a begin block whose non-tail statement is a
# statement-position @test must still yield the block's own tail value.
@testset "block tail after a statement-position @test" begin
    v = begin
        @test true
        99
    end
    @test v == 99
end

# @testset itself in expression position keeps its TestSet-shaped value.
y = @testset "testset value position" begin
    @test true
end
@test y isa Test.DefaultTestSet

true
