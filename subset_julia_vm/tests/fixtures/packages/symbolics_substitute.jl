# Symbolics subset: `substitute` (Issue #6572). Replace symbolic variables with
# values; fully numeric substitutions fold to a number, partial ones stay
# symbolic. `substitute` iterates the substitution dict (it never indexes it), so
# it is unaffected by the inline-chained Dict-index limitation (Issue #7173).

using Test
using Symbolics

@testset "Symbolics substitute: full numeric folding" begin
    @variables x y
    @test substitute(x^2 + 1, x => 3) == 10
    @test substitute(x^2 + 1, Dict(x => 3)) == 10
    @test substitute(x * y, Dict(x => 2, y => 5)) == 10
    @test substitute(2x + 1, x => 4) == 9
    @test substitute(x / y, Dict(x => 10, y => 2)) == 5.0
    @test substitute(sin(x), x => 0) == 0.0
    @test substitute(cos(x) + 1, x => 0) == 2.0
end

@testset "Symbolics substitute: partial and symbolic results" begin
    @variables x y
    # Partial substitution keeps the rest symbolic.
    e = substitute(x + y, x => 3)
    @test operation(value(e)) === :+
    @test substitute(e, y => 4) == 7
    # Substituting an unrelated variable leaves it untouched.
    @test value(substitute(y, x => 3)) isa Sym
    @test value(substitute(y, x => 3)).name === :y
    # Substituting a symbol for a symbol.
    @test isequal(substitute(x + y, x => y), y + y)
    # Substituting a symbolic value.
    @test isequal(substitute(x^2, x => y + 1), (y + 1)^2)
end

@testset "Symbolics substitute: Num works as a Dict key" begin
    @variables x y
    d = Dict(x => 10, y => 20)
    @test length(d) == 2                     # structural hash keeps keys distinct
    @test d[x] == 10                         # hash/isequal agree (bound dict)
    @test haskey(d, y)
    @test hash(x) == hash(Num(Sym(:x)))      # hash consistent with isequal
    @test hash(x) != hash(y)
end

true
