# Symbolics subset: `simplify` and `expand` (Issue #6572).
#
# `simplify` folds constants and combines like terms/factors with canonical
# ordering; `expand` distributes products/powers then simplifies. Because output
# operands are canonically sorted, `expand` results are verified by
# `substitute`-evaluation (order-independent) rather than exact form, as the plan
# prescribes. `simplify` single-result cases have a deterministic canonical form,
# so they are checked with `==`.

using Test
using Symbolics

evalat(e, d) = value(substitute(e, d))

@testset "Symbolics simplify: like terms and factors" begin
    @variables x y
    @test simplify(x + x) == 2x
    @test simplify(2x + 3x) == 5x
    @test simplify(x + x + x) == 3x
    @test simplify(x * x) == x^2
    @test simplify(x^2 * x) == x^3
    @test simplify(2 * (3 * x)) == 6x
    @test simplify(x - x) == 0
    @test simplify(x + 0) == x
    @test simplify(0 * x) == 0
end

@testset "Symbolics simplify: combines across a sum (eval-checked)" begin
    @variables x y
    e = simplify(x + y + x)
    # 2x + y up to canonical ordering: check by evaluation.
    @test evalat(e, Dict(x => 2, y => 3)) == 7
    @test evalat(e, Dict(x => 5, y => 1)) == 11
    # one + and two distinct symbolic terms remain
    @test operation(value(e)) === :+
end

@testset "Symbolics expand: distribution (eval-checked)" begin
    @variables x y
    @test evalat(expand(x * (x + 1)), Dict(x => 2)) == 6      # (2)(3)
    @test evalat(expand(x * (x + 1)), Dict(x => 4)) == 20     # (4)(5)
    @test evalat(expand((x + 1) * (x + 2)), Dict(x => 3)) == 20   # (4)(5)
    @test evalat(expand((x + y) * (x - y)), Dict(x => 5, y => 3)) == 16  # 25-9
    @test evalat(expand((x + y)^2), Dict(x => 2, y => 3)) == 25  # 5^2
    @test evalat(expand((x + y)^3), Dict(x => 2, y => 3)) == 125 # 5^3
end

@testset "Symbolics expand: produces a polynomial, not a product" begin
    @variables x y
    # x*(x+1) expands to a SUM (top operation +), not a product.
    @test operation(value(expand(x * (x + 1)))) === :+
    # (x+y)^2 expands to a sum of three monomials (x^2, 2xy, y^2).
    e = expand((x + y)^2)
    @test operation(value(e)) === :+
    # equivalent to the hand-written polynomial under evaluation
    @test evalat(e, Dict(x => 7, y => 11)) == evalat(x^2 + 2x * y + y^2, Dict(x => 7, y => 11))
end

true
