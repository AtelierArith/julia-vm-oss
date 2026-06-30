# Issue #6288: a closure capturing a variable local to a `@time` (or `@elapsed`)
# block must (1) compile — it previously failed with "Undefined variable" even
# for a plain read-only capture — and (2) observe later reassignments of that
# local (Julia `Core.Box` cell semantics), completing the `@testset` coverage of
# #6281. `@time` lowers its body to `#result# = let … end` (an empty-binding
# `let` block as an assignment value), so both the lambda-capture pre-analysis
# (compile/mod.rs) and the boxing pass (lowering/closure_box.rs) must descend
# into that form.
#
# The fixture harness checks the final value (a failing inner `@test` only
# prints), so correctness is encoded as a trailing boolean, surfaced out of the
# blocks through module-global `Ref`s.

using Test

# --- read-only @time block-local capture (the compile fix) ---
ro = Ref(-1)
@time begin
    c = 7
    get_c = () -> c
    @test get_c() == 7
    ro[] = get_c()
end

# --- @time block-local capture + reassignment (boxing) ---
rw = Ref(-1)
@time begin
    counter = 0
    view_counter = () -> counter
    @test view_counter() == 0
    counter = 5
    @test view_counter() == 5
    rw[] = view_counter()
end

# --- `@time` as an assignment value, capture + reassign in the body ---
rv = @time begin
    a = 3
    b = 4
    f = () -> a + b
    a = 10
    f()
end
@test rv == 14

# --- regression: @testset and bare `begin` still behave (Issues #6281/#6262) ---
ts = Ref(-1)
@testset "regression: @testset capture still works" begin
    n = 1
    read_n = () -> n
    n = 2
    ts[] = read_n()
    @test read_n() == 2
end

# Final value is the regression guard: true only if every closure observed the
# latest value of its captured block-local.
ro[] == 7 && rw[] == 5 && rv == 14 && ts[] == 2
