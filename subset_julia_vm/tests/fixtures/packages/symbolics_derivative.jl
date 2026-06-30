# Symbolics subset: differentiation (Issue #6572).
#
# `Differential(x)` returns a one-argument function (a closure — the VM cannot
# dispatch a struct call operator inside a module, #7185) and `derivative(expr,
# var)` differentiates eagerly. Like upstream's `derivative(...; simplify=false)`
# default, results use only the shallow `_mk*` normalization, so cases that need
# further collection (e.g. a second derivative) are checked through `simplify`
# or `substitute`-evaluation.

using Test
using Symbolics

evalat(e, d) = value(substitute(e, d))

@testset "Symbolics derivative: basic rules" begin
    @variables x y
    @test derivative(x^2, x) == 2x          # power rule, folds to 2x
    @test derivative(x^3, x) == 3 * x^2
    @test derivative(2x + 1, x) == 2
    @test derivative(x * y, x) == y         # product rule, ∂y/∂x = 0
    @test derivative(Num(5), x) == 0        # constant
    @test derivative(x, y) == 0             # independent variable
    @test isequal(derivative(x^2 + sin(x), x), 2x + cos(x))
end

@testset "Symbolics derivative: Differential operator (closure)" begin
    @variables x
    @test Differential(x)(sin(x)) == cos(x)
    @test isequal(Differential(x)(cos(x)), -sin(x))
    @test Differential(x)(exp(x)) == exp(x)
    D = Differential(x)
    @test D(x^2) == 2x                       # bound, then applied
    @test D(log(x)) == 1 / x
end

@testset "Symbolics derivative: chain/quotient/elementary (eval-checked)" begin
    @variables x
    @test evalat(derivative(sin(x^2), x), Dict(x => 1.0)) ≈ 2 * 1.0 * cos(1.0)
    @test evalat(derivative(exp(2x), x), Dict(x => 0.0)) ≈ 2.0           # 2*exp(0)
    @test evalat(derivative(log(x), x), Dict(x => 2.0)) ≈ 0.5
    @test evalat(derivative(x / (x + 1), x), Dict(x => 1.0)) ≈ 0.25      # 1/(x+1)^2
    @test evalat(derivative(sqrt(x), x), Dict(x => 4.0)) ≈ 0.25          # 1/(2√x)
    @test evalat(derivative(tan(x), x), Dict(x => 0.0)) ≈ 1.0           # sec(0)^2
end

@testset "Symbolics derivative: higher order via simplify" begin
    @variables x
    # second derivative of x^3 is 6x; needs simplify to collect 3*(2x).
    @test simplify(derivative(derivative(x^3, x), x)) == 6x
    @test evalat(derivative(derivative(x^3, x), x), Dict(x => 2)) == 12
end

@testset "Symbolics derivative: expand_derivatives is identity (eager)" begin
    @variables x
    d = derivative(sin(x), x)
    @test expand_derivatives(d) == d
end

# Regression for Issue #7186: the general (x-dependent exponent) power rule
#   (a^b)' = a^b·(b'·log a + b·a'/a)
# recurses on the *second* argument and nests several `_mk*` constructors. That
# branch used to make `using Symbolics` hang at load (an unbounded re-analysis in
# PartialStruct-return inference); with the negative cache it both loads and
# evaluates. The very fact this fixture loads `Symbolics` exercises the fix.
@testset "Symbolics derivative: general power rule (Issue #7186)" begin
    @variables x
    # d/dx x^x = x^x·(log x + 1); at x=2 → 4·(log 2 + 1).
    @test evalat(derivative(x^x, x), Dict(x => 2.0)) ≈ 4.0 * (log(2.0) + 1.0)
    # d/dx 2^x = 2^x·log 2; at x=3 → 8·log 2.
    @test evalat(derivative(2^x, x), Dict(x => 3.0)) ≈ 8.0 * log(2.0)
    # An x-independent exponent still takes the simple power rule.
    @test derivative(x^2, x) == 2x
end

true
