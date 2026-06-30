# A nested LOCAL function that shares a name with an existing GLOBAL function
# must shadow that global within its enclosing function's scope, while leaving
# the global intact everywhere else (Issue #8105). Reproduces at top level (no
# @testset required); the @testset here only wraps the assertions.
#
# Two root causes were behind the original bug:
#   1. The small-pure-function inliner did not treat a nested function
#      definition as a local binding, so a call to that name was inlined to the
#      same-named GLOBAL body.
#   2. Nested functions were registered in the shared SHORT-NAME method table,
#      where `add_method`'s signature dedup let an inner zero-arg `g()` REPLACE
#      the global `g()`; a value reference to the global (`f = g; f()`) then
#      picked up the inner body.

using Test

# --- zero-arg shadowing ---------------------------------------------------
g() = 1
function h()
    g() = 2          # nested local g shadows the global g within h
    return g()
end

# --- value-reference variant: binding the global, then calling it ---------
function zztop()
    1
end
function make_zztop()
    zztop() = 2
    return zztop()
end

# --- nested local WITH ARGS shadows a global with args --------------------
gx(x) = x
function hx()
    gx(x) = x + 10
    return gx(5)
end

# --- recursive nested local shadows a same-named global -------------------
fact(n) = -999
function outer_fact()
    fact(n) = n <= 1 ? 1 : n * fact(n - 1)   # self-recursion -> the LOCAL fact
    return fact(5)
end

# --- typed-method visibility: inside h2 only the local table is visible ---
gt(x::String) = "global:" * x
function h2()
    gt(x::Int) = x + 1
    return gt(10)
end

# --- nested local with a name that has NO global counterpart still works ---
function only_local()
    gg() = 7
    return gg()
end

@testset "nested local function shadows same-named global (Issue #8105)" begin
    @test h() == 2          # nested local g()
    @test g() == 1          # global g() intact

    f = zztop
    @test f() == 1               # value of the global, not the inner zztop
    @test make_zztop() == 2      # inner zztop()
    @test zztop() == 1           # global zztop() intact

    @test hx() == 15        # nested local gx(x) = x + 10
    @test gx(5) == 5        # global gx(x) = x intact

    @test outer_fact() == 120    # recursive LOCAL fact, not the global
    @test fact(5) == -999        # global fact intact

    @test h2() == 11        # nested local gt(::Int)
    @test gt("hi") == "global:hi"

    @test only_local() == 7
end

true
