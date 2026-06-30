# Issue #5924: an exact-arity fixed-parameter method must rank MORE specific than
# a vararg method for a call whose argument count matches the fixed arity:
# `g(::Int)` must beat `g(::Int...)` for `g(1)`, regardless of definition order.
#
# Root cause: the method-table dedup in `MethodTable::add_method` collapsed a
# fixed method and a same-projection vararg method into one method. `g(x::Int)`
# projects to params `[Int]` with `vararg_param_index = None`, while
# `g(x::Int...)` projects to the same `[Int]` with `vararg_param_index = Some(0)`.
# Neither the structured `core_signature` (`Tuple{Int}` for both) nor the legacy
# projection encoded the vararg marker, so the second definition *replaced* the
# first. The surviving single method then dispatched by registration order. The
# fix guards the dedup on matching vararg structure so the two stay distinct
# methods, and the existing scorer (which already ranks the fixed method strictly
# higher) selects it. Output matches upstream Julia 1.12.

using Test

# Fixed method declared first, vararg second.
g(x::Int)    = "single"
g(x::Int...) = "vararg"

# Vararg method declared first, fixed second (definition order must not matter).
h(x::Int...) = "vararg"
h(x::Int)    = "single"

@testset "exact-arity fixed method beats vararg (Issue #5924)" begin
    # Exact match: the fixed method wins in both declaration orders.
    @test g(1) == "single"
    @test h(1) == "single"

    # Control: no exact fixed match for two args, so the vararg method is chosen.
    @test g(2, 3) == "vararg"
    @test h(2, 3) == "vararg"

    # Control: zero args also routes to the (variadic) vararg method.
    @test g() == "vararg"
    @test h() == "vararg"
end

# A multi-arg fixed method must still beat a leading-fixed vararg for the exact
# arity (f(::Int, ::Int) vs f(::Int, ::Int...)).
f(a::Int, b::Int)    = "pair"
f(a::Int, b::Int...) = "splat"

@testset "multi-arg fixed method beats trailing vararg (Issue #5924)" begin
    @test f(1, 2) == "pair"
    @test f(1, 2, 3) == "splat"
    @test f(1) == "splat"
end

# The nextest harness only checks the file's FINAL value, and sjulia does not
# abort on a failing bare `@test`, so the final expression is an explicit
# boolean conjunction of every check (Issue #5924 fixture gotcha).
g(1) == "single" &&
    h(1) == "single" &&
    g(2, 3) == "vararg" &&
    h(2, 3) == "vararg" &&
    g() == "vararg" &&
    h() == "vararg" &&
    f(1, 2) == "pair" &&
    f(1, 2, 3) == "splat" &&
    f(1) == "splat"
