# Symbolics subset: canonical-form display of simplified expressions (Issue #7894).
#
# `simplify` now orders a sum's addends to match upstream Symbolics' display
# (ascending total degree, then descending exponent on the earlier variable, so
# `x^2` precedes `x*y` and a constant leads), and `show` renders a negative
# coefficient as a leading/subtraction minus (`a + (-1)*b` -> `a - b`). Together
# these make the `det`/`inv` output match upstream as a *string*, the Phase 2
# goal of Epic #7888.
#
# Scope (per the epic): the monomial sums that `det`/`inv` produce. Full
# polynomial canonicalization, the `2x` coefficient spelling (the subset keeps
# `2*x`), and construction-time `x*x -> x^2` folding of raw (un-simplified)
# products are out of scope; equivalence elsewhere is checked structurally.
#
# The asserted strings are byte-identical to upstream julia 1.12.6.

using Test
using Symbolics
using LinearAlgebra

@testset "Symbolics canonical form: det output string" begin
    @variables x y
    # The headline parity target of the epic.
    @test string(det([x y; x x])) == "x^2 - x*y"
end

@testset "Symbolics canonical form: term ordering" begin
    @variables x y
    # Same total degree: x^2 before x*y; pure powers ordered by variable.
    @test string(simplify(x * x - y * x)) == "x^2 - x*y"
    @test string(simplify(x * x + y * y)) == "x^2 + y^2"
    # A constant leads (degree 0 first); higher-degree power last.
    @test string(simplify(x * x - 1)) == "-1 + x^2"
    @test string(simplify(x * x * x - y)) == "-y + x^3"
end

@testset "Symbolics canonical form: negative-coefficient display" begin
    @variables x y
    # `(-1)*(x*y)` renders `-x*y`, not `-1*(x*y)`.
    @test string(simplify(-(x * y))) == "-x*y"
    # Inside a sum a negative addend renders as a subtraction.
    @test string(simplify(x * x - y * x)) == "x^2 - x*y"
    # A raw subtraction is unchanged.
    @test string(x - y) == "x - y"
    @test string(-x) == "-x"
end

true
