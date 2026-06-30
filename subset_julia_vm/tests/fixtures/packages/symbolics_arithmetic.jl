# Symbolics subset: arithmetic, elementary functions and equality on `Num`
# (Issue #6572). The mixed-type operator methods (`Num⊗Real`/`Real⊗Num`) must
# keep every pair off the generic promote fallback (Issue #5966) — this fixture
# exercises `x + 1` / `1 + x` etc. directly, which would recurse forever if a
# mixed method were missing.

using Test
using Symbolics

@testset "Symbolics arithmetic: term construction" begin
    @variables x y
    @test operation(value(x + 1)) === :+
    @test operation(value(1 + x)) === :+      # mixed Real⊗Num must not recurse
    @test operation(value(x + x)) === :+
    @test operation(value(x - y)) === :-
    @test operation(value(2x)) === :*
    @test arguments(value(2x))[1] == 2
    @test arguments(value(2x))[2] isa Sym
    @test operation(value(x * y)) === :*
    @test operation(value(x / y)) === :/
    @test operation(value(x^2)) === :^
    @test arguments(value(x^2))[2] == 2
    @test operation(value(-x)) === :-
end

@testset "Symbolics arithmetic: constant folding" begin
    @test value(Num(2) + Num(3)) == 5
    @test value(Num(6) * Num(7)) == 42
    @test value(Num(10) - Num(4)) == 6
    @test value(Num(2)^Num(5)) == 32
end

@testset "Symbolics arithmetic: identity simplification" begin
    @variables x
    @test value(0 + x) isa Sym       # 0 + x => x
    @test value(x + 0) isa Sym
    @test value(1 * x) isa Sym       # 1 * x => x
    @test value(x * 1) isa Sym
    @test value(x^1) isa Sym         # x^1 => x
    @test value(x^0) == 1            # x^0 => 1
    @test value(0 * x) == 0          # 0 * x => 0
end

@testset "Symbolics arithmetic: elementary functions" begin
    @variables x
    @test operation(value(sin(x))) === :sin
    @test operation(value(cos(x))) === :cos
    @test operation(value(exp(x))) === :exp
    @test operation(value(log(x))) === :log
    @test operation(value(sqrt(x))) === :sqrt
    @test operation(value(tan(x))) === :tan
    # constant arguments fold to the numeric result
    @test value(cos(Num(0))) == 1.0
end

@testset "Symbolics arithmetic: equality and isequal" begin
    @variables x y
    @test Num(10) == 10              # numeric fold
    @test 10 == Num(10)
    @test 2x == 2 * x                # structural Bool
    @test isequal(x + y, x + y)
    @test !isequal(x + y, y + x)     # shallow: order-sensitive
    @test !(x == y)
    @test iscall(value(x + 1))
    @test !iscall(value(x))
end

true
