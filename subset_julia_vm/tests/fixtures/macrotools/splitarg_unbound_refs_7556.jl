using Test
using MacroTools

@testset "MacroTools splitarg matcher bindings stay branch-local (Issues #7556/#10819)" begin
    @test MacroTools.splitarg(:(x::Int)) == (:x, :Int, false, nothing)
    @test MacroTools.splitarg(:(::Int)) == (nothing, :Int, false, nothing)
    @test MacroTools.splitarg(:(x)) == (:x, :Any, false, nothing)
    @test MacroTools.splitarg(:(args...)) == (:args, :Any, true, nothing)
end

true
