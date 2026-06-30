using Test

# eval(Expr(:ref, arr, idx...)) is getindex (Issue #5932). Array + Matrix only.
# Note: eval runs in module (Main) scope, so the container Symbol must resolve
# to a top-level global, not a @testset/let local.
a = [10, 20, 30]
m = [1 2; 3 4]

@testset "eval Expr(:ref, ...) getindex (Issue #5932)" begin
    @test eval(Expr(:ref, :a, 2)) == 20
    @test eval(Expr(:ref, :a, 1)) == 10
    @test eval(Expr(:ref, :m, 1, 2)) == 2
    @test eval(Expr(:ref, :m, 2, 1)) == 3
end

# The nextest harness only checks the FINAL value == expected(true), and a
# failing bare @test does not abort, so the final expression must be a boolean
# conjunction of the checks (Issue #5932).
eval(Expr(:ref, :a, 2)) == 20 &&
    eval(Expr(:ref, :a, 1)) == 10 &&
    eval(Expr(:ref, :m, 1, 2)) == 2 &&
    eval(Expr(:ref, :m, 2, 1)) == 3
