# Symbolics subset: `@variables` binds Num-wrapped symbolic variables and
# returns a vector of them (Issue #6572). Checks structural facts only — no
# Num arithmetic or comparison yet (those land with the arithmetic step), to
# steer clear of the promote-fallback recursion trap (Issue #5966).

using Test
using Symbolics

@testset "Symbolics @variables binds Num vars" begin
    vars = @variables x y
    @test x isa Num
    @test y isa Num
    @test length(vars) == 2
    @test value(x) isa Sym
    @test value(x).name === :x
    @test value(y).name === :y
    @test value(vars[1]).name === :x
    @test value(vars[2]).name === :y
end

@testset "Symbolics @variables single var" begin
    @variables z
    @test z isa Num
    @test value(z).name === :z
end

true
