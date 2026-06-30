# Issue #6281: a closure capturing a scalar local to an `@testset` block (or a
# bare `begin … end` block) must observe later top-level reassignments of that
# local — Julia `Core.Box` cell semantics. Follow-up to #6262 (which fixed the
# function-local case). Such bodies lower to nested empty-binding `let` blocks
# (`Stmt::Expr(LetBlock { bindings: [], … })`) rather than `Stmt::Block`, so the
# boxing pass (`lowering/closure_box.rs`) must descend into empty-binding `let`
# blocks as defining scopes to unify the binding, its reassignments, and the
# capturing closure.
#
# The fixture harness checks the FINAL value, and a failing inner `@test` does
# not by itself fail the fixture (it only prints). Correctness is therefore also
# encoded as a trailing boolean — surfaced through module-global `Ref`s so the
# value the closure observed inside the block escapes for the final check — while
# the `@test`s remain for human-readable diagnostics.

using Test

# --- @testset-block-local capture + multiple reassignments ---
ts_observed = Ref(-1)
@testset "testset-local capture+reassign (Issue #6281)" begin
    counter = 0
    get_counter = () -> counter
    @test get_counter() == 0
    counter = 5
    @test get_counter() == 5
    counter = 12
    @test get_counter() == 12
    ts_observed[] = get_counter()
end

# --- bare `begin … end` block at top level: same semantics ---
begin_observed = Ref(-1)
begin
    n = 1
    read_n = () -> n
    n = 2
    n = 3
    begin_observed[] = read_n()
end

# True only if each closure observed the latest reassignment (the real guard).
ts_observed[] == 12 && begin_observed[] == 3
