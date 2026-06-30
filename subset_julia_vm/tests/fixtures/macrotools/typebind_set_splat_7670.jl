using Test
using MacroTools

@testset "MacroTools TypeBind splats Set heads (Issue #7670)" begin
    @test MacroTools.isexpr(:(f(1)), Set{Any}([:call])...)
    @test !MacroTools.isexpr(:(f(1)), Set{Any}([:block])...)

    env = Dict{Symbol,Any}()
    bind = MacroTools.TypeBind(:x, Set{Any}([:call]))
    matched = MacroTools.match_inner(bind, :(f(1)), env)

    @test haskey(matched, :x)
    @test haskey(env, :x)
    @test matched[:x].head === :call
    @test matched[:x].args[1] === :f
    @test matched[:x].args[2] == 1
    @test env[:x] == matched[:x]
end

true
